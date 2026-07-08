//! Giveaway chess (8x8) on the generic engine — **antichess with castling: the
//! goal is to *lose* every piece, captures are forced, and the king is an ordinary
//! non-royal piece**. Validated against Fairy-Stockfish `UCI_Variant giveaway` (a
//! built-in; `giveaway_variant()`, `variant.cpp:402`).
//!
//! Giveaway chess keeps standard chess movement — the same sliders, leapers, en
//! passant, and (unlike lichess antichess) **ordinary castling** — but almost every
//! terminal concept is inverted or removed:
//!
//! * **The king is a non-royal Commoner.** It can be captured, it gives no check,
//!   and it may be left en prise. There is no check and no checkmate.
//! * **Captures are forced** ([`WideVariant::mandatory_captures`]): if the side to
//!   move has any capture available, every one of its legal moves must be a capture.
//! * **Pawns may promote to a king** as well as to Q/R/B/N.
//! * **Losing wins.** A side reduced to **zero pieces**, or left with **no legal
//!   move** (stalemate), **wins** — the inverse of standard chess. Giveaway watches
//!   the side's *whole army* (FSF `extinctionPieceTypes = ALL_PIECES`,
//!   `extinctionValue = +VALUE_MATE`) with `threshold = 0`, and stalemate is a win
//!   for the stalemated side ([`WideVariant::stalemate_is_win`]).
//!
//! ## The inverted extinction terminal
//!
//! The win-by-extinction reuses the generic [`WideVariant::extinction_rule`] hook
//! with its two antichess dials set: [`count_total`](crate::geometry::ExtinctionRule::count_total)
//! watches the total piece count (not any one role), and
//! [`extinct_wins`](crate::geometry::ExtinctionRule::extinct_wins) credits the win
//! to the extinct side. The node still truncates to zero moves at the terminal,
//! exactly as Fairy-Stockfish adjudicates it.
//!
//! ## King handling — a non-royal Commoner
//!
//! The non-royal king reuses the machinery Extinction / Three-kings introduced: an
//! empty [`royal_squares`](WideVariant::royal_squares) makes the king-safety code
//! report "never in check", and [`non_royal_king`](WideVariant::non_royal_king)
//! routes the generator through its non-royal branch. Castling stays enabled and is
//! never restricted by attacked squares; a promoted second king is handled by the
//! same multi-king movegen Extinction uses.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant giveaway` — the standard array
//! **with** castling rights (giveaway keeps castling):
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! ```
//!
//! [`ExtinctionRule`]: crate::geometry::ExtinctionRule

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    Bitboard, Board, Chess8x8, ExtinctionRule, PromotionConfig, WideRole, WideVariant,
};
use crate::Color;

/// The standard 8x8 starting placement (giveaway shares the chess array).
pub(super) const GIVEAWAY_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

/// The Giveaway chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// Overrides only what giveaway changes about standard chess: a non-royal Commoner
/// king (like Extinction), mandatory captures, king-promotion, and the inverted
/// "losing wins" terminal (whole-army extinction plus stalemate-as-win). Movement,
/// castling, and en passant are the standard trait defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GiveawayRules;

impl WideVariant<Chess8x8> for GiveawayRules {
    /// The tightest prefix of `WideRole::ALL` covering every fieldable role
    /// (Pawn..King; a pawn may promote to the king, still within the prefix). See
    /// [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(GIVEAWAY_START_PLACEMENT)
            .expect("the giveaway starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            // Giveaway keeps ordinary castling (the non-royal king is never
            // restricted by attacked squares).
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

    // --- inverted "losing wins" terminal ----------------------------------

    /// A side that loses its **whole army** wins. Giveaway watches the total piece
    /// count with `threshold = 0` in the inverted (`extinct_wins`) direction (FSF
    /// `extinctionPieceTypes = ALL_PIECES`, `extinctionValue = +VALUE_MATE`).
    fn extinction_rule() -> Option<ExtinctionRule> {
        Some(ExtinctionRule {
            watched: &[],
            threshold: 0,
            count_total: true,
            extinct_wins: true,
            opponent_min: 0,
        })
    }

    /// A stalemated side (no capture forced, no other legal move) **wins** (FSF
    /// `stalemateValue = +VALUE_MATE`).
    fn stalemate_is_win() -> bool {
        true
    }

    // --- promotion (adds the Commoner king) -------------------------------

    /// A pawn may promote to Knight / Bishop / Rook / Queen **or the king
    /// (Commoner)** — the role order matches FSF's generation.
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

/// Giveaway chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Giveaway::startpos`](GenericPosition::startpos) or parse a plain-chess FEN
/// with [`Giveaway::from_fen`](GenericPosition::from_fen). Movement is the no-check
/// standard-chess set narrowed to forced captures; a side that loses all its pieces
/// or is stalemated wins.
pub type Giveaway = GenericPosition<
    Chess8x8,
    GiveawayRules,
    { <GiveawayRules as WideVariant<Chess8x8>>::ROLE_SPAN },
