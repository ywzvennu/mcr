//! Horde perft (move-generation correctness) regression tests.
//!
//! The reference node counts are public facts transcribed verbatim from
//! shakmaty's `tests/horde.perft` data table:
//!
//! - <https://github.com/niklasf/shakmaty/blob/main/shakmaty/tests/horde.perft>
//!
//! Three positions are covered: the horde starting position, an open-flank
//! middlegame, and a position that exercises white's first-rank pawn
//! double-pushes and the en-passant captures they enable. Matching these counts
//! exercises the kingless white legality path, the first-rank double-push
//! generation, and en-passant against a second-rank target.
//!
//! The cheap depths run in CI; the deepest published depth is `#[ignore]`d and
//! meant to be run with `cargo test --release -- --ignored`. No numbers are
//! invented.

use mcr::{perft_variant, Color, Horde, Role, VariantId};

/// One reference position: its EPD/FEN and the `(depth, node-count)` pairs.
struct PerftCase {
    /// The shakmaty `horde.perft` `id` label.
    id: &'static str,
    /// The starting position (the EPD line plus halfmove/fullmove fields).
    fen: &'static str,
    /// The published `(depth, nodes)` reference pairs.
    nodes: &'static [(u32, u64)],
}

/// The transcribed reference table, copied verbatim from shakmaty's
/// `tests/horde.perft`; none of the numbers are invented.
const CASES: &[PerftCase] = &[
    PerftCase {
        id: "horde-start",
        fen: "rnbqkbnr/pppppppp/8/1PP2PP1/PPPPPPPP/PPPPPPPP/PPPPPPPP/PPPPPPPP w kq - 0 1",
        nodes: &[(1, 8), (2, 128), (3, 1274), (4, 23310)],
    },
    PerftCase {
        id: "horde-open-flank",
        fen: "4k3/pp4q1/3P2p1/8/P3PP2/PPP2r2/PPP5/PPPP4 b - - 0 1",
        nodes: &[(1, 30), (2, 241), (3, 6633), (4, 56539)],
    },
    PerftCase {
        id: "horde-en-passant",
        fen: "k7/5p2/4p2P/3p2P1/2p2P2/1p2P2P/p2P2P1/2P2P2 w - - 0 1",
        nodes: &[(1, 13), (2, 172), (3, 2205), (4, 33781)],
    },
];

/// The deepest CI depth: depths up to and including this run on every invocation;
/// deeper ones are gated behind `#[ignore]`.
const CI_MAX_DEPTH: u32 = 3;

#[test]
fn startpos_is_the_published_horde_start() {
    let pos = Horde::startpos();
    assert_eq!(pos.variant_id(), VariantId::Horde);
    assert_eq!(
        pos.to_fen(),
        "rnbqkbnr/pppppppp/8/1PP2PP1/PPPPPPPP/PPPPPPPP/PPPPPPPP/PPPPPPPP w kq - 0 1"
    );
    assert_eq!(pos.turn(), Color::White);
    let board = pos.core().board();
    assert_eq!(board.pieces(Color::White, Role::King).count(), 0);
    assert_eq!(board.pieces(Color::Black, Role::King).count(), 1);
    assert_eq!(board.pieces(Color::White, Role::Pawn).count(), 36);
}

#[test]
fn perft_cheap_depths() {
    for case in CASES {
        let pos: Horde = case
            .fen
            .parse()
            .unwrap_or_else(|e| panic!("id {}: failed to parse {:?}: {e:?}", case.id, case.fen));
        for &(depth, expected) in case.nodes {
            if depth > CI_MAX_DEPTH {
                continue;
            }
            let got = perft_variant(&pos, depth);
            assert_eq!(got, expected, "id {} perft({depth})", case.id);
        }
    }
}

#[test]
#[ignore = "deep perft; run with --release -- --ignored"]
fn perft_deep_depths() {
    for case in CASES {
        let pos: Horde = case.fen.parse().unwrap();
        for &(depth, expected) in case.nodes {
            if depth <= CI_MAX_DEPTH {
                continue;
            }
            let got = perft_variant(&pos, depth);
            assert_eq!(got, expected, "id {} perft({depth})", case.id);
        }
    }
}
