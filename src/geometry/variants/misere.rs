//! Misère chess (8x8) on the generic engine — **standard chess in which getting
//! checkmated *wins***. Validated against Fairy-Stockfish `UCI_Variant misere` (a
//! built-in; `misere_variant()`, `variant.cpp:381`).
//!
//! Misère is the simplest of the antichess family: it is **ordinary chess in every
//! respect** — a royal king with check and checkmate, standard castling, en
//! passant, `Q`/`R`/`B`/`N` promotion, and (unlike the giveaway / losers family)
//! **no** forced captures — except that the decisive terminal is **inverted**:
//! delivering checkmate *loses*, so the checkmated side **wins**
//! ([`WideVariant::checkmate_is_win`], FSF `checkmateValue = +VALUE_MATE`). A
//! stalemate is the ordinary draw.
//!
//! Because nothing about *movement* changes, misère's move generation and perft are
//! **byte-identical to standard chess**; only the reported [outcome] of a checkmate
//! differs.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant misere` — the standard array:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! ```
//!
//! [outcome]: crate::geometry::GenericPosition::outcome

use super::giveaway::GIVEAWAY_START_PLACEMENT;
use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{Bitboard, Board, Chess8x8, WideVariant};
use crate::Color;

/// The Misère chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// Overrides only the checkmate *direction* ([`WideVariant::checkmate_is_win`]);
/// every movement and terminal rule is otherwise the standard-chess trait default,
/// so its move set is byte-identical to standard chess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MisereRules;

impl WideVariant<Chess8x8> for MisereRules {
    /// The standard army only (Pawn..King, promotions Q/R/B/N). See
    /// [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(GIVEAWAY_START_PLACEMENT)
            .expect("the misère starting placement is valid on an 8x8 board");
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
            checks_against: [0, 0],
        };
        (board, state)
    }

    /// Being checkmated wins (FSF `checkmateValue = +VALUE_MATE`). This is misère's
    /// only departure from standard chess.
    fn checkmate_is_win() -> bool {
        true
    }
}

/// Misère chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Misere::startpos`](GenericPosition::startpos) or parse a plain-chess FEN with
/// [`Misere::from_fen`](GenericPosition::from_fen). Movement is standard chess; a
/// checkmate is a win for the mated side.
pub type Misere =
    GenericPosition<Chess8x8, MisereRules, { <MisereRules as WideVariant<Chess8x8>>::ROLE_SPAN }>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Chess8x8, StandardChess, WideEndReason, WideOutcome};

    #[test]
    fn startpos_and_perft_match_standard_chess() {
        let pos = Misere::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
        // Movement is byte-identical to standard chess.
        let std = GenericPosition::<Chess8x8, StandardChess, 6>::startpos();
        for d in 0..=3 {
            assert_eq!(
                crate::geometry::perft(&pos, d),
                crate::geometry::perft(&std, d),
                "misère perft({d}) equals standard chess",
            );
        }
    }

    #[test]
    fn checkmated_side_wins() {
        // Fool's mate: White (to move) is checkmated — in misère the mated side wins.
        let pos = Misere::from_fen("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3")
            .expect("valid FEN");
        assert!(pos.is_check());
        assert!(pos.legal_moves().is_empty(), "checkmate — no legal move");
        assert_eq!(pos.end_reason(), Some(WideEndReason::Checkmate));
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::White
            }),
            "the checkmated side wins",
        );
    }

    #[test]
    fn stalemate_is_an_ordinary_draw() {
        // Classic K+Q stalemate: Black (to move) has no move and is not in check.
        // Misère leaves stalemate a draw (only checkmate is inverted).
        let pos = Misere::from_fen("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1").expect("valid FEN");
        assert!(!pos.is_check());
        assert!(pos.legal_moves().is_empty());
        assert_eq!(pos.end_reason(), Some(WideEndReason::Stalemate));
        assert_eq!(pos.outcome(), Some(WideOutcome::Draw));
    }
}