>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Chess8x8, Square, WideEndReason, WideOutcome};
    use alloc::string::ToString;

    #[test]
    fn startpos_fen_round_trips_with_castling() {
        let pos = Giveaway::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
        assert_eq!(pos.turn(), Color::White);
        assert_eq!(pos.legal_move_count(), 20);
        assert_eq!(pos.end_reason(), None);
        assert_eq!(pos.outcome(), None);
    }

    #[test]
    fn king_is_non_royal_no_check() {
        // A black rook plainly "attacks" the white king; in giveaway this is no
        // check and imposes no constraint.
        let pos = Giveaway::from_fen("8/8/8/8/8/8/8/r3K3 w - - 0 1").expect("valid FEN");
        assert!(!pos.is_check());
        // No capture is available, so the king may step onto an attacked square.
        let sq = |f, r| Square::<Chess8x8>::from_file_rank(f, r).unwrap();
        assert!(pos
            .legal_moves()
            .iter()
            .any(|m| m.from::<Chess8x8>() == sq(4, 0) && m.to::<Chess8x8>() == sq(3, 1)));
    }

    #[test]
    fn captures_are_forced() {
        // White pawn e4, black pawn d5: exd5 is the only capture, so it is forced.
        let pos = Giveaway::from_fen("8/8/8/3p4/4P3/8/8/K6k w - - 0 1").expect("valid FEN");
        let moves = pos.legal_moves();
        assert!(
            moves.iter().all(|m| m.is_capture()),
            "only captures survive"
        );
        assert_eq!(moves.len(), 1, "exactly the exd5 capture");
    }

    #[test]
    fn zero_pieces_is_a_win_for_that_side() {
        // Black (to move) has no pieces at all — it wins.
        let pos = Giveaway::from_fen("8/8/8/8/8/8/8/K7 b - - 0 1").expect("valid FEN");
        assert!(pos.legal_moves().is_empty(), "terminal — no moves");
        assert_eq!(pos.end_reason(), Some(WideEndReason::VariantWin));
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::Black
            })
        );
    }

    #[test]
    fn no_legal_move_is_a_win_for_side_to_move() {
        // White's only piece, a pawn on a2, is head-to-head blocked by a black pawn
        // on a3: no push, no (diagonal) capture — no legal move, a WIN for White.
        let pos = Giveaway::from_fen("8/8/8/8/8/p7/P7/8 w - - 0 1").expect("valid FEN");
        assert!(pos.legal_moves().is_empty(), "white is immobilised");
        assert_eq!(pos.end_reason(), Some(WideEndReason::Stalemate));
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::White
            })
        );
    }

    #[test]
    fn king_capture_then_win() {
        // Rxh8 is the only capture (forced); it removes Black's last piece, so Black
        // (now with zero pieces, to move) wins.
        let pos = Giveaway::from_fen("R6k/8/8/8/8/8/8/7K w - - 0 1").expect("valid FEN");
        let mv = pos.parse_uci("a8h8").expect("Rxh8 legal");
        let after = pos.play(&mv);
        assert!(after.board().by_color(Color::Black).is_empty());
        assert_eq!(
            after.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::Black
            })
        );
        assert_eq!(after.end_reason(), Some(WideEndReason::VariantWin));
    }

    #[test]
    fn pawn_may_promote_to_king() {
        let pos = Giveaway::from_fen("8/P7/8/8/8/8/8/7k w - - 0 1").expect("valid FEN");
        let ucis: alloc::vec::Vec<_> = pos
            .legal_moves()
            .iter()
            .map(|m| m.to_uci::<Chess8x8>())
            .collect();
        for promo in ["a7a8n", "a7a8b", "a7a8r", "a7a8q", "a7a8k"] {
            assert!(
                ucis.contains(&promo.to_string()),
                "missing {promo}: {ucis:?}"
            );
        }
    }

    #[test]
    fn start_perft_matches_fsf() {
        // FSF `UCI_Variant giveaway` startpos.
        let pos = Giveaway::startpos();
        assert_eq!(crate::geometry::perft(&pos, 3), 8067);
    }
}
