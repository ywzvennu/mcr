//! Suicide chess (8x8) on the generic engine — **antichess (giveaway without
//! castling) with a piece-count stalemate rule**. Validated against
//! Fairy-Stockfish `UCI_Variant suicide` (a built-in; `suicide_variant()`,
//! `variant.cpp:430`, built on `antichess_variant()`).
//!
//! Suicide is [giveaway](super::giveaway) with two changes: **castling is
//! disabled** (the FreeChess / antichess ruleset), and a **stalemate is decided by
//! piece count** rather than being an outright win. Everything else is giveaway:
//!
//! * **The king is a non-royal Commoner** — no check, no checkmate, capturable.
//! * **Captures are forced** ([`WideVariant::mandatory_captures`]).
//! * **Pawns may promote to a king** as well as to Q/R/B/N.
//! * **Losing your whole army wins** — the same total-piece extinction giveaway
//!   uses (FSF `extinctionPieceTypes = ALL_PIECES`, `extinctionValue =
//!   +VALUE_MATE`).
//!
//! ## Stalemate — decided by piece count
//!
//! Where giveaway makes any stalemate an outright win, suicide breaks it by
//! material ([`WideVariant::stalemate_piece_count`], FSF `stalematePieceCount`): the
//! stalemated side with **fewer** pieces wins, an **equal** count draws. This is an
//! adjudication-only difference — a stalemated node generates zero moves in both, so
//! perft is identical to giveaway's on any castling-free position.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant suicide` — the standard array with
//! **no** castling rights (suicide has no castling):
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1
//! ```
//!
//! [`ExtinctionRule`]: crate::geometry::ExtinctionRule

use super::giveaway::GIVEAWAY_START_PLACEMENT;
use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    Bitboard, Board, Chess8x8, ExtinctionRule, PromotionConfig, WideRole, WideVariant,
};
use crate::Color;

/// The Suicide chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// Overrides only what suicide changes about standard chess: a non-royal Commoner
/// king, mandatory captures, king-promotion, no castling, the whole-army "losing
/// wins" terminal, and the piece-count stalemate rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SuicideRules;

impl WideVariant<Chess8x8> for SuicideRules {
    /// The tightest prefix of `WideRole::ALL` covering every fieldable role
    /// (Pawn..King; a pawn may promote to the king). See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(GIVEAWAY_START_PLACEMENT)
            .expect("the suicide starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            // Suicide (antichess) has no castling.
            castling: GenericCastling::NONE,
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
            jieqi_seed: None,
        };
        (board, state)
    }

    // --- no castling ------------------------------------------------------

    fn has_castling() -> bool {
        // Antichess / suicide never castle, even from a FEN that grants rights.
        false
    }

    // --- non-royal Commoner king (no check) -------------------------------

    fn non_royal_king() -> bool {
        true
    }

    fn royal_squares<const R: usize>(
        _board: &Board<Chess8x8, R>,
        _color: Color,
    ) -> Bitboard<Chess8x8> {
        Bitboard::EMPTY
    }

    // --- forced captures --------------------------------------------------

    fn mandatory_captures() -> bool {
        true
    }

    // --- inverted "losing wins" terminal ----------------------------------

    /// A side that loses its **whole army** wins (total-piece extinction, inverted).
    fn extinction_rule() -> Option<ExtinctionRule> {
        Some(ExtinctionRule {
            watched: &[],
            threshold: 0,
            count_total: true,
            extinct_wins: true,
            opponent_min: 0,
        })
    }

    /// A stalemate is decided by piece count: the stalemated side with fewer pieces
    /// wins, an equal count draws (FSF `stalematePieceCount`).
    fn stalemate_piece_count() -> bool {
        true
    }

    // --- promotion (adds the Commoner king) -------------------------------

    fn promotion_config() -> PromotionConfig {
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Knight,
                WideRole::Bishop,
                WideRole::Rook,
                WideRole::Queen,
                WideRole::King,
            ],
        }
    }
}

/// Suicide chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Suicide::startpos`](GenericPosition::startpos) or parse a plain-chess FEN with
/// [`Suicide::from_fen`](GenericPosition::from_fen). Movement is antichess (giveaway
/// without castling); a stalemate is scored by piece count.
pub type Suicide =
    GenericPosition<Chess8x8, SuicideRules, { <SuicideRules as WideVariant<Chess8x8>>::ROLE_SPAN }>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Chess8x8, WideEndReason, WideOutcome};

    #[test]
    fn startpos_has_no_castling_rights() {
        let pos = Suicide::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1"
        );
        assert_eq!(pos.legal_move_count(), 20);
    }

    #[test]
    fn no_castling_even_with_rights_in_fen() {
        // A FEN granting KQkq must still produce no castling move (suicide never
        // castles): 23 moves, not giveaway's 25.
        let pos = Suicide::from_fen("r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1")
            .expect("valid FEN");
        let ucis: alloc::vec::Vec<_> = pos
            .legal_moves()
            .iter()
            .map(|m| m.to_uci::<Chess8x8>())
            .collect();
        assert!(
            !ucis.iter().any(|u| u == "e1g1" || u == "e1c1"),
            "no castling: {ucis:?}"
        );
        assert_eq!(pos.legal_move_count(), 23);
    }

    #[test]
    fn stalemate_with_fewer_pieces_wins() {
        // White (to move) is immobilised (pawn a2 blocked by a3) and has fewer
        // pieces than Black, so White wins the stalemate.
        let pos = Suicide::from_fen("6bb/8/8/8/8/p7/P7/8 w - - 0 1").expect("valid FEN");
        assert!(pos.legal_moves().is_empty());
        assert_eq!(pos.end_reason(), Some(WideEndReason::Stalemate));
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::White
            }),
            "fewer pieces wins the stalemate",
        );
    }

    #[test]
    fn stalemate_with_more_pieces_loses() {
        // White is immobilised (both pawns blocked, the corner bishop shut in by its
        // own g2 pawn) but has MORE pieces (3) than Black (2), so White loses.
        let pos = Suicide::from_fen("8/8/8/8/8/p5p1/P5P1/7B w - - 0 1").expect("valid FEN");
        assert!(pos.legal_moves().is_empty());
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::Black
            }),
            "more pieces loses the stalemate",
        );
    }

    #[test]
    fn stalemate_with_equal_pieces_draws() {
        // Equal piece counts (one each) — the stalemate is a draw.
        let pos = Suicide::from_fen("8/8/8/8/8/p7/P7/8 w - - 0 1").expect("valid FEN");
        assert!(pos.legal_moves().is_empty());
        assert_eq!(pos.outcome(), Some(WideOutcome::Draw));
    }
}
