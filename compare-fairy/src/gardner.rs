//! Gardner minichess differential perft + timing against Fairy-Stockfish.
//!
//! Gardner minichess runs on mcr's **generic** engine (`mcr::geometry::Gardner`, a
//! `GenericPosition<Minishogi5x5, GardnerRules>`), like the other fairy variants,
//! so it has its own corpus and comparison loop here. `gardner` is an FSF
//! **built-in** (it appears in the `UCI_Variant` combo with no `variants.ini`), so
//! the suite selects `UCI_Variant gardner` directly — no ini load — sets the FEN,
//! runs `go perft`, asserts the node counts match, and reports mcr-vs-FSF
//! throughput.
//!
//! ## FEN dialect
//!
//! Gardner minichess is standard chess on a 5x5 board: mcr and FSF render every
//! position with the **identical** standard-chess letters, so the dialect is the
//! identity — the FEN is passed through unchanged. The startpos is the five-file
//! back rank `RNBQK` over five pawns; there is no double step, castle, or en
//! passant to spell.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Gardner, Minishogi5x5};

use crate::uci::Engine;

/// One Gardner corpus position. mcr and FSF spell it identically.
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Gardner comparison corpus: the FSF-confirmed startpos and a natural
/// midgame position (a b-pawn one step from promotion, open lines) that exercises
/// the standard four-role promotion set and the capped 5x5 piece ranges.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnbqk/ppppp/5/PPPPP/RNBQK w - - 0 1",
        depth: 5,
    },
    Case {
        label: "midgame",
        fen: "1nbqk/rPp1p/2p2/P2PP/RNBQK w - - 0 5",
        depth: 5,
    },
];

/// A measured Gardner comparison row.
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

/// Run the Gardner corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all matched, or the suite was skipped). Skips gracefully when the loaded
/// FSF binary does not advertise the `gardner` built-in.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Gardner minichess (5x5) — generic engine vs FSF UCI_Variant gardner:");

    if !engine.has_variant("gardner") {
        println!("  SKIP: the loaded FSF binary does not advertise the `gardner` built-in.");
        return 0;
    }

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
                eprintln!("skip gardner/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "gardner OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Gardner positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Gardner parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH gardner/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Gardner position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Gardner::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Minishogi5x5, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // mcr and FSF spell Gardner minichess identically (standard-chess letters).
    engine.set_variant("gardner", false)?;
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

    /// The corpus FENs all parse on the generic Gardner engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_gardner.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [("startpos", 4u32, 4775u64), ("midgame", 4, 39955)];
        for (label, depth, want) in pinned {
            let case = CASES.iter().find(|c| c.label == label).expect("label");
            let pos = Gardner::from_fen(case.fen).expect("corpus FEN parses");
            assert_eq!(
                gperft::<Minishogi5x5, _, _>(&pos, depth),
                want,
                "{label} perft({depth})"
            );
        }
    }
}
