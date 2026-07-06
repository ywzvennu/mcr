//! Zobrist hashing for the **wide** (fairy-variant) layer: a stable 64-bit key
//! identifying a [`GenericPosition`](super::position::GenericPosition) for
//! repetition detection, opening books, and position deduplication.
//!
//! The key is the XOR of one pseudo-random 64-bit constant per independent feature
//! of the position. Because XOR is its own inverse, a feature can be toggled in or
//! out in O(1), which is what lets [`GenericGame`](super::game::GenericGame)
//! maintain the key **incrementally** across moves rather than rescanning the whole
//! board every ply (issue #311, replacing the per-ply FNV recompute of #245).
//!
//! The hashed features cover **exactly** the components the old
//! [`repetition_key`](super::position::GenericPosition::repetition_key) folded, so
//! two positions collide to the same key iff they are "the same position" for the
//! repetition rules:
//!
//! - one constant per *colored piece on a square* (every [`WideRole`] of each color
//!   on each board square),
//! - the side to move (folded in on black's turn),
//! - one constant per castling / gating right (keyed by the rook's start file),
//! - the en-passant target square (the full square, matching the FNV key — not just
//!   the file),
//! - the Seirawan gating state (eligible squares and per-color Hawk / Elephant
//!   reserves),
//! - the hand / placement pocket (one constant per `(color, role, count)`),
//! - the Duck square, the Alice plane-B membership, and the crazyhouse promoted
//!   mask (each keyed per square).
//!
//! The move clocks and the Janggi consecutive-pass counter are deliberately
//! **not** hashed: they differ on most plies and must not break a repetition, again
//! matching the FNV key.
//!
//! # Determinism
//!
//! The constants come from an in-crate [splitmix64] generator seeded with a fixed
//! constant, so the tables — and therefore every key — are identical across builds
//! and process runs, with no clock, RNG, or external dependency. (The same proven
//! approach the concrete [`crate::zobrist`] table uses.)
//!
//! [splitmix64]: https://en.wikipedia.org/wiki/Xorshift#Initialization

use super::role::WideRole;
use super::GateRole;
use crate::Color;

/// The number of colors.
const COLORS: usize = 2;
/// The number of [`WideRole`]s — the piece-key table's role dimension.
const ROLES: usize = WideRole::COUNT;
/// The largest square count of any wide geometry: the 256-square, U256-backed
/// [`Tenjiku16x16`](super::Tenjiku16x16) board, so every square index is
/// `< MAX_SQUARES`.
const MAX_SQUARES: usize = 256;
/// The historical square-table width — the largest square count of every
/// `u128`-backed board (128 squares). Square-indexed tables draw their first
/// `BASE_SQUARES` keys in the original stream order and defer the high squares to
/// **later, tiered** extension passes, so every board with at most `BASE_SQUARES`
/// squares keeps **byte-identical** keys.
const BASE_SQUARES: usize = 128;
/// The Chu-tier square width: the 144-square [`Chu12x12`](super::Chu12x12) board.
/// The first extension pass draws squares `[BASE_SQUARES..CHU_SQUARES)` exactly as
/// before Dai widened `MAX_SQUARES`, so the 144-square Chu board keeps its high-
/// square keys **byte-identical**; the newer squares `[CHU_SQUARES..DAI_SQUARES)`
/// (reachable only by the 225-square Dai board) are drawn in a second, later tier.
const CHU_SQUARES: usize = 144;
/// The Dai-tier square width: the 225-square [`Dai15x15`](super::Dai15x15) board.
/// The second extension pass draws squares `[CHU_SQUARES..DAI_SQUARES)` exactly as
/// before Tenjiku widened `MAX_SQUARES`, so the 225-square Dai board keeps its
/// high-square keys **byte-identical**; the newest squares
/// `[DAI_SQUARES..MAX_SQUARES)` (reachable only by the 256-square Tenjiku board)
/// are drawn in a third, later tier.
const DAI_SQUARES: usize = 225;
/// The castling table's file dimension: an upper bound on any board width (the
/// widest wide board is ten files), so every rook start file is `< MAX_FILES`.
const MAX_FILES: usize = 16;
/// The hand table's count dimension. No wide variant banks more than this many
/// copies of a single role in hand (Shogi tops out near eighteen pawns); a count at
/// or beyond the bound saturates to the last slot, which only ever costs a
/// (astronomically unlikely) collision between two otherwise-equal positions whose
/// hands differ only past this many of one piece.
const MAX_HAND: usize = 32;
/// The number of Seirawan reserve pieces per color (Hawk, Elephant).
const GATES: usize = 2;

