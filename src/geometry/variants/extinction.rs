//! Extinction chess (8x8) on the generic engine — **standard chess movement in
//! which a side loses the moment any one of its piece *types* is wiped out**.
//! Validated against Fairy-Stockfish `UCI_Variant extinction` (a built-in;
//! `extinction_variant()`, `variant.cpp:449`).
//!
//! Extinction chess keeps every standard chess move — the same pieces, castling,
//! en passant, and promotions — but **there is no check and no checkmate**: the
//! king is a non-royal **Commoner** (it still steps one square in any direction,
//! but it is an ordinary, capturable piece). A side loses **by extinction**:
//!
//! * the game ends the instant a side holds **zero** of *any* piece type it fields
//!   — Pawn, Knight, Bishop, Rook, Queen, **or** the king itself;
//! * capturing the enemy king is legal (the King type reaching 0 is one extinction
//!   among six), and so is capturing the last enemy queen, or the last knight, …;
//! * promoting your **last pawn** loses — it empties your Pawn type;
//! * a side may move into "check" and may leave its king attacked (there is no
//!   royal to keep safe).
//!
//! ## The shared extinction terminal — designed to be reused
//!
//! The loss condition is the **generic** [`WideVariant::extinction_rule`] hook: an
//! [`ExtinctionRule`] naming the *watched* piece types and the *threshold* at which
//! any of them counts as extinct. Extinction chess watches the **whole army**
//! (`[Pawn, Knight, Bishop, Rook, Queen, King]`) with `threshold = 0`. The same
//! hook, with a different slice / threshold, expresses Kinglet (`[Pawn]`, 0),
//! Codrus (`[King]`, 0), and Three-kings (`[King]`, 1) — the follow-on variants
//! that reuse this terminal without touching the engine again.
//!
//! ## King handling — a non-royal Commoner
//!
//! The non-royal king reuses the exact machinery Fog of War / Duck introduced: an
//! empty [`royal_squares`](WideVariant::royal_squares) set makes the generic
//! king-safety code report "never in check", and
//! [`non_royal_king`](WideVariant::non_royal_king) routes the standard generator
//! through its non-royal branch (every pseudo-legal board move is legal, the king
//! has no check mask / pin / king-danger filter). Castling stays enabled and is
//! **never** restricted by attacked squares. Because there is no check, the
//! extinction rule is the game's only decisive terminal.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant extinction`:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! ```
//!
//! Extinction chess shares the standard-chess dialect byte-for-byte (the king is
//! spelled `k`/`K` — the Commoner demotion is a *rule*, not a letter). The move set
//! is the no-check set (startpos perft(4) = `197742`, chess = `197281`); from
//! depth 5 the counts fall **below** the pure no-check numbers as the first
//! type-emptying captures truncate the tree (perft(5) = `4896744`, no-check =
//! `4897256`), exactly as Fairy-Stockfish adjudicates.
//!
//! [`ExtinctionRule`]: crate::geometry::ExtinctionRule

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    Bitboard, Board, Chess8x8, ExtinctionRule, PromotionConfig, WideRole, WideVariant,
};
use crate::Color;

/// The standard 8x8 starting placement (Extinction shares the chess array).
const EXTINCTION_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

/// The full standard army — the piece types Extinction chess watches. A side
/// loses the instant its count of **any** of these drops to zero (including its
/// non-royal king, [`WideRole::King`]).
const EXTINCTION_WATCHED: &[WideRole] = &[
    WideRole::Pawn,
    WideRole::Knight,
    WideRole::Bishop,
    WideRole::Rook,
    WideRole::Queen,
    WideRole::King,
];

/// The Extinction chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides only what Extinction changes about standard chess: the king is a
/// non-royal Commoner (via [`WideVariant::royal_squares`] and
/// [`WideVariant::non_royal_king`], like Fog of War), and the game ends by
/// extinction of any piece type (via [`WideVariant::extinction_rule`]). Every
/// piece's movement, castling, en passant, and promotion rule is the standard
/// trait default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ExtinctionRules;

