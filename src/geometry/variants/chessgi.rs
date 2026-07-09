//! Chessgi (8x8) on the generic engine — **Loop Chess where pawns may also be
//! dropped on the first rank**. It is [`LoopChess`](super::loop_chess::LoopChess)
//! (crazyhouse with the [`drop_loop`](crate::geometry::WideVariant::drop_loop)
//! promoted-piece rule) with one further relaxation, Fairy-Stockfish
//! `firstRankPawnDrops`: a Pawn may be dropped onto its own first rank (rank 1 for
//! White, rank 8 for Black) as well as ranks 2-7 — only the pawn's **promotion
//! rank** (the enemy back rank) stays forbidden. Validated against Fairy-Stockfish
//! `UCI_Variant chessgi` (<https://en.wikipedia.org/wiki/Crazyhouse#Variations>).
//!
//! ## Pieces and movement
//!
//! Identical to standard chess and to Loop Chess: ordinary pawns, castling, en
//! passant, and last-rank promotion to Knight, Bishop, Rook, or Queen. The piece
//! letters `K Q R B N P` are shared between mcr and FSF, so the FEN needs no
//! dialect translation.
//!
//! ## Hand and drops — the one difference from Loop
//!
//! The hand and the [`drop_loop`](crate::geometry::WideVariant::drop_loop)
//! promoted-piece rule are inherited unchanged from Loop Chess (a captured piece
//! banks to the captor's hand and may be dropped; a captured promoted piece keeps
//! its role rather than demoting to a Pawn). The sole difference is the pawn drop
//! region (FSF `firstRankPawnDrops = true`): where Loop forbids a pawn drop on
//! **both** the first and last rank, Chessgi forbids only the pawn's **promotion
//! rank** — the enemy back rank (rank 8 for a White pawn, rank 1 for a Black
//! pawn). A pawn may therefore be dropped onto its own first rank, so this drop
//! region is **colour-dependent**. There is still no *nifu*, and a drop giving
//! check or mate is legal.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant chessgi` (`chessgi_variant()`
//! `startFen`, inherited from `crazyhouse_variant()`):
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1
//! ```
//!
//! The trailing `[]` is the empty crazyhouse hand.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{Bitboard, Board, Chess8x8, Geometry, Square, WideRole, WideVariant};
use crate::Color;

/// The Chessgi rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It is the Loop Chess rule layer (crazyhouse hand + the `drop_loop` promoted
/// rule) with the pawn drop region relaxed to allow first-rank pawn drops (FSF
/// `firstRankPawnDrops`), so only the pawn's promotion rank stays forbidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ChessgiRules;

/// The standard 8x8 starting placement; the hand starts empty.
const CHESSGI_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

impl ChessgiRules {
    /// The single rank a `color` Pawn may **not** be dropped onto: its promotion
    /// rank — the enemy back rank (rank 8 for White, rank 1 for Black), where a
    /// dropped pawn would sit on a promotion square. Unlike Loop, the pawn's own
    /// first rank is *allowed* (FSF `firstRankPawnDrops`), so only this one rank is
    /// masked out.
    fn pawn_forbidden_rank(color: Color) -> Bitboard<Chess8x8> {
        let rank = if color.is_white() {
            Chess8x8::HEIGHT - 1
        } else {
            0
        };
        let mut bb = Bitboard::<Chess8x8>::EMPTY;
        for file in 0..Chess8x8::WIDTH {
            if let Some(sq) = Square::<Chess8x8>::from_file_rank(file, rank) {
                bb.set(sq);
            }
        }
        bb
    }
}

impl WideVariant<Chess8x8> for ChessgiRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(CHESSGI_START_PLACEMENT)
            .expect("the Chessgi starting placement is valid on an 8x8 board");
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
            jieqi_seed: None,
        };
        (board, state)
    }

    // Promotion uses the trait default (Knight, Bishop, Rook, Queen) — ordinary
    // chess promotion targets.

    // --- crazyhouse hand + drops (Loop) + first-rank pawn drops ------------

    fn has_hand() -> bool {
        true
    }

    fn pawn_is_stepper() -> bool {
        // Chessgi pawns are ordinary chess pawns (double push, diagonal capture, en
        // passant, last-rank promotion), not Shogi forward steppers.
        false
    }

    fn demotes_promoted_captures() -> bool {
        // Track the crazyhouse promoted mask so `~` round-trips through the FEN;
        // `drop_loop` (below) then keeps a captured promoted piece as its own role.
        true
    }

    fn drop_loop() -> bool {
        // Inherited from Loop Chess (FSF `dropLoop`): a captured promoted piece keeps
        // its role in hand instead of demoting to a Pawn.
        true
    }

    fn drop_targets<const R: usize>(
        role: WideRole,
        color: Color,
        board: &Board<Chess8x8, R>,
    ) -> Bitboard<Chess8x8> {
        // Every empty square (crazyhouse) — except that a Pawn may not be dropped on
        // its **promotion rank** (the enemy back rank). Unlike Loop, the pawn's own
        // first rank is allowed (FSF `firstRankPawnDrops`), so the forbidden region
        // is a single colour-dependent rank. There is no nifu, so no file filter.
        let empty = !board.occupied();
        if role == WideRole::Pawn {
            empty & !Self::pawn_forbidden_rank(color)
        } else {
            empty
        }
    }

    /// Records a position history so the standard **threefold** repetition draw
    /// ([`WideEndReason::Repetition`](crate::geometry::WideEndReason::Repetition),
    /// fold 3) fires at the [`GenericGame`](crate::geometry::game::GenericGame)
    /// level. Chessgi is an 8x8 crazyhouse-style drop game, so it inherits the
    /// western repetition rule (three-fold plain draw, no perpetual-check
    /// exception) and the `repetition_fold`, `repetition_draw_reason`, and
    /// `perpetual_check_loses` defaults are already correct. History-dependent and
    /// never consulted by a bare [`GenericPosition`], so perft is unchanged.
    fn tracks_repetition() -> bool {
        true
    }
}

/// Chessgi (8x8 Loop Chess with first-rank pawn drops) as a [`GenericPosition`]
/// over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position (the standard array with an empty crazyhouse
/// hand) with [`Chessgi::startpos`](GenericPosition::startpos) or parse a FEN — the
/// placement may carry the hand as a `[..]` bracket and promoted pieces as a `~`
/// suffix — with [`Chessgi::from_fen`](GenericPosition::from_fen).
pub type Chessgi =
    GenericPosition<Chess8x8, ChessgiRules, { <ChessgiRules as WideVariant<Chess8x8>>::ROLE_SPAN }>;
