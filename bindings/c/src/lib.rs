//! # mce-c — C ABI bindings for the `mce` chess engine
//!
//! A flat `extern "C"` surface over an **opaque handle** ([`McePosition`]) that
//! wraps an [`mce::AnyVariant`]. It lets C / C++ tools and GUIs embed the
//! engine's rules and move generation without naming any Rust type.
//!
//! ## Ownership / memory rules
//!
//! - A `McePosition*` returned by [`mce_position_new_from_fen`] or
//!   [`mce_position_startpos`] is **owned by the caller**. The caller must
//!   release it with exactly one call to [`mce_position_free`]. Passing the same
//!   pointer to `free` twice, or freeing a pointer not produced by this library,
//!   is undefined behavior. `mce_position_free(NULL)` is a documented no-op.
//! - All other functions **borrow** the handle and never take ownership; the
//!   caller keeps it alive for the duration of the call and frees it later.
//!   Positions are *immutable* once built except through
//!   [`mce_position_play_uci`], which mutates the handle in place (it advances
//!   the position by one ply).
//! - `const char*` inputs (`fen`, `variant`, `uci`) must be valid,
//!   NUL-terminated C strings; they are only read, never retained or freed by
//!   this library. A NULL string pointer is rejected (error / NULL return).
//!
//! ## Buffer / output-string contract
//!
//! The string-producing functions ([`mce_position_to_fen`],
//! [`mce_position_legal_moves`]) follow one uniform two-call contract:
//!
//! - They write into a caller-provided `char* buf` of `size_t buflen` bytes and
//!   **return the number of bytes the full string needs *including* the NUL
//!   terminator**.
//! - If `buflen` is large enough (`>= needed`), the function writes the string
//!   and a trailing NUL and returns `needed`.
//! - If `buflen` is too small (including `0`, or `buf == NULL`), nothing past
//!   `buflen` is written; when `buf != NULL && buflen > 0` the buffer is left
//!   holding a valid (truncated) NUL-terminated string. The return value is
//!   still the full `needed` length, so the caller can allocate `needed` bytes
//!   and call again. A return of `0` signals an error (e.g. a NULL handle).
//!
//! Typical usage from C:
//!
//! ```c
//! size_t need = mce_position_to_fen(pos, NULL, 0);
//! char *buf = malloc(need);
//! mce_position_to_fen(pos, buf, need);
//! ```
//!
//! ## Panic safety
//!
//! No function unwinds across the FFI boundary. Every body that touches engine
//! code runs inside [`std::panic::catch_unwind`]; a panic is converted into the
//! function's documented error value (NULL, `0`, or a nonzero error code) rather
//! than crossing into C (which would be undefined behavior).

// This is the FFI boundary crate: `unsafe` is unavoidable here (extern "C" and
// raw pointers). It is allowed locally and confined to this crate; every block
// carries a `// SAFETY:` comment. The core `mce` crate stays unsafe-free.
#![allow(clippy::missing_safety_doc)]

use std::ffi::{c_char, c_int, CStr};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

use mce::geometry::{AnyWideVariant, WideVariantId};
use mce::{AnyVariant, Color, Outcome, VariantId};

/// Opaque handle to a chess position of a runtime-chosen variant.
///
/// C code only ever holds a `McePosition*`; the layout is private. Create one
/// with [`mce_position_startpos`] / [`mce_position_new_from_fen`] and release it
/// with [`mce_position_free`].
pub struct McePosition {
    inner: AnyVariant,
}

/// Game-outcome codes returned by [`mce_position_outcome`].
///
/// Kept as plain `int` values so the header exposes a stable C enum.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MceOutcome {
    /// The game is still in progress (no result yet).
    Ongoing = 0,
    /// The game ended in a draw (stalemate, insufficient material, repetition,
    /// the 75-move rule, or a variant-specific draw).
    Draw = 1,
    /// White has won.
    WhiteWins = 2,
    /// Black has won.
    BlackWins = 3,
}

