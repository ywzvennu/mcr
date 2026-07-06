//! Legan chess (8x8) on the generic engine — **standard chess pieces on a corner
//! diagonal army with a directional pawn and an L-shaped corner promotion region**.
//! Validated square-for-square against Fairy-Stockfish `UCI_Variant legan` (a
//! built-in; the Legan pawn is FSF Betza `mflFcflW`).
//!
//! Both armies are arrayed along a diagonal so each side attacks toward the
//! *opposite* corner: White marches up-and-left toward `a8`, Black down-and-right
//! toward `h1`. Every non-pawn piece (King, Queen, Rook, Bishop, Knight) moves
//! exactly as in standard chess; only the **pawn** and the **setup / promotion
//! geometry** are special.
//!
//! ## The Legan pawn
//!
//! The pawn is *directional* rather than forward-facing. For White (mirrored for
//! Black):
//!
//! * **it moves (non-capturing) one square diagonally up-left** — toward the far
//!   `a8` corner (FSF `mflF`, a move-only forward-left Ferz);
//! * **it captures one square along either orthogonal that makes up that diagonal** —
//!   straight up (north) *or* straight left (west) (FSF `cflW`, a capture-only
//!   forward-left Wazir);
//! * **there is no double step and no en passant** (FSF `doubleStep = false`).
//!
//! Black is the mirror: it moves one square diagonally down-right (toward `h1`) and
//! captures straight down (south) or straight right (east).
//!
//! ## The L-shaped corner promotion region
//!
//! A pawn promotes on a **set of squares**, not a rank. White promotes on the
//! upper-left corner — the top edge's left half plus the left edge's upper half:
//! `{a8, b8, c8, d8, a7, a6, a5}`. Black promotes on the mirrored lower-right corner:
//! `{e1, f1, g1, h1, h2, h3, h4}`. Promotion (to Queen / Rook / Bishop / Knight) is
//! therefore possible on squares that are **not the last rank** — e.g. a White pawn
//! reaching `a5`, `a6`, or `a7`. Because no single rank describes this region, the
//! variant overrides the square-aware
//! [`in_promotion_zone_sq`](WideVariant::in_promotion_zone_sq) hook that the pawn
//! generator consults for every pawn destination.
//!
//! ## How the setup is expressed
//!
//! Three hooks over standard chess, all defaulting to the ordinary pawn / rank-based
//! zone so every other variant is byte-identical:
//!
//! * [`role_attacks`](WideVariant::role_attacks) for the Pawn returns the **two
//!   orthogonal capture squares** (north + west for White) — the squares it captures
//!   onto, and therefore the only squares from which it gives check or contributes to
//!   king-danger. Its diagonal *move* is not an attack (it can never capture there),
//!   so it correctly gives no check.
//! * [`pawn_is_legan`](WideVariant::pawn_is_legan) with
//!   [`legan_push_target`](WideVariant::legan_push_target) switches the standard
//!   pawn generator to the single diagonal quiet advance (no double step, no en
//!   passant).
//! * [`in_promotion_zone_sq`](WideVariant::in_promotion_zone_sq) tests membership in
//!   the L-shaped corner region.
//!
//! Castling and en passant are removed ([`has_castling`](WideVariant::has_castling)
//! and [`has_en_passant`](WideVariant::has_en_passant) are `false`); the fifty-move
//! rule and threefold repetition are standard.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant legan` / `position startpos`:
//!
//! ```text
//! knbrp3/bqpp4/npp5/rp1p3P/p3P1PR/5PPN/4PPQB/3PRBNK w - - 0 1
//! ```
//!
//! Because the armies sit on a diagonal and the pawns move diagonally, the perft
//! counts differ from standard chess immediately (startpos perft 1 / 2 / 3 = `8` /
//! `64` / `724`).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{Bitboard, Board, Chess8x8, Square, StandardChess, WideRole, WideVariant};
use crate::Color;

/// The Legan starting placement (the diagonal corner array), confirmed against
/// Fairy-Stockfish's `UCI_Variant legan`.
const LEGAN_START_PLACEMENT: &str = "knbrp3/bqpp4/npp5/rp1p3P/p3P1PR/5PPN/4PPQB/3PRBNK";

/// The Legan chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides only what Legan changes about standard chess: the Pawn's directional
/// attack pattern ([`WideVariant::role_attacks`]), the diagonal quiet move
/// ([`WideVariant::pawn_is_legan`] plus [`WideVariant::legan_push_target`]), the
/// L-shaped corner promotion region ([`WideVariant::in_promotion_zone_sq`]), and the
/// removal of castling and en passant. Every other piece's movement, the promotion
/// set, and the fifty-move rule are the standard trait defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LeganRules;

