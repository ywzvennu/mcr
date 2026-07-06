//! Nightrider chess (8x8) on the generic engine — **standard chess with the
//! knights replaced by Nightriders**, and nothing else changed. A Fairy-Stockfish
//! built-in (`UCI_Variant nightrider`, the standard chess base with the Knight
//! removed and a Nightrider added). Validated square-for-square against
//! Fairy-Stockfish.
//!
//! ## The Nightrider
//!
//! A **Nightrider** ([`WideRole::Nightrider`], FSF `nightrider` letter `n`, Betza
//! `NN`) is a **riding knight**: from its square it makes one knight-leap and may
//! then continue in the **same** knight-direction over empty squares — `(1,2)`,
//! then `(2,4)`, `(3,6)`, … — until it steps off the board or meets a piece,
//! capturing the first occupant on each ray (see
//! [`attacks::nightrider_attacks`]).
//! It moves and captures alike, and it gives **check** and creates **pins** along
//! its knight-rays. Everything else is ordinary chess: standard pawns (double
//! step, en passant, promotion), king, and castling on both sides.
//!
//! ## King safety — the full-verify path
//!
//! Unlike every other rider in the engine, the Nightrider rides **knight-rays**,
//! not the king's rank / file / diagonals. The generic line-based pin and
//! check-interposition machinery (`line` / `between`) is empty for a knight-ray,
//! and the cannon-family geometry fast-accept assumes every king attack travels a
//! board line — both are blind to a knight-ray rider. So this variant opts into
//! [`WideVariant::needs_full_verify`], which routes it through the per-move
//! make/unmake verify generator with the fast-accept disabled: every move is
//! re-tested by `king_safe_after`, whose reverse projection of the Nightrider's own
//! occupancy-aware, symmetric attack set sees its checks and pins exactly. This is
//! why a piece pinned to its king by a Nightrider along a knight-ray is correctly
//! frozen, and a Nightrider check may be answered by interposing on an intermediate
//! landing square.
//!
//! ## Promotion
//!
//! A pawn of either colour reaching the last rank promotes to a **Queen**, **Rook**,
//! **Bishop**, or **Nightrider** (FSF `promotionPieceTypes = q r b n`, where `n` is
//! the Nightrider) — never an ordinary Knight (there are none in this army).
//!
//! ## Confirmed starting FEN
//!
//! Nightrider chess is a FSF **built-in** derived from the standard chess base, so
//! its position is the standard chess start with the knights being Nightriders:
//!
//! ```text
//! FSF dialect: rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! mcr dialect: r****nbqkb****nr/pppppppp/8/8/8/8/PPPPPPPP/R****NBQKB****NR w KQkq - 0 1
//! ```
//!
//! In FSF the back rank's `n` is the Nightrider. mcr already names `n` the standard
//! Knight, and every single-letter base plus the `*` / `**` / `=` / `***` overflow
//! banks are exhausted, so the Nightrider takes the fifth-tier **overflow token**
//! `****n` (recycling the FSF mnemonic `n`, distinct by the `****` prefix): the
//! standard back rank `r n b q k b n r` becomes `r ****n b q k b ****n r`. The two
//! FENs are the same position; the `compare-fairy/` harness rewrites mcr's
//! `****n → n` when driving FSF. Both sides have full castling rights (`KQkq`).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Chess8x8, Geometry, PromotionConfig, Square, StandardChess, WideRole,
    WideVariant,
};
use crate::Color;

/// The confirmed Nightrider starting placement in mcr's role letters: standard
/// chess with the two knights replaced by Nightriders (`****n`), so each back rank
/// is `r ****n b q k b ****n r` and the pawns / king / rooks / queen are standard.
const NIGHTRIDER_START_PLACEMENT: &str =
    "r****nbqkb****nr/pppppppp/8/8/8/8/PPPPPPPP/R****NBQKB****NR";

