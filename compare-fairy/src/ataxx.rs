//! Ataxx differential perft + timing against Fairy-Stockfish (issue #280).
//!
//! Ataxx is **not** a chess variant — no pieces, no king, no attacks — so mcr
//! implements it in a self-contained module (`mcr::ataxx`), separate from the
//! chess engine, rather than on the `AnyVariant` corpus this harness otherwise
//! drives. Like Duck it therefore has its own corpus and comparison loop here.
//! The FSF side selects the built-in `UCI_Variant ataxx` (no `variants.ini`
//! needed), sets the FEN, runs `go perft`, and the node counts are asserted
//! equal.
//!
//! ## FEN dialect
//!
//! mcr uses the **same dialect** Fairy-Stockfish does: a 4-field FEN
//! `<placement> <stm> <halfmove> <fullmove>` over the 7×7 board, `P`/`p` for the
//! two stone colours. So an Ataxx FEN is byte-identical between the two engines
//! — there is no rewrite step. The start position is `P5p/7/7/7/7/7/p5P w 0 1`.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::ataxx::Position;

use crate::uci::Engine;

/// One Ataxx corpus position (mcr == FSF dialect).
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Ataxx comparison corpus: the FSF-confirmed start position, a forced-pass
/// wall (White's lone stone is sealed in while Black moves on), a flip- and
/// jump-heavy open middlegame, a symmetric cross, and a dense checkered top.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "P5p/7/7/7/7/7/p5P w 0 1",
        depth: 4,
    },
    Case {
        label: "pass_wall",
        fen: "Ppp4/ppp4/ppp4/7/7/7/7 w 0 1",
        depth: 4,
    },
    Case {
        label: "mid_extra",
        fen: "P5p/7/7/3P3/7/7/p5P w 0 1",
        depth: 3,
    },
    Case {
        label: "cross",
        fen: "3p3/7/7/3P3/7/7/3p3 w 0 1",
        depth: 4,
    },
    Case {
        label: "checker",
        fen: "PpPpPpP/pPpPpPp/7/7/7/7/3P3 w 0 1",
        depth: 3,
    },
];

/// A measured Ataxx comparison row.
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

/// Run the Ataxx corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all positions matched). Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Ataxx (7x7) — standalone mcr::ataxx vs FSF UCI_Variant ataxx (issue #280):");
    if !engine.has_variant("ataxx") {
        println!("skip ataxx: this FSF build does not advertise UCI_Variant ataxx.");
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
                eprintln!("skip ataxx/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "ataxx OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Ataxx positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Ataxx parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH ataxx/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Ataxx position through mcr's perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Position::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = pos.perft(depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // FSF side: the FEN is the same dialect, sent verbatim to the built-in.
    engine.set_variant("ataxx", false)?;
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

    /// The corpus FENs all parse on the standalone Ataxx module, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_ataxx.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_counts() {
        let pinned = [
            ("startpos", 4u32, 155888u64),
            ("pass_wall", 4, 1961),
            ("mid_extra", 3, 25026),
            ("cross", 4, 386878),
            ("checker", 3, 104864),
        ];
        for case in CASES {
            let pos = Position::from_fen(case.fen).expect("corpus FEN parses");
            let (_, depth, want) = pinned
                .iter()
                .find(|(l, _, _)| *l == case.label)
                .copied()
                .expect("a pinned count for the case");
            assert_eq!(pos.perft(depth), want, "{} perft {depth}", case.label);
        }
    }
}
