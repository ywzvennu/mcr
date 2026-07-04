//! Sittuyin (Burmese chess) differential perft + timing against Fairy-Stockfish
//! (issue #179).
//!
//! Sittuyin runs on mcr's **generic** engine (`mcr::geometry::Sittuyin`, a
//! `GenericPosition<Chess8x8, SittuyinRules>`), like Makruk, so it has its own
//! corpus and comparison loop here. The FSF side selects `UCI_Variant sittuyin`,
//! sets the FEN, runs `go perft`, asserts the node counts match, and reports
//! mcr-vs-FSF throughput.
//!
//! ## FEN dialect
//!
//! mcr and FSF render the same Sittuyin position with **different Met letters**:
//! mcr uses `m`/`M` (its generic Met role letter), FSF uses `f`/`F` (its
//! `sittuyin` Met = ferz). The board / pocket placement is the only field that
//! carries piece letters (Sittuyin has no castling rights and no en-passant
//! target), so [`to_fsf_dialect`] simply swaps `m`↔`f` over the whole FEN before
//! handing it to FSF. FSF accepts the pocket bracket in any order, so no reorder
//! is needed. The comparison asserts only node counts, so the move-string dialect
//! never matters.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Sittuyin};

use crate::uci::Engine;

/// One Sittuyin corpus position. The FEN is mcr's dialect (Met = `m`); the FSF
/// side translates it via [`to_fsf_dialect`].
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Sittuyin comparison corpus: the FSF-confirmed startpos (a placement-phase
/// position), a fully-deployed middlegame, a post-Met-capture promotion
/// middlegame, and a mid-deployment position (one side deployed, one in hand).
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "8/8/4pppp/pppp4/4PPPP/PPPP4/8/8[NNRRKMSSnnrrkmss] w - - 0 1",
        depth: 4,
    },
    Case {
        label: "deployed-mid",
        fen: "rrnmk1n1/1ss5/4pppp/pppp4/4PPPP/PPPP4/1SS5/RRNMK1N1 w - - 0 9",
        depth: 5,
    },
    Case {
        label: "promo-mid",
        fen: "rrn1k1n1/1ss5/4pppp/ppp5/5PPP/PPPP1p2/1SS1M3/RRN1K1N1 b - - 0 11",
        depth: 4,
    },
    Case {
        label: "deploy-mid",
        fen: "8/8/4pppp/pppp4/4PPPP/PPPP4/8/3M2R1[NNRKSSnnrrkmss] b - - 0 3",
        depth: 3,
    },
];

/// Translates an mcr-dialect Sittuyin FEN to FSF's dialect by swapping the Met
/// letter `m`↔`f` (both cases). Safe over the whole FEN: only the placement /
/// pocket field carries piece letters in Sittuyin (no castling, no en passant).
pub(crate) fn to_fsf_dialect(fen: &str) -> String {
    fen.chars()
        .map(|c| match c {
            'm' => 'f',
            'M' => 'F',
            'f' => 'm',
            'F' => 'M',
            other => other,
        })
        .collect()
}

/// A measured Sittuyin comparison row.
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

/// Run the Sittuyin corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all positions matched). Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Sittuyin (Burmese chess) — generic engine vs FSF UCI_Variant sittuyin (issue #179):");
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
                eprintln!("skip sittuyin/{}: {e}", case.label);
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
            "sittuyin OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Sittuyin positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Sittuyin parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH sittuyin/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Sittuyin position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mcr side: the generic Sittuyin position (mcr dialect).
    let pos = Sittuyin::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // FSF side: translate the Met letter to FSF's `f` dialect.
    let fsf_fen = to_fsf_dialect(case.fen);
    engine.set_variant("sittuyin", false)?;
    engine.set_position(&fsf_fen)?;
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

    /// The corpus FENs all parse on the generic Sittuyin engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in
    /// `tests/perft_sittuyin.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 2u32, 7744u64),
            ("deployed-mid", 2, 542),
            ("promo-mid", 2, 537),
            ("deploy-mid", 2, 5280),
        ];
        for case in CASES {
            let pos = Sittuyin::from_fen(case.fen).expect("corpus FEN parses");
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

    /// The `m`↔`f` dialect swap is its own inverse and leaves non-Met letters
    /// untouched.
    #[test]
    fn dialect_swap_round_trips() {
        let mcr = "8/8/4pppp/pppp4/4PPPP/PPPP4/8/8[NNRRKMSSnnrrkmss] w - - 0 1";
        let fsf = to_fsf_dialect(mcr);
        assert!(fsf.contains('F') && fsf.contains('f'));
        assert!(!fsf.contains('M') && !fsf.contains('m'));
        // Inverse restores the original.
        assert_eq!(to_fsf_dialect(&fsf), mcr);
    }
}
