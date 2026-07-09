//! Koedem — "King of the dead" (8x8) on the generic engine: **Crazyhouse-style
//! chess where the king is a non-royal Commoner you must re-drop, and you win by
//! owning *every* king on the board**. Validated against Fairy-Stockfish
//! `UCI_Variant koedem` (a built-in; `koedem_variant()`, `variant.cpp:673`, itself
//! a Bughouse derivative).
//!
//! Koedem keeps standard 8x8 chess movement — the same sliders, leapers, pawns
//! (double push, diagonal capture, en passant, promotion to Q/R/B/N), and castling
//! — plus the Bughouse hand/drop machinery, but with three twists that together
//! make the king "of the dead":
//!
//! * **The king is a non-royal Commoner.** It still steps one square in any
//!   direction, but it is an ordinary, capturable piece: there is no check and no
//!   checkmate, a king may stand next to the enemy king, and capturing a king is a
//!   legal move ([`non_royal_king`](WideVariant::non_royal_king), like Extinction /
//!   Three-kings). This is why Koedem's start perft already exceeds standard chess
//!   (perft(4) = `197742`, chess = `197281`): no move is barred for leaving a king
//!   attacked.
//! * **Drops with the hand fed only from FEN.** Like [`Bughouse`](crate::geometry::Bughouse), Koedem has a
//!   crazyhouse hand ([`has_hand`](WideVariant::has_hand)) but a capture does **not**
//!   bank the taken piece ([`captures_to_hand`](WideVariant::captures_to_hand) is
//!   `false`, FSF's `twoBoards`): reserves ride in the FEN's `[..]` bracket. A held
//!   piece drops onto any empty square (a Pawn only on ranks 2-7).
//! * **Must-drop the Commoner.** While a side holds a king (Commoner) in hand it
//!   **must drop it before anything else** (FSF `mustDrop`, `mustDropType =
//!   COMMONER`): its only legal moves are king drops, until the reserve is emptied
//!   ([`must_drop_role`](WideVariant::must_drop_role)).
//!
//! ## Winning — own all the kings
//!
//! A side loses the moment it holds **no** Commoner while the opponent owns **both**
//! (FSF `extinctionValue = -VALUE_MATE`, `extinctionPieceTypes = COMMONER`,
//! `extinctionOpponentPieceCount = 2`). This reuses the generic
//! [`WideVariant::extinction_rule`] with Koedem's dials: watch the king
//! (`[King]`) at `threshold = 0`, but only decisively when the opponent's count —
//! **taken with the hand**, so a captured king that entered a reserve still counts
//! — reaches `opponent_min = 2`. Losing your last king while the opponent owns only
//! one is **not** yet a loss (the game plays on with your other pieces), exactly as
//! Fairy-Stockfish adjudicates. A side that is genuinely out of moves loses
//! (stalemate is a loss, FSF `stalemateValue = -VALUE_MATE`, via
//! [`stalemate_is_loss`](WideVariant::stalemate_is_loss)); there is no check, so a
//! no-move node is always this stalemate loss or the extinction terminal above.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant koedem` — the standard array with
//! an empty hand:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1
//! ```
//!
//! Koedem shares the standard-chess dialect byte-for-byte (the king is spelled
//! `k`/`K` — the Commoner demotion is a *rule*, not a letter), and the hand rides
//! the same `[..]` bracket FSF uses.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    Bitboard, Board, Chess8x8, ExtinctionRule, Geometry, Square, WideRole, WideVariant,
};
use crate::Color;

/// The standard 8x8 starting placement (Koedem shares the chess array).
const KOEDEM_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

/// The single watched type: the non-royal king / Commoner ([`WideRole::King`]). A
/// side loses when it owns zero of it while the opponent owns both.
const KOEDEM_WATCHED: &[WideRole] = &[WideRole::King];

