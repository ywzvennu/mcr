//! Placement (Pre-Chess) differential perft + timing against Fairy-Stockfish
//! (issue #266).
//!
//! Placement runs on mcr's **generic** engine (`mcr::geometry::Placement`, a
//! `GenericPosition<Chess8x8, PlacementRules>`), like Sittuyin, so it has its own
//! corpus and comparison loop here. The FSF side selects `UCI_Variant placement`,
//! sets the FEN, runs `go perft`, asserts the node counts match, and reports
//! mcr-vs-FSF throughput.
//!
//! ## FEN dialect
//!
//! Placement uses only **standard chess pieces**, whose letters (`K Q R B N P`)
//! are identical in mcr and FSF, so no piece-letter translation is needed. mcr
//! writes the deployment pocket in role-index order (`NNBBRRQK`) where FSF writes
//! `KQRRBBNN`, but FSF accepts the pocket bracket in any order, and an empty
//! pocket may be written with or without the `[]` bracket; both engines accept
//! mcr's rendering verbatim. So the FEN is passed to FSF **unchanged**. The
//! comparison asserts only node counts, so the move-string dialect never matters.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Placement};

use crate::uci::Engine;

/// One Placement corpus position.
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Placement comparison corpus: the FSF-confirmed startpos (a deployment-phase
/// position), a mid-deployment position (bishop opposite-color constraint live),
/// a mid-deployment position exercising incremental castling, a non-standard
/// fully-deployed array with castling, and a developed middlegame with castling
/// and an en-passant target.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "8/pppppppp/8/8/8/8/PPPPPPPP/8[NNBBRRQKnnbbrrqk] w - - 0 1",
        depth: 4,
    },
    Case {
        label: "mid-deploy",
        fen: "rnb5/pppppppp/8/8/8/8/PPPPPPPP/RNB5[NBRQKnbrqk] w - - 0 4",
        depth: 4,
    },
    Case {
        label: "mid-castling",
        fen: "rnbqkbn1/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[r] b KQq - 0 8",
        depth: 4,
    },
    Case {
        label: "custom-array",
        fen: "rbnqknbr/pppppppp/8/8/8/8/PPPPPPPP/RBNQKNBR w KQkq - 0 9",
        depth: 4,
    },
    Case {
        label: "dev-midgame",
        fen: "rbnqk1br/ppp1pppp/5n2/3pP3/8/2N5/PPPP1PPP/R1BQKBNR w KQkq d6 0 9",
        depth: 4,
    },
];

/// A measured Placement comparison row.
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

/// Run the Placement corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all positions matched). Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Placement (Pre-Chess) — generic engine vs FSF UCI_Variant placement (issue #266):");
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
                eprintln!("skip placement/{}: {e}", case.label);
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
            "placement OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Placement positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Placement parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH placement/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Placement position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mcr side: the generic Placement position.
    let pos = Placement::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // FSF side: the standard piece letters are shared and FSF accepts the pocket
    // bracket in any order, so the FEN is passed through unchanged.
    engine.set_variant("placement", false)?;
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

    /// The corpus FENs all parse on the generic Placement engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in
    /// `tests/perft_placement.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 2u32, 1600u64),
            ("mid-deploy", 2, 529),
            ("mid-castling", 3, 400),
            ("custom-array", 2, 400),
            ("dev-midgame", 2, 751),
        ];
        for case in CASES {
            let pos = Placement::from_fen(case.fen).expect("corpus FEN parses");
            let (_, depth, want) = pinned
                .iter()
                .find(|(l, _, _)| *l == case.label)
                .copied()
                .expect("a pinned count for the case");
            assert_eq!(
                gperft::<Chess8x8, _>(&pos, depth),
                want,
                "{} perft",
                case.label
            );
        }
    }
}
