//! Pawn back chess (8x8) on the generic engine — **standard chess with a pawn that
//! may also step backward**. Validated against Fairy-Stockfish `UCI_Variant
//! pawnback` (a built-in; the pawn is FSF Betza `fbmWfceFifmnD` with a
//! `mobilityRegion` cap and an empty `nMoveRuleTypes`).
//!
//! Pawn back chess keeps every standard chess piece, the standard starting array,
//! standard castling, and promotion to Queen / Rook / Bishop / Knight. Its one
//! difference is the pawn, which is an ordinary chess pawn **plus a backward step**:
//!
//! * **non-capturing move** — one square straight **forward** (as usual) *or* one
//!   square straight **backward** along the same file (FSF `fbmW`, a move-only
//!   forward-and-backward Wazir). The backward step is quiet only: it never
//!   captures;
//! * **capture** — one square **diagonally forward**, exactly like an ordinary pawn
//!   (FSF `fceF`); never backward, never diagonally backward;
//! * **initial double advance** — two squares straight **forward** from the second
//!   rank (FSF `ifmnD`), blocked if the intervening square is occupied; the backward
//!   move is always a single square;
//! * **mobility cap** — a pawn may never occupy its own **first rank**: a White pawn
//!   is confined to ranks 2..8 and a Black pawn to ranks 1..7 (FSF `mobilityRegion`).
//!   So a pawn on its home rank (White rank 2) cannot retreat, while a more advanced
//!   pawn may step back one rank but never past that near edge;
//! * **en passant** — standard, only off the straight forward double step; the
//!   backward step neither creates nor is subject to en passant;
//! * **promotion** — standard on the last rank, by the forward move or a diagonal
//!   capture.
//!
//! The board symbol stays `p` / `P` like an ordinary pawn — the backward step is a
//! *rule*, not a letter.
//!
//! ## The fifty-move-rule divergence
//!
//! Because a pawn can move **backward**, a pawn move is no longer irreversible, so
//! Fairy-Stockfish sets pawn back's `nMoveRuleTypes` to the empty set: **pawn moves
//! do not reset the halfmove clock** — only captures (and promotions) do. Pawn
//! shuffling therefore *can* reach the fifty-move draw, which standard chess never
//! does. This layer replicates that with
//! [`pawn_move_resets_move_clock`](WideVariant::pawn_move_resets_move_clock)
//! returning `false`; the [`move_rule_plies`](WideVariant::move_rule_plies) hook then
//! adjudicates the draw at 100 plies.
//!
//! ## How the backward step is expressed
//!
//! Three hooks over standard chess, all defaulting to the ordinary pawn so every
//! other variant is byte-identical:
//!
//! * [`pawn_moves_backward`](WideVariant::pawn_moves_backward) makes the pawn
//!   generator also emit the single backward quiet step;
//! * [`pawn_may_occupy_rank`](WideVariant::pawn_may_occupy_rank) is the mobility cap,
//!   forbidding a pawn from retreating onto its own first rank;
//! * [`pawn_move_resets_move_clock`](WideVariant::pawn_move_resets_move_clock)
//!   returning `false` keeps a pawn move from zeroing the halfmove clock.
//!
//! The pawn's **capture / attack** pattern is the ordinary diagonal-forward one, so
//! there is no `role_attacks` override — check, king-danger, and en passant all use
//! the standard pawn relation.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant pawnback`:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! ```
//!
//! From the start the pawns move exactly like standard chess (the home-rank mobility
//! cap forbids the only backward step available), so startpos perft 1 / 2 / 3 equals
//! standard chess's `20` / `400` / `8902`; the counts diverge once a pawn advances
//! and gains a legal retreat.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{Bitboard, Board, Chess8x8, Geometry, WideVariant};
use crate::Color;

/// The standard 8x8 starting placement (pawn back shares the chess array).
const PAWNBACK_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

/// The pawn back chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides only what pawn back changes about standard chess: the pawn gains a
/// backward quiet step ([`WideVariant::pawn_moves_backward`]) under a mobility cap
/// ([`WideVariant::pawn_may_occupy_rank`]), and a pawn move no longer resets the
/// halfmove clock ([`WideVariant::pawn_move_resets_move_clock`]). Every other
/// piece's movement, castling, promotion set, and the fifty-move / repetition draws
/// are the standard trait defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PawnbackRules;

