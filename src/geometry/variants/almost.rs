//! Almost Chess (8x8) on the generic engine — standard chess with the **Queen
//! replaced by a Chancellor** (Rook + Knight). A Ralph Betza variant; see
//! <https://www.chessvariants.com/diffsetup.dir/almost.html>. Validated
//! square-for-square against Fairy-Stockfish `UCI_Variant almost`.
//!
//! Every rule is standard chess — the 8x8 board, standard pawns (double step, en
//! passant), standard castling — except the piece on the queen's square (d-file):
//!
//! * **Chancellor** (Rook + Knight) — mcr's [`WideRole::Elephant`], whose default
//!   movement (`rook | knight`) is already the chancellor's. FEN letter `e`/`E` in
//!   the mcr dialect (Fairy-Stockfish spells the chancellor `c`/`C`, a dialect
//!   difference the `compare-fairy/` harness reconciles, exactly as for
//!   Capablanca).
//!
//! There is **no Queen** in Almost Chess, so a pawn promotes to a **Chancellor,
//! Rook, Bishop, or Knight** (FSF `promotionPieceTypes = c r b n`) — never a
//! Queen.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant almost` (`position startpos`):
//!
//! ```text
//! FSF dialect: rnbckbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBCKBNR w KQkq - 0 1
//! mcr dialect: rnbekbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBEKBNR w KQkq - 0 1
//! ```
//!
//! The two strings differ only in the chancellor's letter (`c` in FSF, `e` in
//! mcr). Every other piece, castling, the double pawn step, and en passant are
//! standard chess.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{Board, Chess8x8, Geometry, PromotionConfig, WideRole, WideVariant};
use crate::Color;

/// The confirmed Almost Chess starting placement in the mcr dialect (chancellor =
/// `e`/`E`), byte-for-byte equivalent to Fairy-Stockfish's
/// `rnbckbnr/.../RNBCKBNR` modulo the chancellor's letter.
const ALMOST_START_PLACEMENT: &str = "rnbekbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBEKBNR";

/// The Almost Chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides only the starting array (Chancellor on the queen's square) and the
/// promotion set (`c r b n`, no Queen). The Chancellor ([`WideRole::Elephant`])
/// movement is already the trait default, so no `role_attacks` override is needed;
/// pawns, knights, sliders, the king, castling, and en passant are standard chess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct AlmostRules;

impl WideVariant<Chess8x8> for AlmostRules {
    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(ALMOST_START_PLACEMENT)
            .expect("the Almost Chess starting placement is valid on an 8x8 board");
        // Standard chess castling rights for both sides: the kingside rook sits on
        // the last file, the queenside rook on file 0.
        let mut castling = GenericCastling::NONE;
        for color in Color::ALL {
            castling.set(color, 0, Some(Chess8x8::WIDTH - 1));
            castling.set(color, 1, Some(0));
        }
        let state = GenericState {
            turn: Color::White,
            castling,
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
        // FSF `promotionPieceTypes = c r b n`: the Chancellor plus rook, bishop,
        // knight — no Queen. Order matches FSF's promotion set; it affects only
        // move enumeration order, not the perft leaf count.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Elephant, // Chancellor (R+N)
                WideRole::Rook,
                WideRole::Bishop,
                WideRole::Knight,
            ],
        }
    }

    fn has_castling() -> bool {
        true
    }

    /// Almost Chess keeps the standard chess army (bar the Queen) plus the
    /// always-mating Chancellor ([`WideRole::Elephant`]), so the ordinary
    /// insufficient-material draw applies: king vs king, king and a lone minor
    /// (bishop or knight) vs king, and same-colour bishops only. The Chancellor
    /// counts as mating material (a major piece). Adjudication-only and behind the
    /// default-off hook, so perft stays byte-identical.
    fn is_insufficient_material(board: &Board<Chess8x8>, _state: &GenericState<Chess8x8>) -> bool {
        crate::geometry::variant::standard_insufficient_material(board)
    }
}

/// Almost Chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Almost::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Almost::from_fen`](GenericPosition::from_fen). The Chancellor reuses the
/// [`StandardChess`](crate::geometry::StandardChess) compound default, so only the
/// array and the no-Queen promotion set distinguish it from standard chess.
pub type Almost = GenericPosition<Chess8x8, AlmostRules>;

#[cfg(test)]
mod tests {
    use super::*;

    /// The canonical start FEN round-trips and opens with the FSF-confirmed 22
    /// moves (the standard 20 plus the two chancellor knight-hops from d1).
    #[test]
    fn startpos_round_trips() {
        let pos = Almost::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbekbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBEKBNR w KQkq - 0 1"
        );
        assert_eq!(pos.legal_move_count(), 22);
    }

    /// A pawn promotes to Chancellor / Rook / Bishop / Knight — never a Queen.
    #[test]
    fn pawn_promotes_without_queen() {
        let pos = Almost::from_fen("4k3/1P6/8/8/8/8/8/4K3 w - - 0 1").expect("valid");
        let mut roles: alloc::vec::Vec<WideRole> = pos
            .legal_moves()
            .into_iter()
            .filter_map(|m| m.promotion())
            .collect();
        roles.sort();
        roles.dedup();
        let mut want = alloc::vec![
            WideRole::Elephant,
            WideRole::Rook,
            WideRole::Bishop,
            WideRole::Knight,
        ];
        want.sort();
        assert_eq!(roles, want);
        assert!(!roles.contains(&WideRole::Queen), "no queen promotion");
    }
}
