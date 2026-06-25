//! Shared benchmark fixtures for the mce-vs-shakmaty perft comparison.
//!
//! This crate links the GPL-3.0+ `shakmaty` crate for benchmarking only and is
//! never published or distributed. See the crate `README.md` for the licensing
//! rationale. The `mce` library itself does not depend on shakmaty.
//!
//! Every benchmark case runs the same position and depth through both engines
//! and asserts the node counts agree, which keeps the comparison fair and
//! independently re-validates mce's move generation against shakmaty's.

use shakmaty::fen::Fen;
use shakmaty::variant::{
    Antichess, Atomic, Crazyhouse, Horde, KingOfTheHill, RacingKings, ThreeCheck,
};
use shakmaty::{CastlingMode, Chess as ShChess};

/// A single position to benchmark: a human label, the FEN, and the perft depth.
pub struct Case {
    pub variant: &'static str,
    pub fen: &'static str,
    pub depth: u32,
}

/// The benchmark suite: standard chess plus all eight supported variants.
///
/// FENs and the perft reference numbers are reused from the `mce` regression
/// tests (`tests/perft_*.rs`), which in turn transcribe the published shakmaty
/// data tables. Depths are tuned for meaningful-but-quick runs.
pub const CASES: &[Case] = &[
    Case {
        variant: "standard",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        depth: 5,
    },
    Case {
        // A non-standard Chess960 start (Ethereal fischer.epd id 0), Shredder
        // X-FEN castling rights. shakmaty must parse this with Chess960 mode.
        variant: "chess960",
        fen: "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w HFhf - 2 9",
        depth: 4,
    },
    Case {
        variant: "king-of-the-hill",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        depth: 5,
    },
    Case {
        variant: "three-check",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 3+3",
        depth: 5,
    },
    Case {
        variant: "racing-kings",
        fen: "8/8/8/8/8/8/krbnNBRK/qrbnNBRQ w - - 0 1",
        depth: 4,
    },
    Case {
        variant: "atomic",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        depth: 5,
    },
    Case {
        variant: "antichess",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1",
        depth: 5,
    },
    Case {
        variant: "horde",
        fen: "rnbqkbnr/pppppppp/8/1PP2PP1/PPPPPPPP/PPPPPPPP/PPPPPPPP/PPPPPPPP w kq - 0 1",
        depth: 5,
    },
    Case {
        variant: "crazyhouse",
        fen: "r1bqk2r/pppp1ppp/2n1p3/4P3/1b1Pn3/2NB1N2/PPP2PPP/R1BQK2R[] b KQkq -",
        depth: 3,
    },
];

/// Look up a case by its variant label. Panics if unknown (used for benches).
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
