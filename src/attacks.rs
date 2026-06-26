//! Attack and ray lookups for move generation.
//!
//! This module provides the precomputed attack sets that move generation is
//! built on:
//!
//! * **Steppers** — [`pawn_attacks`], [`knight_attacks`], and [`king_attacks`]
//!   read from `const` tables computed at compile time.
//! * **Sliders** — [`bishop_attacks`], [`rook_attacks`], and [`queen_attacks`]
//!   compute attacks for a given occupancy. The slider implementation is the
//!   *hyperbola quintessence* technique: for each ray line through the slider's
//!   square, the blocker-aware attack set is obtained from
//!   `(o - 2s) ^ reverse(reverse(o) - 2 reverse(s))` restricted to the line
//!   mask, where `o` is the occupancy on the line and `s` is the slider square.
//! * **Geometry** — [`between`] and [`line()`] expose precomputed
//!   square-to-square ray tables used by pin and check detection.
//!
//! The whole module is `safe` Rust: steppers are computed with edge-masked bit
//! shifts and sliders use only wrapping/normal integer arithmetic, so no part of
//! it relies on `unsafe` or on out-of-range indexing.

use crate::{Bitboard, Color, Square};

/// Returns the squares a pawn of `color` standing on `sq` attacks.
///
/// Pawn attacks are the two forward diagonals (north for white, south for
/// black); captures off the edge of the board are masked away. A pawn on the
/// last rank attacks nothing.
///
/// ```
/// use mce::{attacks::pawn_attacks, Bitboard, Color, Square};
/// let a = pawn_attacks(Color::White, Square::E4);
/// assert_eq!(a, Bitboard::from(Square::D5) | Bitboard::from(Square::F5));
/// ```
#[must_use]
#[inline]
pub fn pawn_attacks(color: Color, sq: Square) -> Bitboard {
    PAWN_ATTACKS[color as usize][sq.index() as usize]
}

/// Returns the squares a knight on `sq` attacks.
///
/// ```
/// use mce::{attacks::knight_attacks, Square};
/// assert_eq!(knight_attacks(Square::A1).count(), 2);
/// assert_eq!(knight_attacks(Square::D4).count(), 8);
/// ```
#[must_use]
#[inline]
pub fn knight_attacks(sq: Square) -> Bitboard {
    KNIGHT_ATTACKS[sq.index() as usize]
}

/// Returns the squares a king on `sq` attacks (the up-to-eight adjacent
/// squares).
///
/// ```
/// use mce::{attacks::king_attacks, Square};
/// assert_eq!(king_attacks(Square::A1).count(), 3);
/// assert_eq!(king_attacks(Square::E4).count(), 8);
/// ```
#[must_use]
#[inline]
pub fn king_attacks(sq: Square) -> Bitboard {
    KING_ATTACKS[sq.index() as usize]
}

/// Returns the squares a bishop on `sq` attacks given the `occupied` set.
///
/// A ray stops at the first occupied square in each diagonal direction; that
/// blocking square is included in the result (it may be a capturable enemy
/// piece — masking out friendly pieces is the caller's job).
///
/// With the `magic` feature this dispatches to a magic-bitboard lookup; the
/// default build uses hyperbola quintessence. Both return identical sets.
#[must_use]
#[inline]
pub fn bishop_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    #[cfg(feature = "magic")]
    {
        crate::magic::bishop_attacks(sq, occupied)
    }
    #[cfg(not(feature = "magic"))]
    {
        bishop_attacks_hyperbola(sq, occupied)
    }
}

/// Returns the squares a rook on `sq` attacks given the `occupied` set.
///
/// A ray stops at the first occupied square along its file or rank; that
/// blocking square is included in the result.
///
/// With the `magic` feature this dispatches to a magic-bitboard lookup; the
/// default build uses hyperbola quintessence. Both return identical sets.
#[must_use]
#[inline]
pub fn rook_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    #[cfg(feature = "magic")]
    {
        crate::magic::rook_attacks(sq, occupied)
    }
    #[cfg(not(feature = "magic"))]
    {
        rook_attacks_hyperbola(sq, occupied)
    }
}

