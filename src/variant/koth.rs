//! King of the Hill as a [`Variant`]: standard chess plus an immediate win for
//! the side whose king reaches one of the four central squares.
//!
//! Every standard rule — move generation, king safety, castling, promotion, and
//! the ordinary checkmate / stalemate / draw terminations — is inherited
//! unchanged. The sole addition is a terminal rule: the moment a king stands on
//! d4, e4, d5, or e5 ("the hill"), the game ends decisively in favour of that
//! king's owner. Because the hill rule only inspects the position, it never
//! affects which moves are legal, so move generation (and therefore perft) is
//! identical to standard chess from the same placement.

use super::{Variant, VariantId, VariantPosition};
use crate::{EndReason, Position, Square};

/// The four central squares that form the hill. A king on any of them wins the
/// game immediately for its owner.
const HILL: [Square; 4] = [Square::D4, Square::E4, Square::D5, Square::E5];

/// Whether `square` is one of the four central hill squares.
#[inline]
fn is_hill(square: Square) -> bool {
    HILL.contains(&square)
}

/// The king-of-the-hill rule layer: standard chess plus the central-square win
/// condition. A zero-sized marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KingOfTheHillRules;

impl Variant for KingOfTheHillRules {
    type State = ();
    const ID: VariantId = VariantId::KingOfTheHill;

    /// H1: a king on a central square wins immediately for its owner.
    ///
    /// `extra_terminal` runs on the *resulting* position, whose side to move is
    /// the opponent of whoever just moved. A player wins by stepping their own
    /// king onto the hill, after which it is the opponent's turn, so in every
    /// position reachable by play the king on the hill belongs to the side *not*
    /// to move. [`EndReason::KingInTheHill`] is the variant reason whose outcome,
    /// `KingInTheHill.outcome(turn) = Decisive { winner: turn.opposite() }`,
    /// awards the win to exactly that side. We therefore check the
    /// side-not-to-move's king first; a crafted FEN in which only the
    /// side-to-move's king sits on the hill (unreachable by legal play, since
    /// that game would already be over) is also reported terminal so such
    /// positions are never treated as ongoing.
    fn extra_terminal(core: &Position, _state: &Self::State) -> Option<EndReason> {
        let mover = core.turn().opposite();
        if core.board().king_of(mover).is_some_and(is_hill) {
            return Some(EndReason::KingInTheHill);
        }
        if core.board().king_of(core.turn()).is_some_and(is_hill) {
            // Unreachable by legal play; report terminal so a crafted position
            // with the side-to-move's king on the hill is not treated as live.
            // The decisive side is the king's owner; `KingInTheHill` here names
            // the opponent, so this branch only guarantees terminality.
            return Some(EndReason::KingInTheHill);
        }
        None
    }
}