/// The Nightrider-chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides only the Nightrider's movement (a riding knight), the `q r b n`
/// promotion set (`n` being the Nightrider), and opts into the per-move full
/// king-safety verification its knight-ray rides require
/// ([`needs_full_verify`](WideVariant::needs_full_verify)). Every other piece,
/// castling, the double pawn step, and en passant are the standard-chess trait
/// defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NightriderRules;

impl WideVariant<Chess8x8> for NightriderRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field (the start army Pawn / Bishop / Rook / Queen / King, and
    /// the [`WideRole::Nightrider`] at index `146`, also a promotion target); the
    /// movegen loops iterate this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = WideRole::Nightrider.index() + 1;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(NIGHTRIDER_START_PLACEMENT)
            .expect("the Nightrider starting placement is valid on an 8x8 board");
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
            // The Nightrider (Betza `NN`): rides each knight direction until blocked,
            // capturing the first piece on each ray. Symmetric and occupancy-aware,
            // so `attackers_to` / `king_safe_after` reverse-project it directly.
            WideRole::Nightrider => attacks::nightrider_attacks::<Chess8x8>(sq, occupancy),
            // Everything else (pawn, bishop, rook, queen, king) is standard chess.
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    /// Every move is verified by the per-move make/unmake king-safety re-test, with
    /// the geometry fast-accept disabled. The Nightrider rides **knight-rays**,
    /// which are not board lines: the line-based pin / interposition machinery and
    /// the rank/file/diagonal fast-accept cannot express its checks and pins, so
    /// this routes the variant through the authoritative
    /// [`king_safe_after`](crate::geometry::position::GenericPosition) verify. See
    /// [`WideVariant::needs_full_verify`].
    fn needs_full_verify() -> bool {
        true
    }

    // --- promotion: pawns -> Queen / Rook / Bishop / Nightrider ----------------

    fn promotion_config() -> PromotionConfig {
        // FSF `promotionPieceTypes = q r b n`, where `n` is the Nightrider (there is
        // no ordinary Knight in this army).
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Queen,
                WideRole::Rook,
                WideRole::Bishop,
                WideRole::Nightrider,
            ],
        }
    }

    fn has_castling() -> bool {
        true
    }

    /// The western **fifty-move rule**: a position whose halfmove clock has reached
    /// 100 plies (50 full moves with no capture or pawn move) is a
    /// [`WideEndReason::MoveRule`](crate::geometry::WideEndReason::MoveRule) draw,
    /// matching Fairy-Stockfish's default `nMoveRule = 50` for its standard-chess
    /// base. Adjudication-only (the clock never gates move generation), so perft
    /// stays byte-identical.
    fn move_rule_plies() -> Option<u16> {
        Some(100)
    }
}

/// Nightrider chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position (standard chess with Nightriders in place of the
/// knights) with [`Nightrider::startpos`](GenericPosition::startpos) or parse a FEN
/// (mcr dialect) with [`Nightrider::from_fen`](GenericPosition::from_fen). See the
/// [module docs](self) for the riding-knight movement, the full-verify king safety,
/// and the `q r b n` promotion.
pub type Nightrider = GenericPosition<Chess8x8, NightriderRules>;

#[cfg(test)]
mod tests {
    use super::*;

    /// The canonical start FEN round-trips (mcr `****n` dialect) and has 24 legal
    /// moves — four more than standard chess, one extra per Nightrider (each rides a
    /// square further than a knight from its home square).
    #[test]
    fn startpos_round_trips() {
        let pos = Nightrider::startpos();
        assert_eq!(
            pos.to_fen(),
            "r****nbqkb****nr/pppppppp/8/8/8/8/PPPPPPPP/R****NBQKB****NR w KQkq - 0 1"
        );
        assert_eq!(pos.turn(), Color::White);
        // Matches Fairy-Stockfish `UCI_Variant nightrider` startpos perft(1).
        assert_eq!(pos.legal_move_count(), 24);
    }

