//! Georgian Chess (8x8) on the generic engine — the [`Amazon`](super::Amazon)
//! army with **no castling** and **no en passant**, and nothing else changed.
//! Validated square-for-square against Fairy-Stockfish `UCI_Variant georgian`.
//!
//! Fairy-Stockfish defines `georgian` as its `amazon_variant()` with three
//! overrides: the start array places the Amazon on the queen's square with **no
//! castling rights**, `castling = false`, and `enPassantRegion = 0` for both
//! colours. So Georgian is exactly mcr's [`Amazon`](super::Amazon) — the queen
//! replaced by the **Amazon** (Queen + Knight, mcr's [`WideRole::Angel`]),
//! promoting to Amazon / Rook / Bishop / Knight (never a Queen) — with only two
//! rules removed:
//!
//! * **No castling.** [`has_castling`](WideVariant::has_castling) is `false` and
//!   the start state grants no castling rights, so no castle is ever generated.
//! * **No en passant.** [`has_en_passant`](WideVariant::has_en_passant) is
//!   `false`. The pawn **double step stays** — a pawn may still advance two
//!   squares from its start rank — but the double step records **no** en-passant
//!   target (the FEN ep field stays `-`) and no en-passant capture is ever
//!   offered on the reply.
//!
//! Every other rule — the 8x8 board, the Amazon's Queen + Knight movement, the
//! standard pawns (double step, diagonal capture, promotion to the amazon set),
//! and standard checkmate — is identical to [`Amazon`](super::Amazon).
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant georgian`
//! (`position startpos`):
//!
//! ```text
//! FSF dialect: rnbakbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBAKBNR w - - 0 1
//! mcr dialect: rnb**akbnr/pppppppp/8/8/8/8/PPPPPPPP/RNB**AKBNR w - - 0 1
//! ```
//!
//! The two strings are the same position; the amazon is `a`/`A` in FSF and the
//! second-bank overflow `**a`/`**A` in mcr. The castling field is `-` (no
//! rights) and no double step will ever set an ep target.

use super::AmazonRules;
use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{Bitboard, Board, Chess8x8, PromotionConfig, Square, WideRole, WideVariant};
use crate::Color;

/// The confirmed Georgian Chess starting placement in the mcr dialect (amazon =
/// `**a`/`**A`): the Amazon army with no castling rights.
const GEORGIAN_START_PLACEMENT: &str = "rnb**akbnr/pppppppp/8/8/8/8/PPPPPPPP/RNB**AKBNR";

/// The Georgian Chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It is the [`Amazon`](super::Amazon) ruleset with exactly two changes —
/// castling and en passant are both disabled. It reuses Amazon's Amazon-piece
/// movement, slider/directionality flags, promotion set, and draw hooks; only
/// the starting castling rights (none) and the two feature flags differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GeorgianRules;

impl WideVariant<Chess8x8> for GeorgianRules {
    /// The same role span as [`Amazon`](super::Amazon) — the standard army plus
    /// the Amazon ([`WideRole::Angel`]). See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = <AmazonRules as WideVariant<Chess8x8>>::ROLE_SPAN;

    /// The western **fifty-move rule**, as in [`Amazon`](super::Amazon):
    /// adjudication-only, so perft stays byte-identical.
    fn move_rule_plies() -> Option<u16> {
        <AmazonRules as WideVariant<Chess8x8>>::move_rule_plies()
    }

    /// Records a position history for the standard **threefold** repetition draw,
    /// as in [`Amazon`](super::Amazon). History-dependent and never consulted by a
    /// bare position, so perft is unchanged.
    fn tracks_repetition() -> bool {
        <AmazonRules as WideVariant<Chess8x8>>::tracks_repetition()
    }

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(GEORGIAN_START_PLACEMENT)
            .expect("the Georgian Chess starting placement is valid on an 8x8 board");
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
            checks_against: [0, 0],
        };
        (board, state)
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Chess8x8>,
        occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        // The Amazon (Queen + Knight) and every other piece move exactly as in
        // Amazon Chess.
        <AmazonRules as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy)
    }

    fn role_is_slider(role: WideRole) -> bool {
        <AmazonRules as WideVariant<Chess8x8>>::role_is_slider(role)
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        <AmazonRules as WideVariant<Chess8x8>>::role_attack_is_directional(role)
    }

    fn promotion_config() -> PromotionConfig {
        // Inherits Amazon's `a r b n` (Amazon / Rook / Bishop / Knight, no Queen).
        <AmazonRules as WideVariant<Chess8x8>>::promotion_config()
    }

    /// Georgian Chess has **no castling**. The generic move generator consults
    /// this to suppress castle generation, and the FEN layer to reject castling
    /// rights, so neither side can ever castle.
    fn has_castling() -> bool {
        false
    }

    /// Georgian Chess has **no en passant** (`enPassantRegion = 0`). The pawn
    /// double step stays, but records no ep target and offers no ep capture.
    fn has_en_passant() -> bool {
        false
    }

    /// The ordinary insufficient-material draw, as in [`Amazon`](super::Amazon)
    /// (the Amazon counts as mating material). Adjudication-only and behind the
    /// default-off hook, so perft stays byte-identical.
    fn is_insufficient_material<const R: usize>(
        board: &Board<Chess8x8, R>,
        state: &GenericState<Chess8x8, R>,
    ) -> bool {
        <AmazonRules as WideVariant<Chess8x8>>::is_insufficient_material(board, state)
    }
}