/// Resolve a borrowed `*const McePosition` to a shared reference, or return
/// `$err` if it is NULL.
macro_rules! pos_ref {
    ($ptr:expr, $err:expr) => {{
        if $ptr.is_null() {
            return $err;
        }
        // SAFETY: `$ptr` is non-null (checked just above). Per this crate's
        // ownership contract the caller passes a pointer previously returned by
        // a constructor and not yet freed, so it points to a live `McePosition`
        // that outlives this borrow. We only take a shared (`&`) reference.
        unsafe { &*$ptr }
    }};
}

/// Borrow a NUL-terminated C string as `&str`, or return `$err` on NULL / bad
/// UTF-8.
macro_rules! cstr {
    ($ptr:expr, $err:expr) => {{
        if $ptr.is_null() {
            return $err;
        }
        // SAFETY: `$ptr` is non-null (checked just above) and, per the input
        // contract, points to a valid NUL-terminated C string the caller keeps
        // alive for this call. `CStr::from_ptr` reads up to the NUL only; the
        // resulting borrow does not outlive `$ptr`.
        let cstr = unsafe { CStr::from_ptr($ptr) };
        match cstr.to_str() {
            Ok(s) => s,
            Err(_) => return $err,
        }
    }};
}

/// Creates a position from a six-field FEN under the named `variant`.
///
/// `variant` accepts the canonical names and aliases of [`VariantId::from_str`]
/// (e.g. `"chess"`, `"atomic"`, `"crazyhouse"`, `"koth"`, `"960"`).
///
/// Returns a fresh owned `McePosition*`, or **NULL** if either string is NULL /
/// not valid UTF-8, the variant name is unknown, or the FEN does not parse.
///
/// The returned pointer must be released with [`mce_position_free`].
#[no_mangle]
pub extern "C" fn mce_position_new_from_fen(
    fen: *const c_char,
    variant: *const c_char,
) -> *mut McePosition {
    let fen = cstr!(fen, ptr::null_mut());
    let variant = cstr!(variant, ptr::null_mut());
    // catch_unwind guards the engine call so a panic becomes a NULL return.
    let result = catch_unwind(|| {
        let id: VariantId = variant.parse().ok()?;
        let inner = AnyVariant::from_fen(id, fen).ok()?;
        Some(Box::new(McePosition { inner }))
    });
    match result {
        Ok(Some(boxed)) => Box::into_raw(boxed),
        _ => ptr::null_mut(),
    }
}

/// Creates the starting position of the named `variant`.
///
/// `variant` accepts the same names as [`mce_position_new_from_fen`]. Returns a
/// fresh owned `McePosition*`, or **NULL** if `variant` is NULL / not valid
/// UTF-8 / an unknown variant. Release it with [`mce_position_free`].
#[no_mangle]
pub extern "C" fn mce_position_startpos(variant: *const c_char) -> *mut McePosition {
    let variant = cstr!(variant, ptr::null_mut());
    let result = catch_unwind(|| {
        let id: VariantId = variant.parse().ok()?;
        Some(Box::new(McePosition {
            inner: AnyVariant::startpos(id),
        }))
    });
    match result {
        Ok(Some(boxed)) => Box::into_raw(boxed),
        _ => ptr::null_mut(),
    }
}

/// Releases a position created by this library.
///
/// `mce_position_free(NULL)` is a no-op. Calling it twice on the same non-NULL
/// pointer, or on a pointer this library did not produce, is undefined behavior.
#[no_mangle]
pub extern "C" fn mce_position_free(pos: *mut McePosition) {
    if pos.is_null() {
        return;
    }
    // SAFETY: `pos` is non-null and, per the ownership contract, was produced by
    // `Box::into_raw` in one of this crate's constructors and has not been freed
    // yet. Reconstituting the `Box` reclaims that exact allocation; dropping it
    // frees it exactly once.
    let boxed = unsafe { Box::from_raw(pos) };
    drop(boxed);
}

