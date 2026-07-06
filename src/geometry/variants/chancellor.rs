//! Chancellor chess (9x9) on the generic engine — standard western chess widened
//! to a nine-files by nine-ranks board with a Rook + Knight **Chancellor** added
//! to each side's back rank. It validates the [`Chess9x9`] geometry — a distinct
//! 9x9 chess board that never shares its masks with the shogi family on the same
//! shape.
//!
//! Chancellor chess is played on nine files (a..i) by nine ranks (1..9). Beyond
//! the standard chess army each side adds one compound piece:
//!
//! * **Chancellor** (Rook + Knight) — mcr's [`WideRole::Elephant`], whose default
//!   movement (`rook | knight`) is already the chancellor's. FEN letter `e`/`E` in
//!   the mcr dialect (Fairy-Stockfish spells the chancellor `c`/`C`, a dialect
//!   difference the `compare-fairy/` harness reconciles, exactly as for
//!   Capablanca / Grand).
//!
//! ## Rules — standard chess on a 9x9 board
//!
//! Every rule is standard chess, just on the wider board:
//!
//! * **Castling** on the standard files: the king starts on the e-file (file 4)
//!   and the castling rooks on the a-file (file 0) and i-file (file 8). Kingside
//!   the king lands on g (file 6) with the rook on f (file 5); queenside the king
//!   lands on c (file 2) with the rook on d (file 3) — the [`WideVariant`] trait
//!   defaults, matching Fairy-Stockfish's `castlingKingsideFile = FILE_G`,
//!   `castlingQueensideFile = FILE_C`.
//! * **Pawns** double-push from their second rank (rank 2 for white, rank 8 for
//!   black — the trait default `double_push_rank`), capture diagonally, take en
//!   passant, and **promote on the far rank** (rank 9 for white, rank 1 for
//!   black — the trait default `promotion_rank`).
//! * **Promotion** to Queen, Rook, Bishop, Knight, **or Chancellor**, matching
//!   Fairy-Stockfish's `promotionPieceTypes = CHANCELLOR | QUEEN | ROOK | BISHOP |
//!   KNIGHT`.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant chancellor` (its
//! `chancellor_variant()` `startFen`):
//!
//! ```text
//! FSF dialect: rnbqkcnbr/ppppppppp/9/9/9/9/9/PPPPPPPPP/RNBQKCNBR w KQkq - 0 1
//! mcr dialect: rnbqkenbr/ppppppppp/9/9/9/9/9/PPPPPPPPP/RNBQKENBR w KQkq - 0 1
//! ```
//!
//! The two strings differ only in the chancellor's letter (`c` in FSF, `e` in
//! mcr). Back rank, a-file to i-file: Rook, Knight, Bishop, Queen, King,
//! **Chancellor**, Knight, Bishop, Rook. The king stands on the e-file (file 4);
//! the chancellor beside it on the f-file (file 5); the castling rooks are the
//! a-file (file 0) and i-file (file 8) rooks.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{Board, Chess9x9, PromotionConfig, WideRole, WideVariant};
use crate::Color;

/// The Chancellor chess rule layer: a zero-sized [`WideVariant`] over [`Chess9x9`].
///
/// It overrides only what Chancellor chess changes from the standard generic
/// engine: the 9x9 starting array and the promotion set (adding the Chancellor).
/// The Chancellor ([`WideRole::Elephant`]) movement is already the trait default,
/// so no `role_attacks` override is needed; pawns, knights, sliders, the king, and
/// castling (king on the e-file, rooks on the a/i files, standard destinations)
/// are all standard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ChancellorRules;

/// The confirmed Chancellor starting placement in the mcr dialect (chancellor =
/// `e`/`E`), byte-for-byte equivalent to Fairy-Stockfish's
/// `rnbqkcnbr/.../RNBQKCNBR` modulo the chancellor's letter.
const CHANCELLOR_START_PLACEMENT: &str = "rnbqkenbr/ppppppppp/9/9/9/9/9/PPPPPPPPP/RNBQKENBR";

impl WideVariant<Chess9x9> for ChancellorRules {
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

