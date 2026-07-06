//! Capahouse (10x8) on the generic engine — **Capablanca chess plus crazyhouse
//! drops**. It is [`Capablanca`](super::capablanca::Capablanca) (the ten-files
//! board with the Archbishop and Chancellor compounds) with the
//! [`Shogun`](super::shogun::Shogun)/crazyhouse **hand**: every captured piece
//! flips to the captor's side, enters the hand, and may later be **dropped** back
//! onto an empty square as a move. Validated against Fairy-Stockfish
//! `UCI_Variant capahouse` (<https://www.pychess.org/variant/capahouse>).
//!
//! ## Pieces and movement
//!
//! Identical to Capablanca — every rule of movement, castling, en passant, and
//! last-rank promotion is inherited unchanged:
//!
//! * **Archbishop** (Bishop + Knight) — [`WideRole::Hawk`], FEN letter `a`.
//! * **Chancellor** (Rook + Knight) — [`WideRole::Elephant`], FEN letter `e` in
//!   the mcr dialect (Fairy-Stockfish spells it `c`; the `compare-fairy/` harness
//!   reconciles the one-letter difference).
//! * Pawns, knights, bishops, rooks, the queen, and the king move exactly as in
//!   Capablanca; a pawn promotes on the last rank to Queen, Rook, Bishop, Knight,
//!   **Archbishop, or Chancellor**, and castling lands the king two files over on
//!   the Capablanca files (king f -> i/c, rook j -> h / a -> d).
//!
//! ## Hand and drops (crazyhouse)
//!
//! A captured piece banks to the captor's hand. A **natural** piece banks as its
//! own role; a piece that itself reached the board **by promotion** banks as a
//! **Pawn** (the crazyhouse "promoted pieces demote" rule, tracked by the generic
//! promoted mask and rendered in the FEN as a trailing `~`, e.g. `Q~`). From the
//! hand a side may **drop** a held piece onto any empty square, with one
//! restriction confirmed against FSF: a **Pawn may not be dropped on the first or
//! last rank** (rank 1 or rank 8). There is no *nifu* (a dropped pawn may share a
//! file with another pawn) and drops giving check or mate are legal.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant capahouse` (`capahouse_variant()`
//! `startFen`):
//!
//! ```text
//! FSF dialect: rnabqkbcnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBCNR[] w KQkq - 0 1
//! mcr dialect: rnabqkbenr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBENR[] w KQkq - 0 1
//! ```
//!
//! The two differ only in the chancellor's letter (`c` in FSF, `e` in mcr); the
//! trailing `[]` is the empty crazyhouse hand.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    Bitboard, Board, Cap10x8, Geometry, PromotionConfig, Square, WideRole, WideVariant,
};
use crate::Color;

/// The Capahouse rule layer: a zero-sized [`WideVariant`] over [`Cap10x8`].
///
/// It is the Capablanca rule layer plus the crazyhouse hand: the starting array,
/// promotion set, and castle files are Capablanca's, and the hand hooks
/// ([`has_hand`](WideVariant::has_hand),
/// [`demotes_promoted_captures`](WideVariant::demotes_promoted_captures), the pawn
/// drop region) add the drops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CapahouseRules;

/// The confirmed Capahouse starting placement in the mcr dialect (chancellor =
/// `e`/`E`), identical to Capablanca's; the empty hand rides in the FEN's `[]`.
const CAPAHOUSE_START_PLACEMENT: &str = "rnabqkbenr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBENR";

/// The kingside castle side index, matching the position layer's `KINGSIDE`.
const KINGSIDE: usize = 0;

