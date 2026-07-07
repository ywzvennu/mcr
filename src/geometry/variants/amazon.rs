//! Amazon Chess (8x8) on the generic engine — standard chess with the **Queen
//! replaced by an Amazon** (Queen + Knight). See
//! <https://www.chessvariants.com/large.dir/amazonchess.html>. Validated
//! square-for-square against Fairy-Stockfish `UCI_Variant amazon`.
//!
//! Every rule is standard chess — the 8x8 board, standard pawns (double step, en
//! passant), standard castling — except the piece on the queen's square (d-file):
//!
//! * **Amazon** (Queen + Knight) — mcr's [`WideRole::Angel`]. Its movement is the
//!   union of a queen's slides and a knight's leaps; it is a genuinely-new mover on
//!   the 8x8 path (the trait default has no Amazon), so this variant supplies its
//!   [`role_attacks`](WideVariant::role_attacks). FEN token `**a`/`**A` in the mcr
//!   dialect (the second-bank overflow token the Angel shares with Mansindam;
//!   Fairy-Stockfish spells the amazon `a`/`A`, a dialect difference the
//!   `compare-fairy/` harness reconciles).
//!
//! There is **no Queen** in Amazon Chess, so a pawn promotes to an **Amazon,
//! Rook, Bishop, or Knight** (FSF `promotionPieceTypes = a r b n`) — never a
//! Queen.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant amazon` (`position startpos`):
//!
//! ```text
//! FSF dialect: rnbakbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBAKBNR w KQkq - 0 1
//! mcr dialect: rnb**akbnr/pppppppp/8/8/8/8/PPPPPPPP/RNB**AKBNR w KQkq - 0 1
//! ```
//!
//! The two strings are the same position; the amazon is `a`/`A` in FSF and the
//! second-bank overflow `**a`/`**A` in mcr. Every other piece, castling, the
//! double pawn step, and en passant are standard chess.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Chess8x8, Geometry, PromotionConfig, Square, StandardChess, WideRole,
    WideVariant,
};
use crate::Color;

/// The confirmed Amazon Chess starting placement in the mcr dialect (amazon =
/// `**a`/`**A`): standard chess with the queen replaced by the Amazon.
const AMAZON_START_PLACEMENT: &str = "rnb**akbnr/pppppppp/8/8/8/8/PPPPPPPP/RNB**AKBNR";

/// The Amazon Chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides the starting array (Amazon on the queen's square), the Amazon's
/// movement (Queen + Knight), and the promotion set (`a r b n`, no Queen). Every
/// other piece, castling, the double pawn step, and en passant are standard chess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct AmazonRules;

impl WideVariant<Chess8x8> for AmazonRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 68;

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
        let board = Board::<Chess8x8>::from_fen_placement(AMAZON_START_PLACEMENT)
            .expect("the Amazon Chess starting placement is valid on an 8x8 board");
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
        };
        (board, state)
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Chess8x8>,
        occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        match role {
            // Amazon (Queen + Knight): a queen's slides plus the eight knight leaps.
            WideRole::Angel => {
                attacks::queen_attacks::<Chess8x8>(sq, occupancy)
                    | attacks::knight_attacks::<Chess8x8>(sq)
            }
            // Everything else is standard chess.
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn role_is_slider(role: WideRole) -> bool {
        match role {
            // The Amazon slides along the queen lines, so it can pin and be pinned.
            WideRole::Angel => true,
            _ => <StandardChess as WideVariant<Chess8x8>>::role_is_slider(role),
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // The Amazon's attack set (queen slides + knight leaps) is geometrically
        // symmetric, so only the pawn is colour-directional.
        matches!(role, WideRole::Pawn)
    }

    fn promotion_config() -> PromotionConfig {
        // FSF `promotionPieceTypes = a r b n`: the Amazon plus rook, bishop,
        // knight — no Queen. Order matches FSF's promotion set; it affects only
        // move enumeration order, not the perft leaf count.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Angel, // Amazon (Q+N)
                WideRole::Rook,
                WideRole::Bishop,
                WideRole::Knight,
            ],
        }
    }

    fn has_castling() -> bool {
        true
    }

    /// Amazon Chess keeps the standard chess army (bar the Queen) plus the
    /// always-mating Amazon ([`WideRole::Angel`]), so the ordinary
    /// insufficient-material draw applies: king vs king, king and a lone minor
    /// (bishop or knight) vs king, and same-colour bishops only. The Amazon counts
    /// as mating material. Adjudication-only and behind the default-off hook, so
    /// perft stays byte-identical.
    fn is_insufficient_material<const R: usize>(
        board: &Board<Chess8x8, R>,
        _state: &GenericState<Chess8x8, R>,
    ) -> bool {
        crate::geometry::variant::standard_insufficient_material(board)
    }
}

/// Amazon Chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Amazon::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Amazon::from_fen`](GenericPosition::from_fen). Only the array, the Amazon's
/// Queen + Knight movement, and the no-Queen promotion set distinguish it from
/// standard chess.
pub type Amazon =
    GenericPosition<Chess8x8, AmazonRules, { <AmazonRules as WideVariant<Chess8x8>>::ROLE_SPAN }>;

#[cfg(test)]
mod tests {
    use super::*;

    /// The canonical start FEN round-trips and opens with the FSF-confirmed 22
    /// moves (the standard 20 plus the two amazon knight-hops from d1).
    #[test]
    fn startpos_round_trips() {
        let pos = Amazon::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnb**akbnr/pppppppp/8/8/8/8/PPPPPPPP/RNB**AKBNR w KQkq - 0 1"
        );
        assert_eq!(pos.legal_move_count(), 22);
    }

    /// A lone Amazon on an open board reaches queen rays plus the eight knight
    /// leaps (27 queen moves from d4 on an 8x8 board + 8 knight = 35).
    #[test]
    fn amazon_moves_as_queen_plus_knight() {
        let pos = Amazon::from_fen("8/8/8/8/3**A4/8/K7/7k w - - 0 1").expect("valid");
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
        let pos = Amazon::from_fen("4k3/1P6/8/8/8/8/8/4K3 w - - 0 1").expect("valid");
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
