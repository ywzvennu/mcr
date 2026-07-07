//! Codrus differential perft + timing against Fairy-Stockfish.
//!
//! Codrus runs on mcr's **generic** engine (`mcr::geometry::Codrus`). `codrus` is an
//! FSF **built-in** (`codrus_variant()`): giveaway watching only the king, with no
//! king-promotion. mcr and FSF spell it with the identical standard-chess letters,
//! so the FEN passes through unchanged. GPL FENCE unchanged (FSF is a subprocess).

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Codrus};

use crate::uci::Engine;

struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The codrus corpus: startpos, Kiwipete (where dropping king-promotion undercuts
/// giveaway), and an already-won position where a side has lost its king.
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
        label: "lost-king",
        fen: "7R/8/8/8/8/8/8/7K b - - 0 1",
        depth: 4,
    },
];

struct Row {
    label: &'static str,
    depth: u32,
    mcr_nodes: u64,
    fsf_nodes: u64,
    matched: bool,
    mcr_secs: f64,
    fsf_secs: f64,
}

pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Codrus (8x8) — generic engine vs FSF UCI_Variant codrus:");
    if !engine.has_variant("codrus") {
        println!("  SKIP: the loaded FSF binary does not advertise the `codrus` built-in.");
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
            Err(e) => eprintln!("skip codrus/{}: {e}", case.label),
        }
    }
    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "codrus OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }
    if mismatches == 0 {
        println!(
            "OK: all {} codrus positions matched FSF ({nodes} nodes verified).",
            rows.len()
        );
    } else {
        eprintln!("ERROR: {mismatches} codrus parity mismatch(es) vs FSF.");
    }
    mismatches
}

fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Codrus::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();
    engine.set_variant("codrus", false)?;
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

    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 4u32, 153299u64),
            ("kiwipete", 4, 3836),
            ("lost-king", 3, 0),
        ];
        for (label, depth, want) in pinned {
            let case = CASES.iter().find(|c| c.label == label).expect("label");
            let pos = Codrus::from_fen(case.fen).expect("corpus FEN parses");
            assert_eq!(
                gperft::<Chess8x8, _, _>(&pos, depth),
                want,
                "{label} perft({depth})"
            );
        }
    }
}
