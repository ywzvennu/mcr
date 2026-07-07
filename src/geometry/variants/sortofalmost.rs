//! Sort-of-almost Chess (8x8) on the generic engine — standard chess with **only
//! White's Queen replaced by a Chancellor** (Rook + Knight). An asymmetric cousin
//! of Almost Chess; see
//! <https://en.wikipedia.org/wiki/Almost_chess#Sort_of_almost_chess>. Validated
//! square-for-square against Fairy-Stockfish `UCI_Variant sortofalmost`.
//!
//! Every rule is standard chess — the 8x8 board, standard pawns (double step, en
//! passant), standard castling — except that the two sides field **different**
//! heavy pieces:
//!
//! * **White** has a **Chancellor** (Rook + Knight) on the d-file where its queen
//!   would stand, and **no queen**. The Chancellor is mcr's [`WideRole::Elephant`],
//!   whose default movement (`rook | knight`) is already the chancellor's. FEN
//!   letter `e`/`E` in the mcr dialect (Fairy-Stockfish spells the chancellor
//!   `c`/`C`, a dialect difference the `compare-fairy/` harness reconciles, exactly
//!   as for Capablanca / Almost).
//! * **Black** keeps its ordinary **Queen** ([`WideRole::Queen`]) and has no
//!   chancellor.
//!
//! The promotion sets mirror the asymmetry — each side promotes to the heavy piece
//! it actually fields (FSF `promotionPieceTypes`):
//!
//! * **White**: Chancellor, Rook, Bishop, or Knight — never a Queen.
//! * **Black**: Queen, Rook, Bishop, or Knight — never a Chancellor.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant sortofalmost` (`position
//! startpos`):
//!
//! ```text
//! FSF dialect: rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBCKBNR w KQkq - 0 1
//! mcr dialect: rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBEKBNR w KQkq - 0 1
//! ```
//!
//! The two strings differ only in White's chancellor letter (`C` in FSF, `E` in
//! mcr). Black's back rank is standard chess; every other piece, castling, the
//! double pawn step, and en passant are standard chess.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{Board, Chess8x8, Geometry, PromotionConfig, WideRole, WideVariant};
use crate::Color;

/// The confirmed Sort-of-almost Chess starting placement in the mcr dialect
/// (chancellor = `E` on White's queen square), byte-for-byte equivalent to
/// Fairy-Stockfish's `rnbqkbnr/.../RNBCKBNR` modulo the chancellor's letter.
const SORTOFALMOST_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBEKBNR";

/// The Sort-of-almost Chess rule layer: a zero-sized [`WideVariant`] over
/// [`Chess8x8`].
///
/// It overrides only the starting array (a Chancellor on White's queen square, a
/// Queen on Black's) and the per-colour promotion set (White to Chancellor / Rook /
/// Bishop / Knight, Black to Queen / Rook / Bishop / Knight). The Chancellor
/// ([`WideRole::Elephant`]) movement is already the trait default, so no
/// `role_attacks` override is needed; pawns, knights, sliders, the king, castling,
/// and en passant are standard chess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SortofalmostRules;

impl WideVariant<Chess8x8> for SortofalmostRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field: it fields both the Queen (index 4, Black) and the
    /// Chancellor / Elephant (index 11, White), so the span must reach past the
    /// Elephant. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 12;

    /// The western **fifty-move rule**: a position whose halfmove clock has reached
    /// 100 plies (50 full moves with no capture or pawn move) is a
    /// [`WideEndReason::MoveRule`](crate::geometry::WideEndReason::MoveRule) draw,
    /// matching Fairy-Stockfish's default `nMoveRule = 50`. Adjudication-only (the
    /// clock never gates move generation), so perft stays byte-identical.
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
        let board = Board::<Chess8x8>::from_fen_placement(SORTOFALMOST_START_PLACEMENT)
            .expect("the Sort-of-almost Chess starting placement is valid on an 8x8 board");
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
        // The full promotion vocabulary across both colours — Queen (Black) plus
        // Chancellor (White) plus rook, bishop, knight. This is the unfiltered
        // superset (and the FEN round-trip vocabulary); `promotion_targets` narrows
        // it per colour to the heavy piece that side actually fields. The order only
        // affects move enumeration order, not the perft leaf count.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Knight,
                WideRole::Bishop,
                WideRole::Rook,
                WideRole::Queen,    // Black's heavy promotion
                WideRole::Elephant, // Chancellor (R+N): White's heavy promotion
            ],
        }
    }

    /// Sort-of-almost Chess promotes each side to the heavy piece it fields, so the
    /// legal targets depend on the mover's colour (FSF's asymmetric
    /// `promotionPieceTypes`): White to Chancellor / Rook / Bishop / Knight, Black
    /// to Queen / Rook / Bishop / Knight. Neither depends on the running board, so
    /// no extra position state is threaded through make/unmake.
    fn promotion_targets<const R: usize>(
        color: Color,
        _board: &Board<Chess8x8, R>,
    ) -> alloc::vec::Vec<WideRole> {
        match color {
            // FSF `promotionPieceTypes[WHITE] = CHANCELLOR | ROOK | BISHOP | KNIGHT`.
            Color::White => alloc::vec![
                WideRole::Elephant, // Chancellor (R+N)
                WideRole::Rook,
                WideRole::Bishop,
                WideRole::Knight,
            ],
            // FSF `promotionPieceTypes[BLACK] = QUEEN | ROOK | BISHOP | KNIGHT`.
            Color::Black => alloc::vec![
                WideRole::Queen,
                WideRole::Rook,
                WideRole::Bishop,
                WideRole::Knight,
            ],
        }
    }

    fn has_castling() -> bool {
        true
    }

    /// Sort-of-almost Chess keeps a standard chess army bar the piece swap (White's
    /// Chancellor for its Queen), so the ordinary insufficient-material draw applies:
    /// king vs king, king and a lone minor (bishop or knight) vs king, and
    /// same-colour bishops only. Both the Queen and the Chancellor
    /// ([`WideRole::Elephant`]) count as mating material (major pieces).
    /// Adjudication-only and behind the default-off hook, so perft stays
    /// byte-identical.
    fn is_insufficient_material<const R: usize>(
        board: &Board<Chess8x8, R>,
        _state: &GenericState<Chess8x8, R>,
    ) -> bool {
        crate::geometry::variant::standard_insufficient_material(board)
    }
}

