//! Kinglet chess (8x8) on the generic engine — **standard chess movement in which
//! a side loses the moment it has no pawns left**. Validated against
//! Fairy-Stockfish `UCI_Variant kinglet` (a built-in; `extinction_variant()` base,
//! `variant.cpp`).
//!
//! Kinglet chess keeps every standard chess move — the same pieces, castling, and
//! en passant — but with two twists over standard chess:
//!
//! * **there is no check and no checkmate**: the king is a non-royal **Commoner**
//!   (it still steps one square in any direction, but it is an ordinary, capturable
//!   piece), exactly like Extinction chess;
//! * **pawns may only promote to a (non-royal) Commoner/King** — never to Queen,
//!   Rook, Bishop, or Knight;
//! * a side **loses by pawn extinction**: the game ends the instant a side holds
//!   **zero** pawns. Nothing else is watched — losing every knight, rook, bishop,
//!   queen, or even every king is fine; only the Pawn count decides the game.
//!
//! Because only pawns are watched and a promoting pawn becomes a Commoner (leaving
//! the Pawn type), **promoting your last pawn loses**, and so does having your last
//! pawn captured.
//!
//! ## The shared extinction terminal — reused, watching only pawns
//!
//! The loss condition is the **generic** [`WideVariant::extinction_rule`] hook: an
//! [`ExtinctionRule`] naming the *watched* piece types and the *threshold* at which
//! any of them counts as extinct. Kinglet watches only the **Pawn** type
//! (`[Pawn]`) with `threshold = 0` — the same hook Extinction chess rides with the
//! whole army. Extinction chess (`[Pawn, Knight, Bishop, Rook, Queen, King]`, 0),
//! Codrus (`[King]`, 0), and Three-kings (`[King]`, 1) are the sibling variants that
//! reuse this terminal with a different slice / threshold.
//!
//! ## King handling — a non-royal Commoner
//!
//! The non-royal king reuses the exact machinery Fog of War / Duck / Extinction
//! introduced: an empty [`royal_squares`](WideVariant::royal_squares) set makes the
//! generic king-safety code report "never in check", and
//! [`non_royal_king`](WideVariant::non_royal_king) routes the standard generator
//! through its non-royal branch (every pseudo-legal board move is legal, the king
//! has no check mask / pin / king-danger filter). Castling stays enabled and is
//! **never** restricted by attacked squares. Because there is no check, the
//! extinction rule is the game's only decisive terminal.
//!
//! ## Promotion — the Commoner only
//!
//! Kinglet's one movegen difference from Extinction chess: a pawn reaching the last
//! rank may promote **only** to a non-royal Commoner/King. Queen, Rook, Bishop, and
//! Knight promotions — legal in standard chess and in Extinction — are absent, so a
//! pawn on the seventh rank has a single promotion target rather than five. This
//! shrinks the move tree at every promotion node relative to Extinction chess.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant kinglet`:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! ```
//!
//! Kinglet chess shares the standard-chess dialect byte-for-byte (the king is
//! spelled `k`/`K` — the Commoner demotion is a *rule*, not a letter). The move set
//! is the no-check set, so at low depths where no promotion or pawn-count terminal
//! bites the counts equal standard chess (startpos perft 1/2/3 = `20`/`400`/`8902`);
//! deeper the counts diverge as the no-check moves lift them and the
//! Commoner-only promotion / pawn-extinction truncation lower them.
//!
//! [`ExtinctionRule`]: crate::geometry::ExtinctionRule

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    Bitboard, Board, Chess8x8, ExtinctionRule, PromotionConfig, WideRole, WideVariant,
};
use crate::Color;

/// The standard 8x8 starting placement (Kinglet shares the chess array).
const KINGLET_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

/// The only piece type Kinglet chess watches: the Pawn. A side loses the instant
/// its pawn count drops to zero (`threshold = 0`).
const KINGLET_WATCHED: &[WideRole] = &[WideRole::Pawn];

/// The Kinglet chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides only what Kinglet changes about standard chess: the king is a
/// non-royal Commoner (via [`WideVariant::royal_squares`] and
/// [`WideVariant::non_royal_king`], like Extinction), pawns promote **only** to a
/// Commoner/King (via [`WideVariant::promotion_config`]), and the game ends by
/// pawn extinction (via [`WideVariant::extinction_rule`]). Every other piece's
/// movement, castling, and en passant rule is the standard trait default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct KingletRules;

