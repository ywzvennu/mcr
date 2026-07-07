//! Large-board (U256, 12x12 / 15x15 / 16x16) perft and memory-footprint
//! benchmarks.
//!
//! The `geometry` bench tops out at the 10x10 / `u128` Grand geometry. This
//! bench covers the geometry family the others do not exercise under a
//! benchmark: the **U256-backed large-shogi boards** — `Chu12x12` (144 squares),
//! `Dai15x15` (225), and `Tenjiku16x16` (256, filling the two-limb backing
//! exactly) — with their ranging sliders, Lion mechanics, and (Tenjiku) jumping
//! generals and Fire Demon. A regression in the widest backing therefore has an
//! in-repo, FSF-free timing signal.
//!
//! Two harnesses live here:
//!
//! * **`largeboard_perft`** — start-position perft over `Chu12x12`, `Dai15x15`,
//!   and `Tenjiku16x16`, each at two shallow depths. Every node count is pinned
//!   from the matching `tests/perft_*.rs` and asserted in-bench before measuring,
//!   so the bench validates against known-correct values (Chu d2 = 1296 is
//!   HaChu-exact, d3 = 48319 matches HaChu bar one documented HaChu bug; Dai
//!   d1 = 71 / d2 = 5041 and Tenjiku d1 = 72 / d2 = 5663 are HaChu-validated —
//!   see the `perft_dai` / `perft_tenjiku` docs). Depth is kept shallower as the
//!   board grows because U256 perft cost climbs steeply with board size.
//!
//! * **`footprint`** — a `size_of` / `align_of` report for the key move types and
//!   the three backing integers (`u64` / `u128` / U256), plus **every registered
//!   wide variant's concrete position** (iterated over [`WideVariantId::ALL`] via
//!   [`WideVariantId::position_footprint`], so a newly registered variant is
//!   covered automatically), and a movegen buffer sense-check (the heap bytes a
//!   `legal_moves()` result holds at the Chu start position). The table is printed
//!   to stderr on every run; the registered benchmark itself is a trivial
//!   `black_box`-guarded sum so the report participates in `criterion_main!` while
//!   staying essentially free.
//!
//! Run with `cargo bench --bench largeboard`.
//!
//! [`perft`]: mcr::geometry::perft
//! [`WideVariantId::ALL`]: mcr::geometry::WideVariantId::ALL

use criterion::{criterion_group, criterion_main, Criterion};
use mcr::geometry::{
    perft, Chu, Chu12x12, Dai, Dai15x15, Tenjiku, Tenjiku16x16, WideMove, WideVariantId, U256,
};
use mcr::{Move, Position};
use std::hint::black_box;
use std::mem::{align_of, size_of};

// ----- U256 large-board perft -------------------------------------------------

/// Start-position Chu perft node counts, pinned from `tests/perft_chu.rs`.
/// `(depth, expected)` — both validated against the HaChu oracle there.
const CHU_PERFT: &[(u32, u64)] = &[(2, 1296), (3, 48319)];

/// Start-position Dai perft node counts, pinned from `tests/perft_dai.rs`.
/// `(depth, expected)` — both HaChu-validated node-for-node.
const DAI_PERFT: &[(u32, u64)] = &[(1, 71), (2, 5041)];

/// Start-position Tenjiku perft node counts, pinned from `tests/perft_tenjiku.rs`.
/// depth 1 = 72 is HaChu-validated node-for-node; depth 2 = 5663 is the mcr
/// regression pin. Tenjiku is the widest board (256 squares), so it is kept the
/// shallowest.
const TENJIKU_PERFT: &[(u32, u64)] = &[(1, 72), (2, 5663)];

