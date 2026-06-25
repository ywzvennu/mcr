//! Criterion benches comparing mce against shakmaty per variant.
//!
//! One group per variant, each with an `mce` and a `shakmaty` function running
//! the same position and depth as the headline binary. Sample sizes and depths
//! are kept modest so the full sweep finishes in reasonable time:
//!
//! ```text
//! cargo bench -p mce-compare
//! ```
//!
//! Like the rest of this crate it links GPL-3.0+ shakmaty for benchmarking only
//! and is never published or distributed.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use mce_compare::{case, mce_perft, shakmaty_perft, CASES};

fn bench_perft(c: &mut Criterion) {
    for spec in CASES {
        let case = case(spec.variant);
        let mut group = c.benchmark_group(format!("perft/{}", spec.variant));
        // Perft at these depths is comparatively slow, so keep samples modest.
        group.sample_size(10);

        group.bench_function("mce", |b| b.iter(|| black_box(mce_perft(black_box(case)))));
        group.bench_function("shakmaty", |b| {
            b.iter(|| black_box(shakmaty_perft(black_box(case))))
        });

        group.finish();
    }
}

criterion_group!(benches, bench_perft);
criterion_main!(benches);