    /// A Nightrider on an open board reaches every square a whole number of equal
    /// knight-steps away: from a1 it rides b3/c5/d7 (the `(1,2)` ray) and c2/e3/g4
    /// (the `(2,1)` ray) — six squares no plain knight could reach.
    #[test]
    fn nightrider_rides_full_rays_on_open_board() {
        let pos = Nightrider::from_fen("4k3/8/8/8/8/8/8/****N3K3 w - - 0 1").expect("valid FEN");
        let dests: alloc::vec::Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Chess8x8>() == Square::new(0))
            .map(|m| m.to::<Chess8x8>().index())
            .collect();
        for expected in [17u8, 34, 51, 10, 20, 30] {
            // b3, c5, d7 (the (1,2) ray) and c2, e3, g4 (the (2,1) ray)
            assert!(
                dests.contains(&expected),
                "Nightrider on a1 should ride to square {expected}"
            );
        }
    }

    /// A ride is blocked by an intervening piece and captures the first enemy on the
    /// ray: with a black pawn on c5, the a1 Nightrider reaches b3 (empty) and c5
    /// (capture) but not d7 beyond it.
    #[test]
    fn ride_blocks_and_captures_first_enemy() {
        let pos = Nightrider::from_fen("4k3/8/8/2p5/8/8/8/****N3K3 w - - 0 1").expect("valid FEN");
        let dests: alloc::vec::Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Chess8x8>() == Square::new(0))
            .map(|m| m.to::<Chess8x8>().index())
            .collect();
        assert!(dests.contains(&17), "b3 (empty landing) is reachable"); // b3
        assert!(dests.contains(&34), "c5 (first enemy) is a capture"); // c5
        assert!(
            !dests.contains(&51),
            "d7 beyond the blocker is not reachable"
        ); // d7
    }

    /// A Nightrider **pins** a friendly piece to the king along a knight-ray: with
    /// the white king on e1, a white rook on d3, and a black Nightrider on c5 (the
    /// ray e1-d3-c5), the rook is frozen — the only legal moves are the five king
    /// steps. Matches Fairy-Stockfish perft(1) = 5.
    #[test]
    fn nightrider_pins_along_knight_ray() {
        let pos = Nightrider::from_fen("4k3/8/8/2****n5/8/3R4/8/4K3 w - - 0 1").expect("valid FEN");
        assert_eq!(pos.legal_move_count(), 5);
        // Every legal move is a king move; the pinned rook contributes none.
        assert!(pos
            .legal_moves()
            .into_iter()
            .all(|m| m.from::<Chess8x8>() == Square::new(4)));
    }

    /// A Nightrider **check** may be answered by **interposing** on an intermediate
    /// landing square: the white king on a1 is checked by a black Nightrider on c5
    /// (ray a1-b3-c5); a white rook on h3 can block on b3. Legal replies are the
    /// three king escapes plus Rh3-b3. Matches Fairy-Stockfish perft(1) = 4.
    #[test]
    fn nightrider_check_answered_by_interposition() {
        let pos = Nightrider::from_fen("4k3/8/8/2****n5/8/7R/8/K7 w - - 0 1").expect("valid FEN");
        let moves = pos.legal_moves();
        assert_eq!(moves.len(), 4);
        // The rook interposes on b3 (square 17), blocking the knight-ray.
        assert!(moves
            .iter()
            .any(|m| m.from::<Chess8x8>() == Square::new(23) // h3
                && m.to::<Chess8x8>() == Square::new(17))); // b3
    }

    /// A pawn promotes to a Nightrider (not an ordinary Knight): the `q r b n`
    /// promotion set offers exactly four targets on the last rank.
    #[test]
    fn pawn_promotes_to_nightrider() {
        let pos = Nightrider::from_fen("4k3/P7/8/8/8/8/8/4K3 w - - 0 1").expect("valid FEN");
        let promos: alloc::vec::Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter_map(|m| m.promotion())
            .collect();
        assert!(promos.contains(&WideRole::Nightrider));
        assert!(!promos.contains(&WideRole::Knight));
        assert_eq!(promos.len(), 4, "q r b n promotion targets");
    }
}
