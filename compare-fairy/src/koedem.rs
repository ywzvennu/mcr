//! Koedem differential perft + timing against Fairy-Stockfish.
//!
//! Koedem ("King of the dead") runs on mcr's **generic** engine
//! (`mcr::geometry::Koedem`, a `GenericPosition<Chess8x8, KoedemRules>`), like the
//! other fairy variants, so it has its own corpus and comparison loop here. The FSF
//! side selects `UCI_Variant koedem` (a built-in — no `variants.ini` needed), sets
//! the FEN, runs `go perft`, asserts the node counts match, and reports mcr-vs-FSF
//! throughput.
//!
//! ## FEN dialect
//!
//! Koedem uses only **standard chess pieces** (`K Q R B N P` — its king is a
//! Commoner by rule, not by letter), identical in mcr and FSF, and the crazyhouse
//! hand rides the same `[..]` bracket both engines accept, so the FEN is passed to
//! FSF **unchanged** ([`identity`]).
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no GPL
//! code is linked, and Koedem needs no INI.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Koedem};

use crate::uci::Engine;

/// One Koedem corpus position. The FEN is shared verbatim with FSF (identity
/// dialect).
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Koedem comparison corpus (all FSF-confirmed): the startpos, a developed hand
/// middlegame (`[PNpn]`, drops live), a forced Commoner drop (`[K]`, `mustDrop`), a
/// single-king endgame where a king can be captured without ending the game, a
/// two-king middlegame, and a full-reserve drop stress.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1",
        depth: 5,
    },
    Case {
        label: "hand",
        fen: "r1bqk2r/ppp2ppp/2n5/3pp3/3PP3/2N5/PPP2PPP/R1BQK2R[PNpn] w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "kingdrop",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[K] w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "endgame",
        fen: "4k3/pp6/8/8/8/8/6PP/4K3[] w - - 0 1",
        depth: 5,
    },
    Case {
        label: "twoking",
        fen: "3k1k2/pp4pp/8/8/8/8/PP4PP/3K1K2[] w - - 0 1",
        depth: 4,
    },
    Case {
        label: "rich",
        fen: "r3k2r/8/8/8/8/8/8/R3K2R[PNBRQpnbrq] w KQkq - 0 1",
        depth: 3,
    },
];

/// A measured Koedem comparison row.
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

/// Run the Koedem corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all matched). `koedem` is a FSF built-in, so if this binary does not
/// advertise it the block is skipped cleanly (returns 0).
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Koedem — generic engine vs FSF UCI_Variant koedem:");

    if !engine.has_variant("koedem") {
        println!("  (skipped: this FSF binary does not advertise UCI_Variant koedem)");
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
                eprintln!("skip koedem/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "koedem OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s \
             ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Koedem positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Koedem parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH koedem/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Koedem position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Koedem::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // Identity dialect: Koedem shares the standard-chess letters and the `[..]` hand
    // bracket with FSF, so the FEN is passed through unchanged.
    engine.set_variant("koedem", false)?;
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

    /// The corpus FENs all parse on the generic Koedem engine, round-trip through
    /// mcr's FEN I/O, and the pinned shallow counts match the FSF-confirmed numbers
    /// in `tests/perft_koedem.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 3u32, 8902u64),
            ("hand", 3, 790_351),
            ("kingdrop", 3, 16_040),
            ("endgame", 4, 7_224),
            ("twoking", 3, 4_913),
            ("rich", 2, 91_960),
        ];
        for case in CASES {
            let pos = Koedem::from_fen(case.fen).expect("corpus FEN parses");
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
