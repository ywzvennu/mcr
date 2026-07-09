//! Grasshopper chess (8x8) on the generic engine — standard chess with an extra
//! **Grasshopper** in front of each pawn and no pawn double step. Validated
//! node-for-node against Fairy-Stockfish (`UCI_Variant grasshopper`, an FSF
//! built-in).
//!
//! Grasshopper chess is ordinary chess with one new piece and two rule tweaks:
//!
//! * **Grasshopper** ([`WideRole::Grasshopper`], Betza `gQ`) — a queen-line
//!   **hopper**. Along any of the eight queen directions it slides over the empty
//!   run to the first piece it meets (of either colour, the "hurdle") and lands on
//!   the single square **immediately beyond** that hurdle — a quiet move if that
//!   square is empty, a capture if it holds an enemy, and blocked if it holds a
//!   friendly piece. With **no** hurdle on a ray it has no move there, and it can
//!   never land more than one square past the hurdle. It gives check the same way
//!   (a king exactly one square beyond a hurdle along a queen line is in check).
//!   Its attack set is occupancy-aware ([`attacks::grasshopper_attacks`]) and
//!   geometrically asymmetric, so it rides the engine's pseudo-legal + per-move
//!   king-safety **verify** path (via [`has_cannons`](WideVariant::has_cannons)),
//!   exactly as the screen-dependent cannon does, and is detected as an attacker by
//!   forward projection (via
//!   [`role_attack_is_leg_asymmetric`](WideVariant::role_attack_is_leg_asymmetric)).
//! * **Pawns** push one square only — **no double step** (and therefore **no en
//!   passant**), even though they start on the third rank. Diagonal captures and
//!   last-rank promotion are standard.
//! * **Promotion** targets are the Knight, Bishop, Rook, Queen, **or Grasshopper**.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant grasshopper`, `position startpos`) renders the start as
//!
//! ```text
//! rnbqkbnr/gggggggg/pppppppp/8/8/PPPPPPPP/GGGGGGGG/RNBQKBNR w KQkq - 0 1
//! ```
//!
//! a full rank of grasshoppers (`g`/`G`) between the pawns and the back rank, with
//! the pawns on ranks 3 / 6. mcr uses the same board but spells the grasshopper
//! with its fourth-tier overflow token `***j` / `***J`
//! ([`WideRole::Grasshopper`]):
//!
//! ```text
//! rnbqkbnr/***j***j***j***j***j***j***j***j/pppppppp/8/8/PPPPPPPP/***J***J***J***J***J***J***J***J/RNBQKBNR w KQkq - 0 1
//! ```
//!
//! The two are the same position; the `compare-fairy/` harness rewrites `***j → g`
//! when driving FSF.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Chess8x8, Geometry, PromotionConfig, RoyalSlider, Square,
    StandardChess, WideRole, WideVariant,
};
use crate::Color;

/// The confirmed Grasshopper-chess starting placement, in mcr's role letters: the
/// standard back ranks and pawns (on ranks 3 / 6), with a full rank of
/// Grasshoppers (`***j` / `***J`) on ranks 2 / 7.
const GRASSHOPPER_START_PLACEMENT: &str = "rnbqkbnr/\
    ***j***j***j***j***j***j***j***j/\
    pppppppp/8/8/PPPPPPPP/\
    ***J***J***J***J***J***J***J***J/\
    RNBQKBNR";

/// The Grasshopper-chess rule layer: a zero-sized [`WideVariant`] over
/// [`Chess8x8`].
///
/// It changes exactly three things from standard chess: it adds the
/// [`WideRole::Grasshopper`] mover (a queen-line hopper) and its start rank,
/// removes the pawn double step (and hence en passant), and offers the Grasshopper
/// as a promotion target. Because the grasshopper's check and king-danger are
/// hurdle-dependent, the variant opts into the engine's cannon-style verify path
/// ([`has_cannons`](WideVariant::has_cannons)); everything else — castling, pins,
/// check and checkmate — is the generic engine's standard machinery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GrasshopperRules;

