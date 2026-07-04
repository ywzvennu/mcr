//! Chess960 (Fischer random) perft regression tests.
//!
//! The reference node counts are public facts transcribed from the
//! Chess Programming Wiki "Chess960 Perft Results" table, as collected in
//! Ethereal's `fischer.epd` and mirrored by shakmaty's `tests/chess960.perft`:
//!
//! - <https://www.chessprogramming.org/Chess960_Perft_Results>
//! - <https://github.com/AndyGrant/Ethereal/blob/master/src/perft/fischer.epd>
//!
//! The cheap depths run in CI; the deep depths are `#[ignore]`d and meant to be
//! run with `cargo test --release -- --ignored`.

use mcr::{perft_variant, Chess960};

/// One reference position: its EPD/FEN and the `(depth, node-count)` pairs.
struct PerftCase {
    /// A short label (the Chess960 position id from the reference table).
    id: u16,
    /// The starting position in Shredder/X-FEN form.
    fen: &'static str,
    /// The published `(depth, nodes)` reference pairs.
    nodes: &'static [(u32, u64)],
}

/// The transcribed reference table for the first three positions of the CPW /
/// Ethereal `fischer.epd` set, plus two later positions whose castling fields
/// (`ge`, `hd`) exercise non-a/h rook files, plus the standard arrangement.
///
/// Numbers are copied verbatim from the reference table; none are invented.
const CASES: &[PerftCase] = &[
    PerftCase {
        id: 0,
        fen: "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w HFhf - 2 9",
        nodes: &[
            (1, 21),
            (2, 528),
            (3, 12189),
            (4, 326672),
            (5, 8146062),
            (6, 227689589),
        ],
    },
    PerftCase {
        id: 1,
        fen: "2nnrbkr/p1qppppp/8/1ppb4/6PP/3PP3/PPP2P2/BQNNRBKR w HEhe - 1 9",
        nodes: &[
            (1, 21),
            (2, 807),
            (3, 18002),
            (4, 667366),
            (5, 16253601),
            (6, 590751109),
        ],
    },
    PerftCase {
        id: 2,
        fen: "b1q1rrkb/pppppppp/3nn3/8/P7/1PPP4/4PPPP/BQNNRKRB w GE - 1 9",
        nodes: &[
            (1, 20),
            (2, 479),
            (3, 10471),
            (4, 273318),
            (5, 6417013),
            (6, 177654692),
        ],
    },
    PerftCase {
        id: 6,
        fen: "q1bnrkr1/ppppp2p/2n2p2/4b1p1/2NP4/8/PPP1PPPP/QNB1RRKB w ge - 1 9",
        nodes: &[
            (1, 30),
            (2, 860),
            (3, 24566),
            (4, 732757),
            (5, 21093346),
            (6, 649209803),
        ],
    },
    PerftCase {
        id: 9,
        fen: "qn1rbbkr/ppp2p1p/1n1pp1p1/8/3P4/P6P/1PP1PPPK/QNNRBB1R w hd - 2 9",
        nodes: &[
            (1, 28),
            (2, 811),
            (3, 23175),
            (4, 679699),
            (5, 19836606),
            (6, 594527992),
        ],
    },
];

/// The deepest CI depth: depths up to and including this run on every test
/// invocation; deeper ones are gated behind `#[ignore]`.
const CI_MAX_DEPTH: u32 = 4;

#[test]
fn perft_cheap_depths() {
    for case in CASES {
        let pos: Chess960 = case
            .fen
            .parse()
            .unwrap_or_else(|e| panic!("id {}: failed to parse {:?}: {e:?}", case.id, case.fen));
        for &(depth, expected) in case.nodes {
            if depth > CI_MAX_DEPTH {
                continue;
            }
            let got = perft_variant(&pos, depth);
            assert_eq!(
                got, expected,
                "id {} perft({depth}) for {}",
                case.id, case.fen
            );
        }
    }
}

#[test]
fn fen_round_trips() {
    // The Shredder / X-FEN castling fields must survive a parse/write round trip.
    for case in CASES {
        let pos: Chess960 = case.fen.parse().unwrap();
        assert_eq!(pos.to_fen(), case.fen, "round trip for id {}", case.id);
    }
}

#[test]
fn standard_arrangement_reproduces_standard_startpos_perft() {
    // The default Chess960 start (position id 518) is the standard arrangement and
    // must reproduce the canonical standard-chess startpos perft counts.
    let pos = Chess960::startpos();
    assert_eq!(
        pos.to_fen(),
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
    );
    assert_eq!(perft_variant(&pos, 1), 20);
    assert_eq!(perft_variant(&pos, 2), 400);
    assert_eq!(perft_variant(&pos, 3), 8902);
    assert_eq!(perft_variant(&pos, 4), 197281);
    assert_eq!(perft_variant(&pos, 5), 4865609);
}

#[test]
#[ignore = "deep perft; run with --release -- --ignored"]
fn perft_deep_depths() {
    for case in CASES {
        let pos: Chess960 = case.fen.parse().unwrap();
        for &(depth, expected) in case.nodes {
            if depth <= CI_MAX_DEPTH {
                continue;
            }
            let got = perft_variant(&pos, depth);
            assert_eq!(
                got, expected,
                "id {} perft({depth}) for {}",
                case.id, case.fen
            );
        }
    }
}
