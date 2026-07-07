//! Legan chess differential perft + timing against Fairy-Stockfish.
//!
//! Legan runs on mcr's **generic** engine (`mcr::geometry::Legan`, a
//! `GenericPosition<Chess8x8, LeganRules>`), not the concrete `AnyVariant` layer the
//! rest of this harness drives, so it has its own small corpus and comparison loop
//! here (mirroring `berolina.rs`). The FSF side selects `UCI_Variant legan` (a
//! built-in), sets the FEN, runs `go perft`, and the node counts are asserted equal.
//!
//! mcr and FSF spell Legan chess with the **identical** standard-chess letters (the
//! directional pawn stays `p`/`P` — the diagonal move / two-orthogonal capture is a
//! *rule*, not a letter), so the FEN is passed through unchanged. Legan has no double
//! step and no en passant, so the FEN ep field is always `-`.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no GPL
//! code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Legan};

use crate::uci::Engine;

/// One Legan corpus position. mcr and FSF share the FEN dialect, so the same string
/// feeds both engines.
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Legan comparison corpus: the FSF-confirmed startpos, and two midgames (one
/// per side) exercising the diagonal quiet advance, both orthogonal captures, and a
/// promotion in the L-shaped corner region. Depths are modest by default; `full`
/// adds a ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "knbrp3/bqpp4/npp5/rp1p3P/p3P1PR/5PPN/4PPQB/3PRBNK w - - 0 1",
        depth: 5,
    },
    Case {
        label: "midgame-white",
        fen: "7k/1P6/8/3n4/2nP4/8/8/7K w - - 0 1",
        depth: 4,
    },
    Case {
        label: "midgame-black",
        fen: "7k/8/8/8/4pP2/3PP3/6p1/K7 b - - 0 1",
        depth: 4,
    },
];

/// A measured Legan comparison row.
struct Row {
    label: &'static str,
    fen: &'static str,
    depth: u32,
    mcr_nodes: u64,
    fsf_nodes: u64,
    matched: bool,
    mcr_secs: f64,
    fsf_secs: f64,
}

impl Row {
    fn mcr_mnps(&self) -> f64 {
        if self.mcr_secs > 0.0 {
            self.mcr_nodes as f64 / self.mcr_secs / 1e6
        } else {
            f64::INFINITY
        }
    }
    fn fsf_mnps(&self) -> f64 {
        if self.fsf_secs > 0.0 {
            self.fsf_nodes as f64 / self.fsf_secs / 1e6
        } else {
            f64::INFINITY
        }
    }
    fn speedup(&self) -> f64 {
        if self.mcr_secs > 0.0 {
            self.fsf_secs / self.mcr_secs
        } else {
            f64::NAN
        }
    }
}

/// Run the Legan corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all positions matched). Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Legan chess — generic engine vs FSF UCI_Variant legan:");
    let head = format!(
        "{:<14} {:>5} {:>14} {:>14} {:>9} {:>10} {:>10} {:>8}",
        "position", "depth", "mcr nodes", "fsf nodes", "match", "mcr Mn/s", "fsf Mn/s", "mcr/fsf",
    );
    println!("{head}");
    println!("{}", "-".repeat(head.len()));

    let mut rows: Vec<Row> = Vec::with_capacity(CASES.len());
    let mut mismatches = 0usize;

    for case in CASES {
        let depth = if full { case.depth + 1 } else { case.depth };
        match run_case(engine, case, depth) {
            Ok(row) => {
                if !row.matched {
                    mismatches += 1;
                }
                println!(
                    "{:<14} {:>5} {:>14} {:>14} {:>9} {:>10.1} {:>10.1} {:>7.2}x",
                    row.label,
                    row.depth,
                    row.mcr_nodes,
                    row.fsf_nodes,
                    if row.matched { "ok" } else { "MISMATCH" },
                    row.mcr_mnps(),
                    row.fsf_mnps(),
                    row.speedup(),
                );
                rows.push(row);
            }
            Err(e) => {
                eprintln!("skip legan/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "legan OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Legan positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Legan parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH legan/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Legan position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Legan::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // FSF side: legan is a FSF built-in; mcr and FSF share the FEN dialect.
    engine.set_variant("legan", false)?;
    engine.set_position(case.fen)?;
    let fsf = engine.go_perft(depth, false)?;

    Ok(Row {
        label: case.label,
        fen: case.fen,
        depth,
        mcr_nodes,
        fsf_nodes: fsf.nodes,
        matched: mcr_nodes == fsf.nodes,
        mcr_secs,
        fsf_secs: fsf.elapsed.as_secs_f64(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The corpus FENs all parse on the generic Legan engine, and the pinned
    /// depth-3 counts match the FSF-confirmed numbers (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 724u64),
            ("midgame-white", 1376),
            ("midgame-black", 604),
        ];
        for case in CASES {
            let pos = Legan::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Chess8x8, _, _>(&pos, 3);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-3 count for the case");
            assert_eq!(n, want, "{} depth-3 perft", case.label);
        }
    }
}
