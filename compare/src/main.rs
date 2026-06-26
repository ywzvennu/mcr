//! mce-vs-shakmaty perft benchmark — headline report.
//!
//! Runs standard chess and all eight variants through both engines on identical
//! positions, asserts the node counts match, and prints:
//!
//! * a CPU timing table (median wall time + nodes/s per engine);
//! * a memory table (heap allocations + bytes per engine per variant);
//! * one-time lines for the process peak RSS and each engine's static
//!   lookup-table footprint.
//!
//! Build and run in release for meaningful numbers:
//!
//! ```text
//! cargo run --release -p mce-compare      # from the repo root
//! cargo run --release --bin mce-compare   # from inside compare/
//! ```
//!
//! ## Memory methodology
//!
//! * **Heap allocs / bytes** come from a counting [`#[global_allocator]`](alloc)
//!   that wraps `std::alloc::System`. We snapshot the counters immediately
//!   before and after each engine's perft call and report the delta. The
//!   allocation measurement is run as a *separate pass* from the timing pass so
//!   the (cheap, relaxed-atomic) counter never perturbs the CPU table.
//! * **Peak RSS** is read once from `/proc/self/status` `VmHWM` (see [`rss`]).
//!   It is process-wide and monotonic, so it is reported once for the whole run
//!   rather than per engine.
//! * **Static table footprint** is computed from the known table shapes of each
//!   engine (the tables are private, so they are not introspectable from here).
//!   See [`footprint`].
//!
//! This binary links GPL-3.0+ shakmaty for benchmarking only; it is never
//! published or distributed and does not affect the mce library's licensing.

mod alloc;
mod footprint;
mod rss;

use std::time::{Duration, Instant};

use mce_compare::{mce_perft, shakmaty_perft, CASES};

/// Install the counting allocator as the program-wide global allocator.
#[global_allocator]
static GLOBAL: alloc::CountingAlloc = alloc::CountingAlloc;

/// How many timed repetitions to run per engine per case; the median is kept.
const REPS: usize = 5;

/// Time `f` `REPS` times and return `(median_duration, last_result)`.
fn measure(mut f: impl FnMut() -> u64) -> (Duration, u64) {
    let mut samples = Vec::with_capacity(REPS);
    let mut nodes = 0;
    for _ in 0..REPS {
        let start = Instant::now();
        nodes = f();
        samples.push(start.elapsed());
    }
    samples.sort_unstable();
    (samples[samples.len() / 2], nodes)
}

/// Run `f` once, returning `(alloc_delta, result)` measured by the global
/// counting allocator around the single call.
fn measure_allocs(mut f: impl FnMut() -> u64) -> (alloc::AllocDelta, u64) {
    let before = alloc::snapshot();
    let nodes = f();
    let after = alloc::snapshot();
    (alloc::delta(before, after), nodes)
}

fn main() {
    println!("mce vs shakmaty — perft benchmark (release, median of {REPS} runs)");
    println!("engines: mce (path) vs shakmaty 0.27");
    println!();

    let all_match = cpu_and_memory_tables();

    println!();
    footprint::report();

    println!();
    match rss::peak_rss_kib() {
        Some(kib) => println!(
            "peak RSS (process, VmHWM from /proc/self/status): {kib} KiB ({:.1} MiB)",
            kib as f64 / 1024.0,
        ),
        None => println!("peak RSS: n/a (VmHWM unavailable; non-Linux platform?)"),
    }

    println!();
    if all_match {
        println!("All node counts matched between mce and shakmaty.");
    } else {
        eprintln!("ERROR: one or more node counts did NOT match — see messages above.");
        std::process::exit(1);
    }
}

/// Print the CPU timing table and the per-variant memory table.
///
/// Timing and allocation counting are done in two separate passes per variant
/// so the allocator counters never sit inside the timed region. Returns whether
/// every node count matched between the two engines.
fn cpu_and_memory_tables() -> bool {
    // --- CPU timing table ---------------------------------------------------
    let cpu_head = format!(
        "{:<17} {:>5} {:>12} {:>10} {:>11} {:>9} {:>13} {:>13}",
        "variant", "depth", "nodes", "mce ms", "shak ms", "ratio", "mce Mn/s", "shak Mn/s",
    );
    println!("{cpu_head}");
    println!("{}", "-".repeat(cpu_head.len()));

    let mut all_match = true;
    // Remember the per-variant allocation results to print as a second table.
    let mut mem_rows: Vec<(&'static str, alloc::AllocDelta, alloc::AllocDelta)> =
        Vec::with_capacity(CASES.len());

    for case in CASES {
        // Timing pass (counters ignored — kept out of the timed region).
        let (mce_dur, mce_nodes) = measure(|| mce_perft(case));
        let (shak_dur, shak_nodes) = measure(|| shakmaty_perft(case));

        if mce_nodes != shak_nodes {
            all_match = false;
            eprintln!(
                "*** NODE COUNT MISMATCH for {} (depth {}): mce={} shakmaty={} ***",
                case.variant, case.depth, mce_nodes, shak_nodes
            );
        }

        let mce_ms = mce_dur.as_secs_f64() * 1e3;
        let shak_ms = shak_dur.as_secs_f64() * 1e3;
        let ratio = shak_ms / mce_ms;
        let mce_mnps = mce_nodes as f64 / mce_dur.as_secs_f64() / 1e6;
        let shak_mnps = shak_nodes as f64 / shak_dur.as_secs_f64() / 1e6;

        println!(
            "{:<17} {:>5} {:>12} {:>10.2} {:>11.2} {:>9.2} {:>13.1} {:>13.1}",
            case.variant, case.depth, mce_nodes, mce_ms, shak_ms, ratio, mce_mnps, shak_mnps,
        );

        // Allocation pass (separate, untimed) for the same position.
        let (mce_alloc, _) = measure_allocs(|| mce_perft(case));
        let (shak_alloc, _) = measure_allocs(|| shakmaty_perft(case));
        mem_rows.push((case.variant, mce_alloc, shak_alloc));
    }

    // --- Memory table -------------------------------------------------------
    println!();
    println!("memory — heap allocations during one perft call per engine:");
    let mem_head = format!(
        "{:<17} {:>12} {:>12} {:>12} {:>12}",
        "variant", "mce allocs", "mce KiB", "shak allocs", "shak KiB",
    );
    println!("{mem_head}");
    println!("{}", "-".repeat(mem_head.len()));
    for (variant, mce_alloc, shak_alloc) in &mem_rows {
        println!(
            "{:<17} {:>12} {:>12.1} {:>12} {:>12.1}",
            variant,
            mce_alloc.count,
            mce_alloc.bytes as f64 / 1024.0,
            shak_alloc.count,
            shak_alloc.bytes as f64 / 1024.0,
        );
    }

    all_match
}
