//! Berolina chess (8x8) on the generic engine — **standard chess with an inverted
//! pawn**. Validated against Fairy-Stockfish `UCI_Variant berolina` (a built-in;
//! the Berolina pawn is FSF Betza `mfFcfeWimfnA`).
//!
//! Berolina chess keeps every standard chess piece, the standard starting array,
//! standard castling, and promotion to Queen / Rook / Bishop / Knight. Its one
//! difference is the pawn, which is the **mirror image** of the ordinary chess
//! pawn:
//!
//! * **it moves (non-capturing) one square *diagonally* forward** — either forward
//!   diagonal (FSF `mfF`, a move-only forward Ferz);
//! * **it captures one square *straight* forward** — the orthogonal square directly
//!   ahead (FSF `cfeW`, a capture-only forward Wazir);
//! * **its initial double advance is two squares *diagonally* forward** along the
//!   same diagonal, from the second rank (FSF `imfnA`, an initial move-only forward
//!   *lame* Alfil): a jump blocked when the single intervening diagonal square is
//!   occupied;
//! * **en passant applies to that diagonal double step** (FSF `enPassantTypes`): an
//!   enemy Berolina pawn positioned to capture straight-forward onto the *skipped*
//!   diagonal square may take the double-stepped pawn en passant;
//! * **promotion is standard** — reaching the last rank (by the diagonal move or the
//!   straight capture) promotes to Q / R / B / N.
//!
//! The board symbol stays `p` / `P` like an ordinary pawn — the inversion is a
//! *rule*, not a letter.
//!
//! ## How the inversion is expressed
//!
//! Two hooks over standard chess, both defaulting to the ordinary pawn so every
//! other variant is byte-identical:
//!
//! * [`role_attacks`](WideVariant::role_attacks) for the Pawn returns the **single
//!   straight-forward square** — the square it captures onto, and therefore the only
//!   square from which it gives check or contributes to king-danger. Its diagonal
//!   *move* is not an attack (it can never capture there), so it correctly gives no
//!   check.
//! * [`pawn_is_berolina`](WideVariant::pawn_is_berolina) switches the standard
//!   single-king pawn generator to the Berolina geometry: the diagonal quiet advance
//!   and lame diagonal double step (the forward-diagonal squares come from
//!   [`berolina_push_targets`](WideVariant::berolina_push_targets)), the straight
//!   capture (driven by the `role_attacks` override above), the diagonal-double-step
//!   en passant, and last-rank promotion by either.
//!
//! The generic engine records the en-passant victim (the diagonally offset
//! double-stepped pawn, which the single skipped square cannot locate) in
//! `GenericState::ep_captured`, set at the double step and read by the en-passant
//! capture — `None` for every ordinary-pawn variant.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant berolina`:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! ```
//!
//! Because the pawns move diagonally from the first ply, the perft counts differ
//! from standard chess immediately (startpos perft 1 / 2 / 3 = `30` / `900` /
//! `28328`, versus standard chess's `20` / `400` / `8902`).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{Bitboard, Board, Chess8x8, Square, StandardChess, WideRole, WideVariant};
use crate::Color;

/// The standard 8x8 starting placement (Berolina shares the chess array).
const BEROLINA_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

/// The Berolina chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides only what Berolina changes about standard chess: the Pawn's attack
/// pattern (straight forward, via [`WideVariant::role_attacks`]) and the Berolina
/// pawn geometry switch ([`WideVariant::pawn_is_berolina`] plus the forward-diagonal
/// quiet targets of [`WideVariant::berolina_push_targets`]). Every other piece's
/// movement, castling, promotion set, and the 50-move rule are the standard trait
/// defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BerolinaRules;

