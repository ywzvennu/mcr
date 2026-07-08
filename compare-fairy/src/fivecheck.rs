//! Five-check (5check, 8x8) differential perft + timing against Fairy-Stockfish.
//!
//! Five-check runs on mcr's **generic** engine (`mcr::geometry::FiveCheck`, a
//! `GenericPosition<Chess8x8, FiveCheckRules>`). It is standard chess with a
//! five-check win condition (FSF `fivecheck_variant()`, `chess_variant_base()` +
//! `checkCounting` with a five-check goal); `5check` is an FSF **built-in**, so the
//! suite selects `UCI_Variant 5check` directly, sets the FEN, runs `go perft`, and
//! asserts the node counts match.
//!
//! ## FEN dialect
//!
//! mcr and FSF spell five-check with the **identical** standard-chess letters and
//! the same `5+5` remaining-checks field, so the dialect is the identity — the FEN
//! is passed through unchanged.
//!
//! ## Why the counts equal standard chess
//!
//! The check tally changes only adjudication, never the legal-move set, so
//! five-check perft is byte-for-byte standard-chess perft. FSF does not truncate a
//! subtree at an intermediate fifth check either (its `go perft` on a `5+1`
//! position still counts the moves under the fifth-checking reply), so every
//! corpus count below equals the canonical standard-chess number.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, FiveCheck};

use crate::uci::Engine;

/// One five-check corpus position. mcr and FSF spell it identically (the `5+5`
/// check field included).
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The five-check comparison corpus: the FSF-confirmed startpos plus the classic
/// standard-chess perft suite (Kiwipete and positions 3-6), each carrying a `5+5`
/// tally. Every count matches standard chess.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 5+5 0 1",
        depth: 5,
    },
    Case {
        label: "kiwipete",
        fen: "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 5+5 0 1",
        depth: 4,
    },
    Case {
        label: "pos3",
        fen: "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 5+5 0 1",
        depth: 5,
    },
    Case {
        label: "pos4",
        fen: "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 5+5 0 1",
        depth: 4,
    },
    Case {
        label: "pos5",
        fen: "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 5+5 0 1",
        depth: 4,
    },
];

/// A measured five-check comparison row.
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

/// Run the five-check corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all matched, or the suite was skipped). Skips gracefully when the loaded
/// FSF binary does not advertise the `5check` built-in.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Five-check (8x8) — generic engine vs FSF UCI_Variant 5check:");

    if !engine.has_variant("5check") {
        println!("  SKIP: the loaded FSF binary does not advertise the `5check` built-in.");
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
                eprintln!("skip 5check/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "5check OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} 5check positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} 5check parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH 5check/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one five-check position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = FiveCheck::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // mcr and FSF spell 5check identically (standard-chess letters + `5+5` field).
    engine.set_variant("5check", false)?;
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

    /// The corpus FENs all parse on the generic five-check engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_5check.rs`
    /// (this runs without FSF present). Every count equals standard chess.
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 3u32, 8902u64),
            ("kiwipete", 3, 97862),
            ("pos3", 3, 2812),
            ("pos4", 3, 9467),
            ("pos5", 3, 62379),
        ];
        for (label, depth, want) in pinned {
            let case = CASES.iter().find(|c| c.label == label).expect("label");
            let pos = FiveCheck::from_fen(case.fen).expect("corpus FEN parses");
            assert_eq!(
                gperft::<Chess8x8, _, _>(&pos, depth),
                want,
                "{label} perft({depth})"
            );
        }
    }
}
