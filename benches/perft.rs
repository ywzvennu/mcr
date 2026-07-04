//! Perft (move-generation tree walk) benchmarks at a fixed shallow depth.
//!
//! Perft exercises legal-move generation and make-move recursively, so it is a
//! good aggregate signal for move-generation throughput. Depths are kept shallow
//! (3 for the dense Kiwipete position, 4 for the start) so a bench run finishes
//! in seconds.
//!
//! Run with `cargo bench --bench perft`.

use criterion::{criterion_group, criterion_main, Criterion};
use mcr::{perft, Position};
use std::hint::black_box;

const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

fn parse(fen: &str) -> Position {
    Position::from_fen(fen).expect("valid FEN")
}

fn bench_perft(c: &mut Criterion) {
    let mut group = c.benchmark_group("perft");
    for &(name, fen, depth) in &[("startpos_d4", STARTPOS, 4), ("kiwipete_d3", KIWIPETE, 3)] {
        let pos = parse(fen);
        group.bench_function(name, |b| {
            b.iter(|| perft(black_box(&pos), black_box(depth)));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_perft);
criterion_main!(benches);
