//! Spartan chess (8x8) on the generic engine — the first variant exercising
//! **asymmetric armies**, **multiple royal kings + duple check**, and the
//! **Berolina Hoplite** pawn on the [`WideVariant`] layer
//! (`docs/fairy-variants-architecture.md` §4.4). Validated against
//! Fairy-Stockfish `UCI_Variant spartan`.
//!
//! Spartan chess pits a standard (Persian) White army against an asymmetric
//! Black (Spartan) army with its own pieces, two kings, and Berolina pawns.
//!
//! ## Armies
//!
//! * **White = Persians.** The standard six: pawn, knight, bishop, rook, queen,
//!   king, with standard castling (the only side that castles). Every White
//!   movement, the standard pawn, and promotion to `N/B/R/Q` are the trait
//!   defaults.
//! * **Black = Spartans.** A distinct piece set, each a new [`WideRole`] (or, for
//!   the Warlord, the existing [`WideRole::Hawk`]):
//!   * **Lieutenant** ([`WideRole::Lieutenant`]) — a leaper to the six squares one
//!     file away (one step sideways or diagonally) plus a two-square diagonal
//!     jump. It has **no** straight forward/backward step.
//!   * **General** ([`WideRole::General`]) — Rook + Ferz: orthogonal slides plus a
//!     single diagonal step.
//!   * **Captain** ([`WideRole::Captain`]) — Wazir + Dabbaba: a single orthogonal
//!     step plus a two-square orthogonal jump.
//!   * **Warlord** ([`WideRole::Hawk`]) — Bishop + Knight (an Archbishop),
//!     identical to Seirawan's Hawk / Capablanca's Archbishop.
//!   * **King** ([`WideRole::King`]) — a standard king; Black starts with **two**.
//!   * **Hoplite** ([`WideRole::Hoplite`]) — a Berolina pawn: moves one square
//!     **diagonally** forward (two from its start rank), captures one square
//!     **straight** forward. No en passant.
//!
//! ## Two kings + duple check
//!
//! Black starts with two kings. A side with several kings is **in check only when
//! every king is attacked at once** — "duple check" for two kings. Otherwise it is
//! free to move, even to leave a king en prise: it loses that king and continues
//! with the survivor. The legality rule is therefore exactly "after the move, at
//! least one of my kings is unattacked," which the generic engine tests per move
//! on the multi-royal path ([`WideVariant::multi_royal`]). This unifies both
//! colours — White (one king) reduces to ordinary "not in check," Black (two
//! kings) to "not in duple check" — and matches FSF move-for-move.
//!
//! ## Hoplite promotion
//!
//! A Hoplite reaching the last rank promotes to a **Lieutenant, General, Captain,
//! or Warlord** — and to a **King** as well **only while its side has a single
//! king** (regaining the lost second king). It never stays a Hoplite. The
//! board-dependent target set rides on [`WideVariant::promotion_targets`].
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant spartan`, `position startpos`) renders the start as
//!
//! ```text
//! lgkcckwl/hhhhhhhh/8/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1
//! ```
//!
//! with FSF's Spartan letters `l g k c w h` (Lieutenant, General, King, Captain,
//! Warlord, Hoplite). mcr uses the same board but its own role letters — the
//! Lieutenant is `t`, the General `d`, the Captain `i`, the Warlord `a` (Hawk),
//! and the Hoplite `h`:
//!
//! ```text
//! tdkiikat/hhhhhhhh/8/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1
//! ```
//!
//! The two are the same position; the `compare-fairy/` harness translates the
//! Spartan letters when driving FSF. Only White has castling rights (`KQ`).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Chess8x8, Geometry, PromotionConfig, Square, StandardChess, WideRole,
    WideVariant,
};
use crate::Color;
use alloc::vec::Vec;

/// The Spartan rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides the Spartan piece movements, the Berolina Hoplite, the
/// multi-king king-safety, and the board-dependent Hoplite promotion targets.
/// White's pieces, castling, and promotion are the trait defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SpartanRules;

/// The confirmed Spartan starting placement, in mcr's role letters (White =
/// standard `RNBQKBNR`/`PPPPPPPP`; Black = `t d k i i k a t` on the back rank
/// with two kings on the c- and f-files, and a rank of Hoplites).
const SPARTAN_START_PLACEMENT: &str = "tdkiikat/hhhhhhhh/8/8/8/8/PPPPPPPP/RNBQKBNR";

/// The Lieutenant's **capturing** leaps: a one-square diagonal step and a
/// two-square diagonal jump. (Its sideways one-square steps are non-capturing
/// move-only squares — see [`LIEUTENANT_SIDEWAYS`].)
const LIEUTENANT_OFFSETS: [(i8, i8); 8] = [
    (-1, -1),
    (-1, 1),
    (1, -1),
    (1, 1),
    (-2, -2),
    (-2, 2),
    (2, -2),
    (2, 2),
];

/// The Lieutenant's **move-only** (non-capturing) sideways steps: one square
/// left or right. The Lieutenant may slide sideways onto an empty square but
/// never captures sideways (confirmed against FSF).
const LIEUTENANT_SIDEWAYS: [(i8, i8); 2] = [(-1, 0), (1, 0)];

/// The four ferz (one diagonal step) offsets — the General's diagonal component.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// The Captain's leaps: the four Wazir steps plus the four Dabbaba (two-square
/// orthogonal) jumps.
const CAPTAIN_OFFSETS: [(i8, i8); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (2, 0),
    (-2, 0),
    (0, 2),
    (0, -2),
];

