//! Makruk (Thai chess) differential perft + timing against Fairy-Stockfish
//! (issue #168).
//!
//! Makruk runs on mcr's **generic** engine (`mcr::geometry::Makruk`, a
//! `GenericPosition<Chess8x8, MakrukRules>`), not the concrete `AnyVariant`
//! layer the rest of this harness drives, so it has its own small corpus and
//! comparison loop here. The FSF side is identical in spirit: select
//! `UCI_Variant makruk`, set the FEN, run `go perft`, assert the node counts
//! match, and report mcr-vs-FSF throughput.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Makruk};

use crate::uci::Engine;

/// One Makruk corpus position. The FEN is the same dialect FSF parses
/// (`UCI_Variant makruk`), so no rewrite is needed.
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Makruk comparison corpus: the FSF-confirmed startpos plus two midgames.
/// Depths are kept modest by default; `full` deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w - - 0 1",
        depth: 5,
    },
    Case {
        label: "midgame-1",
        fen: "rnsmksnr/8/1ppppppp/p7/4P3/PPPP1PPP/8/RNSKMSNR b - - 0 2",
        depth: 5,
    },
    Case {
        label: "midgame-2",
        fen: "r1smks1r/3n4/ppp1pppp/3p4/3P4/PPP1PPPP/4N3/R1SKMS1R w - - 0 4",
        depth: 5,
    },
];

/// A measured Makruk comparison row.
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

/// Run the Makruk corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all positions matched). Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Makruk (Thai chess) — generic engine vs FSF UCI_Variant makruk (issue #168):");
    let head = format!(
        "{:<12} {:>5} {:>14} {:>14} {:>9} {:>10} {:>10} {:>8}",
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
                    "{:<12} {:>5} {:>14} {:>14} {:>9} {:>10.1} {:>10.1} {:>7.2}x",
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
                eprintln!("skip makruk/{}: {e}", case.label);
            }
        }
    }

    // Node-weighted aggregate throughput.
    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "makruk OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Makruk positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Makruk parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH makruk/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Makruk position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mcr side: the generic Makruk position.
    let pos = Makruk::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // FSF side: makruk uses the same FEN dialect, no rewrite needed.
    engine.set_variant("makruk", false)?;
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

    /// The corpus FENs all parse on the generic Makruk engine, and the pinned
    /// depth-2 counts match the FSF-confirmed numbers in `tests/perft_makruk.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [("startpos", 529u64), ("midgame-1", 576), ("midgame-2", 508)];
        for case in CASES {
            let pos = Makruk::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Chess8x8, _, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }
}