impl WideVariant<Chess8x8> for ExtinctionRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field (Pawn..King, the standard army; promotions are Queen /
    /// Rook / Bishop / Knight, all within the prefix). See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(EXTINCTION_START_PLACEMENT)
            .expect("the Extinction starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            // Standard castling rights: Extinction keeps ordinary castling (the
            // non-royal king is never restricted by attacked squares).
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
        };
        (board, state)
    }

    // --- non-royal Commoner king (no check) -------------------------------

    fn non_royal_king() -> bool {
        // The king is a non-royal Commoner: the standard generator's non-royal
        // branch emits every pseudo-legal board move (no check mask, no pins, no
        // king-danger filter), so a king may step onto an attacked square, a piece
        // may move while "pinned", and capturing the enemy king is a legal move.
        true
    }

    fn royal_squares(_board: &Board<Chess8x8>, _color: Color) -> Bitboard<Chess8x8> {
        // The king is **not royal**: an empty royal set makes the generic
        // king-safety machinery report "never in check". A side loses not by
        // checkmate but by extinction (below), of which a king capture is one case.
        Bitboard::EMPTY
    }

    // --- extinction terminal (the whole army, threshold 0) ----------------

    /// A side loses the moment any of its piece types is wiped out. Extinction
    /// chess watches the **entire** standard army with `threshold = 0` (FSF
    /// `extinctionPieceTypes = ALL_PIECES`, `extinctionPieceCount = 0`).
    fn extinction_rule() -> Option<ExtinctionRule> {
        Some(ExtinctionRule {
            watched: EXTINCTION_WATCHED,
            threshold: 0,
        })
    }

    // --- promotion (adds the Commoner king) -------------------------------

    /// A pawn may promote to Knight / Bishop / Rook / Queen **or the king
    /// (Commoner)** — FSF's extinction lets a pawn become a second Commoner. This
    /// matters for extinction: promoting to a king revives (or bolsters) the King
    /// type, and a side may keep several kings. The role order matches FSF's
    /// generation so the move set is byte-identical.
    fn promotion_config() -> PromotionConfig {
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Knight,
                WideRole::Bishop,
                WideRole::Rook,
                WideRole::Queen,
                WideRole::King,
            ],
        }
    }
}

