//! Per-variant memory-footprint sweep: the runtime-memory counterpart of the
//! `variants` throughput sweep.
//!
//! `variants.rs` measures *time* for every registered variant; this bench
//! measures *space*. For **every** variant — all wide (fairy) variants in
//! [`WideVariantId::ALL`] and all concrete 8x8 variants in [`VariantId::ALL`] —
//! it emits, from the starting position:
//!
//! * `size_of` / `align_of` of the variant's concrete position (the static
//!   per-variant fact reported by
//!   [`WideVariantId::position_footprint`](mcr::geometry::WideVariantId::position_footprint)
//!   for the wide arms, and computed directly for the concrete arms), and
//! * the **heap** a shallow move generation touches: the byte size of the
//!   `legal_moves()` result buffer, and — via a counting `#[global_allocator]`
//!   installed for this bench binary — the number of heap allocations and bytes a
//!   fixed shallow perft performs from that position.
//!
//! The allocator wraps [`System`] and atomically tallies allocations and
//! requested bytes (the same technique as `compare/src/alloc.rs`); the sweep
//! snapshots the counters immediately around each measured region, so the
//! surrounding criterion / formatting allocations are excluded and the reported
//! figure is the work's own heap traffic. The table is printed to stderr on every
//! run (criterion leaves stderr uncaptured), and the registered criterion
//! benchmark is a trivial `black_box`-guarded sum so the report participates in
//! `criterion_main!` while staying essentially free.
//!
//! Because it walks both registries, a newly registered variant is covered
//! automatically. Run with `cargo bench --bench footprint`.
//!
//! [`WideVariantId::ALL`]: mcr::geometry::WideVariantId::ALL
//! [`VariantId::ALL`]: mcr::VariantId

// The library crate denies `unsafe_code`, but a counting `#[global_allocator]`
// intrinsically requires an `unsafe impl GlobalAlloc`. This allow is scoped to
// this one bench binary (it never ships — benches are excluded from the package)
// and overrides the package-level `deny` for this crate root only.
#![allow(unsafe_code)]

use criterion::{criterion_group, criterion_main, Criterion};
use mcr::geometry::{AnyWideVariant, WideMove, WideVariantId};
use mcr::{AnyVariant, Move, VariantId};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::mem::size_of;
use std::sync::atomic::{AtomicU64, Ordering};

// ----- Counting global allocator ---------------------------------------------

/// Number of successful allocations since process start.
static ALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
/// Total bytes requested across all successful allocations since process start.
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);

/// A `System`-backed allocator that atomically counts allocations and the bytes
/// requested. Only the allocating entry points bump the counters; deallocation
/// is forwarded untouched. Requested (`Layout`) bytes are counted rather than
/// rounded-up usable sizes so the figure is deterministic across runs.
struct CountingAlloc;