/// Hyperbola-quintessence bishop attacks — the default slider implementation
/// and the reference the magic cross-check test compares against.
///
/// Compiled for the default build and for any test build (the magic cross-check
/// uses it as ground truth); a non-test `--features magic` build dispatches
/// straight to the magic lookup and never references it.
#[cfg(any(not(feature = "magic"), test))]
#[must_use]
#[inline]
pub(crate) fn bishop_attacks_hyperbola(sq: Square, occupied: Bitboard) -> Bitboard {
    sliding(sq, occupied, DIAG[sq.index() as usize])
        | sliding(sq, occupied, ANTI_DIAG[sq.index() as usize])
}

/// Hyperbola-quintessence rook attacks — the default slider implementation and
/// the reference the magic cross-check test compares against.
#[cfg(any(not(feature = "magic"), test))]
#[must_use]
#[inline]
pub(crate) fn rook_attacks_hyperbola(sq: Square, occupied: Bitboard) -> Bitboard {
    sliding(sq, occupied, file_mask(sq)) | sliding(sq, occupied, rank_mask(sq))
}

/// Returns the squares a queen on `sq` attacks given the `occupied` set.
///
/// The queen combines the rook and bishop rays.
#[must_use]
#[inline]
pub fn queen_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    bishop_attacks(sq, occupied) | rook_attacks(sq, occupied)
}

/// Returns the squares strictly between `a` and `b` when they share a rank,
/// file, or diagonal; otherwise the empty set.
///
/// The endpoints `a` and `b` are never included. Adjacent or identical aligned
/// squares yield the empty set.
///
/// ```
/// use mce::{attacks::between, Bitboard, Square};
/// let mid = between(Square::C1, Square::C8);
/// assert_eq!(mid.count(), 6);
/// assert!(!mid.contains(Square::C1) && !mid.contains(Square::C8));
/// assert_eq!(between(Square::A1, Square::B3), Bitboard::EMPTY);
/// ```
#[must_use]
#[inline]
pub fn between(a: Square, b: Square) -> Bitboard {
    BETWEEN[a.index() as usize][b.index() as usize]
}

/// Returns the full rank, file, or diagonal line through `a` and `b`, extending
/// to the edges of the board; the empty set if the squares are not aligned.
///
/// Both endpoints are included.
///
/// ```
/// use mce::{attacks::line, Bitboard, Square};
/// let l = line(Square::C1, Square::C5);
/// assert_eq!(l, Bitboard::FILE_C);
/// assert_eq!(line(Square::A1, Square::B3), Bitboard::EMPTY);
/// ```
#[must_use]
#[inline]
pub fn line(a: Square, b: Square) -> Bitboard {
    LINE[a.index() as usize][b.index() as usize]
}

// ---------------------------------------------------------------------------
// Slider core (hyperbola quintessence).
// ---------------------------------------------------------------------------

/// Computes the blocker-aware attack set along a single ray `mask` (a rank,
/// file, diagonal, or anti-diagonal that passes through `sq`).
///
/// The result excludes `sq` itself and includes the first blocker on each side.
#[cfg(any(not(feature = "magic"), test))]
fn sliding(sq: Square, occupied: Bitboard, mask: Bitboard) -> Bitboard {
    let s = 1u64 << sq.index();
    let o = occupied.0 & mask.0;

    // Forward direction: subtracting `2s` from the occupancy on the line flips
    // every bit up to (and including) the first blocker above `sq`.
    let forward = o.wrapping_sub(s.wrapping_mul(2));

    // Reverse direction: the same trick on the bit-reversed line covers the
    // squares below `sq`.
    let rev_o = o.reverse_bits();
    let rev_s = s.reverse_bits();
    let reverse = rev_o.wrapping_sub(rev_s.wrapping_mul(2)).reverse_bits();

    Bitboard((forward ^ reverse) & mask.0)
}

/// Returns the file mask through `sq` (the whole file, all eight squares).
#[cfg(any(not(feature = "magic"), test))]
#[inline]
fn file_mask(sq: Square) -> Bitboard {
    Bitboard(Bitboard::FILE_A.0 << (sq.index() % 8))
}

/// Returns the rank mask through `sq` (the whole rank, all eight squares).
#[cfg(any(not(feature = "magic"), test))]
#[inline]
fn rank_mask(sq: Square) -> Bitboard {
    Bitboard(Bitboard::RANK_1.0 << (sq.index() & 56))
}

// ---------------------------------------------------------------------------
// Const stepper tables.
// ---------------------------------------------------------------------------