impl WideVariant<Chess8x8> for PawnbackRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field: the standard army `Pawn..King` and the standard promotion
    /// targets (Knight / Bishop / Rook / Queen), all within `0..=5`. See
    /// [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(PAWNBACK_START_PLACEMENT)
            .expect("the pawn back starting placement is valid on an 8x8 board");
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
        };
        (board, state)
    }

    /// The pawn back pawn also makes a single quiet step straight **backward**
    /// (same file, toward its own side). Drives the backward branch of the generic
    /// pawn generator.
    fn pawn_moves_backward() -> bool {
        true
    }

    /// The mobility cap: a pawn may never occupy its **own first rank** — a White
    /// pawn is confined to ranks 2..8 (0-based `1..=7`) and a Black pawn to ranks
    /// 1..7 (0-based `0..=6`), matching Fairy-Stockfish's `mobilityRegion`. Only the
    /// backward step can reach that near edge, so a pawn on its home rank cannot
    /// retreat while a more advanced one may step back one rank.
    fn pawn_may_occupy_rank(color: Color, rank: u8) -> bool {
        match color {
            // White's forbidden near edge is the first rank (0-based rank 0).
            Color::White => rank != 0,
            // Black's forbidden near edge is the eighth rank (0-based top rank).
            Color::Black => rank != Chess8x8::HEIGHT - 1,
        }
    }

    /// A pawn move does **not** reset the halfmove clock in pawn back: because the
    /// pawn can also retreat, a push is no longer irreversible, so Fairy-Stockfish's
    /// `nMoveRuleTypes` is empty and only captures (and promotions) zero the clock.
    /// Pawn shuffling can therefore reach the fifty-move draw.
    fn pawn_move_resets_move_clock() -> bool {
        false
    }

    /// The western **fifty-move rule**: a position whose halfmove clock has reached
    /// 100 plies (50 full moves with no capture, promotion, or — in this variant —
    /// nothing but pawn shuffling) is a
    /// [`WideEndReason::MoveRule`](crate::geometry::WideEndReason::MoveRule) draw,
    /// matching Fairy-Stockfish's default `nMoveRule = 50` for this standard-army
    /// board. Adjudication-only (the clock never gates move generation), so perft
    /// stays byte-identical.
    fn move_rule_plies() -> Option<u16> {
        Some(100)
    }

    /// Records a position history so the standard **threefold** repetition draw
    /// ([`WideEndReason::Repetition`](crate::geometry::WideEndReason::Repetition),
    /// fold 3) fires at the [`GenericGame`](crate::geometry::game::GenericGame)
    /// level. History-dependent and never consulted by a bare
    /// [`GenericPosition`], so perft is unchanged.
    fn tracks_repetition() -> bool {
        true
    }
}

