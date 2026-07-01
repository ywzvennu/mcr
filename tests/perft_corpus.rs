//! Offline perft reference corpus — the always-available correctness floor.
//!
//! Validation elsewhere leans on the Fairy-Stockfish (FSF) subprocess in
//! `compare-fairy/` for a live differential. That harness stays, but it needs an
//! external GPL binary present. This corpus is the complement: a single,
//! self-contained runner that checks mce's move generation against *pinned*
//! reference node counts, so core correctness is provable in a plain
//! `cargo test` with **no external engine present**.
//!
//! A perft count is the number of leaf nodes in the legal-move game tree at a
//! fixed depth. Matching a known-correct table exercises move generation,
//! make/unmake, and the special rules (castling, en passant, promotion, drops,
//! pins, and check evasion) end to end.
//!
//! # Provenance
//!
//! Every pinned count carries a documented source. Nothing here is derived from
//! mce itself — the references are external facts:
//!
//! - **Standard chess** — the canonical Chess Programming Wiki (CPW) perft
//!   results: the initial position (depths 1–6 = 20 / 400 / 8902 / 197281 /
//!   4865609 / 119060324), Kiwipete, and CPW positions 3–6.
//!   <https://www.chessprogramming.org/Perft_Results>
//! - **Chess960** — the CPW "Chess960 Perft Results" table, as collected in
//!   Ethereal's `fischer.epd`.
//!   <https://www.chessprogramming.org/Chess960_Perft_Results>
//! - **Chess-family variants** (atomic, antichess, crazyhouse, horde, racing
//!   kings, three-check, king-of-the-hill) — the published shakmaty perft
//!   fixtures, which mirror the CPW / lichess community tables.
//!   <https://github.com/niklasf/shakmaty/tree/master/shakmaty/tests>
//! - **Wide / fairy variants** (xiangqi, shogi, makruk, …) — no widely
//!   published perft table exists, so the reference was produced **once** with
//!   the FSF oracle (`UCI_Variant <name>`, `go perft`) and pinned here as a
//!   constant. Each is tagged with the FSF confirmation. The values also match
//!   the per-variant `tests/perft_*.rs` suites, which were validated
//!   node-for-node against FSF when each variant landed. **This test never
//!   invokes FSF** — it only checks mce against the pinned constants.
//!
//! # Layering
//!
//! The cheap depths run on every `cargo test` invocation. The deeper entries are
//! `#[ignore]`d so the default run stays fast; sweep them all with:
//!
//! ```text
//! cargo test --release --test perft_corpus -- --include-ignored
//! ```

use mce::geometry::{AnyWideVariant, WideVariantId};
use mce::{perft, perft_variant, AnyVariant, Chess960, Position, VariantId};

/// One reference position: a label, its FEN, and the pinned `(depth, nodes)`
/// pairs. Pairs at depth `> cheap_max` are gated behind `#[ignore]`.
struct Case {
    /// A short human label (usually the source table's position id).
    label: &'static str,
    /// The position in FEN / EPD form (variant-specific dialect where noted).
    fen: &'static str,
    /// The pinned `(depth, node-count)` reference pairs.
    nodes: &'static [(u32, u64)],
    /// The deepest depth that runs in the cheap (non-`#[ignore]`) layer.
    cheap_max: u32,
}

/// A wide/fairy startpos reference: the variant id, its pinned counts, and the
/// cheap-layer cutoff. The startpos is taken from [`AnyWideVariant::startpos`].
struct WideStartCase {
    /// The wide-variant identifier whose startpos is measured.
    id: WideVariantId,
    /// A short human label for assertion messages.
    label: &'static str,
    /// The pinned `(depth, node-count)` reference pairs.
    nodes: &'static [(u32, u64)],
    /// The deepest depth that runs in the cheap (non-`#[ignore]`) layer.
    cheap_max: u32,
}

/// Runs `f(depth) -> nodes` for each pair in `case`, keeping only the depths for
/// which `keep(depth)` holds, and asserts the pinned value.
fn run<F: Fn(u32) -> u64>(
    label: &str,
    fen: &str,
    nodes: &[(u32, u64)],
    keep: impl Fn(u32) -> bool,
    f: F,
) {
    for &(depth, expected) in nodes {
        if !keep(depth) {
            continue;
        }
        let got = f(depth);
        assert_eq!(
            got, expected,
            "{label} perft({depth}) for {fen:?}: expected {expected}, got {got}"
        );
    }
}

// ===========================================================================
// Standard chess — CPW perft results (published external facts).
// ===========================================================================

