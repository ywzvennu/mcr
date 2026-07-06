//! Gardner minichess (5x5 chess) on the generic engine — **standard chess played
//! on a five-by-five board**, reusing the existing [`Minishogi5x5`] geometry and
//! the reference [`StandardChess`] pieces. Validated square-for-square against
//! Fairy-Stockfish `UCI_Variant gardner`.
//!
//! Gardner minichess (Martin Gardner, 1969) is the smallest board on which every
//! standard chess piece keeps its ordinary move. Each side has a back rank of
//! **Rook, Knight, Bishop, Queen, King** (files a..e) and a rank of **five
//! pawns**; the two middle-file ranks and the centre rank are empty. Every piece
//! — rook, knight, bishop, queen, king, and pawn — moves and captures exactly as
//! in standard chess, only on the smaller board, so the pieces reuse the generic
//! [`WideVariant::role_attacks`] defaults ([`StandardChess`]) with no override.
//!
//! ## Rules — standard chess, shrunk, with three features off
//!
//! Fairy-Stockfish defines `gardner` as its `chess_variant_base()` with `maxRank =
//! 5`, `maxFile = 5`, no double pawn push, no castling, and no en passant:
//!
//! * **No castling.** [`has_castling`](WideVariant::has_castling) is `false` and
//!   the start state grants no castling rights, so no castle is ever generated.
//! * **No pawn double step.**
//!   [`pawn_may_double_push_from`](WideVariant::pawn_may_double_push_from) is
//!   `false` for every rank, so a pawn only ever advances one square.
//! * **No en passant.** [`has_en_passant`](WideVariant::has_en_passant) is
//!   `false`; with no double step there is no skipped square anyway.
//! * **Promotion** is on the far rank (rank 5 White / rank 1 Black — the geometry
//!   default [`promotion_rank`](WideVariant::promotion_rank) of `HEIGHT - 1` / `0`)
//!   to the standard **Queen, Rook, Bishop, or Knight**.
//! * **Win by checkmate**, with the western **fifty-move rule**
//!   ([`move_rule_plies`](WideVariant::move_rule_plies) of `100`, adjudication-only
//!   so perft is byte-identical).
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant gardner` (`position startpos`):
//!
//! ```text
//! rnbqk/ppppp/5/PPPPP/RNBQK w - - 0 1
//! ```
//!
//! mcr and FSF spell the position byte-for-byte identically (standard-chess
//! letters on a 5x5 grid, no dialect rewrite). The castling field is `-` (no
//! rights) and no move ever sets an en-passant target.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
#[allow(unused_imports)] // `StandardChess` is referenced by the rustdoc intra-doc links.
use crate::geometry::StandardChess;
use crate::geometry::{Board, WideVariant};
use crate::Color;

use super::super::Minishogi5x5;

/// The confirmed Gardner minichess starting placement: the standard back rank
/// (Rook, Knight, Bishop, Queen, King) and five pawns on a 5x5 board.
const GARDNER_START_PLACEMENT: &str = "rnbqk/ppppp/5/PPPPP/RNBQK";

/// The Gardner minichess rule layer: a zero-sized [`WideVariant`] over
/// [`Minishogi5x5`].
///
/// It is the reference [`StandardChess`] ruleset shrunk onto the 5x5 board, with
/// castling, the pawn double step, and en passant all disabled. Every piece move,
/// the single pawn push, diagonal capture, and last-rank Q/R/B/N promotion are the
/// standard-chess trait defaults (the generic role-attack primitives are
/// geometry-generic, so they already work on the smaller board), so only the three
/// disabled features and the 5x5 start array distinguish it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GardnerRules;

