//! Embassy Chess (10x8) on the generic engine — a Capablanca-board variant
//! (Kevin Hill's Embassy Chess) with the king on the e-file and its own castle
//! geometry. Validated square-for-square against Fairy-Stockfish
//! `UCI_Variant embassy`.
//!
//! Embassy is played on the same ten-files by eight-ranks board as Capablanca and
//! adds the same two compound pieces, so it reuses the [`Cap10x8`] geometry and the
//! compound-piece defaults; the opening array and the castle files differ:
//!
//! * **Chancellor** (Rook + Knight) — mcr's [`WideRole::Elephant`], FEN letter
//!   `e`/`E` (FSF spells it `c`/`C`; the `compare-fairy/` harness reconciles).
//! * **Archbishop** (Bishop + Knight) — mcr's [`WideRole::Hawk`], FEN letter
//!   `a`/`A` in both mcr and FSF.
//!
//! Every other rule is standard chess: pawns push one (or two from their second
//! rank), capture diagonally, take en passant, and promote on the last rank to
//! Queen, Rook, Bishop, Knight, Archbishop, or Chancellor.
//!
//! ## Castling geometry
//!
//! The king starts on the **e-file** (file 4), so — matching Fairy-Stockfish's
//! `embassy` castle files — it lands two files toward the rook:
//!
//! * **Kingside**: king e1 -> **h1** (file 7), rook j1 -> **g1** (file 6).
//! * **Queenside**: king e1 -> **b1** (file 1), rook a1 -> **c1** (file 2).
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant embassy` (`position startpos`):
//!
//! ```text
//! FSF dialect: rnbqkcabnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNBQKCABNR w KQkq - 0 1
//! mcr dialect: rnbqkeabnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNBQKEABNR w KQkq - 0 1
//! ```
//!
//! The two differ only in the chancellor's letter (`c` in FSF, `e` in mcr). Back
//! rank a-file to j-file: Rook, Knight, Bishop, Queen, King, Chancellor,
//! Archbishop, Bishop, Knight, Rook.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{Board, Cap10x8, PromotionConfig, WideRole, WideVariant};
use crate::Color;

/// The confirmed Embassy starting placement in the mcr dialect (chancellor =
/// `e`/`E`), byte-for-byte equivalent to Fairy-Stockfish's
/// `rnbqkcabnr/.../RNBQKCABNR` modulo the chancellor's letter.
const EMBASSY_START_PLACEMENT: &str = "rnbqkeabnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNBQKEABNR";

/// The kingside castle side index, matching the position layer's `KINGSIDE`.
const KINGSIDE: usize = 0;

/// The Embassy Chess rule layer: a zero-sized [`WideVariant`] over [`Cap10x8`].
///
/// It overrides the 10x8 starting array, the wider promotion set (adding
/// Archbishop and Chancellor), and the Embassy castle destination files (king on
/// the e-file). The Archbishop ([`WideRole::Hawk`]) and Chancellor
/// ([`WideRole::Elephant`]) movement is already the trait default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct EmbassyRules;

impl WideVariant<Cap10x8> for EmbassyRules {
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
        let board = Board::<Cap10x8>::from_fen_placement(EMBASSY_START_PLACEMENT)
            .expect("the Embassy starting placement is valid on a 10x8 board");
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
        };
        (board, state)
    }

    fn promotion_config() -> PromotionConfig {
        // The six-role Capablanca-family promotion set (FSF order c a q r b n).
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
        // King on the e-file (file 4). Kingside: king e1 -> h1 (7), rook j1 -> g1
        // (6). Queenside: king e1 -> b1 (1), rook a1 -> c1 (2).
        if side == KINGSIDE {
            (7, 6)
        } else {
            (1, 2)
        }
    }

    /// Embassy keeps the standard chess army plus the always-mating Archbishop
    /// ([`WideRole::Hawk`]) and Chancellor ([`WideRole::Elephant`]), so the ordinary
    /// insufficient-material draw applies. Adjudication-only and behind the
    /// default-off hook, so perft stays byte-identical.
    fn is_insufficient_material(board: &Board<Cap10x8>, _state: &GenericState<Cap10x8>) -> bool {
        crate::geometry::variant::standard_insufficient_material(board)
    }
}

/// Embassy Chess as a [`GenericPosition`] over the 10x8 [`Cap10x8`] geometry.
///
/// Construct the starting position with
/// [`Embassy::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Embassy::from_fen`](GenericPosition::from_fen).
pub type Embassy = GenericPosition<Cap10x8, EmbassyRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Square, WideMoveKind};

    #[test]
    fn startpos_round_trips() {
        let pos = Embassy::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkeabnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNBQKEABNR w KQkq - 0 1"
        );
        assert_eq!(pos.legal_move_count(), 28);
    }

    #[test]
    fn castling_uses_embassy_files() {
        let pos = Embassy::from_fen("r3k4r/pppppppppp/10/10/10/10/PPPPPPPPPP/R3K4R w KQkq - 0 1")
            .expect("valid");
        let mut saw_kingside = false;
        let mut saw_queenside = false;
        for mv in pos.legal_moves() {
            match mv.kind() {
                WideMoveKind::CastleKingside => {
                    saw_kingside = true;
                    assert_eq!(mv.to_uci::<Cap10x8>(), "e1h1");
                    let next = pos.play(&mv);
                    assert_eq!(
                        next.board().king_of(Color::White),
                        Square::<Cap10x8>::from_file_rank(7, 0),
                    );
                }
                WideMoveKind::CastleQueenside => {
                    saw_queenside = true;
                    assert_eq!(mv.to_uci::<Cap10x8>(), "e1b1");
                    let next = pos.play(&mv);
                    assert_eq!(
                        next.board().king_of(Color::White),
                        Square::<Cap10x8>::from_file_rank(1, 0),
                    );
                }
                _ => {}
            }
        }
        assert!(saw_kingside && saw_queenside, "both castles available");
    }
}