/// File `a` mask, used to suppress horizontal wraparound in const shifts.
const FILE_A: u64 = Bitboard::FILE_A.0;
/// File `h` mask, used to suppress horizontal wraparound in const shifts.
const FILE_H: u64 = Bitboard::FILE_H.0;
/// Files `a` and `b`, used for knight moves that span two files westward.
const FILE_AB: u64 = FILE_A | (FILE_A << 1);
/// Files `g` and `h`, used for knight moves that span two files eastward.
const FILE_GH: u64 = FILE_H | (FILE_H >> 1);

/// King attacks for the single-square set `b`, computed with edge-masked shifts.
const fn king_from(b: u64) -> u64 {
    // North and south of the square, then the square plus those two columns
    // shifted east and west (with file-edge masking to prevent wraparound).
    let vert = (b << 8) | (b >> 8);
    let span = b | vert;
    let east = (span & !FILE_H) << 1;
    let west = (span & !FILE_A) >> 1;
    vert | east | west
}

/// Knight attacks for the single-square set `b`, computed with edge-masked
/// shifts so moves never wrap around a file edge.
const fn knight_from(b: u64) -> u64 {
    let l1 = (b & !FILE_A) >> 1;
    let l2 = (b & !FILE_AB) >> 2;
    let r1 = (b & !FILE_H) << 1;
    let r2 = (b & !FILE_GH) << 2;
    let h1 = l1 | r1; // one file away
    let h2 = l2 | r2; // two files away
    (h1 << 16) | (h1 >> 16) | (h2 << 8) | (h2 >> 8)
}

/// White pawn attacks for the single-square set `b`.
const fn white_pawn_from(b: u64) -> u64 {
    ((b & !FILE_A) << 7) | ((b & !FILE_H) << 9)
}

/// Black pawn attacks for the single-square set `b`.
const fn black_pawn_from(b: u64) -> u64 {
    ((b & !FILE_H) >> 7) | ((b & !FILE_A) >> 9)
}

/// Identifies which stepper generator a [`build_steppers`] call should run.
///
/// Function pointers cannot be invoked in `const` evaluation, so the generator
/// is selected by this enum and dispatched with a `match` instead.
#[derive(Clone, Copy)]
enum Stepper {
    Knight,
    King,
    WhitePawn,
    BlackPawn,
}

/// Builds a `[Bitboard; 64]` stepper table for the given generator kind.
const fn build_steppers(kind: Stepper) -> [Bitboard; 64] {
    let mut table = [Bitboard::EMPTY; 64];
    let mut i = 0;
    while i < 64 {
        let b = 1u64 << i;
        let bits = match kind {
            Stepper::Knight => knight_from(b),
            Stepper::King => king_from(b),
            Stepper::WhitePawn => white_pawn_from(b),
            Stepper::BlackPawn => black_pawn_from(b),
        };
        table[i] = Bitboard(bits);
        i += 1;
    }
    table
}

/// Precomputed knight attacks indexed by square.
static KNIGHT_ATTACKS: [Bitboard; 64] = build_steppers(Stepper::Knight);
/// Precomputed king attacks indexed by square.
static KING_ATTACKS: [Bitboard; 64] = build_steppers(Stepper::King);
/// Precomputed pawn attacks indexed by `[color][square]`.
static PAWN_ATTACKS: [[Bitboard; 64]; 2] = [
    build_steppers(Stepper::WhitePawn),
    build_steppers(Stepper::BlackPawn),
];

// ---------------------------------------------------------------------------
// Const ray-line masks for sliders (diagonals and anti-diagonals).
// ---------------------------------------------------------------------------

/// Builds the diagonal (a1–h8 direction) mask through every square.
///
/// The diagonal is the set of squares with the same `rank - file`.
#[cfg(any(not(feature = "magic"), test))]
const fn build_diag() -> [Bitboard; 64] {
    let mut table = [Bitboard::EMPTY; 64];
    let mut sq = 0i32;
    while sq < 64 {
        let file = sq % 8;
        let rank = sq / 8;
        let mut bits = 0u64;
        let mut other = 0i32;
        while other < 64 {
            if (other % 8) - (other / 8) == file - rank {
                bits |= 1u64 << other;
            }
            other += 1;
        }
        table[sq as usize] = Bitboard(bits);
        sq += 1;
    }
    table
}

