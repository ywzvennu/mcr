//! Perft for atomic chess through the generic variant layer.
//!
//! Atomic has its own king-safety rules (capture explosions, connected kings,
//! exploding the enemy king out of check) and a destination-centred blast, so
//! its node counts diverge from standard chess. The reference numbers below are
//! the published atomic perft values transcribed verbatim from shakmaty's data
//! table `shakmaty/tests/atomic.perft` (which matches CPW / lichess). They are
//! public facts; only the engine code is original.
//!
//! Cheap depths run in CI; the deeper sweeps are `#[ignore]`d and meant for a
//! `cargo test --release -- --ignored` run.
//!
//! The shakmaty table also contains three `atomic960-castle-*` positions that
//! combine atomic with Chess960 castling geometry. Those require the
//! atomic-on-960 combination (Shredder-FEN castling rights and arbitrary rook
//! files), which is a different variant pairing than the standard-castling
//! `Atomic` exercised here, so they are intentionally not included.

use mcr::{perft_variant, Atomic};

/// Asserts every `(depth, expected)` pair for the atomic position parsed from
/// `fen`.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Atomic::from_fen(fen).expect("valid atomic FEN");
    for &(depth, expected) in cases {
        let got = perft_variant(&pos, depth);
        assert_eq!(
            got, expected,
            "atomic perft({depth}) for {fen}: expected {expected}, got {got}"
        );
    }
}

// -- id atomic-start -------------------------------------------------------

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

#[test]
fn perft_start_shallow() {
    check(START, &[(1, 20), (2, 400), (3, 8902), (4, 197326)]);
}

// -- id programfox-1 (also id shakmaty-bench, identical position) ----------

const PROGRAMFOX_1: &str = "rn2kb1r/1pp1p2p/p2q1pp1/3P4/2P3b1/4PN2/PP3PPP/R2QKB1R b KQkq - 0 1";

#[test]
fn perft_programfox_1_shallow() {
    check(PROGRAMFOX_1, &[(1, 40), (2, 1238), (3, 45237)]);
}

#[test]
#[ignore = "deep perft; run with --release -- --ignored"]
fn perft_programfox_1_deep() {
    check(PROGRAMFOX_1, &[(4, 1434825)]);
}

// -- id programfox-2 -------------------------------------------------------

const PROGRAMFOX_2: &str = "rn1qkb1r/p5pp/2p5/3p4/N3P3/5P2/PPP4P/R1BQK3 w Qkq - 0 1";

#[test]
fn perft_programfox_2_shallow() {
    check(PROGRAMFOX_2, &[(1, 28), (2, 833), (3, 23353)]);
}

#[test]
#[ignore = "deep perft; run with --release -- --ignored"]
fn perft_programfox_2_deep() {
    check(PROGRAMFOX_2, &[(4, 714499)]);
}