/// Writes the position's FEN into `buf` and returns the length needed
/// (including the NUL terminator). See the crate-level buffer contract. Returns
/// `0` if `pos` is NULL.
#[no_mangle]
pub extern "C" fn mce_position_to_fen(
    pos: *const McePosition,
    buf: *mut c_char,
    buflen: usize,
) -> usize {
    let pos = pos_ref!(pos, 0);
    let fen = match catch_unwind(AssertUnwindSafe(|| pos.inner.to_fen())) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    write_c_string(&fen, buf, buflen)
}

/// Writes the legal moves of the side to move into `buf` as a single
/// **space-separated** list of UCI strings (e.g. `"e2e4 e2e3 g1f3 ..."`), and
/// returns the length needed including the NUL terminator. See the crate-level
/// buffer contract. An empty move list yields the empty string (`needed == 1`,
/// just the NUL). Returns `0` if `pos` is NULL.
#[no_mangle]
pub extern "C" fn mce_position_legal_moves(
    pos: *const McePosition,
    buf: *mut c_char,
    buflen: usize,
) -> usize {
    let pos = pos_ref!(pos, 0);
    let joined = match catch_unwind(AssertUnwindSafe(|| {
        let p = &pos.inner;
        let ucis: Vec<String> = p.legal_moves().iter().map(|m| p.to_uci(m)).collect();
        ucis.join(" ")
    })) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    write_c_string(&joined, buf, buflen)
}

/// Parses `uci` against the position and, if it names a legal move, plays it,
/// **mutating the handle in place** (advancing it one ply).
///
/// Returns `0` on success. Returns a nonzero error code otherwise:
/// `1` if `pos` or `uci` is NULL or `uci` is not valid UTF-8, `2` if the move is
/// malformed or illegal in this position. On any nonzero return the position is
/// left unchanged.
#[no_mangle]
pub extern "C" fn mce_position_play_uci(pos: *mut McePosition, uci: *const c_char) -> c_int {
    if pos.is_null() {
        return 1;
    }
    let uci = cstr!(uci, 1);
    // SAFETY: `pos` is non-null (checked) and, per the ownership contract,
    // points to a live, caller-owned `McePosition`. This is the only API that
    // mutates the handle, so an exclusive (`&mut`) borrow does not alias any
    // other live reference for the duration of this call.
    let pos = unsafe { &mut *pos };
    let played = catch_unwind(AssertUnwindSafe(|| match pos.inner.parse_uci(uci) {
        // `parse_uci` only returns a *legal* move, so `play` cannot panic on it.
        Ok(mv) => Some(pos.inner.play(&mv)),
        Err(_) => None,
    }));
    match played {
        Ok(Some(next)) => {
            pos.inner = next;
            0
        }
        Ok(None) => 2,
        Err(_) => 2,
    }
}

/// Returns `1` if the side to move is in check, `0` if not. Returns `0` if `pos`
/// is NULL or the underlying call panics.
#[no_mangle]
pub extern "C" fn mce_position_is_check(pos: *const McePosition) -> c_int {
    let pos = pos_ref!(pos, 0);
    match catch_unwind(AssertUnwindSafe(|| pos.inner.is_check())) {
        Ok(true) => 1,
        _ => 0,
    }
}

/// Returns the game outcome as an [`MceOutcome`] code (an `int`).
///
/// `MCE_OUTCOME_ONGOING` (0) means the game is not over. Otherwise the value is
/// `DRAW`, `WHITE_WINS`, or `BLACK_WINS`. Returns `ONGOING` if `pos` is NULL or
/// the call panics (treat a NULL handle as a programming error on the caller's
/// side; this never crashes).
#[no_mangle]
pub extern "C" fn mce_position_outcome(pos: *const McePosition) -> MceOutcome {
    let pos = pos_ref!(pos, MceOutcome::Ongoing);
    match catch_unwind(AssertUnwindSafe(|| pos.inner.outcome())) {
        Ok(Some(Outcome::Decisive {
            winner: Color::White,
        })) => MceOutcome::WhiteWins,
        Ok(Some(Outcome::Decisive {
            winner: Color::Black,
        })) => MceOutcome::BlackWins,
        Ok(Some(Outcome::Draw)) => MceOutcome::Draw,
        _ => MceOutcome::Ongoing,
    }
}