    fn starting_position() -> (Board<Chess9x9>, GenericState<Chess9x9>) {
        let board = Board::<Chess9x9>::from_fen_placement(CHANCELLOR_START_PLACEMENT)
            .expect("the Chancellor starting placement is valid on a 9x9 board");
        let state = GenericState {
            turn: Color::White,
            // Both colors castle with both rooks (a/i files); the generic castling
            // layer reads the rook files from the back rank, so the standard `KQkq`
            // field maps to the a-file (queenside) and i-file (kingside) rooks on
            // this 9-wide board.
            castling: GenericCastling::standard::<Chess9x9>(),
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
        // A Chancellor-chess pawn promotes to any of the five non-pawn, non-king
        // roles of the army: the four standard plus the Chancellor. The order
        // matches Fairy-Stockfish's promotion set (knight, bishop, rook, queen,
        // chancellor); the order only affects move enumeration order, not the perft
        // leaf count.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Knight,
                WideRole::Bishop,
                WideRole::Rook,
                WideRole::Queen,
                WideRole::Elephant, // Chancellor (R+N)
            ],
        }
    }

    /// Chancellor chess keeps the standard chess army plus the always-mating
    /// Chancellor ([`WideRole::Elephant`]), so the ordinary insufficient-material
    /// draw applies on the 9x9 board: king vs king, king and a lone minor (bishop
    /// or knight) vs king, and same-colour bishops only. The Chancellor counts as
    /// mating material (matching Fairy-Stockfish, which classes the chancellor as a
    /// major piece). Adjudication-only and behind the default-off hook, so perft
    /// stays byte-identical.
    fn is_insufficient_material(board: &Board<Chess9x9>, _state: &GenericState<Chess9x9>) -> bool {
        crate::geometry::variant::standard_insufficient_material(board)
    }
}

/// Chancellor chess as a [`GenericPosition`] over the 9x9 [`Chess9x9`] geometry.
///
/// Construct the starting position with
/// [`Chancellor::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Chancellor::from_fen`](GenericPosition::from_fen). The Chancellor reuses the
/// [`StandardChess`](crate::geometry::StandardChess) compound default, so only the
/// array and the promotion set distinguish it from standard chess widened to 9x9.
pub type Chancellor = GenericPosition<Chess9x9, ChancellorRules>;

#[cfg(test)]
mod insufficient_material_tests {
    use super::Chancellor;
    use crate::geometry::{WideEndReason, WideOutcome};

    fn end_reason(fen: &str) -> Option<WideEndReason> {
        Chancellor::from_fen(fen)
            .expect("valid chancellor fen")
            .end_reason()
    }

    #[test]
    fn lone_kings_draw() {
        let pos = Chancellor::from_fen("4k4/9/9/9/9/9/9/9/4K4 w - - 0 1").expect("valid fen");
        assert_eq!(pos.end_reason(), Some(WideEndReason::InsufficientMaterial));
        assert_eq!(pos.outcome(), Some(WideOutcome::Draw));
    }

    #[test]
    fn king_and_single_minor_draw() {
        assert_eq!(
            end_reason("4k4/9/9/9/9/9/9/9/4KN3 w - - 0 1"),
            Some(WideEndReason::InsufficientMaterial)
        );
        assert_eq!(
            end_reason("4k4/9/9/9/9/9/9/9/4KB3 w - - 0 1"),
            Some(WideEndReason::InsufficientMaterial)
        );
    }

    #[test]
    fn same_colour_bishops_draw() {
        // White Ba1 and black Ba9 are both on the dark complex.
        assert_eq!(
            end_reason("b3k4/9/9/9/9/9/9/9/B3K4 w - - 0 1"),
            Some(WideEndReason::InsufficientMaterial)
        );
    }

    #[test]
    fn opposite_colour_bishops_are_sufficient() {
        // White Ba1 (dark) vs black Bb9 (light): a mate exists, not adjudicated.
        assert_eq!(end_reason("1b2k4/9/9/9/9/9/9/9/B3K4 w - - 0 1"), None);
    }

    #[test]
    fn compound_piece_is_sufficient() {
        // The Chancellor (R+N, `E`) is a major piece: a lone one beside the king is
        // not an insufficient-material draw.
        assert_eq!(end_reason("4k4/9/9/9/9/9/9/9/4KE3 w - - 0 1"), None);
    }
}
