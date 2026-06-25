//! Variant move-generation benchmarks for hot variant paths.
//!
//! Atomic and Crazyhouse have the most divergent (and expensive) move-generation
//! logic — Atomic resolves capture explosions, Crazyhouse layers piece drops on
//! top of board moves — so they are the most useful variants to track for
//! regressions.
//!
//! Run with `cargo bench --bench variants`.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mce::{Atomic, Crazyhouse};

// A middlegame atomic position with live capture/explosion interactions.
const ATOMIC_MIDGAME: &str = "rn2kb1r/1pp1p2p/p2q1pp1/3P4/2P3b1/4PN2/PP3PPP/R2QKB1R b KQkq - 0 1";
// A crazyhouse middlegame with non-empty pockets, exercising both board moves
// and drop generation.
const CRAZYHOUSE_MIDGAME: &str =
    "r1bqk2r/pppp1ppp/2n1p3/4P3/1b1Pn3/2NB1N2/PPP2PPP/R1BQK2R[NPbp] b KQkq - 0 1";

fn bench_variants(c: &mut Criterion) {
    let mut group = c.benchmark_group("variant_legal_moves");

    let atomic = Atomic::from_fen(ATOMIC_MIDGAME).expect("valid atomic FEN");
    group.bench_function("atomic", |b| {
        b.iter(|| black_box(black_box(&atomic).legal_moves()));
    });

    let crazyhouse: Crazyhouse = CRAZYHOUSE_MIDGAME.parse().expect("valid crazyhouse FEN");
    group.bench_function("crazyhouse", |b| {
        b.iter(|| black_box(black_box(&crazyhouse).legal_moves()));
    });

    group.finish();
}

criterion_group!(benches, bench_variants);
criterion_main!(benches);