/// Counts the leaf nodes reachable in exactly `depth` plies from this position
/// (a standard perft). Returns `0` if `pos` is NULL or the computation panics.
///
/// Note that `depth == 0` legitimately returns `1`.
#[no_mangle]
pub extern "C" fn mce_perft(pos: *const McePosition, depth: u32) -> u64 {
    let pos = pos_ref!(pos, 0);
    // A panic (or a NULL handle, handled above) yields 0, the documented error.
    catch_unwind(AssertUnwindSafe(|| pos.inner.perft(depth))).unwrap_or_default()
}

// -- Fairy (geometry-layer) variants ----------------------------------------
//
// The same opaque-handle surface as above, over `AnyWideVariant` rather than the
// concrete engine's `AnyVariant`. These reach the geometry-layer fairy variants
// (xiangqi, shogi, janggi, orda, …) whose board geometry differs from 8x8, so
// they need a distinct handle type and a distinct set of entry points. Ownership,
// the two-call buffer contract, and panic safety are identical to the standard
// functions above.

/// Opaque handle to a fairy-variant chess position chosen at runtime.
///
/// C code only ever holds a `MceFairyPosition*`; the layout is private. Create
/// one with [`mce_fairy_position_startpos`] /
/// [`mce_fairy_position_new_from_fen`] and release it with
/// [`mce_fairy_position_free`].
pub struct MceFairyPosition {
    inner: AnyWideVariant,
}

/// Creates a fairy position from a FEN under the named `variant`.
///
/// `variant` accepts the canonical names and aliases of
/// [`WideVariantId::from_str`] (e.g. `"xiangqi"`, `"shogi"`, `"janggi"`,
/// `"orda"`, `"cchess"`).
///
/// Returns a fresh owned `MceFairyPosition*`, or **NULL** if either string is
/// NULL / not valid UTF-8, the variant name is unknown, or the FEN does not
/// parse. Release it with [`mce_fairy_position_free`].
#[no_mangle]
pub extern "C" fn mce_fairy_position_new_from_fen(
    fen: *const c_char,
    variant: *const c_char,
) -> *mut MceFairyPosition {
    let fen = cstr!(fen, ptr::null_mut());
    let variant = cstr!(variant, ptr::null_mut());
    let result = catch_unwind(|| {
        let id: WideVariantId = variant.parse().ok()?;
        let inner = AnyWideVariant::from_fen(id, fen).ok()?;
        Some(Box::new(MceFairyPosition { inner }))
    });
    match result {
        Ok(Some(boxed)) => Box::into_raw(boxed),
        _ => ptr::null_mut(),
    }
}

/// Creates the starting position of the named fairy `variant`.
///
/// `variant` accepts the same names as [`mce_fairy_position_new_from_fen`].
/// Returns a fresh owned `MceFairyPosition*`, or **NULL** if `variant` is NULL /
/// not valid UTF-8 / an unknown variant. Release it with
/// [`mce_fairy_position_free`].
#[no_mangle]
pub extern "C" fn mce_fairy_position_startpos(variant: *const c_char) -> *mut MceFairyPosition {
    let variant = cstr!(variant, ptr::null_mut());
    let result = catch_unwind(|| {
        let id: WideVariantId = variant.parse().ok()?;
        Some(Box::new(MceFairyPosition {
            inner: AnyWideVariant::startpos(id),
        }))
    });
    match result {
        Ok(Some(boxed)) => Box::into_raw(boxed),
        _ => ptr::null_mut(),
    }
}

