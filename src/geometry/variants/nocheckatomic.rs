//! Nocheckatomic (8x8) on the generic engine — **atomic chess with a non-royal
//! Commoner king**: every capture detonates, and you win by making the enemy's
//! Commoner go extinct, not by checkmate. Validated against Fairy-Stockfish
//! `UCI_Variant nocheckatomic` (a built-in; `nocheckatomic_variant()`,
//! `variant.cpp:510`, the ICC "atomic without checks" rules).
//!
//! Nocheckatomic keeps every standard 8x8 chess move — the same sliders, leapers,
//! pawns (double push, diagonal capture, en passant, promotion to Q/R/B/N), and
//! castling — with two twists that together make it atomic-without-check:
//!
//! * **The king is a non-royal Commoner** (FSF removes `KING`, adds `COMMONER 'k'`,
//!   `castlingKingPiece = COMMONER`). It still steps one square in any direction,
//!   but it is an ordinary, capturable piece: there is **no check and no
//!   checkmate**, a Commoner may stand next to the enemy Commoner, and a capture
//!   that blows up your *own* Commoner is a legal (losing) move. This reuses the
//!   exact machinery Extinction / Koedem introduced — an empty
//!   [`royal_squares`](WideVariant::royal_squares) and
//!   [`non_royal_king`](WideVariant::non_royal_king) — so every pseudo-legal move
//!   is legal.
//! * **Every capture detonates an atomic blast** (FSF `blastOnCapture`). The
//!   capturing piece and every **non-pawn** piece on the eight squares around the
//!   square it lands on are removed along with the captured piece; pawns adjacent
//!   to the blast survive. The blast is centred on the destination square for
//!   *every* capture, including en passant, and is applied in the shared move-apply
//!   path via [`blast_on_capture`](WideVariant::blast_on_capture).
//!
//! ## Winning — extinguish the enemy Commoner
//!
//! A side loses the moment it owns **no** Commoner (FSF `extinctionValue =
//! -VALUE_MATE`, `extinctionPieceTypes = COMMONER`), whether the Commoner was
//! captured directly or caught in a blast. This reuses the generic
//! [`extinction_rule`](WideVariant::extinction_rule) watching `[King]` at
//! `threshold = 0` — the same terminal Extinction and Codrus ride. Because the king
//! is non-royal there is no check, so this Commoner extinction is the only decisive
//! terminal. (The concrete, king-safe **atomic** — FSF's `extinctionPseudoRoyal` —
//! is a different, more restrictive variant that lives in the narrow layer.)
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant nocheckatomic` — the standard
//! array (the Commoner is spelled `k`/`K`; the demotion is a *rule*, not a letter):
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! ```

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    Bitboard, Board, Chess8x8, ExtinctionRule, PromotionConfig, WideRole, WideVariant,
};
use crate::Color;

/// The standard 8x8 starting placement (Nocheckatomic shares the chess array).
const NOCHECKATOMIC_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

/// The single watched type: the non-royal king / Commoner ([`WideRole::King`]). A
/// side loses when it owns zero of it — captured or blasted.
const NOCHECKATOMIC_WATCHED: &[WideRole] = &[WideRole::King];

/// The Nocheckatomic rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides only what atomic-without-check changes about standard chess: the
/// king is a non-royal Commoner (via [`WideVariant::royal_squares`] and
/// [`WideVariant::non_royal_king`], like Extinction), a capture detonates (via
/// [`WideVariant::blast_on_capture`]), and the game ends by Commoner extinction
/// (via [`WideVariant::extinction_rule`]). Movement, castling, en passant, and
/// Q/R/B/N promotion are the standard trait defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NocheckatomicRules;

