//! mce-vs-shakmaty perft benchmark — comprehensive multi-position report.
//!
//! Runs a *curated basket* of positions per variant (opening / midgame /
//! tactical / endgame) through both engines, asserts the node counts match for
//! every position (a broad independent correctness cross-check), and prints:
//!
//! * a per-position CPU table (median ns/node, Mn/s, mce/shakmaty ratio, spread);
//! * a per-variant aggregated CPU table (median Mn/s + ratio + spread);
//! * a per-variant and aggregate memory table (heap allocs + bytes per engine);
//! * each engine's static lookup-table footprint and the process peak RSS;
//! * a parity summary (positions checked, all matched) and overall summary.
//!
//! Build and run in release for meaningful numbers:
//!
//! ```text
//! cargo run --release --bin mce-compare              # default (HQ sliders)
//! cargo run --release --features magic --bin mce-compare   # magic sliders
//! cargo run --release --bin mce-compare -- --csv    # machine-readable rows
//! cargo run --release --bin mce-compare -- --json   # machine-readable rows
//! ```
//!
//! ## CPU methodology
//!
//! For each position the two engines are sampled in an **interleaved** A/B/A/B…
//! schedule: every iteration times one mce run and one shakmaty run back to
//! back, alternating which engine goes first. This cancels slow thermal/clock
//! drift that would otherwise bias whichever engine was measured in a separate
//! block (the methodology that fixed earlier false ±5% readings). Each engine is
//! warmed up before timing, then sampled `SAMPLES` times; we report the median
//! (robust to outliers), the min, and a spread (IQR + coefficient of variation).
//!
//! ## Memory methodology
//!
//! * **Heap allocs / bytes** come from a counting [`#[global_allocator]`](alloc)
//!   that wraps `std::alloc::System`. We snapshot the counters immediately
//!   before and after each engine's perft call and report the delta. The
//!   allocation measurement is a *separate pass* from the timing pass so the
//!   (cheap, relaxed-atomic) counter never perturbs the CPU table.
//! * **Peak RSS** is read once from `/proc/self/status` `VmHWM` (see [`rss`]).
//!   It is process-wide and monotonic, so it is reported once for the whole run.
//! * **Static table footprint** is computed from the known table shapes of each
//!   engine (the tables are private, so they are not introspectable here). See
//!   [`footprint`].
//!
//! This binary links GPL-3.0+ shakmaty for benchmarking only; it is never
//! published or distributed and does not affect the mce library's licensing.

mod alloc;
mod footprint;
mod rss;
mod stats;

use std::time::Instant;

use mce_compare::{mce_perft, shakmaty_perft, Case, CASES, VARIANTS};
use stats::{summarize, TimeStats};

/// Install the counting allocator as the program-wide global allocator.
#[global_allocator]
static GLOBAL: alloc::CountingAlloc = alloc::CountingAlloc;

/// Warm-up iterations per engine before timing (not recorded).
const WARMUP: usize = 2;
/// Interleaved A/B samples taken per position per engine.
const SAMPLES: usize = 17;

/// Machine-readable output selected on the command line.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    /// Human-readable tables (default).
    Human,
    /// Append CSV rows after the tables.
    Csv,
    /// Append a JSON document after the tables.
    Json,
}

/// One fully measured basket position.
struct Measured {
    variant: &'static str,
    position: &'static str,
    depth: u32,
    nodes: u64,
    matched: bool,
    mce: TimeStats,
    shak: TimeStats,
    mce_alloc: alloc::AllocDelta,
    shak_alloc: alloc::AllocDelta,
}

impl Measured {
    /// mce throughput in millions of nodes per second (median-based).
    fn mce_mnps(&self) -> f64 {
        self.nodes as f64 / self.mce.median_s / 1e6
    }
    /// shakmaty throughput in millions of nodes per second (median-based).
    fn shak_mnps(&self) -> f64 {
        self.nodes as f64 / self.shak.median_s / 1e6
    }
    /// mce peak throughput (from the fastest sample) in Mn/s.
    fn mce_peak_mnps(&self) -> f64 {
        self.nodes as f64 / self.mce.min_s / 1e6
    }
    /// mce/shakmaty speed ratio (>1 means mce is faster), median-based.
    ///
    /// Guarded against a zero mce median (which a real ~1M+ node perft never
    /// produces, but keep the table free of `inf`/`NaN` regardless).
    fn ratio(&self) -> f64 {
        if self.mce.median_s > 0.0 {
            self.shak.median_s / self.mce.median_s
        } else {
            f64::NAN
        }
    }
}

