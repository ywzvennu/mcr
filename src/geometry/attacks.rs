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
//! * **Geometry rays** — [`between`] and [`line`](fn@line) give the strictly-between and
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
/// use mcr::geometry::{attacks::leaper_attacks, Chess8x8, Square};
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
/// use mcr::geometry::{attacks::pawn_attacks, Chess8x8, Square};
/// use mcr::Color;
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
/// use mcr::geometry::{attacks::knight_attacks, Chess8x8, Square};
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

/// The four full line masks (rank, file, diagonal, anti-diagonal) through a fixed
/// square, precomputed once so a hot loop that re-derives a slider's reach from
/// the *same* square against changing occupancy never rebuilds them.
///
/// The slider primitives ([`rook_attacks`] / [`bishop_attacks`] / [`queen_attacks`])
/// derive these four masks on every call — and [`diag_mask`] / [`anti_diag_mask`]
/// each run a `HEIGHT - 1`-round directional fill. The cannon king-safety verify
/// re-tests "is the king attacked" on a fresh post-move occupancy for every
/// sibling move of a node, but the king square is fixed across those siblings, so
/// its line masks are constant: computing them once with [`KingLineMasks::new`]
/// and reusing them via [`rook_attacks_masked`] / [`bishop_attacks_masked`] /
/// [`queen_attacks_masked`] removes the per-move mask rebuild. The reach computed
/// is bit-for-bit identical to the plain primitives (same `sliding` over the same
/// masks).
#[derive(Clone, Copy)]
pub(crate) struct KingLineMasks<G: Geometry> {
    /// The square the masks are taken through (the royal square).
    sq: Square<G>,
    /// The full file through `sq`.
    file: Bitboard<G>,
    /// The full rank through `sq`.
    rank: Bitboard<G>,
    /// The full NE/SW diagonal through `sq`.
    diag: Bitboard<G>,
    /// The full NW/SE anti-diagonal through `sq`.
    anti: Bitboard<G>,
}

impl<G: Geometry> KingLineMasks<G> {
    /// Precomputes the four line masks through `sq`.
    #[inline]
    pub(crate) fn new(sq: Square<G>) -> KingLineMasks<G> {
        KingLineMasks {
            sq,
            file: file_mask::<G>(sq),
            rank: rank_mask::<G>(sq),
            diag: diag_mask::<G>(sq),
            anti: anti_diag_mask::<G>(sq),
        }
    }

    /// The square the masks were built for.
    #[inline]
    pub(crate) fn square(self) -> Square<G> {
        self.sq
    }
}

/// Rook reach from the precomputed king square against `occupied` — identical to
/// `rook_attacks(masks.square(), occupied)` but reusing the cached rank/file
/// masks instead of re-deriving them.
#[must_use]
#[inline]
pub(crate) fn rook_attacks_masked<G: Geometry>(
    masks: KingLineMasks<G>,
    occupied: Bitboard<G>,
) -> Bitboard<G> {
    sliding(masks.sq, occupied, masks.file) | sliding(masks.sq, occupied, masks.rank)
}

/// Bishop reach from the precomputed king square against `occupied` — identical to
/// `bishop_attacks(masks.square(), occupied)` but reusing the cached diagonal
/// masks (whose fill is the costly part) instead of re-deriving them.
#[must_use]
#[inline]
pub(crate) fn bishop_attacks_masked<G: Geometry>(
    masks: KingLineMasks<G>,
    occupied: Bitboard<G>,
) -> Bitboard<G> {
    sliding(masks.sq, occupied, masks.diag) | sliding(masks.sq, occupied, masks.anti)
}

