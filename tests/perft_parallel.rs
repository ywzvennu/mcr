//! Parity tests for the optional `parallel` feature.
//!
//! `perft_parallel` / `perft_variant_parallel` split the root moves across a
//! rayon thread pool. Because perft is embarrassingly parallel — the subtrees
//! below the root's children are independent and `u64` addition is associative —
//! the node count must be byte-identical to the serial `perft` / `perft_variant`
//! regardless of how many threads sum the subtrees. These tests assert exactly
//! that equality, which is the acceptance criterion for the feature. The whole
//! file is gated on `parallel`, so a default `cargo test` compiles it away.
#![cfg(feature = "parallel")]

use mcr::{
    perft, perft_parallel, perft_variant, perft_variant_parallel, Atomic, Chess, Crazyhouse,
    Position,
};

const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

/// Standard `perft_parallel` must match serial `perft` on the startpos across a
/// range of depths, including the trivial depth-0/1 cases that bypass the pool.
#[test]
fn parallel_matches_serial_startpos() {
    let pos = Position::startpos();
    for depth in 0..=5 {
        assert_eq!(
            perft_parallel(&pos, depth),
            perft(&pos, depth),
            "startpos depth {depth}"
        );
    }
}

/// The same parity on the Kiwipete tactical position (high root branching, the
/// stress case for the split).
#[test]
fn parallel_matches_serial_kiwipete() {
    let pos = Position::from_fen(KIWIPETE).expect("valid FEN");
    for depth in 0..=4 {
        assert_eq!(
            perft_parallel(&pos, depth),
            perft(&pos, depth),
            "kiwipete depth {depth}"
        );
    }
}

/// `perft_variant_parallel` over standard `Chess` must reproduce the serial
/// variant counts (and, transitively, the standard `perft` numbers).
#[test]
fn parallel_variant_matches_serial_chess() {
    let pos = Chess::startpos();
    for depth in 0..=5 {
        assert_eq!(
            perft_variant_parallel(&pos, depth),
            perft_variant(&pos, depth),
            "chess depth {depth}"
        );
    }
}

/// Atomic has its own king-safety and blast rules, exercising the variant path
/// (not just the bulk-countable standard one) under the parallel split.
#[test]
fn parallel_variant_matches_serial_atomic() {
    let pos = Atomic::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
        .expect("valid atomic FEN");
    for depth in 0..=4 {
        assert_eq!(
            perft_variant_parallel(&pos, depth),
            perft_variant(&pos, depth),
            "atomic depth {depth}"
        );
    }
}

/// Crazyhouse adds drops (a different move set and, for some nodes, heap spill),
/// a second variant family for the parallel parity check.
#[test]
fn parallel_variant_matches_serial_crazyhouse() {
    let pos: Crazyhouse = "r1bqk2r/pppp1ppp/2n1p3/4P3/1b1Pn3/2NB1N2/PPP2PPP/R1BQK2R[] b KQkq -"
        .parse()
        .expect("valid crazyhouse FEN");
    for depth in 0..=4 {
        assert_eq!(
            perft_variant_parallel(&pos, depth),
            perft_variant(&pos, depth),
            "crazyhouse depth {depth}"
        );
    }
}
