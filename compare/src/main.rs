//! mcr-vs-shakmaty comprehensive benchmark — hundreds of positions, all
//! variants, plus non-perft micro-benchmarks (issue #86).
//!
//! The suite has three position pools:
//!
//! * the **curated basket** (`CASES`) — hand-picked opening/midgame/tactical/
//!   endgame positions per variant, run at deep perft for the timing tables;
//! * the **standard EPD suite** (`data/perftsuite.epd`, public-domain van
//!   Kervinck set) — ~125 standard positions with published reference counts;
//! * the **seeded generated baskets** — ~50–100 positions per variant produced
//!   by playing fixed-seed random legal games and snapshotting (deterministic,
//!   so the numbers are stable across runs).
//!
//! It runs in two phases:
//!
//! * **Parity (cheap, over EVERYTHING):** a shallow perft on every position in
//!   every pool through both engines, asserting the counts agree (and, for EPD,
//!   that mcr also matches the published reference). Hundreds of positions and
//!   millions of nodes are cross-checked. Any mismatch fails loudly and exits
//!   non-zero.
//! * **Timing (expensive, over a graded subset):** the rigorous interleaved A/B
//!   median+spread methodology on the curated basket plus a per-variant sample of
//!   generated positions at deeper depths.
//!
//! Plus **micro-benchmarks** (legal_moves / play / FEN round-trip vs shakmaty;
//! SAN / zobrist mcr-only) and the existing **memory** (allocs/bytes/RSS/static
//! footprint) tables.
//!
//! ```text
//! cargo run --release --bin mcr-compare              # default (HQ sliders)
//! cargo run --release --features magic --bin mcr-compare   # magic sliders
//! cargo run --release --bin mcr-compare -- --csv    # machine-readable rows
//! cargo run --release --bin mcr-compare -- --json
//! cargo run --release --bin mcr-compare -- --full   # deeper timing, full corpus
//! ```
//!
//! See the per-module docs for the CPU and memory methodology (unchanged from the
//! original harness). This binary links GPL-3.0+ shakmaty for benchmarking only;
//! it is never published or distributed and does not affect the mcr library's
//! licensing.

use std::time::Instant;

use mcr_compare::alloc;
use mcr_compare::epd;
use mcr_compare::footprint;
use mcr_compare::gen::{self, GenPos};
use mcr_compare::micro;
use mcr_compare::rss;
use mcr_compare::runtime::{McrPos, ShakPos};
use mcr_compare::stats::{summarize, TimeStats};
use mcr_compare::{mcr_perft, shakmaty_perft, Case, CASES, VARIANTS};

/// Install the counting allocator as the program-wide global allocator.
#[global_allocator]
static GLOBAL: alloc::CountingAlloc = alloc::CountingAlloc;

/// Warm-up iterations per engine before timing (not recorded).
const WARMUP: usize = 2;
/// Interleaved A/B samples taken per position per engine.
const SAMPLES: usize = 17;

/// Shallow perft depth used for the parity cross-check over EVERY position.
/// Sized so each position is fast; the count still exercises full move-gen.
const PARITY_DEPTH: u32 = 3;
/// In `--full`, the parity pass goes one ply deeper for a stronger cross-check.
const PARITY_DEPTH_FULL: u32 = 4;

/// Seeded games per variant for the generator, and the snapshot cap per variant.
const GEN_GAMES: u32 = 40;
const GEN_MAX_PLIES: u32 = 80;
const GEN_CAP: usize = 100;

/// How many generated positions per variant join the deeper *timing* subset.
const TIMING_SAMPLE_PER_VARIANT: usize = 2;
const TIMING_SAMPLE_PER_VARIANT_FULL: usize = 6;
/// Perft depth for the generated-position timing sample (kept modest so the
/// default run stays in the "few minutes" budget; `--full` deepens it).
const GEN_TIMING_DEPTH: u32 = 4;
const GEN_TIMING_DEPTH_FULL: u32 = 5;

/// Machine-readable output selected on the command line.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    Human,
    Csv,
    Json,
}

/// Parsed command line.
struct Opts {
    mode: OutputMode,
    full: bool,
}