fn main() {
    let mode = parse_args();

    println!("mce vs shakmaty — comprehensive perft benchmark");
    println!(
        "release build; {} positions; interleaved A/B sampling; {} samples + {} warmup per engine",
        CASES.len(),
        SAMPLES,
        WARMUP,
    );
    #[cfg(feature = "magic")]
    println!("mce slider backend: magic bitboards (--features magic)");
    #[cfg(not(feature = "magic"))]
    println!("mce slider backend: hyperbola-quintessence (default)");
    println!("engines: mce (path) vs shakmaty 0.27");
    println!();

    // ---- measure every basket position ------------------------------------
    let measured: Vec<Measured> = CASES.iter().map(measure_case).collect();

    per_position_cpu_table(&measured);
    println!();
    per_variant_cpu_table(&measured);
    println!();
    memory_table(&measured);
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
    let mismatches = parity_summary(&measured);

    match mode {
        OutputMode::Human => {}
        OutputMode::Csv => {
            println!();
            emit_csv(&measured);
        }
        OutputMode::Json => {
            println!();
            emit_json(&measured);
        }
    }

    println!();
    if mismatches == 0 {
        println!(
            "OK: all {} positions matched between mce and shakmaty.",
            measured.len()
        );
    } else {
        eprintln!(
            "ERROR: {mismatches} of {} positions did NOT match — see messages above.",
            measured.len()
        );
        std::process::exit(1);
    }
}

/// Parse the (tiny) command line: an optional `--csv` / `--json` flag.
fn parse_args() -> OutputMode {
    let mut mode = OutputMode::Human;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--csv" => mode = OutputMode::Csv,
            "--json" => mode = OutputMode::Json,
            "--help" | "-h" => {
                println!("usage: mce-compare [--csv | --json]");
                println!("  build with --features magic for the magic-bitboard slider numbers");
                std::process::exit(0);
            }
            other => {
                eprintln!("warning: ignoring unknown argument {other:?}");
            }
        }
    }
    mode
}

/// Warm up, then take `SAMPLES` interleaved A/B timings of both engines on one
/// case, and a separate untimed allocation pass; assert the node counts agree.
fn measure_case(case: &'static Case) -> Measured {
    // Warm up both engines (fills caches / branch predictors; not recorded).
    let mut mce_nodes = 0;
    let mut shak_nodes = 0;
    for _ in 0..WARMUP {
        mce_nodes = mce_perft(case);
        shak_nodes = shakmaty_perft(case);
    }

    let mut mce_samples = Vec::with_capacity(SAMPLES);
    let mut shak_samples = Vec::with_capacity(SAMPLES);
    // Interleave: alternate which engine is timed first each iteration so any
    // slow drift over the sampling window is shared evenly between the two.
    for i in 0..SAMPLES {
        if i % 2 == 0 {
            mce_samples.push(time_once(|| mce_nodes = mce_perft(case)));
            shak_samples.push(time_once(|| shak_nodes = shakmaty_perft(case)));
        } else {
            shak_samples.push(time_once(|| shak_nodes = shakmaty_perft(case)));
            mce_samples.push(time_once(|| mce_nodes = mce_perft(case)));
        }
    }

    let matched = mce_nodes == shak_nodes;
    if !matched {
        eprintln!(
            "*** NODE COUNT MISMATCH for {}/{} (depth {}): mce={} shakmaty={} ***",
            case.variant, case.position, case.depth, mce_nodes, shak_nodes
        );
    }

    // Separate, untimed allocation pass for the same position.
    let mce_alloc = measure_allocs(|| mce_perft(case));
    let shak_alloc = measure_allocs(|| shakmaty_perft(case));

    Measured {
        variant: case.variant,
        position: case.position,
        depth: case.depth,
        nodes: mce_nodes,
        matched,
        mce: summarize(&mce_samples),
        shak: summarize(&shak_samples),
        mce_alloc,
        shak_alloc,
    }
}

/// Time a single call of `f`, returning the elapsed nanoseconds.
fn time_once(mut f: impl FnMut()) -> u64 {
    let start = Instant::now();
    f();
    start.elapsed().as_nanos() as u64
}

/// Run `f` once, returning the allocation delta measured by the global counting
/// allocator around the single call.
fn measure_allocs(mut f: impl FnMut() -> u64) -> alloc::AllocDelta {
    let before = alloc::snapshot();
    let _ = f();
    let after = alloc::snapshot();
    alloc::delta(before, after)
}

