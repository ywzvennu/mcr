//! Torpedo chess (8x8) on the generic engine — **standard chess in which a pawn
//! may make its two-square advance from ANY rank**, not only from its starting
//! rank. Every other rule is standard chess, unchanged.
//!
//! Torpedo is the reference [`StandardChess`] ruleset over [`Chess8x8`] with the
//! single pawn double-step rule relaxed: a pawn double-steps whenever both squares
//! ahead of it are empty, wherever it stands. Single pushes, diagonal captures,
//! **en passant**, promotion, castling, and checkmate are all the standard-chess
//! trait defaults.
//!
//! ## Rules — standard chess with an unrestricted pawn double-step
//!
//! * **Double-step from any rank.** The one override:
//!   [`pawn_may_double_push_from`](WideVariant::pawn_may_double_push_from) returns
//!   `true` for every rank, so a pawn on (say) the fourth rank may leap to the
//!   sixth when the fifth and sixth are both empty. The generic pawn generator
//!   still requires both squares ahead to be vacant, exactly as for the standard
//!   starting-rank double-step.
//! * **En passant off any double-step.** A double-step from any rank sets the
//!   en-passant target on the intermediate square (the origin shifted one rank
//!   forward), and an enemy pawn may capture en passant onto it, capturing the
//!   pawn that leapt — the standard machinery derives both squares from the move's
//!   origin, so it is correct for a mid-board leap without any change.
//! * **Everything else is standard chess.** Pawns single-push and capture
//!   diagonally, promote on the far rank to Queen / Rook / Bishop / Knight, both
//!   sides castle, and the game is **won by checkmate** with the standard 50-move
//!   rule.
//!
//! No draw hook is overridden: like the reference [`StandardChess`], torpedo carries
//! the trait-default terminal rules, matching Fairy-Stockfish's `torpedo` for perft.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant torpedo` (FSF's
//! `chess_variant_base()` with `doubleStepRegion` set to all squares for both
//! colours). The array, castling field, and every other field are the standard
//! chess start:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
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

/// The confirmed torpedo starting placement: the standard chess array.
const TORPEDO_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

/// The torpedo chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It is the reference [`StandardChess`] ruleset with exactly one change — a pawn
/// may make its two-square advance from **any** rank (not only its starting rank),
/// through
/// [`pawn_may_double_push_from`](WideVariant::pawn_may_double_push_from). Every
/// other rule — single pushes, diagonal captures, en passant, promotion, castling,
/// and checkmate — is the standard-chess trait default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TorpedoRules;

impl WideVariant<Chess8x8> for TorpedoRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field (Pawn..King, the standard army; promotions are Queen /
    /// Rook / Bishop / Knight, all within the prefix). See
    /// [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(TORPEDO_START_PLACEMENT)
            .expect("the standard starting placement is valid on an 8x8 board");
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
            board_b: crate::geometry::Bitboard::EMPTY,
            petrified: crate::geometry::Bitboard::EMPTY,
            checks_against: [0, 0],
        };
        (board, state)
    }

    /// Torpedo's single rule change: a pawn may make its two-square advance from
    /// **any** rank. The generic pawn generator still requires both squares ahead
    /// to be empty, and the en-passant target it derives from the move's origin is
    /// correct for a double-step made from any rank.
    fn pawn_may_double_push_from(_rank: u8, _color: Color) -> bool {
        true
    }
}

