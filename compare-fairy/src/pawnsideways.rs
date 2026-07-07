//! Pawn-sideways chess differential perft + timing against Fairy-Stockfish.
//!
//! Pawn-sideways runs on mcr's **generic** engine (`mcr::geometry::Pawnsideways`, a
//! `GenericPosition<Chess8x8, PawnsidewaysRules>`), not the concrete `AnyVariant`
//! layer the rest of this harness drives, so it has its own small corpus and
//! comparison loop here (mirroring `berolina.rs`). The FSF side selects
//! `UCI_Variant pawnsideways` (a built-in), sets the FEN, runs `go perft`, and the
//! node counts are asserted equal.
//!
//! mcr and FSF spell pawn-sideways chess with the **identical** standard-chess
//! letters (the pawn that may step sideways stays `p`/`P` — the sideways step is a
//! *rule*, not a letter), so the FEN is passed through unchanged. A sideways step
//! never creates an en-passant target; the ordinary forward double-step en passant
//! is spelled identically in both engines.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Pawnsideways};

use crate::uci::Engine;

/// One pawn-sideways corpus position. mcr and FSF share the FEN dialect, so the
/// same string feeds both engines.
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The pawn-sideways comparison corpus: the FSF-confirmed startpos, a midgame, a
/// sideways-blocking/unblocking tangle, and a pin position where a pawn may or may
/// not step sideways. Depths are modest by default; `full` adds a ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        depth: 5,
    },
    Case {
        label: "midgame",
        fen: "rnbqkbnr/pp1ppppp/8/2p5/3P4/8/PPP1PPPP/RNBQKBNR w KQkq - 0 1",
        depth: 4,
    },
    Case {
        // A both-armies pawn wall: sideways steps let pawns slide along the ranks,
        // blocking and unblocking each other's forward pushes.
        label: "sideways-wall",
        fen: "4k3/pppppppp/8/8/8/8/PPPPPPPP/4K3 w - - 0 1",
        depth: 5,
    },
    Case {
        // Rooks pin the e-file pawns to their kings: the pinned pawn may push along
        // the file but neither sideways step stays on the pin line, while the
        // unpinned pawns step sideways freely.
        label: "pins",
        fen: "4r1k1/pppp1ppp/8/8/8/8/PPPP1PPP/4R1K1 w - - 0 1",
        depth: 4,
    },
];

/// A measured pawn-sideways comparison row.
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

/// Run the pawn-sideways corpus through mcr and FSF. Returns the number of
/// mismatches (0 = all positions matched). Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Pawn-sideways chess — generic engine vs FSF UCI_Variant pawnsideways:");
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
                eprintln!("skip pawnsideways/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "pawnsideways OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} pawn-sideways positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} pawn-sideways parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH pawnsideways/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one pawn-sideways position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Pawnsideways::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // FSF side: pawnsideways is a FSF built-in; mcr and FSF share the FEN dialect.
    engine.set_variant("pawnsideways", false)?;
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

    /// The corpus FENs all parse on the generic pawn-sideways engine, and the
    /// pinned depth-2 counts match the FSF-confirmed numbers (this runs without FSF
    /// present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 400u64),
            ("midgame", 879),
            ("sideways-wall", 324),
            ("pins", 847),
        ];
        for case in CASES {
            let pos = Pawnsideways::from_fen(case.fen).expect("corpus FEN parses");
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