const STANDARD: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        // CPW "Initial Position": 20 / 400 / 8902 / 197281 / 4865609 / 119060324.
        nodes: &[
            (1, 20),
            (2, 400),
            (3, 8902),
            (4, 197281),
            (5, 4865609),
            (6, 119060324),
        ],
        cheap_max: 5,
    },
    Case {
        label: "kiwipete",
        fen: "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        // CPW "Position 2" (Kiwipete).
        nodes: &[(1, 48), (2, 2039), (3, 97862), (4, 4085603), (5, 193690690)],
        cheap_max: 3,
    },
    Case {
        label: "cpw-pos3",
        fen: "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        // CPW "Position 3".
        nodes: &[
            (1, 14),
            (2, 191),
            (3, 2812),
            (4, 43238),
            (5, 674624),
            (6, 11030083),
        ],
        cheap_max: 5,
    },
    Case {
        label: "cpw-pos4",
        fen: "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        // CPW "Position 4".
        nodes: &[(1, 6), (2, 264), (3, 9467), (4, 422333), (5, 15833292)],
        cheap_max: 4,
    },
    Case {
        label: "cpw-pos5",
        fen: "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        // CPW "Position 5".
        nodes: &[(1, 44), (2, 1486), (3, 62379), (4, 2103487), (5, 89941194)],
        cheap_max: 3,
    },
    Case {
        label: "cpw-pos6",
        fen: "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
        // CPW "Position 6".
        nodes: &[(1, 46), (2, 2079), (3, 89890), (4, 3894594)],
        cheap_max: 3,
    },
];

#[test]
fn standard_cheap() {
    for c in STANDARD {
        let pos = Position::from_fen(c.fen).expect("valid FEN");
        run(
            c.label,
            c.fen,
            c.nodes,
            |d| d <= c.cheap_max,
            |d| perft(&pos, d),
        );
    }
}

#[test]
#[ignore = "deep perft; run with --release --test perft_corpus -- --ignored"]
fn standard_deep() {
    for c in STANDARD {
        let pos = Position::from_fen(c.fen).expect("valid FEN");
        run(
            c.label,
            c.fen,
            c.nodes,
            |d| d > c.cheap_max,
            |d| perft(&pos, d),
        );
    }
}

// ===========================================================================
// Chess960 — CPW "Chess960 Perft Results" (via Ethereal fischer.epd).
// ===========================================================================

const CHESS960: &[Case] = &[
    Case {
        label: "c960-id0",
        fen: "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w HFhf - 2 9",
        nodes: &[
            (1, 21),
            (2, 528),
            (3, 12189),
            (4, 326672),
            (5, 8146062),
            (6, 227689589),
        ],
        cheap_max: 4,
    },
    Case {
        label: "c960-id1",
        fen: "2nnrbkr/p1qppppp/8/1ppb4/6PP/3PP3/PPP2P2/BQNNRBKR w HEhe - 1 9",
        nodes: &[
            (1, 21),
            (2, 807),
            (3, 18002),
            (4, 667366),
            (5, 16253601),
            (6, 590751109),
        ],
        cheap_max: 4,
    },
    Case {
        label: "c960-id2",
        fen: "b1q1rrkb/pppppppp/3nn3/8/P7/1PPP4/4PPPP/BQNNRKRB w GE - 1 9",
        nodes: &[
            (1, 20),
            (2, 479),
            (3, 10471),
            (4, 273318),
            (5, 6417013),
            (6, 177654692),
        ],
        cheap_max: 4,
    },
];

#[test]
fn chess960_cheap() {
    for c in CHESS960 {
        let pos: Chess960 = c.fen.parse().expect("valid Chess960 FEN");
        run(
            c.label,
            c.fen,
            c.nodes,
            |d| d <= c.cheap_max,
            |d| perft_variant(&pos, d),
        );
    }
}

#[test]
#[ignore = "deep perft; run with --release --test perft_corpus -- --ignored"]
fn chess960_deep() {
    for c in CHESS960 {
        let pos: Chess960 = c.fen.parse().expect("valid Chess960 FEN");
        run(
            c.label,
            c.fen,
            c.nodes,
            |d| d > c.cheap_max,
            |d| perft_variant(&pos, d),
        );
    }
}

// ===========================================================================
// Chess-family variants — published shakmaty perft fixtures (mirror the
// CPW / lichess community tables). Driven through the uniform `AnyVariant`.
// ===========================================================================

/// A chess-family reference: its `VariantId`, a `Case`, transcribed from the
/// matching shakmaty `tests/<variant>.perft` fixture.
struct FamilyCase {
    /// The variant selector for [`AnyVariant::from_fen`].
    id: VariantId,
    /// The position, depths, and cheap cutoff.
    case: Case,
}