/// Builds the anti-diagonal (a8–h1 direction) mask through every square.
///
/// The anti-diagonal is the set of squares with the same `rank + file`.
#[cfg(any(not(feature = "magic"), test))]
const fn build_anti_diag() -> [Bitboard; 64] {
    let mut table = [Bitboard::EMPTY; 64];
    let mut sq = 0i32;
    while sq < 64 {
        let file = sq % 8;
        let rank = sq / 8;
        let mut bits = 0u64;
        let mut other = 0i32;
        while other < 64 {
            if (other % 8) + (other / 8) == file + rank {
                bits |= 1u64 << other;
            }
            other += 1;
        }
        table[sq as usize] = Bitboard(bits);
        sq += 1;
    }
    table
}

/// Diagonal masks (a1–h8 direction) indexed by square.
#[cfg(any(not(feature = "magic"), test))]
static DIAG: [Bitboard; 64] = build_diag();
/// Anti-diagonal masks (a8–h1 direction) indexed by square.
#[cfg(any(not(feature = "magic"), test))]
static ANTI_DIAG: [Bitboard; 64] = build_anti_diag();

// ---------------------------------------------------------------------------
// Geometry tables: `between` and `line`.
// ---------------------------------------------------------------------------

/// Returns the unit step `(df, dr)` from `a` toward `b` when they are aligned
/// on a rank, file, or diagonal; `None` otherwise (including when `a == b`).
const fn step_toward(a: i32, b: i32) -> Option<(i32, i32)> {
    let af = a % 8;
    let ar = a / 8;
    let bf = b % 8;
    let br = b / 8;
    let df = bf - af;
    let dr = br - ar;
    if df == 0 && dr == 0 {
        return None;
    }
    if df == 0 {
        return Some((0, if dr > 0 { 1 } else { -1 }));
    }
    if dr == 0 {
        return Some((if df > 0 { 1 } else { -1 }, 0));
    }
    if df == dr || df == -dr {
        return Some((if df > 0 { 1 } else { -1 }, if dr > 0 { 1 } else { -1 }));
    }
    None
}

/// Walks from `a` toward `b` collecting the squares strictly between them, or
/// returns `0` if the squares are not aligned.
const fn between_bits(a: i32, b: i32) -> u64 {
    let (df, dr) = match step_toward(a, b) {
        Some(step) => step,
        None => return 0,
    };
    let mut file = a % 8 + df;
    let mut rank = a / 8 + dr;
    let mut bits = 0u64;
    // Stop one square before `b`.
    while file != b % 8 || rank != b / 8 {
        bits |= 1u64 << (rank * 8 + file);
        file += df;
        rank += dr;
    }
    bits
}

/// Walks the full line through `a` and `b` to both board edges, or returns `0`
/// if the squares are not aligned.
const fn line_bits(a: i32, b: i32) -> u64 {
    let (df, dr) = match step_toward(a, b) {
        Some(step) => step,
        None => return 0,
    };
    let mut bits = 1u64 << a;
    // Walk forward to the edge.
    let mut file = a % 8 + df;
    let mut rank = a / 8 + dr;
    while file >= 0 && file < 8 && rank >= 0 && rank < 8 {
        bits |= 1u64 << (rank * 8 + file);
        file += df;
        rank += dr;
    }
    // Walk backward to the edge.
    let mut file = a % 8 - df;
    let mut rank = a / 8 - dr;
    while file >= 0 && file < 8 && rank >= 0 && rank < 8 {
        bits |= 1u64 << (rank * 8 + file);
        file -= df;
        rank -= dr;
    }
    bits
}

/// Builds the `[[Bitboard; 64]; 64]` `between` table.
const fn build_between() -> [[Bitboard; 64]; 64] {
    let mut table = [[Bitboard::EMPTY; 64]; 64];
    let mut a = 0i32;
    while a < 64 {
        let mut b = 0i32;
        while b < 64 {
            table[a as usize][b as usize] = Bitboard(between_bits(a, b));
            b += 1;
        }
        a += 1;
    }
    table
}

/// Builds the `[[Bitboard; 64]; 64]` `line` table.
const fn build_line() -> [[Bitboard; 64]; 64] {
    let mut table = [[Bitboard::EMPTY; 64]; 64];
    let mut a = 0i32;
    while a < 64 {
        let mut b = 0i32;
        while b < 64 {
            table[a as usize][b as usize] = Bitboard(line_bits(a, b));
            b += 1;
        }
        a += 1;
    }
    table
}