/// The fixed seed for the constant generator. Changing it reshuffles every key (and
/// would break the pinned startpos-hash tests), so it must stay constant. Distinct
/// from the concrete [`crate::zobrist`] seed so the two layers' tables are
/// independent.
const SEED: u64 = 0x2545_F491_4F6C_DD1D;

/// One step of the [splitmix64] PRNG: advances `state` and returns a well-mixed
/// 64-bit output. A tiny, public-domain-style mixing function used only to fill the
/// constant tables deterministically; it is not a cryptographic RNG.
///
/// [splitmix64]: https://en.wikipedia.org/wiki/Xorshift#Initialization
#[inline]
const fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The full set of wide-layer Zobrist constants, generated deterministically from
/// [`SEED`].
struct Keys {
    /// One key per `[color][role][square]`.
    pieces: [[[u64; MAX_SQUARES]; ROLES]; COLORS],
    /// Folded in when it is black's turn to move.
    black_to_move: u64,
    /// One key per `[color][side][rook-file]` castling / gating right (side `0` is
    /// kingside, `1` queenside).
    castling: [[[u64; MAX_FILES]; 2]; COLORS],
    /// One key per en-passant target square.
    ep: [u64; MAX_SQUARES],
    /// One key per Seirawan gating-eligible square.
    gating_eligible: [u64; MAX_SQUARES],
    /// One key per `[color][gate]` Seirawan reserve still in hand.
    gating_reserve: [[u64; GATES]; COLORS],
    /// One key per `[color][role][count]` of pieces in hand / pocket.
    hand: [[[u64; MAX_HAND]; ROLES]; COLORS],
    /// One key per Duck square.
    duck: [u64; MAX_SQUARES],
    /// One key per Alice plane-B square.
    alice: [u64; MAX_SQUARES],
    /// One key per crazyhouse promoted-mask square.
    promoted: [u64; MAX_SQUARES],
    /// One key per petrified-chess wall square.
    petrified: [u64; MAX_SQUARES],
}

/// Fills a `[u64; N]` table with successive generator outputs.
macro_rules! fill {
    ($state:expr, $table:expr) => {{
        let mut i = 0;
        while i < $table.len() {
            $table[i] = splitmix64($state);
            i += 1;
        }
    }};
}

/// Fills the half-open index range `[$lo, $hi)` of a table with successive
/// generator outputs. Used to draw a square-indexed table's historical
/// `[0, BASE_SQUARES)` region in the original stream order, then its high-square
/// `[BASE_SQUARES, MAX_SQUARES)` extension in a separate later pass.
macro_rules! fill_range {
    ($state:expr, $table:expr, $lo:expr, $hi:expr) => {{
        let mut i = $lo;
        while i < $hi {
            $table[i] = splitmix64($state);
            i += 1;
        }
    }};
}

