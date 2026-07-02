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
//! - **Generic-geometry Capablanca-family & long-tail variants** (almost, amazon,
//!   chigorin, gothic, embassy, janus, caparandom, chancellor, courier, tencubed,
//!   opulent) — like the wide/fairy set, no published perft table exists, so each
//!   startpos count was produced with the FSF oracle (`UCI_Variant <name>`, `go
//!   perft`) and pinned here. Every depth below was **re-confirmed against FSF on
//!   2026-07-02** and also matches the per-variant `tests/perft_<name>.rs` suites.
//! - **Chu Shogi** (12x12) — the reference oracle is **HaChu** (no FSF perft). The
//!   depth-1/2 counts are byte-identical/node-for-node HaChu matches; depth-3 pins
//!   mce's *correct* 48319 (HaChu 0.23 yields 48317 via a documented
//!   anti-diagonal-Lion bug that misses two legal captures — mce is right); depth-4
//!   is an mce regression pin. See `tests/perft_chu.rs` for the full write-up.
//!
//! # Layering
//!
//! The cheap depths run on every `cargo test` invocation. The deeper entries are
//! `#[ignore]`d so the default run stays fast; sweep them all with:
//!
//! ```text
//! cargo test --release --test perft_corpus -- --include-ignored
//! ```

use mce::geometry::{
    perft as gperft, Almost, Amazon, AnyWideVariant, Cap10x8, Caparandom, Chancellor, Chess8x8,
    Chess9x9, Chigorin, Chu, Chu12x12, Courier, Courier12x8, Embassy, Gothic, Grand10x10, Janus,
    Opulent, Tencubed, WideVariantId,
};
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
    // FSF shogi startpos perft: 30 / 900 / 25470 / 719731 / 19861490 / 547581517,
    // depths 1-5 confirmed 2026-07-01; depth 6 confirmed 2026-07-02.
    WideStartCase {
        id: WideVariantId::Shogi,
        label: "shogi",
        nodes: &[
            (1, 30),
            (2, 900),
            (3, 25470),
            (4, 719731),
            (5, 19861490),
            (6, 547581517),
        ],
        cheap_max: 3,
    },
    // FSF minishogi startpos perft: 14 / 181 / 2512 / 35401 / 533203 / 8276188,
    // depths 1-5 confirmed 2026-07-01; depth 6 confirmed 2026-07-02.
    WideStartCase {
        id: WideVariantId::Minishogi,
        label: "minishogi",
        nodes: &[
            (1, 14),
            (2, 181),
            (3, 2512),
            (4, 35401),
            (5, 533203),
            (6, 8276188),
        ],
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
    // FSF grand startpos perft: 65 / 4225 / 259514 / 15921643 / 959883584,
    // depths 1-4 confirmed 2026-07-01; depth 5 confirmed 2026-07-02.
    WideStartCase {
        id: WideVariantId::Grand,
        label: "grand",
        nodes: &[
            (1, 65),
            (2, 4225),
            (3, 259514),
            (4, 15921643),
            (5, 959883584),
        ],
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
    // FSF minixiangqi startpos perft: 19 / 331 / 6664 / 127164 / 2666905 / 54612676,
    // depths 1-5 confirmed 2026-07-01; depth 6 confirmed 2026-07-02.
    WideStartCase {
        id: WideVariantId::Minixiangqi,
        label: "minixiangqi",
        nodes: &[
            (1, 19),
            (2, 331),
            (3, 6664),
            (4, 127164),
            (5, 2666905),
            (6, 54612676),
        ],
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

// ===========================================================================
// Generic-geometry Capablanca-family & long-tail variants — FSF-confirmed
// startpos perft, pinned as constants. Each was produced with `UCI_Variant
// <name>` / `go perft` on the FSF oracle (`largeboards=yes` for the wide
// boards) and **re-confirmed against FSF on 2026-07-02**. The same numbers
// appear in the per-variant `tests/perft_<name>.rs` suites. FENs are the mce
// dialect (compound pieces spelled with mce's overflow tokens); FSF spells the
// compounds with its own single letters (see each `tests/perft_<name>.rs`).
// This test never invokes FSF — it only checks mce against the pinned counts.
// ===========================================================================

/// A generic-geometry reference: a label, its mce-dialect FEN, the pinned
/// `(depth, nodes)` pairs, and the cheap-layer cutoff. Checked through the typed
/// [`mce::geometry::perft`] for the geometry named by the enclosing group.
struct GeomCase {
    /// A short human label (the variant name).
    label: &'static str,
    /// The startpos in the mce FEN dialect.
    fen: &'static str,
    /// The pinned `(depth, node-count)` reference pairs.
    nodes: &'static [(u32, u64)],
    /// The deepest depth that runs in the cheap (non-`#[ignore]`) layer.
    cheap_max: u32,
}

/// Emits a `<cheap>` / `<deep>` `#[test]` pair that runs every [`GeomCase`] in
/// `$cases` through `gperft::<$geom, _>`, the cheap test keeping depths
/// `<= cheap_max` and the `#[ignore]`d deep test the rest. `$ty` is the variant
/// position type (its `from_fen` parses the mce-dialect FEN).
macro_rules! geom_group {
    ($cheap:ident, $deep:ident, $ty:ty, $geom:ty, $cases:expr) => {
        #[test]
        fn $cheap() {
            for c in $cases {
                let pos = <$ty>::from_fen(c.fen).expect("valid variant FEN");
                run(
                    c.label,
                    c.fen,
                    c.nodes,
                    |d| d <= c.cheap_max,
                    |d| gperft::<$geom, _>(&pos, d),
                );
            }
        }

        #[test]
        #[ignore = "deep perft; run with --release --test perft_corpus -- --ignored"]
        fn $deep() {
            for c in $cases {
                let pos = <$ty>::from_fen(c.fen).expect("valid variant FEN");
                run(
                    c.label,
                    c.fen,
                    c.nodes,
                    |d| d > c.cheap_max,
                    |d| gperft::<$geom, _>(&pos, d),
                );
            }
        }
    };
}

// -- 8x8 Capablanca-family (Queen swapped / asymmetric armies) --------------
// FSF `almost` / `amazon` / `chigorin`, confirmed 2026-07-02. Each is its own
// rules type on the shared `Chess8x8` geometry, so it gets its own group.
const ALMOST: &[GeomCase] = &[GeomCase {
    // almost: Queen -> Chancellor (Rook+Knight). FSF almost startpos.
    label: "almost",
    fen: "rnbekbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBEKBNR w KQkq - 0 1",
    nodes: &[(1, 22), (2, 484), (3, 11895), (4, 290522), (5, 7812388)],
    cheap_max: 4,
}];
geom_group!(
    geom_almost_cheap,
    geom_almost_deep,
    Almost,
    Chess8x8,
    ALMOST
);

const AMAZON: &[GeomCase] = &[GeomCase {
    // amazon: Queen -> Amazon (Queen+Knight). FSF amazon startpos.
    label: "amazon",
    fen: "rnb**akbnr/pppppppp/8/8/8/8/PPPPPPPP/RNB**AKBNR w KQkq - 0 1",
    nodes: &[(1, 22), (2, 484), (3, 12483), (4, 318185), (5, 9319911)],
    cheap_max: 4,
}];
geom_group!(
    geom_amazon_cheap,
    geom_amazon_deep,
    Amazon,
    Chess8x8,
    AMAZON
);

const CHIGORIN: &[GeomCase] = &[GeomCase {
    // chigorin: White knight army + Chancellor vs Black bishop army + Queen.
    // FSF chigorin startpos.
    label: "chigorin",
    fen: "rbbqkbbr/pppppppp/8/8/8/8/PPPPPPPP/RNNEKNNR w KQkq - 0 1",
    nodes: &[(1, 26), (2, 416), (3, 11408), (4, 229973), (5, 6624527)],
    cheap_max: 4,
}];
geom_group!(
    geom_chigorin_cheap,
    geom_chigorin_deep,
    Chigorin,
    Chess8x8,
    CHIGORIN
);

// -- 10x8 Capablanca board (Chancellor + Archbishop army) -------------------
// FSF `gothic` / `embassy` / `janus` / `caparandom` (largeboards), confirmed
// 2026-07-02. All share the `Cap10x8` geometry but each is its own rules type,
// so each gets its own group.
const GOTHIC: &[GeomCase] = &[GeomCase {
    // gothic: Capablanca board, Gothic back-rank order. FSF gothic startpos.
    label: "gothic",
    fen: "rnbqekabnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNBQEKABNR w KQkq - 0 1",
    nodes: &[(1, 28), (2, 784), (3, 25283), (4, 808984), (5, 28946187)],
    cheap_max: 3,
}];
geom_group!(geom_gothic_cheap, geom_gothic_deep, Gothic, Cap10x8, GOTHIC);

const EMBASSY: &[GeomCase] = &[GeomCase {
    // embassy: Capablanca board, king on e-file. FSF embassy startpos.
    label: "embassy",
    fen: "rnbqkeabnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNBQKEABNR w KQkq - 0 1",
    nodes: &[(1, 28), (2, 784), (3, 25281), (4, 809539), (5, 28937546)],
    cheap_max: 3,
}];
geom_group!(
    geom_embassy_cheap,
    geom_embassy_deep,
    Embassy,
    Cap10x8,
    EMBASSY
);

const JANUS: &[GeomCase] = &[GeomCase {
    // janus: Capablanca board, two Januses (Bishop+Knight), no Chancellor. FSF
    // janus startpos.
    label: "janus",
    fen: "ranbkqbnar/pppppppppp/10/10/10/10/PPPPPPPPPP/RANBKQBNAR w KQkq - 0 1",
    nodes: &[(1, 28), (2, 782), (3, 24747), (4, 772074), (5, 26869186)],
    cheap_max: 3,
}];
geom_group!(geom_janus_cheap, geom_janus_deep, Janus, Cap10x8, JANUS);

const CAPARANDOM: &[GeomCase] = &[GeomCase {
    // caparandom: Capablanca army shuffled, Chess960-style castling (file-letter
    // `JAja` rights). FSF caparandom startpos (canonical Capablanca array).
    label: "caparandom",
    fen: "rnabqkbenr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBENR w JAja - 0 1",
    nodes: &[(1, 28), (2, 784), (3, 25228), (4, 805128)],
    cheap_max: 3,
}];
geom_group!(
    geom_caparandom_cheap,
    geom_caparandom_deep,
    Caparandom,
    Cap10x8,
    CAPARANDOM
);

// -- 9x9 Chancellor chess --------------------------------------------------
// FSF `chancellor` (largeboards), confirmed 2026-07-02.
const CHANCELLOR: &[GeomCase] = &[GeomCase {
    // chancellor: standard army widened to 9x9 with a Chancellor. FSF startpos.
    label: "chancellor",
    fen: "rnbqkenbr/ppppppppp/9/9/9/9/9/PPPPPPPPP/RNBQKENBR w KQkq - 0 1",
    nodes: &[(1, 24), (2, 576), (3, 15896), (4, 436656), (5, 13466196)],
    cheap_max: 3,
}];
geom_group!(
    geom_chancellor_cheap,
    geom_chancellor_deep,
    Chancellor,
    Chess9x9,
    CHANCELLOR
);

// -- 12x8 Courier chess ----------------------------------------------------
// FSF `courier` (largeboards), confirmed 2026-07-02. Non-standard start array
// (advanced a/g/l pawns, Ferz on g3/g6); short-range Alfil/Ferz/Wazir/Man army.
const COURIER: &[GeomCase] = &[GeomCase {
    label: "courier",
    fen:
        "rn*xb*uk1*jb*xnr/1ppppp1pppp1/6m5/p5p4p/P5P4P/6M5/1PPPPP1PPPP1/RN*XB*UK1*JB*XNR w - - 0 1",
    nodes: &[(1, 26), (2, 678), (3, 18406), (4, 500337), (5, 14144849)],
    cheap_max: 3,
}];
geom_group!(
    geom_courier_cheap,
    geom_courier_deep,
    Courier,
    Courier12x8,
    COURIER
);

// -- 10x10 Omega-family (Grand board + extra leapers) ----------------------
// FSF `tencubed` / `opulent` (largeboards), confirmed 2026-07-02.
const TENCUBED: &[GeomCase] = &[GeomCase {
    // tencubed: Grand army + Wizard (Camel+Ferz) + Champion (Wazir+Alfil+Dabbaba).
    label: "tencubed",
    fen: "2**x**wae**w**x2/1rnbqkbnr1/pppppppppp/10/10/10/10/PPPPPPPPPP/1RNBQKBNR1/2**X**WAE**W**X2 w - - 0 1",
    nodes: &[(1, 40), (2, 1600), (3, 68230), (4, 2906895), (5, 131575398)],
    cheap_max: 3,
}];
geom_group!(
    geom_tencubed_cheap,
    geom_tencubed_deep,
    Tencubed,
    Grand10x10,
    TENCUBED
);

const OPULENT: &[GeomCase] = &[GeomCase {
    // opulent: Grand army + Wizard + Lion (Ferz+Dabbaba+Threeleaper) + augmented
    // Knight (Knight+Wazir).
    label: "opulent",
    fen: "r**w6**wr/e**yb**zqk**zb**ya/pppppppppp/10/10/10/10/PPPPPPPPPP/E**YB**ZQK**ZB**YA/R**W6**WR w - - 0 1",
    nodes: &[(1, 50), (2, 2500), (3, 133829), (4, 7147971), (5, 402780823)],
    cheap_max: 3,
}];
geom_group!(
    geom_opulent_cheap,
    geom_opulent_deep,
    Opulent,
    Grand10x10,
    OPULENT
);

// ===========================================================================
// Chu Shogi (12x12) — HaChu-referenced, pinned as constants.
//
// The reference oracle is HaChu (H. G. Muller); FSF has no Chu perft. Depth 1
// (36) is a byte-identical move-set match; depth 2 (1296) matches HaChu
// node-for-node. Depth 3 pins mce's **correct** 48319: HaChu 0.23 yields 48317
// because of a documented anti-diagonal-Lion bug (it misses two legal captures
// after `1. f3f5 d8d7`) — mce is right, and every other node matches. Depth 4
// (1802285) is an mce-only regression pin (a node-by-node HaChu cross-check at
// ~1.8M nodes is intractable). See `tests/perft_chu.rs` for the full write-up.
// ===========================================================================

/// Chu start-position perft, cheap layer (depths 1-3). Depth 3's 48319 is mce's
/// correct value (HaChu 0.23's buggy 48317 is *not* pinned; see the module head).
#[test]
fn chu_cheap() {
    let pos = Chu::startpos();
    assert_eq!(gperft::<Chu12x12, _>(&pos, 1), 36);
    assert_eq!(gperft::<Chu12x12, _>(&pos, 2), 1296);
    assert_eq!(gperft::<Chu12x12, _>(&pos, 3), 48319);
}

/// Chu start-position perft, deep layer (depth 4): an mce regression pin.
#[test]
#[ignore = "deep perft; run with --release --test perft_corpus -- --ignored"]
fn chu_deep() {
    let pos = Chu::startpos();
    assert_eq!(gperft::<Chu12x12, _>(&pos, 4), 1802285);
}