/// Squares strictly between two aligned squares, indexed by `[a][b]`.
static BETWEEN: [[Bitboard; 64]; 64] = build_between();
/// Full ray line through two aligned squares, indexed by `[a][b]`.
static LINE: [[Bitboard; 64]; 64] = build_line();

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Bitboard, Color, Square};

    /// Collects a bitboard from a list of squares for concise expectations.
    fn bb(squares: &[Square]) -> Bitboard {
        squares.iter().copied().collect()
    }

    #[test]
    fn pawn_attacks_center() {
        assert_eq!(
            pawn_attacks(Color::White, Square::E4),
            bb(&[Square::D5, Square::F5])
        );
        assert_eq!(
            pawn_attacks(Color::Black, Square::E4),
            bb(&[Square::D3, Square::F3])
        );
    }

    #[test]
    fn pawn_attacks_edges() {
        // a-file white pawn attacks only b-file.
        assert_eq!(pawn_attacks(Color::White, Square::A2), bb(&[Square::B3]));
        // h-file white pawn attacks only g-file.
        assert_eq!(pawn_attacks(Color::White, Square::H2), bb(&[Square::G3]));
        // a-file black pawn attacks only b-file.
        assert_eq!(pawn_attacks(Color::Black, Square::A7), bb(&[Square::B6]));
        assert_eq!(pawn_attacks(Color::Black, Square::H7), bb(&[Square::G6]));
    }

    #[test]
    fn pawn_attacks_last_rank_empty() {
        // A white pawn on rank 8 cannot move further; attacks are empty.
        assert_eq!(pawn_attacks(Color::White, Square::E8), Bitboard::EMPTY);
        // A black pawn on rank 1 likewise.
        assert_eq!(pawn_attacks(Color::Black, Square::E1), Bitboard::EMPTY);
    }

    #[test]
    fn knight_attacks_corner_edge_center() {
        // Corner a1: only two destinations.
        assert_eq!(knight_attacks(Square::A1), bb(&[Square::B3, Square::C2]));
        // Corner h8.
        assert_eq!(knight_attacks(Square::H8), bb(&[Square::F7, Square::G6]));
        // Edge a4: four destinations.
        assert_eq!(
            knight_attacks(Square::A4),
            bb(&[Square::B2, Square::C3, Square::C5, Square::B6])
        );
        // Center d4: all eight.
        assert_eq!(
            knight_attacks(Square::D4),
            bb(&[
                Square::C2,
                Square::E2,
                Square::B3,
                Square::F3,
                Square::B5,
                Square::F5,
                Square::C6,
                Square::E6,
            ])
        );
        assert_eq!(knight_attacks(Square::D4).count(), 8);
    }

    #[test]
    fn knight_never_wraps() {
        // b1 must not produce any a-file/h-file wrap artifacts.
        assert_eq!(
            knight_attacks(Square::B1),
            bb(&[Square::A3, Square::C3, Square::D2])
        );
        assert_eq!(
            knight_attacks(Square::G1),
            bb(&[Square::E2, Square::F3, Square::H3])
        );
    }

    #[test]
    fn king_attacks_corner_edge_center() {
        // Corner a1: three squares.
        assert_eq!(
            king_attacks(Square::A1),
            bb(&[Square::B1, Square::A2, Square::B2])
        );
        // Edge a4: five squares.
        assert_eq!(
            king_attacks(Square::A4),
            bb(&[Square::A3, Square::B3, Square::B4, Square::A5, Square::B5,])
        );
        // Center e4: all eight neighbours.
        assert_eq!(
            king_attacks(Square::E4),
            bb(&[
                Square::D3,
                Square::E3,
                Square::F3,
                Square::D4,
                Square::F4,
                Square::D5,
                Square::E5,
                Square::F5,
            ])
        );
        assert_eq!(king_attacks(Square::E4).count(), 8);
    }

    #[test]
    fn king_never_wraps() {
        // h-file king must not reach the a-file.
        assert_eq!(
            king_attacks(Square::H4),
            bb(&[Square::G3, Square::H3, Square::G4, Square::G5, Square::H5,])
        );
    }

    #[test]
    fn bishop_empty_board_is_two_diagonals() {
        // On an empty board the bishop reaches both full diagonals minus itself.
        let attacks = bishop_attacks(Square::D4, Bitboard::EMPTY);
        let expected = (DIAG[Square::D4.index() as usize] | ANTI_DIAG[Square::D4.index() as usize])
            .without(Square::D4);
        assert_eq!(attacks, expected);
        // Corner bishop sees a single long diagonal of seven squares.
        assert_eq!(bishop_attacks(Square::A1, Bitboard::EMPTY).count(), 7);
    }

    #[test]
    fn rook_empty_board_is_file_xor_rank() {
        let sq = Square::D4;
        let attacks = rook_attacks(sq, Bitboard::EMPTY);
        let file = super::file_mask(sq);
        let rank = super::rank_mask(sq);
        // Full file and rank through the square, minus the square itself.
        let expected = (file | rank).without(sq);
        assert_eq!(attacks, expected);
        assert_eq!(attacks.count(), 14);
    }

    #[test]
    fn rook_blockers_cut_rays() {
        // Rook on d4, blockers on d6 and f4.
        let occ = bb(&[Square::D6, Square::F4, Square::D4]);
        let attacks = rook_attacks(Square::D4, occ);
        // North: d5, d6 (blocker included), nothing beyond (no d7/d8).
        assert!(attacks.contains(Square::D5));
        assert!(attacks.contains(Square::D6));
        assert!(!attacks.contains(Square::D7));
        assert!(!attacks.contains(Square::D8));
        // East: e4, f4 (blocker included), nothing beyond.
        assert!(attacks.contains(Square::E4));
        assert!(attacks.contains(Square::F4));
        assert!(!attacks.contains(Square::G4));
        assert!(!attacks.contains(Square::H4));
        // South and west are unobstructed to the edge.
        assert!(attacks.contains(Square::D1));
        assert!(attacks.contains(Square::A4));
        // The rook never attacks its own square.
        assert!(!attacks.contains(Square::D4));
    }

    #[test]
    fn bishop_blockers_cut_rays() {
        // Bishop on c1, blocker on e3 along the a3..h6 anti? no, the c1-h6 diag.
        let occ = bb(&[Square::E3]);
        let attacks = bishop_attacks(Square::C1, occ);
        // North-east: d2, e3 (blocker), not f4/g5/h6 beyond.
        assert!(attacks.contains(Square::D2));
        assert!(attacks.contains(Square::E3));
        assert!(!attacks.contains(Square::F4));
        // North-west: b2, a3 to the edge (unobstructed).
        assert!(attacks.contains(Square::B2));
        assert!(attacks.contains(Square::A3));
    }

    #[test]
    fn queen_is_union_of_rook_and_bishop() {
        let occ = bb(&[Square::D6, Square::F4, Square::B2]);
        for index in 0..64u8 {
            let sq = Square::new(index);
            assert_eq!(
                queen_attacks(sq, occ),
                rook_attacks(sq, occ) | bishop_attacks(sq, occ)
            );
        }
    }

    #[test]
    fn slider_blocker_on_every_ray_is_included() {
        // Rook fully surrounded one step away: attacks exactly the four neighbours.
        let occ = bb(&[Square::D5, Square::D3, Square::C4, Square::E4]);
        assert_eq!(rook_attacks(Square::D4, occ), occ);
    }

    #[test]
    fn between_excludes_endpoints() {
        let mid = between(Square::C1, Square::C8);
        assert_eq!(
            mid,
            bb(&[
                Square::C2,
                Square::C3,
                Square::C4,
                Square::C5,
                Square::C6,
                Square::C7,
            ])
        );
        assert!(!mid.contains(Square::C1));
        assert!(!mid.contains(Square::C8));
    }

    #[test]
    fn between_is_symmetric_and_diagonal() {
        assert_eq!(
            between(Square::C1, Square::C8),
            between(Square::C8, Square::C1)
        );
        // Diagonal a1..h8: between excludes both ends.
        assert_eq!(
            between(Square::A1, Square::H8),
            bb(&[
                Square::B2,
                Square::C3,
                Square::D4,
                Square::E5,
                Square::F6,
                Square::G7,
            ])
        );
    }

    #[test]
    fn between_non_aligned_is_empty() {
        assert_eq!(between(Square::A1, Square::B3), Bitboard::EMPTY);
        assert_eq!(between(Square::D4, Square::E6), Bitboard::EMPTY);
        // Identical squares.
        assert_eq!(between(Square::D4, Square::D4), Bitboard::EMPTY);
        // Adjacent aligned squares: nothing strictly between.
        assert_eq!(between(Square::D4, Square::D5), Bitboard::EMPTY);
    }

    #[test]
    fn line_includes_endpoints_and_edges() {
        // File line is the whole file.
        assert_eq!(line(Square::C1, Square::C5), Bitboard::FILE_C);
        // Rank line is the whole rank.
        assert_eq!(line(Square::A4, Square::H4), Bitboard::RANK_4);
        // Diagonal through b2 reaches a1 and h8.
        let diag = line(Square::B2, Square::D4);
        assert!(diag.contains(Square::A1));
        assert!(diag.contains(Square::H8));
        assert!(diag.contains(Square::B2));
        assert!(diag.contains(Square::D4));
        assert_eq!(diag.count(), 8);
    }

    #[test]
    fn line_non_aligned_is_empty() {
        assert_eq!(line(Square::A1, Square::B3), Bitboard::EMPTY);
        assert_eq!(line(Square::D4, Square::D4), Bitboard::EMPTY);
    }

    #[test]
    fn between_is_a_subset_of_line() {
        for a in 0..64u8 {
            for b in 0..64u8 {
                let a = Square::new(a);
                let b = Square::new(b);
                let between = between(a, b);
                let line = line(a, b);
                // Every "between" square lies on the line.
                assert_eq!(between & line, between);
            }
        }
    }

    #[test]
    fn slider_consistency_with_step_scan() {
        // Cross-check the hyperbola-quintessence sliders against an independent
        // brute-force ray scan for many squares and occupancies.
        let occupancies = [
            Bitboard::EMPTY,
            Bitboard::FULL,
            bb(&[Square::D4, Square::E5, Square::C3]),
            bb(&[Square::A1, Square::H8, Square::A8, Square::H1, Square::D4]),
            Bitboard::EDGES,
        ];
        for index in 0..64u8 {
            let sq = Square::new(index);
            for &occ in &occupancies {
                assert_eq!(
                    rook_attacks(sq, occ),
                    scan_rays(sq, occ, &[(0, 1), (0, -1), (1, 0), (-1, 0)]),
                    "rook mismatch at {sq} occ {occ:?}"
                );
                assert_eq!(
                    bishop_attacks(sq, occ),
                    scan_rays(sq, occ, &[(1, 1), (1, -1), (-1, 1), (-1, -1)]),
                    "bishop mismatch at {sq} occ {occ:?}"
                );
            }
        }
    }

    /// Cross-check: under the `magic` feature the public sliders MUST return
    /// exactly the hyperbola-quintessence results for every square over many
    /// random occupancies. This is the correctness guarantee that makes the
    /// magic feature a transparent drop-in.
    #[cfg(feature = "magic")]
    #[test]
    fn magic_matches_hyperbola_every_square() {
        // Small deterministic xorshift so the test needs no external crate.
        let mut state = 0x1234_5678_9abc_def0u64;
        let mut rand = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };

        for index in 0..64u8 {
            let sq = Square::new(index);
            // Always check the empty and full boards explicitly.
            for occ in [Bitboard::EMPTY, Bitboard::FULL] {
                assert_eq!(
                    super::rook_attacks(sq, occ),
                    rook_attacks_hyperbola(sq, occ)
                );
                assert_eq!(
                    super::bishop_attacks(sq, occ),
                    bishop_attacks_hyperbola(sq, occ)
                );
            }
            // Then 256 random occupancies per square (16384 cases total).
            for _ in 0..256 {
                let occ = Bitboard(rand());
                assert_eq!(
                    super::rook_attacks(sq, occ),
                    rook_attacks_hyperbola(sq, occ),
                    "magic rook mismatch at {sq} occ {occ:?}"
                );
                assert_eq!(
                    super::bishop_attacks(sq, occ),
                    bishop_attacks_hyperbola(sq, occ),
                    "magic bishop mismatch at {sq} occ {occ:?}"
                );
            }
        }
    }

    /// Independent reference: scan each direction until off-board or a blocker
    /// (inclusive). Used only to validate the production sliders.
    fn scan_rays(sq: Square, occupied: Bitboard, dirs: &[(i8, i8)]) -> Bitboard {
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
}
