//! Janus Chess (10x8) on the generic engine — a Capablanca-board variant with
//! **two Januses** (Bishop + Knight) per side and no Chancellor. Validated
//! square-for-square against Fairy-Stockfish `UCI_Variant janus`.
//!
//! Janus is played on the same ten-files by eight-ranks board as Capablanca. Its
//! only compound piece is the Janus (a.k.a. Archbishop / Cardinal):
//!
//! * **Janus** (Bishop + Knight) — mce's [`WideRole::Hawk`], whose default
//!   movement (`bishop | knight`) is already the Janus's. FEN letter `a`/`A` in
//!   the mce dialect (Fairy-Stockfish spells the Janus `j`/`J`, a dialect
//!   difference the `compare-fairy/` harness reconciles). There is **no
//!   Chancellor** in Janus Chess.
//!
//! Every other rule is standard chess: pawns push one (or two from their second
//! rank), capture diagonally, take en passant, and promote on the last rank to
//! Queen, Rook, Bishop, Knight, or **Janus** (never a Chancellor).
//!
//! ## Castling geometry
//!
//! The king starts on the **e-file** (file 4), with the Januses on the b/i files:
//!
//! * **Kingside**: king e1 -> **i1** (file 8), rook j1 -> **h1** (file 7).
//! * **Queenside**: king e1 -> **b1** (file 1), rook a1 -> **c1** (file 2).
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant janus` (`position startpos`):
//!
//! ```text
//! FSF dialect: rjnbkqbnjr/pppppppppp/10/10/10/10/PPPPPPPPPP/RJNBKQBNJR w KQkq - 0 1
//! mce dialect: ranbkqbnar/pppppppppp/10/10/10/10/PPPPPPPPPP/RANBKQBNAR w KQkq - 0 1
//! ```
//!
//! The two differ only in the Janus's letter (`j` in FSF, `a` in mce). Back rank
//! a-file to j-file: Rook, Janus, Knight, Bishop, King, Queen, Bishop, Knight,
//! Janus, Rook.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{Board, Cap10x8, PromotionConfig, WideRole, WideVariant};
use crate::Color;

/// The confirmed Janus starting placement in the mce dialect (Janus = `a`/`A`),
/// byte-for-byte equivalent to Fairy-Stockfish's `rjnbkqbnjr/.../RJNBKQBNJR`
/// modulo the Janus's letter.
const JANUS_START_PLACEMENT: &str = "ranbkqbnar/pppppppppp/10/10/10/10/PPPPPPPPPP/RANBKQBNAR";

/// The kingside castle side index, matching the position layer's `KINGSIDE`.
const KINGSIDE: usize = 0;

/// The Janus Chess rule layer: a zero-sized [`WideVariant`] over [`Cap10x8`].
///
/// It overrides the 10x8 starting array, the promotion set (adding the Janus, but
/// no Chancellor), and the Janus castle destination files (king on the e-file).
/// The Janus ([`WideRole::Hawk`]) movement is already the trait default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct JanusRules;

impl WideVariant<Cap10x8> for JanusRules {
    fn starting_position() -> (Board<Cap10x8>, GenericState<Cap10x8>) {
        let board = Board::<Cap10x8>::from_fen_placement(JANUS_START_PLACEMENT)
            .expect("the Janus starting placement is valid on a 10x8 board");
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::standard::<Cap10x8>(),
            ep_square: None,
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
        // FSF `promotionPieceTypes = j q r b n`: the Janus plus the four standard
        // roles — no Chancellor. Order affects only move enumeration order.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Hawk, // Janus (B+N)
                WideRole::Queen,
                WideRole::Rook,
                WideRole::Bishop,
                WideRole::Knight,
            ],
        }
    }

    fn castle_dest_files(side: usize) -> (u8, u8) {
        // King on the e-file (file 4). Kingside: king e1 -> i1 (8), rook j1 -> h1
        // (7). Queenside: king e1 -> b1 (1), rook a1 -> c1 (2).
        if side == KINGSIDE {
            (8, 7)
        } else {
            (1, 2)
        }
    }

    /// Janus keeps the standard chess army plus the always-mating Janus
    /// ([`WideRole::Hawk`]), so the ordinary insufficient-material draw applies.
    /// Adjudication-only and behind the default-off hook, so perft stays
    /// byte-identical.
    fn is_insufficient_material(board: &Board<Cap10x8>, _state: &GenericState<Cap10x8>) -> bool {
        crate::geometry::variant::standard_insufficient_material(board)
    }
}

/// Janus Chess as a [`GenericPosition`] over the 10x8 [`Cap10x8`] geometry.
///
/// Construct the starting position with
/// [`Janus::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Janus::from_fen`](GenericPosition::from_fen).
pub type Janus = GenericPosition<Cap10x8, JanusRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Square, WideMoveKind};

    #[test]
    fn startpos_round_trips() {
        let pos = Janus::startpos();
        assert_eq!(
            pos.to_fen(),
            "ranbkqbnar/pppppppppp/10/10/10/10/PPPPPPPPPP/RANBKQBNAR w KQkq - 0 1"
        );
        assert_eq!(pos.legal_move_count(), 28);
        // Two Januses (Hawks) per side.
        assert_eq!(pos.board().pieces(Color::White, WideRole::Hawk).count(), 2);
    }

    #[test]
    fn pawn_promotes_to_janus_not_chancellor() {
        let pos = Janus::from_fen("5k4/4P5/10/10/10/10/10/5K4 w - - 0 1").expect("valid");
        let mut roles: alloc::vec::Vec<WideRole> = pos
            .legal_moves()
            .into_iter()
            .filter_map(|m| m.promotion())
            .collect();
        roles.sort();
        roles.dedup();
        let mut want = alloc::vec![
            WideRole::Hawk,
            WideRole::Queen,
            WideRole::Rook,
            WideRole::Bishop,
            WideRole::Knight,
        ];
        want.sort();
        assert_eq!(roles, want);
        assert!(
            !roles.contains(&WideRole::Elephant),
            "no chancellor promotion"
        );
    }

    #[test]
    fn castling_uses_janus_files() {
        let pos = Janus::from_fen("r3k4r/pppppppppp/10/10/10/10/PPPPPPPPPP/R3K4R w KQkq - 0 1")
            .expect("valid");
        let mut saw_kingside = false;
        let mut saw_queenside = false;
        for mv in pos.legal_moves() {
            match mv.kind() {
                WideMoveKind::CastleKingside => {
                    saw_kingside = true;
                    assert_eq!(mv.to_uci::<Cap10x8>(), "e1i1");
                    assert_eq!(
                        pos.play(&mv).board().king_of(Color::White),
                        Square::<Cap10x8>::from_file_rank(8, 0),
                    );
                }
                WideMoveKind::CastleQueenside => {
                    saw_queenside = true;
                    assert_eq!(mv.to_uci::<Cap10x8>(), "e1b1");
                    assert_eq!(
                        pos.play(&mv).board().king_of(Color::White),
                        Square::<Cap10x8>::from_file_rank(1, 0),
                    );
                }
                _ => {}
            }
        }
        assert!(saw_kingside && saw_queenside, "both castles available");
    }
}