/// The Koedem rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It combines the Bughouse hand/drop machinery ([`has_hand`](WideVariant::has_hand),
/// [`captures_to_hand`](WideVariant::captures_to_hand) `false`) with a non-royal
/// Commoner king (like Extinction), the `mustDrop` Commoner rule
/// ([`must_drop_role`](WideVariant::must_drop_role)), and the "own all the kings"
/// terminal ([`extinction_rule`](WideVariant::extinction_rule) with
/// `opponent_min = 2`). Movement, castling, en passant, and Q/R/B/N promotion are
/// the standard trait defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct KoedemRules;

impl KoedemRules {
    /// The squares a Pawn may **not** be dropped onto: the first and last rank,
    /// where a dropped pawn would sit on a promotion rank (the crazyhouse rule,
    /// shared with [`Bughouse`]).
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

impl WideVariant<Chess8x8> for KoedemRules {
    /// The tightest prefix of `WideRole::ALL` covering every fieldable role
    /// (Pawn..King, the standard army; a held Commoner drops as a King, still
    /// within the prefix). See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(KOEDEM_START_PLACEMENT)
            .expect("the Koedem starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            // Standard castling rights: Koedem keeps ordinary castling (the
            // non-royal king is never restricted by attacked squares).
            castling: GenericCastling::standard::<Chess8x8>(),
            ep_square: None,
            ep_captured: None,
            gating: GenericGating::NONE,
            duck: None,
            // The hand starts empty; reserves ride in the FEN's `[..]` bracket.
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
        // The king is a non-royal Commoner: every pseudo-legal board move is legal
        // (no check mask, no pins, no king-danger filter), a king may step onto an
        // attacked square, and capturing a king is a legal move.
        true
    }

    fn royal_squares<const R: usize>(
        _board: &Board<Chess8x8, R>,
        _color: Color,
    ) -> Bitboard<Chess8x8> {
        // The king is **not royal**: an empty royal set makes the generic
        // king-safety machinery report "never in check". A side loses not by
        // checkmate but by owning no Commoner while the opponent owns all (below).
        Bitboard::EMPTY
    }

    // --- crazyhouse hand + drops, fed externally (Bughouse's `twoBoards`) --

    fn has_hand() -> bool {
        true
    }

    fn captures_to_hand() -> bool {
        // FSF `twoBoards`: a captured piece is *not* banked into the captor's hand.
        // Reserves ride in the FEN's `[..]` bracket only. This is what makes Koedem
        // (like Bughouse) diverge from Crazyhouse.
        false
    }

    fn pawn_is_stepper() -> bool {
        // Koedem pawns are ordinary chess pawns (double push, diagonal capture, en
        // passant, last-rank promotion), not Shogi forward steppers.
        false
    }

    fn drop_targets<const R: usize>(
        role: WideRole,
        _color: Color,
        board: &Board<Chess8x8, R>,
    ) -> Bitboard<Chess8x8> {
        // Every empty square (crazyhouse) — except a Pawn may not be dropped on the
        // first or last rank. A held Commoner (King) drops onto every empty square,
        // exactly as FSF's `mustDrop` re-deployment generates.
        let empty = !board.occupied();
        if role == WideRole::Pawn {
            empty & !Self::pawn_forbidden_ranks()
        } else {
            empty
        }
    }

    // --- must-drop the Commoner (FSF `mustDrop` / `mustDropType`) ----------

    fn must_drop_role() -> Option<WideRole> {
        // While a side holds a king (Commoner) in hand it must drop it before any
        // other move: its legal moves narrow to Commoner drops only.
        Some(WideRole::King)
    }

    // --- own-all-Commoners terminal ---------------------------------------

    /// A side loses the moment it owns **no** Commoner while the opponent owns
    /// **both** (`extinctionOpponentPieceCount = 2`), counted with the hand so a
    /// king held in a reserve still counts toward "owning all".
    fn extinction_rule() -> Option<ExtinctionRule> {
        Some(ExtinctionRule {
            watched: KOEDEM_WATCHED,
            threshold: 0,
            count_total: false,
            extinct_wins: false,
            opponent_min: 2,
        })
    }