fn bench_perft(c: &mut Criterion) {
    let mut group = c.benchmark_group("largeboard_perft");

    let chu = Chu::startpos();
    for &(depth, expected) in CHU_PERFT {
        // Self-validate against the known-correct count before measuring, so a
        // movegen regression fails the bench rather than silently timing wrong
        // numbers.
        let got = perft::<Chu12x12, _, _>(&chu, depth);
        assert_eq!(
            got, expected,
            "chu startpos perft({depth}) = {got}, expected {expected}"
        );
        group.bench_function(format!("chu_12x12/startpos_d{depth}"), |b| {
            b.iter(|| perft::<Chu12x12, _, _>(black_box(&chu), black_box(depth)));
        });
    }

    let dai = Dai::startpos();
    for &(depth, expected) in DAI_PERFT {
        let got = perft::<Dai15x15, _, _>(&dai, depth);
        assert_eq!(
            got, expected,
            "dai startpos perft({depth}) = {got}, expected {expected}"
        );
        group.bench_function(format!("dai_15x15/startpos_d{depth}"), |b| {
            b.iter(|| perft::<Dai15x15, _, _>(black_box(&dai), black_box(depth)));
        });
    }

    let tenjiku = Tenjiku::startpos();
    for &(depth, expected) in TENJIKU_PERFT {
        let got = perft::<Tenjiku16x16, _, _>(&tenjiku, depth);
        assert_eq!(
            got, expected,
            "tenjiku startpos perft({depth}) = {got}, expected {expected}"
        );
        group.bench_function(format!("tenjiku_16x16/startpos_d{depth}"), |b| {
            b.iter(|| perft::<Tenjiku16x16, _, _>(black_box(&tenjiku), black_box(depth)));
        });
    }

    group.finish();
}

// ----- Memory / size_of footprint --------------------------------------------

/// One row of the footprint report: a type name and its `size_of` / `align_of`.
fn row(name: &str, size: usize, align: usize) -> (String, usize, usize) {
    (name.to_string(), size, align)
}

/// Collects the `size_of` / `align_of` table: the key move types and the three
/// backing integers, then **every** registered wide variant's concrete
/// `GenericPosition`, iterated over [`WideVariantId::ALL`] via
/// [`WideVariantId::position_footprint`] rather than a hand-picked handful. A
/// newly registered variant therefore joins the report automatically.
fn footprint_table() -> Vec<(String, usize, usize)> {
    let mut rows = vec![
        // Move encodings (backing-independent).
        row("Move (concrete u16)", size_of::<Move>(), align_of::<Move>()),
        row(
            "WideMove (u64)",
            size_of::<WideMove>(),
            align_of::<WideMove>(),
        ),
        // Backing integers.
        row("u64 backing", size_of::<u64>(), align_of::<u64>()),
        row("u128 backing", size_of::<u128>(), align_of::<u128>()),
        row("U256 backing", size_of::<U256>(), align_of::<U256>()),
        // Concrete frozen 8x8 engine position (u64 bitboards).
        row(
            "Position (concrete 8x8/u64)",
            size_of::<Position>(),
            align_of::<Position>(),
        ),
    ];

    // Every registered wide variant's concrete `GenericPosition`.
    for &id in WideVariantId::ALL {
        let (size, align) = id.position_footprint();
        rows.push(row(
            &format!("GenericPosition {}", id.as_str()),
            size,
            align,
        ));
    }

    rows
}

/// Prints the footprint report to stderr (criterion leaves stderr uncaptured, so
/// this shows on every run) and returns the summed byte total for the guard.
fn report_footprint() -> usize {
    let table = footprint_table();
    let width = table.iter().map(|(n, _, _)| n.len()).max().unwrap_or(0);

    eprintln!("\n=== size_of / align_of footprint ===");
    eprintln!("{:<width$}  {:>7}  {:>5}", "type", "size_of", "align");
    let mut total = 0usize;
    for (name, size, align) in &table {
        eprintln!("{name:<width$}  {size:>7}  {align:>5}");
        total += *size;
    }

    // Movegen buffer sense-check: the heap held by a `legal_moves()` result at
    // the Chu start position (the U256 path's widest-branching startpos).
    let moves = Chu::startpos().legal_moves();
    let buf_bytes = moves.len() * size_of::<WideMove>();
    eprintln!(
        "chu startpos legal_moves(): {} moves, {} heap bytes ({} B/move)",
        moves.len(),
        buf_bytes,
        size_of::<WideMove>()
    );
    eprintln!("=====================================\n");

    total + buf_bytes
}

fn bench_footprint(c: &mut Criterion) {
    // Emit the human-readable report once up front.
    let total = report_footprint();

    // Register a trivial guarded benchmark so the report is part of the criterion
    // run without doing meaningful work.
    let mut group = c.benchmark_group("footprint");
    group.bench_function("size_of_sum", |b| {
        b.iter(|| black_box(black_box(total)));
    });
    group.finish();
}

criterion_group!(benches, bench_perft, bench_footprint);
criterion_main!(benches);