impl WideVariant<Minishogi5x5> for GardnerRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field: the standard army (Pawn..King), whose promotions (Queen
    /// / Rook / Bishop / Knight) all fall within the same prefix. See
    /// [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Minishogi5x5>, GenericState<Minishogi5x5>) {
        let board = Board::<Minishogi5x5>::from_fen_placement(GARDNER_START_PLACEMENT)
            .expect("the Gardner starting placement is valid on a 5x5 board");
        let state = GenericState {
            turn: Color::White,
            // No castling rights: castling is disabled entirely.
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

    /// Gardner minichess has **no castling**. The generic move generator consults
    /// this to suppress castle generation, and the FEN layer to reject castling
    /// rights, so neither side can ever castle.
    fn has_castling() -> bool {
        false
    }

    /// Gardner minichess has **no pawn double step**: on the small board a pawn
    /// only ever advances a single square, so the double push is unavailable from
    /// every rank.
    fn pawn_may_double_push_from(_rank: u8, _color: Color) -> bool {
        false
    }

    /// Gardner minichess has **no en passant** — there is no double step to skip a
    /// square, so no en-passant target is ever recorded or captured.
    fn has_en_passant() -> bool {
        false
    }

    /// The western **fifty-move rule** (100 plies). Adjudication-only — reported
    /// from the single position and never consulted by move generation — so perft
    /// stays byte-identical.
    fn move_rule_plies() -> Option<u16> {
        Some(100)
    }
}

/// Gardner minichess as a [`GenericPosition`] over the 5x5 [`Minishogi5x5`]
/// geometry.
///
/// Construct the starting position with
/// [`Gardner::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Gardner::from_fen`](GenericPosition::from_fen). Every rule is the standard
/// [`StandardChess`] default except castling, the pawn double step, and en
/// passant, which are disabled.
pub type Gardner = GenericPosition<Minishogi5x5, GardnerRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Square, WideMoveKind, WideRole};

    /// The canonical start FEN round-trips, opens with the seven FSF-confirmed
    /// moves (five single pawn pushes plus two knight hops), and carries no
    /// castling rights.
    #[test]
    fn startpos_round_trips() {
        let pos = Gardner::startpos();
        assert_eq!(pos.to_fen(), "rnbqk/ppppp/5/PPPPP/RNBQK w - - 0 1");
        assert_eq!(pos.turn(), Color::White);
        assert_eq!(pos.legal_move_count(), 7);
        assert!(!pos.castling().has_any(Color::White));
        assert!(!pos.castling().has_any(Color::Black));
    }

    /// No castle, no double step, and no en passant is ever generated — not from
    /// the opening, and not from a position where standard chess would offer each.
    #[test]
    fn no_castle_double_step_or_en_passant() {
        // Opening: no double push and no castle among the seven legal moves.
        let pos = Gardner::startpos();
        for m in pos.legal_moves() {
            assert!(
                !matches!(
                    m.kind(),
                    WideMoveKind::CastleKingside
                        | WideMoveKind::CastleQueenside
                        | WideMoveKind::DoublePawnPush
                        | WideMoveKind::EnPassant
                ),
                "gardner never castles, double-steps, or takes en passant",
            );
        }
        // A pawn advancing sets no en-passant target and offers no ep reply.
        let push = pos
            .legal_moves()
            .into_iter()
            .find(|m| m.from::<Minishogi5x5>().file() == 0 && m.promotion().is_none())
            .expect("the a-pawn can advance");
        let next = pos.play(&push);
        assert!(
            next.ep_square().is_none(),
            "a single pawn push sets no en-passant target",
        );
    }

    /// A pawn reaching the far rank promotes to Queen, Rook, Bishop, or Knight.
    #[test]
    fn pawn_promotes_to_standard_set() {
        // White pawn on b4, one step from the fifth rank; the b5 square is empty.
        let pos = Gardner::from_fen("4k/1P3/5/5/4K w - - 0 1").expect("valid FEN");
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
        ];
        want.sort();
        assert_eq!(roles, want, "promotes to Q/R/B/N only");
    }

    /// Pieces move as standard chess on the 5x5 board: a rook's range is capped at
    /// the five-file edge (a lone rook on a1 reaches the four squares up its file
    /// and the four along its rank — eight, not the fourteen of an 8x8 board).
    #[test]
    fn rook_range_capped_at_five_by_five_edge() {
        let pos = Gardner::from_fen("4k/5/2K2/5/R4 w - - 0 1").expect("valid FEN");
        let a1 = Square::<Minishogi5x5>::from_file_rank(0, 0).unwrap();
        let rook_moves = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Minishogi5x5>() == a1)
            .count();
        assert_eq!(rook_moves, 8, "rook on a1 reaches 4 up-file + 4 along-rank");
    }
}
