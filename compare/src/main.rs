//! mce-vs-shakmaty perft benchmark — headline report.
//!
//! Runs standard chess and all eight variants through both engines on identical
//! positions, asserts the node counts match, and prints a timing table. Build
//! and run in release for meaningful numbers:
//!
//! ```text
//! cargo run --release -p mce-compare      # from the repo root
//! cargo run --release --bin mce-compare   # from inside compare/
//! ```
//!
//! This binary links GPL-3.0+ shakmaty for benchmarking only; it is never
//! published or distributed and does not affect the mce library's licensing.

use std::time::{Duration, Instant};

use mce_compare::{mce_perft, shakmaty_perft, CASES};

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

fn main() {
    println!("mce vs shakmaty — perft benchmark (release, median of {REPS} runs)");
    println!("engines: mce (path) vs shakmaty 0.27");
    println!();

    // Column header.
    let head = format!(
        "{:<17} {:>5} {:>12} {:>10} {:>11} {:>9} {:>13} {:>13}",
        "variant", "depth", "nodes", "mce ms", "shak ms", "ratio", "mce Mn/s", "shak Mn/s",
    );
    println!("{head}");
    println!("{}", "-".repeat(head.len()));

    let mut all_match = true;

    for case in CASES {
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
    }

    println!();
    if all_match {
        println!("All node counts matched between mce and shakmaty.");
    } else {
        eprintln!("ERROR: one or more node counts did NOT match — see messages above.");
        std::process::exit(1);
    }
}
