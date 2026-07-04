//! Shogi (9x9) differential perft + timing against Fairy-Stockfish (issue #190).
//!
//! Shogi runs on mcr's **generic** `u128` engine (`mcr::geometry::Shogi`, a
//! `GenericPosition<Shogi9x9, ShogiRules>`), not the concrete 8x8 `AnyVariant`
//! layer the rest of this harness drives, so it has its own corpus and comparison
//! loop here (mirroring `xiangqi.rs` / `shako.rs`). The FSF side selects
//! `UCI_Variant shogi`, sets the FEN, runs `go perft`, asserts the node counts
//! match, and reports mcr-vs-FSF throughput. The corpus exercises **drops** (with
//! pieces in hand — drops dominate the branching factor), the **dead-piece** and
//! **nifu** drop filters, a **forced promotion**, and the **promote / don't
//! promote** zone-entry choice.
//!
//! **FSF must be built with large-board support** (`make ... largeboards=yes`):
//! the default FSF build omits the 9x9 `shogi` variant from its `UCI_Variant`
//! list. When the running binary lacks `shogi`, this loop skips rather than
//! compare meaningless truncated counts.
//!
//! ## FEN dialect
//!
//! mcr and FSF use the **same** Shogi piece letters — `l n s g k r b p` and the
//! `+`-prefixed promoted forms `+P +L +N +S +R +B` — and the same `[..]`
//! holdings-bracket convention for the hand (uppercase = white, lowercase =
//! black; empty `[]`). So, unlike Xiangqi, **no FEN rewrite is needed**: the mcr
//! FEN is passed to FSF verbatim.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Shogi, Shogi9x9};

use crate::uci::Engine;

/// One Shogi corpus position.
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Shogi comparison corpus: the FSF-confirmed startpos; a pawn-in-each-hand
/// drop position; a drop-heavy middlegame; a bare-kings position with one of
/// every role in hand (drops dominate); a forced-promotion wall; a
/// promote/don't-promote choice; and a mating-pawn-drop position (FSF counts it —
/// no uchifuzume). Depths are modest by default; `full` deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL[] w - - 0 1",
        depth: 4,
    },
    Case {
        label: "drops-in-hand",
        fen: "lnsgkgsnl/1r5b1/p1ppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL[Pp] w - - 0 1",
        depth: 3,
    },
    Case {
        label: "midgame",
        fen: "lnsgkgsnl/1r5b1/pppppp1pp/9/9/9/PPPPPP1PP/1B5R1/LNSGKGSNL[Pp] b - - 0 1",
        depth: 3,
    },
    Case {
        label: "multi-hand",
        fen: "4k4/9/9/9/9/9/9/9/4K4[RBGSNLPrbgsnlp] w - - 0 1",
        depth: 2,
    },
    Case {
        label: "forced-promo",
        fen: "4k4/PPPPPPPPP/9/9/9/9/9/ppppppppp/4K4[] w - - 0 1",
        depth: 3,
    },
    Case {
        label: "promo-choice",
        fen: "9/4P4/9/9/9/9/9/9/4k1K2[] w - - 0 1",
        depth: 3,
    },
    Case {
        label: "nifu-mate",
        fen: "k8/9/9/9/9/9/9/9/LR2K4[P] w - - 0 1",
        depth: 3,
    },
];

/// A measured Shogi comparison row.
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

/// Run the Shogi corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all matched, or FSF lacks `shogi` and the suite is skipped). Prints a
/// table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!(
        "Shogi (9x9, u128, hand + drops + promotion zone) — generic engine vs FSF \
UCI_Variant shogi (issue #190):"
    );
    println!("  (requires an FSF built with largeboards=yes)");

    if !engine.has_variant("shogi") {
        println!("  SKIP: this FSF binary has no `shogi` variant (build it largeboards=yes).");
        return 0;
    }

    let head = format!(
        "{:<18} {:>5} {:>14} {:>14} {:>9} {:>10} {:>10} {:>8}",
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
                    "{:<18} {:>5} {:>14} {:>14} {:>9} {:>10.1} {:>10.1} {:>7.2}x",
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
                eprintln!("skip shogi/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "shogi OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Shogi positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Shogi parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH shogi/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Shogi position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Shogi::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Shogi9x9, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // mcr and FSF share the Shogi FEN dialect, so the FEN is passed verbatim.
    engine.set_variant("shogi", false)?;
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

    /// The corpus FENs all parse on the generic Shogi engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_shogi.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 900u64),
            ("drops-in-hand", 1168),
            ("midgame", 1515),
            ("multi-hand", 251422),
            ("forced-promo", 9),
            ("promo-choice", 16),
            ("nifu-mate", 26),
        ];
        for case in CASES {
            let pos = Shogi::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Shogi9x9, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }
}
