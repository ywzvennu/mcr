//! Pawn-sideways chess (8x8) on the generic engine — **standard chess with a pawn
//! that may also step sideways**. Validated against Fairy-Stockfish
//! `UCI_Variant pawnsideways` (a built-in; its pawn is FSF Betza `fsmWfceFifmnD`).
//!
//! Pawn-sideways chess keeps every standard chess piece, the standard starting
//! array, standard castling, and promotion to Queen / Rook / Bishop / Knight. Its
//! one difference is the pawn, which is an **ordinary chess pawn plus one extra
//! quiet move**: a single step **sideways** (left or right) along its own rank onto
//! an empty square. Everything else about the pawn is standard:
//!
//! * **non-capturing move** — one square straight forward (as usual) **or** one
//!   square sideways onto an empty square (the extra rule);
//! * **capture** — one square diagonally forward, exactly like an ordinary pawn;
//! * **initial double step** — two squares straight forward from the second rank,
//!   blocked if the intervening square is occupied (standard, forward only — never
//!   sideways);
//! * **en passant** — standard, applying only to the straight forward double step;
//!   a sideways step never creates or is subject to en passant;
//! * **promotion** — standard, on reaching the last rank by a forward move or a
//!   diagonal capture. A sideways step stays on the same rank, so it can never
//!   promote.
//!
//! The board symbol stays `p` / `P` like an ordinary pawn — the sideways step is a
//! *rule*, not a letter.
//!
//! ## How the sideways step is expressed
//!
//! A single hook over standard chess, defaulting to the ordinary pawn so every
//! other variant is byte-identical:
//!
//! * [`pawn_moves_sideways`](WideVariant::pawn_moves_sideways) makes the standard
//!   single-king pawn generator **also** emit the two sideways quiet steps (same
//!   rank, file ±1) whenever the target square is empty. Each is filtered by the
//!   same check mask and pin line as the forward push, so pins are handled for
//!   free (a pinned pawn may step sideways only if the target stays on its pin
//!   line, which a same-rank step almost never satisfies). The forward push,
//!   forward double step, diagonal capture, en passant, and promotion are all the
//!   unchanged standard pawn logic.
//!
//! The sideways step is deliberately **not** part of
//! [`role_attacks`](WideVariant::role_attacks) — the pawn's attack (and therefore
//! its check and king-danger threat) stays the two forward diagonals — so a
//! sideways move gives no check, and because it is a plain
//! [`WideMoveKind::Quiet`](super::super::WideMoveKind::Quiet) it creates no
//! en-passant target.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant pawnsideways`:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! ```
//!
//! At the start the count matches standard chess (perft 1 = `20`): every rank-2
//! pawn is flanked by another pawn (or the board edge), so no sideways step is yet
//! available — the extra moves surface only once pawns have advanced (startpos
//! perft 3 = `10022` versus standard chess's `8902`).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{Bitboard, Board, Chess8x8, WideVariant};
use crate::Color;

/// The standard 8x8 starting placement (pawn-sideways shares the chess array).
const PAWNSIDEWAYS_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

/// The pawn-sideways chess rule layer: a zero-sized [`WideVariant`] over
/// [`Chess8x8`].
///
/// It overrides only what pawn-sideways changes about standard chess: the Pawn may
/// take an extra sideways quiet step ([`WideVariant::pawn_moves_sideways`]). Every
/// other piece's movement, the pawn's forward push / double step / diagonal capture
/// / en passant / promotion, castling, the promotion set, and the 50-move rule are
/// the standard trait defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PawnsidewaysRules;

impl WideVariant<Chess8x8> for PawnsidewaysRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field: the standard army `Pawn..King` and the standard promotion
    /// targets (Knight / Bishop / Rook / Queen), all within `0..=5`. See
    /// [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(PAWNSIDEWAYS_START_PLACEMENT)
            .expect("the pawn-sideways starting placement is valid on an 8x8 board");
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

    /// The Pawn (`WideRole::Pawn`) may take an extra **sideways** quiet step (one
    /// square left or right along its own rank onto an empty square) in addition to
    /// the ordinary forward moves and diagonal captures. Drives the sideways branch
    /// of the generic pawn generator.
    fn pawn_moves_sideways() -> bool {
        true
    }

    /// The western **fifty-move rule**: a position whose halfmove clock has reached
    /// 100 plies (50 full moves with no capture or pawn move) is a
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