impl WideVariant<Chess8x8> for NocheckatomicRules {
    /// The tightest prefix of `WideRole::ALL` covering every fieldable role
    /// (Pawn..King, the standard army; promotions are Q/R/B/N, within the prefix).
    /// See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(NOCHECKATOMIC_START_PLACEMENT)
            .expect("the Nocheckatomic starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            // Standard castling rights: the Commoner castles (the non-royal king is
            // never restricted by attacked squares).
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
        };
        (board, state)
    }

    // --- non-royal Commoner king (no check) -------------------------------

    fn non_royal_king() -> bool {
        // The king is a non-royal Commoner: every pseudo-legal board move is legal
        // (no check mask, no pins, no king-danger filter), a Commoner may step onto
        // an attacked square, capturing a Commoner is a legal move, and a capture
        // that blows up your own Commoner is legal (it simply loses).
        true
    }

    fn royal_squares<const R: usize>(
        _board: &Board<Chess8x8, R>,
        _color: Color,
    ) -> Bitboard<Chess8x8> {
        // The king is **not royal**: an empty royal set makes the generic
        // king-safety machinery report "never in check". A side loses by Commoner
        // extinction (below), not by checkmate.
        Bitboard::EMPTY
    }

    // --- atomic blast on capture ------------------------------------------

    fn blast_on_capture() -> bool {
        // Every capture detonates: the capturer and every non-pawn piece on the
        // eight adjacent squares are removed with the captured piece.
        true
    }

    // --- Commoner-extinction terminal (threshold 0) -----------------------

    /// A side loses the moment it owns **no** Commoner (FSF `extinctionValue =
    /// -VALUE_MATE`, `extinctionPieceTypes = COMMONER`, `extinctionPieceCount = 0`),
    /// whether captured or blasted.
    fn extinction_rule() -> Option<ExtinctionRule> {
        Some(ExtinctionRule {
            watched: NOCHECKATOMIC_WATCHED,
            threshold: 0,
            count_total: false,
            extinct_wins: false,
            opponent_min: 0,
        })
    }

    // --- promotion (standard Q/R/B/N) -------------------------------------

    /// A pawn promotes to Knight / Bishop / Rook / Queen — the standard set. Unlike
    /// Extinction, FSF's nocheckatomic does **not** add the Commoner to
    /// `promotionPieceTypes`, so each side always fields exactly one Commoner.
    fn promotion_config() -> PromotionConfig {
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Knight,
                WideRole::Bishop,
                WideRole::Rook,
                WideRole::Queen,
            ],
        }
    }
}

/// Nocheckatomic as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Nocheckatomic::startpos`](GenericPosition::startpos) or parse a plain-chess
/// FEN with [`Nocheckatomic::from_fen`](GenericPosition::from_fen). Movement is the
/// no-check standard-chess set; every capture detonates, and a side that owns no
/// Commoner has lost.
pub type Nocheckatomic = GenericPosition<
    Chess8x8,
    NocheckatomicRules,
    { <NocheckatomicRules as WideVariant<Chess8x8>>::ROLE_SPAN },