/// Releases a fairy position created by this library.
///
/// `mce_fairy_position_free(NULL)` is a no-op. Calling it twice on the same
/// non-NULL pointer, or on a pointer this library did not produce, is undefined
/// behavior.
#[no_mangle]
pub extern "C" fn mce_fairy_position_free(pos: *mut MceFairyPosition) {
    if pos.is_null() {
        return;
    }
    // SAFETY: `pos` is non-null and, per the ownership contract, was produced by
    // `Box::into_raw` in one of this crate's fairy constructors and has not been
    // freed yet. Reconstituting the `Box` reclaims that exact allocation;
    // dropping it frees it exactly once.
    let boxed = unsafe { Box::from_raw(pos) };
    drop(boxed);
}

/// Writes the position's FEN into `buf` and returns the length needed (including
/// the NUL terminator). See the crate-level buffer contract. Returns `0` if
/// `pos` is NULL.
#[no_mangle]
pub extern "C" fn mce_fairy_position_to_fen(
    pos: *const MceFairyPosition,
    buf: *mut c_char,
    buflen: usize,
) -> usize {
    let pos = pos_ref!(pos, 0);
    let fen = match catch_unwind(AssertUnwindSafe(|| pos.inner.to_fen())) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    write_c_string(&fen, buf, buflen)
}

/// Writes the legal moves of the side to move into `buf` as a single
/// **space-separated** list of UCI strings, and returns the length needed
/// including the NUL terminator. See the crate-level buffer contract. An empty
/// move list yields the empty string (`needed == 1`). Returns `0` if `pos` is
/// NULL.
#[no_mangle]
pub extern "C" fn mce_fairy_position_legal_moves(
    pos: *const MceFairyPosition,
    buf: *mut c_char,
    buflen: usize,
) -> usize {
    let pos = pos_ref!(pos, 0);
    let joined = match catch_unwind(AssertUnwindSafe(|| {
        let p = &pos.inner;
        let ucis: Vec<String> = p.legal_moves().iter().map(|m| p.to_uci(m)).collect();
        ucis.join(" ")
    })) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    write_c_string(&joined, buf, buflen)
}

/// Parses `uci` against the position and, if it names a legal move, plays it,
/// **mutating the handle in place** (advancing it one ply).
///
/// Returns `0` on success, `1` if `pos` or `uci` is NULL or `uci` is not valid
/// UTF-8, `2` if the move is malformed or illegal. On any nonzero return the
/// position is left unchanged.
#[no_mangle]
pub extern "C" fn mce_fairy_position_play_uci(
    pos: *mut MceFairyPosition,
    uci: *const c_char,
) -> c_int {
    if pos.is_null() {
        return 1;
    }
    let uci = cstr!(uci, 1);
    // SAFETY: `pos` is non-null (checked) and, per the ownership contract,
    // points to a live, caller-owned `MceFairyPosition`. This is the only API
    // that mutates the handle, so an exclusive (`&mut`) borrow does not alias any
    // other live reference for the duration of this call.
    let pos = unsafe { &mut *pos };
    let played = catch_unwind(AssertUnwindSafe(|| match pos.inner.parse_uci(uci) {
        // `parse_uci` only returns a *legal* move, so `play` cannot panic on it.
        Some(mv) => Some(pos.inner.play(&mv)),
        None => None,
    }));
    match played {
        Ok(Some(next)) => {
            pos.inner = next;
            0
        }
        Ok(None) => 2,
        Err(_) => 2,
    }
}

/// Returns `1` if the side to move is in check, `0` if not. Returns `0` if `pos`
/// is NULL or the underlying call panics.
#[no_mangle]
pub extern "C" fn mce_fairy_position_is_check(pos: *const MceFairyPosition) -> c_int {
    let pos = pos_ref!(pos, 0);
    match catch_unwind(AssertUnwindSafe(|| pos.inner.is_check())) {
        Ok(true) => 1,
        _ => 0,
    }
}