/// Torpedo chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Torpedo::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Torpedo::from_fen`](GenericPosition::from_fen). Every rule is the standard
/// [`StandardChess`] default except the pawn double-step, which is available from
/// any rank.
pub type Torpedo =
    GenericPosition<Chess8x8, TorpedoRules, { <TorpedoRules as WideVariant<Chess8x8>>::ROLE_SPAN }>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{WideMove, WideMoveKind, WideRole};

    // The generic `Square` renders as `(file,rank)` zero-based coordinates, so these
    // tests identify a move by its `(from, to)` file/rank rather than by an algebraic
    // string. Files/ranks are 0-based: e = file 4, and rank N is index N-1.
    fn is_move(m: &WideMove, ff: u8, fr: u8, tf: u8, tr: u8) -> bool {
        let from = m.from::<Chess8x8>();
        let to = m.to::<Chess8x8>();
        from.file() == ff && from.rank() == fr && to.file() == tf && to.rank() == tr
    }

    /// The canonical start FEN round-trips and carries the standard castling rights.
    #[test]
    fn startpos_fen_round_trips() {
        let pos = Torpedo::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
        assert_eq!(pos.turn(), Color::White);
        // Startpos: identical to standard chess (every pawn is on its start rank).
        assert_eq!(pos.legal_move_count(), 20);
    }

    /// A pawn on a **non-starting** rank may double-step when the two squares ahead
    /// are empty — the torpedo rule. Standard chess would emit only the single push.
    #[test]
    fn pawn_double_steps_from_non_starting_rank() {
        // White pawn on e4 (not its start rank), e5 and e6 empty.
        let pos = Torpedo::from_fen("4k3/8/8/8/4P3/8/8/4K3 w - - 0 1").expect("valid FEN");
        // e4 = (4, 3) -> e6 = (4, 5).
        let dbl = pos
            .legal_moves()
            .into_iter()
            .find(|m| matches!(m.kind(), WideMoveKind::DoublePawnPush) && is_move(m, 4, 3, 4, 5));
        assert!(
            dbl.is_some(),
            "a torpedo pawn on e4 double-steps to e6 (e5/e6 empty)"
        );
        // Playing it sets the en-passant target on the intermediate square e5 = (4, 4).
        let next = pos.play(&dbl.unwrap());
        let ep = next.ep_square().expect("a double push sets an ep target");
        assert_eq!(
            (ep.file(), ep.rank()),
            (4, 4),
            "the ep target of an e4->e6 leap is the intermediate square e5",
        );
    }

    /// A blocked path forbids the mid-board double-step: both squares ahead must be
    /// empty, exactly as for the starting-rank double-step.
    #[test]
    fn mid_board_double_step_needs_both_squares_empty() {
        // A black pawn sits on e6, blocking the far square of a white e4->e6 leap.
        let pos = Torpedo::from_fen("4k3/8/4p3/8/4P3/8/8/4K3 w - - 0 1").expect("valid FEN");
        let leap = pos
            .legal_moves()
            .into_iter()
            .any(|m| matches!(m.kind(), WideMoveKind::DoublePawnPush) && is_move(&m, 4, 3, 4, 5));
        assert!(
            !leap,
            "the double-step is blocked when the far square is occupied"
        );
    }

    /// The opponent may capture **en passant** a pawn that double-stepped from a
    /// mid-board rank, onto the intermediate square, removing the leaping pawn.
    #[test]
    fn en_passant_off_a_mid_board_double_step() {
        // White pawn e4, black pawn d6, black to recapture after the leap.
        let pos = Torpedo::from_fen("4k3/8/3p4/8/4P3/8/8/4K3 w - - 0 1").expect("valid FEN");
        // e4 = (4, 3) -> e6 = (4, 5).
        let leap = pos
            .legal_moves()
            .into_iter()
            .find(|m| matches!(m.kind(), WideMoveKind::DoublePawnPush) && is_move(m, 4, 3, 4, 5))
            .expect("white leaps e4->e6");
        let after = pos.play(&leap);
        let ep_sq = after.ep_square().expect("the leap sets an ep target");
        assert_eq!((ep_sq.file(), ep_sq.rank()), (4, 4), "ep target e5");

        // Black's d6 = (3, 5) pawn captures en passant onto e5 = (4, 4).
        let ep = after
            .legal_moves()
            .into_iter()
            .find(|m| matches!(m.kind(), WideMoveKind::EnPassant) && is_move(m, 3, 5, 4, 4))
            .expect("black captures the leaping pawn en passant d6xe5");
        let done = after.play(&ep);
        // The white pawn that leapt to e6 is gone; a black pawn now stands on e5.
        assert_eq!(
            done.to_fen(),
            "4k3/8/8/4p3/8/8/8/4K3 w - - 0 2",
            "en passant off a mid-board double-step removes the leaping pawn",
        );
    }

    /// A torpedo double-step that lands on the **promotion rank** promotes: it emits
    /// one move per promotion role (Q/R/B/N) and **no** `DoublePawnPush`, exactly as
    /// Fairy-Stockfish does. A white pawn on e6 with e7/e8 empty may leap to e8.
    #[test]
    fn double_step_onto_promotion_rank_promotes() {
        let pos = Torpedo::from_fen("5k2/8/4P3/8/8/8/8/4K3 w - - 0 1").expect("valid FEN");
        // e6 = (4, 5) -> e8 = (4, 7), the promotion rank.
        let count = pos
            .legal_moves()
            .into_iter()
            .filter(|m| is_move(m, 4, 5, 4, 7))
            .count();
        assert_eq!(
            count, 4,
            "a double-step onto the last rank promotes to four roles, not a double push",
        );
        assert!(
            pos.legal_moves()
                .into_iter()
                .filter(|m| is_move(m, 4, 5, 4, 7))
                .all(|m| m.promotion().is_some()),
            "every e6->e8 leap is a promotion, never a plain DoublePawnPush",
        );
        // Playing one promotion leaves no en-passant target (the pawn is gone).
        let q = pos
            .legal_moves()
            .into_iter()
            .find(|m| is_move(m, 4, 5, 4, 7) && m.promotion() == Some(WideRole::Queen))
            .expect("a queen promotion exists");
        let next = pos.play(&q);
        assert!(
            next.ep_square().is_none(),
            "a promoting double-step sets no ep target",
        );
    }
}