/// Pawn-sideways chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Pawnsideways::startpos`](GenericPosition::startpos) or parse a plain-chess FEN
/// with [`Pawnsideways::from_fen`](GenericPosition::from_fen). Movement is standard
/// chess with the extra sideways pawn step.
pub type Pawnsideways = GenericPosition<Chess8x8, PawnsidewaysRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Square, WideMoveKind, WideRole};

    fn sq(file: u8, rank: u8) -> Square<Chess8x8> {
        Square::<Chess8x8>::from_file_rank(file, rank).unwrap()
    }

    /// The canonical start FEN round-trips with standard castling rights, and the
    /// first-ply count equals standard chess (no pawn can step sideways yet).
    #[test]
    fn startpos_fen_and_move_count() {
        let pos = Pawnsideways::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
        assert_eq!(pos.turn(), Color::White);
        // The startpos count equals standard chess's 20: every rank-2 pawn is
        // flanked by another pawn (or the board edge), so none can step sideways
        // yet — the extra moves only appear once pawns have advanced. This matches
        // Fairy-Stockfish's pawnsideways startpos perft 1.
        assert_eq!(pos.legal_move_count(), 20);
    }

    /// A sideways quiet step is generated onto an empty adjacent-file same-rank
    /// square and is NOT generated onto an occupied one; it is never a capture and
    /// stays on the same rank.
    #[test]
    fn sideways_onto_empty_not_occupied() {
        // White pawn d4. c4 empty, e4 holds a Black knight. The pawn may step
        // sideways to c4 (empty) but not onto e4 (occupied — sideways never
        // captures), and it still pushes forward to d5.
        let pos = Pawnsideways::from_fen("4k3/8/8/8/3Pn3/8/8/4K3 w - - 0 1").expect("valid FEN");
        let d4 = sq(3, 3);
        let moves: alloc::vec::Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Chess8x8>() == d4)
            .collect();
        assert!(
            moves
                .iter()
                .any(|m| m.to::<Chess8x8>() == sq(2, 3) && !m.is_capture()),
            "sideways quiet step onto empty c4",
        );
        assert!(
            !moves.iter().any(|m| m.to::<Chess8x8>() == sq(4, 3)),
            "no sideways step onto the occupied e4 (sideways never captures)",
        );
        // The forward push to d5 is still there, and the pawn may capture the
        // knight only if it were on a forward diagonal — here it is not, so exactly
        // the sideways c4 and the forward d5 remain.
        assert!(
            moves
                .iter()
                .any(|m| m.to::<Chess8x8>() == sq(3, 4) && !m.is_capture()),
            "ordinary forward push to d5",
        );
        assert_eq!(moves.len(), 2, "exactly c4 and d5");
    }

    /// A sideways step gives no check and creates no en-passant target, even when
    /// it lands next to the enemy king or beside an enemy pawn.
    #[test]
    fn sideways_gives_no_check_and_no_ep() {
        // White pawn e5 steps sideways to d5, landing directly in front of the
        // Black king on d6 — an ordinary pawn's forward move there would be no
        // check either, but crucially the sideways landing is a plain quiet move.
        let pos = Pawnsideways::from_fen("8/8/3k4/4P3/8/8/8/4K3 w - - 0 1").expect("valid FEN");
        let step = pos
            .legal_moves()
            .into_iter()
            .find(|m| m.from::<Chess8x8>() == sq(4, 4) && m.to::<Chess8x8>() == sq(3, 4))
            .expect("e5-d5 sideways step is legal");
        assert!(
            matches!(step.kind(), WideMoveKind::Quiet),
            "the sideways step is a plain quiet move",
        );
        let after = pos.play(&step);
        assert!(!after.is_check(), "a sideways step gives no check");
        assert_eq!(
            after.ep_square(),
            None,
            "a sideways step creates no ep target"
        );
    }

    /// The ordinary forward double step still sets a standard en-passant target and
    /// is capturable en passant — the sideways rule leaves it untouched.
    #[test]
    fn forward_double_step_and_en_passant_still_work() {
        // White pawn e2 double-steps to e4 beside a Black pawn on d4; Black then
        // takes en passant onto e3.
        let pos = Pawnsideways::from_fen("4k3/8/8/8/3p4/8/4P3/4K3 w - - 0 1").expect("valid FEN");
        let e2e4 = pos
            .legal_moves()
            .into_iter()
            .find(|m| {
                m.from::<Chess8x8>() == sq(4, 1)
                    && m.to::<Chess8x8>() == sq(4, 3)
                    && matches!(m.kind(), WideMoveKind::DoublePawnPush)
            })
            .expect("e2-e4 forward double step is legal");
        let after = pos.play(&e2e4);
        assert_eq!(
            after.ep_square(),
            Some(sq(4, 2)),
            "standard ep target on e3"
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

    /// A pawn pinned to its king along a file may not step sideways off the pin
    /// line (the sideways target leaves the pin ray), so no sideways move appears.
    #[test]
    fn pinned_pawn_cannot_step_sideways_off_the_pin() {
        // White king e1, White pawn e2, Black rook e8: the pawn is pinned along the
        // e-file. It may push forward along the file but neither sideways step
        // (d2 / f2) stays on the pin line, so none is generated.
        let pos = Pawnsideways::from_fen("4r3/8/8/8/8/8/4P3/4K3 w - - 0 1").expect("valid FEN");
        let e2 = sq(4, 1);
        let moves: alloc::vec::Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Chess8x8>() == e2)
            .collect();
        assert!(
            moves.iter().all(|m| m.to::<Chess8x8>().file() == 4),
            "a file-pinned pawn makes only forward moves, never a sideways one",
        );
    }
}