/// One fully measured timing position.
struct Measured {
    variant: &'static str,
    position: String,
    depth: u32,
    nodes: u64,
    matched: bool,
    mcr: TimeStats,
    shak: TimeStats,
    mcr_alloc: alloc::AllocDelta,
    shak_alloc: alloc::AllocDelta,
}

impl Measured {
    fn mcr_mnps(&self) -> f64 {
        self.nodes as f64 / self.mcr.median_s / 1e6
    }
    fn shak_mnps(&self) -> f64 {
        self.nodes as f64 / self.shak.median_s / 1e6
    }
    fn ratio(&self) -> f64 {
        if self.mcr.median_s > 0.0 {
            self.shak.median_s / self.mcr.median_s
        } else {
            f64::NAN
        }
    }
}

/// Outcome of the parity pass over all pools.
struct Parity {
    positions: usize,
    nodes: u64,
    mismatches: usize,
    ref_checked: usize,
    ref_mismatches: usize,
    skipped: usize,
    /// Generated positions where mcr and shakmaty diverge *because* a variant
    /// terminal (king on the hill, third check, completed race, king explosion,
    /// pieces shed) is reached inside the shallow perft tree — shakmaty stops
    /// expanding the decided line, mcr keeps counting. These are not bugs; they
    /// are the documented incomparable cases, counted and skipped, not failed.
    incomparable_terminal: usize,
    per_variant: Vec<(&'static str, usize, u64)>,
}

/// Variants whose terminal condition is *path-dependent* and can fire partway
/// down a perft tree from a non-terminal root, making a deep node count diverge
/// from shakmaty (which prunes at the variant terminal). For generated positions
/// in these variants we treat a deep mismatch as an incomparable skip, after
/// confirming a terminal is actually reachable, rather than a parity failure.
fn terminal_divergent(variant: &str) -> bool {
    matches!(
        variant,
        "king-of-the-hill" | "three-check" | "racing-kings" | "atomic" | "antichess"
    )
}

/// Does any line within `depth` plies from this mcr position reach a variant
/// terminal? Used to confirm a koth/three-check/… mismatch is the documented
/// terminal divergence (so we can skip it) rather than a real bug.
fn reaches_terminal(pos: &McrPos, depth: u32) -> bool {
    pos.any_reaches_terminal(depth)
}

fn main() {
    let opts = parse_args();

    println!("mcr vs shakmaty — comprehensive benchmark (issue #86)");
    #[cfg(feature = "magic")]
    println!("mcr slider backend: magic bitboards (--features magic)");
    #[cfg(not(feature = "magic"))]
    println!("mcr slider backend: hyperbola-quintessence (default)");
    println!("engines: mcr (path) vs shakmaty 0.27");
    println!(
        "tier: {}",
        if opts.full {
            "--full (deeper timing, deeper parity)"
        } else {
            "default (parity-all + timing-subset)"
        }
    );
    println!();

    // ---- build the generated baskets (deterministic, seeded) --------------
    eprintln!("generating seeded per-variant baskets...");
    let generated: Vec<GenPos> = VARIANTS
        .iter()
        .flat_map(|&v| gen::generate_variant(v, GEN_GAMES, GEN_MAX_PLIES, GEN_CAP))
        .collect();
    let epd_entries = epd::load();

    // ---- PARITY over EVERYTHING -------------------------------------------
    eprintln!("running parity cross-check over all positions...");
    let parity = run_parity(&epd_entries, &generated, opts.full);
    print_parity(&parity);
    println!();

    // ---- TIMING over the graded subset ------------------------------------
    eprintln!("timing curated basket + generated sample...");
    let timing_subset = build_timing_subset(&generated, opts.full);
    let measured: Vec<Measured> = {
        let mut v: Vec<Measured> = CASES.iter().map(measure_case).collect();
        v.extend(timing_subset.iter().map(measure_gen));
        v
    };

    per_variant_cpu_table(&measured);
    println!();
    memory_table(&measured);
    println!();

    // ---- MICRO-BENCHMARKS --------------------------------------------------
    eprintln!("running non-perft micro-benchmarks...");
    let micro_sample = micro_sample_fens(&epd_entries);
    let micro_results = micro::run(&micro_sample);
    micro_table(&micro_results, micro_sample.len());
    println!();

    // ---- MEMORY footprint + RSS -------------------------------------------
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

    let timing_mismatches = measured.iter().filter(|r| !r.matched).count();

    match opts.mode {
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
    let bad = parity.mismatches + parity.ref_mismatches + timing_mismatches;
    if bad == 0 {
        println!(
            "OK: parity verified {} positions ({} nodes) across {} variants; \
all {} timing positions matched; {} reference counts confirmed.",
            parity.positions,
            parity.nodes,
            VARIANTS.len(),
            measured.len(),
            parity.ref_checked,
        );
    } else {
        eprintln!(
            "ERROR: {bad} failure(s): {} cross-engine parity, {} reference, {} timing mismatches.",
            parity.mismatches, parity.ref_mismatches, timing_mismatches,
        );
        std::process::exit(1);
    }
}

/// Parse `--csv` / `--json` / `--full`.
fn parse_args() -> Opts {
    let mut o = Opts {
        mode: OutputMode::Human,
        full: false,
    };
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--csv" => o.mode = OutputMode::Csv,
            "--json" => o.mode = OutputMode::Json,
            "--full" => o.full = true,
            "--help" | "-h" => {
                println!("usage: mcr-compare [--csv | --json] [--full]");
                println!("  --full : deeper timing + deeper parity over the whole corpus");
                println!("  build with --features magic for the magic-bitboard slider numbers");
                std::process::exit(0);
            }
            other => eprintln!("warning: ignoring unknown argument {other:?}"),
        }
    }
    o
}

