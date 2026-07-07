//! Bughouse **single-board** differential perft + timing against Fairy-Stockfish
//! (issue #279).
//!
//! Bughouse is a 2-board team game, but a single board is full-information and
//! deterministic — crazyhouse with the hand fed externally (FSF `twoBoards`) — so
//! FSF's `UCI_Variant bughouse` `go perft` is meaningful for it. mcr runs the
//! single board on its **generic** engine (`mcr::geometry::Bughouse`, a
//! `GenericPosition<Chess8x8, BughouseRules>`); this module drives FSF over the
//! byte-identical position and asserts node counts match. The one single-board
//! rule that separates Bughouse from Crazyhouse — a capture does **not** bank into
//! the captor's hand — is exercised by the `capture` case (it diverges from
//! Crazyhouse at depth 3, and matches FSF's Bughouse count).
//!
//! ## FEN dialect
//!
//! Bughouse uses only **standard chess pieces** (`K Q R B N P`), whose letters are
//! identical in mcr and FSF, and FSF accepts the crazyhouse hand bracket in any
//! order (and the empty `[]` present or omitted), so the FEN is passed to FSF
//! **unchanged**. The comparison asserts only node counts.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Bughouse, Chess8x8};

use crate::uci::Engine;

/// One Bughouse corpus position.
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Bughouse comparison corpus: the FSF-confirmed empty-hand start (identical
/// to standard chess), a midgame with a Knight + Pawn in each hand (drops live), a
/// capture position pinning the no-bank divergence from Crazyhouse, a lone-Queen
/// drop isolating the drop generator, and a full two-sided reserve.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "hand-npnp",
        fen: "r1bqk2r/ppp2ppp/2n5/3pp3/3PP3/2N5/PPP2PPP/R1BQK2R[NPnp] w KQkq - 0 1",
        depth: 3,
    },
    Case {
        label: "capture-no-bank",
        fen: "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR[] w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "qdrop",
        fen: "4k3/8/8/8/8/8/8/4K3[Q] w - - 0 1",
        depth: 3,
    },
    Case {
        label: "rich-reserve",
        fen: "r3k2r/8/8/8/8/8/8/R3K2R[QRBNPqrbnp] w KQkq - 0 1",
        depth: 2,
    },
];

/// A measured Bughouse comparison row.
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

/// Run the Bughouse corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all positions matched). Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Bughouse (single board) — generic engine vs FSF UCI_Variant bughouse (issue #279):");
    let head = format!(
        "{:<16} {:>5} {:>14} {:>14} {:>9} {:>10} {:>10} {:>8}",
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
                    "{:<16} {:>5} {:>14} {:>14} {:>9} {:>10.1} {:>10.1} {:>7.2}x",
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
                eprintln!("skip bughouse/{}: {e}", case.label);
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
            "bughouse OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Bughouse positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Bughouse parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH bughouse/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Bughouse position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mcr side: the generic single-board Bughouse position.
    let pos = Bughouse::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // FSF side: standard piece letters are shared and the hand bracket is accepted
    // verbatim, so the FEN is passed through unchanged.
    engine.set_variant("bughouse", false)?;
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

    /// The corpus FENs all parse on the generic Bughouse engine and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_bughouse.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 3u32, 8902u64),
            ("hand-npnp", 2, 9720),
            ("capture-no-bank", 3, 27226),
            ("qdrop", 2, 230),
            ("rich-reserve", 2, 78889),
        ];
        for case in CASES {
            let pos = Bughouse::from_fen(case.fen).expect("corpus FEN parses");
            let (_, depth, want) = pinned
                .iter()
                .find(|(l, _, _)| *l == case.label)
                .copied()
                .expect("a pinned count for the case");
            assert_eq!(
                gperft::<Chess8x8, _, _>(&pos, depth),
                want,
                "{} perft",
                case.label
            );
        }
    }
}
