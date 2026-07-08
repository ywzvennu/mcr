//! Chigorin Chess (8x8) on the generic engine — an **asymmetric** variant pitting
//! a White *knight* army (knights + a Chancellor, no bishops or queen) against a
//! Black *bishop* army (bishops + a queen, no knights). Named for Mikhail
//! Chigorin; see <https://www.chessvariants.com/diffsetup.dir/chigorin.html>.
//! Validated square-for-square against Fairy-Stockfish `UCI_Variant chigorin`.
//!
//! ## Armies (asymmetric)
//!
//! * **White** — Rook, Knight, Knight, **Chancellor** (Rook + Knight, mcr's
//!   [`WideRole::Elephant`], FEN `e`/`E`; FSF spells it `c`/`C`), King, Knight,
//!   Knight, Rook. No bishops, no queen: a pure knight army.
//! * **Black** — Rook, Bishop, Bishop, Queen, King, Bishop, Bishop, Rook. No
//!   knights, no chancellor: a pure bishop army.
//!
//! Both kings and rooks are on their standard files, so castling is standard 8x8.
//! Pawns, the double step, and en passant are standard chess.
//!
//! ## Colour-restricted promotion
//!
//! A pawn promotes only within its own army's character (FSF's per-colour
//! `promotionPieceTypes`):
//!
//! * **White** pawns promote to a **Chancellor, Rook, or Knight** — never a Queen
//!   or Bishop.
//! * **Black** pawns promote to a **Queen, Rook, or Bishop** — never a Knight or
//!   Chancellor.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant chigorin` (`position startpos`):
//!
//! ```text
//! FSF dialect: rbbqkbbr/pppppppp/8/8/8/8/PPPPPPPP/RNNCKNNR w KQkq - 0 1
//! mcr dialect: rbbqkbbr/pppppppp/8/8/8/8/PPPPPPPP/RNNEKNNR w KQkq - 0 1
//! ```
//!
//! The two differ only in White's chancellor letter (`c` in FSF, `e` in mcr). The
//! Black back rank is standard piece letters, identical in both dialects.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{Board, Chess8x8, Geometry, PromotionConfig, WideRole, WideVariant};
use crate::Color;

/// The confirmed Chigorin starting placement in the mcr dialect (White chancellor
/// = `E`): Black bishop army over White knight army, byte-for-byte equivalent to
/// FSF's `rbbqkbbr/.../RNNCKNNR` modulo the chancellor's letter.
const CHIGORIN_START_PLACEMENT: &str = "rbbqkbbr/pppppppp/8/8/8/8/PPPPPPPP/RNNEKNNR";

/// White's promotion targets: Chancellor, Rook, Knight (its knight-army pieces).
const WHITE_PROMOTIONS: [WideRole; 3] = [WideRole::Elephant, WideRole::Rook, WideRole::Knight];

/// Black's promotion targets: Queen, Rook, Bishop (its bishop-army pieces).
const BLACK_PROMOTIONS: [WideRole; 3] = [WideRole::Queen, WideRole::Rook, WideRole::Bishop];

/// The Chigorin Chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides the asymmetric starting array and the colour-restricted promotion
/// targets. The Chancellor ([`WideRole::Elephant`]) movement is already the trait
/// default, so no `role_attacks` override is needed; every piece, castling, the
/// double pawn step, and en passant are standard chess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ChigorinRules;

