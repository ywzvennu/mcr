//! Duck chess (8x8) on the generic engine — the first variant exercising the
//! neutral-**Duck** blocker and the **two-part move** on the [`WideVariant`]
//! layer (`docs/fairy-variants-architecture.md` §4.4). Validated against
//! Fairy-Stockfish `UCI_Variant duck`.
//!
//! Duck chess is standard 8x8 chess plus a single neutral **Duck**: a universal
//! blocker that belongs to neither side. Each turn is **one ply made of two
//! actions**: (1) a normal piece move, then (2) the Duck is moved to any empty
//! square (a *different* square each turn; on the very first move it enters the
//! board). A node's children are therefore the cross-product
//! `(legal piece moves) × (empty squares the duck may occupy after that move)`.
//!
//! ## The three mechanics
//!
//! * **Duck blocker.** The Duck is added to the occupancy for every slider and
//!   stepper: no piece may land on it, and it blocks slider rays. Knights jump
//!   *over* it (their attack set ignores occupancy), so it blocks landing, not
//!   knight paths. It is neither side's piece — never captured, never material.
//!   The Duck square lives in [`GenericState::duck`](crate::geometry::position).
//!
//! * **Two-part move.** The Duck destination rides in the [`WideMove`] high-word
//!   addendum ([`WideMove::with_duck`]); movegen emits each base piece move
//!   crossed with every legal duck placement, and `apply` moves the piece then
//!   the Duck. The whole path is gated behind [`WideVariant::has_duck`]
//!   (default-off), so every other variant is byte-identical.
//!
//! * **No check.** The king is **not royal** ([`royal_squares`] is empty), so
//!   there is no check, pin, or self-check filtering: a king may move to or be
//!   left on an attacked square, and *capturing the enemy king* is a legal move —
//!   it is how the game is won. A side with no legal move has lost. These
//!   outcomes do not affect perft node counts (which is what the FSF gate pins).
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant duck`:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! ```
//!
//! The Duck is not yet on the board at the start (it enters on the first move),
//! so the opening FEN is the plain chess array. Once placed, the Duck renders as
//! a `*` in the placement field (e.g. `...*PPPPPPP...`), the FSF dialect, parsed
//! and written by the generic
//! [`from_fen`](crate::geometry::GenericPosition::from_fen) /
//! [`to_fen`](crate::geometry::GenericPosition::to_fen) when
//! [`has_duck`](WideVariant::has_duck) is `true`.
//!
//! [`royal_squares`]: WideVariant::royal_squares
//! [`WideMove`]: crate::geometry::WideMove
//! [`WideMove::with_duck`]: crate::geometry::WideMove::with_duck

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{Bitboard, Board, Chess8x8, WideVariant};
use crate::Color;

/// The Duck-chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides only what Duck chess adds to standard chess: the neutral Duck
/// (via [`WideVariant::has_duck`]) and the non-royal king (via
/// [`WideVariant::royal_squares`]). Every piece's movement, castling, and pawn
/// rule is the standard trait default; the duck-blocker, the two-part move, and
/// the no-check legality all live in the generic engine, switched on by the two
/// hooks below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DuckRules;

/// The standard 8x8 starting placement (Duck chess shares the chess array; the
/// Duck is not on the board until the first move places it).
const DUCK_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

impl WideVariant<Chess8x8> for DuckRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(DUCK_START_PLACEMENT)
            .expect("the Duck starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::standard::<Chess8x8>(),
            ep_square: None,
            ep_captured: None,
            gating: GenericGating::NONE,
            // The Duck enters on the first move; it is off the board at the start.
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

    fn has_duck() -> bool {
        true
    }

    fn royal_squares<const R: usize>(
        _board: &Board<Chess8x8, R>,
        _color: Color,
    ) -> Bitboard<Chess8x8> {
        // The king is not royal in Duck chess: there is no check. An empty royal
        // set makes the generic king-safety machinery report "never in check",
        // and the duck generator skips check / pin filtering entirely.
        Bitboard::EMPTY
    }
}

/// Duck chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Duck::startpos`](GenericPosition::startpos) or parse a FEN — the placement
/// may carry a `*` for the Duck — with
/// [`Duck::from_fen`](GenericPosition::from_fen). Each move is a two-part ply (a
/// piece move plus a duck placement); see the [module docs](self).
pub type Duck =
    GenericPosition<Chess8x8, DuckRules, { <DuckRules as WideVariant<Chess8x8>>::ROLE_SPAN }>;