>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{
        perft as gperft, Chess8x8, Square, WideEndReason, WideOutcome, WideRole,
    };

    fn sq(s: &str) -> Square<Chess8x8> {
        let b = s.as_bytes();
        Square::<Chess8x8>::from_file_rank(b[0] - b'a', b[1] - b'1').unwrap()
    }

    /// The canonical start FEN round-trips with standard castling and is not
    /// terminal; the no-check king already lifts perft above chess.
    #[test]
    fn startpos_fen_round_trips() {
        let pos = Nocheckatomic::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
        assert_eq!(pos.turn(), Color::White);
        assert_eq!(pos.legal_move_count(), 20);
        assert_eq!(pos.end_reason(), None);
        assert_eq!(pos.outcome(), None);
        assert!(!pos.is_check(), "a non-royal Commoner is never in check");
    }

    /// A capture detonates: the capturer and every adjacent non-pawn is removed,
    /// while an adjacent pawn and a non-adjacent piece survive.
    #[test]
    fn capture_explodes_adjacent_non_pawns() {
        // White queen d1 captures the rook on d8. The blast centre d8 removes the
        // capturing queen, the captured rook, and the adjacent king on c8, sparing
        // the adjacent pawn on e7; the knight on b8 (not adjacent to d8) survives.
        let pos = Nocheckatomic::from_fen("1nkr4/4p3/8/8/8/8/8/3QK3 w - - 0 1").unwrap();
        let mv = pos
            .legal_moves()
            .into_iter()
            .find(|m| m.from::<Chess8x8>() == sq("d1") && m.to::<Chess8x8>() == sq("d8"))
            .expect("Qxd8 is legal");
        let after = pos.play(&mv);
        let b = after.board();
        assert!(b.piece_at(sq("d8")).is_none(), "captured rook gone");
        assert!(b.piece_at(sq("d1")).is_none(), "capturing queen gone");
        assert!(b.piece_at(sq("c8")).is_none(), "adjacent king destroyed");
        assert_eq!(
            b.piece_at(sq("e7")).map(|p| p.role),
            Some(WideRole::Pawn),
            "pawn adjacent to the blast survives"
        );
        assert_eq!(
            b.piece_at(sq("b8")).map(|p| p.role),
            Some(WideRole::Knight),
            "non-adjacent knight survives"
        );
        // Black's Commoner was blasted -> Black is extinct, White wins.
        assert_eq!(after.extinction_loser(), Some(Color::Black));
        assert_eq!(after.end_reason(), Some(WideEndReason::VariantWin));
        assert_eq!(
            after.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::White
            })
        );
    }

    /// A pawn adjacent to the blast survives, but a pawn that is the capturer or the
    /// captured piece dies (the centre and captured square are always cleared).
    #[test]
    fn capturing_pawn_and_captured_pawn_die() {
        // exd5: the captured pawn (blast centre d5) and the capturing pawn both go.
        let pos = Nocheckatomic::from_fen("4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1").unwrap();
        let mv = pos
            .legal_moves()
            .into_iter()
            .find(|m| m.from::<Chess8x8>() == sq("e4") && m.to::<Chess8x8>() == sq("d5"))
            .expect("exd5 is legal");
        let after = pos.play(&mv);
        let b = after.board();
        assert!(b.piece_at(sq("d5")).is_none(), "captured pawn gone");
        assert!(b.piece_at(sq("e4")).is_none(), "capturing pawn gone");
    }

    /// En passant blast is centred on the destination square, not the captured
    /// pawn's square (matching published atomic perft).
    #[test]
    fn en_passant_blast_centre_is_destination() {
        // exd6 e.p.: captured pawn d5 and capturing pawn (landing d6) both go; a
        // knight on c7 (adjacent to d6) is destroyed, a knight on c4 (adjacent only
        // to the captured pawn d5) survives.
        let pos = Nocheckatomic::from_fen("4k3/2n5/8/3pP3/2n5/8/8/4K3 w - d6 0 1").unwrap();
        let mv = pos
            .legal_moves()
            .into_iter()
            .find(|m| m.from::<Chess8x8>() == sq("e5") && m.to::<Chess8x8>() == sq("d6"))
            .expect("exd6 e.p. is legal");
        let after = pos.play(&mv);
        let b = after.board();
        assert!(b.piece_at(sq("d5")).is_none(), "captured pawn gone");
        assert!(b.piece_at(sq("d6")).is_none(), "capturing pawn gone");
        assert!(
            b.piece_at(sq("c7")).is_none(),
            "non-pawn adjacent to d6 destroyed"
        );
        assert_eq!(
            b.piece_at(sq("c4")).map(|p| p.role),
            Some(WideRole::Knight),
            "piece adjacent only to the captured pawn survives"
        );
    }

    /// The Commoner is non-royal: it may step onto an attacked square and may even
    /// capture the enemy Commoner (blowing itself up too) — every pseudo-legal move
    /// is legal.
    #[test]
    fn king_is_non_royal_no_check() {
        let pos = Nocheckatomic::from_fen("4k3/8/8/8/8/8/8/r3K3 w - - 0 1").unwrap();
        assert!(!pos.is_check());
        assert!(pos
            .legal_moves()
            .iter()
            .any(|m| m.from::<Chess8x8>() == sq("e1") && m.to::<Chess8x8>() == sq("e2")));
    }

    /// Startpos perft matches the FSF-confirmed counts (pinned in full in
    /// `tests/perft_nocheckatomic.rs`).
    #[test]
    fn start_perft_matches_fsf() {
        let pos = Nocheckatomic::startpos();
        assert_eq!(gperft::<Chess8x8, _, _>(&pos, 1), 20);
        assert_eq!(gperft::<Chess8x8, _, _>(&pos, 2), 400);
        assert_eq!(gperft::<Chess8x8, _, _>(&pos, 3), 8902);
        assert_eq!(gperft::<Chess8x8, _, _>(&pos, 4), 197_779);
    }

    /// Make/unmake round-trips through the blast: the whole-state `Undo` snapshot
    /// restores every piece a capture detonates.
    #[test]
    fn make_unmake_walk_through_blasts() {
        let mut pos = Nocheckatomic::from_fen(
            "r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4",
        )
        .unwrap();
        pos.assert_make_unmake_walk(3);
    }
}
