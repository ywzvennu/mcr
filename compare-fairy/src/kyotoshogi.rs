//! Kyoto Shogi (5x5 flipping) differential perft + timing against
//! Fairy-Stockfish (#232).
//!
//! Kyoto Shogi runs on mcr's **generic** `u64` engine
//! (`mcr::geometry::Kyotoshogi`, a `GenericPosition<Minishogi5x5,
//! KyotoshogiRules>`), reusing the 5x5 board, persistent hand, and drops, with one
//! distinctive new mechanic: **every piece flips to its alternate form after each
//! move it makes**. The FSF side selects `UCI_Variant kyotoshogi`, sets the FEN,
//! runs `go perft`, asserts the node counts match, and reports mcr-vs-FSF
//! throughput. The corpus exercises the **per-move flip** (base ↔ promoted forms),
//! **dual-form drops** (`dropPromoted`), and the promoted **sliders** (`+P` Rook,
//! `+S` Bishop).
//!
//! **FSF must be built with large-board support** (`make ... largeboards=yes`):
//! the default FSF build omits the 5x5 `kyotoshogi` variant from its
//! `UCI_Variant` list. When the running binary lacks `kyotoshogi`, this loop skips
//! rather than compare meaningless truncated counts.
//!
//! ## FEN dialect
//!
//! mcr and FSF use the **same** Kyoto Shogi piece letters — `p s l n k` and the
//! `+`-prefixed promoted forms `+P +S +L +N` — and the same `[..]` holdings-bracket
//! convention for the hand. So, like Minishogi, **no FEN rewrite is needed**: the
//! mcr FEN is passed to FSF verbatim.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Kyotoshogi, Minishogi5x5};

use crate::uci::Engine;

/// One Kyoto Shogi corpus position.
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Kyoto Shogi comparison corpus: the FSF-confirmed startpos; a Silver+Pawn
/// dual-form-drop position; a bare-kings position with one of every base droppable
/// role in hand (drops dominate); a board with a promoted Silver (flip of board
/// pieces); and the two-promoted-slider middlegame. Depths are modest by default;
/// `full` deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "p+nks+l/5/5/5/+LSK+NP[] w - - 0 1",
        depth: 5,
    },
    Case {
        label: "drops-in-hand",
        fen: "2k2/5/5/5/2K2[SPsp] w - - 0 1",
        depth: 3,
    },
    Case {
        label: "multi-hand",
        fen: "2k2/5/5/5/2K2[PSLNpsln] w - - 0 1",
        depth: 2,
    },
    Case {
        label: "flip-midgame",
        fen: "p+nks+l/5/2+S2/5/+LSK+NP[] w - - 0 1",
        depth: 4,
    },
    Case {
        label: "promoted-sliders",
        fen: "1k3/5/1+s3/5/1K2+P[] w - - 0 1",
        depth: 4,
    },
];

/// A measured Kyoto Shogi comparison row.
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

/// Run the Kyoto Shogi corpus through mcr and FSF. Returns the number of
/// mismatches (0 = all matched, or FSF lacks `kyotoshogi` and the suite is
/// skipped). Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!(
        "Kyoto Shogi (5x5 flipping, u64, hand + dual-form drops) — generic engine vs FSF \
UCI_Variant kyotoshogi (issue #232):"
    );
    println!("  (requires an FSF built with largeboards=yes)");

    if !engine.has_variant("kyotoshogi") {
        println!("  SKIP: this FSF binary has no `kyotoshogi` variant (build it largeboards=yes).");
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
                eprintln!("skip kyotoshogi/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "kyotoshogi OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Kyoto Shogi positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Kyoto Shogi parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH kyotoshogi/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Kyoto Shogi position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Kyotoshogi::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Minishogi5x5, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // mcr and FSF share the Kyoto Shogi FEN dialect, so the FEN is passed verbatim.
    engine.set_variant("kyotoshogi", false)?;
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

    /// The corpus FENs all parse on the generic Kyoto Shogi engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in
    /// `tests/perft_kyotoshogi.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 137u64),
            ("drops-in-hand", 7665),
            ("multi-hand", 28889),
            ("flip-midgame", 171),
            ("promoted-sliders", 99),
        ];
        for case in CASES {
            let pos = Kyotoshogi::from_fen(case.fen).expect("corpus FEN parses");
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
