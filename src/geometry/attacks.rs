//! Generic attack and ray generation over an arbitrary [`Geometry`].
//!
//! This is the parallel generic analogue of the frozen concrete
//! [`crate::attacks`] module: the same steppers, hyperbola-quintessence sliders,
//! and `between` / `line` rays, but parametrised over `G: Geometry` so they work
//! for the standard 8x8 board (`u64` backing) and for wider boards such as
//! [`Cap10x8`](super::Cap10x8) (`u128` backing, ten files) alike.
//!
//! Everything here is `safe`, `no_std` Rust:
//!
//! * **Steppers** — [`pawn_attacks`], [`knight_attacks`], [`king_attacks`] are
//!   built on the generic [`leaper_attacks`] primitive, which expands a list of
//!   `(file, rank)` offsets at a square with full edge handling (no wrap across
//!   the board's left/right edges, no leak off the top/bottom).
//! * **Sliders** — [`bishop_attacks`], [`rook_attacks`], [`queen_attacks`] use
//!   hyperbola quintessence generalised to `G::Bits`: the `o ^ (o - 2s)` trick
//!   over per-direction line masks (rank / file / diagonal / anti-diagonal)
//!   regenerated for the geometry from its width and height. Pure integer
//!   arithmetic, no `unsafe`.
//! * **Geometry rays** — [`between`] and [`line`] give the strictly-between and
//!   full-line squares for two aligned squares.
//!
//! The slider line masks and ray walks are recomputed per call from the
//! geometry's dimensions. They are cheap (a bounded walk along one ray) and keep
//! the surface free of const-generic array lengths, which are not expressible
//! for `[Bitboard<G>; G::SQUARES as usize]` on stable Rust. The 8x8 results are
//! bit-for-bit identical to the frozen concrete tables (see the equivalence
//! tests).

use super::backing::BitboardBacking;
use super::{Bitboard, Geometry, Square};
use crate::Color;

// ---------------------------------------------------------------------------
// Leaper primitive.
// ---------------------------------------------------------------------------

/// Returns the squares a leaper on `sq` reaches by the given `(file, rank)`
/// offsets, dropping any offset that lands off the board.
///
/// This is the reusable stepper primitive: every fairy leaper (knight, ferz =
/// `±1` diagonal, wazir = `±1` orthogonal, the gold/silver generals, the knight
/// component of an archbishop, and so on) is a fixed offset list fed to this
/// function. Edges are respected exactly — an offset that would cross the left
/// or right edge, or leave the top or bottom, simply contributes nothing rather
/// than wrapping, because the destination file/rank are range-checked through
/// [`Square::offset`].
///
/// ```
/// use mce::geometry::{attacks::leaper_attacks, Chess8x8, Square};
/// // Wazir (orthogonal one-steppers) on a corner reaches two squares.
/// let wazir = leaper_attacks::<Chess8x8>(Square::new(0), &[(1, 0), (-1, 0), (0, 1), (0, -1)]);
/// assert_eq!(wazir.count(), 2);
/// ```
#[must_use]
pub fn leaper_attacks<G: Geometry>(sq: Square<G>, offsets: &[(i8, i8)]) -> Bitboard<G> {
    let mut bb = Bitboard::EMPTY;
    for &(df, dr) in offsets {
        if let Some(dest) = sq.offset(df, dr) {
            bb.set(dest);
        }
    }
    bb
}

/// The eight knight (`(±1,±2)` / `(±2,±1)`) leaps.
const KNIGHT_OFFSETS: [(i8, i8); 8] = [
    (1, 2),
    (2, 1),
    (2, -1),
    (1, -2),
    (-1, -2),
    (-2, -1),
    (-2, 1),
    (-1, 2),
];

