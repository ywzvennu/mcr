//! Large-board (U256 / 12x12) perft and memory-footprint benchmarks.
//!
//! The `geometry` bench tops out at the 10x10 / `u128` Grand geometry. This
//! bench covers the one geometry family the others do not exercise under a
//! benchmark: the **U256-backed `Chu12x12`** (144-square) layer that hosts Chu
//! Shogi, with its Lion mechanics and promote-on-entry rule. A regression in the
//! widest backing therefore has an in-repo, FSF-free timing signal.
//!
//! Two harnesses live here:
//!
//! * **`largeboard_perft`** — start-position perft over `Chu12x12` at two shallow
//!   depths. Both node counts are pinned from `tests/perft_chu.rs` and asserted
//!   in-bench before measuring, so the bench is validating against known-correct
//!   values (depth 2 = 1296 is HaChu-exact; depth 3 = 48319 matches HaChu at
//!   every node bar one documented HaChu bug — see `variants::chu` docs). Depths
//!   are kept shallow because U256 perft is heavy.
//!
//! * **`footprint`** — a `size_of` / `align_of` report for the key position and
//!   move types across the three backing integers (`u64` / `u128` / U256), plus a
//!   movegen buffer sense-check (the heap bytes a `legal_moves()` result holds at
//!   the Chu start position). The table is printed to stderr on every run; the
//!   registered benchmark itself is a trivial `black_box`-guarded sum so the
//!   report participates in `criterion_main!` while staying essentially free.
//!
//! Run with `cargo bench --bench largeboard`.
//!
//! [`perft`]: mcr::geometry::perft

use criterion::{criterion_group, criterion_main, Criterion};
use mcr::geometry::{perft, Chu, Chu12x12, Grand, Seirawan, WideMove, U256};
use mcr::{Move, Position};
use std::hint::black_box;
use std::mem::{align_of, size_of};

// ----- U256 / 12x12 perft -----------------------------------------------------

/// Start-position Chu perft node counts, pinned from `tests/perft_chu.rs`.
/// `(depth, expected)` — both validated against the HaChu oracle there.
const CHU_PERFT: &[(u32, u64)] = &[(2, 1296), (3, 48319)];

fn bench_perft(c: &mut Criterion) {
    let mut group = c.benchmark_group("largeboard_perft");

    let start = Chu::startpos();
    for &(depth, expected) in CHU_PERFT {
        // Self-validate against the known-correct count before measuring, so a
        // movegen regression fails the bench rather than silently timing wrong
        // numbers.
        let got = perft::<Chu12x12, _>(&start, depth);
        assert_eq!(
            got, expected,
            "chu startpos perft({depth}) = {got}, expected {expected}"
        );
        group.bench_function(format!("chu_12x12/startpos_d{depth}"), |b| {
            b.iter(|| perft::<Chu12x12, _>(black_box(&start), black_box(depth)));
        });
    }

    group.finish();
}

// ----- Memory / size_of footprint --------------------------------------------

/// One row of the footprint report: a type name and its `size_of` / `align_of`.
fn row(name: &str, size: usize, align: usize) -> (String, usize, usize) {
    (name.to_string(), size, align)
}

/// Collects the `size_of` / `align_of` table for the key move and position types
/// across the three backing integers.
fn footprint_table() -> Vec<(String, usize, usize)> {
    vec![
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
        // GenericPosition across the three backings.
        row(
            "GenericPosition Seirawan (Chess8x8/u64)",
            size_of::<Seirawan>(),
            align_of::<Seirawan>(),
        ),
        row(
            "GenericPosition Grand (Grand10x10/u128)",
            size_of::<Grand>(),
            align_of::<Grand>(),
        ),
        row(
            "GenericPosition Chu (Chu12x12/U256)",
            size_of::<Chu>(),
            align_of::<Chu>(),
        ),
    ]
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