/// Queen reach from the precomputed king square against `occupied` — the union of
/// [`rook_attacks_masked`] and [`bishop_attacks_masked`].
#[must_use]
#[inline]
pub(crate) fn queen_attacks_masked<G: Geometry>(
    masks: KingLineMasks<G>,
    occupied: Bitboard<G>,
) -> Bitboard<G> {
    rook_attacks_masked(masks, occupied) | bishop_attacks_masked(masks, occupied)
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

/// Returns the squares a **Nightrider** on `sq` attacks given the `occupied` set:
/// it rides each of the eight knight directions, taking successive equal
/// knight-leaps outward until it steps off the board or meets a piece, including
/// the first occupant on each ray (a capture) but nothing beyond it.
///
/// A Nightrider is the **riding** generalisation of the knight (Betza `NN`): from
/// `sq` it makes one knight-leap `(±1,±2)` / `(±2,±1)` and may then continue in the
/// **same** direction by repeating that leap — `(1,2)`, then `(2,4)`, `(3,6)`, … —
/// so long as every intermediate landing square is empty. The ride along a
/// direction stops at (and includes) the first occupied square, exactly as a
/// slider stops at the first blocker on its ray; the caller masks out friendly
/// occupants. On an unobstructed board it reaches every square a whole number of
/// equal knight-steps from `sq`. Edges are respected — a leap off the board simply
/// ends that ray, and the offsets never wrap. The pattern is geometrically
/// symmetric (a Nightrider on `a` attacks `b` iff a Nightrider on `b` attacks `a`
/// under the same occupancy), so `attackers_to` reverse-projects it directly.
///
/// ```
/// use mcr::geometry::{attacks::nightrider_attacks, Bitboard, Chess8x8, Square};
/// // A Nightrider on a1 rides the (1,2) ray across an empty board: b3, c5, d7.
/// let bb = nightrider_attacks::<Chess8x8>(Square::new(0), Bitboard::EMPTY);
/// assert!(bb.contains(Square::new(17))); // b3
/// assert!(bb.contains(Square::new(34))); // c5
/// assert!(bb.contains(Square::new(51))); // d7
/// ```
#[must_use]
pub fn nightrider_attacks<G: Geometry>(sq: Square<G>, occupied: Bitboard<G>) -> Bitboard<G> {
    let mut bb = Bitboard::EMPTY;
    for &(df, dr) in &KNIGHT_OFFSETS {
        let mut cur = sq.offset(df, dr);
        while let Some(dest) = cur {
            bb.set(dest);
            // Stop at the first piece on the ray (included as a capture target);
            // an empty landing square lets the ride continue one more knight-leap.
            if occupied.contains(dest) {
                break;
            }
            cur = dest.offset(df, dr);
        }
    }
    bb
}

// ---------------------------------------------------------------------------
// Orthogonal ray masks + nearest-occupant bit-scan (cannon / king-line path).
// ---------------------------------------------------------------------------

/// The four orthogonal half-rays from a square, one per cannon direction.
///
/// Each field is the set of on-board squares strictly beyond `sq` along one
/// orthogonal ray (it never includes `sq` itself). The two **ascending** rays
/// (`east`, `north`) hold squares with a higher bit index than `sq`; their
/// nearest occupant to `sq` is the lowest set bit of `occ & ray`
/// (`trailing_zeros`). The two **descending** rays (`west`, `south`) hold lower
/// bit indices; their nearest occupant is the highest set bit (`leading_zeros`).
///
/// These per-direction masks let the cannon find each ray's screen and target
/// with one or two nearest-occupant bit-scans instead of stepping square by
/// square — the precomputed-ray-table fast path of issue #209. The masks are
/// pure geometry (no occupancy), so for any given square they are constant; they
/// are built from the existing file/rank line masks split at `sq`'s bit, which
/// the optimiser folds for the monomorphised geometry exactly as a stored
/// per-square table would.
struct OrthoRays<G: Geometry> {
    /// East half-ray (ascending bit index — same rank, higher files).
    east: Bitboard<G>,
    /// West half-ray (descending — same rank, lower files).
    west: Bitboard<G>,
    /// North half-ray (ascending — same file, higher ranks).
    north: Bitboard<G>,
    /// South half-ray (descending — same file, lower ranks).
    south: Bitboard<G>,
}

/// Builds the four orthogonal half-rays from `sq` (see [`OrthoRays`]).
#[inline]
fn ortho_rays<G: Geometry>(sq: Square<G>) -> OrthoRays<G> {
    let s = G::Bits::bit(sq.index() as u32);
    // Bits strictly above `sq`: clear `sq` and everything below it. `2s - 1` is
    // the mask of `sq` and all lower bits; its complement is the strictly-above
    // set. `2s` is formed as `s + s` to avoid an overflow panic at the top bit.
    let two_s = s.wrapping_add(s);
    let above = !two_s.wrapping_sub(G::Bits::ONE);
    // Bits strictly below `sq`: `s - 1`.
    let below = s.wrapping_sub(G::Bits::ONE);

    let file = file_mask::<G>(sq).0;
    let rank = rank_mask::<G>(sq).0;

    OrthoRays {
        north: Bitboard::<G>(file & above),
        south: Bitboard::<G>(file & below),
        east: Bitboard::<G>(rank & above),
        west: Bitboard::<G>(rank & below),
    }
}

/// Returns the nearest occupant to `sq` on an **ascending** half-ray: the lowest
/// set bit of `masked` (which must already be `occupied & ray`), or `None` if
/// the ray is clear. The nearest square on an ascending ray is the lowest-index
/// occupant, i.e. `trailing_zeros`.
#[inline]
fn nearest_up<G: Geometry>(masked: Bitboard<G>) -> Option<Square<G>> {
    if masked.0.is_zero() {
        None
    } else {
        Some(Square::new(masked.0.trailing_zeros() as u8))
    }
}

/// Returns the nearest occupant to `sq` on a **descending** half-ray: the
/// highest set bit of `masked` (`occupied & ray`), or `None` if clear. The
/// nearest square on a descending ray is the highest-index occupant, i.e.
/// `BITS - 1 - leading_zeros`.
#[inline]
fn nearest_down<G: Geometry>(masked: Bitboard<G>) -> Option<Square<G>> {
    if masked.0.is_zero() {
        None
    } else {
        Some(Square::new(
            (G::Bits::BITS - 1 - masked.0.leading_zeros()) as u8,
        ))
    }
}

/// On the ascending half-ray `ray` (from `sq`, exclusive), returns the cannon
/// **target**: the first occupant strictly beyond the first occupant (the
/// screen). `None` if there is no screen or nothing past it.
#[inline]
fn cannon_target_up<G: Geometry>(occupied: Bitboard<G>, ray: Bitboard<G>) -> Option<Square<G>> {
    let masked = occupied & ray;
    let screen = nearest_up(masked)?;
    // Squares beyond the screen on the same ascending ray: clear the screen and
    // everything below it, then the next occupant is the nearest above.
    let beyond = Bitboard::<G>(masked.0 & !screen_and_below::<G>(screen));
    nearest_up(beyond)
}

/// On the descending half-ray `ray`, returns the cannon **target** beyond the
/// first occupant (the screen). `None` if no screen or nothing past it.
#[inline]
fn cannon_target_down<G: Geometry>(occupied: Bitboard<G>, ray: Bitboard<G>) -> Option<Square<G>> {
    let masked = occupied & ray;
    let screen = nearest_down(masked)?;
    // Beyond the screen, descending: keep only bits strictly below the screen.
    let beyond = Bitboard::<G>(masked.0 & screen_below::<G>(screen));
    nearest_down(beyond)
}

/// The mask of `sq`'s bit and every lower bit (`2s - 1`).
#[inline]
fn screen_and_below<G: Geometry>(sq: Square<G>) -> G::Bits {
    let s = G::Bits::bit(sq.index() as u32);
    s.wrapping_add(s).wrapping_sub(G::Bits::ONE)
}

/// The mask of every bit strictly below `sq` (`s - 1`).
#[inline]
fn screen_below<G: Geometry>(sq: Square<G>) -> G::Bits {
    G::Bits::bit(sq.index() as u32).wrapping_sub(G::Bits::ONE)
}

/// One ascending-ray Janggi-cannon contribution: given the half-ray `ray` (from
/// `sq`, exclusive, ascending), the full `occupied` set, and the `cannons`
/// subset, returns the empty squares jumped past one non-cannon screen (the
/// quiet jumps) and the over-screen capture target if it exists and is not a
/// cannon. A ray whose first piece (the screen) is a cannon is dead and yields
/// nothing.
#[inline]
fn janggi_ray_up<G: Geometry>(
    occupied: Bitboard<G>,
    cannons: Bitboard<G>,
    ray: Bitboard<G>,
) -> (Bitboard<G>, Option<Square<G>>) {
    let masked = occupied & ray;
    let Some(screen) = nearest_up(masked) else {
        return (Bitboard::EMPTY, None);
    };
    // The screen may not itself be a cannon: such a ray is dead.
    if cannons.contains(screen) {
        return (Bitboard::EMPTY, None);
    }
    // The segment of the ray strictly beyond the screen (ascending): clear the
    // screen and everything at or below it.
    let beyond_mask = ray.0 & !screen_and_below::<G>(screen);
    let beyond_occ = Bitboard::<G>(masked.0 & beyond_mask);
    match nearest_up(beyond_occ) {
        // A target piece past the screen: the quiet squares are those strictly
        // between the screen and the target; the capture lands on the target
        // unless it is a cannon.
        Some(target) => {
            let quiet = Bitboard::<G>(beyond_mask & screen_below::<G>(target));
            let cap = if cannons.contains(target) {
                None
            } else {
                Some(target)
            };
            (quiet, cap)
        }
        // Nothing past the screen: every square beyond it is a quiet jump.
        None => (Bitboard::<G>(beyond_mask), None),
    }
}

/// The descending-ray analogue of [`janggi_ray_up`].
#[inline]
fn janggi_ray_down<G: Geometry>(
    occupied: Bitboard<G>,
    cannons: Bitboard<G>,
    ray: Bitboard<G>,
) -> (Bitboard<G>, Option<Square<G>>) {
    let masked = occupied & ray;
    let Some(screen) = nearest_down(masked) else {
        return (Bitboard::EMPTY, None);
    };
    if cannons.contains(screen) {
        return (Bitboard::EMPTY, None);
    }
    // The segment strictly beyond the screen (descending): keep only bits below
    // the screen.
    let beyond_mask = ray.0 & screen_below::<G>(screen);
    let beyond_occ = Bitboard::<G>(masked.0 & beyond_mask);
    match nearest_down(beyond_occ) {
        Some(target) => {
            // Quiet squares strictly between screen and target: above the target.
            let quiet = Bitboard::<G>(beyond_mask & !screen_and_below::<G>(target));
            let cap = if cannons.contains(target) {
                None
            } else {
                Some(target)
            };
            (quiet, cap)
        }
        None => (Bitboard::<G>(beyond_mask), None),
    }
}

// ---------------------------------------------------------------------------
// Cannon primitive (Xiangqi / Janggi / Shako).
// ---------------------------------------------------------------------------

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
/// use mcr::geometry::{attacks::cannon_quiet_moves, Chess8x8, Bitboard, Square};
/// // On an empty 8x8 board a cannon on a1 quietly slides the whole rank and file
/// // (14 squares), like a rook.
/// let q = cannon_quiet_moves::<Chess8x8>(Square::new(0), Bitboard::EMPTY);
/// assert_eq!(q.count(), 14);
/// ```
#[must_use]
#[inline]
pub fn cannon_quiet_moves<G: Geometry>(sq: Square<G>, occupied: Bitboard<G>) -> Bitboard<G> {
    // A cannon's quiet rays are exactly a rook's slides minus the first blocker
    // it would land on: `rook_attacks` stops at and includes that blocker, so
    // dropping the occupied squares leaves only the reachable empty squares. This
    // is the hyperbola-quintessence slider — a handful of bit ops per ray, no
    // square-by-square walk.
    rook_attacks(sq, occupied) & !occupied
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
/// use mcr::geometry::{attacks::cannon_capture_targets, Chess8x8, Bitboard, Square};
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
    // Each of the four orthogonal rays contributes at most one target — the first
    // occupant strictly beyond the first occupant (the screen). Found by two
    // nearest-occupant bit-scans on the masked ray rather than walking it.
    let rays = ortho_rays::<G>(sq);
    let mut bb = Bitboard::EMPTY;
    if let Some(t) = cannon_target_up(occupied, rays.north) {
        bb.set(t);
    }
    if let Some(t) = cannon_target_up(occupied, rays.east) {
        bb.set(t);
    }
    if let Some(t) = cannon_target_down(occupied, rays.south) {
        bb.set(t);
    }
    if let Some(t) = cannon_target_down(occupied, rays.west) {
        bb.set(t);
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
/// use mcr::geometry::{attacks::janggi_cannon_quiet, Chess8x8, Bitboard, Square};
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
    let rays = ortho_rays::<G>(sq);
    let mut bb = Bitboard::EMPTY;
    bb |= janggi_ray_up(occupied, cannons, rays.north).0;
    bb |= janggi_ray_up(occupied, cannons, rays.east).0;
    bb |= janggi_ray_down(occupied, cannons, rays.south).0;
    bb |= janggi_ray_down(occupied, cannons, rays.west).0;
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
/// use mcr::geometry::{attacks::janggi_cannon_capture, Chess8x8, Bitboard, Square};
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
    let rays = ortho_rays::<G>(sq);
    let mut bb = Bitboard::EMPTY;
    if let Some(t) = janggi_ray_up(occupied, cannons, rays.north).1 {
        bb.set(t);
    }
    if let Some(t) = janggi_ray_up(occupied, cannons, rays.east).1 {
        bb.set(t);
    }
    if let Some(t) = janggi_ray_down(occupied, cannons, rays.south).1 {
        bb.set(t);
    }
    if let Some(t) = janggi_ray_down(occupied, cannons, rays.west).1 {
        bb.set(t);
    }
    bb
}

// ---------------------------------------------------------------------------
// Diagonal cannon primitives (Cannon Shogi bishop-cannon / bishop-hopper).
// ---------------------------------------------------------------------------

/// The four diagonal half-rays from `sq`, the diagonal analogue of [`OrthoRays`].
/// Built by splitting the full diagonal / anti-diagonal line masks at `sq`'s bit,
/// exactly as [`ortho_rays`] splits the rank / file masks.
struct DiagRays<G: Geometry> {
    /// North-east half-ray (ascending bit index — `rank+file` rises along the
    /// NE/SW diagonal, higher ranks **and** files).
    ne: Bitboard<G>,
    /// South-west half-ray (descending — lower ranks and files).
    sw: Bitboard<G>,
    /// North-west half-ray (ascending — higher ranks, lower files, along the
    /// NW/SE anti-diagonal).
    nw: Bitboard<G>,
    /// South-east half-ray (descending — lower ranks, higher files).
    se: Bitboard<G>,
}

/// Builds the four diagonal half-rays from `sq` (see [`DiagRays`]).
#[inline]
fn diag_rays<G: Geometry>(sq: Square<G>) -> DiagRays<G> {
    let s = G::Bits::bit(sq.index() as u32);
    let two_s = s.wrapping_add(s);
    let above = !two_s.wrapping_sub(G::Bits::ONE);
    let below = s.wrapping_sub(G::Bits::ONE);

    let diag = diag_mask::<G>(sq).0;
    let anti = anti_diag_mask::<G>(sq).0;

    DiagRays {
        ne: Bitboard::<G>(diag & above),
        sw: Bitboard::<G>(diag & below),
        nw: Bitboard::<G>(anti & above),
        se: Bitboard::<G>(anti & below),
    }
}

/// Returns the squares a **bishop-cannon** on `sq` may capture on — the diagonal
/// analogue of [`cannon_capture_targets`]: along each of the four diagonals, the
/// first piece beyond **exactly one** intervening piece (the screen). Pure
/// geometry (occupancy-aware); the caller masks out friendly occupants.
///
/// ```
/// use mcr::geometry::{attacks::diag_cannon_capture_targets, Chess8x8, Bitboard, Square};
/// // Screen on c3, enemy on e5: a bishop-cannon on a1 captures over the screen onto e5.
/// let occ = Bitboard::<Chess8x8>::EMPTY
///     .with(Square::new(18)) // c3 screen
///     .with(Square::new(36)); // e5 target
/// let caps = diag_cannon_capture_targets::<Chess8x8>(Square::new(0), occ);
/// assert_eq!(caps, Bitboard::EMPTY.with(Square::new(36)));
/// ```
#[must_use]
#[inline]
pub fn diag_cannon_capture_targets<G: Geometry>(
    sq: Square<G>,
    occupied: Bitboard<G>,
) -> Bitboard<G> {
    let rays = diag_rays::<G>(sq);
    let mut bb = Bitboard::EMPTY;
    if let Some(t) = cannon_target_up(occupied, rays.ne) {
        bb.set(t);
    }
    if let Some(t) = cannon_target_up(occupied, rays.nw) {
        bb.set(t);
    }
    if let Some(t) = cannon_target_down(occupied, rays.sw) {
        bb.set(t);
    }
    if let Some(t) = cannon_target_down(occupied, rays.se) {
        bb.set(t);
    }
    bb
}

/// Returns the **empty** squares a **bishop-hopper** on `sq` may move to — the
/// diagonal analogue of [`janggi_cannon_quiet`] (with no cannon restriction): on
/// each diagonal, every empty square strictly **beyond the first screen**, up to
/// (but not including) the next piece. Pairs with [`diag_cannon_capture_targets`]
/// for the bishop-hopper's full move-and-capture set; both move and capture
/// require jumping exactly one screen.
#[must_use]
#[inline]
pub fn diag_cannon_quiet_jumps<G: Geometry>(sq: Square<G>, occupied: Bitboard<G>) -> Bitboard<G> {
    let rays = diag_rays::<G>(sq);
    let mut bb = Bitboard::EMPTY;
    bb |= janggi_ray_up(occupied, Bitboard::EMPTY, rays.ne).0;
    bb |= janggi_ray_up(occupied, Bitboard::EMPTY, rays.nw).0;
    bb |= janggi_ray_down(occupied, Bitboard::EMPTY, rays.sw).0;
    bb |= janggi_ray_down(occupied, Bitboard::EMPTY, rays.se).0;
    bb
}

// ---------------------------------------------------------------------------
// Grasshopper primitive (queen-line hopper).
// ---------------------------------------------------------------------------

/// One ascending-ray grasshopper landing square: the square **immediately
/// beyond** the first occupant (the "hurdle") on the ascending half-ray `ray`.
/// `None` if the ray has no hurdle, or the hurdle sits on the ray's last square
/// (nothing lies beyond it).
///
/// Unlike the cannon ([`cannon_target_up`]) — which slides *past* the screen to
/// the next occupied square — the grasshopper stops on the single geometric cell
/// right after the hurdle: the nearest (lowest) *ray* square strictly above it,
/// occupied or empty.
#[inline]
fn grasshopper_target_up<G: Geometry>(
    occupied: Bitboard<G>,
    ray: Bitboard<G>,
) -> Option<Square<G>> {
    let hurdle = nearest_up(occupied & ray)?;
    // The ray's own squares strictly beyond the hurdle; the nearest (lowest) is
    // the cell immediately past it along the ray.
    let beyond = Bitboard::<G>(ray.0 & !screen_and_below::<G>(hurdle));
    nearest_up(beyond)
}

/// The descending-ray analogue of [`grasshopper_target_up`]: the square
/// immediately beyond (below) the first occupant on the descending half-ray.
#[inline]
fn grasshopper_target_down<G: Geometry>(
    occupied: Bitboard<G>,
    ray: Bitboard<G>,
) -> Option<Square<G>> {
    let hurdle = nearest_down(occupied & ray)?;
    let beyond = Bitboard::<G>(ray.0 & screen_below::<G>(hurdle));
    nearest_down(beyond)
}

/// Returns the squares a **Grasshopper** (Betza `gQ`) on `sq` may move to or
/// threaten under `occupied`: along each of the eight queen rays, the single
/// square **immediately beyond the first piece** (the "hurdle") it meets.
///
/// The grasshopper is a hopper: on each ray it skips the empty run out to the
/// first occupied square (of either colour), then its only reachable cell is the
/// one square directly past that hurdle. That landing square is returned whether
/// it is empty (a quiet hop), holds an enemy (a capture), **or** holds a friendly
/// piece — the caller masks out friendly occupants exactly as it does for the
/// slider and cannon primitives. A ray with no hurdle, or whose hurdle sits on
/// the board edge (nothing beyond), contributes nothing. Unlike a leaper this set
/// is *occupancy-aware* (the hurdle is read from `occupied`) and geometrically
/// asymmetric, so it is both the grasshopper's move set and — since it lands on
/// the square past the hurdle — the set of squares from which it gives check.
///
/// ```
/// use mcr::geometry::{attacks::grasshopper_attacks, Chess8x8, Bitboard, Square};
/// // A hurdle on a4: a grasshopper on a1 hops to a5, the square just beyond it,
/// // and — with no other hurdle on any ray — nowhere else.
/// let occ = Bitboard::<Chess8x8>::EMPTY.with(Square::new(24)); // a4
/// let att = grasshopper_attacks::<Chess8x8>(Square::new(0), occ);
/// assert!(att.contains(Square::new(32))); // a5
/// assert_eq!(att.count(), 1);
/// ```
#[must_use]
#[inline]
pub fn grasshopper_attacks<G: Geometry>(sq: Square<G>, occupied: Bitboard<G>) -> Bitboard<G> {
    let o = ortho_rays::<G>(sq);
    let d = diag_rays::<G>(sq);
    let mut bb = Bitboard::EMPTY;
    if let Some(t) = grasshopper_target_up(occupied, o.north) {
        bb.set(t);
    }
    if let Some(t) = grasshopper_target_up(occupied, o.east) {
        bb.set(t);
    }
    if let Some(t) = grasshopper_target_down(occupied, o.south) {
        bb.set(t);
    }
    if let Some(t) = grasshopper_target_down(occupied, o.west) {
        bb.set(t);
    }
    if let Some(t) = grasshopper_target_up(occupied, d.ne) {
        bb.set(t);
    }
    if let Some(t) = grasshopper_target_up(occupied, d.nw) {
        bb.set(t);
    }
    if let Some(t) = grasshopper_target_down(occupied, d.sw) {
        bb.set(t);
    }
    if let Some(t) = grasshopper_target_down(occupied, d.se) {
        bb.set(t);
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
/// use mcr::geometry::{attacks::horse_attacks, Bitboard, Square, Xiangqi9x10};
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
/// use mcr::geometry::{attacks::elephant_attacks_blockable, Bitboard, Square, Xiangqi9x10};
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

/// The eight Xiang-Fu **Mahout** leaps paired with the single square each passes
/// over. Each entry is `(target_df, target_dr, leg_df, leg_dr)`: a two-square
/// target — the four 2-diagonal Alfil leaps `(±2,±2)` and the four 2-orthogonal
/// Dabbaba leaps `(±2,0)` / `(0,±2)` — whose blocking "leg" is the single square
/// halfway to it `(±1,±1)` / `(±1,0)` / `(0,±1)`. The leg is the geometric
/// midpoint, so the relation is symmetric (a Mahout reaches a square iff a Mahout
/// on that square reaches back, the leg being the same square either way).
const MAHOUT_LEGS: [(i8, i8, i8, i8); 8] = [
    (2, 2, 1, 1),
    (2, -2, 1, -1),
    (-2, 2, -1, 1),
    (-2, -2, -1, -1),
    (2, 0, 1, 0),
    (-2, 0, -1, 0),
    (0, 2, 0, 1),
    (0, -2, 0, -1),
];

/// Returns the squares a Xiang-Fu **Mahout** on `sq` attacks given the `occupied`
/// set: the eight two-square targets (4 diagonal Alfil + 4 orthogonal Dabbaba)
/// minus any whose **leg** (the single square it passes over) is occupied.
///
/// A Mahout leaps exactly two squares in any of the eight directions but
/// **cannot jump** — the leap is blocked if the one square between `sq` and the
/// target is occupied. It moves and captures alike (the attack set equals the
/// move set). Geometry-only and occupancy-aware; the leg is the symmetric
/// midpoint, so reverse-projecting the pattern from a target square recovers the
/// same attackers (no leg asymmetry, unlike the Xiangqi horse).
///
/// ```
/// use mcr::geometry::{attacks::mahout_attacks_blockable, Bitboard, Square, Shogi9x9};
/// // A central Mahout on an empty board reaches all eight two-step squares.
/// let sq = Square::<Shogi9x9>::from_file_rank(4, 4).unwrap();
/// assert_eq!(mahout_attacks_blockable::<Shogi9x9>(sq, Bitboard::EMPTY).count(), 8);
/// ```
#[must_use]
pub fn mahout_attacks_blockable<G: Geometry>(sq: Square<G>, occupied: Bitboard<G>) -> Bitboard<G> {
    let mut bb = Bitboard::EMPTY;
    for &(tf, tr, lf, lr) in &MAHOUT_LEGS {
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
/// use mcr::geometry::{attacks::janggi_elephant_attacks, Bitboard, Square, Xiangqi9x10};
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
// King attack lines (slider / cannon / flying-general king-safety geometry).
// ---------------------------------------------------------------------------

/// Returns every square on the king's **rank, file, and two diagonals**,
/// excluding the king's own square — the union of the four full lines along
/// which a slider, a cannon (over a screen), or the Xiangqi flying general can
/// attack the king.
///
/// This is the masked-table form of the king-line geometry: it ORs the four line
/// masks the slider primitives already derive by arithmetic (rank, file,
/// diagonal, anti-diagonal) rather than walking eight rays a square at a time, so
/// the cannon / multi-royal verify path pays a handful of bit ops per node for
/// its fast-accept filter. Bit-for-bit identical to the per-step ray walk it
/// replaces (the slider line masks are validated square-by-square against
/// independent ray scans on every supported geometry).
#[must_use]
#[inline]
pub fn king_attack_lines<G: Geometry>(king: Square<G>) -> Bitboard<G> {
    let lines = file_mask::<G>(king)
        | rank_mask::<G>(king)
        | diag_mask::<G>(king)
        | anti_diag_mask::<G>(king);
    lines.without(king)
}

/// Like [`king_attack_lines`] but with the king's two diagonals **truncated to
/// `diag_radius` squares per direction** — the rank and file stay full length.
///
/// This is the king-line geometry for a cannon-royal variant that has **no
/// long-range diagonal king attacker** (Janggi, Xiangqi): no piece slides to the
/// king along a board diagonal, so the only diagonal squares whose occupancy can
/// change the king's safety are the few near ones — a hobbled leaper's leg (the
/// Horse's diagonal-neighbour leg; the Janggi Elephant's two legs, both within two
/// diagonal steps) and a palace screen (the cannon's palace-diagonal jump screen,
/// or a palace-diagonal chariot blocker, one step away). Capping the diagonals at
/// `diag_radius` therefore drops only squares that provably cannot bear on the
/// king, so a move touching one of them is safely fast-accepted rather than
/// verified. The rank and file are kept full because a chariot or cannon attacks
/// the king along the whole of them.
///
/// A `diag_radius` at least the board's diagonal length reproduces
/// [`king_attack_lines`] exactly; callers pass the smallest radius that still
/// covers every near-diagonal threat (`2` for Janggi/Xiangqi).
#[must_use]
#[inline]
pub fn king_attack_lines_diag_capped<G: Geometry>(king: Square<G>, diag_radius: u8) -> Bitboard<G> {
    let mut lines = file_mask::<G>(king) | rank_mask::<G>(king);
    lines = lines.without(king);
    for &(df, dr) in &[(1, 1), (1, -1), (-1, 1), (-1, -1)] {
        let mut cur = king.offset(df, dr);
        let mut steps = 0u8;
        while steps < diag_radius {
            let Some(sq) = cur else { break };
            lines.set(sq);
            cur = sq.offset(df, dr);
            steps += 1;
        }
    }
    lines
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
/// use mcr::geometry::{attacks::between, Chess8x8, Square};
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
/// use mcr::geometry::{attacks::line, Chess8x8, Square};
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

    /// Independent reference for the Xiangqi/Shako cannon **quiet** moves: walk
    /// each ray, collecting empties up to (not including) the first piece. This is
    /// the original square-by-square definition the bit-scan path replaced.
    fn scan_cannon_quiet<G: Geometry>(sq: Square<G>, occ: Bitboard<G>) -> Bitboard<G> {
        let mut bb = Bitboard::EMPTY;
        for &(df, dr) in &[(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let mut cur = sq.offset(df, dr);
            while let Some(next) = cur {
                if occ.contains(next) {
                    break;
                }
                bb.set(next);
                cur = next.offset(df, dr);
            }
        }
        bb
    }

    /// Independent reference for the Xiangqi/Shako cannon **capture** targets:
    /// over exactly one screen, the first piece beyond it.
    fn scan_cannon_caps<G: Geometry>(sq: Square<G>, occ: Bitboard<G>) -> Bitboard<G> {
        let mut bb = Bitboard::EMPTY;
        for &(df, dr) in &[(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let mut cur = sq.offset(df, dr);
            let screen = loop {
                match cur {
                    None => break None,
                    Some(n) if occ.contains(n) => break Some(n),
                    Some(n) => cur = n.offset(df, dr),
                }
            };
            let Some(screen) = screen else { continue };
            let mut cur = screen.offset(df, dr);
            while let Some(next) = cur {
                if occ.contains(next) {
                    bb.set(next);
                    break;
                }
                cur = next.offset(df, dr);
            }
        }
        bb
    }

    /// Independent reference for the Janggi cannon **quiet** jumps: empties beyond
    /// one non-cannon screen, up to the next piece.
    fn scan_janggi_quiet<G: Geometry>(
        sq: Square<G>,
        occ: Bitboard<G>,
        cannons: Bitboard<G>,
    ) -> Bitboard<G> {
        let mut bb = Bitboard::EMPTY;
        for &(df, dr) in &[(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let mut cur = sq.offset(df, dr);
            let screen = loop {
                match cur {
                    None => break None,
                    Some(n) if occ.contains(n) => break Some(n),
                    Some(n) => cur = n.offset(df, dr),
                }
            };
            let Some(screen) = screen else { continue };
            if cannons.contains(screen) {
                continue;
            }
            let mut cur = screen.offset(df, dr);
            while let Some(next) = cur {
                if occ.contains(next) {
                    break;
                }
                bb.set(next);
                cur = next.offset(df, dr);
            }
        }
        bb
    }

    /// Independent reference for the Janggi cannon **capture** targets: first
    /// non-cannon piece beyond one non-cannon screen.
    fn scan_janggi_caps<G: Geometry>(
        sq: Square<G>,
        occ: Bitboard<G>,
        cannons: Bitboard<G>,
    ) -> Bitboard<G> {
        let mut bb = Bitboard::EMPTY;
        for &(df, dr) in &[(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let mut cur = sq.offset(df, dr);
            let screen = loop {
                match cur {
                    None => break None,
                    Some(n) if occ.contains(n) => break Some(n),
                    Some(n) => cur = n.offset(df, dr),
                }
            };
            let Some(screen) = screen else { continue };
            if cannons.contains(screen) {
                continue;
            }
            let mut cur = screen.offset(df, dr);
            while let Some(next) = cur {
                if occ.contains(next) {
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

    /// Sweep every square × a basket of occupancies (and, for Janggi, cannon
    /// subsets) on a geometry `G`, asserting the bit-scan cannon primitives equal
    /// the independent square-by-square references — the byte-identity guarantee
    /// behind the issue-#209 rewrite.
    fn cannon_bitscan_matches_walk<G>(squares: u8)
    where
        G: Geometry,
        G::Bits: core::fmt::Debug,
    {
        let mut next = rng();
        let board = Bitboard::<G>::FULL;
        // Build a backing value bit-for-bit from a u128 seed, generic over the
        // backing width; off-board high bits are then masked away.
        let cast = |x: u128| -> Bitboard<G> {
            let mut v = G::Bits::ZERO;
            let mut bit = 0u32;
            while bit < G::Bits::BITS && bit < 128 {
                if (x >> bit) & 1 == 1 {
                    v = v | G::Bits::bit(bit);
                }
                bit += 1;
            }
            Bitboard::<G>(v) & board
        };
        let mut occs: Vec<Bitboard<G>> = Vec::new();
        occs.push(Bitboard::EMPTY);
        occs.push(board);
        for _ in 0..48 {
            let lo = next() as u128;
            let hi = (next() as u128) << 64;
            occs.push(cast(lo | hi));
        }
        for index in 0..squares {
            let sq = Square::<G>::new(index);
            for &occ in &occs {
                assert_eq!(
                    cannon_quiet_moves::<G>(sq, occ),
                    scan_cannon_quiet(sq, occ),
                    "cannon quiet sq {index}"
                );
                assert_eq!(
                    cannon_capture_targets::<G>(sq, occ),
                    scan_cannon_caps(sq, occ),
                    "cannon caps sq {index}"
                );
                // Janggi: pair the occupancy with a couple of cannon subsets.
                let subsets = [Bitboard::EMPTY, occ, cast(next() as u128) & occ];
                for cannons in subsets {
                    assert_eq!(
                        janggi_cannon_quiet::<G>(sq, occ, cannons),
                        scan_janggi_quiet(sq, occ, cannons),
                        "janggi quiet sq {index}"
                    );
                    assert_eq!(
                        janggi_cannon_capture::<G>(sq, occ, cannons),
                        scan_janggi_caps(sq, occ, cannons),
                        "janggi caps sq {index}"
                    );
                }
            }
        }
    }

    /// Independent reference for the king attack lines: walk all eight directions
    /// to the board edge, the original per-step definition.
    fn scan_king_lines<G: Geometry>(king: Square<G>) -> Bitboard<G> {
        let mut bb = Bitboard::EMPTY;
        for &(df, dr) in &[
            (1, 0),
            (0, 1),
            (1, 1),
            (1, -1),
            (-1, 0),
            (0, -1),
            (-1, -1),
            (-1, 1),
        ] {
            let mut cur = king.offset(df, dr);
            while let Some(next) = cur {
                bb.set(next);
                cur = next.offset(df, dr);
            }
        }
        bb
    }

    fn king_lines_match_walk<G>(squares: u8)
    where
        G: Geometry,
        G::Bits: core::fmt::Debug,
    {
        for index in 0..squares {
            let sq = Square::<G>::new(index);
            assert_eq!(
                king_attack_lines::<G>(sq),
                scan_king_lines(sq),
                "king lines sq {index}"
            );
        }
    }

    #[test]
    fn king_lines_match_walk_all_geometries() {
        use crate::geometry::{Grand10x10, Minishogi5x5, Minixiangqi7x7, Shogi9x9, Xiangqi9x10};
        king_lines_match_walk::<Chess8x8>(64);
        king_lines_match_walk::<Cap10x8>(80);
        king_lines_match_walk::<Grand10x10>(100);
        king_lines_match_walk::<Xiangqi9x10>(90);
        king_lines_match_walk::<Shogi9x9>(81);
        king_lines_match_walk::<Minixiangqi7x7>(49);
        king_lines_match_walk::<Minishogi5x5>(25);
    }

    #[test]
    fn cannon_bitscan_matches_walk_chess8x8() {
        cannon_bitscan_matches_walk::<Chess8x8>(64);
    }

    #[test]
    fn cannon_bitscan_matches_walk_cap10x8() {
        cannon_bitscan_matches_walk::<Cap10x8>(80);
    }

    #[test]
    fn cannon_bitscan_matches_walk_grand10x10() {
        use crate::geometry::Grand10x10;
        cannon_bitscan_matches_walk::<Grand10x10>(100);
    }

    #[test]
    fn cannon_bitscan_matches_walk_xiangqi9x10() {
        use crate::geometry::Xiangqi9x10;
        cannon_bitscan_matches_walk::<Xiangqi9x10>(90);
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

    // ----- cargo-mutants triage (issue #370): survivor-killing coverage --------
    //
    // NOTES on equivalent mutants (documented rather than "killed"): several
    // slider/king-line primitives are the union of provably **disjoint** bit sets —
    // a rook's file ray and rank ray share only the origin square (excluded); a
    // bishop's two diagonals likewise; a queen is the union of the (disjoint) rook
    // and bishop reach; and `king_attack_lines` ORs four lines that pairwise meet
    // only at the king (removed by `.without(king)`). For disjoint operands
    // `a | b == a ^ b`, so a `|`->`^` mutation on any of these unions is a genuine
    // no-op (equivalent) mutant that no test can distinguish. The `|`->`&`
    // mutation, by contrast, collapses a disjoint union to the empty set, which the
    // equivalence sweeps below do catch.

    /// Builds a backing value bit-for-bit from a `u128` seed, generic over the
    /// backing width, masked to the on-board squares. Shared by the sweeps below.
    fn seed_occ<G: Geometry>(x: u128) -> Bitboard<G> {
        let mut v = G::Bits::ZERO;
        let mut bit = 0u32;
        while bit < G::Bits::BITS && bit < 128 {
            if (x >> bit) & 1 == 1 {
                v = v | G::Bits::bit(bit);
            }
            bit += 1;
        }
        Bitboard::<G>(v) & Bitboard::<G>::FULL
    }

    /// A basket of occupancies for a geometry: empty, full, and randomised.
    fn occ_basket<G: Geometry>(count: usize) -> Vec<Bitboard<G>> {
        let mut next = rng();
        let mut occs = Vec::new();
        occs.push(Bitboard::EMPTY);
        occs.push(Bitboard::<G>::FULL);
        for _ in 0..count {
            let lo = next() as u128;
            let hi = (next() as u128) << 64;
            occs.push(seed_occ::<G>(lo | hi));
        }
        occs
    }

    #[test]
    fn masked_sliders_match_plain_primitives() {
        // The KingLineMasks reuse variants (`*_attacks_masked`) only cache the four
        // line masks; they must be bit-identical to the plain sliders that re-derive
        // them each call. Sweep every square × a basket of occupancies on an 8x8
        // (u64) and a 10x8 (u128) geometry, and check the cached square round-trips.
        fn check<G>(squares: u8)
        where
            G: Geometry,
            G::Bits: core::fmt::Debug,
        {
            let occs = occ_basket::<G>(24);
            for index in 0..squares {
                let sq = Square::<G>::new(index);
                let masks = KingLineMasks::new(sq);
                assert_eq!(masks.square(), sq, "square round-trip {index}");
                for &occ in &occs {
                    assert_eq!(
                        rook_attacks_masked::<G>(masks, occ),
                        rook_attacks::<G>(sq, occ),
                        "rook masked {index}"
                    );
                    assert_eq!(
                        bishop_attacks_masked::<G>(masks, occ),
                        bishop_attacks::<G>(sq, occ),
                        "bishop masked {index}"
                    );
                    assert_eq!(
                        queen_attacks_masked::<G>(masks, occ),
                        queen_attacks::<G>(sq, occ),
                        "queen masked {index}"
                    );
                }
            }
        }
        check::<Chess8x8>(64);
        check::<Cap10x8>(80);
    }

    /// Independent reference for a Shogi lance: the blocker-aware forward file ray
    /// (north for white, south for black), stopping at and including the first
    /// occupant.
    fn scan_lance<G: Geometry>(color: Color, sq: Square<G>, occ: Bitboard<G>) -> Bitboard<G> {
        let step: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = Bitboard::EMPTY;
        let mut cur = sq.offset(0, step);
        while let Some(next) = cur {
            bb.set(next);
            if occ.contains(next) {
                break;
            }
            cur = next.offset(0, step);
        }
        bb
    }

    #[test]
    fn lance_matches_forward_file_ray() {
        // Both colours, every square, a basket of occupancies: the lance equals the
        // independent forward-ray scan. Pins the forward direction (white north /
        // black south) and the `file_ray & forward` masking.
        fn check<G>(squares: u8)
        where
            G: Geometry,
            G::Bits: core::fmt::Debug,
        {
            let occs = occ_basket::<G>(24);
            for index in 0..squares {
                let sq = Square::<G>::new(index);
                for &occ in &occs {
                    for color in [Color::White, Color::Black] {
                        assert_eq!(
                            lance_attacks::<G>(color, sq, occ),
                            scan_lance::<G>(color, sq, occ),
                            "lance {color:?} sq {index}"
                        );
                    }
                }
            }
        }
        check::<Chess8x8>(64);
        check::<Cap10x8>(80);
        // Spot-check the direction split concretely: a white lance on a1 slides the
        // whole a-file north; a black lance on a8 slides it south.
        let white = lance_attacks::<Chess8x8>(Color::White, Square::new(0), Bitboard::EMPTY);
        assert_eq!(
            white,
            rook_attacks::<Chess8x8>(Square::new(0), Bitboard::EMPTY)
                & file_mask(Square::<Chess8x8>::new(0))
        );
        assert!(white.contains(Square::new(56)) && !white.contains(Square::new(1)));
        let black = lance_attacks::<Chess8x8>(Color::Black, Square::new(56), Bitboard::EMPTY);
        assert!(black.contains(Square::new(0)) && !black.contains(Square::new(57)));
    }

    /// Independent reference for the diagonal cannon / bishop-hopper: along each of
    /// the four diagonals, the empty squares beyond the first screen (quiet jumps)
    /// and the first piece past it (capture target).
    fn scan_diag_cannon<G: Geometry>(
        sq: Square<G>,
        occ: Bitboard<G>,
    ) -> (Bitboard<G>, Bitboard<G>) {
        let mut quiet = Bitboard::EMPTY;
        let mut caps = Bitboard::EMPTY;
        for &(df, dr) in &[(1, 1), (-1, -1), (-1, 1), (1, -1)] {
            let mut cur = sq.offset(df, dr);
            let screen = loop {
                match cur {
                    None => break None,
                    Some(n) if occ.contains(n) => break Some(n),
                    Some(n) => cur = n.offset(df, dr),
                }
            };
            let Some(screen) = screen else { continue };
            let mut cur = screen.offset(df, dr);
            while let Some(next) = cur {
                if occ.contains(next) {
                    caps.set(next);
                    break;
                }
                quiet.set(next);
                cur = next.offset(df, dr);
            }
        }
        (quiet, caps)
    }

    #[test]
    fn diag_cannon_matches_walk() {
        // Sweep the diagonal-cannon quiet jumps and capture targets against the
        // independent diagonal walk, on an 8x8 (u64) and 10x8 (u128) geometry. Pins
        // the `diag_rays` half-ray split and the per-ray unions.
        fn check<G>(squares: u8)
        where
            G: Geometry,
            G::Bits: core::fmt::Debug,
        {
            let occs = occ_basket::<G>(24);
            for index in 0..squares {
                let sq = Square::<G>::new(index);
                for &occ in &occs {
                    let (quiet, caps) = scan_diag_cannon::<G>(sq, occ);
                    assert_eq!(
                        diag_cannon_quiet_jumps::<G>(sq, occ),
                        quiet,
                        "diag cannon quiet sq {index}"
                    );
                    assert_eq!(
                        diag_cannon_capture_targets::<G>(sq, occ),
                        caps,
                        "diag cannon caps sq {index}"
                    );
                }
            }
        }
        check::<Chess8x8>(64);
        check::<Cap10x8>(80);
    }

    // ----- Grasshopper primitive (queen-line hopper) -------------------------

    /// Independent reference for the grasshopper: along each of the eight queen
    /// directions, walk to the first occupant (the hurdle), then the single square
    /// immediately beyond it is a target if it is on the board — occupied or not.
    fn scan_grasshopper<G: Geometry>(sq: Square<G>, occ: Bitboard<G>) -> Bitboard<G> {
        let mut bb = Bitboard::EMPTY;
        for &(df, dr) in &[
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (-1, -1),
            (-1, 1),
            (1, -1),
        ] {
            let mut cur = sq.offset(df, dr);
            let hurdle = loop {
                match cur {
                    None => break None,
                    Some(n) if occ.contains(n) => break Some(n),
                    Some(n) => cur = n.offset(df, dr),
                }
            };
            let Some(hurdle) = hurdle else { continue };
            if let Some(land) = hurdle.offset(df, dr) {
                bb.set(land);
            }
        }
        bb
    }

    #[test]
    fn grasshopper_matches_walk() {
        // Sweep the grasshopper landing set against the independent eight-direction
        // walk, on an 8x8 (u64), 10x8 (u128), and 10x10 (u128) geometry. Pins the
        // `ortho_rays` / `diag_rays` half-ray split and the one-beyond-hurdle scan.
        use crate::geometry::Grand10x10;
        fn check<G>(squares: u8)
        where
            G: Geometry,
            G::Bits: core::fmt::Debug,
        {
            let occs = occ_basket::<G>(24);
            for index in 0..squares {
                let sq = Square::<G>::new(index);
                for &occ in &occs {
                    assert_eq!(
                        grasshopper_attacks::<G>(sq, occ),
                        scan_grasshopper::<G>(sq, occ),
                        "grasshopper sq {index}"
                    );
                }
            }
        }
        check::<Chess8x8>(64);
        check::<Cap10x8>(80);
        check::<Grand10x10>(100);
    }

    #[test]
    fn grasshopper_lands_immediately_beyond_a_hurdle() {
        // a1 grasshopper, hurdle on a4 (idx 24): it hops to a5 (idx 32), the single
        // square beyond the hurdle — never a2/a3 (short of the hurdle) nor a6+.
        let occ = Bitboard::<Chess8x8>::EMPTY.with(Square::new(24));
        let att = grasshopper_attacks::<Chess8x8>(Square::new(0), occ);
        assert_eq!(att, Bitboard::EMPTY.with(Square::new(32)));

        // A hurdle two-plus squares out still yields exactly the one cell beyond it:
        // hurdle on a5 (idx 32) => landing a6 (idx 40).
        let occ = Bitboard::<Chess8x8>::EMPTY.with(Square::new(32));
        let att = grasshopper_attacks::<Chess8x8>(Square::new(0), occ);
        assert_eq!(att, Bitboard::EMPTY.with(Square::new(40)));
    }

    #[test]
    fn grasshopper_needs_a_hurdle_no_empty_ray_move() {
        // On an empty board a grasshopper has no hurdle on any ray, so it can move
        // nowhere at all.
        for index in 0..64u8 {
            assert_eq!(
                grasshopper_attacks::<Chess8x8>(Square::new(index), Bitboard::EMPTY),
                Bitboard::EMPTY,
                "empty-board grasshopper sq {index} must be immobile"
            );
        }
    }

    #[test]
    fn grasshopper_landing_includes_occupant_beyond_the_hurdle() {
        // The landing square is returned whether empty or occupied (the caller masks
        // friendly / splits capture): hurdle a4 (idx 24), a piece on the landing a5
        // (idx 32) => a5 is still in the set (a capture target there).
        let occ = Bitboard::<Chess8x8>::EMPTY
            .with(Square::new(24))
            .with(Square::new(32));
        let att = grasshopper_attacks::<Chess8x8>(Square::new(0), occ);
        assert!(att.contains(Square::new(32)));
    }

    #[test]
    fn grasshopper_does_not_wrap_or_leak_off_board() {
        // On the 10x10 u128 geometry a grasshopper's landings stay on the board for
        // every square and a basket of occupancies (no bit escapes the 100-square
        // region, and no ray wraps across a file edge).
        use crate::geometry::Grand10x10;
        let off = !Grand10x10::BOARD_MASK;
        let occs = occ_basket::<Grand10x10>(24);
        for index in 0..100u8 {
            let sq = Square::<Grand10x10>::new(index);
            for &occ in &occs {
                assert_eq!(grasshopper_attacks::<Grand10x10>(sq, occ).0 & off, 0);
            }
        }
    }

    /// Independent reference for the Xiangqi elephant: four two-square diagonal
    /// leaps, each blocked by its midpoint "eye". The eye is derived geometrically
    /// as half the target offset — independent of the library's `ELEPHANT_EYES`.
    fn scan_elephant<G: Geometry>(sq: Square<G>, occ: Bitboard<G>) -> Bitboard<G> {
        let mut bb = Bitboard::EMPTY;
        for &(dx, dy) in &[(2, 2), (2, -2), (-2, 2), (-2, -2)] {
            let Some(eye) = sq.offset(dx / 2, dy / 2) else {
                continue;
            };
            if occ.contains(eye) {
                continue;
            }
            if let Some(dest) = sq.offset(dx, dy) {
                bb.set(dest);
            }
        }
        bb
    }

    /// Independent reference for the Xiang-Fu Mahout: eight two-square leaps (four
    /// Alfil diagonals + four Dabbaba orthogonals), each blocked by its midpoint.
    fn scan_mahout<G: Geometry>(sq: Square<G>, occ: Bitboard<G>) -> Bitboard<G> {
        let mut bb = Bitboard::EMPTY;
        for &(dx, dy) in &[
            (2, 2),
            (2, -2),
            (-2, 2),
            (-2, -2),
            (2, 0),
            (-2, 0),
            (0, 2),
            (0, -2),
        ] {
            let Some(leg) = sq.offset(dx / 2, dy / 2) else {
                continue;
            };
            if occ.contains(leg) {
                continue;
            }
            if let Some(dest) = sq.offset(dx, dy) {
                bb.set(dest);
            }
        }
        bb
    }

    /// Independent reference for the Janggi elephant: one orthogonal step then two
    /// diagonal steps outward, blocked at each intervening square. Leg 1 is the
    /// orthogonal step along the longer axis; leg 2 is one diagonal step past it.
    fn scan_janggi_elephant<G: Geometry>(sq: Square<G>, occ: Bitboard<G>) -> Bitboard<G> {
        let mut bb = Bitboard::EMPTY;
        let dirs: [(i8, i8); 8] = [
            (2, 3),
            (-2, 3),
            (2, -3),
            (-2, -3),
            (3, 2),
            (3, -2),
            (-3, 2),
            (-3, -2),
        ];
        for &(dx, dy) in &dirs {
            let sx = if dx > 0 { 1 } else { -1 };
            let sy = if dy > 0 { 1 } else { -1 };
            let (l1x, l1y) = if dy.abs() > dx.abs() {
                (0, sy)
            } else {
                (sx, 0)
            };
            let Some(leg1) = sq.offset(l1x, l1y) else {
                continue;
            };
            if occ.contains(leg1) {
                continue;
            }
            let Some(leg2) = sq.offset(l1x + sx, l1y + sy) else {
                continue;
            };
            if occ.contains(leg2) {
                continue;
            }
            if let Some(dest) = sq.offset(dx, dy) {
                bb.set(dest);
            }
        }
        bb
    }

    #[test]
    fn blockable_leapers_match_reference() {
        // Sweep the Xiangqi elephant, the Mahout, and the Janggi elephant against
        // independent references that derive each leap's blocking square(s) from the
        // target offset — so a sign flip in the offset tables (ELEPHANT_EYES /
        // MAHOUT_LEGS / JANGGI_ELEPHANT_LEGS) diverges on some square.
        use crate::geometry::{Shogi9x9, Xiangqi9x10};
        let occs = occ_basket::<Xiangqi9x10>(24);
        for index in 0..90u8 {
            let sq = Square::<Xiangqi9x10>::new(index);
            for &occ in &occs {
                assert_eq!(
                    elephant_attacks_blockable::<Xiangqi9x10>(sq, occ),
                    scan_elephant::<Xiangqi9x10>(sq, occ),
                    "elephant sq {index}"
                );
                assert_eq!(
                    janggi_elephant_attacks::<Xiangqi9x10>(sq, occ),
                    scan_janggi_elephant::<Xiangqi9x10>(sq, occ),
                    "janggi elephant sq {index}"
                );
            }
        }
        let occs = occ_basket::<Shogi9x9>(24);
        for index in 0..81u8 {
            let sq = Square::<Shogi9x9>::new(index);
            for &occ in &occs {
                assert_eq!(
                    mahout_attacks_blockable::<Shogi9x9>(sq, occ),
                    scan_mahout::<Shogi9x9>(sq, occ),
                    "mahout sq {index}"
                );
            }
        }
    }

    /// Independent reference for the diagonal-capped king lines: the full rank and
    /// file rays (walked to the edge), plus the four diagonals truncated to
    /// `radius` squares per direction.
    fn scan_king_capped<G: Geometry>(king: Square<G>, radius: u8) -> Bitboard<G> {
        let mut bb = Bitboard::EMPTY;
        for &(df, dr) in &[(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let mut cur = king.offset(df, dr);
            while let Some(n) = cur {
                bb.set(n);
                cur = n.offset(df, dr);
            }
        }
        for &(df, dr) in &[(1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let mut cur = king.offset(df, dr);
            let mut steps = 0u8;
            while steps < radius {
                let Some(n) = cur else { break };
                bb.set(n);
                cur = n.offset(df, dr);
                steps += 1;
            }
        }
        bb
    }

    #[test]
    fn king_lines_diag_capped_matches_reference() {
        // Several radii, every square, on an 8x8 and a 10x8 geometry: the capped
        // king lines equal the independent rank/file walk plus radius-limited
        // diagonals. A radius covering the whole board reproduces the uncapped
        // `king_attack_lines`.
        fn check<G>(squares: u8)
        where
            G: Geometry,
            G::Bits: core::fmt::Debug,
        {
            for &radius in &[0u8, 1, 2, 3, 20] {
                for index in 0..squares {
                    let sq = Square::<G>::new(index);
                    assert_eq!(
                        king_attack_lines_diag_capped::<G>(sq, radius),
                        scan_king_capped::<G>(sq, radius),
                        "capped king radius {radius} sq {index}"
                    );
                }
            }
            for index in 0..squares {
                let sq = Square::<G>::new(index);
                assert_eq!(
                    king_attack_lines_diag_capped::<G>(sq, 20),
                    king_attack_lines::<G>(sq),
                    "capped==uncapped sq {index}"
                );
            }
        }
        check::<Chess8x8>(64);
        check::<Cap10x8>(80);
    }
}
