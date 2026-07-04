//! Criterion benches comparing mcr against shakmaty per variant.
//!
//! One group per variant, each with an `mcr` and a `shakmaty` function running
//! the same position and depth as the headline binary. Sample sizes and depths
//! are kept modest so the full sweep finishes in reasonable time:
//!
//! ```text
//! cargo bench -p mcr-compare
//! ```
//!
//! Like the rest of this crate it links GPL-3.0+ shakmaty for benchmarking only
//! and is never published or distributed.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use mcr_compare::{case, mcr_perft, shakmaty_perft, VARIANTS};

fn bench_perft(c: &mut Criterion) {
    // One group per variant, benchmarking that variant's first (start) basket
    // position. The headline `mcr-compare` binary measures the whole basket;
    // these criterion groups give one stable representative per variant.
    for &variant in VARIANTS {
        let case = case(variant);
        let mut group = c.benchmark_group(format!("perft/{variant}"));
        // Perft at these depths is comparatively slow, so keep samples modest.
        group.sample_size(10);

        group.bench_function("mcr", |b| b.iter(|| black_box(mcr_perft(black_box(case)))));
        group.bench_function("shakmaty", |b| {
            b.iter(|| black_box(shakmaty_perft(black_box(case))))
        });

        group.finish();
    }
}

criterion_group!(benches, bench_perft);
criterion_main!(benches);