/// Pawn back chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Pawnback::startpos`](GenericPosition::startpos) or parse a plain-chess FEN with
/// [`Pawnback::from_fen`](GenericPosition::from_fen). Movement is standard chess with
/// a pawn that may also step one square straight backward.
pub type Pawnback = GenericPosition<Chess8x8, PawnbackRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Square, WideMoveKind, WideRole};

    fn sq(file: u8, rank: u8) -> Square<Chess8x8> {
        Square::<Chess8x8>::from_file_rank(file, rank).unwrap()
    }

    /// The canonical start FEN round-trips with standard castling rights, and the
    /// home-rank mobility cap forbids every backward step, so the first-ply count is
    /// the standard-chess `20`.
    #[test]
    fn startpos_fen_and_move_count() {
        let pos = Pawnback::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
        assert_eq!(pos.turn(), Color::White);
        // No pawn can retreat off rank 2, so the count matches standard chess.
        assert_eq!(pos.legal_move_count(), 20);
    }

    /// An advanced pawn generates the empty square directly **behind** it as a quiet
    /// (non-capturing) backward move, alongside its forward push.
    #[test]
    fn advanced_pawn_steps_backward() {
        // White pawn d4 (advanced past its home rank): it may push to d5 and retreat
        // to d3.
        let pos = Pawnback::from_fen("4k3/8/8/8/3P4/8/8/4K3 w - - 0 1").expect("valid FEN");
        let d4 = sq(3, 3);
        let moves: alloc::vec::Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Chess8x8>() == d4)
            .collect();
        assert!(
            moves
                .iter()
                .any(|m| m.to::<Chess8x8>() == sq(3, 4) && !m.is_capture()),
            "forward push to d5",
        );
        assert!(
            moves
                .iter()
                .any(|m| m.to::<Chess8x8>() == sq(3, 2) && matches!(m.kind(), WideMoveKind::Quiet)),
            "backward quiet step to d3",
        );
        assert_eq!(moves.len(), 2, "exactly d5 and d3");
    }

    /// A pawn on its home rank cannot step backward — the mobility cap forbids a
    /// White pawn from stepping onto rank 1.
    #[test]
    fn home_rank_pawn_cannot_retreat() {
        // White pawn e2 on its start rank: forward push e3, double step e4, but no
        // retreat onto e1.
        let pos = Pawnback::from_fen("4k3/8/8/8/8/8/4P3/K7 w - - 0 1").expect("valid FEN");
        let e2 = sq(4, 1);
        let moves: alloc::vec::Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Chess8x8>() == e2)
            .collect();
        assert!(
            !moves.iter().any(|m| m.to::<Chess8x8>() == sq(4, 0)),
            "no backward step onto rank 1 (mobility cap)",
        );
        assert!(
            moves.iter().any(|m| m.to::<Chess8x8>() == sq(4, 2)),
            "forward push to e3",
        );
        assert!(
            moves.iter().any(|m| m.to::<Chess8x8>() == sq(4, 3)
                && matches!(m.kind(), WideMoveKind::DoublePawnPush)),
            "double step to e4",
        );
        assert_eq!(moves.len(), 2, "exactly e3 and e4 — no retreat");
    }

    /// A forward or backward pawn move does **not** reset the halfmove clock (the
    /// clock increments), unlike standard chess where a pawn push zeroes it.
    #[test]
    fn pawn_move_does_not_reset_clock() {
        let pos = Pawnback::from_fen("4k3/8/8/8/3P4/8/8/4K3 w - - 7 20").expect("valid FEN");
        // Forward push d4-d5: the clock advances rather than zeroing.
        let push = pos
            .legal_moves()
            .into_iter()
            .find(|m| m.from::<Chess8x8>() == sq(3, 3) && m.to::<Chess8x8>() == sq(3, 4))
            .expect("d4-d5 is legal");
        assert_eq!(
            pos.play(&push).halfmove_clock(),
            8,
            "forward push increments the clock"
        );
        // Backward step d4-d3: likewise no reset.
        let back = pos
            .legal_moves()
            .into_iter()
            .find(|m| m.from::<Chess8x8>() == sq(3, 3) && m.to::<Chess8x8>() == sq(3, 2))
            .expect("d4-d3 is legal");
        assert_eq!(
            pos.play(&back).halfmove_clock(),
            8,
            "backward step increments the clock"
        );
    }

    /// A capture still resets the halfmove clock — only pawn *moves* are exempt.
    #[test]
    fn capture_still_resets_clock() {
        // White pawn d4 captures a black knight on e5 (diagonal-forward capture).
        let pos = Pawnback::from_fen("4k3/8/8/4n3/3P4/8/8/4K3 w - - 9 30").expect("valid FEN");
        let cap = pos
            .legal_moves()
            .into_iter()
            .find(|m| {
                m.from::<Chess8x8>() == sq(3, 3) && m.to::<Chess8x8>() == sq(4, 4) && m.is_capture()
            })
            .expect("dxe5 is legal");
        assert_eq!(
            pos.play(&cap).halfmove_clock(),
            0,
            "a capture zeroes the clock"
        );
    }

    /// The forward double step still creates a standard en-passant target that an
    /// enemy pawn captures diagonally.
    #[test]
    fn double_step_and_en_passant() {
        // White pawn e2 double-steps to e4; a Black pawn on d4 takes en passant.
        let pos = Pawnback::from_fen("4k3/8/8/8/3p4/8/4P3/4K3 w - - 0 1").expect("valid FEN");
        let dbl = pos
            .legal_moves()
            .into_iter()
            .find(|m| m.from::<Chess8x8>() == sq(4, 1) && m.to::<Chess8x8>() == sq(4, 3))
            .expect("e2-e4 double step is legal");
        let after = pos.play(&dbl);
        assert_eq!(
            after.ep_square(),
            Some(sq(4, 2)),
            "ep target is the skipped e3"
        );
        let ep = after
            .legal_moves()
            .into_iter()
            .find(|m| {
                m.from::<Chess8x8>() == sq(3, 3)
                    && m.to::<Chess8x8>() == sq(4, 2)
                    && matches!(m.kind(), WideMoveKind::EnPassant)
            })
            .expect("d4xe3 en passant is legal");
        let done = after.play(&ep);
        assert_eq!(
            done.board().pieces(Color::White, WideRole::Pawn).count(),
            0,
            "the en-passant capture removed White's pawn on e4",
        );
    }
}