impl WideVariant<Chess8x8> for ChigorinRules {
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

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(CHIGORIN_START_PLACEMENT)
            .expect("the Chigorin starting placement is valid on an 8x8 board");
        // Standard chess castling rights for both sides.
        let mut castling = GenericCastling::NONE;
        for color in Color::ALL {
            castling.set(color, 0, Some(Chess8x8::WIDTH - 1));
            castling.set(color, 1, Some(0));
        }
        let state = GenericState {
            turn: Color::White,
            castling,
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

    fn promotion_config() -> PromotionConfig {
        // The union of both armies' promotion targets — the FEN / round-trip
        // vocabulary. `promotion_targets` filters this per colour.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Elephant, // Chancellor (White)
                WideRole::Queen,    // (Black)
                WideRole::Rook,
                WideRole::Bishop, // (Black)
                WideRole::Knight, // (White)
            ],
        }
    }

    fn promotion_targets<const R: usize>(
        color: Color,
        _board: &Board<Chess8x8, R>,
    ) -> alloc::vec::Vec<WideRole> {
        // White (the knight army) promotes to Chancellor / Rook / Knight; Black
        // (the bishop army) to Queen / Rook / Bishop. This is a fixed per-colour
        // set (unlike Grand's board-dependent limit), matching FSF's per-colour
        // `promotionPieceTypes`.
        match color {
            Color::White => WHITE_PROMOTIONS.to_vec(),
            Color::Black => BLACK_PROMOTIONS.to_vec(),
        }
    }

    fn has_castling() -> bool {
        true
    }

    /// Chigorin's armies are the standard bishops / knights / rooks plus the
    /// always-mating Chancellor ([`WideRole::Elephant`]) and a Queen, so the
    /// ordinary insufficient-material draw applies. Adjudication-only and behind
    /// the default-off hook, so perft stays byte-identical.
    fn is_insufficient_material<const R: usize>(
        board: &Board<Chess8x8, R>,
        _state: &GenericState<Chess8x8, R>,
    ) -> bool {
        crate::geometry::variant::standard_insufficient_material(board)
    }
}

/// Chigorin Chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Chigorin::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Chigorin::from_fen`](GenericPosition::from_fen). Only the asymmetric array and
/// the colour-restricted promotion targets distinguish it from standard chess.
pub type Chigorin = GenericPosition<
    Chess8x8,
    ChigorinRules,
    { <ChigorinRules as WideVariant<Chess8x8>>::ROLE_SPAN },
>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startpos_round_trips() {
        let pos = Chigorin::startpos();
        assert_eq!(
            pos.to_fen(),
            "rbbqkbbr/pppppppp/8/8/8/8/PPPPPPPP/RNNEKNNR w KQkq - 0 1"
        );
        // White: four knights, one chancellor, no bishops, no queen.
        assert_eq!(
            pos.board().pieces(Color::White, WideRole::Knight).count(),
            4
        );
        assert_eq!(
            pos.board().pieces(Color::White, WideRole::Elephant).count(),
            1
        );
        assert_eq!(
            pos.board().pieces(Color::White, WideRole::Bishop).count(),
            0
        );
        // Black: four bishops, one queen, no knights.
        assert_eq!(
            pos.board().pieces(Color::Black, WideRole::Bishop).count(),
            4
        );
        assert_eq!(
            pos.board().pieces(Color::Black, WideRole::Knight).count(),
            0
        );
    }

    /// White pawns promote to Chancellor / Rook / Knight (never Queen or Bishop).
    #[test]
    fn white_promotes_to_knight_army() {
        let pos = Chigorin::from_fen("4k3/1P6/8/8/8/8/8/4K3 w - - 0 1").expect("valid");
        let mut roles: alloc::vec::Vec<WideRole> = pos
            .legal_moves()
            .into_iter()
            .filter_map(|m| m.promotion())
            .collect();
        roles.sort();
        roles.dedup();
        let mut want = alloc::vec![WideRole::Elephant, WideRole::Rook, WideRole::Knight];
        want.sort();
        assert_eq!(roles, want);
    }

    /// Black pawns promote to Queen / Rook / Bishop (never Knight or Chancellor).
    #[test]
    fn black_promotes_to_bishop_army() {
        let pos = Chigorin::from_fen("4k3/8/8/8/8/8/1p6/4K3 b - - 0 1").expect("valid");
        let mut roles: alloc::vec::Vec<WideRole> = pos
            .legal_moves()
            .into_iter()
            .filter_map(|m| m.promotion())
            .collect();
        roles.sort();
        roles.dedup();
        let mut want = alloc::vec![WideRole::Queen, WideRole::Rook, WideRole::Bishop];
        want.sort();
        assert_eq!(roles, want);
    }
}
