//! Variant move-generation benchmarks for hot variant paths.
//!
//! Two harnesses live here:
//!
//! * The **classic 8x8 variants** — Atomic and Crazyhouse — have the most
//!   divergent (and expensive) move-generation logic on the frozen `u64` path:
//!   Atomic resolves capture explosions, Crazyhouse layers piece drops on top of
//!   board moves, so they are the most useful classic variants to track.
//! * The **wide fairy-variant sweep** — every variant in [`WideVariantId::ALL`],
//!   benched uniformly through [`AnyWideVariant`]. Rather than a hand-picked
//!   subset, this iterates the whole registry, so a newly registered variant
//!   (the Capablanca family, courier, tencubed, opulent, the U256-backed Chu
//!   Shogi with its Lion mechanics, …) is covered automatically the moment it
//!   joins the enum. Each variant is benched from its starting position for both
//!   `legal_moves` throughput and a fixed shallow perft (depth 2 — one make-move
//!   ply plus a fresh generation, a good aggregate move-gen signal that stays
//!   bounded even for the wide-branching boards).
//!
//! Run with `cargo bench --bench variants`. To scope to one group, filter by its
//! name, e.g. `cargo bench --bench variants -- wide_variant_perft`.

use criterion::{criterion_group, criterion_main, Criterion};
use mce::geometry::{AnyWideVariant, WideVariantId};
use mce::{Atomic, Crazyhouse};
use std::hint::black_box;

// A middlegame atomic position with live capture/explosion interactions.
const ATOMIC_MIDGAME: &str = "rn2kb1r/1pp1p2p/p2q1pp1/3P4/2P3b1/4PN2/PP3PPP/R2QKB1R b KQkq - 0 1";
// A crazyhouse middlegame with non-empty pockets, exercising both board moves
// and drop generation.
const CRAZYHOUSE_MIDGAME: &str =
    "r1bqk2r/pppp1ppp/2n1p3/4P3/1b1Pn3/2NB1N2/PPP2PPP/R1BQK2R[NPbp] b KQkq - 0 1";

/// Fixed perft depth for the wide-variant sweep. Two plies exercises legal-move
/// generation, make-move, and a second generation from the resulting positions —
/// enough to catch a hot-path regression — while staying bounded on the
/// wide-branching boards (Duck, Chu) so a full sweep finishes quickly.
const WIDE_PERFT_DEPTH: u32 = 2;

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

/// Move-generation throughput from the starting position for every registered
/// wide (fairy) variant.
fn bench_wide_movegen(c: &mut Criterion) {
    let mut group = c.benchmark_group("wide_variant_movegen");
    for &id in WideVariantId::ALL {
        let pos = AnyWideVariant::startpos(id);
        group.bench_function(id.as_str(), |b| {
            b.iter(|| black_box(black_box(&pos).legal_moves()));
        });
    }
    group.finish();
}

/// Fixed-depth perft throughput from the starting position for every registered
/// wide (fairy) variant.
fn bench_wide_perft(c: &mut Criterion) {
    let mut group = c.benchmark_group("wide_variant_perft");
    for &id in WideVariantId::ALL {
        let pos = AnyWideVariant::startpos(id);
        group.bench_function(id.as_str(), |b| {
            b.iter(|| black_box(black_box(&pos).perft(black_box(WIDE_PERFT_DEPTH))));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_variants, bench_wide_movegen, bench_wide_perft);
criterion_main!(benches);