const FAMILY: &[FamilyCase] = &[
    // -- Atomic: shakmaty atomic.perft. Blast rules diverge from standard at
    //    depth 4 (197326 vs 197281). "programfox-1" is the shakmaty bench pos.
    FamilyCase {
        id: VariantId::Atomic,
        case: Case {
            label: "atomic-start",
            fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            nodes: &[(1, 20), (2, 400), (3, 8902), (4, 197326)],
            cheap_max: 4,
        },
    },
    FamilyCase {
        id: VariantId::Atomic,
        case: Case {
            label: "atomic-programfox-1",
            fen: "rn2kb1r/1pp1p2p/p2q1pp1/3P4/2P3b1/4PN2/PP3PPP/R2QKB1R b KQkq - 0 1",
            nodes: &[(1, 40), (2, 1238), (3, 45237), (4, 1434825)],
            cheap_max: 3,
        },
    },
    // -- Antichess: shakmaty antichess.perft. No castling; forced captures.
    FamilyCase {
        id: VariantId::Antichess,
        case: Case {
            label: "antichess-start",
            fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1",
            nodes: &[(1, 20), (2, 400), (3, 8067), (4, 153299)],
            cheap_max: 4,
        },
    },
    // -- Racing Kings: shakmaty racingkings.perft.
    FamilyCase {
        id: VariantId::RacingKings,
        case: Case {
            label: "racingkings-start",
            fen: "8/8/8/8/8/8/krbnNBRK/qrbnNBRQ w - - 0 1",
            nodes: &[(1, 21), (2, 421), (3, 11264), (4, 296242)],
            cheap_max: 4,
        },
    },
    // -- Horde: shakmaty horde.perft. Kingless white side, first-rank double
    //    pushes and the en passant they enable.
    FamilyCase {
        id: VariantId::Horde,
        case: Case {
            label: "horde-start",
            fen: "rnbqkbnr/pppppppp/8/1PP2PP1/PPPPPPPP/PPPPPPPP/PPPPPPPP/PPPPPPPP w kq - 0 1",
            nodes: &[(1, 8), (2, 128), (3, 1274), (4, 23310)],
            cheap_max: 3,
        },
    },
    // -- Crazyhouse: shakmaty crazyhouse.perft. Drops from a filled pocket.
    FamilyCase {
        id: VariantId::Crazyhouse,
        case: Case {
            label: "crazyhouse-drops",
            fen: "2k5/8/8/8/8/8/8/4K3[Qn] w - -",
            nodes: &[(1, 67), (2, 3083), (3, 88634)],
            cheap_max: 2,
        },
    },
    // -- King of the Hill: the hill rule never changes the legal-move set on an
    //    ongoing position, so the startpos matches standard chess node-for-node
    //    (CPW initial position). shakmaty covers this equivalence.
    FamilyCase {
        id: VariantId::KingOfTheHill,
        case: Case {
            label: "koth-start",
            fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            nodes: &[(1, 20), (2, 400), (3, 8902), (4, 197281)],
            cheap_max: 4,
        },
    },
    // -- Three-check: the check counter never changes the legal-move set, so the
    //    startpos matches standard chess node-for-node. The trailing `3+3` field
    //    selects the three-check parse.
    FamilyCase {
        id: VariantId::ThreeCheck,
        case: Case {
            label: "threecheck-start",
            fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 3+3",
            nodes: &[(1, 20), (2, 400), (3, 8902), (4, 197281)],
            cheap_max: 4,
        },
    },
];

#[test]
fn chess_family_cheap() {
    for fc in FAMILY {
        let pos = AnyVariant::from_fen(fc.id, fc.case.fen).expect("valid variant FEN");
        run(
            fc.case.label,
            fc.case.fen,
            fc.case.nodes,
            |d| d <= fc.case.cheap_max,
            |d| pos.perft(d),
        );
    }
}

#[test]
#[ignore = "deep perft; run with --release --test perft_corpus -- --ignored"]
fn chess_family_deep() {
    for fc in FAMILY {
        let pos = AnyVariant::from_fen(fc.id, fc.case.fen).expect("valid variant FEN");
        run(
            fc.case.label,
            fc.case.fen,
            fc.case.nodes,
            |d| d > fc.case.cheap_max,
            |d| pos.perft(d),
        );
    }
}

// ===========================================================================
// Wide / fairy variants — FSF-confirmed startpos perft, pinned as constants.
// Each was produced with `UCI_Variant <name>` / `go perft` on the FSF oracle;
// the depth-3 value of every one below was re-confirmed 2026-07-01. The same
// numbers appear in the per-variant `tests/perft_*.rs` suites.
// ===========================================================================

