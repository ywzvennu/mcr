//! Gothic Chess (10x8) on the generic engine — a Capablanca-board variant with a
//! different back-rank order (Ed Trice's Gothic Chess). Validated
//! square-for-square against Fairy-Stockfish `UCI_Variant gothic`.
//!
//! Gothic is played on the same ten-files by eight-ranks board as Capablanca and
//! adds the same two compound pieces, so it reuses the [`Cap10x8`] geometry, the
//! compound-piece defaults, and Capablanca's castle geometry wholesale — only the
//! opening array differs:
//!
//! * **Chancellor** (Rook + Knight) — mcr's [`WideRole::Elephant`], FEN letter
//!   `e`/`E` (FSF spells it `c`/`C`; the `compare-fairy/` harness reconciles).
//! * **Archbishop** (Bishop + Knight) — mcr's [`WideRole::Hawk`], FEN letter
//!   `a`/`A` in both mcr and FSF.
//!
//! Every other rule is standard chess: pawns push one (or two from their second
//! rank), capture diagonally, take en passant, and promote on the last rank to
//! Queen, Rook, Bishop, Knight, Archbishop, or Chancellor. The king and rooks
//! castle on the Capablanca files (king on the f-file, rooks on the a/j files).
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant gothic` (`position startpos`):
//!
//! ```text
//! FSF dialect: rnbqckabnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNBQCKABNR w KQkq - 0 1
//! mcr dialect: rnbqekabnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNBQEKABNR w KQkq - 0 1
//! ```
//!
//! The two differ only in the chancellor's letter (`c` in FSF, `e` in mcr). Back
//! rank a-file to j-file: Rook, Knight, Bishop, Queen, Chancellor, King,
//! Archbishop, Bishop, Knight, Rook. The king stands on the f-file (file 5); the
//! castling rooks are the a-file and j-file rooks.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{Board, Cap10x8, PromotionConfig, WideRole, WideVariant};
use crate::Color;

/// The confirmed Gothic starting placement in the mcr dialect (chancellor =
/// `e`/`E`), byte-for-byte equivalent to Fairy-Stockfish's
/// `rnbqckabnr/.../RNBQCKABNR` modulo the chancellor's letter.
const GOTHIC_START_PLACEMENT: &str = "rnbqekabnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNBQEKABNR";

/// The kingside castle side index, matching the position layer's `KINGSIDE`.
const KINGSIDE: usize = 0;

/// The Gothic Chess rule layer: a zero-sized [`WideVariant`] over [`Cap10x8`].
///
/// It overrides only the 10x8 starting array, the wider promotion set (adding
/// Archbishop and Chancellor), and the Capablanca castle destination files. The
/// Archbishop ([`WideRole::Hawk`]) and Chancellor ([`WideRole::Elephant`])
/// movement is already the trait default, so no `role_attacks` override is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GothicRules;

impl WideVariant<Cap10x8> for GothicRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 12;

    /// The western **fifty-move rule**: a position whose halfmove clock has
    /// reached 100 plies (50 full moves with no capture or pawn move) is a
    /// [`WideEndReason::MoveRule`](crate::geometry::WideEndReason::MoveRule) draw,
    /// matching Fairy-Stockfish's default `nMoveRule = 50` for this standard-army
    /// large board. Adjudication-only (the clock never gates move generation), so
    /// perft stays byte-identical.
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

    fn starting_position() -> (Board<Cap10x8>, GenericState<Cap10x8>) {
        let board = Board::<Cap10x8>::from_fen_placement(GOTHIC_START_PLACEMENT)
            .expect("the Gothic starting placement is valid on a 10x8 board");
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::standard::<Cap10x8>(),
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

    fn promotion_config() -> PromotionConfig {
        // The six-role Capablanca-family promotion set (FSF order c a q r b n).
        // Order affects only move enumeration, not the perft leaf count.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Elephant, // Chancellor (R+N)
                WideRole::Hawk,     // Archbishop (B+N)
                WideRole::Queen,
                WideRole::Rook,
                WideRole::Bishop,
                WideRole::Knight,
            ],
        }
    }

    fn castle_dest_files(side: usize) -> (u8, u8) {
        // Capablanca castling: king f1 -> i1 (file 8) / c1 (file 2), rook beside it.
        if side == KINGSIDE {
            (8, 7)
        } else {
            (2, 3)
        }
    }

    /// Gothic keeps the standard chess army plus the always-mating Archbishop
    /// ([`WideRole::Hawk`]) and Chancellor ([`WideRole::Elephant`]), so the ordinary
    /// insufficient-material draw applies. Adjudication-only and behind the
    /// default-off hook, so perft stays byte-identical.
    fn is_insufficient_material(board: &Board<Cap10x8>, _state: &GenericState<Cap10x8>) -> bool {
        crate::geometry::variant::standard_insufficient_material(board)
    }
}

/// Gothic Chess as a [`GenericPosition`] over the 10x8 [`Cap10x8`] geometry.
///
/// Construct the starting position with
/// [`Gothic::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Gothic::from_fen`](GenericPosition::from_fen).
pub type Gothic = GenericPosition<Cap10x8, GothicRules>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startpos_round_trips() {
        let pos = Gothic::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqekabnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNBQEKABNR w KQkq - 0 1"
        );
        // FSF-confirmed opening move count.
        assert_eq!(pos.legal_move_count(), 28);
    }

    #[test]
    fn pawn_promotes_to_six_roles() {
        let pos = Gothic::from_fen("5k4/4P5/10/10/10/10/10/5K4 w - - 0 1").expect("valid");
        let mut roles: alloc::vec::Vec<WideRole> = pos
            .legal_moves()
            .into_iter()
            .filter_map(|m| m.promotion())
            .collect();
        roles.sort();
        roles.dedup();
        let mut want = alloc::vec![
            WideRole::Knight,
            WideRole::Bishop,
            WideRole::Rook,
            WideRole::Queen,
            WideRole::Hawk,
            WideRole::Elephant,
        ];
        want.sort();
        assert_eq!(roles, want);
    }
}