/// Georgian Chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Georgian::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Georgian::from_fen`](GenericPosition::from_fen). It is the
/// [`Amazon`](super::Amazon) ruleset with castling and en passant removed.
pub type Georgian = GenericPosition<
    Chess8x8,
    GeorgianRules,
    { <GeorgianRules as WideVariant<Chess8x8>>::ROLE_SPAN },
>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::WideMoveKind;

    /// The canonical start FEN round-trips and opens with the 22 amazon moves
    /// (the standard 20 plus the two amazon knight-hops from d1) — no castling
    /// rights in the FEN.
    #[test]
    fn startpos_round_trips() {
        let pos = Georgian::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnb**akbnr/pppppppp/8/8/8/8/PPPPPPPP/RNB**AKBNR w - - 0 1"
        );
        assert_eq!(pos.legal_move_count(), 22);
        assert!(!pos.castling().has_any(Color::White));
        assert!(!pos.castling().has_any(Color::Black));
    }

    /// A position where standard/amazon chess would castle emits no castle move:
    /// king on e1 with both rooks on a1/h1 and an empty back rank produces king
    /// steps only, never a `CastleKingside` / `CastleQueenside`.
    #[test]
    fn no_castle_move_when_amazon_would() {
        let pos = Georgian::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w - - 0 1").expect("valid FEN");
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
        assert_eq!(castles, 0, "Georgian chess never castles");
    }

    /// A double pawn push still happens, but records **no** en-passant target and
    /// offers the opponent **no** en-passant capture.
    #[test]
    fn double_step_stays_but_no_en_passant() {
        // White pawn on e2 next to a black pawn on d4: a double step to e4 would,
        // in standard/amazon chess, let d4xe3 e.p. Here it must not.
        let pos = Georgian::from_fen("4k3/8/8/8/3p4/8/4P3/4K3 w - - 0 1").expect("valid FEN");
        let dbl = pos
            .legal_moves()
            .into_iter()
            .find(|m| matches!(m.kind(), WideMoveKind::DoublePawnPush))
            .expect("the e2 pawn can advance two squares");
        let next = pos.play(&dbl);
        assert!(
            next.ep_square().is_none(),
            "a double push in Georgian sets no en-passant target",
        );
        assert_eq!(
            next.to_fen(),
            "4k3/8/8/8/3pP3/8/8/4K3 b - - 0 1",
            "the FEN ep field stays `-`",
        );
        let ep_captures = next
            .legal_moves()
            .into_iter()
            .filter(|m| matches!(m.kind(), WideMoveKind::EnPassant))
            .count();
        assert_eq!(ep_captures, 0, "no en-passant capture is ever offered");
    }

    /// The Amazon moves as Queen + Knight (27 queen moves from d4 + 8 knight).
    #[test]
    fn amazon_moves_as_queen_plus_knight() {
        let pos = Georgian::from_fen("8/8/8/8/3**A4/8/K7/7k w - - 0 1").expect("valid");
        let sq = Square::<Chess8x8>::from_file_rank(3, 3).unwrap();
        let n = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Chess8x8>() == sq)
            .count();
        assert_eq!(n, 35, "amazon = queen (27 from d4) + knight (8)");
    }

    /// A pawn promotes to Amazon / Rook / Bishop / Knight — never a Queen.
    #[test]
    fn pawn_promotes_without_queen() {
        let pos = Georgian::from_fen("4k3/1P6/8/8/8/8/8/4K3 w - - 0 1").expect("valid");
        let mut roles: alloc::vec::Vec<WideRole> = pos
            .legal_moves()
            .into_iter()
            .filter_map(|m| m.promotion())
            .collect();
        roles.sort();
        roles.dedup();
        let mut want = alloc::vec![
            WideRole::Angel,
            WideRole::Rook,
            WideRole::Bishop,
            WideRole::Knight,
        ];
        want.sort();
        assert_eq!(roles, want);
        assert!(!roles.contains(&WideRole::Queen), "no queen promotion");
    }
}