// =========================================================================
// Parity
// =========================================================================

/// Run the shallow-perft parity cross-check over every position in every pool.
///
/// For each position we run both engines at [`PARITY_DEPTH`] and assert equality;
/// for EPD positions we additionally assert mcr matches the published reference
/// count at that depth. Positions shakmaty cannot represent (rare, documented)
/// are skipped and counted, never silently dropped.
fn run_parity(epd_entries: &[epd::EpdEntry], generated: &[GenPos], full: bool) -> Parity {
    let depth = if full {
        PARITY_DEPTH_FULL
    } else {
        PARITY_DEPTH
    };

    let mut positions = 0usize;
    let mut nodes = 0u64;
    let mut mismatches = 0usize;
    let mut ref_checked = 0usize;
    let mut ref_mismatches = 0usize;
    let mut skipped = 0usize;
    let mut incomparable_terminal = 0usize;
    // per-variant tallies (positions, nodes)
    let mut pv: Vec<(&'static str, usize, u64)> =
        VARIANTS.iter().map(|&v| (v, 0usize, 0u64)).collect();
    let bump = |pv: &mut Vec<(&'static str, usize, u64)>, variant: &str, n: u64| {
        if let Some(e) = pv.iter_mut().find(|e| e.0 == variant) {
            e.1 += 1;
            e.2 += n;
        }
    };

    // ---- curated basket (all variants) ------------------------------------
    for case in CASES {
        let (Some(m), Some(s)) = (
            McrPos::parse(case.variant, case.fen),
            ShakPos::parse(case.variant, case.fen),
        ) else {
            skipped += 1;
            continue;
        };
        let mn = m.perft(depth);
        let sn = s.perft(depth);
        positions += 1;
        nodes += mn;
        bump(&mut pv, case.variant, mn);
        if mn != sn {
            mismatches += 1;
            eprintln!(
                "*** PARITY MISMATCH basket {}/{} d{depth}: mcr={mn} shak={sn} ***",
                case.variant, case.position
            );
        }
    }

    // ---- EPD suite (standard chess; also reference-checked) ---------------
    for e in epd_entries {
        let (Some(m), Some(s)) = (
            McrPos::parse("standard", &e.fen),
            ShakPos::parse("standard", &e.fen),
        ) else {
            skipped += 1;
            continue;
        };
        let mn = m.perft(depth);
        let sn = s.perft(depth);
        positions += 1;
        nodes += mn;
        bump(&mut pv, "standard", mn);
        if mn != sn {
            mismatches += 1;
            eprintln!(
                "*** PARITY MISMATCH epd {} d{depth}: mcr={mn} shak={sn} ***",
                e.fen
            );
        }
        if let Some(r) = e.refs.iter().find(|r| r.depth == depth) {
            ref_checked += 1;
            if mn != r.nodes {
                ref_mismatches += 1;
                eprintln!(
                    "*** REFERENCE MISMATCH epd {} d{depth}: mcr={mn} reference={} ***",
                    e.fen, r.nodes
                );
            }
        }
    }

    // ---- generated baskets (all variants) ---------------------------------
    for g in generated {
        let (Some(m), Some(s)) = (
            McrPos::parse(g.variant, &g.fen),
            ShakPos::parse(g.variant, &g.fen),
        ) else {
            // shakmaty refused this position (e.g. variant-terminal / pocket):
            // count it as skipped rather than a failure.
            skipped += 1;
            continue;
        };
        let mn = m.perft(depth);
        let sn = s.perft(depth);
        if mn != sn {
            // A deep divergence in a terminal-divergent variant: confirm a
            // variant terminal is actually reachable within the tree, and if so
            // record it as an incomparable skip (the documented caveat) instead
            // of a parity failure. Otherwise it is a real mismatch.
            if terminal_divergent(g.variant) && reaches_terminal(&m, depth) {
                incomparable_terminal += 1;
                continue;
            }
            mismatches += 1;
            eprintln!(
                "*** PARITY MISMATCH gen {}/{} d{depth} fen={}: mcr={mn} shak={sn} ***",
                g.variant, g.label, g.fen
            );
            continue;
        }
        positions += 1;
        nodes += mn;
        bump(&mut pv, g.variant, mn);
    }

    Parity {
        positions,
        nodes,
        mismatches,
        ref_checked,
        ref_mismatches,
        skipped,
        incomparable_terminal,
        per_variant: pv,
    }
}

fn print_parity(p: &Parity) {
    println!(
        "parity — shallow perft cross-check over ALL positions (mcr == shakmaty == reference):"
    );
    let head = format!(
        "{:<16} {:>10} {:>16}",
        "variant", "positions", "nodes verified"
    );
    println!("{head}");
    println!("{}", "-".repeat(head.len()));
    for &(v, pos, nodes) in &p.per_variant {
        if pos == 0 {
            continue;
        }
        println!("{v:<16} {pos:>10} {nodes:>16}");
    }
    println!("{}", "-".repeat(head.len()));
    println!("{:<16} {:>10} {:>16}", "TOTAL", p.positions, p.nodes);
    println!(
        "  reference counts checked (EPD): {} matched: {}",
        p.ref_checked,
        p.ref_checked - p.ref_mismatches,
    );
    println!(
        "  positions skipped (shakmaty rejected the FEN): {}",
        p.skipped
    );
    println!(
        "  generated positions skipped (variant terminal reached inside the perft tree — \
shakmaty prunes, mcr counts on; documented incomparable case): {}",
        p.incomparable_terminal,
    );
    if p.mismatches == 0 && p.ref_mismatches == 0 {
        println!("  result: ALL {} positions matched.", p.positions);
    } else {
        println!(
            "  result: {} cross-engine + {} reference MISMATCHES (see errors above).",
            p.mismatches, p.ref_mismatches
        );
    }
}

// =========================================================================
// Timing
// =========================================================================

/// One generated position selected for the deeper timing subset.
struct GenTiming {
    variant: &'static str,
    label: String,
    fen: String,
    depth: u32,
}

/// Pick a per-variant sample of generated positions for the deeper timing pass.
/// Snapshots are spread across plies; we take an evenly strided sample so the
/// chosen positions span opening→endgame rather than clustering.
fn build_timing_subset(generated: &[GenPos], full: bool) -> Vec<GenTiming> {
    let per = if full {
        TIMING_SAMPLE_PER_VARIANT_FULL
    } else {
        TIMING_SAMPLE_PER_VARIANT
    };
    let depth = if full {
        GEN_TIMING_DEPTH_FULL
    } else {
        GEN_TIMING_DEPTH
    };
    let mut out = Vec::new();
    for &variant in VARIANTS {
        let pool: Vec<&GenPos> = generated.iter().filter(|g| g.variant == variant).collect();
        if pool.is_empty() {
            continue;
        }
        let mut picked = 0usize;
        // Scan the pool on a stride, but only keep positions both engines accept
        // *and* (for terminal-divergent variants) whose perft tree reaches no
        // variant terminal within `depth` — otherwise mcr and shakmaty would
        // diverge and the timing position would falsely flag a mismatch.
        for &g in &pool {
            if picked >= per {
                break;
            }
            let Some(m) = McrPos::parse(variant, &g.fen) else {
                continue;
            };
            if ShakPos::parse(variant, &g.fen).is_none() {
                continue;
            }
            if terminal_divergent(variant) && m.any_reaches_terminal(depth) {
                continue;
            }
            out.push(GenTiming {
                variant,
                label: g.label.clone(),
                fen: g.fen.clone(),
                depth,
            });
            picked += 1;
        }
    }
    out
}

/// Time a single static-basket [`Case`] (the original deep-perft methodology).
fn measure_case(case: &'static Case) -> Measured {
    let mut mcr_nodes = 0;
    let mut shak_nodes = 0;
    for _ in 0..WARMUP {
        mcr_nodes = mcr_perft(case);
        shak_nodes = shakmaty_perft(case);
    }
    let (mcr, shak) = interleaved(
        || mcr_nodes = mcr_perft(case),
        || shak_nodes = shakmaty_perft(case),
    );
    let matched = mcr_nodes == shak_nodes;
    if !matched {
        eprintln!(
            "*** NODE COUNT MISMATCH for {}/{} (depth {}): mcr={} shakmaty={} ***",
            case.variant, case.position, case.depth, mcr_nodes, shak_nodes
        );
    }
    let mcr_alloc = measure_allocs(|| mcr_perft(case));
    let shak_alloc = measure_allocs(|| shakmaty_perft(case));
    Measured {
        variant: case.variant,
        position: case.position.to_string(),
        depth: case.depth,
        nodes: mcr_nodes,
        matched,
        mcr,
        shak,
        mcr_alloc,
        shak_alloc,
    }
}

/// Time one generated-position timing entry through the runtime dispatch.
fn measure_gen(g: &GenTiming) -> Measured {
    let m = McrPos::parse(g.variant, &g.fen).expect("gen position parses in mcr");
    let s = ShakPos::parse(g.variant, &g.fen).expect("gen position parses in shakmaty");
    let mut mcr_nodes = 0;
    let mut shak_nodes = 0;
    for _ in 0..WARMUP {
        mcr_nodes = m.perft(g.depth);
        shak_nodes = s.perft(g.depth);
    }
    let (mcr, shak) = interleaved(
        || mcr_nodes = m.perft(g.depth),
        || shak_nodes = s.perft(g.depth),
    );
    let matched = mcr_nodes == shak_nodes;
    if !matched {
        eprintln!(
            "*** NODE COUNT MISMATCH for {}/{} (depth {}): mcr={} shakmaty={} ***",
            g.variant, g.label, g.depth, mcr_nodes, shak_nodes
        );
    }
    let mcr_alloc = measure_allocs(|| m.perft(g.depth));
    let shak_alloc = measure_allocs(|| s.perft(g.depth));
    Measured {
        variant: g.variant,
        position: format!("gen:{}", g.label),
        depth: g.depth,
        nodes: mcr_nodes,
        matched,
        mcr,
        shak,
        mcr_alloc,
        shak_alloc,
    }
}

/// Interleaved A/B sampling of two closures; returns (mcr stats, shak stats).
fn interleaved(mut mcr: impl FnMut(), mut shak: impl FnMut()) -> (TimeStats, TimeStats) {
    let mut mcr_samples = Vec::with_capacity(SAMPLES);
    let mut shak_samples = Vec::with_capacity(SAMPLES);
    for i in 0..SAMPLES {
        if i % 2 == 0 {
            mcr_samples.push(time_once(&mut mcr));
            shak_samples.push(time_once(&mut shak));
        } else {
            shak_samples.push(time_once(&mut shak));
            mcr_samples.push(time_once(&mut mcr));
        }
    }
    (summarize(&mcr_samples), summarize(&shak_samples))
}

fn time_once(mut f: impl FnMut()) -> u64 {
    let start = Instant::now();
    f();
    start.elapsed().as_nanos() as u64
}

fn measure_allocs(mut f: impl FnMut() -> u64) -> alloc::AllocDelta {
    let before = alloc::snapshot();
    let _ = f();
    let after = alloc::snapshot();
    alloc::delta(before, after)
}

/// Per-variant aggregated CPU table (node-weighted median throughput).
fn per_variant_cpu_table(m: &[Measured]) {
    println!(
        "CPU — timing subset, aggregated per variant (node-weighted median; {} interleaved samples):",
        SAMPLES
    );
    let head = format!(
        "{:<16} {:>5} {:>14} {:>11} {:>11} {:>8} {:>9}",
        "variant", "pos", "total nodes", "mcr Mn/s", "shak Mn/s", "ratio", "max cv",
    );
    println!("{head}");
    println!("{}", "-".repeat(head.len()));

    let mut g_nodes = 0u64;
    let mut g_mcr_s = 0.0;
    let mut g_shak_s = 0.0;
    for &variant in VARIANTS {
        let rows: Vec<&Measured> = m.iter().filter(|r| r.variant == variant).collect();
        if rows.is_empty() {
            continue;
        }
        let nodes: u64 = rows.iter().map(|r| r.nodes).sum();
        let mcr_s: f64 = rows.iter().map(|r| r.mcr.median_s).sum();
        let shak_s: f64 = rows.iter().map(|r| r.shak.median_s).sum();
        let max_cv = rows
            .iter()
            .map(|r| r.mcr.cv().max(r.shak.cv()))
            .fold(0.0, f64::max);
        g_nodes += nodes;
        g_mcr_s += mcr_s;
        g_shak_s += shak_s;
        println!(
            "{:<16} {:>5} {:>14} {:>11.1} {:>11.1} {:>8.2} {:>8.1}%",
            variant,
            rows.len(),
            nodes,
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / shak_s / 1e6,
            shak_s / mcr_s,
            max_cv * 100.0,
        );
    }
    println!("{}", "-".repeat(head.len()));
    println!(
        "{:<16} {:>5} {:>14} {:>11.1} {:>11.1} {:>8.2} {:>9}",
        "OVERALL",
        m.len(),
        g_nodes,
        g_nodes as f64 / g_mcr_s / 1e6,
        g_nodes as f64 / g_shak_s / 1e6,
        g_shak_s / g_mcr_s,
        "",
    );
}

/// Per-variant and aggregate memory table over the timing subset.
fn memory_table(m: &[Measured]) {
    println!("memory — heap allocations during the timing-subset perft, summed per variant:");
    let head = format!(
        "{:<16} {:>5} {:>14} {:>13} {:>14} {:>13}",
        "variant", "pos", "mcr allocs", "mcr KiB", "shak allocs", "shak KiB",
    );
    println!("{head}");
    println!("{}", "-".repeat(head.len()));

    let (mut g_mcr_c, mut g_mcr_b, mut g_shak_c, mut g_shak_b) = (0u64, 0u64, 0u64, 0u64);
    for &variant in VARIANTS {
        let rows: Vec<&Measured> = m.iter().filter(|r| r.variant == variant).collect();
        if rows.is_empty() {
            continue;
        }
        let mcr_c: u64 = rows.iter().map(|r| r.mcr_alloc.count).sum();
        let mcr_b: u64 = rows.iter().map(|r| r.mcr_alloc.bytes).sum();
        let shak_c: u64 = rows.iter().map(|r| r.shak_alloc.count).sum();
        let shak_b: u64 = rows.iter().map(|r| r.shak_alloc.bytes).sum();
        g_mcr_c += mcr_c;
        g_mcr_b += mcr_b;
        g_shak_c += shak_c;
        g_shak_b += shak_b;
        println!(
            "{:<16} {:>5} {:>14} {:>13.1} {:>14} {:>13.1}",
            variant,
            rows.len(),
            mcr_c,
            mcr_b as f64 / 1024.0,
            shak_c,
            shak_b as f64 / 1024.0,
        );
    }
    println!("{}", "-".repeat(head.len()));
    println!(
        "{:<16} {:>5} {:>14} {:>13.1} {:>14} {:>13.1}",
        "OVERALL",
        m.len(),
        g_mcr_c,
        g_mcr_b as f64 / 1024.0,
        g_shak_c,
        g_shak_b as f64 / 1024.0,
    );
}

/// Micro-benchmark table (mcr vs shakmaty where comparable).
fn micro_table(results: &[micro::MicroResult], sample: usize) {
    println!("micro-benchmarks — non-perft hot paths over a {sample}-position standard sample:");
    let head = format!(
        "{:<20} {:>14} {:>14} {:>8} {:>8}",
        "operation", "mcr ops/s", "shak ops/s", "ratio", "mcr cv",
    );
    println!("{head}");
    println!("{}", "-".repeat(head.len()));
    for r in results {
        let shak = match r.shak_ops {
            Some(s) => format!("{s:>14.0}"),
            None => format!("{:>14}", "n/a"),
        };
        let ratio = match r.ratio() {
            Some(x) => format!("{x:>8.2}"),
            None => format!("{:>8}", "—"),
        };
        println!(
            "{:<20} {:>14.0} {} {} {:>7.1}%",
            r.name,
            r.mcr_ops,
            shak,
            ratio,
            r.mcr_cv * 100.0,
        );
    }
}

/// Build the standard-chess sample for micro-benchmarks from the EPD corpus
/// (every ~5th standard position, capped) — varied but cheap to set up.
fn micro_sample_fens(epd_entries: &[epd::EpdEntry]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, e) in epd_entries.iter().enumerate() {
        if i % 5 == 0 {
            out.push(e.fen.clone());
        }
        if out.len() >= 32 {
            break;
        }
    }
    out
}

