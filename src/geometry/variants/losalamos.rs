//! Los Alamos chess (6x6, no bishops) on the generic engine — the 1956 MANIAC-I
//! program's chess, the first chess-like game a computer ever played. It runs on
//! mcr's generic engine over a new six-by-six (36-square) [`Losalamos6x6`]
//! geometry, fielding the **standard chess army minus the Bishop**. Validated
//! against Fairy-Stockfish `UCI_Variant losalamos`.
//!
//! ## What Los Alamos chess is
//!
//! A 6x6 board with the diagonal-sliding Bishop removed (the piece the era's
//! machine could not afford to search) and only six pawns a side. The back rank is
//! **R N Q K N R** — Rook, Knight, Queen, King, Knight, Rook — with a full rank of
//! six pawns in front. Every piece that remains (Rook, Knight, Queen, King, Pawn)
//! moves exactly as in standard chess; the board is simply narrower and shorter.
//!
//! ## Rules — standard chess minus the bishop, on 6x6
//!
//! * **No bishop.** The Bishop is absent from the start array and is **not** a
//!   promotion target; the role is simply never present.
//! * **No castling.** [`has_castling`](WideVariant::has_castling) is `false` and
//!   the start state carries no castling rights.
//! * **No pawn double-step.** A pawn advances exactly one square
//!   ([`pawn_may_double_push_from`](WideVariant::pawn_may_double_push_from) is
//!   always `false`), so there is no two-square opening move.
//! * **No en passant.** [`has_en_passant`](WideVariant::has_en_passant) is
//!   `false`; no en-passant target is ever recorded.
//! * **Promotion** happens on the far rank (rank 6 / 0-based 5 for White, rank 1 /
//!   0-based 0 for Black) to **Queen, Rook, or Knight** — never a Bishop, which the
//!   army does not field.
//! * The **fifty-move rule** and **threefold repetition** are the standard western
//!   terminals (Fairy-Stockfish's default `nMoveRule = 50` for this board). Both
//!   are adjudication-only, so perft is byte-identical.
//! * **Win by checkmate**, otherwise standard 6x6 chess.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant losalamos` (a built-in): six
//! files, six ranks, the bishop-less back rank, and a `-` castling field:
//!
//! ```text
//! rnqknr/pppppp/6/6/PPPPPP/RNQKNR w - - 0 1
//! ```
//!
//! mcr and FSF spell every position with the **identical** standard-chess letters
//! (no bishops to disambiguate), so the differential harness passes the FEN
//! through unchanged.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
#[allow(unused_imports)] // `StandardChess` is referenced by the rustdoc intra-doc links.
use crate::geometry::StandardChess;
use crate::geometry::{Board, PromotionConfig, WideRole, WideVariant};
use crate::Color;

use super::super::Losalamos6x6;

/// The confirmed Los Alamos starting placement: the bishop-less R N Q K N R back
/// rank with six pawns a side.
const LOSALAMOS_START_PLACEMENT: &str = "rnqknr/pppppp/6/6/PPPPPP/RNQKNR";

/// The Los Alamos chess rule layer: a zero-sized [`WideVariant`] over
/// [`Losalamos6x6`].
///
/// It is the reference [`StandardChess`] ruleset restricted to a 6x6 board with
/// the Bishop removed and the opening two-square pawn push, en passant, and
/// castling all disabled. Every remaining piece (Rook, Knight, Queen, King, Pawn)
/// moves as in standard chess — those role-attack primitives are geometry-generic,
/// so the trait default already produces the correct 6x6 moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LosalamosRules;

impl WideVariant<Losalamos6x6> for LosalamosRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field. The army is Pawn / Knight / Rook / Queen / King and the
    /// promotion set is Queen / Rook / Knight — the Bishop (index 2) is never
    /// present, but the King sits at index 5, so the span is the standard six-role
    /// prefix. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Losalamos6x6>, GenericState<Losalamos6x6>) {
        let board = Board::<Losalamos6x6>::from_fen_placement(LOSALAMOS_START_PLACEMENT)
            .expect("the Los Alamos starting placement is valid on a 6x6 board");
        let state = GenericState {
            turn: Color::White,
            // No castling in Los Alamos chess.
            castling: GenericCastling::NONE,
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

    /// Promotion targets are Queen, Rook, and Knight — **no Bishop**, which the Los
    /// Alamos army never fields. The default
    /// [`promotion_targets`](WideVariant::promotion_targets) returns this set, so the
    /// pawn generator emits exactly these three roles on the far rank.
    fn promotion_config() -> PromotionConfig {
        PromotionConfig {
            roles: alloc::vec![WideRole::Queen, WideRole::Rook, WideRole::Knight],
        }
    }

    /// Los Alamos chess has no castling.
    fn has_castling() -> bool {
        false
    }

    /// Los Alamos chess has no en passant: a pawn only ever advances one square, so
    /// no double step and no en-passant target is recorded.
    fn has_en_passant() -> bool {
        false
    }

    /// A Los Alamos pawn never makes the two-square opening advance — from any rank
    /// it may only single-step.
    fn pawn_may_double_push_from(_rank: u8, _color: Color) -> bool {
        false
    }

    /// The western **fifty-move rule**: a position whose halfmove clock has reached
    /// 100 plies (50 full moves with no capture or pawn move) is a
    /// [`WideEndReason::MoveRule`](crate::geometry::WideEndReason::MoveRule) draw,
    /// matching Fairy-Stockfish's default `nMoveRule = 50`. Adjudication-only, so
    /// perft stays byte-identical.
    fn move_rule_plies() -> Option<u16> {
        Some(100)
    }

    /// Records position history so the standard **threefold** repetition draw fires
    /// at the [`GenericGame`](crate::geometry::game::GenericGame) level. History-
    /// dependent and never consulted by a bare [`GenericPosition`], so perft is
    /// unchanged.
    fn tracks_repetition() -> bool {
        true
    }
}

