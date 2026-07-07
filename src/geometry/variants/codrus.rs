//! Codrus (8x8) on the generic engine — **giveaway chess in which you win by
//! losing your *king***. Validated against Fairy-Stockfish `UCI_Variant codrus` (a
//! built-in; `codrus_variant()`, `variant.cpp:440`).
//!
//! Codrus is [giveaway](super::giveaway) with two changes: the watched extinction
//! type is the **king (Commoner) alone**, and pawns promote to Q/R/B/N only (not to
//! a king). Everything else is giveaway:
//!
//! * **The king is a non-royal Commoner** — no check, no checkmate, it may be left
//!   en prise and captured.
//! * **Captures are forced** ([`WideVariant::mandatory_captures`]).
//! * **Ordinary castling** is kept (like giveaway, unlike lichess antichess).
//! * **Losing your king wins.** A side whose king is captured — its king count
//!   drops to zero — **wins** (FSF `extinctionPieceTypes = COMMONER`,
//!   `extinctionValue = +VALUE_MATE`); so does being **stalemated**
//!   ([`WideVariant::stalemate_is_win`], inherited from the giveaway base).
//!
//! ## Terminal — the inverted king extinction
//!
//! Codrus reuses the generic [`WideVariant::extinction_rule`] hook watching
//! `[King]` with `threshold = 0` in the inverted
//! ([`extinct_wins`](crate::geometry::ExtinctionRule::extinct_wins)) direction. It
//! is the win-direction mirror of [Petrified](super::petrified) / a plain
//! king-capture *loss*: here the captured-king side wins.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant codrus` — the standard array with
//! castling rights (codrus keeps giveaway's castling):
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
use crate::geometry::{Bitboard, Board, Chess8x8, ExtinctionRule, WideRole, WideVariant};
use crate::Color;

/// The single watched type: the non-royal king ([`WideRole::King`]). A side wins the
/// instant its king count drops to zero (its king is captured).
const CODRUS_WATCHED: &[WideRole] = &[WideRole::King];

/// The Codrus rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// Overrides only what codrus changes about standard chess: a non-royal Commoner
/// king, mandatory captures, the inverted "lose your king wins" terminal, and
/// stalemate-as-win. Movement, castling, en passant, and the standard `Q`/`R`/`B`/`N`
/// promotion (a pawn cannot become a king, so the king count only decreases) are the
/// trait defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CodrusRules;

impl WideVariant<Chess8x8> for CodrusRules {
    /// The tightest prefix of `WideRole::ALL` covering every fieldable role
    /// (Pawn..King, the standard army; promotions are Q/R/B/N). See
    /// [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(GIVEAWAY_START_PLACEMENT)
            .expect("the codrus starting placement is valid on an 8x8 board");
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

    // --- inverted king-extinction terminal --------------------------------

    /// A side that loses its **king** wins. Codrus watches `[King]` with
    /// `threshold = 0` in the inverted (`extinct_wins`) direction.
    fn extinction_rule() -> Option<ExtinctionRule> {
        Some(ExtinctionRule {
            watched: CODRUS_WATCHED,
            threshold: 0,
            count_total: false,
            extinct_wins: true,
        })
    }

    /// A stalemated side wins (inherited from the giveaway base).
    fn stalemate_is_win() -> bool {
        true
    }

    // Promotion is the standard `[Knight, Bishop, Rook, Queen]` default: no
    // king-promotion, so the king count only ever decreases.
}

/// Codrus as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Codrus::startpos`](GenericPosition::startpos) or parse a plain-chess FEN with
/// [`Codrus::from_fen`](GenericPosition::from_fen). Movement is the no-check
/// standard-chess set narrowed to forced captures; losing your king (or being
/// stalemated) wins.
pub type Codrus =
    GenericPosition<Chess8x8, CodrusRules, { <CodrusRules as WideVariant<Chess8x8>>::ROLE_SPAN }>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Chess8x8, WideEndReason, WideOutcome};

    #[test]
    fn startpos_fen_round_trips() {
        let pos = Codrus::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
        assert_eq!(pos.legal_move_count(), 20);
        assert_eq!(pos.end_reason(), None);
    }

    #[test]
    fn losing_the_king_wins() {
        // Rxh8 is the only capture (forced); it removes Black's king, so Black — now
        // kingless, to move — wins.
        let pos = Codrus::from_fen("R6k/8/8/8/8/8/8/7K w - - 0 1").expect("valid FEN");
        let mv = pos.parse_uci("a8h8").expect("Rxh8 legal");
        let after = pos.play(&mv);
        assert_eq!(
            after.board().pieces(Color::Black, WideRole::King).count(),
            0
        );
        assert_eq!(after.extinction_loser(), Some(Color::Black));
        assert_eq!(after.end_reason(), Some(WideEndReason::VariantWin));
        assert_eq!(
            after.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::Black
            })
        );
        assert!(after.legal_moves().is_empty(), "terminal — no moves");
    }

    #[test]
    fn pawn_cannot_promote_to_king() {
        let pos = Codrus::from_fen("8/P7/8/8/8/8/8/6kK w - - 0 1").expect("valid FEN");
        let ucis: alloc::vec::Vec<_> = pos
            .legal_moves()
            .iter()
            .map(|m| m.to_uci::<Chess8x8>())
            .collect();
        assert!(
            !ucis.iter().any(|u| u == "a7a8k"),
            "no king-promotion: {ucis:?}"
        );
    }
}
