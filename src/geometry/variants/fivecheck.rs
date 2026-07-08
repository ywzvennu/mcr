//! Five-check (5check, 8x8) on the generic engine — **standard chess with a
//! single terminal twist: a side that delivers check five times wins
//! immediately.** Validated against Fairy-Stockfish `UCI_Variant 5check`.
//!
//! Five-check is Fairy-Stockfish's `fivecheck_variant()`: the ordinary
//! `chess_variant_base()` with `checkCounting = true` and a **five**-check goal
//! (its three-check sibling with the target raised to five). Movement, castling,
//! the pawn double step and en passant, promotion, and the ordinary
//! checkmate / stalemate terminations are **exactly** standard chess — this rule
//! layer reuses the reference [`StandardChess`] army and 8x8 [`Chess8x8`]
//! geometry and overrides only the terminal rule via
//! [`check_count_to_win`](WideVariant::check_count_to_win).
//!
//! ## The five-check rule
//!
//! A running per-side tally of checks **delivered against** each king is carried
//! in the position state ([`GenericState::checks_against`](crate::geometry::position::GenericState::checks_against)):
//! after each move that leaves the mover's opponent in check, the count against
//! that opponent rises by one. When the count against a king reaches **five**
//! that side has lost and the **checker** wins — reported before the ordinary
//! checkmate / stalemate test, so the fifth check wins even when it is not mate.
//!
//! **Move generation is identical to standard chess.** The check tally changes
//! only adjudication, never the legal-move set, so `5check` perft is byte-for-byte
//! standard-chess perft (exactly as the concrete three-check behaves). The counter
//! rides make/unmake and the FEN.
//!
//! ## FEN — the `5+5` remaining-checks field
//!
//! Five-check carries Fairy-Stockfish's `checkCounting` FEN slot, a `W+B` pair
//! between the en-passant field and the clocks, where `W`/`B` are the checks each
//! side may still be **given** before losing, counting down from five. The start
//! is `5+5` (no checks yet); once White has checked once the field reads `5+4`
//! (Black may be checked four more times), and a component of `0` means that side
//! has been checked five times and lost. Internally the state stores checks
//! *delivered against* each side, the inverse (`5 - remaining`).
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant 5check` (`position startpos`):
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 5+5 0 1
//! ```
//!
//! mcr and FSF spell the position byte-for-byte identically (standard-chess
//! letters, the `5+5` check field, no dialect rewrite).

use crate::geometry::position::{GenericPosition, GenericState};
use crate::geometry::{Board, Chess8x8, StandardChess, WideVariant};

/// The number of checks a side must deliver to win a five-check game.
const WIN_CHECKS: u8 = 5;

/// The five-check rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`] that
/// reuses every [`StandardChess`] rule and adds only the five-check terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FiveCheckRules;

impl WideVariant<Chess8x8> for FiveCheckRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field: the standard army (Pawn..King), whose promotions (Queen
    /// / Rook / Bishop / Knight) all fall within the same prefix. See
    /// [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    /// The standard 8x8 chess starting position (with full castling rights and a
    /// `5+5` check tally), delegated to [`StandardChess`].
    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        <StandardChess as WideVariant<Chess8x8>>::starting_position()
    }

    /// **Win on the fifth check.** The engine carries a per-side tally of checks
    /// delivered and, when it reaches five against a king, ends the game as a win
    /// for the checker. Every move-generation, castling, promotion, and ordinary
    /// terminal hook is the standard-chess default, so perft is byte-identical.
    fn check_count_to_win() -> Option<u8> {
        Some(WIN_CHECKS)
    }
}

/// Five-check (5check) as a [`GenericPosition`] over the 8x8 [`Chess8x8`]
/// geometry.
///
/// Construct the starting position with
/// [`FiveCheck::startpos`](GenericPosition::startpos) or parse a FEN — which may
/// carry the `5+5` remaining-checks field — with
/// [`FiveCheck::from_fen`](GenericPosition::from_fen). It is standard chess except
/// that **delivering the fifth check wins**; see the [module docs](self).
pub type FiveCheck = GenericPosition<
    Chess8x8,
    FiveCheckRules,
    { <FiveCheckRules as WideVariant<Chess8x8>>::ROLE_SPAN },