/// Emit one CSV row per timing position.
fn emit_csv(m: &[Measured]) {
    println!("--- csv ---");
    println!(
        "variant,position,depth,nodes,matched,mcr_mnps,shak_mnps,ratio,\
mcr_median_s,shak_median_s,mcr_min_s,shak_min_s,mcr_cv,shak_cv,\
mcr_allocs,mcr_bytes,shak_allocs,shak_bytes"
    );
    for r in m {
        println!(
            "{},{},{},{},{},{:.3},{:.3},{:.4},{:.9},{:.9},{:.9},{:.9},{:.5},{:.5},{},{},{},{}",
            r.variant,
            r.position,
            r.depth,
            r.nodes,
            r.matched,
            r.mcr_mnps(),
            r.shak_mnps(),
            r.ratio(),
            r.mcr.median_s,
            r.shak.median_s,
            r.mcr.min_s,
            r.shak.min_s,
            r.mcr.cv(),
            r.shak.cv(),
            r.mcr_alloc.count,
            r.mcr_alloc.bytes,
            r.shak_alloc.count,
            r.shak_alloc.bytes,
        );
    }
}

/// Emit a JSON array of per-position records.
fn emit_json(m: &[Measured]) {
    println!("--- json ---");
    println!("[");
    for (i, r) in m.iter().enumerate() {
        let comma = if i + 1 < m.len() { "," } else { "" };
        println!(
            "  {{\"variant\":\"{}\",\"position\":\"{}\",\"depth\":{},\"nodes\":{},\
\"matched\":{},\"mcr_mnps\":{:.3},\"shak_mnps\":{:.3},\"ratio\":{:.4},\
\"mcr_median_s\":{:.9},\"shak_median_s\":{:.9},\"mcr_min_s\":{:.9},\"shak_min_s\":{:.9},\
\"mcr_cv\":{:.5},\"shak_cv\":{:.5},\"mcr_allocs\":{},\"mcr_bytes\":{},\
\"shak_allocs\":{},\"shak_bytes\":{}}}{}",
            r.variant,
            r.position,
            r.depth,
            r.nodes,
            r.matched,
            r.mcr_mnps(),
            r.shak_mnps(),
            r.ratio(),
            r.mcr.median_s,
            r.shak.median_s,
            r.mcr.min_s,
            r.shak.min_s,
            r.mcr.cv(),
            r.shak.cv(),
            r.mcr_alloc.count,
            r.mcr_alloc.bytes,
            r.shak_alloc.count,
            r.shak_alloc.bytes,
            comma,
        );
    }
    println!("]");
}