/// The eight king (one-step in any of the eight directions) leaps.
const KING_OFFSETS: [(i8, i8); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// Returns the squares a pawn of `color` standing on `sq` attacks.
///
/// White pawns attack the two squares one rank toward the last rank (north-east
/// and north-west); black pawns attack one rank toward the first rank. Captures
/// off a side edge are dropped, and a pawn on the far rank attacks nothing.
///
/// ```
/// use mce::geometry::{attacks::pawn_attacks, Chess8x8, Square};
/// use mce::Color;
/// // A white pawn in the centre attacks the two forward diagonals.
/// assert_eq!(pawn_attacks::<Chess8x8>(Color::White, Square::new(28)).count(), 2);
/// ```
#[must_use]
pub fn pawn_attacks<G: Geometry>(color: Color, sq: Square<G>) -> Bitboard<G> {
    let offsets: [(i8, i8); 2] = if color.is_white() {
        [(-1, 1), (1, 1)]
    } else {
        [(-1, -1), (1, -1)]
    };
    leaper_attacks(sq, &offsets)
}

/// Returns the squares a knight on `sq` attacks.
///
/// ```
/// use mce::geometry::{attacks::knight_attacks, Chess8x8, Square};
/// // A knight in the centre of an 8x8 board reaches all eight squares.
/// assert_eq!(knight_attacks::<Chess8x8>(Square::new(27)).count(), 8);
/// ```
#[must_use]
pub fn knight_attacks<G: Geometry>(sq: Square<G>) -> Bitboard<G> {
    leaper_attacks(sq, &KNIGHT_OFFSETS)
}

/// Returns the squares a king on `sq` attacks (the up-to-eight adjacent
/// squares).
#[must_use]
pub fn king_attacks<G: Geometry>(sq: Square<G>) -> Bitboard<G> {
    leaper_attacks(sq, &KING_OFFSETS)
}

// ---------------------------------------------------------------------------
// Slider line masks (regenerated per geometry from width/height).
// ---------------------------------------------------------------------------

/// The full file through `sq` (every square sharing its file).
///
/// Computed by arithmetic, not a per-rank loop: the first-file mask
/// ([`Geometry::FILE_A_MASK`], one bit per rank) shifted left by the file index
/// places one bit on every rank of `sq`'s file. The hot slider path calls this
/// once per rook/queen/elephant per node, so the closed form matters.
#[inline]
fn file_mask<G: Geometry>(sq: Square<G>) -> Bitboard<G> {
    Bitboard(G::FILE_A_MASK) << sq.file() as u32
}

/// The full rank through `sq` (every square sharing its rank).
///
/// Computed by arithmetic: a contiguous run of the `WIDTH` low bits (the first
/// rank) shifted up to `sq`'s rank, with no per-file loop. The first-rank run is
/// derived once from the geometry's width.
#[inline]
fn rank_mask<G: Geometry>(sq: Square<G>) -> Bitboard<G> {
    let first_rank = Bitboard::<G>(first_rank_mask::<G>());
    first_rank << (sq.rank() as u32 * G::WIDTH as u32)
}

/// The `WIDTH` low bits set: the first-rank run of a board of geometry `G`,
/// as the backing integer. `(ONE << WIDTH) - ONE` via `wrapping_sub`, valid for
/// any width `< Bits::BITS` (every supported geometry: 8 or 10 over 64/128 bits).
#[inline]
fn first_rank_mask<G: Geometry>() -> G::Bits {
    (G::Bits::ONE << G::WIDTH as u32).wrapping_sub(G::Bits::ONE)
}

/// The diagonal (north-east / south-west, constant `rank - file`) through `sq`.
///
/// Built by a directional **fill** rather than a step-by-step ray walk: starting
/// from `sq`'s bit, the set is repeatedly grown by its own north-east and
/// south-west neighbours — the geometry's edge-masked diagonal shifts, which clip
/// at the file edges for free so the fill never wraps. Each round extends the run
/// by one cell in each direction, so `HEIGHT - 1` rounds complete a diagonal
/// (which spans at most `HEIGHT` ranks). Bit-identical to the old per-step ray
/// walk (the slider equivalence tests sweep every square against an independent
/// ray scan, on both the 8x8 and the `u128` geometries), but it advances both
/// rays of every diagonal on the board in lock-step rather than walking one
/// square at a time.
#[inline]
fn diag_mask<G: Geometry>(sq: Square<G>) -> Bitboard<G> {
    diagonal_fill(
        Bitboard::from_square(sq),
        Bitboard::north_east,
        Bitboard::south_west,
    )
}

/// The anti-diagonal (north-west / south-east, constant `rank + file`) through
/// `sq`. Built by the same fill as [`diag_mask`], along the NW/SE directions.
#[inline]
fn anti_diag_mask<G: Geometry>(sq: Square<G>) -> Bitboard<G> {
    diagonal_fill(
        Bitboard::from_square(sq),
        Bitboard::north_west,
        Bitboard::south_east,
    )
}

/// Grows `seed` into the full (anti-)diagonal it lies on by repeatedly unioning it
/// with its `pos` / `neg` diagonal neighbours, each an **edge-masked** directional
/// shift (so a cell on a file edge contributes nothing across it — no wrap).
///
/// A diagonal spans at most `HEIGHT` ranks, so `HEIGHT - 1` single-step rounds
/// reach every cell from any starting square on it. Correct for any geometry up to
/// the 128-bit backing, and equivalent to the per-square ray walk it replaces.
#[inline]
fn diagonal_fill<G: Geometry>(
    mut bb: Bitboard<G>,
    pos: fn(Bitboard<G>) -> Bitboard<G>,
    neg: fn(Bitboard<G>) -> Bitboard<G>,
) -> Bitboard<G> {
    // `HEIGHT - 1` rounds reach the two ends of any diagonal from any seed on it.
    let rounds = G::HEIGHT.saturating_sub(1);
    for _ in 0..rounds {
        bb = bb | pos(bb) | neg(bb);
    }
    bb
}

/// Builds the full line through `sq` in the `(df, dr)` direction and its
/// opposite, to both board edges (including `sq` itself).
fn walk_line<G: Geometry>(sq: Square<G>, df: i8, dr: i8) -> Bitboard<G> {
    let mut bb = Bitboard::from_square(sq);
    for &(sf, sr) in &[(df, dr), (-df, -dr)] {
        let mut cur = sq.offset(sf, sr);
        while let Some(next) = cur {
            bb.set(next);
            cur = next.offset(sf, sr);
        }
    }
    bb
}

// ---------------------------------------------------------------------------
// Slider core (hyperbola quintessence over `G::Bits`).
// ---------------------------------------------------------------------------

/// Computes the blocker-aware attack set along a single ray `mask` (a rank,
/// file, diagonal, or anti-diagonal passing through `sq`), generalised to
/// `G::Bits`.
///
/// The result excludes `sq` itself and includes the first blocker on each side.
/// The reversal operates over the whole backing width and is masked back to the
/// line, exactly as the frozen `u64` path does.
fn sliding<G: Geometry>(sq: Square<G>, occupied: Bitboard<G>, mask: Bitboard<G>) -> Bitboard<G> {
    let s = G::Bits::bit(sq.index() as u32);
    let o = occupied.0 & mask.0;

    // Forward: subtracting `2s` flips every bit up to and including the first
    // blocker above `sq`. `2s` is formed as `s + s` to avoid a debug overflow
    // panic when `s` is the high bit of the backing integer.
    let two_s = s.wrapping_add(s);
    let forward = o.wrapping_sub(two_s);

    // Reverse: the same trick on the bit-reversed line covers below `sq`.
    let rev_o = o.reverse_bits();
    let rev_s = s.reverse_bits();
    let two_rev_s = rev_s.wrapping_add(rev_s);
    let reverse = rev_o.wrapping_sub(two_rev_s).reverse_bits();

    Bitboard((forward ^ reverse) & mask.0)
}

/// Returns the squares a bishop on `sq` attacks given the `occupied` set.
///
/// Each diagonal ray stops at the first occupied square (which is included). The
/// caller masks out friendly pieces.
#[must_use]
pub fn bishop_attacks<G: Geometry>(sq: Square<G>, occupied: Bitboard<G>) -> Bitboard<G> {
    sliding(sq, occupied, diag_mask(sq)) | sliding(sq, occupied, anti_diag_mask(sq))
}

/// Returns the squares a rook on `sq` attacks given the `occupied` set.
///
/// Each orthogonal ray stops at the first occupied square (which is included).
#[must_use]
pub fn rook_attacks<G: Geometry>(sq: Square<G>, occupied: Bitboard<G>) -> Bitboard<G> {
    sliding(sq, occupied, file_mask(sq)) | sliding(sq, occupied, rank_mask(sq))
}

/// Returns the squares a queen on `sq` attacks given the `occupied` set (the
/// union of the rook and bishop rays).
#[must_use]
pub fn queen_attacks<G: Geometry>(sq: Square<G>, occupied: Bitboard<G>) -> Bitboard<G> {
    rook_attacks(sq, occupied) | bishop_attacks(sq, occupied)
}

/// Returns the squares a Shogi Lance of `color` on `sq` attacks given the
/// `occupied` set: the blocker-aware **forward** file ray only (north for white,
/// south for black), stopping at and including the first occupant.
///
/// It is the rook's file ray restricted to the half of the file in the side's
/// forward direction — the Lance slides any number of squares straight ahead but
/// never sideways or backward.
#[must_use]
pub fn lance_attacks<G: Geometry>(
    color: Color,
    sq: Square<G>,
    occupied: Bitboard<G>,
) -> Bitboard<G> {
    let file_ray = sliding(sq, occupied, file_mask(sq));
    // Keep only the squares ahead of `sq` on its file. Forward is the higher
    // ranks for white, the lower ranks for black.
    let mut forward = Bitboard::<G>::EMPTY;
    let step: i8 = if color.is_white() { 1 } else { -1 };
    let mut cur = sq.offset(0, step);
    while let Some(next) = cur {
        forward.set(next);
        cur = next.offset(0, step);
    }
    file_ray & forward
}

// ---------------------------------------------------------------------------
// Cannon primitive (Xiangqi / Janggi / Shako).
// ---------------------------------------------------------------------------

/// The four orthogonal ray directions a cannon travels along.
const CANNON_DIRS: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// Returns the squares a cannon on `sq` may move to **without capturing**: the
/// empty rook-ray squares, exactly as a rook's quiet moves, stopping one short
/// of the first piece on each ray.
///
/// A cannon slides like a rook over empty squares but does **not** capture the
/// piece it first meets — for that it needs a screen (see
/// [`cannon_capture_targets`]). The result therefore contains only empty
/// squares: every orthogonal square reachable before the first blocker on each
/// of the four rays. It is the rook quiet-move set, computed directly so the
/// cannon needs no rook call.
///
/// This is geometry-only: no palace, river, or color restriction is applied, so
/// the same primitive serves the Xiangqi and Janggi cannons (which add those
/// region masks on top) and the Shako cannon (which adds none).
///
/// ```
/// use mce::geometry::{attacks::cannon_quiet_moves, Chess8x8, Bitboard, Square};
/// // On an empty 8x8 board a cannon on a1 quietly slides the whole rank and file
/// // (14 squares), like a rook.
/// let q = cannon_quiet_moves::<Chess8x8>(Square::new(0), Bitboard::EMPTY);
/// assert_eq!(q.count(), 14);
/// ```
#[must_use]
#[inline]
pub fn cannon_quiet_moves<G: Geometry>(sq: Square<G>, occupied: Bitboard<G>) -> Bitboard<G> {
    let mut bb = Bitboard::EMPTY;
    for &(df, dr) in &CANNON_DIRS {
        let mut cur = sq.offset(df, dr);
        while let Some(next) = cur {
            if occupied.contains(next) {
                break; // first piece on the ray: a cannon cannot move onto it.
            }
            bb.set(next);
            cur = next.offset(df, dr);
        }
    }
    bb
}

/// Returns the squares a cannon on `sq` **threatens / may capture on**: along
/// each orthogonal ray, the first piece beyond **exactly one** intervening
/// piece (the "screen" / "mount").
///
/// This is the cannon's attack set — the squares it can land a capture on, and
/// equally the squares from which it gives check. It is *occupancy-aware* and
/// returns at most one square per ray: walk out to the first piece (the screen),
/// then continue past it to the next piece (the target). If a ray has no screen,
/// or nothing beyond the screen, that ray contributes nothing. The result
/// includes both colours' pieces — the caller masks out friendly occupants, the
/// same convention the slider primitives use.
///
/// Pairs with [`cannon_quiet_moves`] to give the cannon's full move set: quiet
/// rook-rays plus over-one-screen captures. Pure geometry (no palace / river),
/// so Xiangqi and Janggi inherit it and add their region masks on top.
///
/// ```
/// use mce::geometry::{attacks::cannon_capture_targets, Chess8x8, Bitboard, Square};
/// // Screen on a4, enemy on a7: a cannon on a1 captures over the screen onto a7.
/// let occ = Bitboard::<Chess8x8>::EMPTY
///     .with(Square::new(24)) // a4 screen
///     .with(Square::new(48)); // a7 target
/// let caps = cannon_capture_targets::<Chess8x8>(Square::new(0), occ);
/// assert!(caps.contains(Square::new(48)));
/// assert_eq!(caps.count(), 1);
/// ```
#[must_use]
#[inline]
pub fn cannon_capture_targets<G: Geometry>(sq: Square<G>, occupied: Bitboard<G>) -> Bitboard<G> {
    let mut bb = Bitboard::EMPTY;
    for &(df, dr) in &CANNON_DIRS {
        // Walk to the first piece on the ray (the screen).
        let mut cur = sq.offset(df, dr);
        let screen = loop {
            match cur {
                None => break None,
                Some(next) if occupied.contains(next) => break Some(next),
                Some(next) => cur = next.offset(df, dr),
            }
        };
        let Some(screen) = screen else { continue };
        // Walk past the screen to the next piece (the capture target).
        let mut cur = screen.offset(df, dr);
        while let Some(next) = cur {
            if occupied.contains(next) {
                bb.set(next);
                break;
            }
            cur = next.offset(df, dr);
        }
    }
    bb
}

// ---------------------------------------------------------------------------
// Janggi cannon primitive (screen-mandatory, screen/target may not be a cannon).
// ---------------------------------------------------------------------------

/// Returns the squares a **Janggi cannon** (포) on `sq` may move to **without
/// capturing**, along the four orthogonal rays, given the full `occupied` set and
/// the subset `cannons` of squares holding a cannon (either colour).
///
/// The Janggi cannon differs fundamentally from the Xiangqi / Shako cannon
/// ([`cannon_quiet_moves`]): it **cannot move along an empty ray at all** — every
/// move, quiet or capturing, must jump exactly one **screen** (a non-cannon
/// piece). So this returns the *empty* squares **beyond the first screen** on each
/// ray (up to, but not including, the next piece). Crucially the screen **may not
/// itself be a cannon**: a ray whose first piece is a cannon is dead (no jump).
///
/// This is geometry-only on the four orthogonal directions; the palace-diagonal
/// jump (over a screen on the palace centre) is layered on by the variant, which
/// owns the palace geometry. Pairs with [`janggi_cannon_capture`] for the full
/// move set.
///
/// ```
/// use mce::geometry::{attacks::janggi_cannon_quiet, Chess8x8, Bitboard, Square};
/// // a1 cannon, screen (non-cannon) on a4, nothing beyond: quiet jumps land on
/// // a5..a8 (the empty squares past the one screen).
/// let occ = Bitboard::<Chess8x8>::EMPTY.with(Square::new(24)); // a4 screen
/// let q = janggi_cannon_quiet::<Chess8x8>(Square::new(0), occ, Bitboard::EMPTY);
/// assert!(q.contains(Square::new(32))); // a5
/// assert!(!q.contains(Square::new(24))); // the screen is never a destination
/// ```
#[must_use]
#[inline]
pub fn janggi_cannon_quiet<G: Geometry>(
    sq: Square<G>,
    occupied: Bitboard<G>,
    cannons: Bitboard<G>,
) -> Bitboard<G> {
    let mut bb = Bitboard::EMPTY;
    for &(df, dr) in &CANNON_DIRS {
        // Walk to the first piece on the ray (the screen).
        let mut cur = sq.offset(df, dr);
        let screen = loop {
            match cur {
                None => break None,
                Some(next) if occupied.contains(next) => break Some(next),
                Some(next) => cur = next.offset(df, dr),
            }
        };
        let Some(screen) = screen else { continue };
        // The screen may not be a cannon — a cannon cannot use another cannon as
        // its mount.
        if cannons.contains(screen) {
            continue;
        }
        // Every empty square beyond the screen, up to the next piece (exclusive).
        let mut cur = screen.offset(df, dr);
        while let Some(next) = cur {
            if occupied.contains(next) {
                break;
            }
            bb.set(next);
            cur = next.offset(df, dr);
        }
    }
    bb
}

/// Returns the squares a **Janggi cannon** on `sq` **may capture on** along the
/// four orthogonal rays: the first piece beyond **exactly one screen**, where the
/// screen is a non-cannon piece **and** the captured target is itself **not a
/// cannon**.
///
/// This is the cannon's attack set (also the squares from which it gives check).
/// Two extra restrictions over the plain [`cannon_capture_targets`]: the screen
/// may not be a cannon (a ray whose first piece is a cannon is dead), and a cannon
/// may not capture a cannon (a target that is a cannon yields nothing on that
/// ray). The result includes both colours; the caller masks out friendly pieces.
///
/// The palace-diagonal jump is layered on by the variant (it owns the palace
/// geometry). Pure geometry on the orthogonals here.
///
/// ```
/// use mce::geometry::{attacks::janggi_cannon_capture, Chess8x8, Bitboard, Square};
/// // a1 cannon, non-cannon screen a4, enemy non-cannon a7: capture lands on a7.
/// let occ = Bitboard::<Chess8x8>::EMPTY.with(Square::new(24)).with(Square::new(48));
/// let caps = janggi_cannon_capture::<Chess8x8>(Square::new(0), occ, Bitboard::EMPTY);
/// assert_eq!(caps, Bitboard::EMPTY.with(Square::new(48)));
/// ```
#[must_use]
#[inline]
pub fn janggi_cannon_capture<G: Geometry>(
    sq: Square<G>,
    occupied: Bitboard<G>,
    cannons: Bitboard<G>,
) -> Bitboard<G> {
    let mut bb = Bitboard::EMPTY;
    for &(df, dr) in &CANNON_DIRS {
        // Walk to the first piece on the ray (the screen).
        let mut cur = sq.offset(df, dr);
        let screen = loop {
            match cur {
                None => break None,
                Some(next) if occupied.contains(next) => break Some(next),
                Some(next) => cur = next.offset(df, dr),
            }
        };
        let Some(screen) = screen else { continue };
        // The screen may not be a cannon.
        if cannons.contains(screen) {
            continue;
        }
        // Walk past the screen to the next piece (the capture target).
        let mut cur = screen.offset(df, dr);
        while let Some(next) = cur {
            if occupied.contains(next) {
                // A cannon may not capture another cannon.
                if !cannons.contains(next) {
                    bb.set(next);
                }
                break;
            }
            cur = next.offset(df, dr);
        }
    }
    bb
}

// ---------------------------------------------------------------------------
// Blockable-leg leapers (Xiangqi horse and elephant).
// ---------------------------------------------------------------------------

/// The eight Xiangqi-horse leaps paired with their **hobbling leg** — the
/// orthogonally-adjacent square in the direction of the leap's long axis, which
/// blocks the leap when occupied. Each entry is `(target_df, target_dr, leg_df,
/// leg_dr)`: a knight target `(±1,±2)` / `(±2,±1)` whose leg is the single
/// orthogonal step toward it `(0,±1)` / `(±1,0)`.
const HORSE_LEGS: [(i8, i8, i8, i8); 8] = [
    (1, 2, 0, 1),
    (-1, 2, 0, 1),
    (1, -2, 0, -1),
    (-1, -2, 0, -1),
    (2, 1, 1, 0),
    (2, -1, 1, 0),
    (-2, 1, -1, 0),
    (-2, -1, -1, 0),
];

/// Returns the squares a Xiangqi **horse** on `sq` attacks given the `occupied`
/// set: the knight targets minus any whose **hobbling leg** is occupied.
///
/// A horse leaps like a knight, but each of its eight leaps is blocked ("the
/// horse's leg is hobbled") if the orthogonally-adjacent square one step toward
/// the leap's long axis is occupied — a non-symmetric, occupancy-aware rule. For
/// a leap that moves two ranks (`(±1,±2)`) the leg is the square one rank toward
/// it `(0,±1)`; for one that moves two files (`(±2,±1)`) the leg is one file
/// toward it `(±1,0)`. Edges are respected: a leap whose target or leg falls off
/// the board contributes nothing, and the masked shifts never wrap.
///
/// ```
/// use mce::geometry::{attacks::horse_attacks, Bitboard, Square, Xiangqi9x10};
/// // A horse in the open reaches all eight knight squares with no leg blocked.
/// let sq = Square::<Xiangqi9x10>::from_file_rank(4, 4).unwrap();
/// assert_eq!(horse_attacks::<Xiangqi9x10>(sq, Bitboard::EMPTY).count(), 8);
/// ```
#[must_use]
pub fn horse_attacks<G: Geometry>(sq: Square<G>, occupied: Bitboard<G>) -> Bitboard<G> {
    let mut bb = Bitboard::EMPTY;
    for &(tf, tr, lf, lr) in &HORSE_LEGS {
        // The leg must be on the board and empty for the leap to be available.
        let Some(leg) = sq.offset(lf, lr) else {
            continue;
        };
        if occupied.contains(leg) {
            continue;
        }
        if let Some(dest) = sq.offset(tf, tr) {
            bb.set(dest);
        }
    }
    bb
}

/// The four Xiangqi-elephant leaps paired with their **eye** — the intervening
/// diagonal square that blocks the two-step jump when occupied. Each entry is
/// `(target_df, target_dr, eye_df, eye_dr)`: a two-diagonal target `(±2,±2)`
/// whose eye is the one-diagonal square halfway to it `(±1,±1)`.
const ELEPHANT_EYES: [(i8, i8, i8, i8); 4] = [
    (2, 2, 1, 1),
    (2, -2, 1, -1),
    (-2, 2, -1, 1),
    (-2, -2, -1, -1),
];

/// Returns the squares a Xiangqi **elephant** on `sq` attacks given the
/// `occupied` set: the four two-square-diagonal targets minus any whose **eye**
/// (the intervening diagonal square) is occupied.
///
/// An elephant jumps exactly two squares diagonally, but the jump is blocked
/// ("blocking the elephant's eye") if the single diagonal square between `sq` and
/// the target is occupied. This is geometry-only and occupancy-aware; the
/// **river** confinement (an elephant may not cross to the far half) is *not*
/// applied here — the variant masks the result to the elephant's own half, the
/// same way it adds palace and river masks on top of the other primitives.
///
/// ```
/// use mce::geometry::{attacks::elephant_attacks_blockable, Bitboard, Square, Xiangqi9x10};
/// // A central elephant on an empty board reaches all four two-diagonal squares.
/// let sq = Square::<Xiangqi9x10>::from_file_rank(4, 4).unwrap();
/// assert_eq!(elephant_attacks_blockable::<Xiangqi9x10>(sq, Bitboard::EMPTY).count(), 4);
/// ```
#[must_use]
pub fn elephant_attacks_blockable<G: Geometry>(
    sq: Square<G>,
    occupied: Bitboard<G>,
) -> Bitboard<G> {
    let mut bb = Bitboard::EMPTY;
    for &(tf, tr, ef, er) in &ELEPHANT_EYES {
        let Some(eye) = sq.offset(ef, er) else {
            continue;
        };
        if occupied.contains(eye) {
            continue;
        }
        if let Some(dest) = sq.offset(tf, tr) {
            bb.set(dest);
        }
    }
    bb
}

/// The eight Janggi-elephant (象) leaps, each paired with the **two intervening
/// squares** that block the jump when occupied. Each entry is
/// `(target_df, target_dr, leg1_df, leg1_dr, leg2_df, leg2_dr)`: the elephant
/// steps **one orthogonal square** (leg 1), then **two diagonal squares**
/// continuing outward; leg 2 is the first of those diagonal squares. The target
/// is `(±2,±3)` / `(±3,±2)` — a longer leap than the Xiangqi elephant's `(±2,±2)`.
///
/// For an up/down-biased leap (target `(±2,±3)`) leg 1 is the orthogonal step in
/// the rank direction `(0,±1)`; for a left/right-biased leap (target `(±3,±2)`)
/// leg 1 is the orthogonal step in the file direction `(±1,0)`. Leg 2 is the
/// diagonal square one step past leg 1 toward the target.
const JANGGI_ELEPHANT_LEGS: [(i8, i8, i8, i8, i8, i8); 8] = [
    // Up-biased (two files, three ranks): ortho step is the rank step (0,±1).
    (2, 3, 0, 1, 1, 2),
    (-2, 3, 0, 1, -1, 2),
    (2, -3, 0, -1, 1, -2),
    (-2, -3, 0, -1, -1, -2),
    // Side-biased (three files, two ranks): ortho step is the file step (±1,0).
    (3, 2, 1, 0, 2, 1),
    (3, -2, 1, 0, 2, -1),
    (-3, 2, -1, 0, -2, 1),
    (-3, -2, -1, 0, -2, -1),
];

/// Returns the squares a **Janggi elephant** (象) on `sq` attacks given the
/// `occupied` set: the eight `(±2,±3)` / `(±3,±2)` targets minus any whose path is
/// blocked.
///
/// The Janggi elephant moves **one square orthogonally then two squares
/// diagonally** outward — a longer leap than the Xiangqi elephant's two-diagonal
/// jump. The path is **blockable at each intervening square**: the orthogonal step
/// (leg 1) and the first diagonal step (leg 2). If either is occupied, that leap
/// is unavailable. Unlike the Xiangqi elephant, the Janggi elephant is **not
/// river-bound** (it roams the whole board); the variant applies no half-mask.
///
/// Geometry-only and occupancy-aware; edges are respected (an off-board target or
/// leg contributes nothing, and the masked offsets never wrap).
///
/// ```
/// use mce::geometry::{attacks::janggi_elephant_attacks, Bitboard, Square, Xiangqi9x10};
/// // A central elephant on an empty board reaches all eight long-diagonal squares.
/// let sq = Square::<Xiangqi9x10>::from_file_rank(4, 4).unwrap();
/// assert_eq!(janggi_elephant_attacks::<Xiangqi9x10>(sq, Bitboard::EMPTY).count(), 8);
/// ```
#[must_use]
pub fn janggi_elephant_attacks<G: Geometry>(sq: Square<G>, occupied: Bitboard<G>) -> Bitboard<G> {
    let mut bb = Bitboard::EMPTY;
    for &(tf, tr, l1f, l1r, l2f, l2r) in &JANGGI_ELEPHANT_LEGS {
        // Leg 1: the orthogonal step must be on the board and empty.
        let Some(leg1) = sq.offset(l1f, l1r) else {
            continue;
        };
        if occupied.contains(leg1) {
            continue;
        }
        // Leg 2: the first diagonal step must be on the board and empty.
        let Some(leg2) = sq.offset(l2f, l2r) else {
            continue;
        };
        if occupied.contains(leg2) {
            continue;
        }
        if let Some(dest) = sq.offset(tf, tr) {
            bb.set(dest);
        }
    }
    bb
}

// ---------------------------------------------------------------------------
// Geometry rays: `between` and `line`.
// ---------------------------------------------------------------------------

/// Returns the unit step `(df, dr)` from `a` toward `b` when they are aligned on
/// a rank, file, or diagonal; `None` otherwise (including when `a == b`).
fn step_toward<G: Geometry>(a: Square<G>, b: Square<G>) -> Option<(i8, i8)> {
    let df = b.file() as i16 - a.file() as i16;
    let dr = b.rank() as i16 - a.rank() as i16;
    if df == 0 && dr == 0 {
        return None;
    }
    let sign = |x: i16| -> i8 {
        if x > 0 {
            1
        } else if x < 0 {
            -1
        } else {
            0
        }
    };
    if df == 0 || dr == 0 || df == dr || df == -dr {
        Some((sign(df), sign(dr)))
    } else {
        None
    }
}

/// Returns the squares strictly between `a` and `b` when they share a rank,
/// file, or diagonal; otherwise the empty set.
///
/// The endpoints are never included; adjacent or identical aligned squares yield
/// the empty set.
///
/// ```
/// use mce::geometry::{attacks::between, Chess8x8, Square};
/// // C1 to C8 on an 8x8 board: the six squares strictly between.
/// assert_eq!(between::<Chess8x8>(Square::new(2), Square::new(58)).count(), 6);
/// ```
#[must_use]
pub fn between<G: Geometry>(a: Square<G>, b: Square<G>) -> Bitboard<G> {
    let Some((df, dr)) = step_toward(a, b) else {
        return Bitboard::EMPTY;
    };
    let mut bb = Bitboard::EMPTY;
    let mut cur = a.offset(df, dr);
    while let Some(next) = cur {
        if next == b {
            break;
        }
        bb.set(next);
        cur = next.offset(df, dr);
    }
    bb
}

/// Returns the full rank, file, or diagonal line through `a` and `b`, extended
/// to the board edges; the empty set if they are not aligned.
///
/// Both endpoints are included.
///
/// ```
/// use mce::geometry::{attacks::line, Chess8x8, Square};
/// // A whole 8x8 file line has eight squares.
/// assert_eq!(line::<Chess8x8>(Square::new(2), Square::new(34)).count(), 8);
/// ```
#[must_use]
pub fn line<G: Geometry>(a: Square<G>, b: Square<G>) -> Bitboard<G> {
    let Some((df, dr)) = step_toward(a, b) else {
        return Bitboard::EMPTY;
    };
    walk_line(a, df, dr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Cap10x8, Chess8x8};
    use crate::{attacks as concrete, Bitboard as CBitboard, Square as CSquare};
    use alloc::vec::Vec;

    /// Maps a generic 8x8 bitboard to the concrete one for direct comparison.
    fn c(bb: Bitboard<Chess8x8>) -> CBitboard {
        CBitboard(bb.0)
    }

    /// A deterministic xorshift over `u64` for sampling occupancies.
    fn rng() -> impl FnMut() -> u64 {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        }
    }

    // ----- 8x8 equivalence with the frozen concrete path ----------------------

    #[test]
    fn pawn_equivalence_all_squares() {
        for index in 0..64u8 {
            let g = Square::<Chess8x8>::new(index);
            let cs = CSquare::new(index);
            assert_eq!(
                c(pawn_attacks::<Chess8x8>(Color::White, g)),
                concrete::pawn_attacks(Color::White, cs),
                "white pawn {index}"
            );
            assert_eq!(
                c(pawn_attacks::<Chess8x8>(Color::Black, g)),
                concrete::pawn_attacks(Color::Black, cs),
                "black pawn {index}"
            );
        }
    }

    #[test]
    fn knight_king_equivalence_all_squares() {
        for index in 0..64u8 {
            let g = Square::<Chess8x8>::new(index);
            let cs = CSquare::new(index);
            assert_eq!(
                c(knight_attacks::<Chess8x8>(g)),
                concrete::knight_attacks(cs),
                "knight {index}"
            );
            assert_eq!(
                c(king_attacks::<Chess8x8>(g)),
                concrete::king_attacks(cs),
                "king {index}"
            );
        }
    }

    #[test]
    fn slider_equivalence_all_squares_and_occupancies() {
        let mut next = rng();
        for index in 0..64u8 {
            let g = Square::<Chess8x8>::new(index);
            let cs = CSquare::new(index);
            // Empty and full boards explicitly, plus many random occupancies.
            let mut occs = Vec::new();
            occs.push(0u64);
            occs.push(!0u64);
            for _ in 0..64 {
                occs.push(next());
            }
            for &raw in &occs {
                let go = Bitboard::<Chess8x8>(raw);
                let co = CBitboard(raw);
                assert_eq!(
                    c(rook_attacks::<Chess8x8>(g, go)),
                    concrete::rook_attacks(cs, co),
                    "rook {index} occ {raw:#x}"
                );
                assert_eq!(
                    c(bishop_attacks::<Chess8x8>(g, go)),
                    concrete::bishop_attacks(cs, co),
                    "bishop {index} occ {raw:#x}"
                );
                assert_eq!(
                    c(queen_attacks::<Chess8x8>(g, go)),
                    concrete::queen_attacks(cs, co),
                    "queen {index} occ {raw:#x}"
                );
            }
        }
    }

    #[test]
    fn between_line_equivalence_all_pairs() {
        for a in 0..64u8 {
            for b in 0..64u8 {
                let ga = Square::<Chess8x8>::new(a);
                let gb = Square::<Chess8x8>::new(b);
                let ca = CSquare::new(a);
                let cb = CSquare::new(b);
                assert_eq!(
                    c(between::<Chess8x8>(ga, gb)),
                    concrete::between(ca, cb),
                    "between {a} {b}"
                );
                assert_eq!(
                    c(line::<Chess8x8>(ga, gb)),
                    concrete::line(ca, cb),
                    "line {a} {b}"
                );
            }
        }
    }

    // ----- Leaper helper edge behaviour ---------------------------------------

    #[test]
    fn leaper_respects_edges_8x8() {
        // Ferz (diagonal one-steppers) on a corner reaches exactly one square.
        let ferz = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
        assert_eq!(leaper_attacks::<Chess8x8>(Square::new(0), &ferz).count(), 1);
        // Wazir on a corner reaches exactly two squares.
        let wazir = [(1, 0), (-1, 0), (0, 1), (0, -1)];
        assert_eq!(
            leaper_attacks::<Chess8x8>(Square::new(0), &wazir).count(),
            2
        );
    }

    // ----- Cannon primitive (Xiangqi / Janggi / Shako) ------------------------

    #[test]
    fn cannon_quiet_matches_rook_on_empty_board() {
        // With no pieces to capture, the cannon's quiet moves are exactly a
        // rook's slides (no first-blocker capture, since nothing is occupied).
        for index in 0..64u8 {
            let sq = Square::<Chess8x8>::new(index);
            let q = cannon_quiet_moves::<Chess8x8>(sq, Bitboard::EMPTY);
            assert_eq!(
                q,
                rook_attacks::<Chess8x8>(sq, Bitboard::EMPTY),
                "sq {index}"
            );
            // Nothing to capture over: no screen anywhere.
            assert_eq!(
                cannon_capture_targets::<Chess8x8>(sq, Bitboard::EMPTY),
                Bitboard::EMPTY
            );
        }
    }

    #[test]
    fn cannon_quiet_stops_before_first_piece() {
        // a1 cannon, blocker on a4 (index 24): quiet a-file moves are a2, a3
        // only (it stops one short of the blocker and never lands on it).
        let occ = Bitboard::<Chess8x8>::EMPTY.with(Square::new(24));
        let q = cannon_quiet_moves::<Chess8x8>(Square::new(0), occ);
        assert!(q.contains(Square::new(8))); // a2
        assert!(q.contains(Square::new(16))); // a3
        assert!(!q.contains(Square::new(24))); // a4 (the blocker) — not quiet
        assert!(!q.contains(Square::new(32))); // a5 (beyond) — blocked
                                               // The single blocker is a lone screen with nothing beyond it: no capture.
        assert_eq!(
            cannon_capture_targets::<Chess8x8>(Square::new(0), occ),
            Bitboard::EMPTY
        );
    }

    #[test]
    fn cannon_captures_over_exactly_one_screen() {
        // a1 cannon, screen on a4, enemy on a7: captures a7 over the screen.
        let occ = Bitboard::<Chess8x8>::EMPTY
            .with(Square::new(24)) // a4 screen
            .with(Square::new(48)); // a7 target
        let caps = cannon_capture_targets::<Chess8x8>(Square::new(0), occ);
        assert_eq!(caps, Bitboard::EMPTY.with(Square::new(48)));
        // The screen itself is never a capture target, nor a quiet square.
        let q = cannon_quiet_moves::<Chess8x8>(Square::new(0), occ);
        assert!(!q.contains(Square::new(24)));
        assert!(!q.contains(Square::new(48)));
    }

    #[test]
    fn cannon_needs_exactly_one_screen_not_two() {
        // a1 cannon, two adjacent screens a3+a4, then a gap, then a target a7:
        // the FIRST piece beyond the (a3) screen is a4 — itself the second
        // contiguous piece — so the cannon's only target is a4. The shielded a7
        // is unreachable: a capture lands on the first piece past exactly one
        // screen, and two pieces sit between the cannon and a7.
        let occ = Bitboard::<Chess8x8>::EMPTY
            .with(Square::new(16)) // a3 screen
            .with(Square::new(24)) // a4 first piece beyond the screen (target)
            .with(Square::new(48)); // a7 (shielded by the a3+a4 double block)
        let caps = cannon_capture_targets::<Chess8x8>(Square::new(0), occ);
        assert_eq!(
            caps,
            Bitboard::EMPTY.with(Square::new(24)),
            "capture is the first piece past one screen; a7 is shielded by two pieces"
        );
        assert!(!caps.contains(Square::new(48)), "two pieces shield a7");
    }

    #[test]
    fn cannon_capture_is_first_piece_beyond_screen_only() {
        // a1 cannon, screen a4, then a6 and a8 both occupied: only the FIRST
        // piece beyond the screen (a6) is a target; a8 is shielded by a6.
        let occ = Bitboard::<Chess8x8>::EMPTY
            .with(Square::new(24)) // a4 screen
            .with(Square::new(40)) // a6 first target
            .with(Square::new(56)); // a8 (shielded)
        let caps = cannon_capture_targets::<Chess8x8>(Square::new(0), occ);
        assert_eq!(caps, Bitboard::EMPTY.with(Square::new(40)));
    }

    #[test]
    fn cannon_independent_per_ray() {
        // Central cannon on d4 (index 27) with a screen+target on each of the
        // four rays captures exactly four squares, one per direction.
        let d4 = Square::<Chess8x8>::new(27);
        // North: d6 screen (43), d8 target (59). South: d2 screen (11), d... none
        // below; use d3 screen (19) and d1 (3) target. East: f4 screen (29), h4
        // (31) target. West: b4 screen (25), a4 (24) target.
        let occ = Bitboard::<Chess8x8>::EMPTY
            .with(Square::new(43))
            .with(Square::new(59))
            .with(Square::new(19))
            .with(Square::new(3))
            .with(Square::new(29))
            .with(Square::new(31))
            .with(Square::new(25))
            .with(Square::new(24));
        let caps = cannon_capture_targets::<Chess8x8>(d4, occ);
        let expect = Bitboard::<Chess8x8>::EMPTY
            .with(Square::new(59))
            .with(Square::new(3))
            .with(Square::new(31))
            .with(Square::new(24));
        assert_eq!(caps, expect);
    }

    #[test]
    fn cannon_does_not_wrap_or_leak_off_board() {
        // On the 10x10 u128 geometry, a cannon's moves and captures stay on the
        // board and never wrap across the file edges.
        use crate::geometry::Grand10x10;
        let off = !Grand10x10::BOARD_MASK;
        // Dense occupancy: the whole board.
        let occ = Bitboard::<Grand10x10>(Grand10x10::BOARD_MASK);
        for index in 0..100u8 {
            let sq = Square::<Grand10x10>::new(index);
            assert_eq!(cannon_quiet_moves::<Grand10x10>(sq, occ).0 & off, 0);
            assert_eq!(cannon_capture_targets::<Grand10x10>(sq, occ).0 & off, 0);
            // Captures and quiets are disjoint and both lie on rook lines.
            let q = cannon_quiet_moves::<Grand10x10>(sq, Bitboard::EMPTY);
            for dest in q {
                assert!(dest.file() == sq.file() || dest.rank() == sq.rank());
            }
        }
        // Edge wrap: a cannon on the last file (j, file 9) of rank 0 with a
        // screen+target on the next rank's a-file must NOT capture across the
        // wrap. Put a screen on i1 (file 8 rank 0) and a target at file 7 rank 0:
        // capture stays on rank 0.
        let j1 = Square::<Grand10x10>::from_file_rank(9, 0).unwrap();
        let occ = Bitboard::<Grand10x10>::EMPTY
            .with(Square::from_file_rank(8, 0).unwrap()) // i1 screen
            .with(Square::from_file_rank(7, 0).unwrap()); // h1 target
        let caps = cannon_capture_targets::<Grand10x10>(j1, occ);
        assert_eq!(
            caps,
            Bitboard::EMPTY.with(Square::from_file_rank(7, 0).unwrap())
        );
    }

    // ----- Cap10x8 (u128) brute-force cross-checks ----------------------------

    /// Independent reference: scan each direction until off-board or a blocker
    /// (inclusive). Used to validate the generic sliders on the `u128` board.
    fn scan_rays<G: Geometry>(
        sq: Square<G>,
        occupied: Bitboard<G>,
        dirs: &[(i8, i8)],
    ) -> Bitboard<G> {
        let mut out = Bitboard::EMPTY;
        for &(df, dr) in dirs {
            let mut cur = sq.offset(df, dr);
            while let Some(next) = cur {
                out.set(next);
                if occupied.contains(next) {
                    break;
                }
                cur = next.offset(df, dr);
            }
        }
        out
    }

    #[test]
    fn cap10x8_sliders_match_ray_scan() {
        // A handful of structured occupancies plus randomised ones, confined to
        // the 80 on-board squares.
        let mut next = rng();
        let board = Cap10x8::BOARD_MASK;
        let mut occs: Vec<u128> = Vec::new();
        occs.push(0);
        occs.push(board);
        for _ in 0..32 {
            let lo = next() as u128;
            let hi = (next() as u128) << 64;
            occs.push((lo | hi) & board);
        }
        for index in 0..80u8 {
            let sq = Square::<Cap10x8>::new(index);
            for &raw in &occs {
                let occ = Bitboard::<Cap10x8>(raw);
                assert_eq!(
                    rook_attacks::<Cap10x8>(sq, occ),
                    scan_rays(sq, occ, &[(0, 1), (0, -1), (1, 0), (-1, 0)]),
                    "rook {index} occ {raw:#x}"
                );
                assert_eq!(
                    bishop_attacks::<Cap10x8>(sq, occ),
                    scan_rays(sq, occ, &[(1, 1), (1, -1), (-1, 1), (-1, -1)]),
                    "bishop {index} occ {raw:#x}"
                );
                assert_eq!(
                    queen_attacks::<Cap10x8>(sq, occ),
                    rook_attacks::<Cap10x8>(sq, occ) | bishop_attacks::<Cap10x8>(sq, occ),
                    "queen {index} occ {raw:#x}"
                );
            }
        }
    }

    #[test]
    fn grand10x10_sliders_match_ray_scan() {
        // Grand chess is ten ranks as well as ten files, so its longest diagonal
        // spans all ten cells — the case that exercises the diagonal fill past the
        // eight-cell diagonals of `Cap10x8`. Cross-check every square and a basket
        // of occupancies against an independent ray scan.
        use crate::geometry::Grand10x10;
        let mut next = rng();
        let board = Grand10x10::BOARD_MASK;
        let mut occs: Vec<u128> = Vec::new();
        occs.push(0);
        occs.push(board);
        for _ in 0..32 {
            let lo = next() as u128;
            let hi = (next() as u128) << 64;
            occs.push((lo | hi) & board);
        }
        for index in 0..100u8 {
            let sq = Square::<Grand10x10>::new(index);
            for &raw in &occs {
                let occ = Bitboard::<Grand10x10>(raw);
                assert_eq!(
                    rook_attacks::<Grand10x10>(sq, occ),
                    scan_rays(sq, occ, &[(0, 1), (0, -1), (1, 0), (-1, 0)]),
                    "rook {index} occ {raw:#x}"
                );
                assert_eq!(
                    bishop_attacks::<Grand10x10>(sq, occ),
                    scan_rays(sq, occ, &[(1, 1), (1, -1), (-1, 1), (-1, -1)]),
                    "bishop {index} occ {raw:#x}"
                );
            }
        }
        // The full main diagonal (a1..j10) and anti-diagonal (a10..j1) span ten
        // cells; on an empty board a corner bishop must reach all nine others.
        let a1 = Square::<Grand10x10>::from_file_rank(0, 0).unwrap();
        let diag = bishop_attacks::<Grand10x10>(a1, Bitboard::EMPTY);
        assert!(
            diag.contains(Square::from_file_rank(9, 9).unwrap()),
            "a1 bishop must see the far corner j10 along the full ten-cell diagonal"
        );
        assert_eq!(
            diag.count(),
            9,
            "a1 bishop sees the other nine diagonal cells"
        );
    }

    #[test]
    fn cap10x8_attacks_stay_on_board() {
        // No generic attack may set a bit outside the 80-square board mask.
        let off = !Cap10x8::BOARD_MASK;
        let occ = Bitboard::<Cap10x8>(Cap10x8::BOARD_MASK);
        for index in 0..80u8 {
            let sq = Square::<Cap10x8>::new(index);
            assert_eq!(knight_attacks::<Cap10x8>(sq).0 & off, 0);
            assert_eq!(king_attacks::<Cap10x8>(sq).0 & off, 0);
            assert_eq!(pawn_attacks::<Cap10x8>(Color::White, sq).0 & off, 0);
            assert_eq!(rook_attacks::<Cap10x8>(sq, occ).0 & off, 0);
            assert_eq!(bishop_attacks::<Cap10x8>(sq, occ).0 & off, 0);
        }
    }

    #[test]
    fn cap10x8_knight_does_not_wrap_edges() {
        // A knight on the a-file (file 0) must not reach the j-file (file 9).
        for rank in 0..8u8 {
            let sq = Square::<Cap10x8>::from_file_rank(0, rank).unwrap();
            for dest in knight_attacks::<Cap10x8>(sq) {
                assert!(dest.file() <= 2, "a-file knight wrapped to {dest:?}");
            }
            let sq = Square::<Cap10x8>::from_file_rank(9, rank).unwrap();
            for dest in knight_attacks::<Cap10x8>(sq) {
                assert!(dest.file() >= 7, "j-file knight wrapped to {dest:?}");
            }
        }
    }

    #[test]
    fn cap10x8_rook_does_not_wrap_across_tenth_file() {
        // Rook on file 9 of rank 0 with an empty board: the east ray must be
        // empty (no wrap onto rank 1), and the rank ray must cover files 0..=8.
        let sq = Square::<Cap10x8>::from_file_rank(9, 0).unwrap();
        let attacks = rook_attacks::<Cap10x8>(sq, Bitboard::EMPTY);
        for dest in attacks {
            // Every attacked square shares either the file or the rank.
            assert!(
                dest.file() == 9 || dest.rank() == 0,
                "rook reached unaligned {dest:?}"
            );
        }
        // The rank ray reaches file 0 of rank 0 (index 0) without leaking up.
        assert!(attacks.contains(Square::new(0)));
        assert!(!attacks.contains(Square::new(10)), "wrapped onto rank 1");
    }

    // ----- Xiangqi blockable-leg leapers (horse, elephant) --------------------

    #[test]
    fn horse_unobstructed_reaches_eight_in_open() {
        use crate::geometry::Xiangqi9x10;
        let sq = Square::<Xiangqi9x10>::from_file_rank(4, 4).unwrap();
        let a = horse_attacks::<Xiangqi9x10>(sq, Bitboard::EMPTY);
        assert_eq!(a.count(), 8);
        // Every target is a knight leap from `sq`.
        for d in a {
            let df = (d.file() as i8 - sq.file() as i8).abs();
            let dr = (d.rank() as i8 - sq.rank() as i8).abs();
            assert!((df, dr) == (1, 2) || (df, dr) == (2, 1), "{d:?}");
        }
    }

    #[test]
    fn horse_each_leg_blocks_exactly_two_leaps() {
        use crate::geometry::Xiangqi9x10;
        let sq = Square::<Xiangqi9x10>::from_file_rank(4, 4).unwrap(); // e5, index 40
                                                                       // North leg (0,+1): blocks the two north leaps (±1,+2) -> d7,f7.
        let leg_n = Bitboard::<Xiangqi9x10>::EMPTY.with(sq.offset(0, 1).unwrap());
        let an = horse_attacks::<Xiangqi9x10>(sq, leg_n);
        assert_eq!(an.count(), 6);
        assert!(!an.contains(sq.offset(-1, 2).unwrap()));
        assert!(!an.contains(sq.offset(1, 2).unwrap()));
        // The east leg (+1,0) and the other leaps stay available.
        assert!(an.contains(sq.offset(2, 1).unwrap()));

        // East leg (+1,0): blocks the two east leaps (+2,±1).
        let leg_e = Bitboard::<Xiangqi9x10>::EMPTY.with(sq.offset(1, 0).unwrap());
        let ae = horse_attacks::<Xiangqi9x10>(sq, leg_e);
        assert_eq!(ae.count(), 6);
        assert!(!ae.contains(sq.offset(2, 1).unwrap()));
        assert!(!ae.contains(sq.offset(2, -1).unwrap()));
        // The north leaps remain.
        assert!(ae.contains(sq.offset(1, 2).unwrap()));
    }

    #[test]
    fn horse_does_not_wrap_edges() {
        use crate::geometry::Xiangqi9x10;
        // A horse on the a-file (file 0) must not leap to the i-file (file 8).
        for rank in 0..10u8 {
            let sq = Square::<Xiangqi9x10>::from_file_rank(0, rank).unwrap();
            for d in horse_attacks::<Xiangqi9x10>(sq, Bitboard::EMPTY) {
                assert!(d.file() <= 2, "a-file horse wrapped to {d:?}");
            }
            let sq = Square::<Xiangqi9x10>::from_file_rank(8, rank).unwrap();
            for d in horse_attacks::<Xiangqi9x10>(sq, Bitboard::EMPTY) {
                assert!(d.file() >= 6, "i-file horse wrapped to {d:?}");
            }
        }
    }

    #[test]
    fn elephant_unobstructed_reaches_four_in_open() {
        use crate::geometry::Xiangqi9x10;
        let sq = Square::<Xiangqi9x10>::from_file_rank(4, 4).unwrap();
        let a = elephant_attacks_blockable::<Xiangqi9x10>(sq, Bitboard::EMPTY);
        assert_eq!(a.count(), 4);
        for d in a {
            let df = (d.file() as i8 - sq.file() as i8).abs();
            let dr = (d.rank() as i8 - sq.rank() as i8).abs();
            assert_eq!((df, dr), (2, 2), "{d:?}");
        }
    }

    #[test]
    fn elephant_each_eye_blocks_one_leap() {
        use crate::geometry::Xiangqi9x10;
        let sq = Square::<Xiangqi9x10>::from_file_rank(4, 4).unwrap();
        // Block the NE eye (+1,+1): removes only the NE target (+2,+2).
        let eye = Bitboard::<Xiangqi9x10>::EMPTY.with(sq.offset(1, 1).unwrap());
        let a = elephant_attacks_blockable::<Xiangqi9x10>(sq, eye);
        assert_eq!(a.count(), 3);
        assert!(!a.contains(sq.offset(2, 2).unwrap()));
        // The other three eyes are clear, so their targets remain.
        assert!(a.contains(sq.offset(-2, 2).unwrap()));
        assert!(a.contains(sq.offset(2, -2).unwrap()));
        assert!(a.contains(sq.offset(-2, -2).unwrap()));
    }

    #[test]
    fn elephant_does_not_wrap_edges() {
        use crate::geometry::Xiangqi9x10;
        // An elephant on the a-file may only reach the c-file (file 2), never the
        // far side by wrapping.
        for rank in 0..10u8 {
            let sq = Square::<Xiangqi9x10>::from_file_rank(0, rank).unwrap();
            for d in elephant_attacks_blockable::<Xiangqi9x10>(sq, Bitboard::EMPTY) {
                assert_eq!(d.file(), 2, "a-file elephant wrapped to {d:?}");
            }
        }
    }

    // ----- Janggi cannon (screen-mandatory, screen/target may not be a cannon) --

    #[test]
    fn janggi_cannon_needs_a_screen_no_empty_ray_move() {
        use crate::geometry::Xiangqi9x10;
        // A lone cannon on an empty board can neither move nor capture: with no
        // screen on any ray, both sets are empty (unlike the Xiangqi cannon, whose
        // quiet set is the full rook rays).
        let sq = Square::<Xiangqi9x10>::from_file_rank(4, 4).unwrap();
        assert_eq!(
            janggi_cannon_quiet::<Xiangqi9x10>(sq, Bitboard::EMPTY, Bitboard::EMPTY),
            Bitboard::EMPTY
        );
        assert_eq!(
            janggi_cannon_capture::<Xiangqi9x10>(sq, Bitboard::EMPTY, Bitboard::EMPTY),
            Bitboard::EMPTY
        );
    }

    #[test]
    fn janggi_cannon_jumps_one_screen_quiet_and_capture() {
        use crate::geometry::Chess8x8;
        // a1 cannon; a4 a non-cannon screen; a7 a non-cannon enemy. Quiet jumps
        // land on a5, a6 (empty past the screen, before the target); the capture
        // lands on a7.
        let occ = Bitboard::<Chess8x8>::EMPTY
            .with(Square::new(24)) // a4 screen
            .with(Square::new(48)); // a7 target
        let q = janggi_cannon_quiet::<Chess8x8>(Square::new(0), occ, Bitboard::EMPTY);
        assert!(q.contains(Square::new(32))); // a5
        assert!(q.contains(Square::new(40))); // a6
        assert!(!q.contains(Square::new(24))); // the screen
        assert!(!q.contains(Square::new(48))); // the target is a capture, not quiet
        let caps = janggi_cannon_capture::<Chess8x8>(Square::new(0), occ, Bitboard::EMPTY);
        assert_eq!(caps, Bitboard::EMPTY.with(Square::new(48)));
    }

    #[test]
    fn janggi_cannon_screen_may_not_be_a_cannon() {
        use crate::geometry::Chess8x8;
        // a1 cannon; the screen a4 IS a cannon; enemy non-cannon a7. The ray is
        // dead — no quiet jump and no capture, because a cannon cannot mount over
        // another cannon.
        let occ = Bitboard::<Chess8x8>::EMPTY
            .with(Square::new(24)) // a4 screen (a cannon)
            .with(Square::new(48)); // a7 target
        let cannons = Bitboard::<Chess8x8>::EMPTY.with(Square::new(24));
        assert_eq!(
            janggi_cannon_quiet::<Chess8x8>(Square::new(0), occ, cannons),
            Bitboard::EMPTY
        );
        assert_eq!(
            janggi_cannon_capture::<Chess8x8>(Square::new(0), occ, cannons),
            Bitboard::EMPTY
        );
    }

    #[test]
    fn janggi_cannon_may_not_capture_a_cannon() {
        use crate::geometry::Chess8x8;
        // a1 cannon; non-cannon screen a4; the target a7 IS a cannon. The capture
        // is forbidden, but the empty squares between (a5, a6) are still quiet
        // jumps.
        let occ = Bitboard::<Chess8x8>::EMPTY
            .with(Square::new(24)) // a4 screen (not a cannon)
            .with(Square::new(48)); // a7 target (a cannon)
        let cannons = Bitboard::<Chess8x8>::EMPTY.with(Square::new(48));
        assert_eq!(
            janggi_cannon_capture::<Chess8x8>(Square::new(0), occ, cannons),
            Bitboard::EMPTY,
            "a cannon may not capture a cannon"
        );
        let q = janggi_cannon_quiet::<Chess8x8>(Square::new(0), occ, cannons);
        assert!(q.contains(Square::new(32)) && q.contains(Square::new(40)));
    }

    // ----- Janggi elephant (1 orthogonal + 2 diagonal, blockable at each step) --

    #[test]
    fn janggi_elephant_open_reaches_eight_long_leaps() {
        use crate::geometry::Xiangqi9x10;
        let sq = Square::<Xiangqi9x10>::from_file_rank(4, 4).unwrap();
        let a = janggi_elephant_attacks::<Xiangqi9x10>(sq, Bitboard::EMPTY);
        assert_eq!(a.count(), 8);
        for d in a {
            let df = (d.file() as i8 - sq.file() as i8).abs();
            let dr = (d.rank() as i8 - sq.rank() as i8).abs();
            assert!((df, dr) == (2, 3) || (df, dr) == (3, 2), "{d:?}");
        }
    }

    #[test]
    fn janggi_elephant_orthogonal_leg_blocks_two_leaps() {
        use crate::geometry::Xiangqi9x10;
        // e5 elephant (file 4, rank 4). The north orthogonal leg (0,+1) is shared
        // by the two up-biased north leaps to (±2,+3): blocking it removes both,
        // leaving six.
        let sq = Square::<Xiangqi9x10>::from_file_rank(4, 4).unwrap();
        let leg = Bitboard::<Xiangqi9x10>::EMPTY.with(sq.offset(0, 1).unwrap());
        let a = janggi_elephant_attacks::<Xiangqi9x10>(sq, leg);
        assert_eq!(a.count(), 6);
        assert!(!a.contains(sq.offset(2, 3).unwrap()));
        assert!(!a.contains(sq.offset(-2, 3).unwrap()));
        // A side-biased leap is unaffected.
        assert!(a.contains(sq.offset(3, 2).unwrap()));
    }

    #[test]
    fn janggi_elephant_diagonal_leg_blocks_one_leap() {
        use crate::geometry::Xiangqi9x10;
        // e5 elephant. The NE diagonal leg (+1,+2) lies on the path to only the
        // (+2,+3) leap: blocking it removes exactly that one leap (seven remain).
        let sq = Square::<Xiangqi9x10>::from_file_rank(4, 4).unwrap();
        let leg = Bitboard::<Xiangqi9x10>::EMPTY.with(sq.offset(1, 2).unwrap());
        let a = janggi_elephant_attacks::<Xiangqi9x10>(sq, leg);
        assert_eq!(a.count(), 7);
        assert!(!a.contains(sq.offset(2, 3).unwrap()));
        // Its mirror (the -2,+3 leap, sharing the same ortho leg but a different
        // diagonal leg) is still available.
        assert!(a.contains(sq.offset(-2, 3).unwrap()));
    }

    #[test]
    fn janggi_elephant_does_not_wrap_edges() {
        use crate::geometry::Xiangqi9x10;
        // An a-file elephant (file 0) may only reach files 2 and 3, never wrap.
        for rank in 0..10u8 {
            let sq = Square::<Xiangqi9x10>::from_file_rank(0, rank).unwrap();
            for d in janggi_elephant_attacks::<Xiangqi9x10>(sq, Bitboard::EMPTY) {
                assert!(d.file() == 2 || d.file() == 3, "a-file elephant to {d:?}");
            }
        }
    }

    #[test]
    fn janggi_cannon_does_not_wrap_or_leak() {
        use crate::geometry::Grand10x10;
        let off = !Grand10x10::BOARD_MASK;
        let occ = Bitboard::<Grand10x10>(Grand10x10::BOARD_MASK);
        for index in 0..100u8 {
            let sq = Square::<Grand10x10>::new(index);
            assert_eq!(
                janggi_cannon_quiet::<Grand10x10>(sq, occ, Bitboard::EMPTY).0 & off,
                0
            );
            assert_eq!(
                janggi_cannon_capture::<Grand10x10>(sq, occ, Bitboard::EMPTY).0 & off,
                0
            );
        }
    }

    #[test]
    fn cap10x8_between_line_aligned() {
        // File line through file 5: between two ends excludes them and is a
        // subset of the line.
        let a = Square::<Cap10x8>::from_file_rank(5, 0).unwrap();
        let b = Square::<Cap10x8>::from_file_rank(5, 7).unwrap();
        let mid = between::<Cap10x8>(a, b);
        let whole = line::<Cap10x8>(a, b);
        assert_eq!(mid.count(), 6);
        assert!(!mid.contains(a) && !mid.contains(b));
        assert_eq!(mid & whole, mid);
        assert_eq!(whole.count(), 8);
        assert!(whole.contains(a) && whole.contains(b));
        // Non-aligned squares yield empty rays.
        let p = Square::<Cap10x8>::from_file_rank(0, 0).unwrap();
        let q = Square::<Cap10x8>::from_file_rank(1, 2).unwrap();
        assert_eq!(between::<Cap10x8>(p, q), Bitboard::EMPTY);
        assert_eq!(line::<Cap10x8>(p, q), Bitboard::EMPTY);
    }
}
