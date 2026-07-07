//! Pocket Knight chess differential perft + timing against Fairy-Stockfish.
//!
//! Pocket Knight runs on mcr's **generic** engine (`mcr::geometry::Pocketknight`, a
//! `GenericPosition<Chess8x8, PocketknightRules>`), like the other fairy variants,
//! so it has its own corpus and comparison loop here. `pocketknight` is an FSF
//! **built-in** (it appears in the `UCI_Variant` combo with no `variants.ini`), so
//! the suite selects `UCI_Variant pocketknight` directly — no ini load — sets the
//! FEN, runs `go perft`, asserts the node counts match, and reports mcr-vs-FSF
//! throughput.
//!
//! ## FEN dialect
//!
//! Pocket Knight is standard chess with one extra Knight in hand per side: mcr and
//! FSF render every position with the **identical** standard-chess letters and bank
//! the Knight as `N`/`n` in the `[Nn]` holdings bracket, so the dialect is the
//! identity — the FEN is passed through unchanged. The pocket widens the branching
//! from the very first ply (a Knight drop onto every empty square); once both
//! pockets empty (captures never refill them) the tree collapses to standard chess.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Pocketknight};

use crate::uci::Engine;

/// One Pocket Knight corpus position. mcr and FSF spell it identically.
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Pocket Knight comparison corpus: the FSF-confirmed startpos (both pockets
/// full — a Knight drop onto every empty square on top of the 20 opening moves),
/// the castling-rich Kiwipete with both pockets, a one-sided pocket (only White
/// holds a Knight, `[N]`), an empty-pocket startpos (no hand → collapses onto plain
/// standard chess), and a promotion position with both pockets.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[Nn] w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "kiwipete",
        fen: "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R[Nn] w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "one-sided",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[N] w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "empty-pocket",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1",
        depth: 5,
    },
    Case {
        label: "promo",
        fen: "4k3/1P6/8/8/8/8/6p1/4K3[Nn] w - - 0 1",
        depth: 4,
    },
];

/// A measured Pocket Knight comparison row.
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

/// Run the Pocket Knight corpus through mcr and FSF. Returns the number of
/// mismatches (0 = all matched, or the suite was skipped). Skips gracefully when
/// the loaded FSF binary does not advertise the `pocketknight` built-in.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Pocket Knight chess (8x8) — generic engine vs FSF UCI_Variant pocketknight:");

    if !engine.has_variant("pocketknight") {
        println!("  SKIP: the loaded FSF binary does not advertise the `pocketknight` built-in.");
        return 0;
    }

    let head = format!(
        "{:<13} {:>5} {:>14} {:>14} {:>9} {:>10} {:>10} {:>8}",
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
                    "{:<13} {:>5} {:>14} {:>14} {:>9} {:>10.1} {:>10.1} {:>7.2}x",
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
                eprintln!("skip pocketknight/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "pocketknight OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Pocket Knight positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Pocket Knight parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH pocketknight/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Pocket Knight position through mcr's generic perft and FSF's
/// `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Pocketknight::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // mcr and FSF spell Pocket Knight identically (standard-chess letters, `[Nn]`).
    engine.set_variant("pocketknight", false)?;
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

    /// The corpus FENs all parse on the generic Pocket Knight engine, and the
    /// pinned shallow counts match the FSF-confirmed numbers in
    /// `tests/perft_pocketknight.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 3u32, 88617u64),
            ("kiwipete", 3, 390853),
            ("one-sided", 3, 35942),
            ("empty-pocket", 3, 8902),
            ("promo", 3, 72683),
        ];
        for (label, depth, want) in pinned {
            let case = CASES.iter().find(|c| c.label == label).expect("label");
            let pos = Pocketknight::from_fen(case.fen).expect("corpus FEN parses");
            assert_eq!(
                gperft::<Chess8x8, _, _>(&pos, depth),
                want,
                "{label} perft({depth})"
            );
        }
    }
}
