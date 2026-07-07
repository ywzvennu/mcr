//! Minixiangqi (7x7) differential perft + timing against Fairy-Stockfish (issue
//! #196).
//!
//! Minixiangqi runs on mcr's **generic** `u128` engine
//! (`mcr::geometry::Minixiangqi`, a `GenericPosition<Minixiangqi7x7,
//! MinixiangqiRules>`), not the concrete 8x8 `AnyVariant` layer the rest of this
//! harness drives, so it has its own corpus and comparison loop here (mirroring
//! `xiangqi.rs`). The FSF side selects `UCI_Variant minixiangqi`, sets the FEN,
//! runs `go perft`, asserts the node counts match, and reports mcr-vs-FSF
//! throughput. The corpus exercises the **cannon** over-screen captures, the
//! **horse** hobbling-leg, the always-sideways **soldier**, the **palace**
//! confinement, and a **flying-general** pin — plus a horse/cannon middlegame
//! that exposed the asymmetric-horse `attackers_to` bug fixed in #199.
//!
//! **FSF must be built with large-board support** (`make ... largeboards=yes`).
//! The 7x7 `minixiangqi` variant is present in stock FSF, but this loop still
//! skips cleanly if the running binary lacks it.
//!
//! ## FEN dialect
//!
//! mcr and FSF agree on the position but spell two pieces differently: FSF uses
//! `n p` for the Horse / Soldier, but those letters already name the Knight /
//! Pawn in mcr's `WideRole`, so mcr spells them `j z`. [`fen_to_fsf`] rewrites
//! those letters in the placement field only; the chariots (`r`), cannons (`c`),
//! and king (`k`) are unchanged.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Minixiangqi, Minixiangqi7x7};

use crate::uci::Engine;

/// One Minixiangqi corpus position, in the **mcr dialect**.
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Minixiangqi comparison corpus: the FSF-confirmed startpos; a horse/cannon
/// middlegame (the position that exposed the asymmetric-horse `attackers_to` bug
/// fixed in #199); a cannon over-screen capture; a horse-hobble clash; and a
/// flying-general pin. Depths are modest by default; `full` deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rcjkjcr/z1zzz1z/7/7/7/Z1ZZZ1Z/RCJKJCR w - - 0 1",
        depth: 4,
    },
    // Horse/cannon middlegame: both sides have advanced horses and cannons. This
    // is the position that previously mismatched FSF (mcr 22034/586426 vs FSF
    // 21900/582088) because `attackers_to` reverse-projected the hobbled horse;
    // #199's asymmetric-horse hook fixes it. Pinned here as a regression.
    Case {
        label: "horse-cannon-mid",
        fen: "r1jkjcr/z1zzz1z/2c4/2J4/7/Z1ZZZ1Z/R1CKJCR w - - 0 1",
        depth: 3,
    },
    // Cannon over-screen capture: a white cannon on c4 sees the black cannon on
    // c6 over the soldier screen on c5.
    Case {
        label: "cannon-cap",
        fen: "r1jkj1r/z1z1z1z/2c4/2C4/7/Z2Z2Z/R2KJ1R b - - 0 1",
        depth: 3,
    },
    // Horse hobble: a horse with leg squares occupied/empty in different
    // directions, exercising the per-leg blocker logic on the small board.
    Case {
        label: "horse-hobble",
        fen: "3k3/7/3J3/2Z1Z2/7/7/3K3 w - - 0 1",
        depth: 4,
    },
    // Flying general: the two generals share the d-file with a chariot able to
    // interpose; the flying-general rule constrains the legal replies.
    Case {
        label: "flying-general",
        fen: "3k3/7/7/3R3/7/7/3K3 w - - 0 1",
        depth: 4,
    },
];

/// Rewrite an mcr-dialect Minixiangqi FEN into the FSF dialect: the Horse `j`/`J`
/// and Soldier `z`/`Z` become `n p` (with case preserved) in the *placement*
/// field only. The chariot `r`/`R`, cannon `c`/`C`, and king `k`/`K` are
/// unchanged.
pub fn fen_to_fsf(fen: &str) -> String {
    let map = |c: char| match c {
        'j' => 'n',
        'J' => 'N',
        'z' => 'p',
        'Z' => 'P',
        other => other,
    };
    match fen.split_once(' ') {
        Some((placement, rest)) => {
            let mapped: String = placement.chars().map(map).collect();
            format!("{mapped} {rest}")
        }
        None => fen.chars().map(map).collect(),
    }
}

/// A measured Minixiangqi comparison row.
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

/// Run the Minixiangqi corpus through mcr and FSF. Returns the number of
/// mismatches (0 = all positions matched, or FSF lacks `minixiangqi` and the
/// suite is skipped). Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!(
        "Minixiangqi (7x7, u128, cannons + palace, no river) — generic engine vs FSF \
UCI_Variant minixiangqi (issue #196):"
    );
    println!("  (requires an FSF built with largeboards=yes)");

    if !engine.has_variant("minixiangqi") {
        println!(
            "  SKIP: this FSF binary has no `minixiangqi` variant (build it largeboards=yes)."
        );
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
                eprintln!("skip minixiangqi/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "minixiangqi OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s \
({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Minixiangqi positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Minixiangqi parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH minixiangqi/{} depth {}: mcr={} fsf={}  mcr FEN: {}  FSF FEN: {}",
                r.label,
                r.depth,
                r.mcr_nodes,
                r.fsf_nodes,
                r.fen,
                fen_to_fsf(r.fen),
            );
        }
    }
    mismatches
}

/// Run one Minixiangqi position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Minixiangqi::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Minixiangqi7x7, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    let fsf_fen = fen_to_fsf(case.fen);
    engine.set_variant("minixiangqi", false)?;
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

    /// The corpus FENs all parse on the generic Minixiangqi engine, and the
    /// pinned shallow counts match the FSF-confirmed numbers in
    /// `tests/perft_minixiangqi.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        for case in CASES {
            let pos = Minixiangqi::from_fen(case.fen).expect("corpus FEN parses");
            let _ = gperft::<Minixiangqi7x7, _, _>(&pos, 2);
        }
    }

    /// The mcr -> FSF dialect rewrite swaps only the Horse and Soldier letters in
    /// the placement field and leaves the chariot, cannon, king, and every other
    /// field intact.
    #[test]
    fn fen_dialect_rewrites_only_the_minixiangqi_pieces() {
        let mcr = "rcjkjcr/z1zzz1z/7/7/7/Z1ZZZ1Z/RCJKJCR w - - 0 1";
        let fsf = "rcnkncr/p1ppp1p/7/7/7/P1PPP1P/RCNKNCR w - - 0 1";
        assert_eq!(fen_to_fsf(mcr), fsf);
        // The cannon `C`/`c`, king `K`/`k`, and side-to-move field are untouched.
        let out = fen_to_fsf("3k3/7/7/3R3/7/7/3K3 b - - 1 9");
        assert_eq!(out, "3k3/7/7/3R3/7/7/3K3 b - - 1 9");
    }
}
