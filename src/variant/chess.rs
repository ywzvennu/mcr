//! Standard chess as a [`Variant`]: [`ChessRules`] overrides nothing, so it runs
//! entirely on the provided standard-chess defaults.

use super::{Variant, VariantId, VariantPosition};

/// The standard-chess rule layer. A zero-sized marker that takes every
/// [`Variant`] default, so [`Chess`] reproduces the plain [`crate::Position`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChessRules;

impl Variant for ChessRules {
    type State = ();
    const ID: VariantId = VariantId::Standard;
    // Every hook is the provided standard-chess default; nothing is overridden.
}

/// Standard chess as a [`VariantPosition`].
///
/// `Chess` runs on the fast-legality sentinel path, so it reproduces every
/// [`crate::Position`] behaviour — perft, SAN, UCI, Zobrist — bit for bit, while
/// going through the generic variant machinery.
pub type Chess = VariantPosition<ChessRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variant::{perft_variant, VariantPosition};
    use crate::{Position, Square};

    /// A standard-chess rule layer that opts *out* of the fast-legality sentinel,
    /// forcing the generic pseudo-legal + make-move filter path. Used only to
    /// regression-test that the slow path (which every non-standard variant will
    /// use) reproduces the standard legal-move set and perft counts.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    struct SlowChess;

    impl Variant for SlowChess {
        type State = ();
        const ID: VariantId = VariantId::Standard;
        const USES_FAST_LEGALITY: bool = false;
    }

    type SlowPosition = VariantPosition<SlowChess>;

    #[test]
    fn slow_path_matches_standard_perft() {
        // The slow pseudo-legal + make-move filter must reproduce the standard
        // perft counts on tricky positions (pins, ep, castling, promotions).
        for (fen, depth, expected) in [
            (
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
                4,
                197281u64,
            ),
            (
                "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
                3,
                97862,
            ),
            ("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1", 4, 43238),
            (
                "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
                3,
                9467,
            ),
        ] {
            let pos = SlowPosition::from_fen(fen).unwrap();
            assert_eq!(
                perft_variant(&pos, depth),
                expected,
                "slow-path perft({depth}) for {fen}"
            );
        }
    }

    #[test]
    fn startpos_matches_core() {
        let chess = Chess::startpos();
        assert_eq!(chess.core(), &Position::startpos());
        assert_eq!(chess.turn(), crate::Color::White);
        assert_eq!(chess.variant_id(), VariantId::Standard);
        assert_eq!(chess.legal_moves().len(), 20);
        assert_eq!(
            chess.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
        // The variant zobrist equals the core zobrist (unit state contributes
        // nothing), so the pinned standard startpos key is unchanged.
        assert_eq!(chess.zobrist(), Position::startpos().zobrist());
        assert_eq!(chess.zobrist().get(), 0x8FF6_F282_E19D_060D);
    }

    #[test]
    fn fen_round_trip_and_default() {
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
        let chess: Chess = fen.parse().unwrap();
        assert_eq!(chess.to_fen(), fen);
        assert_eq!(Chess::default(), Chess::startpos());
    }

    #[test]
    fn play_and_uci_match_core() {
        let chess = Chess::startpos();
        let e4 = chess.parse_uci("e2e4").unwrap();
        let after = chess.play(&e4);
        assert_eq!(after.turn(), crate::Color::Black);
        assert_eq!(after.core().ep_square(), Some(Square::E3));
        assert_eq!(
            after.to_fen(),
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1"
        );
        // The variant play matches the core play step for step.
        let core_after =
            Position::startpos().play(&Position::startpos().parse_uci("e2e4").unwrap());
        assert_eq!(after.core(), &core_after);
        assert_eq!(after.zobrist(), core_after.zobrist());
    }

    #[test]
    fn perft_matches_core() {
        let chess = Chess::startpos();
        assert_eq!(perft_variant(&chess, 1), 20);
        assert_eq!(perft_variant(&chess, 2), 400);
        assert_eq!(perft_variant(&chess, 3), 8902);
        assert_eq!(perft_variant(&chess, 4), 197281);
    }

    #[test]
    fn outcome_matches_core() {
        // Fool's mate.
        let chess: Chess = "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3"
            .parse()
            .unwrap();
        assert_eq!(chess.outcome(), chess.core().outcome());
        assert_eq!(chess.end_reason(), Some(crate::EndReason::Checkmate));
        assert!(chess.is_check());
    }

    #[test]
    fn pseudo_then_filter_equals_fast_path() {
        // Force the slow pseudo-legal + make-move filter and confirm it yields
        // exactly the fast core legal set for a tricky position (pins, ep,
        // castling). ChessRules uses the fast path, so drive the slow path
        // directly through the core to validate the seam.
        let fens = [
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
            "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1",
        ];
        for fen in fens {
            let core = Position::from_fen(fen).unwrap();
            let mut pseudo_list = crate::movelist::MoveList::new();
            core.pseudo_into(&mut pseudo_list);
            pseudo_list.retain(|mv| core.move_keeps_king_safe(mv));
            let mut pseudo = pseudo_list.into_vec();
            let mut fast = core.legal_moves();
            pseudo.sort_by_key(|m| (m.from().index(), m.to().index(), format!("{:?}", m.kind())));
            fast.sort_by_key(|m| (m.from().index(), m.to().index(), format!("{:?}", m.kind())));
            assert_eq!(pseudo, fast, "pseudo+filter != fast for {fen}");
        }
    }
}
