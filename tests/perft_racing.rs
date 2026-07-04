//! Racing Kings perft regression tests.
//!
//! The reference node counts are public facts transcribed verbatim from
//! shakmaty's `tests/racingkings.perft` data table (the same numbers used by
//! lichess / shakmaty to validate Racing Kings move generation):
//!
//! - <https://github.com/niklasf/shakmaty/blob/main/shakmaty/tests/racingkings.perft>
//!
//! None of the numbers are invented. The cheap depths run in CI; the deep depths
//! are `#[ignore]`d and meant to be run with
//! `cargo test --release -- --ignored`.

use mcr::{perft_variant, RacingKings};

/// One reference position: its FEN and the published `(depth, node-count)` pairs.
struct PerftCase {
    /// A short label (the shakmaty position id).
    id: &'static str,
    /// The starting position FEN.
    fen: &'static str,
    /// The published `(depth, nodes)` reference pairs.
    nodes: &'static [(u32, u64)],
}

/// The transcribed reference table from shakmaty's `racingkings.perft`:
///
/// - `racingkings-start`: the canonical start `8/8/8/8/8/8/krbnNBRK/qrbnNBRQ`.
/// - `occupied-goal`: a midgame position with pieces already standing on the
///   eighth (goal) rank, which exercises movegen with the goal rank occupied.
const CASES: &[PerftCase] = &[
    PerftCase {
        id: "racingkings-start",
        fen: "8/8/8/8/8/8/krbnNBRK/qrbnNBRQ w - - 0 1",
        nodes: &[(1, 21), (2, 421), (3, 11264), (4, 296242)],
    },
    PerftCase {
        id: "occupied-goal",
        fen: "4brn1/2K2k2/8/8/8/8/8/8 w - - 0 1",
        nodes: &[
            (1, 6),
            (2, 33),
            (3, 178),
            (4, 3151),
            (5, 12981),
            (6, 265932),
        ],
    },
];

/// The depth below which a `(depth, nodes)` pair runs in CI; deeper pairs are
/// reserved for the `#[ignore]`d release sweep.
const CHEAP_DEPTH: u32 = 4;

/// Asserts the published counts for `case` at the depths matching `keep`.
fn run_case(case: &PerftCase, keep: impl Fn(u32) -> bool) {
    let pos = RacingKings::from_fen(case.fen).expect("valid racing-kings FEN");
    for &(depth, expected) in case.nodes {
        if !keep(depth) {
            continue;
        }
        let got = perft_variant(&pos, depth);
        assert_eq!(
            got, expected,
            "racing-kings perft({depth}) for {} ({}): expected {expected}, got {got}",
            case.id, case.fen
        );
    }
}

#[test]
fn racing_kings_perft_shallow() {
    for case in CASES {
        run_case(case, |depth| depth < CHEAP_DEPTH);
    }
}

#[test]
#[ignore = "deep perft; run with --release -- --ignored"]
fn racing_kings_perft_deep() {
    for case in CASES {
        run_case(case, |depth| depth >= CHEAP_DEPTH);
    }
}
