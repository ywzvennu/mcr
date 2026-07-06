//! No-castle chess differential perft + timing against Fairy-Stockfish.
//!
//! No-castle chess runs on mcr's **generic** engine (`mcr::geometry::Nocastle`, a
//! `GenericPosition<Chess8x8, NocastleRules>`), like the other fairy variants, so
//! it has its own corpus and comparison loop here. `nocastle` is an FSF
//! **built-in** (it appears in the `UCI_Variant` combo with no `variants.ini`), so
//! the suite selects `UCI_Variant nocastle` directly — no ini load — sets the FEN,
//! runs `go perft`, asserts the node counts match, and reports mcr-vs-FSF
//! throughput.
//!
//! ## FEN dialect
//!
//! No-castle chess is standard chess with castling disabled: mcr and FSF render
//! every position with the **identical** standard-chess letters, so the dialect is
//! the identity — the FEN is passed through unchanged. The startpos matches
//! standard chess (a castle is unreachable in the opening); the divergence appears
//! wherever standard chess would castle, exactly the castle moves being absent.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Nocastle};

use crate::uci::Engine;

/// One no-castle corpus position. mcr and FSF spell it identically.
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The no-castle comparison corpus: the FSF-confirmed startpos (matches standard
/// chess until a castle would arise), the classic Kiwipete position and a cleared
/// back-rank rooks-and-kings position (both castling-rich in standard chess, so
/// their node counts drop by exactly the missing castles here), and a promotion
/// position exercising the standard four-role promotion set.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1",
        depth: 5,
    },
    Case {
        label: "kiwipete",
        fen: "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w - - 0 1",
        depth: 4,
    },
    Case {
        label: "rooks-kings",
        fen: "r3k2r/8/8/8/8/8/8/R3K2R w - - 0 1",
        depth: 5,
    },
    Case {
        label: "promo",
        fen: "4k3/1P6/8/8/8/8/6p1/4K3 w - - 0 1",
        depth: 5,
    },
];

/// A measured no-castle comparison row.
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

/// Run the no-castle corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all matched, or the suite was skipped). Skips gracefully when the loaded
/// FSF binary does not advertise the `nocastle` built-in.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("No-castle chess (8x8) — generic engine vs FSF UCI_Variant nocastle:");

    if !engine.has_variant("nocastle") {
        println!("  SKIP: the loaded FSF binary does not advertise the `nocastle` built-in.");
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
                eprintln!("skip nocastle/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "nocastle OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} no-castle positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} no-castle parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH nocastle/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one no-castle position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Nocastle::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // mcr and FSF spell no-castle chess identically (standard-chess letters).
    engine.set_variant("nocastle", false)?;
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

    /// The corpus FENs all parse on the generic no-castle engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_nocastle.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 3u32, 8902u64),
            ("kiwipete", 3, 86677),
            ("rooks-kings", 3, 11522),
        ];
        for (label, depth, want) in pinned {
            let case = CASES.iter().find(|c| c.label == label).expect("label");
            let pos = Nocastle::from_fen(case.fen).expect("corpus FEN parses");
            assert_eq!(
                gperft::<Chess8x8, _>(&pos, depth),
                want,
                "{label} perft({depth})"
            );
        }
    }
}