impl WideVariant<Chess8x8> for BerolinaRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field: the standard army `Pawn..King` and the standard promotion
    /// targets (Knight / Bishop / Rook / Queen), all within `0..=5`. See
    /// [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(BEROLINA_START_PLACEMENT)
            .expect("the Berolina starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::standard::<Chess8x8>(),
            ep_square: None,
            ep_captured: None,
            gating: GenericGating::NONE,
            duck: None,
            placement: GenericPlacement::NONE,
            halfmove_clock: 0,
            fullmove_number: 1,
            consecutive_passes: 0,
            board_b: Bitboard::EMPTY,
            petrified: Bitboard::EMPTY,
            checks_against: [0, 0],
            jieqi_seed: None,
        };
        (board, state)
    }

    /// The Berolina pawn **captures — and so attacks / checks — one square straight
    /// forward**. Returning only that square (never the diagonals) makes king-safety
    /// see the pawn's real threat and leaves its diagonal *move* a non-attacking
    /// quiet advance. Every other role keeps the standard pattern.
    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Chess8x8>,
        occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        match role {
            WideRole::Pawn => {
                let forward: i8 = if color.is_white() { 1 } else { -1 };
                let mut bb = Bitboard::<Chess8x8>::EMPTY;
                if let Some(dest) = sq.offset(0, forward) {
                    bb.set(dest);
                }
                bb
            }
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    /// The Pawn (`WideRole::Pawn`) is a Berolina pawn on the standard single-king
    /// path: diagonal advance, straight capture, diagonal-double-step en passant,
    /// standard promotion. Drives the Berolina branches of the generic pawn
    /// generator and apply.
    fn pawn_is_berolina() -> bool {
        true
    }

    /// The forward-diagonal quiet-advance targets of a Berolina pawn: the two
    /// squares one step diagonally forward. The generic pawn generator emits the
    /// single step onto an empty one of these, and the lame two-square jump along
    /// the same diagonal from the start rank.
    fn berolina_push_targets(color: Color, from: Square<Chess8x8>) -> Bitboard<Chess8x8> {
        let forward: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = Bitboard::<Chess8x8>::EMPTY;
        for df in [-1, 1] {
            if let Some(dest) = from.offset(df, forward) {
                bb.set(dest);
            }
        }
        bb
    }

    /// The western **fifty-move rule**: a position whose halfmove clock has reached
    /// 100 plies (50 full moves with no capture or pawn move) is a
    /// [`WideEndReason::MoveRule`](crate::geometry::WideEndReason::MoveRule) draw,
    /// matching Fairy-Stockfish's default `nMoveRule = 50` for this standard-army
    /// board. Adjudication-only (the clock never gates move generation), so perft
    /// stays byte-identical.
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
}

/// Berolina chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Berolina::startpos`](GenericPosition::startpos) or parse a plain-chess FEN with
/// [`Berolina::from_fen`](GenericPosition::from_fen). Movement is standard chess with
/// the inverted (diagonal-move, straight-capture) Berolina pawn.
pub type Berolina = GenericPosition<
    Chess8x8,
    BerolinaRules,
    { <BerolinaRules as WideVariant<Chess8x8>>::ROLE_SPAN },
