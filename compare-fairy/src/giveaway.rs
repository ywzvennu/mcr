//! Giveaway chess differential perft + timing against Fairy-Stockfish.
//!
//! Giveaway chess runs on mcr's **generic** engine (`mcr::geometry::Giveaway`, a
//! `GenericPosition<Chess8x8, GiveawayRules>`), like the other fairy variants, so it
//! has its own corpus and comparison loop here. `giveaway` is an FSF **built-in**
//! (`giveaway_variant()`), so the suite selects `UCI_Variant giveaway` directly — no
//! ini load — sets the FEN, runs `go perft`, asserts the node counts match, and
//! reports mcr-vs-FSF throughput.
//!
//! ## FEN dialect
//!
//! Giveaway is antichess with castling and a non-royal Commoner king: mcr and FSF
//! render every position with the **identical** standard-chess letters (the king is
//! `k`/`K`), so the dialect is the identity — the FEN is passed through unchanged.
//! The divergence from standard chess is in *legality* (mandatory captures prune
//! quiets, no check) and *termination* (losing the whole army, or being stalemated,
//! wins).
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no GPL
//! code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Giveaway};

use crate::uci::Engine;

/// One giveaway corpus position. mcr and FSF spell it identically.
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The giveaway comparison corpus: the FSF-confirmed startpos (mandatory captures
/// already reshape the tree from move 1), Kiwipete (rich tactics, king-promotion
/// reachable), a castling-reachable position (non-royal castling), and an
/// already-won position where a side has no pieces (`go perft` = 0 at every depth).
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        depth: 5,
    },
    Case {
        label: "kiwipete",
        fen: "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "castling",
        fen: "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "zero-pieces",
        fen: "8/8/8/8/8/8/8/K7 b - - 0 1",
        depth: 4,
    },
];

/// A measured giveaway comparison row.
struct Row {
    label: &'static str,
    depth: u32,
    mcr_nodes: u64,
    fsf_nodes: u64,
    matched: bool,
    mcr_secs: f64,
    fsf_secs: f64,
}

/// Run the giveaway corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all matched, or the suite was skipped).
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Giveaway chess (8x8) — generic engine vs FSF UCI_Variant giveaway:");

    if !engine.has_variant("giveaway") {
        println!("  SKIP: the loaded FSF binary does not advertise the `giveaway` built-in.");
        return 0;
    }

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
                    "  {:<12} depth {:>2}: mcr {:>12} fsf {:>12}  {}",
                    row.label,
                    row.depth,
                    row.mcr_nodes,
                    row.fsf_nodes,
                    if row.matched { "ok" } else { "MISMATCH" },
                );
                rows.push(row);
            }
            Err(e) => eprintln!("skip giveaway/{}: {e}", case.label),
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "giveaway OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }
    if mismatches == 0 {
        println!(
            "OK: all {} giveaway positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} giveaway parity mismatch(es) vs FSF.");
    }
    mismatches
}

/// Run one giveaway position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Giveaway::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    engine.set_variant("giveaway", false)?;
    engine.set_position(case.fen)?;
    let fsf = engine.go_perft(depth, false)?;

    Ok(Row {
        label: case.label,
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

    /// The corpus FENs all parse on the generic giveaway engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_giveaway.rs`.
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 4u32, 153299u64),
            ("kiwipete", 3, 487),
            ("castling", 3, 14860),
            ("zero-pieces", 3, 0),
        ];
        for (label, depth, want) in pinned {
            let case = CASES.iter().find(|c| c.label == label).expect("label");
            let pos = Giveaway::from_fen(case.fen).expect("corpus FEN parses");
            assert_eq!(
                gperft::<Chess8x8, _, _>(&pos, depth),
                want,
                "{label} perft({depth})"
            );
        }
    }
}