impl WideVariant<Chess8x8> for KingletRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field (Pawn..King, the standard army; the only promotion target
    /// is the King, within the prefix). See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(KINGLET_START_PLACEMENT)
            .expect("the Kinglet starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            // Standard castling rights: Kinglet keeps ordinary castling (the
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
            petrified: Bitboard::EMPTY,
            checks_against: [0, 0],
            jieqi_seed: None,
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

    fn royal_squares<const R: usize>(
        _board: &Board<Chess8x8, R>,
        _color: Color,
    ) -> Bitboard<Chess8x8> {
        // The king is **not royal**: an empty royal set makes the generic
        // king-safety machinery report "never in check". A side loses not by
        // checkmate but by pawn extinction (below).
        Bitboard::EMPTY
    }

    // --- pawn-extinction terminal (watching only pawns, threshold 0) ------

    /// A side loses the moment it has no pawns left. Kinglet watches **only** the
    /// Pawn type with `threshold = 0` (FSF `extinctionPieceTypes = {PAWN}`,
    /// `extinctionValue = -VALUE_MATE`, `extinctionPieceCount = 0`).
    fn extinction_rule() -> Option<ExtinctionRule> {
        Some(ExtinctionRule {
            watched: KINGLET_WATCHED,
            threshold: 0,
            count_total: false,
            extinct_wins: false,
            opponent_min: 0,
        })
    }

    // --- promotion (Commoner only) ----------------------------------------

    /// A pawn may promote **only** to a non-royal Commoner/King — never to Queen,
    /// Rook, Bishop, or Knight. This is Kinglet's one movegen difference from
    /// Extinction chess (which offers Q/R/B/N/King); FSF's `kinglet` promotion set
    /// is `{COMMONER}` alone. Since only pawns are watched, promoting empties the
    /// Pawn type, so promoting your **last** pawn loses.
    fn promotion_config() -> PromotionConfig {
        PromotionConfig {
            roles: alloc::vec![WideRole::King],
        }
    }
}

/// Kinglet chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Kinglet::startpos`](GenericPosition::startpos) or parse a plain-chess FEN with
/// [`Kinglet::from_fen`](GenericPosition::from_fen). Movement is the no-check
/// standard-chess set with Commoner-only promotion; the game ends by pawn
/// extinction.
pub type Kinglet =
    GenericPosition<Chess8x8, KingletRules, { <KingletRules as WideVariant<Chess8x8>>::ROLE_SPAN }>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Chess8x8, WideOutcome};

    /// The canonical start FEN round-trips and keeps standard castling rights, and
    /// the startpos is not (yet) terminal — both sides field their eight pawns.
    #[test]
    fn startpos_fen_round_trips_with_castling() {
        let pos = Kinglet::startpos();
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
    /// attacked — there is no self-check filter. Both sides still have pawns here,
    /// so nobody is (yet) extinct.
    #[test]
    fn king_is_non_royal_no_check() {
        use crate::geometry::Square;
        let pos =
            Kinglet::from_fen("1r1qkbn1/p7/8/4r3/8/8/P7/RNBQK3 w - - 0 1").expect("valid FEN");
        assert_eq!(
            pos.end_reason(),
            None,
            "not terminal — both sides have pawns"
        );
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

    /// **Promotion is Commoner-only:** a pawn reaching the last rank has exactly one
    /// promotion target (the King/Commoner), not the five of Extinction chess.
    #[test]
    fn promotion_offers_only_the_commoner() {
        use crate::geometry::Square;
        // White pawn on a7, one step from promotion; both sides still have pawns so
        // the position is not terminal. Black's back rank is empty on the a-file.
        let pos = Kinglet::from_fen("4k3/P6p/8/8/8/8/7P/4K3 w - - 0 1").expect("valid FEN");
        let a8 = Square::<Chess8x8>::from_file_rank(0, 7).unwrap();
        let promotions: alloc::vec::Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.to::<Chess8x8>() == a8 && m.promotion().is_some())
            .collect();
        assert_eq!(
            promotions.len(),
            1,
            "exactly one promotion move (Commoner only), not Q/R/B/N/Commoner",
        );
        assert_eq!(
            promotions[0].promotion(),
            Some(WideRole::King),
            "the sole promotion target is the non-royal Commoner/King",
        );
    }

    /// **Adjudication test (coverage-gate registered):** a side that loses its last
    /// pawn has lost by extinction. White captures Black's only pawn; the resulting
    /// position has zero Black pawns, so it is terminal — a decisive win for White —
    /// and generates no moves.
    #[test]
    fn kinglet_last_pawn_loss() {
        // Black has a single pawn on b7; White's rook on b1 faces it up the open
        // b-file. Both sides otherwise hold non-pawn material (nobody is pre-extinct
        // in pawns), and White keeps its own pawns.
        let pos = Kinglet::from_fen("4k3/1p6/8/8/8/8/P7/1R2K3 w - - 0 1").expect("valid FEN");
        assert_eq!(pos.end_reason(), None, "not yet terminal — both have pawns");

        // Rxb7 removes Black's last pawn.
        let rxb7 = pos
            .legal_moves()
            .into_iter()
            .find(|m| {
                m.from::<Chess8x8>() == crate::geometry::Square::<Chess8x8>::new(1) // b1
                    && m.to::<Chess8x8>() == crate::geometry::Square::<Chess8x8>::new(49)
                // b7
            })
            .expect("Rxb7 is a legal move");
        let after = pos.play(&rxb7);

        assert_eq!(
            after.board().pieces(Color::Black, WideRole::Pawn).count(),
            0,
            "Black has no pawn left",
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
        // Terminal: the losing (pawn-extinct) side generates no continuation.
        assert!(
            after.legal_moves().is_empty(),
            "a pawn-extinct position is a terminal perft leaf",
        );
    }

    /// Promoting your **last** pawn loses: the pawn leaves the Pawn type (becoming a
    /// Commoner), emptying it. White's only pawn promotes and White is immediately
    /// pawn-extinct.
    #[test]
    fn promoting_last_pawn_loses() {
        use crate::geometry::Square;
        // White's sole pawn on a7 (Black keeps a pawn on h7, so only White is at
        // risk). Promoting a8 empties White's Pawn type.
        let pos = Kinglet::from_fen("4k3/P6p/8/8/8/8/8/4K3 w - - 0 1").expect("valid FEN");
        assert_eq!(
            pos.end_reason(),
            None,
            "not yet terminal — White still has a pawn"
        );

        let a8 = Square::<Chess8x8>::from_file_rank(0, 7).unwrap();
        let promo = pos
            .legal_moves()
            .into_iter()
            .find(|m| m.to::<Chess8x8>() == a8 && m.promotion().is_some())
            .expect("the a7 pawn can promote");
        let after = pos.play(&promo);

        assert_eq!(
            after.board().pieces(Color::White, WideRole::Pawn).count(),
            0,
            "White emptied its Pawn type by promoting its last pawn",
        );
        assert_eq!(after.extinction_loser(), Some(Color::White));
        assert_eq!(
            after.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::Black
            }),
        );
    }

    /// A position where a side already has no pawns is terminal at any depth — the
    /// same truncation Fairy-Stockfish's `go perft` reports as 0. Losing every
    /// *non-pawn* type, by contrast, is fine: only pawns are watched.
    #[test]
    fn zero_pawns_is_terminal_but_zero_non_pawns_is_not() {
        // White has no pawns (rank 2 empty); it has already lost.
        let no_pawns =
            Kinglet::from_fen("rnbqkbnr/pppppppp/8/8/8/8/8/RNBQKBNR w KQkq - 0 1").expect("valid");
        assert_eq!(no_pawns.extinction_loser(), Some(Color::White));
        assert!(no_pawns.legal_moves().is_empty(), "terminal — no moves");
        assert_eq!(
            no_pawns.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::Black
            }),
        );

        // A side with pawns but no queen (a non-pawn type at zero) is NOT terminal:
        // Kinglet watches only pawns.
        let no_queen =
            Kinglet::from_fen("rnb1kbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
                .expect("valid");
        assert_eq!(
            no_queen.extinction_loser(),
            None,
            "a missing queen is irrelevant — only pawns are watched",
        );
        assert!(
            !no_queen.legal_moves().is_empty(),
            "not terminal — both sides still have pawns",
        );
    }
}