impl Keys {
    /// Generates the constants by drawing successive [`splitmix64`] outputs in a
    /// fixed order. A `const fn`, so the tables are computed at compile time and are
    /// identical on every run.
    const fn generate() -> Keys {
        let mut state = SEED;

        let mut pieces = [[[0u64; MAX_SQUARES]; ROLES]; COLORS];
        let mut c = 0;
        while c < COLORS {
            let mut r = 0;
            while r < ROLES {
                fill_range!(&mut state, pieces[c][r], 0, BASE_SQUARES);
                r += 1;
            }
            c += 1;
        }

        let black_to_move = splitmix64(&mut state);

        let mut castling = [[[0u64; MAX_FILES]; 2]; COLORS];
        let mut c = 0;
        while c < COLORS {
            let mut s = 0;
            while s < 2 {
                fill!(&mut state, castling[c][s]);
                s += 1;
            }
            c += 1;
        }

        let mut ep = [0u64; MAX_SQUARES];
        fill_range!(&mut state, ep, 0, BASE_SQUARES);

        let mut gating_eligible = [0u64; MAX_SQUARES];
        fill_range!(&mut state, gating_eligible, 0, BASE_SQUARES);

        let mut gating_reserve = [[0u64; GATES]; COLORS];
        let mut c = 0;
        while c < COLORS {
            fill!(&mut state, gating_reserve[c]);
            c += 1;
        }

        let mut hand = [[[0u64; MAX_HAND]; ROLES]; COLORS];
        let mut c = 0;
        while c < COLORS {
            let mut r = 0;
            while r < ROLES {
                fill!(&mut state, hand[c][r]);
                r += 1;
            }
            c += 1;
        }

        let mut duck = [0u64; MAX_SQUARES];
        fill_range!(&mut state, duck, 0, BASE_SQUARES);

        let mut alice = [0u64; MAX_SQUARES];
        fill_range!(&mut state, alice, 0, BASE_SQUARES);

        let mut promoted = [0u64; MAX_SQUARES];
        fill_range!(&mut state, promoted, 0, BASE_SQUARES);

        // Extension tier 1: the Chu high squares `[BASE_SQUARES, CHU_SQUARES)`,
        // reachable by the 144-square Chu board. Drawn (in this exact order) right
        // after every historical draw above, so all boards with at most
        // `BASE_SQUARES` squares — and the 144-square Chu board's own high-square
        // keys — stay byte-for-byte identical to before Dai widened `MAX_SQUARES`.
        let mut c = 0;
        while c < COLORS {
            let mut r = 0;
            while r < ROLES {
                fill_range!(&mut state, pieces[c][r], BASE_SQUARES, CHU_SQUARES);
                r += 1;
            }
            c += 1;
        }
        fill_range!(&mut state, ep, BASE_SQUARES, CHU_SQUARES);
        fill_range!(&mut state, gating_eligible, BASE_SQUARES, CHU_SQUARES);
        fill_range!(&mut state, duck, BASE_SQUARES, CHU_SQUARES);
        fill_range!(&mut state, alice, BASE_SQUARES, CHU_SQUARES);
        fill_range!(&mut state, promoted, BASE_SQUARES, CHU_SQUARES);

        // Extension tier 2: the Dai high squares `[CHU_SQUARES, DAI_SQUARES)`,
        // reachable only by the 225-square Dai board. Drawn after tier 1 and before
        // tier 3, so every board with at most `DAI_SQUARES` squares (including Chu
        // and Dai) is untouched by the Tenjiku widening below.
        let mut c = 0;
        while c < COLORS {
            let mut r = 0;
            while r < ROLES {
                fill_range!(&mut state, pieces[c][r], CHU_SQUARES, DAI_SQUARES);
                r += 1;
            }
            c += 1;
        }
        fill_range!(&mut state, ep, CHU_SQUARES, DAI_SQUARES);
        fill_range!(&mut state, gating_eligible, CHU_SQUARES, DAI_SQUARES);
        fill_range!(&mut state, duck, CHU_SQUARES, DAI_SQUARES);
        fill_range!(&mut state, alice, CHU_SQUARES, DAI_SQUARES);
        fill_range!(&mut state, promoted, CHU_SQUARES, DAI_SQUARES);

        // Extension tier 3: the Tenjiku high squares `[DAI_SQUARES, MAX_SQUARES)`,
        // reachable only by the 256-square Tenjiku board. Drawn last of all, so
        // every smaller board (including Chu and Dai) keeps byte-identical keys.
        let mut c = 0;
        while c < COLORS {
            let mut r = 0;
            while r < ROLES {
                fill_range!(&mut state, pieces[c][r], DAI_SQUARES, MAX_SQUARES);
                r += 1;
            }
            c += 1;
        }
        fill_range!(&mut state, ep, DAI_SQUARES, MAX_SQUARES);
        fill_range!(&mut state, gating_eligible, DAI_SQUARES, MAX_SQUARES);
        fill_range!(&mut state, duck, DAI_SQUARES, MAX_SQUARES);
        fill_range!(&mut state, alice, DAI_SQUARES, MAX_SQUARES);
        fill_range!(&mut state, promoted, DAI_SQUARES, MAX_SQUARES);

        // Petrified-chess wall keys. Drawn dead last, after every historical and
        // extension-tier draw above, so every other table — and thus every other
        // variant's Zobrist keys — stays byte-for-byte identical. Petrified chess is
        // an 8x8 variant, but the whole square range is filled for uniformity.
        let mut petrified = [0u64; MAX_SQUARES];
        fill!(&mut state, petrified);

        Keys {
            pieces,
            black_to_move,
            castling,
            ep,
            gating_eligible,
            gating_reserve,
            hand,
            duck,
            alice,
            promoted,
            petrified,
        }
    }
}

