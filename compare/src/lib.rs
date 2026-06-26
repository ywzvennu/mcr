//! Shared benchmark fixtures for the mce-vs-shakmaty perft comparison.
//!
//! This crate links the GPL-3.0+ `shakmaty` crate for benchmarking only and is
//! never published or distributed. See the crate `README.md` for the licensing
//! rationale. The `mce` library itself does not depend on shakmaty.
//!
//! # The basket
//!
//! Rather than one start position per variant, the suite runs a *curated basket*
//! of positions per variant — opening, midgame, tactical, and endgame — so the
//! measured throughput reflects a realistic mix of node shapes instead of just
//! the (atypically branchy) starting position. Every basket position is run
//! through both engines at the same depth and the node counts are asserted
//! equal, which makes the suite a broad, independent correctness cross-check in
//! addition to a benchmark.
//!
//! FENs and depths are reused from the `mce` regression tests
//! (`tests/perft*.rs`) wherever possible; a handful of extra antichess and
//! endgame FENs are documented inline. Depths are chosen so each individual
//! perft visits roughly 1–20M nodes (large enough that timing is signal, not
//! noise) while the whole suite still finishes in a reasonable time.

use shakmaty::fen::Fen;
use shakmaty::variant::{
    Antichess, Atomic, Crazyhouse, Horde, KingOfTheHill, RacingKings, ThreeCheck,
};
use shakmaty::{CastlingMode, Chess as ShChess};

/// A single benchmark position: its variant, a short human label, the FEN, and
/// the perft depth to run.
pub struct Case {
    /// Variant key (e.g. `"standard"`, `"atomic"`); selects the engine path.
    pub variant: &'static str,
    /// Short label distinguishing this position within its variant's basket.
    pub position: &'static str,
    /// The position in FEN (variant-extended where applicable).
    pub fen: &'static str,
    /// Perft depth to run for this position.
    pub depth: u32,
}

/// The ordered list of variant keys, used for grouping output by variant.
pub const VARIANTS: &[&str] = &[
    "standard",
    "chess960",
    "king-of-the-hill",
    "three-check",
    "racing-kings",
    "atomic",
    "antichess",
    "horde",
    "crazyhouse",
];

