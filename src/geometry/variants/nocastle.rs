//! No-castle chess (8x8) on the generic engine — **standard chess with castling
//! disabled**, and nothing else changed. It is the reference [`StandardChess`]
//! ruleset over [`Chess8x8`] with the one rule turned off: neither side may ever
//! castle, so the back-rank king and rooks move only as ordinary pieces.
//!
//! No-castle chess is the simplest possible fairy variant: it reuses the standard
//! army, the standard 8x8 geometry, the standard pawn double-step / en passant /
//! promotion, and standard checkmate — it removes castling and leaves everything
//! else identical. From the opening array the two games are indistinguishable
//! until a castle would become available; a position with cleared back-rank
//! squares (castling rights present in standard chess) is where the node counts
//! diverge, exactly the castling moves being absent.
//!
//! ## Rules — standard chess without castling
//!
//! * **No castling.** [`has_castling`](WideVariant::has_castling) is `false`, and
//!   the starting state carries no castling rights, so the generator never emits a
//!   castle. Every other king and rook move is standard.
//! * **Pawns** double-push from their second rank, capture diagonally, take en
//!   passant, and promote on the far rank (the trait defaults) to Queen, Rook,
//!   Bishop, or Knight.
//! * **Win by checkmate**, standard 8x8 chess otherwise.
//!
//! No draw hook is overridden: like the reference [`StandardChess`], no-castle
//! chess carries the trait-default terminal rules (no fifty-move / repetition /
//! insufficient-material adjudication at the bare-position level), matching
//! Fairy-Stockfish's `nocastle` for perft.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant nocastle`
//! (`nocastle_variant()`, `variant.cpp:62` — the standard `chess_variant()` with
//! `castling = false`). The array is the standard chess array; the castling field
//! is `-` (no rights):
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1
//! ```
//!
//! mcr and FSF spell the position byte-for-byte identically (standard chess
//! letters, no dialect rewrite).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
#[allow(unused_imports)] // `StandardChess` is referenced by the rustdoc intra-doc links.
use crate::geometry::StandardChess;
use crate::geometry::{Board, Chess8x8, WideVariant};
use crate::Color;

/// The confirmed no-castle starting placement: the standard chess array.
const NOCASTLE_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

/// The no-castle chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It is the reference [`StandardChess`] ruleset with exactly one change —
/// castling is disabled ([`has_castling`](WideVariant::has_castling) is `false`
/// and the start state grants no castling rights). Every piece, the pawn
/// double-step, en passant, promotion, and checkmate are the standard-chess trait
/// defaults, so only the absence of castling distinguishes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NocastleRules;

impl WideVariant<Chess8x8> for NocastleRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (Pawn..King, the standard army; promotions are Queen
    /// / Rook / Bishop / Knight, all within the prefix). See
    /// [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(NOCASTLE_START_PLACEMENT)
            .expect("the standard starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            // No castling rights: castling is disabled entirely.
            castling: GenericCastling::NONE,
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

    /// No-castle chess has no castling. The generic move generator consults this to
    /// suppress castle generation, and the FEN layer to reject castling rights, so
    /// neither side can ever castle — the single rule that separates this variant
    /// from standard chess.
    fn has_castling() -> bool {
        false
    }
}

/// No-castle chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Nocastle::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Nocastle::from_fen`](GenericPosition::from_fen). Every rule is the standard
/// [`StandardChess`] default except castling, which is disabled.
pub type Nocastle = GenericPosition<Chess8x8, NocastleRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::WideMoveKind;

    /// The canonical start FEN round-trips and has no castling rights.
    #[test]
    fn startpos_fen_round_trips_without_castling() {
        let pos = Nocastle::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1"
        );
        assert_eq!(pos.turn(), Color::White);
        assert_eq!(pos.legal_move_count(), 20);
        assert!(!pos.castling().has_any(Color::White));
        assert!(!pos.castling().has_any(Color::Black));
    }

    /// A position where standard chess would castle emits no castle move here: the
    /// king on e1 with both rooks on a1/h1 and empty back rank produces king steps
    /// only, never a `CastleKingside` / `CastleQueenside`.
    #[test]
    fn no_castle_move_when_standard_chess_would() {
        let pos = Nocastle::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w - - 0 1").expect("valid FEN");
        let castles = pos
            .legal_moves()
            .into_iter()
            .filter(|m| {
                matches!(
                    m.kind(),
                    WideMoveKind::CastleKingside | WideMoveKind::CastleQueenside
                )
            })
            .count();
        assert_eq!(castles, 0, "no-castle chess never castles");
    }

    /// A double pawn push still sets an en-passant target — every non-castling rule
    /// is standard chess.
    #[test]
    fn pawn_double_push_sets_en_passant() {
        let pos = Nocastle::startpos();
        let dbl = pos
            .legal_moves()
            .into_iter()
            .find(|m| matches!(m.kind(), WideMoveKind::DoublePawnPush))
            .expect("a double pawn push exists at the start");
        let next = pos.play(&dbl);
        assert!(
            next.ep_square().is_some(),
            "a double push creates an en-passant target",
        );
    }

    /// A FEN with the `-` castling field parses and carries no castling rights.
    #[test]
    fn parsed_fen_has_no_castling_rights() {
        let pos = Nocastle::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1")
            .expect("valid no-castle FEN");
        assert!(!pos.castling().has_any(Color::White));
        assert!(!pos.castling().has_any(Color::Black));
    }
}