/// Per-position CPU table: median ns/node, Mn/s for each engine, ratio, spread.
fn per_position_cpu_table(m: &[Measured]) {
    // Every position is sampled the same number of times; surface it once.
    let n = m.first().map(|r| r.mce.n).unwrap_or(SAMPLES);
    println!("CPU — per position (median of {n} interleaved samples; peak = fastest sample):");
    let head = format!(
        "{:<16} {:<16} {:>3} {:>12} {:>9} {:>9} {:>9} {:>6} {:>7} {:>7}",
        "variant",
        "position",
        "d",
        "nodes",
        "mce Mn/s",
        "mce peak",
        "shak Mn/s",
        "ratio",
        "mce iqr",
        "mce cv",
    );
    println!("{head}");
    println!("{}", "-".repeat(head.len()));
    for r in m {
        let flag = if r.matched { "" } else { "  <-- MISMATCH" };
        println!(
            "{:<16} {:<16} {:>3} {:>12} {:>9.1} {:>9.1} {:>9.1} {:>6.2} {:>6.1}% {:>6.1}%{}",
            r.variant,
            r.position,
            r.depth,
            r.nodes,
            r.mce_mnps(),
            r.mce_peak_mnps(),
            r.shak_mnps(),
            r.ratio(),
            r.mce.rel_iqr() * 100.0,
            r.mce.cv() * 100.0,
            flag,
        );
    }
}

/// Per-variant aggregated CPU table.
///
/// Aggregation sums each variant's total node count and total median wall time
/// across its basket, so the variant Mn/s is a node-weighted throughput (the
/// natural "how fast does this engine churn this variant's mix" figure). The
/// spread column reports the worst per-position coefficient of variation in the
/// variant, i.e. how noisy the noisiest position was.
fn per_variant_cpu_table(m: &[Measured]) {
    println!("CPU — aggregated per variant (node-weighted median throughput):");
    let head = format!(
        "{:<16} {:>5} {:>14} {:>11} {:>11} {:>8} {:>9}",
        "variant", "pos", "total nodes", "mce Mn/s", "shak Mn/s", "ratio", "max cv",
    );
    println!("{head}");
    println!("{}", "-".repeat(head.len()));

    let mut g_nodes = 0u64;
    let mut g_mce_s = 0.0;
    let mut g_shak_s = 0.0;
    for &variant in VARIANTS {
        let rows: Vec<&Measured> = m.iter().filter(|r| r.variant == variant).collect();
        if rows.is_empty() {
            continue;
        }
        let nodes: u64 = rows.iter().map(|r| r.nodes).sum();
        let mce_s: f64 = rows.iter().map(|r| r.mce.median_s).sum();
        let shak_s: f64 = rows.iter().map(|r| r.shak.median_s).sum();
        let max_cv = rows
            .iter()
            .map(|r| r.mce.cv().max(r.shak.cv()))
            .fold(0.0, f64::max);

        g_nodes += nodes;
        g_mce_s += mce_s;
        g_shak_s += shak_s;

        println!(
            "{:<16} {:>5} {:>14} {:>11.1} {:>11.1} {:>8.2} {:>8.1}%",
            variant,
            rows.len(),
            nodes,
            nodes as f64 / mce_s / 1e6,
            nodes as f64 / shak_s / 1e6,
            shak_s / mce_s,
            max_cv * 100.0,
        );
    }
    println!("{}", "-".repeat(head.len()));
    println!(
        "{:<16} {:>5} {:>14} {:>11.1} {:>11.1} {:>8.2} {:>9}",
        "OVERALL",
        m.len(),
        g_nodes,
        g_nodes as f64 / g_mce_s / 1e6,
        g_nodes as f64 / g_shak_s / 1e6,
        g_shak_s / g_mce_s,
        "",
    );
}

