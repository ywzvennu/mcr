//! Atomar (8x8) on the generic engine — **nocheckatomic with blast-immune,
//! mutually-immune Commoners**. Validated against Fairy-Stockfish `UCI_Variant
//! atomar` (a built-in; `atomar_variant()`, `variant.cpp:533`).
//!
//! Atomar is [Nocheckatomic](super::nocheckatomic) — atomic chess with a non-royal
//! Commoner king, won by making the enemy Commoner go extinct — plus the two
//! "diplomatic immunity" rules of the [atomar rules][pdf]:
//!
//! * **Commoners are blast-immune** (FSF `blastImmuneTypes = COMMONER`): a Commoner
//!   on a square adjacent to an explosion **survives** it, where in nocheckatomic it
//!   would be blown up. Because Fairy-Stockfish builds the blast set as
//!   `((neighbours & non-pawns) | destination) & ~blastImmune`, the immunity is
//!   applied *after* the capturer's own landing square is added, so a **capturing
//!   Commoner even survives its own capture** — it removes what it took and the
//!   surrounding non-pawns but stays on the board. Modelled by
//!   [`role_is_blast_immune`](WideVariant::role_is_blast_immune).
//! * **Commoners are mutually immune** (FSF `mutuallyImmuneTypes = COMMONER`, "the
//!   diplomacy rule"): a Commoner may stand beside the enemy Commoner but may
//!   **never capture it** — a Commoner-takes-Commoner move is illegal and is never
//!   generated. Modelled by
//!   [`role_is_mutually_immune`](WideVariant::role_is_mutually_immune).
//!
//! Everything else — standard 8x8 movement, castling, en passant, Q/R/B/N
//! promotion, the blast on every capture, and the win-by-Commoner-extinction
//! terminal — is inherited from nocheckatomic's rule set (re-declared here rather
//! than delegated, since a [`WideVariant`] is a zero-sized marker type). The two
//! immunities make Atomar diverge from Nocheckatomic wherever a Commoner sits next
//! to a capture or the two Commoners face each other.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant atomar` (the standard array, the
//! Commoner spelled `k`/`K`):
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! ```
//!
//! [pdf]: https://web.archive.org/web/20230519082613/https://chronatog.com/wp-content/uploads/2021/09/atomar-chess-rules.pdf

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    Bitboard, Board, Chess8x8, ExtinctionRule, PromotionConfig, WideRole, WideVariant,
};
use crate::Color;

/// The standard 8x8 starting placement (Atomar shares the chess array).
const ATOMAR_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

/// The single watched type: the non-royal king / Commoner ([`WideRole::King`]). A
/// side loses when it owns zero of it.
const ATOMAR_WATCHED: &[WideRole] = &[WideRole::King];

/// The Atomar rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// Identical to [`NocheckatomicRules`](super::nocheckatomic::NocheckatomicRules) —
/// non-royal Commoner, blast on capture, win by Commoner extinction — with the two
/// Commoner immunities added: [`role_is_blast_immune`](WideVariant::role_is_blast_immune)
/// and [`role_is_mutually_immune`](WideVariant::role_is_mutually_immune) both flag
/// the Commoner ([`WideRole::King`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct AtomarRules;

impl WideVariant<Chess8x8> for AtomarRules {
    /// The tightest prefix of `WideRole::ALL` covering every fieldable role
    /// (Pawn..King; promotions are Q/R/B/N). See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(ATOMAR_START_PLACEMENT)
            .expect("the Atomar starting placement is valid on an 8x8 board");
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
        };
        (board, state)
    }

    // --- non-royal Commoner king (no check) -------------------------------

    fn non_royal_king() -> bool {
        true
    }

    fn royal_squares<const R: usize>(
        _board: &Board<Chess8x8, R>,
        _color: Color,
    ) -> Bitboard<Chess8x8> {
        Bitboard::EMPTY
    }

    // --- atomic blast on capture, with immune Commoners -------------------

    fn blast_on_capture() -> bool {
        true
    }

    /// The Commoner ([`WideRole::King`]) is **blast-immune** (FSF
    /// `blastImmuneTypes = COMMONER`): it survives an explosion on an adjacent
    /// square, and — since it is spared even at the blast centre — a capturing
    /// Commoner survives its own capture.
    fn role_is_blast_immune(role: WideRole) -> bool {
        role == WideRole::King
    }

    /// The Commoner ([`WideRole::King`]) is **mutually immune** (FSF
    /// `mutuallyImmuneTypes = COMMONER`): a Commoner may never capture the enemy
    /// Commoner, so that capture is never generated.
    fn role_is_mutually_immune(role: WideRole) -> bool {
        role == WideRole::King
    }

    // --- Commoner-extinction terminal (threshold 0) -----------------------

    fn extinction_rule() -> Option<ExtinctionRule> {
        Some(ExtinctionRule {
            watched: ATOMAR_WATCHED,
            threshold: 0,
            count_total: false,
            extinct_wins: false,
            opponent_min: 0,
        })
    }

    // --- promotion (standard Q/R/B/N) -------------------------------------

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

/// Atomar as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Atomar::startpos`](GenericPosition::startpos) or parse a plain-chess FEN with
/// [`Atomar::from_fen`](GenericPosition::from_fen). Like Nocheckatomic every capture
/// detonates, but Commoners survive adjacent blasts and can never take one another.
pub type Atomar =
    GenericPosition<Chess8x8, AtomarRules, { <AtomarRules as WideVariant<Chess8x8>>::ROLE_SPAN }>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{perft as gperft, Chess8x8, Square, WideRole};

    fn sq(s: &str) -> Square<Chess8x8> {
        let b = s.as_bytes();
        Square::<Chess8x8>::from_file_rank(b[0] - b'a', b[1] - b'1').unwrap()
    }

    /// The canonical start FEN round-trips and is not terminal.
    #[test]
    fn startpos_fen_round_trips() {
        let pos = Atomar::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
        assert_eq!(pos.legal_move_count(), 20);
        assert_eq!(pos.end_reason(), None);
    }

    /// **Blast immunity:** a Commoner adjacent to an explosion survives, where in
    /// nocheckatomic it would be blown up.
    #[test]
    fn commoner_survives_adjacent_blast() {
        // White queen d1 captures the rook on d8. The blast would catch the black
        // king on c8 (adjacent to d8) — but in Atomar the Commoner is immune, so it
        // survives; the queen, the rook, and the adjacent knight on e8... here the
        // adjacent pawn e7 (immune, pawn) survives and the king c8 survives.
        let pos = Atomar::from_fen("2kr4/4p3/8/8/8/8/8/3QK3 w - - 0 1").unwrap();
        let mv = pos
            .legal_moves()
            .into_iter()
            .find(|m| m.from::<Chess8x8>() == sq("d1") && m.to::<Chess8x8>() == sq("d8"))
            .expect("Qxd8 is legal");
        let after = pos.play(&mv);
        let b = after.board();
        assert!(b.piece_at(sq("d8")).is_none(), "captured rook gone");
        assert!(b.piece_at(sq("d1")).is_none(), "capturing queen gone");
        assert_eq!(
            b.piece_at(sq("c8")).map(|p| p.role),
            Some(WideRole::King),
            "adjacent Commoner is blast-immune and survives"
        );
        // Black keeps its Commoner, so the game is not over.
        assert_eq!(after.extinction_loser(), None);
        assert_eq!(after.end_reason(), None);
    }

    /// **Capturing-Commoner immunity:** a Commoner that captures survives its own
    /// blast (it is spared even at the centre), unlike in nocheckatomic.
    #[test]
    fn capturing_commoner_survives_own_blast() {
        // White Commoner e4 captures the rook on e5; the blast centre e5 would
        // remove the capturing Commoner in nocheckatomic, but it is blast-immune in
        // Atomar and stays on e5.
        let pos = Atomar::from_fen("8/8/8/4r3/4K3/8/8/4k3 w - - 0 1").unwrap();
        let mv = pos
            .legal_moves()
            .into_iter()
            .find(|m| m.from::<Chess8x8>() == sq("e4") && m.to::<Chess8x8>() == sq("e5"))
            .expect("Kxe5 is legal");
        let after = pos.play(&mv);
        let b = after.board();
        assert!(b.piece_at(sq("e5")).map(|p| p.role) == Some(WideRole::King));
        assert!(b.piece_at(sq("e4")).is_none(), "the Commoner left e4");
    }

    /// **Mutual immunity:** a Commoner may stand beside the enemy Commoner but may
    /// never capture it, so no Commoner-takes-Commoner move is generated.
    #[test]
    fn commoner_cannot_capture_commoner() {
        // Adjacent Commoners on e4 (White) and e5 (Black). White's Commoner has no
        // Kxe5 move.
        let pos = Atomar::from_fen("8/8/8/4k3/4K3/8/8/8 w - - 0 1").unwrap();
        assert!(
            !pos.legal_moves()
                .iter()
                .any(|m| m.from::<Chess8x8>() == sq("e4") && m.to::<Chess8x8>() == sq("e5")),
            "a Commoner may not capture the enemy Commoner"
        );
        // It may still step to the empty squares beside it.
        assert!(pos
            .legal_moves()
            .iter()
            .any(|m| m.from::<Chess8x8>() == sq("e4") && m.to::<Chess8x8>() == sq("d5")));
    }

    /// **Adjudication test (coverage-gate registered):** directly capturing the
    /// enemy Commoner wins — the immunities spare only pieces caught in a blast, not
    /// the piece a capture takes outright. A rook takes the lone black Commoner, so
    /// Black is extinct and White wins.
    #[test]
    fn capturing_the_commoner_wins() {
        use crate::geometry::{WideEndReason, WideOutcome};
        let pos = Atomar::from_fen("4k3/8/8/8/8/8/8/4RK2 w - - 0 1").unwrap();
        assert_eq!(pos.end_reason(), None, "not yet terminal");
        let mv = pos
            .legal_moves()
            .into_iter()
            .find(|m| m.from::<Chess8x8>() == sq("e1") && m.to::<Chess8x8>() == sq("e8"))
            .expect("Rxe8 captures the Commoner");
        let after = pos.play(&mv);
        assert_eq!(
            after.board().pieces(Color::Black, WideRole::King).count(),
            0,
            "Black's Commoner was captured"
        );
        assert_eq!(after.extinction_loser(), Some(Color::Black));
        assert_eq!(after.end_reason(), Some(WideEndReason::VariantWin));
        assert_eq!(
            after.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::White
            })
        );
        assert!(after.legal_moves().is_empty(), "terminal — no continuation");
    }

    /// Startpos perft matches the FSF-confirmed counts (pinned in full in
    /// `tests/perft_atomar.rs`).
    #[test]
    fn start_perft_matches_fsf() {
        let pos = Atomar::startpos();
        assert_eq!(gperft::<Chess8x8, _, _>(&pos, 1), 20);
        assert_eq!(gperft::<Chess8x8, _, _>(&pos, 2), 400);
        assert_eq!(gperft::<Chess8x8, _, _>(&pos, 3), 8902);
        assert_eq!(gperft::<Chess8x8, _, _>(&pos, 4), 197_779);
    }

    /// The adjacent-Commoners position diverges from nocheckatomic: mutual immunity
    /// removes the Kxe5 capture, so the count is one lower at depth 1.
    #[test]
    fn adjacent_commoners_perft_reflects_mutual_immunity() {
        let pos = Atomar::from_fen("8/8/8/4k3/4K3/8/8/8 w - - 0 1").unwrap();
        assert_eq!(gperft::<Chess8x8, _, _>(&pos, 1), 7);
        assert_eq!(gperft::<Chess8x8, _, _>(&pos, 2), 52);
        assert_eq!(gperft::<Chess8x8, _, _>(&pos, 3), 397);
    }
}