/// Returns the fairy game outcome as an [`MceOutcome`] code (an `int`), with the
/// same encoding as [`mce_position_outcome`]. Returns `ONGOING` if `pos` is NULL
/// or the call panics.
#[no_mangle]
pub extern "C" fn mce_fairy_position_outcome(pos: *const MceFairyPosition) -> MceOutcome {
    let pos = pos_ref!(pos, MceOutcome::Ongoing);
    match catch_unwind(AssertUnwindSafe(|| pos.inner.outcome())) {
        Ok(Some(mce::geometry::WideOutcome::Decisive {
            winner: Color::White,
        })) => MceOutcome::WhiteWins,
        Ok(Some(mce::geometry::WideOutcome::Decisive {
            winner: Color::Black,
        })) => MceOutcome::BlackWins,
        Ok(Some(mce::geometry::WideOutcome::Draw)) => MceOutcome::Draw,
        _ => MceOutcome::Ongoing,
    }
}

/// Counts the leaf nodes reachable in exactly `depth` plies from this fairy
/// position (a perft). Returns `0` if `pos` is NULL or the computation panics.
/// `depth == 0` legitimately returns `1`.
#[no_mangle]
pub extern "C" fn mce_fairy_perft(pos: *const MceFairyPosition, depth: u32) -> u64 {
    let pos = pos_ref!(pos, 0);
    catch_unwind(AssertUnwindSafe(|| pos.inner.perft(depth))).unwrap_or_default()
}

