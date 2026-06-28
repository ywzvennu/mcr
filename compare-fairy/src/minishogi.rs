//! Minishogi (5x5) differential perft + timing against Fairy-Stockfish (#195).
//!
//! Minishogi runs on mce's **generic** `u64` engine (`mce::geometry::Minishogi`,
//! a `GenericPosition<Minishogi5x5, MinishogiRules>`), not the concrete 8x8
//! `AnyVariant` layer the rest of this harness drives, so it has its own corpus
//! and comparison loop here (mirroring `shogi.rs`). The FSF side selects
//! `UCI_Variant minishogi`, sets the FEN, runs `go perft`, asserts the node
//! counts match, and reports mce-vs-FSF throughput. The corpus exercises
//! **drops** (with pieces in hand), the **dead-piece** and **nifu** drop
//! filters, a **forced promotion**, and the **promote / don't-promote**
//! zone-entry choice on the single-rank zone.
//!
//! **FSF must be built with large-board support** (`make ... largeboards=yes`):
//! the default FSF build omits the 5x5 `minishogi` variant from its
//! `UCI_Variant` list. When the running binary lacks `minishogi`, this loop skips
//! rather than compare meaningless truncated counts.
//!
//! ## FEN dialect
//!
//! mce and FSF use the **same** Minishogi piece letters — `s g k r b p` and the
//! `+`-prefixed promoted forms `+P +S +R +B` — and the same `[..]`
//! holdings-bracket convention for the hand. So, like Shogi, **no FEN rewrite is
//! needed**: the mce FEN is passed to FSF verbatim.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mce::geometry::{perft as gperft, Minishogi, Minishogi5x5};

use crate::uci::Engine;

/// One Minishogi corpus position.
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Minishogi comparison corpus: the FSF-confirmed startpos; a pawn-in-each-hand
/// drop position; a bare-kings position with one of every droppable role in hand
/// (drops dominate); a forced-promotion pawn; a promote/don't-promote silver; a
/// drop-heavy open middlegame; and a nifu position. Depths are modest by default;
/// `full` deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rbsgk/4p/5/P4/KGSBR[] w - - 0 1",
        depth: 4,
    },
    Case {
        label: "drops-in-hand",
        fen: "rbsgk/5/5/5/KGSBR[Pp] w - - 0 1",
        depth: 4,
    },
    Case {
        label: "multi-hand",
        fen: "k4/5/5/5/4K[RBGSPrbgsp] w - - 0 1",
        depth: 2,
    },
    Case {
        label: "forced-promo",
        fen: "4k/4P/5/5/4K[] w - - 0 1",
        depth: 4,
    },
    Case {
        label: "promo-choice",
        fen: "4k/4S/5/5/4K[] w - - 0 1",
        depth: 4,
    },
    Case {
        label: "midgame",
        fen: "2k2/5/R3r/5/2K2[Pp] w - - 0 1",
        depth: 3,
    },
    Case {
        label: "nifu",
        fen: "2k2/5/P4/5/2K2[P] w - - 0 1",
        depth: 3,
    },
];

/// A measured Minishogi comparison row.
struct Row {
    label: &'static str,
    fen: &'static str,
    depth: u32,
    mce_nodes: u64,
    fsf_nodes: u64,
    matched: bool,
    mce_secs: f64,
    fsf_secs: f64,
}

impl Row {
    fn mce_mnps(&self) -> f64 {
        if self.mce_secs > 0.0 {
            self.mce_nodes as f64 / self.mce_secs / 1e6
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
        if self.mce_secs > 0.0 {
            self.fsf_secs / self.mce_secs
        } else {
            f64::NAN
        }
    }
}

/// Run the Minishogi corpus through mce and FSF. Returns the number of mismatches
/// (0 = all matched, or FSF lacks `minishogi` and the suite is skipped). Prints a
/// table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!(
        "Minishogi (5x5, u64, hand + drops + promotion zone) — generic engine vs FSF \
UCI_Variant minishogi (issue #195):"
    );
    println!("  (requires an FSF built with largeboards=yes)");

    if !engine.has_variant("minishogi") {
        println!("  SKIP: this FSF binary has no `minishogi` variant (build it largeboards=yes).");
        return 0;
    }

    let head = format!(
        "{:<18} {:>5} {:>14} {:>14} {:>9} {:>10} {:>10} {:>8}",
        "position", "depth", "mce nodes", "fsf nodes", "match", "mce Mn/s", "fsf Mn/s", "mce/fsf",
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
                    row.mce_nodes,
                    row.fsf_nodes,
                    if row.matched { "ok" } else { "MISMATCH" },
                    row.mce_mnps(),
                    row.fsf_mnps(),
                    row.speedup(),
                );
                rows.push(row);
            }
            Err(e) => {
                eprintln!("skip minishogi/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mce_nodes).sum();
    let mce_s: f64 = rows.iter().map(|r| r.mce_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mce_s > 0.0 && fsf_s > 0.0 {
        println!(
            "minishogi OVERALL: {nodes} nodes verified; mce {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mce_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mce_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Minishogi positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Minishogi parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH minishogi/{} depth {}: mce={} fsf={}  FEN: {}",
                r.label, r.depth, r.mce_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Minishogi position through mce's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Minishogi::from_fen(case.fen).map_err(|e| format!("mce rejected FEN: {e:?}"))?;
    let mce_start = Instant::now();
    let mce_nodes = gperft::<Minishogi5x5, _>(&pos, depth);
    let mce_secs = mce_start.elapsed().as_secs_f64();

    // mce and FSF share the Minishogi FEN dialect, so the FEN is passed verbatim.
    engine.set_variant("minishogi", false)?;
    engine.set_position(case.fen)?;
    let fsf = engine.go_perft(depth, false)?;

    Ok(Row {
        label: case.label,
        fen: case.fen,
        depth,
        mce_nodes,
        fsf_nodes: fsf.nodes,
        matched: mce_nodes == fsf.nodes,
        mce_secs,
        fsf_secs: fsf.elapsed.as_secs_f64(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The corpus FENs all parse on the generic Minishogi engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in
    /// `tests/perft_minishogi.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 181u64),
            ("drops-in-hand", 36),
            ("multi-hand", 10671),
            ("forced-promo", 9),
            ("promo-choice", 11),
            ("midgame", 808),
            ("nifu", 100),
        ];
        for case in CASES {
            let pos = Minishogi::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Minishogi5x5, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }
}