/// The wide-layer Zobrist constant tables, computed once at compile time.
static KEYS: Keys = Keys::generate();

/// Index of a color into the constant tables.
#[inline]
const fn color_index(color: Color) -> usize {
    match color {
        Color::White => 0,
        Color::Black => 1,
    }
}

/// Index of a Seirawan reserve piece into the reserve table.
#[inline]
const fn gate_index(gate: GateRole) -> usize {
    match gate {
        GateRole::Hawk => 0,
        GateRole::Elephant => 1,
    }
}

/// The piece-square key for a colored [`WideRole`] on a square.
#[inline]
pub(crate) fn piece_key(color: Color, role: WideRole, square: u8) -> u64 {
    KEYS.pieces[color_index(color)][role.index()][square as usize]
}

/// The side-to-move key, folded in only when it is black's turn.
#[inline]
pub(crate) fn side_key(turn: Color) -> u64 {
    match turn {
        Color::White => 0,
        Color::Black => KEYS.black_to_move,
    }
}

/// The key for one castling / gating right, identified by the rook's start `file`.
/// `side` is `0` for kingside, `1` for queenside (the [`GenericCastling`] order).
///
/// [`GenericCastling`]: super::position::GenericCastling
#[inline]
pub(crate) fn castling_key(color: Color, side: usize, file: u8) -> u64 {
    KEYS.castling[color_index(color)][side][file as usize]
}

/// The key for the en-passant target square.
#[inline]
pub(crate) fn ep_key(square: u8) -> u64 {
    KEYS.ep[square as usize]
}

/// The key for a Seirawan gating-eligible square.
#[inline]
pub(crate) fn gating_eligible_key(square: u8) -> u64 {
    KEYS.gating_eligible[square as usize]
}

/// The key for a Seirawan reserve piece still in `color`'s hand.
#[inline]
pub(crate) fn gating_reserve_key(color: Color, gate: GateRole) -> u64 {
    KEYS.gating_reserve[color_index(color)][gate_index(gate)]
}

/// The key for holding `count` (`>= 1`) copies of `role` in `color`'s hand /
/// pocket. A count at or beyond [`MAX_HAND`] saturates to the last slot.
#[inline]
pub(crate) fn hand_key(color: Color, role: WideRole, count: u8) -> u64 {
    let idx = (count as usize).min(MAX_HAND - 1);
    KEYS.hand[color_index(color)][role.index()][idx]
}

/// The key for the Duck occupying a square.
#[inline]
pub(crate) fn duck_key(square: u8) -> u64 {
    KEYS.duck[square as usize]
}

/// The key for an occupant on the Alice plane B at a square.
#[inline]
pub(crate) fn alice_key(square: u8) -> u64 {
    KEYS.alice[square as usize]
}

/// The key for a crazyhouse promoted-mask square.
#[inline]
pub(crate) fn promoted_key(square: u8) -> u64 {
    KEYS.promoted[square as usize]
}

/// The key for a petrified-chess wall square.
#[inline]
pub(crate) fn petrified_key(square: u8) -> u64 {
    KEYS.petrified[square as usize]
}