/// Writes `s` (plus a NUL) into `buf`/`buflen` per the crate buffer contract and
/// returns `s.len() + 1` (the bytes needed including the terminator).
///
/// Truncates safely when `buflen` is too small, always NUL-terminating whatever
/// it does write (when `buf` is non-null and `buflen > 0`).
fn write_c_string(s: &str, buf: *mut c_char, buflen: usize) -> usize {
    let needed = s.len() + 1; // +1 for the trailing NUL
    if buf.is_null() || buflen == 0 {
        return needed;
    }
    // Copy as many bytes as fit, reserving one byte for the NUL terminator.
    let copy_len = core::cmp::min(s.len(), buflen - 1);
    // SAFETY: `buf` is non-null and the caller guarantees it points to at least
    // `buflen` writable bytes. `copy_len <= buflen - 1`, so the byte copy stays
    // strictly within `buflen - 1` bytes and the terminator write lands at index
    // `copy_len <= buflen - 1`, i.e. in bounds. Source and destination cannot
    // overlap: `s` is a Rust-owned slice, `buf` a caller C buffer.
    unsafe {
        ptr::copy_nonoverlapping(s.as_ptr().cast::<c_char>(), buf, copy_len);
        *buf.add(copy_len) = 0;
    }
    needed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    fn cs(s: &str) -> CString {
        CString::new(s).unwrap()
    }

    #[test]
    fn startpos_legal_move_count_and_perft() {
        let v = cs("chess");
        let pos = mce_position_startpos(v.as_ptr());
        assert!(!pos.is_null());

        // Two-call buffer contract for legal moves.
        let need = mce_position_legal_moves(pos, ptr::null_mut(), 0);
        let mut buf = vec![0u8; need];
        let got = mce_position_legal_moves(pos, buf.as_mut_ptr().cast(), buf.len());
        assert_eq!(got, need);
        let s = CStr::from_bytes_until_nul(&buf).unwrap().to_str().unwrap();
        assert_eq!(s.split_whitespace().count(), 20);

        assert_eq!(mce_perft(pos, 1), 20);
        assert_eq!(mce_perft(pos, 2), 400);
        assert_eq!(mce_perft(pos, 0), 1);

        mce_position_free(pos);
    }

    #[test]
    fn fen_roundtrip_and_play() {
        let v = cs("chess");
        let pos = mce_position_startpos(v.as_ptr());

        let need = mce_position_to_fen(pos, ptr::null_mut(), 0);
        let mut buf = vec![0u8; need];
        mce_position_to_fen(pos, buf.as_mut_ptr().cast(), buf.len());
        let fen = CStr::from_bytes_until_nul(&buf).unwrap().to_str().unwrap();
        assert_eq!(
            fen,
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );

        let mv = cs("e2e4");
        assert_eq!(mce_position_play_uci(pos, mv.as_ptr()), 0);
        let bad = cs("e2e5");
        assert_eq!(mce_position_play_uci(pos, bad.as_ptr()), 2);
        let garbage = cs("zzzz");
        assert_eq!(mce_position_play_uci(pos, garbage.as_ptr()), 2);

        mce_position_free(pos);
    }

    #[test]
    fn truncation_is_safe_and_nul_terminated() {
        let v = cs("chess");
        let pos = mce_position_startpos(v.as_ptr());
        let mut small = [0xAAu8; 4];
        let need = mce_position_to_fen(pos, small.as_mut_ptr().cast(), small.len());
        assert!(need > small.len());
        // Always NUL-terminated within the buffer.
        assert!(small.contains(&0));
        let s = CStr::from_bytes_until_nul(&small)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(s, "rnb"); // first 3 chars + NUL
        mce_position_free(pos);
    }

    #[test]
    fn checkmate_outcome_and_check() {
        // Fool's mate: 1. f3 e5 2. g4 Qh4#
        let v = cs("chess");
        let fen = cs("rnbqkbnr/pppp1ppp/8/4p3/6P1/5P2/PPPPP2P/RNBQKBNR b KQkq g3 0 2");
        let pos = mce_position_new_from_fen(fen.as_ptr(), v.as_ptr());
        assert!(!pos.is_null());
        assert_eq!(mce_position_outcome(pos), MceOutcome::Ongoing);

        let mate = cs("d8h4");
        assert_eq!(mce_position_play_uci(pos, mate.as_ptr()), 0);
        assert_eq!(mce_position_is_check(pos), 1);
        assert_eq!(mce_position_outcome(pos), MceOutcome::BlackWins);
        mce_position_free(pos);
    }

    #[test]
    fn null_and_bad_inputs_are_rejected() {
        assert!(mce_position_startpos(ptr::null()).is_null());
        let bad = cs("notavariant");
        assert!(mce_position_startpos(bad.as_ptr()).is_null());

        let v = cs("chess");
        let badfen = cs("not a fen");
        assert!(mce_position_new_from_fen(badfen.as_ptr(), v.as_ptr()).is_null());

        // NULL handle: documented safe error values, no crash.
        assert_eq!(mce_perft(ptr::null(), 1), 0);
        assert_eq!(mce_position_to_fen(ptr::null(), ptr::null_mut(), 0), 0);
        assert_eq!(mce_position_is_check(ptr::null()), 0);
        assert_eq!(mce_position_outcome(ptr::null()), MceOutcome::Ongoing);
        assert_eq!(mce_position_play_uci(ptr::null_mut(), v.as_ptr()), 1);
        mce_position_free(ptr::null_mut()); // no-op
    }

    #[test]
    fn variant_startpos_counts() {
        let v = cs("atomic");
        let pos = mce_position_startpos(v.as_ptr());
        assert_eq!(mce_perft(pos, 1), 20);
        mce_position_free(pos);
    }

    #[test]
    fn fairy_startpos_legal_moves_and_perft() {
        // Construct a fairy variant by name and run perft — the acceptance gate.
        // FSF-confirmed Xiangqi startpos counts (tests/perft_xiangqi.rs).
        let v = cs("xiangqi");
        let pos = mce_fairy_position_startpos(v.as_ptr());
        assert!(!pos.is_null());

        let need = mce_fairy_position_legal_moves(pos, ptr::null_mut(), 0);
        let mut buf = vec![0u8; need];
        let got = mce_fairy_position_legal_moves(pos, buf.as_mut_ptr().cast(), buf.len());
        assert_eq!(got, need);
        let s = CStr::from_bytes_until_nul(&buf).unwrap().to_str().unwrap();
        assert_eq!(s.split_whitespace().count(), 44);

        assert_eq!(mce_fairy_perft(pos, 0), 1);
        assert_eq!(mce_fairy_perft(pos, 1), 44);
        assert_eq!(mce_fairy_perft(pos, 2), 1920);
        assert_eq!(mce_fairy_perft(pos, 3), 79666);
        assert_eq!(mce_fairy_position_is_check(pos), 0);
        assert_eq!(mce_fairy_position_outcome(pos), MceOutcome::Ongoing);

        mce_fairy_position_free(pos);

        // A second geometry (9x9 Shogi).
        let s = cs("shogi");
        let shogi = mce_fairy_position_startpos(s.as_ptr());
        assert_eq!(mce_fairy_perft(shogi, 1), 30);
        assert_eq!(mce_fairy_perft(shogi, 2), 900);
        mce_fairy_position_free(shogi);
    }

    #[test]
    fn fairy_fen_roundtrip_and_play() {
        let v = cs("xiangqi");
        let pos = mce_fairy_position_startpos(v.as_ptr());

        let need = mce_fairy_position_to_fen(pos, ptr::null_mut(), 0);
        let mut buf = vec![0u8; need];
        mce_fairy_position_to_fen(pos, buf.as_mut_ptr().cast(), buf.len());
        let fen = CStr::from_bytes_until_nul(&buf).unwrap().to_str().unwrap();

        // Re-parse the startpos FEN under the variant.
        let fenc = cs(fen);
        let reparsed = mce_fairy_position_new_from_fen(fenc.as_ptr(), v.as_ptr());
        assert!(!reparsed.is_null());
        mce_fairy_position_free(reparsed);

        // A bad move is rejected with code 2 and leaves the position unchanged.
        let bad = cs("a0a9");
        assert_eq!(mce_fairy_position_play_uci(pos, bad.as_ptr()), 2);
        let garbage = cs("zzzz");
        assert_eq!(mce_fairy_position_play_uci(pos, garbage.as_ptr()), 2);

        // Alias resolution: "cchess" -> xiangqi (same startpos FEN).
        let alias = cs("cchess");
        let aliased = mce_fairy_position_startpos(alias.as_ptr());
        let need2 = mce_fairy_position_to_fen(aliased, ptr::null_mut(), 0);
        let mut buf2 = vec![0u8; need2];
        mce_fairy_position_to_fen(aliased, buf2.as_mut_ptr().cast(), buf2.len());
        let fen2 = CStr::from_bytes_until_nul(&buf2).unwrap().to_str().unwrap();
        assert_eq!(fen, fen2);
        mce_fairy_position_free(aliased);

        mce_fairy_position_free(pos);
    }

    #[test]
    fn fairy_null_and_bad_inputs_are_rejected() {
        assert!(mce_fairy_position_startpos(ptr::null()).is_null());
        let bad = cs("notafairyvariant");
        assert!(mce_fairy_position_startpos(bad.as_ptr()).is_null());

        let v = cs("xiangqi");
        let badfen = cs("not a fen");
        assert!(mce_fairy_position_new_from_fen(badfen.as_ptr(), v.as_ptr()).is_null());

        // NULL handle: documented safe error values, no crash.
        assert_eq!(mce_fairy_perft(ptr::null(), 1), 0);
        assert_eq!(
            mce_fairy_position_to_fen(ptr::null(), ptr::null_mut(), 0),
            0
        );
        assert_eq!(mce_fairy_position_is_check(ptr::null()), 0);
        assert_eq!(mce_fairy_position_outcome(ptr::null()), MceOutcome::Ongoing);
        assert_eq!(mce_fairy_position_play_uci(ptr::null_mut(), v.as_ptr()), 1);
        mce_fairy_position_free(ptr::null_mut()); // no-op
    }
}