/// Sort-of-almost Chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Sortofalmost::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Sortofalmost::from_fen`](GenericPosition::from_fen). The Chancellor reuses the
/// [`StandardChess`](crate::geometry::StandardChess) compound default, so only the
/// array and the per-colour promotion sets distinguish it from standard chess.
pub type Sortofalmost = GenericPosition<
    Chess8x8,
    SortofalmostRules,
    { <SortofalmostRules as WideVariant<Chess8x8>>::ROLE_SPAN },
>;

#[cfg(test)]
mod tests {
    use super::*;

    /// The canonical start FEN round-trips and opens with the FSF-confirmed 22
    /// moves (the standard 20 plus the two chancellor knight-hops from d1).
    #[test]
    fn startpos_round_trips() {
        let pos = Sortofalmost::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBEKBNR w KQkq - 0 1"
        );
        assert_eq!(pos.legal_move_count(), 22);
    }

    /// The White Chancellor on its home square (d1) moves as Rook + Knight: with an
    /// otherwise empty board it combines the rook rays down its file and along its
    /// rank with the knight hops off d1 — reaching squares (like c3) no rook could.
    #[test]
    fn white_chancellor_moves_as_rook_plus_knight() {
        // White chancellor alone on d1 (mcr `E`), kings out of the way.
        let pos = Sortofalmost::from_fen("7k/8/8/8/8/8/8/3E1K2 w - - 0 1").expect("valid");
        let from = 3u8; // d1
        let chancellor_moves = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Chess8x8>().index() == from)
            .count();
        // A lone knight from d1 has only four on-board hops; the chancellor also
        // rides the d-file and the first rank, so it must have far more than that.
        assert!(
            chancellor_moves >= 12,
            "chancellor should combine rook and knight reach, got {chancellor_moves}"
        );
        // A knight-only destination (c3, index 18) must be reachable — a plain rook
        // sharing the file/rank could never land there.
        let c3 = 2u8 + 2 * 8; // file c (2), rank 3 (index 2)
        assert!(
            pos.legal_moves().into_iter().any(|m| {
                m.from::<Chess8x8>().index() == from && m.to::<Chess8x8>().index() == c3
            }),
            "chancellor must reach the knight square c3"
        );
    }

    /// A White pawn promotes to Chancellor / Rook / Bishop / Knight — never a Queen.
    #[test]
    fn white_pawn_promotes_to_chancellor_not_queen() {
        let pos = Sortofalmost::from_fen("4k3/1P6/8/8/8/8/8/4K3 w - - 0 1").expect("valid");
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
        assert!(
            !roles.contains(&WideRole::Queen),
            "White has no queen promotion"
        );
    }

    /// A Black pawn promotes to Queen / Rook / Bishop / Knight — never a Chancellor.
    #[test]
    fn black_pawn_promotes_to_queen_not_chancellor() {
        let pos = Sortofalmost::from_fen("4k3/8/8/8/8/8/1p6/4K3 b - - 0 1").expect("valid");
        let mut roles: alloc::vec::Vec<WideRole> = pos
            .legal_moves()
            .into_iter()
            .filter_map(|m| m.promotion())
            .collect();
        roles.sort();
        roles.dedup();
        let mut want = alloc::vec![
            WideRole::Queen,
            WideRole::Rook,
            WideRole::Bishop,
            WideRole::Knight,
        ];
        want.sort();
        assert_eq!(roles, want);
        assert!(
            !roles.contains(&WideRole::Elephant),
            "Black has no chancellor promotion"
        );
    }
}