/// King of the Hill as a [`VariantPosition`].
///
/// Movegen and king safety are exactly standard chess; the only difference is
/// the central-square win reported through [`VariantPosition::outcome`]. Perft
/// from any placement matches standard chess.
pub type KingOfTheHill = VariantPosition<KingOfTheHillRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variant::perft_variant;
    use crate::{Color, EndReason, Outcome};

    /// Plays a sequence of UCI moves from a starting position.
    fn play_line(mut pos: KingOfTheHill, ucis: &[&str]) -> KingOfTheHill {
        for uci in ucis {
            let mv = pos.parse_uci(uci).expect("legal uci move");
            pos = pos.play(&mv);
        }
        pos
    }

    #[test]
    fn startpos_is_ongoing() {
        let pos = KingOfTheHill::startpos();
        assert_eq!(pos.variant_id(), VariantId::KingOfTheHill);
        assert_eq!(pos.legal_moves().len(), 20);
        assert!(pos.outcome().is_none());
        // The hill rule contributes no extra state, so the FEN is standard.
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
    }

    #[test]
    fn white_king_reaching_e4_wins() {
        // White king one step from the hill; Ke4 ends the game for White. A spare
        // rook keeps the material sufficient so a non-terminal position reports
        // `None` rather than an insufficient-material draw.
        let pos: KingOfTheHill = "4k3/8/8/8/8/8/8/R3K3 w - - 0 1".parse().unwrap();
        assert!(pos.outcome().is_none());
        let after = play_line(pos, &["e1e2"]); // walk up
        let after = play_line(after, &["e8e7"]);
        let after = play_line(after, &["e2e3"]);
        let after = play_line(after, &["e7e6"]);
        let after = play_line(after, &["e3e4"]); // White king onto the hill
        assert_eq!(
            after.outcome(),
            Some(Outcome::Decisive {
                winner: Color::White
            })
        );
        assert_eq!(after.end_reason(), Some(EndReason::KingInTheHill));
    }

    #[test]
    fn black_king_reaching_d5_wins() {
        let pos: KingOfTheHill = "r3k3/8/8/8/8/8/8/4K3 b - - 0 1".parse().unwrap();
        let after = play_line(pos, &["e8d7"]);
        let after = play_line(after, &["e1e2"]);
        let after = play_line(after, &["d7d6"]);
        let after = play_line(after, &["e2e3"]);
        let after = play_line(after, &["d6d5"]); // Black king onto the hill
        assert_eq!(
            after.outcome(),
            Some(Outcome::Decisive {
                winner: Color::Black
            })
        );
        assert_eq!(after.end_reason(), Some(EndReason::KingInTheHill));
    }

    #[test]
    fn each_hill_square_wins() {
        // For every central square, a king parked there (with the opponent to
        // move) is a decisive win for the king's owner.
        for (fen, winner) in [
            ("4k3/8/8/8/3K4/8/8/8 b - - 0 1", Color::White), // White Kd4 (rank4)
            ("4k3/8/8/8/4K3/8/8/8 b - - 0 1", Color::White), // White Ke4 (rank4)
            ("4k3/8/8/3K4/8/8/8/8 b - - 0 1", Color::White), // White Kd5 (rank5)
            ("4k3/8/8/4K3/8/8/8/8 b - - 0 1", Color::White), // White Ke5 (rank5)
            ("8/8/8/8/3k4/8/8/4K3 w - - 0 1", Color::Black), // Black Kd4 (rank4)
            ("8/8/8/8/4k3/8/8/4K3 w - - 0 1", Color::Black), // Black Ke4 (rank4)
            ("8/8/8/3k4/8/8/8/4K3 w - - 0 1", Color::Black), // Black Kd5 (rank5)
            ("8/8/8/4k3/8/8/8/4K3 w - - 0 1", Color::Black), // Black Ke5 (rank5)
        ] {
            let pos: KingOfTheHill = fen.parse().unwrap();
            assert_eq!(
                pos.outcome(),
                Some(Outcome::Decisive { winner }),
                "king on hill should win for {winner:?} in {fen}"
            );
            assert_eq!(
                pos.end_reason(),
                Some(EndReason::KingInTheHill),
                "king on hill should report KingInTheHill in {fen}"
            );
        }
    }

    #[test]
    fn king_already_on_hill_is_terminal_but_moves_still_generate() {
        // A position with a king already on the hill is terminal: outcome() is
        // decisive, yet legal_moves() still enumerates the underlying chess moves
        // (the hill rule only affects outcome, never move generation).
        let pos: KingOfTheHill = "4k3/8/8/8/4K3/8/8/8 b - - 0 1".parse().unwrap();
        assert_eq!(
            pos.outcome(),
            Some(Outcome::Decisive {
                winner: Color::White
            })
        );
        assert!(
            !pos.legal_moves().is_empty(),
            "moves still generate on a terminal hill position"
        );
    }

    #[test]
    fn non_hill_king_position_is_ongoing() {
        // Kings near but not on the hill: the game continues. A rook keeps the
        // material sufficient so the result is `None`, not a material draw.
        let pos: KingOfTheHill = "4k3/8/8/8/8/4K3/8/R7 b - - 0 1".parse().unwrap();
        assert!(pos.outcome().is_none());
    }

    #[test]
    fn ordinary_checkmate_still_wins() {
        // King of the Hill inherits ordinary checkmate: fool's mate ends
        // decisively for Black with no king anywhere near the hill.
        let pos: KingOfTheHill = "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3"
            .parse()
            .unwrap();
        assert_eq!(
            pos.outcome(),
            Some(Outcome::Decisive {
                winner: Color::Black
            })
        );
        assert_eq!(pos.end_reason(), Some(EndReason::Checkmate));
    }

    #[test]
    fn movegen_matches_standard_from_same_placement() {
        // The hill rule never changes the legal-move set, so perft from a
        // non-terminal placement matches standard chess. (Startpos perft to a
        // larger depth lives in the dedicated perft test file.)
        let pos = KingOfTheHill::startpos();
        assert_eq!(perft_variant(&pos, 1), 20);
        assert_eq!(perft_variant(&pos, 2), 400);
        assert_eq!(perft_variant(&pos, 3), 8902);
    }
}