    /// A stalemated side (no legal move, and — being non-royal — never in check)
    /// **loses** (FSF `stalemateValue = -VALUE_MATE`).
    fn stalemate_is_loss() -> bool {
        true
    }

    /// Koedem inherits the western three-fold repetition draw (history-dependent,
    /// resolved at the [`GenericGame`](crate::geometry::game::GenericGame) level and
    /// never consulted by a bare position, so perft is unchanged), like
    /// [`Bughouse`](crate::geometry::Bughouse).
    fn tracks_repetition() -> bool {
        true
    }
}

/// Koedem as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Koedem::startpos`](GenericPosition::startpos) or parse a FEN — the placement
/// may carry the hand as a `[..]` bracket — with
/// [`Koedem::from_fen`](GenericPosition::from_fen). Movement is the no-check
/// standard-chess set plus crazyhouse-style drops; a side that holds a Commoner in
/// hand must drop it, and a side that owns no Commoner while the opponent owns both
/// has lost.
pub type Koedem =
    GenericPosition<Chess8x8, KoedemRules, { <KoedemRules as WideVariant<Chess8x8>>::ROLE_SPAN }>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{perft as gperft, Chess8x8, WideEndReason, WideOutcome};

    /// The canonical start FEN round-trips (with the empty hand and standard
    /// castling) and the startpos is not terminal.
    #[test]
    fn startpos_fen_round_trips() {
        let pos = Koedem::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1"
        );
        assert_eq!(pos.turn(), Color::White);
        assert_eq!(pos.legal_move_count(), 20);
        assert_eq!(pos.end_reason(), None);
        assert_eq!(pos.outcome(), None);
    }

    /// The king is non-royal: a side may leave its king attacked and may step onto
    /// an attacked square — there is no check.
    #[test]
    fn king_is_non_royal_no_check() {
        let pos = Koedem::from_fen("8/8/8/8/8/8/8/r3K3[] w - - 0 1").expect("valid FEN");
        assert!(!pos.is_check(), "a non-royal king is never in check");
        let sq = |f, r| Square::<Chess8x8>::from_file_rank(f, r).unwrap();
        // Ke1-e2 steps onto the rook's file (an attacked square) — legal here.
        assert!(pos
            .legal_moves()
            .iter()
            .any(|m| m.from::<Chess8x8>() == sq(4, 0) && m.to::<Chess8x8>() == sq(4, 1)));
    }

    /// **Adjudication test (coverage-gate registered):** a side that holds a
    /// Commoner in hand **must drop it before anything else** (FSF `mustDrop`).
    /// White has an extra king in hand over the standard array: its only legal
    /// moves are king drops, one per empty square.
    #[test]
    fn forced_commoner_drop() {
        let pos = Koedem::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[K] w KQkq - 0 1")
            .expect("valid FEN");
        let moves = pos.legal_moves();
        assert!(
            moves
                .iter()
                .all(|m| m.is_drop() && m.drop_role() == Some(WideRole::King)),
            "every legal move must be a Commoner drop while one is held in hand",
        );
        // 64 squares minus the 32 occupied = 32 empty king-drop squares.
        assert_eq!(moves.len(), 32, "one king drop per empty square");
        assert_eq!(pos.end_reason(), None);
    }

    /// A held non-Commoner does **not** force a drop: with a rook (but no king) in
    /// hand, ordinary board moves and rook drops are all legal.
    #[test]
    fn non_commoner_in_hand_does_not_force_a_drop() {
        let pos = Koedem::from_fen("4k3/8/8/8/8/8/8/4K3[R] w - - 0 1").expect("valid FEN");
        let moves = pos.legal_moves();
        assert!(
            moves.iter().any(|m| !m.is_drop()),
            "board moves stay legal when no Commoner is held",
        );
        assert!(
            moves
                .iter()
                .any(|m| m.is_drop() && m.drop_role() == Some(WideRole::Rook)),
            "the held rook is droppable",
        );
    }

    /// **Adjudication test (coverage-gate registered):** capturing the enemy's last
    /// Commoner while owning both **wins**. White holds two kings (d1, f1) and Black
    /// one (d2); White's `Kd1`x`d2` removes Black's last Commoner, leaving Black
    /// with zero and White with two — a decisive win for White.
    #[test]
    fn capturing_last_commoner_wins() {
        let pos = Koedem::from_fen("8/8/8/8/8/8/3k4/3K1K2[] w - - 0 1").expect("valid FEN");
        assert_eq!(
            pos.end_reason(),
            None,
            "not yet terminal — Black owns a king"
        );
        let kxd2 = pos
            .legal_moves()
            .into_iter()
            .find(|m| m.to_uci::<Chess8x8>() == "d1d2")
            .expect("Kd1xd2 is legal");
        let after = pos.play(&kxd2);
        assert_eq!(
            after.board().pieces(Color::Black, WideRole::King).count(),
            0,
            "Black has no Commoner left",
        );
        assert_eq!(after.extinction_loser(), Some(Color::Black));
        assert_eq!(after.end_reason(), Some(WideEndReason::VariantWin));
        assert_eq!(
            after.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::White
            }),
        );
        assert!(
            after.legal_moves().is_empty(),
            "an owned-all-Commoners position is a terminal perft leaf",
        );
    }

    /// Losing your last Commoner while the opponent owns only **one** is *not* a
    /// loss (the game plays on with your other pieces) — the `opponent_min = 2`
    /// clause, matching FSF: Black has no king but a rook, White owns a single king.
    #[test]
    fn losing_last_king_is_not_yet_a_loss() {
        let pos = Koedem::from_fen("r7/8/8/8/8/8/8/5K2[] b - - 0 1").expect("valid FEN");
        assert_eq!(
            pos.extinction_loser(),
            None,
            "White owns only one Commoner, so Black is not yet extinct",
        );
        assert_eq!(pos.end_reason(), None, "the game plays on");
        assert!(!pos.legal_moves().is_empty(), "Black still has rook moves");
    }

    /// A side genuinely out of moves loses (stalemate is a loss, FSF
    /// `stalemateValue = -VALUE_MATE`): Black has zero pieces and cannot move.
    #[test]
    fn no_move_is_a_loss_for_side_to_move() {
        let pos = Koedem::from_fen("8/8/8/8/8/8/8/K7[] b - - 0 1").expect("valid FEN");
        assert!(pos.legal_moves().is_empty(), "Black has no move");
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::White
            }),
        );
    }

    /// The empty hand and a seeded reserve round-trip through FEN.
    #[test]
    fn hand_round_trips_through_fen() {
        for fen in [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[Kk] w KQkq - 0 1",
            "4k3/8/8/8/8/8/8/4K3[PNBRQpnbrq] w - - 0 1",
        ] {
            let pos = Koedem::from_fen(fen).expect("valid FEN");
            assert_eq!(pos.to_fen(), fen, "round trip for {fen}");
        }
    }

    /// Startpos perft matches the FSF-confirmed counts (pinned in full in
    /// `tests/perft_koedem.rs`); the no-check king already lifts perft above chess.
    #[test]
    fn start_perft_matches_fsf() {
        let pos = Koedem::startpos();
        assert_eq!(gperft::<Chess8x8, _, _>(&pos, 1), 20);
        assert_eq!(gperft::<Chess8x8, _, _>(&pos, 2), 400);
        assert_eq!(gperft::<Chess8x8, _, _>(&pos, 3), 8902);
        assert_eq!(gperft::<Chess8x8, _, _>(&pos, 4), 197_742);
    }
}
