//! Chaturanga differential perft + timing against Fairy-Stockfish (`UCI_Variant
//! chaturanga`, a built-in).
//!
//! Chaturanga is Shatranj with the baring-the-king loss removed and the standard
//! chess starting array (see `mcr::geometry::Chaturanga`). Like Shatranj it runs
//! on mcr's **generic** engine (`GenericPosition<Chess8x8, ChaturangaRules>`), so
//! it has its own small corpus and comparison loop here. `chaturanga` is an FSF
//! built-in (no `variants.ini` needed): select `UCI_Variant chaturanga`, set the
//! FEN, run `go perft`, and assert the node counts match.
//!
//! ## FEN dialect
//!
//! Identical to Shatranj's: mcr spells the Ferz `m` and the Alfil `*x`, which FSF
//! spells `q` and `b`. [`to_fsf_dialect`] rewrites them (`*x → b`, `m → q`), reusing
//! Shatranj's mapping since the two variants share every piece.
//!
//! The bared-king corpus position is the standout: where Shatranj truncates a
//! bared node to a terminal leaf (`go perft` returns 0), chaturanga has no baring
//! rule and plays on, so both engines here return a non-zero count.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chaturanga, Chess8x8};

use crate::uci::Engine;

/// One Chaturanga corpus position. The FEN is mcr's dialect; the FSF side
/// translates it via [`to_fsf_dialect`].
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Chaturanga comparison corpus: the FSF-confirmed startpos (the standard
/// array), a symmetric middlegame, and a bared-king endgame that — unlike Shatranj
/// — is **not** truncated (chaturanga has no baring rule, so `go perft` is
/// non-zero). Depths are kept modest by default; `full` deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rn*xmk*xnr/pppppppp/8/8/8/8/PPPPPPPP/RN*XMK*XNR w - - 0 1",
        depth: 5,
    },
    Case {
        label: "midgame",
        fen: "r1*xmk*xnr/pppp1ppp/2n5/4p3/4P3/2N5/PPPP1PPP/R1*XMK*XNR w - - 0 1",
        depth: 4,
    },
    Case {
        label: "bared-plays-on",
        fen: "4k3/8/8/2P1P3/3*X4/2P1P3/8/4K3 w - - 0 1",
        depth: 4,
    },
];

/// Translates an mcr-dialect Chaturanga FEN to FSF's dialect: Alfil `*x → b`, Ferz
/// `m → q` (both cases). Chaturanga shares Shatranj's pieces, so this reuses
/// Shatranj's mapping.
pub(crate) fn to_fsf_dialect(fen: &str) -> String {
    crate::shatranj::to_fsf_dialect(fen)
}

/// A measured Chaturanga comparison row.
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

/// Run the Chaturanga corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all positions matched). Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Chaturanga — generic engine vs FSF UCI_Variant chaturanga:");
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
                eprintln!("skip chaturanga/{}: {e}", case.label);
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
            "chaturanga OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Chaturanga positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Chaturanga parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH chaturanga/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Chaturanga position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mcr side: the generic Chaturanga position.
    let pos = Chaturanga::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // FSF side: rewrite the mcr dialect to FSF's `b`/`q` letters.
    let fsf_fen = to_fsf_dialect(case.fen);
    engine.set_variant("chaturanga", false)?;
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

    /// The corpus FENs all parse on the generic Chaturanga engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in
    /// `tests/perft_chaturanga.rs` (this runs without FSF present). The bared node
    /// counts non-zero — chaturanga has no baring truncation.
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 2u32, 256u64),
            ("midgame", 2, 440),
            ("bared-plays-on", 2, 60),
        ];
        for (label, depth, want) in pinned {
            let case = CASES.iter().find(|c| c.label == label).expect("label");
            let pos = Chaturanga::from_fen(case.fen).expect("corpus FEN parses");
            assert_eq!(
                gperft::<Chess8x8, _, _>(&pos, depth),
                want,
                "{label} perft({depth})"
            );
        }
    }

    #[test]
    fn dialect_round_trips_pieces() {
        assert_eq!(
            to_fsf_dialect("rn*xmk*xnr/pppppppp/8/8/8/8/PPPPPPPP/RN*XMK*XNR w - - 0 1"),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1"
        );
    }
}