>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{GameStatus, WideVariantId};
    use crate::Color;

    /// Plays a sequence of UCI moves from a starting five-check position.
    fn play_line(mut pos: FiveCheck, ucis: &[&str]) -> FiveCheck {
        for uci in ucis {
            let mv = pos
                .legal_moves()
                .into_iter()
                .find(|m| m.to_uci::<Chess8x8>() == *uci)
                .unwrap_or_else(|| panic!("legal uci move {uci}"));
            pos = pos.play(&mv);
        }
        pos
    }

    /// The canonical start FEN round-trips with the `5+5` field and opens with the
    /// twenty standard-chess moves.
    #[test]
    fn startpos_round_trips_with_check_field() {
        let pos = FiveCheck::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 5+5 0 1"
        );
        assert_eq!(pos.turn(), Color::White);
        assert_eq!(pos.legal_move_count(), 20);
        assert!(pos.outcome().is_none());
    }

    /// A quiet checking move increments the tally and decrements that side's FEN
    /// remaining count, without changing the move set.
    #[test]
    fn quiet_check_decrements_remaining() {
        let pos = FiveCheck::from_fen("4k3/8/8/8/8/8/8/Q3K3 w - - 5+5 0 1").expect("valid FEN");
        let after = play_line(pos, &["a1a8"]); // Qa8+ checks the black king.
        assert!(after.is_check());
        // Black may now be checked four more times: `5+4`.
        assert_eq!(after.to_fen(), "Q3k3/8/8/8/8/8/8/4K3 b - - 5+4 1 1");
    }

    /// The fifth check delivered by White wins immediately — before any checkmate
    /// consideration — chasing the bare black king down the board.
    #[test]
    fn fifth_check_wins_for_white() {
        // Four checks already delivered against Black (`5+1`); the fifth wins.
        let pos = FiveCheck::from_fen("4k3/8/8/8/8/8/8/3QK3 w - - 5+1 0 1").expect("valid FEN");
        let p1 = play_line(pos, &["d1d8"]); // Qd8+ — the fifth check.
        assert!(p1.is_check());
        assert!(matches!(
            p1.status(),
            GameStatus::VariantWin {
                winner: Color::White,
                ..
            }
        ));
    }

    /// Black wins by delivering its fifth check.
    #[test]
    fn fifth_check_wins_for_black() {
        let pos = FiveCheck::from_fen("3qk3/8/8/8/8/8/8/4K3 b - - 1+5 0 1").expect("valid FEN");
        let after = play_line(pos, &["d8d1"]); // Qd1+ — Black's fifth check.
        assert!(after.is_check());
        assert!(matches!(
            after.status(),
            GameStatus::VariantWin {
                winner: Color::Black,
                ..
            }
        ));
    }

    /// Ordinary checkmate still wins even when no side reached five checks.
    #[test]
    fn ordinary_checkmate_still_wins() {
        // Fool's mate: Black mates on move 2 with only one check ever delivered.
        let pos = FiveCheck::from_fen(
            "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 5+5 1 3",
        )
        .expect("valid FEN");
        assert!(matches!(
            pos.status(),
            GameStatus::Checkmate {
                winner: Color::Black
            }
        ));
    }

    /// The remaining-checks field round-trips through FEN, including asymmetric and
    /// exhausted (`0`) components, and a missing field defaults to `5+5`.
    #[test]
    fn check_field_round_trips_and_defaults() {
        for fen in [
            "4k3/8/8/8/8/8/8/3QK3 b - - 5+3 1 1",
            "4k3/8/8/8/8/8/8/3QK3 w - - 2+0 4 3",
        ] {
            let pos = FiveCheck::from_fen(fen).expect("valid FEN");
            assert_eq!(pos.to_fen(), fen, "round trip for {fen}");
        }
        // A plain six-field FEN defaults to the full `5+5` tally.
        let pos = FiveCheck::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
            .expect("valid FEN");
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 5+5 0 1"
        );
    }

    /// The `AnyWideVariant` id resolves from the FSF name and its aliases.
    #[test]
    fn id_resolves_from_name_and_aliases() {
        for name in ["5check", "fivecheck", "five-check"] {
            assert_eq!(
                name.parse::<WideVariantId>().unwrap(),
                WideVariantId::Fivecheck,
                "name {name}"
            );
        }
    }

    /// A malformed check field is rejected.
    #[test]
    fn malformed_check_field_rejected() {
        for bad in [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 5-5 0 1",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 6+5 0 1",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - x+5 0 1",
        ] {
            assert!(FiveCheck::from_fen(bad).is_err(), "should reject {bad}");
        }
    }
}