// SAFETY: every method forwards to the corresponding `System` method with the
// same arguments and returns its pointer unchanged; the only added work is a
// pair of relaxed atomic fetch-adds on the success path, which cannot affect
// allocator soundness.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        if !ptr.is_null() {
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc_zeroed(layout);
        if !ptr.is_null() {
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = System.realloc(ptr, layout, new_size);
        if !new_ptr.is_null() && new_size > layout.size() {
            // Count the growth as a fresh allocation of the additional bytes.
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add((new_size - layout.size()) as u64, Ordering::Relaxed);
        }
        new_ptr
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

/// The `(allocations, bytes)` a closure performs, measured by snapshotting the
/// global counters immediately around it. The result is `black_box`-guarded so
/// the work is not optimized away.
fn alloc_delta<T>(f: impl FnOnce() -> T) -> (u64, u64) {
    let c0 = ALLOC_COUNT.load(Ordering::Relaxed);
    let b0 = ALLOC_BYTES.load(Ordering::Relaxed);
    let out = f();
    let c1 = ALLOC_COUNT.load(Ordering::Relaxed);
    let b1 = ALLOC_BYTES.load(Ordering::Relaxed);
    black_box(out);
    (c1.wrapping_sub(c0), b1.wrapping_sub(b0))
}

// ----- Sweep ------------------------------------------------------------------

/// Fixed perft depth for the allocation measurement — the same two-ply depth the
/// `variants` throughput sweep uses, so the time and space sweeps line up.
const PERFT_DEPTH: u32 = 2;

/// One measured row of the footprint report.
struct Footprint {
    name: String,
    size: usize,
    align: usize,
    moves: usize,
    move_bytes: usize,
    perft_nodes: u64,
    perft_allocs: u64,
    perft_bytes: u64,
}

/// Measures one wide (fairy) variant from its starting position.
fn measure_wide(id: WideVariantId) -> Footprint {
    let (size, align) = id.position_footprint();
    let pos = AnyWideVariant::startpos(id);
    let moves = pos.legal_moves();
    let move_bytes = moves.len() * size_of::<WideMove>();
    let moves_len = moves.len();
    drop(moves);

    let mut perft_nodes = 0;
    let (perft_allocs, perft_bytes) = alloc_delta(|| {
        perft_nodes = pos.perft(PERFT_DEPTH);
        perft_nodes
    });

    Footprint {
        name: id.as_str().to_string(),
        size,
        align,
        moves: moves_len,
        move_bytes,
        perft_nodes,
        perft_allocs,
        perft_bytes,
    }
}

/// Measures one concrete 8x8 variant from its starting position. The concrete
/// engine encodes moves as [`Move`] (a `u16`) rather than [`WideMove`].
fn measure_concrete(id: VariantId) -> Footprint {
    let pos = AnyVariant::startpos(id);
    let moves = pos.legal_moves();
    let move_bytes = moves.len() * size_of::<Move>();
    let moves_len = moves.len();
    drop(moves);

    // `AnyVariant` stores each arm inline, so the enum size is an upper bound on
    // every arm; it is the runtime footprint a caller who dispatches at runtime
    // pays, and the closest concrete-side analogue of the wide accessor.
    let size = size_of::<AnyVariant>();
    let align = std::mem::align_of::<AnyVariant>();

    let mut perft_nodes = 0;
    let (perft_allocs, perft_bytes) = alloc_delta(|| {
        perft_nodes = pos.perft(PERFT_DEPTH);
        perft_nodes
    });

    Footprint {
        name: id.as_str().to_string(),
        size,
        align,
        moves: moves_len,
        move_bytes,
        perft_nodes,
        perft_allocs,
        perft_bytes,
    }
}

/// Prints the footprint report to stderr and returns a `black_box` guard total.
fn report() -> u64 {
    let mut rows: Vec<Footprint> = WideVariantId::ALL
        .iter()
        .map(|&id| measure_wide(id))
        .collect();
    let wide_count = rows.len();
    rows.extend(VariantId::ALL.iter().map(|&id| measure_concrete(id)));
    let concrete_count = rows.len() - wide_count;

    let width = rows.iter().map(|r| r.name.len()).max().unwrap_or(0).max(7);

    eprintln!(
        "\n=== per-variant footprint (perft depth {PERFT_DEPTH} alloc counts) ===\
         \n{wide_count} wide + {concrete_count} concrete variants\n"
    );
    eprintln!(
        "{:<width$}  {:>7}  {:>5}  {:>6}  {:>9}  {:>8}  {:>10}  {:>12}",
        "variant", "size_of", "align", "moves", "mv_bytes", "p_nodes", "p_allocs", "p_bytes",
    );

    let mut guard = 0u64;
    for r in &rows {
        eprintln!(
            "{:<width$}  {:>7}  {:>5}  {:>6}  {:>9}  {:>8}  {:>10}  {:>12}",
            r.name,
            r.size,
            r.align,
            r.moves,
            r.move_bytes,
            r.perft_nodes,
            r.perft_allocs,
            r.perft_bytes,
        );
        guard = guard
            .wrapping_add(r.size as u64)
            .wrapping_add(r.perft_nodes)
            .wrapping_add(r.perft_allocs);
    }
    eprintln!("======================================================================\n");
    guard
}

fn bench_footprint(c: &mut Criterion) {
    // Emit the human-readable report once up front.
    let total = report();

    // Register a trivial guarded benchmark so the report is part of the criterion
    // run without doing meaningful work.
    let mut group = c.benchmark_group("footprint");
    group.bench_function("report_guard", |b| {
        b.iter(|| black_box(black_box(total)));
    });
    group.finish();
}

criterion_group!(benches, bench_footprint);
criterion_main!(benches);