impl WideVariant<Cap10x8> for CapahouseRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 12;

    fn starting_position() -> (Board<Cap10x8>, GenericState<Cap10x8>) {
        let board = Board::<Cap10x8>::from_fen_placement(CAPAHOUSE_START_PLACEMENT)
            .expect("the Capahouse starting placement is valid on a 10x8 board");
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::standard::<Cap10x8>(),
            ep_square: None,
            gating: GenericGating::NONE,
            duck: None,
            // The crazyhouse hand starts empty; it rides in `GenericPlacement`.
            placement: GenericPlacement::NONE,
            halfmove_clock: 0,
            fullmove_number: 1,
            consecutive_passes: 0,
            board_b: crate::geometry::Bitboard::EMPTY,
        };
        (board, state)
    }

    fn promotion_config() -> PromotionConfig {
        // A Capahouse pawn promotes to any of the six non-pawn, non-king army
        // roles — the four standard plus the two Capablanca compounds.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Knight,
                WideRole::Bishop,
                WideRole::Rook,
                WideRole::Queen,
                WideRole::Hawk,     // Archbishop (B+N)
                WideRole::Elephant, // Chancellor (R+N)
            ],
        }
    }

    fn castle_dest_files(side: usize) -> (u8, u8) {
        // Capablanca castling files, matching FSF's castlingKingsideFile = FILE_I
        // (8) / castlingQueensideFile = FILE_C (2).
        if side == KINGSIDE {
            // King f1 -> i1 (file 8); rook j1 -> h1 (file 7).
            (8, 7)
        } else {
            // King f1 -> c1 (file 2); rook a1 -> d1 (file 3).
            (2, 3)
        }
    }

    // --- crazyhouse hand + drops ------------------------------------------

    fn has_hand() -> bool {
        true
    }

    fn pawn_is_stepper() -> bool {
        // Capahouse pawns are ordinary Capablanca/chess pawns (double push,
        // diagonal capture, en passant, last-rank promotion), not Shogi forward
        // steppers.
        false
    }

    fn demotes_promoted_captures() -> bool {
        // The crazyhouse rule: a captured piece that reached the board by
        // promotion banks as a Pawn (`Q~` -> `p` in hand), tracked by the generic
        // promoted mask. `captures_to_hand` keeps its default `true`, and
        // `role_hand_base` its default identity (a natural Archbishop/Chancellor
        // banks as itself — `promoted_base` leaves the compounds untouched).
        true
    }

    fn drop_targets(role: WideRole, _color: Color, board: &Board<Cap10x8>) -> Bitboard<Cap10x8> {
        // Every empty square (crazyhouse) — except that a Pawn may not be dropped
        // on the first or last rank (FSF confirms pawn drops only on ranks 2-7).
        // There is no nifu, so no file filter.
        let empty = !board.occupied();
        if role == WideRole::Pawn {
            empty & !Self::pawn_forbidden_ranks()
        } else {
            empty
        }
    }

    /// Records a position history so the standard **threefold** repetition draw
    /// ([`WideEndReason::Repetition`](crate::geometry::WideEndReason::Repetition),
    /// fold 3) fires at the [`GenericGame`](crate::geometry::game::GenericGame)
    /// level. This Capablanca + crazyhouse-drops hybrid inherits the western
    /// repetition rule (three-fold plain draw, no perpetual-check exception), so
    /// the `repetition_fold`, `repetition_draw_reason`, and
    /// `perpetual_check_loses` defaults are already correct. History-dependent and
    /// never consulted by a bare
    /// [`GenericPosition`], so perft is unchanged.
    fn tracks_repetition() -> bool {
        true
    }
}

impl CapahouseRules {
    /// The squares a Pawn may **not** be dropped onto: the first and last rank
    /// (rank 1 and rank 8), where a dropped pawn would be on a promotion rank.
    fn pawn_forbidden_ranks() -> Bitboard<Cap10x8> {
        let mut bb = Bitboard::<Cap10x8>::EMPTY;
        for file in 0..Cap10x8::WIDTH {
            for rank in [0, Cap10x8::HEIGHT - 1] {
                if let Some(sq) = Square::<Cap10x8>::from_file_rank(file, rank) {
                    bb.set(sq);
                }
            }
        }
        bb
    }
}

/// Capahouse (10x8 Capablanca + crazyhouse drops) as a [`GenericPosition`] over
/// the 10x8 [`Cap10x8`] geometry.
///
/// Construct the starting position (the Capablanca array with an empty crazyhouse
/// hand) with [`Capahouse::startpos`](GenericPosition::startpos) or parse a FEN —
/// the placement may carry the hand as a `[..]` bracket and promoted pieces as a
/// `~` suffix — with [`Capahouse::from_fen`](GenericPosition::from_fen).
pub type Capahouse = GenericPosition<Cap10x8, CapahouseRules>;