impl LeganRules {
    /// The single diagonal quiet-advance square of a Legan pawn of `color` on
    /// `from`: one square up-left for White, down-right for Black.
    fn diagonal_step(color: Color, from: Square<Chess8x8>) -> Option<Square<Chess8x8>> {
        match color {
            Color::White => from.offset(-1, 1),
            Color::Black => from.offset(1, -1),
        }
    }
}

impl WideVariant<Chess8x8> for LeganRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field: the standard army `Pawn..King` and the standard promotion
    /// targets (Knight / Bishop / Rook / Queen), all within `0..=5`. See
    /// [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(LEGAN_START_PLACEMENT)
            .expect("the Legan starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            // No castling rights: Legan has no castling.
            castling: GenericCastling::NONE,
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
        };
        (board, state)
    }

    /// The Legan pawn **captures — and so attacks / checks — one square along either
    /// orthogonal making up its forward diagonal**: north or west for White, south or
    /// east for Black. Returning exactly those squares makes king-safety see the
    /// pawn's real threat and leaves its diagonal *move* a non-attacking quiet
    /// advance. Every other role keeps the standard pattern.
    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Chess8x8>,
        occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        match role {
            WideRole::Pawn => {
                let steps: [(i8, i8); 2] = match color {
                    Color::White => [(0, 1), (-1, 0)],
                    Color::Black => [(0, -1), (1, 0)],
                };
                let mut bb = Bitboard::<Chess8x8>::EMPTY;
                for (df, dr) in steps {
                    if let Some(dest) = sq.offset(df, dr) {
                        bb.set(dest);
                    }
                }
                bb
            }
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    /// The Pawn (`WideRole::Pawn`) is a Legan pawn: a single diagonal quiet advance,
    /// two-orthogonal capture, corner-region promotion, no double step, no en
    /// passant. Drives the Legan branch of the generic pawn generator.
    fn pawn_is_legan() -> bool {
        true
    }

    /// The single diagonal quiet-advance square of a Legan pawn — one square up-left
    /// for White, down-right for Black.
    fn legan_push_target(color: Color, from: Square<Chess8x8>) -> Option<Square<Chess8x8>> {
        Self::diagonal_step(color, from)
    }

    /// Legan has **no en passant** (FSF `doubleStep = false`): the pawn never double
    /// steps, so no en-passant target is ever recorded and no en-passant capture is
    /// ever offered.
    fn has_en_passant() -> bool {
        false
    }

    /// Legan has **no castling**.
    fn has_castling() -> bool {
        false
    }

    /// A pawn of `color` promotes on the **L-shaped corner region**, not a rank:
    /// White on `{a8, b8, c8, d8, a7, a6, a5}` (the top edge's left half plus the left
    /// edge's upper half), Black on the mirrored `{e1, f1, g1, h1, h2, h3, h4}`. This
    /// is the square-aware zone the pawn generator consults for every destination.
    fn in_promotion_zone_sq(color: Color, sq: Square<Chess8x8>) -> bool {
        let file = sq.file();
        let rank = sq.rank();
        match color {
            // Top edge files a..d (0..=3) on rank 8, and left edge file a (0) on
            // ranks 5..8 (4..=7).
            Color::White => (rank == 7 && file <= 3) || (file == 0 && rank >= 4),
            // Bottom edge files e..h (4..=7) on rank 1, and right edge file h (7) on
            // ranks 1..4 (0..=3).
            Color::Black => (rank == 0 && file >= 4) || (file == 7 && rank <= 3),
        }
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

/// Legan chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Legan::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Legan::from_fen`](GenericPosition::from_fen). Movement is standard chess with a
/// directional pawn (diagonal move, two-orthogonal capture) and an L-shaped corner
/// promotion region.
pub type Legan = GenericPosition<Chess8x8, LeganRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Square, WideMoveKind};

    fn sq(file: u8, rank: u8) -> Square<Chess8x8> {
        Square::<Chess8x8>::from_file_rank(file, rank).unwrap()
    }

    /// The canonical start FEN round-trips (no castling rights, no ep), and the
    /// diagonal army gives a first-ply count that matches Fairy-Stockfish's legan
    /// startpos perft 1.
    #[test]
    fn startpos_fen_and_move_count() {
        let pos = Legan::startpos();
        assert_eq!(
            pos.to_fen(),
            "knbrp3/bqpp4/npp5/rp1p3P/p3P1PR/5PPN/4PPQB/3PRBNK w - - 0 1"
        );
        assert_eq!(pos.turn(), Color::White);
        assert_eq!(pos.legal_move_count(), 8);
    }

    /// A White Legan pawn's non-capturing move is the single up-left diagonal, and it
    /// captures straight up (north) or straight left (west) — never the diagonal,
    /// never east.
    #[test]
    fn diagonal_move_and_orthogonal_captures() {
        // White pawn d4; enemy knights on d5 (north), c4 (west), e4 (east), and on
        // the diagonal landing square c5. Empty otherwise.
        let pos = Legan::from_fen("7k/8/8/2nn4/2nPn3/8/8/7K w - - 0 1").expect("valid FEN");
        let d4 = sq(3, 3);
        let moves: alloc::vec::Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Chess8x8>() == d4)
            .collect();
        // c5 is occupied, so the diagonal quiet advance is blocked; the pawn captures
        // north (d5) and west (c4) only.
        assert!(
            moves
                .iter()
                .any(|m| m.to::<Chess8x8>() == sq(3, 4) && m.is_capture()),
            "north capture onto d5",
        );
        assert!(
            moves
                .iter()
                .any(|m| m.to::<Chess8x8>() == sq(2, 3) && m.is_capture()),
            "west capture onto c4",
        );
        // Never captures east (e4) and never onto the diagonal (c5, which is a move
        // square, here blocked).
        assert!(
            !moves.iter().any(|m| m.to::<Chess8x8>() == sq(4, 3)),
            "no east capture",
        );
        assert_eq!(moves.len(), 2, "exactly dxc4 and dxd5");

        // With the diagonal clear, the quiet advance is the up-left square.
        let pos2 = Legan::from_fen("7k/8/8/8/3P4/8/8/7K w - - 0 1").expect("valid FEN");
        let quiet: alloc::vec::Vec<_> = pos2
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Chess8x8>() == d4)
            .collect();
        assert_eq!(quiet.len(), 1, "one pawn move");
        assert_eq!(
            quiet[0].to::<Chess8x8>(),
            sq(2, 4),
            "quiet advance up-left to c5"
        );
        assert!(!quiet[0].is_capture());
    }

    /// A White pawn promotes on a corner-region square that is **not** the last rank
    /// (a5, a6, a7), and a pawn reaching the top rank *outside* the region (e8) does
    /// **not** promote.
    #[test]
    fn promotes_on_corner_not_only_last_rank() {
        // White pawn b6 advances up-left to a7 (in the L-region) and must promote to
        // each of the four roles.
        let pos = Legan::from_fen("7k/8/1P6/8/8/8/8/7K w - - 0 1").expect("valid FEN");
        let promos: alloc::vec::Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter(|m| {
                m.from::<Chess8x8>() == sq(1, 5)
                    && m.to::<Chess8x8>() == sq(0, 6)
                    && matches!(m.kind(), WideMoveKind::Promotion { .. })
            })
            .collect();
        assert_eq!(
            promos.len(),
            4,
            "a7 promotes to Q/R/B/N (not the last rank)"
        );

        // White pawn f7 advances up-left to e8: the top rank but outside the region,
        // so it is a single plain quiet move — no promotion.
        let pos2 = Legan::from_fen("7k/5P2/8/8/8/8/8/6K1 w - - 0 1").expect("valid FEN");
        let to_e8: alloc::vec::Vec<_> = pos2
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Chess8x8>() == sq(5, 6) && m.to::<Chess8x8>() == sq(4, 7))
            .collect();
        assert_eq!(to_e8.len(), 1, "e8 is not in the promotion region");
        assert!(
            matches!(to_e8[0].kind(), WideMoveKind::Quiet),
            "reaching e8 is a plain move, not a promotion",
        );
    }

    /// There is no double step (so no pawn ever advances two squares) and no en
    /// passant target is ever recorded.
    #[test]
    fn no_double_step_or_en_passant() {
        let pos = Legan::startpos();
        // No pawn move covers two ranks, and no move is a double push.
        for m in pos.legal_moves() {
            assert!(
                !matches!(m.kind(), WideMoveKind::DoublePawnPush),
                "Legan has no double step",
            );
        }
        // A single diagonal advance sets no ep target.
        let mv = pos
            .legal_moves()
            .into_iter()
            .find(|m| matches!(m.kind(), WideMoveKind::Quiet))
            .expect("a quiet pawn advance exists");
        let after = pos.play(&mv);
        assert_eq!(
            after.ep_square(),
            None,
            "no en-passant target after a pawn move"
        );
    }
}
