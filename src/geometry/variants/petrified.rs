//! Petrified chess — sideways pawns plus turn-to-stone captures.
//!
//! Petrified chess is standard 8x8 chess with two twists (Fairy-Stockfish's
//! built-in `petrified`, itself the `pawnsideways` base plus a petrify rule):
//!
//! * **Sideways pawns.** A pawn may take a single quiet step sideways (left or
//!   right onto an empty square), exactly as in
//!   [Pawn-sideways chess](super::pawnsideways). Double steps, diagonal captures,
//!   en passant, and promotion are otherwise standard.
//! * **Petrify on capture.** When a **Queen, Rook, Bishop, or Knight** makes a
//!   capture it is *turned to stone* on the square it lands on: the piece is
//!   removed from the board and that square becomes an inert, colorless **wall**.
//!   A wall blocks sliding pieces and can never move, capture, be captured, or
//!   give check for the rest of the game. A capturing **pawn** is *not* petrified.
//! * **Pseudo-royal Commoner.** The king is a **Commoner** — spelled `k`/`K` and
//!   moving like an ordinary king, but *not* checkmated. Instead a side loses when
//!   its Commoner would be captured / goes extinct (FSF `extinctionPseudoRoyal`
//!   with `extinctionPieceTypes = COMMONER`). Because a Commoner is in the petrify
//!   set, capturing would turn it to stone and forfeit its royalty, so the
//!   Commoner **may never capture**. Consequently two Commoners may stand adjacent
//!   (neither attacks the other), and the Commoner never attacks, defends, or
//!   gives check.
//!
//! Standard castling (with the Commoner as the castling king piece) is retained.
//!
//! ## Wiring
//!
//! The Commoner reuses [`WideRole::King`] (spelled `k`/`K`), so the FEN and the
//! generic castling machinery work unchanged. Pseudo-royalty rides the multi-royal
//! path with [`WideVariant::royals_all_must_survive`]; the petrify mechanic rides
//! the [`WideVariant::has_petrify`] / [`WideVariant::role_petrifies`] hooks, and the
//! Commoner's no-capture / no-attack behaviour rides
//! [`WideVariant::royal_cannot_capture`]. The wall squares live in
//! [`GenericState::petrified`](crate::geometry::position::GenericState::petrified).
//!
//! Confirmed start FEN `rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1`
//! with startpos perft(1) = 20, perft(3) = 10022, validated against
//! Fairy-Stockfish (`UCI_Variant petrified`).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{Bitboard, Board, Chess8x8, ExtinctionRule, WideRole, WideVariant};
use crate::Color;

/// The standard 8x8 starting placement (Petrified shares the chess array; the
/// king letter `k`/`K` is the pseudo-royal Commoner).
const PETRIFIED_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

/// The pseudo-royal piece Petrified chess watches for extinction: the Commoner
/// ([`WideRole::King`]). A side loses the instant its Commoner count drops to
/// zero (FSF `extinctionPieceTypes = COMMONER`, `extinctionPseudoRoyal`).
const PETRIFIED_WATCHED: &[WideRole] = &[WideRole::King];

/// The Petrified chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PetrifiedRules;

impl WideVariant<Chess8x8> for PetrifiedRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field (Pawn..King, the standard army; promotions are Queen /
    /// Rook / Bishop / Knight, all within the prefix). See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(PETRIFIED_START_PLACEMENT)
            .expect("the Petrified starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::standard::<Chess8x8>(),
            ep_square: None,
            ep_captured: None,
            gating: GenericGating::NONE,
            duck: None,
            placement: GenericPlacement::NONE,
            halfmove_clock: 0,
            fullmove_number: 1,
            consecutive_passes: 0,
            board_b: Bitboard::EMPTY,
            petrified: Bitboard::EMPTY,
        };
        (board, state)
    }

    // --- sideways pawns (from the pawnsideways base) ----------------------

    /// The Pawn may take an extra sideways quiet step onto an adjacent empty
    /// square, exactly as in [Pawn-sideways chess](super::pawnsideways).
    fn pawn_moves_sideways() -> bool {
        true
    }

    // --- pseudo-royal Commoner (multi-royal, all-must-survive) ------------

    /// The Commoner is pseudo-royal via the multi-royal path: routing through it
    /// (rather than the fast single-royal generator) lets the engine apply the
    /// Commoner's no-capture / no-attack rules and the petrify occupancy during
    /// per-move legality verification.
    fn multi_royal() -> bool {
        true
    }

    /// The single Commoner must be left safe by every move — the strict
    /// pseudo-royal rule (FSF `extinctionPseudoRoyal`): a side may not leave its
    /// Commoner en prise, and loses when it cannot avoid its capture.
    fn royals_all_must_survive() -> bool {
        true
    }

    /// The Commoner may not capture (a capture would petrify it and forfeit its
    /// royalty), so it never attacks, defends, or gives check — two Commoners may
    /// stand adjacent.
    fn royal_cannot_capture() -> bool {
        true
    }

    // --- petrify-on-capture -----------------------------------------------

    /// Petrified chess has the turn-to-stone mechanic.
    fn has_petrify() -> bool {
        true
    }

    /// A capturing Queen, Rook, Bishop, or Knight is petrified. A capturing Pawn is
    /// not, and the pseudo-royal Commoner can never capture (so is never petrified).
    fn role_petrifies(role: WideRole) -> bool {
        matches!(
            role,
            WideRole::Queen | WideRole::Rook | WideRole::Bishop | WideRole::Knight
        )
    }

    // --- extinction terminal (Commoner, threshold 0) ----------------------

    /// A side loses the moment its Commoner is captured / goes extinct (FSF
    /// `extinctionPieceTypes = COMMONER`, `extinctionPieceCount = 0`).
    fn extinction_rule() -> Option<ExtinctionRule> {
        Some(ExtinctionRule {
            watched: PETRIFIED_WATCHED,
            threshold: 0,
        })
    }

    // --- standard-chess housekeeping --------------------------------------

    /// Standard castling (the Commoner is the castling king piece).
    fn has_castling() -> bool {
        true
    }

    /// The western fifty-move rule (FSF `nMoveRule = 50`, i.e. 100 plies).
    fn move_rule_plies() -> Option<u16> {
        Some(100)
    }

    /// Records a position history for the standard threefold repetition draw.
    fn tracks_repetition() -> bool {
        true
    }
}

/// Petrified chess as a ready-to-use position type over [`Chess8x8`].
pub type Petrified = GenericPosition<
    Chess8x8,
    PetrifiedRules,
    { <PetrifiedRules as WideVariant<Chess8x8>>::ROLE_SPAN },
>;