>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Square;

    fn sq(file: u8, rank: u8) -> Square<Chess8x8> {
        Square::<Chess8x8>::from_file_rank(file, rank).unwrap()
    }

    /// The canonical start FEN round-trips with standard castling rights, and the
    /// diagonal-moving pawns give a first-ply count that differs from standard chess.
    #[test]
    fn startpos_fen_and_move_count() {
        let pos = Berolina::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
        assert_eq!(pos.turn(), Color::White);
        // Four knight moves + 26 diagonal pawn moves (single + double diagonal
        // advances) = 30, matching Fairy-Stockfish's berolina startpos perft 1.
        assert_eq!(pos.legal_move_count(), 30);
    }

    /// A Berolina pawn's non-capturing move is diagonal and its capture is straight:
    /// a lone pawn generates its two forward diagonals as quiet moves and takes a
    /// piece standing straight ahead.
    #[test]
    fn diagonal_move_and_straight_capture() {
        // White pawn d4, enemy knight straight ahead on d5, empty diagonals c5/e5.
        let pos = Berolina::from_fen("4k3/8/8/3n4/3P4/8/8/4K3 w - - 0 1").expect("valid FEN");
        let d4 = sq(3, 3);
        let moves: alloc::vec::Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Chess8x8>() == d4)
            .collect();
        // Two diagonal quiet advances (c5, e5) and the straight capture (d5).
        assert!(
            moves
                .iter()
                .any(|m| m.to::<Chess8x8>() == sq(2, 4) && !m.is_capture()),
            "diagonal quiet advance to c5",
        );
        assert!(
            moves
                .iter()
                .any(|m| m.to::<Chess8x8>() == sq(4, 4) && !m.is_capture()),
            "diagonal quiet advance to e5",
        );
        assert!(
            moves
                .iter()
                .any(|m| m.to::<Chess8x8>() == sq(3, 4) && m.is_capture()),
            "straight capture onto d5",
        );
        // It never captures diagonally, and never moves straight onto an empty square.
        assert_eq!(moves.len(), 3, "exactly c5, e5, and dxd5");
    }

    /// The lame diagonal double step is blocked when the intervening square is
    /// occupied, and creates an en-passant target the enemy captures straight onto —
    /// removing the diagonally offset double-stepped pawn.
    #[test]
    fn double_step_and_en_passant() {
        // White pawn c2 double-steps c2-e4 (through d3). Black pawn on d4 then
        // captures straight forward (down) onto d3 en passant, removing e4.
        let pos = Berolina::from_fen("4k3/8/8/8/3p4/8/2P5/4K3 w - - 0 1").expect("valid FEN");
        let c2e4 = pos
            .legal_moves()
            .into_iter()
            .find(|m| m.from::<Chess8x8>() == sq(2, 1) && m.to::<Chess8x8>() == sq(4, 3))
            .expect("c2-e4 diagonal double step is legal");
        let after = pos.play(&c2e4);
        // The skipped diagonal square d3 is the en-passant target.
        assert_eq!(
            after.ep_square(),
            Some(sq(3, 2)),
            "ep target is the skipped d3"
        );
        // The FEN carries the Fairy-Stockfish two-square form `<skipped><captured>`
        // (`d3e4`), and re-parsing it restores the exact victim so the ep round-trips.
        assert_eq!(after.to_fen(), "4k3/8/8/8/3pP3/8/8/4K3 b - d3e4 0 1");
        let reparsed = Berolina::from_fen(&after.to_fen()).expect("two-square ep FEN parses");
        assert_eq!(
            reparsed.to_fen(),
            after.to_fen(),
            "the two-square ep FEN round-trips"
        );

        let ep = after
            .legal_moves()
            .into_iter()
            .find(|m| {
                m.from::<Chess8x8>() == sq(3, 3)
                    && m.to::<Chess8x8>() == sq(3, 2)
                    && matches!(m.kind(), crate::geometry::WideMoveKind::EnPassant)
            })
            .expect("d4xd3 en passant is legal");
        let done = after.play(&ep);
        assert_eq!(
            done.board().pieces(Color::White, WideRole::Pawn).count(),
            0,
            "the en-passant capture removed White's pawn on e4",
        );
        assert_eq!(
            done.board().piece_at(sq(3, 2)).map(|p| p.color),
            Some(Color::Black),
            "the Black pawn now stands on d3",
        );
    }

    /// The double step is *lame*: a piece on the intervening diagonal square blocks it.
    #[test]
    fn double_step_is_lame() {
        // White pawn b2 with a Black knight on c3 (the intervening square of b2-d4).
        let pos = Berolina::from_fen("4k3/8/8/8/8/2n5/1P6/4K3 w - - 0 1").expect("valid FEN");
        let b2 = sq(1, 1);
        let moves: alloc::vec::Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Chess8x8>() == b2)
            .collect();
        // c3 is occupied (not an empty diagonal), so the only pawn move is a3 — the
        // double step to d4 is blocked by the piece on c3, and c3 is not a straight
        // capture square.
        assert_eq!(moves.len(), 1, "only b2-a3");
        assert_eq!(moves[0].to::<Chess8x8>(), sq(0, 2), "the sole move is a3");
    }

    /// A Berolina pawn gives check along the straight-forward line; its diagonal move
    /// into the square directly in front of the king is not a check (and cannot
    /// capture the king there).
    #[test]
    fn checks_straight_not_diagonal() {
        // White pawn e6, Black king e7: the pawn attacks straight forward onto e7, so
        // Black is in check.
        let checking = Berolina::from_fen("8/4k3/4P3/8/8/8/8/4K3 b - - 0 1").expect("valid FEN");
        assert!(
            checking.is_check(),
            "the pawn checks straight forward onto e7"
        );

        // White pawn d6, Black king e7: the pawn's diagonal move would reach e7 but a
        // Berolina pawn cannot capture diagonally, so this is not a check.
        let not_checking =
            Berolina::from_fen("8/4k3/3P4/8/8/8/8/4K3 b - - 0 1").expect("valid FEN");
        assert!(
            !not_checking.is_check(),
            "the diagonal move gives no check (the pawn captures only straight)",
        );
    }
}