/// Extinction chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Extinction::startpos`](GenericPosition::startpos) or parse a plain-chess FEN
/// with [`Extinction::from_fen`](GenericPosition::from_fen). Movement is the
/// no-check standard-chess set; the game ends by extinction of any piece type.
pub type Extinction = GenericPosition<Chess8x8, ExtinctionRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Chess8x8, WideOutcome};

    /// The canonical start FEN round-trips and keeps standard castling rights, and
    /// the startpos is not (yet) terminal — every army type is present.
    #[test]
    fn startpos_fen_round_trips_with_castling() {
        let pos = Extinction::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
        assert_eq!(pos.turn(), Color::White);
        // No check restriction, castling both sides — the no-check move count is 20
        // at the start (the same as standard chess, before any "check" appears).
        assert_eq!(pos.legal_move_count(), 20);
        assert_eq!(pos.end_reason(), None, "the startpos is not terminal");
        assert_eq!(pos.outcome(), None);
    }

    /// The king is non-royal: a side may move into "check" and leave its king
    /// attacked — there is no self-check filter. Both sides field all six army
    /// types here, so nobody is (yet) extinct; a black rook on e5 "attacks" the
    /// white king down the open e-file.
    #[test]
    fn king_is_non_royal_no_check() {
        use crate::geometry::Square;
        let pos =
            Extinction::from_fen("1r1qkbn1/p7/8/4r3/8/8/P7/RNBQK3 w - - 0 1").expect("valid FEN");
        assert_eq!(pos.end_reason(), None, "not terminal — all types present");
        assert!(!pos.is_check(), "a non-royal king is never in check");

        let sq = |file, rank| Square::<Chess8x8>::from_file_rank(file, rank).unwrap();
        let moves = pos.legal_moves();
        // A non-king move (a2-a3) is legal even though it leaves the white king
        // "attacked" — a royal king would be forced to answer the "check".
        assert!(
            moves
                .iter()
                .any(|m| m.from::<Chess8x8>() == sq(0, 1) && m.to::<Chess8x8>() == sq(0, 2)),
            "a2-a3 is legal despite the king being attacked (no self-check filter)",
        );
        // The king may even step *into* the rook's line (e1-e2 onto an attacked
        // square) — impossible for a royal king.
        assert!(
            moves
                .iter()
                .any(|m| m.from::<Chess8x8>() == sq(4, 0) && m.to::<Chess8x8>() == sq(4, 1)),
            "the king may step onto an attacked square (non-royal)",
        );
    }

    /// **Adjudication test (coverage-gate registered):** a side that loses its last
    /// piece of a type has lost by extinction. White captures Black's only queen
    /// down the open d-file; the resulting position has zero Black queens, so it is
    /// terminal — a decisive win for White — and generates no moves.
    #[test]
    fn extinction_last_of_a_type_loses() {
        // Both sides hold all six army types (nobody is pre-extinct): White
        // K/Q/R/B/N/P, Black K/Q/R/B/N/P, with the white rook on d1 facing the lone
        // black queen on d8 down an open file.
        let pos = Extinction::from_fen("3qkbnr/p7/8/8/8/8/P7/QNBRK3 w - - 0 1").expect("valid FEN");
        assert_eq!(
            pos.end_reason(),
            None,
            "not yet terminal — all types present"
        );

        // Rxd8 removes Black's last queen.
        let rxd8 = pos
            .legal_moves()
            .into_iter()
            .find(|m| {
                m.from::<Chess8x8>() == crate::geometry::Square::<Chess8x8>::new(3) // d1
                    && m.to::<Chess8x8>() == crate::geometry::Square::<Chess8x8>::new(59)
                // d8
            })
            .expect("Rxd8 is a legal move");
        let after = pos.play(&rxd8);

        assert_eq!(
            after.board().pieces(Color::Black, WideRole::Queen).count(),
            0,
            "Black has no queen left",
        );
        assert_eq!(after.extinction_loser(), Some(Color::Black));
        assert_eq!(
            after.end_reason(),
            Some(crate::geometry::WideEndReason::VariantWin)
        );
        assert_eq!(
            after.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::White
            }),
        );
        // Terminal: the losing (extinct) side generates no continuation.
        assert!(
            after.legal_moves().is_empty(),
            "an extinct position is a terminal perft leaf",
        );
    }

    /// A position where a side is *already* missing a type is terminal at any depth
    /// — the same truncation Fairy-Stockfish's `go perft` reports as 0.
    #[test]
    fn missing_a_type_is_terminal() {
        // Black is missing its queen (all other types present); it has already lost.
        let pos = Extinction::from_fen("rnb1kbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
            .expect("valid FEN");
        assert_eq!(pos.extinction_loser(), Some(Color::Black));
        assert!(pos.legal_moves().is_empty(), "terminal — no moves");
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::White
            }),
        );
    }

    /// The **Pawn** type is watched too (not just the king): a side with zero pawns
    /// is extinct. This is why promoting your last pawn loses in Extinction chess —
    /// it empties the Pawn type. Here White has no pawns and so has already lost.
    #[test]
    fn zero_pawns_is_extinction() {
        // White: full back rank, no pawns (rank 2 empty). Black: full army.
        let pos = Extinction::from_fen("rnbqkbnr/pppppppp/8/8/8/8/8/RNBQKBNR w KQkq - 0 1")
            .expect("valid FEN");
        assert_eq!(pos.extinction_loser(), Some(Color::White));
        assert!(pos.legal_moves().is_empty(), "terminal — no moves");
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::Black
            }),
        );
    }
}