/// Per-variant and aggregate memory table (heap allocs + bytes per engine).
///
/// Values are summed over each variant's basket positions, then over all
/// positions for the OVERALL row.
fn memory_table(m: &[Measured]) {
    println!("memory — heap allocations during perft, summed over each variant's basket:");
    let head = format!(
        "{:<16} {:>5} {:>14} {:>13} {:>14} {:>13}",
        "variant", "pos", "mce allocs", "mce KiB", "shak allocs", "shak KiB",
    );
    println!("{head}");
    println!("{}", "-".repeat(head.len()));

    let (mut g_mce_c, mut g_mce_b, mut g_shak_c, mut g_shak_b) = (0u64, 0u64, 0u64, 0u64);
    for &variant in VARIANTS {
        let rows: Vec<&Measured> = m.iter().filter(|r| r.variant == variant).collect();
        if rows.is_empty() {
            continue;
        }
        let mce_c: u64 = rows.iter().map(|r| r.mce_alloc.count).sum();
        let mce_b: u64 = rows.iter().map(|r| r.mce_alloc.bytes).sum();
        let shak_c: u64 = rows.iter().map(|r| r.shak_alloc.count).sum();
        let shak_b: u64 = rows.iter().map(|r| r.shak_alloc.bytes).sum();
        g_mce_c += mce_c;
        g_mce_b += mce_b;
        g_shak_c += shak_c;
        g_shak_b += shak_b;
        println!(
            "{:<16} {:>5} {:>14} {:>13.1} {:>14} {:>13.1}",
            variant,
            rows.len(),
            mce_c,
            mce_b as f64 / 1024.0,
            shak_c,
            shak_b as f64 / 1024.0,
        );
    }
    println!("{}", "-".repeat(head.len()));
    println!(
        "{:<16} {:>5} {:>14} {:>13.1} {:>14} {:>13.1}",
        "OVERALL",
        m.len(),
        g_mce_c,
        g_mce_b as f64 / 1024.0,
        g_shak_c,
        g_shak_b as f64 / 1024.0,
    );
}

/// Print the parity summary and return the number of mismatched positions.
fn parity_summary(m: &[Measured]) -> usize {
    let total = m.len();
    let mismatches = m.iter().filter(|r| !r.matched).count();
    let total_nodes: u64 = m.iter().map(|r| r.nodes).sum();
    println!("parity — mce perft node counts vs shakmaty (independent cross-check):");
    println!(
        "  variants: {}   positions: {}   total nodes verified: {}",
        VARIANTS.len(),
        total,
        total_nodes,
    );
    if mismatches == 0 {
        println!("  result:   ALL {total} positions matched.");
    } else {
        println!("  result:   {mismatches} of {total} positions MISMATCHED (see errors above).");
    }
    mismatches
}

/// Emit one CSV row per position for tracking the numbers over time.
fn emit_csv(m: &[Measured]) {
    println!("--- csv ---");
    println!(
        "variant,position,depth,nodes,matched,mce_mnps,shak_mnps,ratio,\
mce_median_s,shak_median_s,mce_min_s,shak_min_s,mce_cv,shak_cv,\
mce_allocs,mce_bytes,shak_allocs,shak_bytes"
    );
    for r in m {
        println!(
            "{},{},{},{},{},{:.3},{:.3},{:.4},{:.9},{:.9},{:.9},{:.9},{:.5},{:.5},{},{},{},{}",
            r.variant,
            r.position,
            r.depth,
            r.nodes,
            r.matched,
            r.mce_mnps(),
            r.shak_mnps(),
            r.ratio(),
            r.mce.median_s,
            r.shak.median_s,
            r.mce.min_s,
            r.shak.min_s,
            r.mce.cv(),
            r.shak.cv(),
            r.mce_alloc.count,
            r.mce_alloc.bytes,
            r.shak_alloc.count,
            r.shak_alloc.bytes,
        );
    }
}

/// Emit a JSON array of per-position records (one object per position).
fn emit_json(m: &[Measured]) {
    println!("--- json ---");
    println!("[");
    for (i, r) in m.iter().enumerate() {
        let comma = if i + 1 < m.len() { "," } else { "" };
        println!(
            "  {{\"variant\":\"{}\",\"position\":\"{}\",\"depth\":{},\"nodes\":{},\
\"matched\":{},\"mce_mnps\":{:.3},\"shak_mnps\":{:.3},\"ratio\":{:.4},\
\"mce_median_s\":{:.9},\"shak_median_s\":{:.9},\"mce_min_s\":{:.9},\"shak_min_s\":{:.9},\
\"mce_cv\":{:.5},\"shak_cv\":{:.5},\"mce_allocs\":{},\"mce_bytes\":{},\
\"shak_allocs\":{},\"shak_bytes\":{}}}{}",
            r.variant,
            r.position,
            r.depth,
            r.nodes,
            r.matched,
            r.mce_mnps(),
            r.shak_mnps(),
            r.ratio(),
            r.mce.median_s,
            r.shak.median_s,
            r.mce.min_s,
            r.shak.min_s,
            r.mce.cv(),
            r.shak.cv(),
            r.mce_alloc.count,
            r.mce_alloc.bytes,
            r.shak_alloc.count,
            r.shak_alloc.bytes,
            comma,
        );
    }
    println!("]");
}