/// Los Alamos chess as a [`GenericPosition`] over the 6x6 [`Losalamos6x6`]
/// geometry.
///
/// Construct the starting position with
/// [`Losalamos::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Losalamos::from_fen`](GenericPosition::from_fen). The army is the standard
/// chess pieces minus the Bishop; there is no castling, no pawn double-step, and
/// no en passant, and pawns promote on the far rank to Queen, Rook, or Knight.
pub type Losalamos = GenericPosition<Losalamos6x6, LosalamosRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Square, WideMoveKind};

    /// The canonical start FEN round-trips and grants no castling rights.
    #[test]
    fn startpos_fen_round_trips_without_castling() {
        let pos = Losalamos::startpos();
        assert_eq!(pos.to_fen(), "rnqknr/pppppp/6/6/PPPPPP/RNQKNR w - - 0 1");
        assert_eq!(pos.turn(), Color::White);
        assert!(!pos.castling().has_any(Color::White));
        assert!(!pos.castling().has_any(Color::Black));
    }

    /// The board is six by six and holds no bishop at the start.
    #[test]
    fn board_is_six_by_six_with_no_bishop() {
        use crate::geometry::Geometry;
        assert_eq!(Losalamos6x6::WIDTH, 6);
        assert_eq!(Losalamos6x6::HEIGHT, 6);
        assert_eq!(Losalamos6x6::SQUARES, 36);
        let pos = Losalamos::startpos();
        let board = pos.board();
        assert!(board.pieces(Color::White, WideRole::Bishop).is_empty());
        assert!(board.pieces(Color::Black, WideRole::Bishop).is_empty());
        // The full army: 6 pawns, 2 rooks, 2 knights, 1 queen, 1 king per side.
        assert_eq!(board.pieces(Color::White, WideRole::Pawn).count(), 6);
        assert_eq!(board.pieces(Color::White, WideRole::Rook).count(), 2);
        assert_eq!(board.pieces(Color::White, WideRole::Knight).count(), 2);
        assert_eq!(board.pieces(Color::White, WideRole::Queen).count(), 1);
        assert_eq!(board.pieces(Color::White, WideRole::King).count(), 1);
    }

    /// No pawn ever makes a double push, and playing a single push records no
    /// en-passant target.
    #[test]
    fn no_pawn_double_push_and_no_en_passant() {
        let pos = Losalamos::startpos();
        assert!(
            !pos.legal_moves()
                .into_iter()
                .any(|m| matches!(m.kind(), WideMoveKind::DoublePawnPush)),
            "Los Alamos pawns never double-step",
        );
        // The b2 pawn advances a single square (b2b3); playing it sets no ep target.
        let push = pos
            .legal_moves()
            .into_iter()
            .find(|m| m.to_uci::<Losalamos6x6>() == "b2b3")
            .expect("the single pawn push b2b3 exists at the start");
        let next = pos.play(&push);
        assert!(
            next.ep_square().is_none(),
            "no en-passant target is ever set"
        );
    }

    /// A position where standard chess would castle emits no castle move.
    #[test]
    fn never_castles() {
        let pos = Losalamos::startpos();
        let castles = pos
            .legal_moves()
            .into_iter()
            .filter(|m| {
                matches!(
                    m.kind(),
                    WideMoveKind::CastleKingside | WideMoveKind::CastleQueenside
                )
            })
            .count();
        assert_eq!(castles, 0, "Los Alamos chess never castles");
    }

    /// A pawn promoting on the far rank yields exactly Queen / Rook / Knight — never
    /// a Bishop.
    #[test]
    fn pawn_promotes_to_queen_rook_knight_only() {
        // White pawn on b5 promotes on b6; kings placed clear of the pawn.
        let pos = Losalamos::from_fen("5k/1P4/6/6/6/K5 w - - 0 1").expect("valid FEN");
        let promo_roles: alloc::vec::Vec<WideRole> = pos
            .legal_moves()
            .into_iter()
            .filter_map(|m| m.kind().promotion())
            .collect();
        assert!(!promo_roles.is_empty(), "a promotion is available");
        assert!(
            promo_roles
                .iter()
                .all(|r| matches!(r, WideRole::Queen | WideRole::Rook | WideRole::Knight)),
            "promotions are Q/R/N only, got {promo_roles:?}",
        );
        assert!(
            !promo_roles.contains(&WideRole::Bishop),
            "no bishop promotion in Los Alamos chess",
        );
    }

    /// A rook slides the full 6-file width but no further — its range is capped at
    /// the sixth-file edge (no wrap onto the next rank).
    #[test]
    fn rook_range_capped_at_the_six_file_edge() {
        // Lone white rook on a1 with kings out of the way: it reaches b1..f1 along
        // the rank (five squares) and a2..a6 up the file (five squares).
        let pos = Losalamos::from_fen("5k/6/6/6/6/R4K w - - 0 1").expect("valid FEN");
        let rook_from = Square::<Losalamos6x6>::from_file_rank(0, 0).unwrap();
        let along_rank = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Losalamos6x6>() == rook_from && m.to::<Losalamos6x6>().rank() == 0)
            .count();
        // b1..e1 are empty (f1 holds the king), so four quiet rook steps along rank 1.
        assert_eq!(along_rank, 4, "rook stops before the king on f1, no wrap");
    }
}