impl WideVariant<Chess8x8> for GrasshopperRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field. The Grasshopper is the highest-indexed role
    /// ([`WideRole::Grasshopper`] = `146`), so the span is the full
    /// [`WideRole::COUNT`]. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = WideRole::COUNT;

    /// The western **fifty-move rule**: 100 plies with no capture or pawn move is a
    /// [`WideEndReason::MoveRule`](crate::geometry::WideEndReason::MoveRule) draw,
    /// matching Fairy-Stockfish's default `nMoveRule = 50`. Adjudication-only (never
    /// gates move generation), so perft stays byte-identical.
    fn move_rule_plies() -> Option<u16> {
        Some(100)
    }

    /// Records a position history so the standard **threefold** repetition draw
    /// fires at the [`GenericGame`](crate::geometry::game::GenericGame) level.
    /// History-dependent and never consulted by a bare [`GenericPosition`], so perft
    /// is unchanged.
    fn tracks_repetition() -> bool {
        true
    }

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(GRASSHOPPER_START_PLACEMENT)
            .expect("the Grasshopper-chess starting placement is valid on an 8x8 board");
        // Both sides castle, with rooks on files 0 and WIDTH-1 (standard layout).
        let mut castling = GenericCastling::NONE;
        for color in [Color::White, Color::Black] {
            castling.set(color, 0, Some(Chess8x8::WIDTH - 1));
            castling.set(color, 1, Some(0));
        }
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
            checks_against: [0, 0],
            jieqi_seed: None,
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
            // The Grasshopper: along each queen ray, the single square immediately
            // beyond the first hurdle. Occupancy-aware, so this same set is its move
            // set (the generator splits quiet / capture by occupancy) and its threat
            // set (the squares from which it gives check).
            WideRole::Grasshopper => attacks::grasshopper_attacks::<Chess8x8>(sq, occupancy),
            // Pawns, knights, sliders, king are standard chess.
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn role_is_slider(role: WideRole) -> bool {
        // The Grasshopper is a hopper, not a line slider: on an empty board it has
        // no move at all, so it can never pin a lone blocker the way a rook does.
        // Every other role keeps the standard classification.
        <StandardChess as WideVariant<Chess8x8>>::role_is_slider(role)
    }

    fn role_attack_is_leg_asymmetric(role: WideRole) -> bool {
        // The Grasshopper's landing set is geometrically asymmetric — "a attacks b"
        // is not "b attacks a" under the same occupancy (a hopper lands one beyond a
        // hurdle, not symmetrically back). Reverse-projecting the pattern from the
        // king square would be wrong, so attacker / king-safety detection forward-
        // projects from each grasshopper instead, exactly as the move generator does.
        // Every standard role is symmetric and needs no special handling.
        matches!(role, WideRole::Grasshopper)
    }

    fn royal_slider_kind(role: WideRole) -> Option<RoyalSlider> {
        // Rook, Bishop, and Queen are the plain standard sliders here, so the verify
        // path can reverse-project them from the king with its precomputed line
        // masks instead of rebuilding the slider masks per sibling move. The
        // Grasshopper (asymmetric, hurdle-dependent) is not a standard slider.
        match role {
            WideRole::Rook => Some(RoyalSlider::Rook),
            WideRole::Bishop => Some(RoyalSlider::Bishop),
            WideRole::Queen => Some(RoyalSlider::Queen),
            _ => None,
        }
    }

    fn promotion_config() -> PromotionConfig {
        // A pawn promotes to Knight, Bishop, Rook, Queen, or Grasshopper — FSF's
        // `promotionPieceTypes`. Order affects only enumeration, not the perft count.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Knight,
                WideRole::Bishop,
                WideRole::Rook,
                WideRole::Queen,
                WideRole::Grasshopper,
            ],
        }
    }

    fn has_cannons() -> bool {
        // The Grasshopper's check and king-danger are hurdle-dependent: a king
        // sliding along its ray, or a piece interposing on / vacating the empty run
        // or moving the hurdle, changes the attack in a way the lifted-king danger
        // map and the `between` interpose mask cannot capture. So the variant takes
        // the engine's pseudo-legal + per-move king-safety verify path (the same one
        // the screen-dependent cannon uses), which recomputes attacks on the true
        // post-move occupancy.
        true
    }

    fn pawn_may_double_push_from(_rank: u8, _color: Color) -> bool {
        // Grasshopper-chess pawns never double-step (FSF `doubleStep = false`), even
        // from their third-rank start — and with no double step there is no en
        // passant target ever set.
        false
    }
}

/// Grasshopper chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Grasshopper::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Grasshopper::from_fen`](GenericPosition::from_fen). The Grasshopper uses the
/// reusable [`attacks::grasshopper_attacks`] hopper primitive; everything else is
/// standard chess with single-step pawns.
pub type Grasshopper = GenericPosition<
    Chess8x8,
    GrasshopperRules,
    { <GrasshopperRules as WideVariant<Chess8x8>>::ROLE_SPAN },
>;