const WIDE: &[WideStartCase] = &[
    // FSF xiangqi startpos perft: 44 / 1920 / 79666 / 3290240 / 133312995, confirmed 2026-07-01.
    WideStartCase {
        id: WideVariantId::Xiangqi,
        label: "xiangqi",
        nodes: &[(1, 44), (2, 1920), (3, 79666), (4, 3290240), (5, 133312995)],
        cheap_max: 3,
    },
    // FSF shogi startpos perft: 30 / 900 / 25470 / 719731 / 19861490, confirmed 2026-07-01.
    WideStartCase {
        id: WideVariantId::Shogi,
        label: "shogi",
        nodes: &[(1, 30), (2, 900), (3, 25470), (4, 719731), (5, 19861490)],
        cheap_max: 3,
    },
    // FSF minishogi startpos perft: 14 / 181 / 2512 / 35401 / 533203, confirmed 2026-07-01.
    WideStartCase {
        id: WideVariantId::Minishogi,
        label: "minishogi",
        nodes: &[(1, 14), (2, 181), (3, 2512), (4, 35401), (5, 533203)],
        cheap_max: 4,
    },
    // FSF makruk startpos perft: 23 / 529 / 12012 / 273026 / 6223994 / 142078049, confirmed 2026-07-01.
    WideStartCase {
        id: WideVariantId::Makruk,
        label: "makruk",
        nodes: &[
            (1, 23),
            (2, 529),
            (3, 12012),
            (4, 273026),
            (5, 6223994),
            (6, 142078049),
        ],
        cheap_max: 4,
    },
    // FSF capablanca startpos perft: 28 / 784 / 25228 / 805128 / 28741319 / 1015802437, confirmed 2026-07-01.
    WideStartCase {
        id: WideVariantId::Capablanca,
        label: "capablanca",
        nodes: &[
            (1, 28),
            (2, 784),
            (3, 25228),
            (4, 805128),
            (5, 28741319),
            (6, 1015802437),
        ],
        cheap_max: 3,
    },
    // FSF janggi startpos perft: 32 / 1024 / 33000 / 1065277 / 35243995, confirmed 2026-07-01.
    WideStartCase {
        id: WideVariantId::Janggi,
        label: "janggi",
        nodes: &[(1, 32), (2, 1024), (3, 33000), (4, 1065277), (5, 35243995)],
        cheap_max: 3,
    },
    // FSF shatranj startpos perft: 16 / 256 / 4176 / 68122 / 1164248 / 19864709 / 357218656, confirmed 2026-07-01.
    WideStartCase {
        id: WideVariantId::Shatranj,
        label: "shatranj",
        nodes: &[
            (1, 16),
            (2, 256),
            (3, 4176),
            (4, 68122),
            (5, 1164248),
            (6, 19864709),
            (7, 357218656),
        ],
        cheap_max: 4,
    },
    // FSF grand startpos perft: 65 / 4225 / 259514 / 15921643, confirmed 2026-07-01.
    WideStartCase {
        id: WideVariantId::Grand,
        label: "grand",
        nodes: &[(1, 65), (2, 4225), (3, 259514), (4, 15921643)],
        cheap_max: 3,
    },
    // FSF shako startpos perft: 58 / 3364 / 185938 / 10273158 / 559582321, confirmed 2026-07-01.
    WideStartCase {
        id: WideVariantId::Shako,
        label: "shako",
        nodes: &[
            (1, 58),
            (2, 3364),
            (3, 185938),
            (4, 10273158),
            (5, 559582321),
        ],
        cheap_max: 3,
    },
    // FSF minixiangqi startpos perft: 19 / 331 / 6664 / 127164 / 2666905, confirmed 2026-07-01.
    WideStartCase {
        id: WideVariantId::Minixiangqi,
        label: "minixiangqi",
        nodes: &[(1, 19), (2, 331), (3, 6664), (4, 127164), (5, 2666905)],
        cheap_max: 4,
    },
];

#[test]
fn wide_variants_cheap() {
    for w in WIDE {
        let pos = AnyWideVariant::startpos(w.id);
        run(
            w.label,
            "startpos",
            w.nodes,
            |d| d <= w.cheap_max,
            |d| pos.perft(d),
        );
    }
}

#[test]
#[ignore = "deep perft; run with --release --test perft_corpus -- --ignored"]
fn wide_variants_deep() {
    for w in WIDE {
        let pos = AnyWideVariant::startpos(w.id);
        run(
            w.label,
            "startpos",
            w.nodes,
            |d| d > w.cheap_max,
            |d| pos.perft(d),
        );
    }
}