/// The full benchmark basket: standard chess plus all eight variants, each with
/// several positions exercising opening / midgame / tactical / endgame shapes.
///
/// Depths are tuned (see module docs) so each perft visits ~1–20M nodes. The
/// reference node counts are the published perft values transcribed in the mce
/// regression tests; positions whose deep counts are not in those tests are
/// still fully validated here because mce and shakmaty must agree on every one.
pub const CASES: &[Case] = &[
    // ---- standard: canonical perft positions ------------------------------
    Case {
        variant: "standard",
        position: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        depth: 5, // 4,865,609
    },
    Case {
        variant: "standard",
        position: "kiwipete",
        fen: "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        depth: 4, // 4,085,603
    },
    Case {
        variant: "standard",
        position: "cpw3-rook-ep",
        fen: "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        depth: 6, // 11,030,083
    },
    Case {
        variant: "standard",
        position: "cpw4-promotions",
        fen: "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        depth: 5, // 15,833,292
    },
    Case {
        variant: "standard",
        position: "cpw5-tactical",
        fen: "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        depth: 4, // 2,103,487
    },
    Case {
        variant: "standard",
        position: "cpw6-midgame",
        fen: "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
        depth: 4, // 3,894,594
    },
    Case {
        // Quiet king-and-pawnless endgame (two lone kings); low branching,
        // deep search — the opposite shape from the branchy startpos.
        variant: "standard",
        position: "kvk-endgame",
        fen: "8/8/8/4k3/8/4K3/8/8 w - - 0 1",
        depth: 8, // 4,676,096
    },
    // ---- chess960: midgame Fischer-random arrangements (Ethereal fischer.epd)
    Case {
        variant: "chess960",
        position: "frc-id0",
        fen: "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w HFhf - 2 9",
        depth: 5, // 8,146,062
    },
    Case {
        variant: "chess960",
        position: "frc-id1",
        fen: "2nnrbkr/p1qppppp/8/1ppb4/6PP/3PP3/PPP2P2/BQNNRBKR w HEhe - 1 9",
        depth: 5, // 16,253,601
    },
    Case {
        variant: "chess960",
        position: "frc-id2",
        fen: "b1q1rrkb/pppppppp/3nn3/8/P7/1PPP4/4PPPP/BQNNRKRB w GE - 1 9",
        depth: 5, // 6,417,013
    },
    Case {
        variant: "chess960",
        position: "frc-id6",
        fen: "q1bnrkr1/ppppp2p/2n2p2/4b1p1/2NP4/8/PPP1PPPP/QNB1RRKB w ge - 1 9",
        depth: 5, // 21,093,346 (slightly above 20M, kept for variety)
    },
    Case {
        variant: "chess960",
        position: "frc-id9",
        fen: "qn1rbbkr/ppp2p1p/1n1pp1p1/8/3P4/P6P/1PP1PPPK/QNNRBB1R w hd - 2 9",
        depth: 5, // 19,836,606
    },
    // ---- king-of-the-hill -------------------------------------------------
    Case {
        variant: "king-of-the-hill",
        position: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        depth: 5, // 4,865,609
    },
    Case {
        variant: "king-of-the-hill",
        position: "kiwipete",
        fen: "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        depth: 4, // 4,085,603
    },
    Case {
        variant: "king-of-the-hill",
        position: "cpw3-rook-ep",
        fen: "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        depth: 6, // 11,030,083
    },
    Case {
        variant: "king-of-the-hill",
        position: "cpw6-midgame",
        fen: "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
        depth: 4, // 3,894,594
    },
    Case {
        // Tactical middlegame (pieces hanging in the centre). The hill-race
        // terminal condition is checked on every node but never triggers within
        // this depth, so the node count matches standard chess and shakmaty —
        // unlike a kings-near-the-hill endgame, where shakmaty stops expanding a
        // line the instant a king reaches a hill square and mce does not, so
        // those positions cannot be used for an apples-to-apples parity check.
        variant: "king-of-the-hill",
        position: "cpw5-tactical",
        fen: "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        depth: 4, // 2,103,487
    },
    // ---- three-check: varying remaining-check counts ----------------------
    Case {
        variant: "three-check",
        position: "startpos-3+3",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 3+3",
        depth: 5, // 4,865,609
    },
    Case {
        variant: "three-check",
        position: "kiwipete-3+3",
        fen: "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1 3+3",
        depth: 4, // 4,085,603
    },
    Case {
        // All three-check positions keep the full 3+3 budget and use depths at
        // which neither king is checked three times on any line. That keeps the
        // node count identical to standard chess and to shakmaty: when the check
        // budget *is* exhausted within the search, shakmaty stops expanding the
        // (now decided) line while mce keeps counting, so a low-budget or
        // forcing position would mismatch and is deliberately avoided here. The
        // three-check terminal check still runs on every node — it just never
        // fires within these depths.
        variant: "three-check",
        position: "cpw4-3+3",
        fen: "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1 3+3",
        depth: 5, // 15,833,292
    },
    Case {
        variant: "three-check",
        position: "cpw5-3+3",
        fen: "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8 3+3",
        depth: 4, // 2,103,487
    },
    Case {
        variant: "three-check",
        position: "cpw6-3+3",
        fen: "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10 3+3",
        depth: 4, // 3,894,594
    },
    // ---- racing-kings -----------------------------------------------------
    Case {
        variant: "racing-kings",
        position: "startpos",
        fen: "8/8/8/8/8/8/krbnNBRK/qrbnNBRQ w - - 0 1",
        depth: 5, // 9,472,927
    },
    Case {
        // Both kings already advanced into the open board — the race is on and
        // many lines reach the 8th-rank goal, exercising the win condition.
        variant: "racing-kings",
        position: "occupied-goal",
        fen: "4brn1/2K2k2/8/8/8/8/8/8 w - - 0 1",
        depth: 7, // 1,244,842
    },
    // ---- atomic: capture/explosion-rich middlegames -----------------------
    Case {
        variant: "atomic",
        position: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        depth: 5, // 4,864,979
    },
    Case {
        variant: "atomic",
        position: "programfox-1",
        fen: "rn2kb1r/1pp1p2p/p2q1pp1/3P4/2P3b1/4PN2/PP3PPP/R2QKB1R b KQkq - 0 1",
        depth: 4, // 1,434,825
    },
    Case {
        variant: "atomic",
        position: "programfox-2",
        fen: "rn1qkb1r/p5pp/2p5/3p4/N3P3/5P2/PPP4P/R1BQK3 w Qkq - 0 1",
        depth: 5, // 21,134,061 (slightly above 20M, kept for an explosion-heavy mix)
    },
    // ---- antichess: forced-capture-heavy positions ------------------------
    Case {
        variant: "antichess",
        position: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1",
        depth: 5, // 2,732,672
    },
    Case {
        // After 1.e4 e5 2.Nc3 (forced-capture-rich open centre). New FEN
        // (not in the regression tests); validated here by mce==shakmaty.
        variant: "antichess",
        position: "open-center",
        fen: "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/8/PPPP1PPP/RNBQKBNR w - - 2 3",
        depth: 5, // 2,799,464
    },
    Case {
        // After 1.e4 c5 (asymmetric open centre). New FEN; validated by parity.
        variant: "antichess",
        position: "sicilian-ish",
        fen: "rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w - - 0 2",
        depth: 5, // 2,421,128
    },
    Case {
        // Lone queen vs. lone king giveaway endgame: forced-capture chains
        // dominate. New FEN; validated by parity.
        variant: "antichess",
        position: "q-vs-k-giveaway",
        fen: "8/8/8/2k5/5Q2/8/8/2K5 w - - 0 1",
        depth: 6, // 3,207,363
    },
    // ---- horde: pawn-swarm positions --------------------------------------
    Case {
        variant: "horde",
        position: "startpos",
        fen: "rnbqkbnr/pppppppp/8/1PP2PP1/PPPPPPPP/PPPPPPPP/PPPPPPPP/PPPPPPPP w kq - 0 1",
        depth: 6, // 5,396,554
    },
    Case {
        variant: "horde",
        position: "open-flank",
        fen: "4k3/pp4q1/3P2p1/8/P3PP2/PPP2r2/PPP5/PPPP4 b - - 0 1",
        depth: 6, // 14,177,327
    },
    Case {
        variant: "horde",
        position: "en-passant",
        fen: "k7/5p2/4p2P/3p2P1/2p2P2/1p2P2P/p2P2P1/2P2P2 w - - 0 1",
        depth: 6, // 7,174,007
    },
    // ---- crazyhouse: non-empty pockets / drop-heavy -----------------------
    Case {
        variant: "crazyhouse",
        position: "middlegame",
        fen: "r1bqk2r/pppp1ppp/2n1p3/4P3/1b1Pn3/2NB1N2/PPP2PPP/R1BQK2R[] b KQkq -",
        depth: 4, // 2,083,382
    },
    Case {
        // Non-empty pockets on a near-empty board: drops dominate the move list.
        variant: "crazyhouse",
        position: "drops-Qn",
        fen: "2k5/8/8/8/8/8/8/4K3[Qn] w - -",
        depth: 4, // 932,554
    },
    Case {
        // A full both-colour pocket of every piece type (no pawns, to stay
        // within shakmaty's material limit): the drop count explodes.
        variant: "crazyhouse",
        position: "drops-many",
        fen: "2k5/8/8/8/8/8/8/4K3[QRBNqrbn] w - -",
        depth: 3, // 8,493,545
    },
];