impl WideVariant<Chess8x8> for SpartanRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 18;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(SPARTAN_START_PLACEMENT)
            .expect("the Spartan starting placement is valid on an 8x8 board");
        // Only White has castling rights (it is the only side with a rook-and-king
        // back rank); Black's Spartan back rank never castles. The kingside rook
        // sits on the last file, the queenside rook on file 0.
        let mut castling = GenericCastling::NONE;
        castling.set(Color::White, 0, Some(Chess8x8::WIDTH - 1));
        castling.set(Color::White, 1, Some(0));
        let state = GenericState {
            turn: Color::White,
            castling,
            ep_square: None,
            ep_captured: None,
            gating: GenericGating::NONE,
            duck: None,
            placement: GenericPlacement::NONE,
            halfmove_clock: 0,
            fullmove_number: 1,
            consecutive_passes: 0,
            board_b: crate::geometry::Bitboard::EMPTY,
            petrified: crate::geometry::Bitboard::EMPTY,
        };
        (board, state)
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Chess8x8>,
        occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        match role {
            // Lieutenant: a fixed leaper (sideways/diagonal step + diagonal jump).
            WideRole::Lieutenant => attacks::leaper_attacks::<Chess8x8>(sq, &LIEUTENANT_OFFSETS),
            // General: Rook + Ferz.
            WideRole::General => {
                attacks::rook_attacks::<Chess8x8>(sq, occupancy)
                    | attacks::leaper_attacks::<Chess8x8>(sq, &FERZ_OFFSETS)
            }
            // Captain: Wazir + Dabbaba.
            WideRole::Captain => attacks::leaper_attacks::<Chess8x8>(sq, &CAPTAIN_OFFSETS),
            // Warlord = Hawk (Bishop + Knight): reuse the census compound.
            WideRole::Hawk => <StandardChess as WideVariant<Chess8x8>>::role_attacks(
                WideRole::Hawk,
                color,
                sq,
                occupancy,
            ),
            // Hoplite: its **attack** (for check / king-danger) is the single
            // square straight forward — the only square it captures onto. Its
            // diagonal *move* is generated by the Berolina pawn path, not here.
            WideRole::Hoplite => {
                let forward: i8 = if color.is_white() { 1 } else { -1 };
                let mut bb = Bitboard::<Chess8x8>::EMPTY;
                if let Some(dest) = sq.offset(0, forward) {
                    bb.set(dest);
                }
                bb
            }
            // White's army and the kings are standard.
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn quiet_only_targets(
        role: WideRole,
        _color: Color,
        sq: Square<Chess8x8>,
        _occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        // The Lieutenant's sideways one-square steps are move-only: it may slide
        // left/right onto an empty square but never captures sideways. (The
        // generic generator filters these by emptiness.)
        match role {
            WideRole::Lieutenant => attacks::leaper_attacks::<Chess8x8>(sq, &LIEUTENANT_SIDEWAYS),
            _ => Bitboard::EMPTY,
        }
    }

    fn role_is_slider(role: WideRole) -> bool {
        match role {
            // The General slides orthogonally (its rook component can be pinned).
            WideRole::General => true,
            // The Lieutenant, Captain, and Hoplite are pure steppers/leapers; the
            // Warlord = Hawk and every standard role keep the trait classification.
            WideRole::Lieutenant | WideRole::Captain | WideRole::Hoplite => false,
            _ => <StandardChess as WideVariant<Chess8x8>>::role_is_slider(role),
        }
    }

    // --- multi-king + Berolina (Spartan-specific) -------------------------

    fn multi_royal() -> bool {
        true
    }

    fn has_berolina_pawns() -> bool {
        true
    }

    fn berolina_push_targets(color: Color, from: Square<Chess8x8>) -> Bitboard<Chess8x8> {
        // The Hoplite's non-capturing advance is the two diagonal-forward squares.
        let forward: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = Bitboard::<Chess8x8>::EMPTY;
        for df in [-1, 1] {
            if let Some(dest) = from.offset(df, forward) {
                bb.set(dest);
            }
        }
        bb
    }

    // --- Hoplite promotion ------------------------------------------------

    fn promotion_config() -> PromotionConfig {
        // The static set (used when a Hoplite has the maximum number of kings, so
        // it may not promote to King): Lieutenant, General, Captain, Warlord.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Lieutenant,
                WideRole::General,
                WideRole::Captain,
                WideRole::Hawk,
            ],
        }
    }

    fn promotion_targets(color: Color, board: &Board<Chess8x8>) -> Vec<WideRole> {
        // Promotion is **per army**, so the target set depends on the *promoting
        // side*, not just the board. The generic engine routes *every* promotion
        // through this hook, White's included, so White must be handled explicitly:
        // `promotion_config`'s default set is the Spartan one, and a colour-blind
        // body would (wrongly) hand a White pawn the four Spartan promotions plus an
        // illegal King.
        match color {
            // White = Persians: a standard pawn promotes to the standard `N/B/R/Q`
            // — never to a Spartan piece, never to a King.
            Color::White => alloc::vec![
                WideRole::Knight,
                WideRole::Bishop,
                WideRole::Rook,
                WideRole::Queen,
            ],
            // Black = Spartans: a Hoplite promotes to Lieutenant/General/Captain/
            // Warlord, plus a King **only while its side has a single king**
            // (regaining the lost second king).
            Color::Black => {
                let mut roles = Self::promotion_config().roles;
                if board.kings_of(color).count() < 2 {
                    roles.push(WideRole::King);
                }
                roles
            }
        }
    }
}

/// Spartan chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position (the asymmetric Persian-vs-Spartan array with
/// Black's two kings) with [`Spartan::startpos`](GenericPosition::startpos) or
/// parse a FEN with [`Spartan::from_fen`](GenericPosition::from_fen). See the
/// [module docs](self) for the piece movements, multi-king / duple-check rule,
/// and the Berolina Hoplite.
pub type Spartan = GenericPosition<Chess8x8, SpartanRules>;
