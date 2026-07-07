//! Losers chess (8x8) on the generic engine — **standard chess with a *royal*
//! king, forced captures, and every terminal inverted: you win by getting
//! checkmated, stalemated, or reduced to a bare king**. Validated against
//! Fairy-Stockfish `UCI_Variant losers` (a built-in; `losers_variant()`,
//! `variant.cpp:389`).
//!
//! Unlike the giveaway family, losers keeps a **royal** king — there *is* check and
//! checkmate, king safety applies, and castling is ordinary. What changes is the
//! **direction** of every decisive terminal, plus forced captures:
//!
//! * **Captures are forced** ([`WideVariant::mandatory_captures`]): whenever a legal
//!   capture exists, every legal move must be a capture. The narrowing runs on the
//!   already king-safe move set, exactly as Fairy-Stockfish's `legal()` does.
//! * **Being checkmated wins** ([`WideVariant::checkmate_is_win`], FSF
//!   `checkmateValue = +VALUE_MATE`).
//! * **Being stalemated wins** ([`WideVariant::stalemate_is_win`], FSF
//!   `stalemateValue = +VALUE_MATE`).
//! * **Being reduced to a bare king wins** — the total-piece extinction with
//!   `threshold = 1` in the inverted direction (FSF `extinctionPieceTypes =
//!   ALL_PIECES`, `extinctionPieceCount = 1`, `extinctionValue = +VALUE_MATE`): a
//!   side left with only its king has won.
//!
//! Because the king stays royal it can never be captured, so the bare-king
//! extinction (not a king capture) is the material win; the `threshold = 1` node
//! truncates to zero moves exactly as Fairy-Stockfish adjudicates it.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant losers` — the standard array with
//! castling rights:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! ```
//!
//! [`ExtinctionRule`]: crate::geometry::ExtinctionRule

use super::giveaway::GIVEAWAY_START_PLACEMENT;
use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{Bitboard, Board, Chess8x8, ExtinctionRule, WideVariant};
use crate::Color;

/// The Losers chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// Overrides only the terminal *direction* and forced captures; the king stays
/// royal (there is check and checkmate), movement / castling / en passant / standard
/// `Q`/`R`/`B`/`N` promotion are the trait defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LosersRules;

impl WideVariant<Chess8x8> for LosersRules {
    /// The tightest prefix of `WideRole::ALL` covering every fieldable role
    /// (Pawn..King, the standard army; promotions are Q/R/B/N). See
    /// [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(GIVEAWAY_START_PLACEMENT)
            .expect("the losers starting placement is valid on an 8x8 board");
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

    // The king is **royal** (there is check / checkmate): `non_royal_king` and
    // `royal_squares` keep their defaults.

    // --- forced captures --------------------------------------------------

    fn mandatory_captures() -> bool {
        true
    }

    // --- inverted terminals -----------------------------------------------

    /// A side reduced to a **bare king** (total piece count `<= 1`) wins — the
    /// total-piece extinction in the inverted (`extinct_wins`) direction.
    fn extinction_rule() -> Option<ExtinctionRule> {
        Some(ExtinctionRule {
            watched: &[],
            threshold: 1,
            count_total: true,
            extinct_wins: true,
        })
    }

    /// Being checkmated wins (FSF `checkmateValue = +VALUE_MATE`).
    fn checkmate_is_win() -> bool {
        true
    }

    /// Being stalemated wins (FSF `stalemateValue = +VALUE_MATE`).
    fn stalemate_is_win() -> bool {
        true
    }

    // Promotion is the standard `[Knight, Bishop, Rook, Queen]` default (no
    // king-promotion — the king stays royal).
}

/// Losers chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Losers::startpos`](GenericPosition::startpos) or parse a plain-chess FEN with
/// [`Losers::from_fen`](GenericPosition::from_fen). Movement is king-safe
/// standard-chess narrowed to forced captures; getting checkmated, stalemated, or
/// bared to a lone king wins.
pub type Losers =
    GenericPosition<Chess8x8, LosersRules, { <LosersRules as WideVariant<Chess8x8>>::ROLE_SPAN }>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{WideEndReason, WideOutcome};

    #[test]
    fn startpos_fen_round_trips() {
        let pos = Losers::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
        assert_eq!(pos.legal_move_count(), 20);
        assert_eq!(pos.end_reason(), None);
    }

    #[test]
    fn king_is_royal_there_is_check() {
        // A black rook checks the white king down the open e-file; the king is royal,
        // so this IS check (unlike the giveaway family).
        let pos = Losers::from_fen("4r3/8/8/8/8/8/8/4K3 w - - 0 1").expect("valid FEN");
        assert!(pos.is_check(), "the royal king is in check");
    }

    #[test]
    fn captures_are_forced() {
        // White pawn e4, black pawn d5: exd5 is the only capture, so it is forced.
        let pos = Losers::from_fen("4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1").expect("valid FEN");
        let moves = pos.legal_moves();
        assert!(
            moves.iter().all(|m| m.is_capture()),
            "only captures survive"
        );
        assert_eq!(moves.len(), 1);
    }

    #[test]
    fn bare_king_wins_for_the_bared_side() {
        // Black (to move) is reduced to a lone king — it has won by being bared.
        let pos = Losers::from_fen("7k/8/8/3P4/8/8/8/K7 b - - 0 1").expect("valid FEN");
        assert_eq!(pos.extinction_loser(), Some(Color::Black));
        assert!(pos.legal_moves().is_empty(), "terminal — no moves");
        assert_eq!(pos.end_reason(), Some(WideEndReason::VariantWin));
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::Black
            }),
        );
    }

    #[test]
    fn checkmated_side_wins() {
        // Fool's mate: White (to move) is checkmated — in losers the mated side wins.
        let pos = Losers::from_fen("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3")
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
}
