//! Loop Chess (8x8) on the generic engine — **Crazyhouse where promoted pieces
//! keep their role when captured**. It is standard 8x8 chess plus the crazyhouse
//! hand — every captured piece flips to the captor's side, enters the hand, and
//! may later be **dropped** back onto an empty square as a move — with the one
//! Loop twist (Fairy-Stockfish `dropLoop`): a captured piece that itself reached
//! the board **by promotion** banks as its **own (promoted) role**, not as a Pawn.
//! Validated against Fairy-Stockfish `UCI_Variant loop`
//! (<https://en.wikipedia.org/wiki/Crazyhouse#Variations>).
//!
//! ## Pieces and movement
//!
//! Identical to standard chess: pawns double-push, capture diagonally, take en
//! passant, and promote on the last rank to Knight, Bishop, Rook, or Queen; the
//! king castles normally. The piece letters `K Q R B N P` are shared between mcr
//! and FSF, so the FEN needs no dialect translation.
//!
//! ## Hand and drops (crazyhouse) + the Loop rule
//!
//! A captured piece banks to the captor's hand, flipped to their colour. From the
//! hand a side may **drop** a held piece onto any empty square, with one
//! restriction confirmed against FSF: a **Pawn may not be dropped on the first or
//! last rank** (rank 1 or rank 8). There is no *nifu* (a dropped pawn may share a
//! file with another pawn) and a drop giving check or mate is legal.
//!
//! The single rule that separates Loop Chess from ordinary Crazyhouse is
//! [`drop_loop`](crate::geometry::WideVariant::drop_loop): in Crazyhouse a captured
//! piece that reached the board **by promotion** is demoted to a **Pawn** in hand
//! (`Q~` -> `p`); in Loop it keeps its promoted role (`Q~` -> `Q`). The promoted
//! mask is still tracked (so `~` round-trips through the FEN and the captured piece
//! is *known* to be promoted), but the demotion is skipped. This is exactly FSF's
//! `pieceToHand = !capturedPromoted || drop_loop() ? ~captured : PAWN`, and it is
//! the only thing that makes Loop's node counts differ from a demoting Crazyhouse
//! in positions with a captured promoted piece.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant loop` (`loop_variant()`
//! `startFen`, inherited from `crazyhouse_variant()`):
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1
//! ```
//!
//! The trailing `[]` is the empty crazyhouse hand (FSF accepts it present or
//! omitted).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{Bitboard, Board, Chess8x8, Geometry, Square, WideRole, WideVariant};
use crate::Color;

/// The Loop Chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It is standard 8x8 chess plus the crazyhouse hand
/// ([`has_hand`](WideVariant::has_hand),
/// [`captures_to_hand`](WideVariant::captures_to_hand)) with the promoted mask
/// tracked ([`demotes_promoted_captures`](WideVariant::demotes_promoted_captures))
/// but the demotion skipped ([`drop_loop`](WideVariant::drop_loop)), so a captured
/// promoted piece banks as its own role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LoopChessRules;

/// The standard 8x8 starting placement; the hand starts empty.
const LOOP_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

impl LoopChessRules {
    /// The squares a Pawn may **not** be dropped onto: the first and last rank
    /// (rank 1 and rank 8), where a dropped pawn would sit on a promotion rank.
    fn pawn_forbidden_ranks() -> Bitboard<Chess8x8> {
        let mut bb = Bitboard::<Chess8x8>::EMPTY;
        for file in 0..Chess8x8::WIDTH {
            for rank in [0, Chess8x8::HEIGHT - 1] {
                if let Some(sq) = Square::<Chess8x8>::from_file_rank(file, rank) {
                    bb.set(sq);
                }
            }
        }
        bb
    }
}

impl WideVariant<Chess8x8> for LoopChessRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(LOOP_START_PLACEMENT)
            .expect("the Loop Chess starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::standard::<Chess8x8>(),
            ep_square: None,
            ep_captured: None,
            gating: GenericGating::NONE,
            duck: None,
            // The crazyhouse hand starts empty; it rides in `GenericPlacement`.
            placement: GenericPlacement::NONE,
            halfmove_clock: 0,
            fullmove_number: 1,
            consecutive_passes: 0,
            board_b: Bitboard::EMPTY,
            petrified: Bitboard::EMPTY,
            checks_against: [0, 0],
        };
        (board, state)
    }

    // Promotion uses the trait default (Knight, Bishop, Rook, Queen) — ordinary
    // chess promotion targets.

    // --- crazyhouse hand + drops ------------------------------------------

    fn has_hand() -> bool {
        true
    }

    fn pawn_is_stepper() -> bool {
        // Loop pawns are ordinary chess pawns (double push, diagonal capture, en
        // passant, last-rank promotion), not Shogi forward steppers.
        false
    }

    fn demotes_promoted_captures() -> bool {
        // Track the crazyhouse promoted mask so `~` round-trips through the FEN and a
        // captured promoted piece is *known* to have arrived by promotion. The Loop
        // rule ([`drop_loop`]) then decides whether it banks as its promoted role or
        // a Pawn — here, as its promoted role.
        true
    }

    fn drop_loop() -> bool {
        // The Loop Chess rule (FSF `dropLoop`): a captured promoted piece keeps its
        // role in hand (`Q~` -> `Q`) instead of demoting to a Pawn (`Q~` -> `p`). The
        // only thing that distinguishes Loop from a demoting Crazyhouse.
        true
    }

    fn drop_targets<const R: usize>(
        role: WideRole,
        _color: Color,
        board: &Board<Chess8x8, R>,
    ) -> Bitboard<Chess8x8> {
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
    /// level. Loop Chess is an 8x8 crazyhouse-style drop game, so it inherits the
    /// western repetition rule (three-fold plain draw, no perpetual-check
    /// exception) and the `repetition_fold`, `repetition_draw_reason`, and
    /// `perpetual_check_loses` defaults are already correct. History-dependent and
    /// never consulted by a bare [`GenericPosition`], so perft is unchanged.
    fn tracks_repetition() -> bool {
        true
    }
}

/// Loop Chess (8x8 crazyhouse with promoted-piece drops) as a [`GenericPosition`]
/// over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position (the standard array with an empty crazyhouse
/// hand) with [`LoopChess::startpos`](GenericPosition::startpos) or parse a FEN —
/// the placement may carry the hand as a `[..]` bracket and promoted pieces as a
/// `~` suffix — with [`LoopChess::from_fen`](GenericPosition::from_fen).
pub type LoopChess = GenericPosition<
    Chess8x8,
    LoopChessRules,
    { <LoopChessRules as WideVariant<Chess8x8>>::ROLE_SPAN },
>;
