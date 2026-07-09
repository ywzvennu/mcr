//! Atomar differential perft + timing against Fairy-Stockfish.
//!
//! Atomar runs on mcr's **generic** engine (`mcr::geometry::Atomar`, a
//! `GenericPosition<Chess8x8, AtomarRules>`), like the other fairy variants, so it
//! has its own corpus and comparison loop here. The FSF side selects `UCI_Variant
//! atomar` (a built-in — no `variants.ini` needed), sets the FEN, runs `go perft`,
//! asserts the node counts match, and reports mcr-vs-FSF throughput.
//!
//! Atomar is nocheckatomic with two Commoner immunities: Commoners are blast-immune
//! (they survive adjacent explosions, and a capturing Commoner survives its own
//! blast) and mutually immune (a Commoner may never capture the enemy Commoner). The
//! corpus therefore includes the adjacent-Commoners position (mutual immunity shrinks
//! the move set) and a Commoner-beside-captures position (blast immunity keeps it
//! alive), both diverging from nocheckatomic.
//!
//! ## FEN dialect
//!
//! Atomar uses only **standard chess pieces** (`K Q R B N P` — its king is a Commoner
//! by rule, not by letter), identical in mcr and FSF, so the FEN is passed to FSF
//! **unchanged**.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no GPL
//! code is linked, and Atomar needs no INI.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Atomar, Chess8x8};

use crate::uci::Engine;

/// One Atomar corpus position. The FEN is shared verbatim with FSF.
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Atomar comparison corpus (all FSF-confirmed): the startpos, the Kiwipete
/// tactical middlegame, an Italian opening, a symmetric blast cluster, the adjacent
/// Commoners (mutual immunity), and a Commoner beside captures (blast immunity).
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "kiwi",
        fen: "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "italian",
        fen: "r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4",
        depth: 4,
    },
    Case {
        label: "blast",
        fen: "r2qkb1r/ppp2ppp/2n2n2/3pp1B1/3PP1b1/2N2N2/PPP2PPP/R2QKB1R w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "adjacent",
        fen: "8/8/8/4k3/4K3/8/8/8 w - - 0 1",
        depth: 4,
    },
    Case {
        label: "immune",
        fen: "4k3/8/3r4/2rK4/8/8/8/8 w - - 0 1",
        depth: 4,
    },
];

/// A measured Atomar comparison row.
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

/// Run the Atomar corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all matched). `atomar` is a FSF built-in, so if this binary does not
/// advertise it the block is skipped cleanly (returns 0).
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Atomar — generic engine vs FSF UCI_Variant atomar:");

    if !engine.has_variant("atomar") {
        println!("  (skipped: this FSF binary does not advertise UCI_Variant atomar)");
        return 0;
    }

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
                eprintln!("skip atomar/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "atomar OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s \
             ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Atomar positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Atomar parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH atomar/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Atomar position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Atomar::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // Identity dialect: Atomar shares the standard-chess letters with FSF, so the FEN
    // is passed through unchanged.
    engine.set_variant("atomar", false)?;
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

    /// The corpus FENs all parse on the generic Atomar engine, round-trip through
    /// mcr's FEN I/O, and the pinned shallow counts match the FSF-confirmed numbers in
    /// `tests/perft_atomar.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 3u32, 8902u64),
            ("kiwi", 3, 93_221),
            ("italian", 3, 33_284),
            ("blast", 3, 64_182),
            ("adjacent", 3, 397),
            ("immune", 4, 39_924),
        ];
        for case in CASES {
            let pos = Atomar::from_fen(case.fen).expect("corpus FEN parses");
            assert_eq!(pos.to_fen(), case.fen, "{} round-trips", case.label);
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