/// All cases for a given variant key, in declaration order.
pub fn cases_for(variant: &'static str) -> impl Iterator<Item = &'static Case> {
    CASES.iter().filter(move |c| c.variant == variant)
}

/// Look up the first case for a variant label. Panics if unknown (used for
/// the criterion benches, which sample one representative position per variant).
pub fn case(variant: &str) -> &'static Case {
    CASES
        .iter()
        .find(|c| c.variant == variant)
        .unwrap_or_else(|| panic!("no benchmark case for variant {variant:?}"))
}

/// Run perft for `case` using the `mce` engine.
pub fn mce_perft(case: &Case) -> u64 {
    use mce::{
        perft_variant, Antichess as MAntichess, Atomic as MAtomic, Chess, Chess960,
        Crazyhouse as MCrazyhouse, Horde as MHorde, KingOfTheHill as MKoth, RacingKings as MRacing,
        ThreeCheck as MThreeCheck,
    };

    macro_rules! run {
        ($ty:ty) => {{
            let pos = <$ty>::from_fen(case.fen).expect("valid mce FEN");
            perft_variant(&pos, case.depth)
        }};
    }

    match case.variant {
        "standard" => run!(Chess),
        "chess960" => run!(Chess960),
        "king-of-the-hill" => run!(MKoth),
        "three-check" => run!(MThreeCheck),
        "racing-kings" => run!(MRacing),
        "atomic" => run!(MAtomic),
        "antichess" => run!(MAntichess),
        "horde" => run!(MHorde),
        "crazyhouse" => run!(MCrazyhouse),
        other => panic!("unknown variant {other:?}"),
    }
}

/// Run perft for `case` using the `shakmaty` engine.
///
/// Standard and the variant types parse with [`CastlingMode::Standard`];
/// Chess960 uses [`CastlingMode::Chess960`] so the X-FEN rights resolve.
pub fn shakmaty_perft(case: &Case) -> u64 {
    macro_rules! run {
        ($ty:ty, $mode:expr) => {{
            let pos: $ty = Fen::from_ascii(case.fen.as_bytes())
                .expect("valid shakmaty FEN")
                .into_position($mode)
                .expect("legal shakmaty position");
            shakmaty::perft(&pos, case.depth)
        }};
    }

    match case.variant {
        "standard" => run!(ShChess, CastlingMode::Standard),
        "chess960" => run!(ShChess, CastlingMode::Chess960),
        "king-of-the-hill" => run!(KingOfTheHill, CastlingMode::Standard),
        "three-check" => run!(ThreeCheck, CastlingMode::Standard),
        "racing-kings" => run!(RacingKings, CastlingMode::Standard),
        "atomic" => run!(Atomic, CastlingMode::Standard),
        "antichess" => run!(Antichess, CastlingMode::Standard),
        "horde" => run!(Horde, CastlingMode::Standard),
        "crazyhouse" => run!(Crazyhouse, CastlingMode::Standard),
        other => panic!("unknown variant {other:?}"),
    }
}
