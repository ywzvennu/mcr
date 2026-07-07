//! Janggi (Korean chess, 9x10) differential perft + timing against Fairy-Stockfish
//! (issue #205).
//!
//! Janggi runs on mcr's **generic** `u128` engine (`mcr::geometry::Janggi`, a
//! `GenericPosition<Xiangqi9x10, JanggiRules>`), reusing the Xiangqi 9x10
//! geometry. The FSF side selects `UCI_Variant janggi`, sets the FEN, runs `go
//! perft`, asserts the node counts match, and reports mcr-vs-FSF throughput. The
//! corpus exercises the **screen-cannon** (incl. screen-is-cannon and
//! target-is-cannon, both forbidden), the **cannon palace-diagonal jump**, the
//! **palace diagonals** (general / guard / chariot), the **long blockable
//! elephant**, the **sideways soldier** and its forward palace diagonal, and the
//! **pass** (legal vs forbidden-in-check).
//!
//! **FSF must be built with large-board support** (`make ... largeboards=yes`):
//! the default FSF build omits the 9x10 `janggi` variant from its `UCI_Variant`
//! list. When the running binary lacks `janggi`, this loop skips rather than
//! compare meaningless truncated counts.
//!
//! ## FEN dialect
//!
//! mcr and FSF agree on the position but spell four pieces differently: FSF uses
//! `a n b p` for the Guard / Horse / Elephant / Soldier, but those letters name
//! the Hawk / Knight / Bishop / Pawn in mcr's `WideRole`, so mcr spells them
//! `u j x z` (the Xiangqi elephant already took `o`, so the Janggi elephant is
//! `x`). [`fen_to_fsf`] rewrites those letters in the placement field only; the
//! chariots (`r`) and cannons (`c`) are unchanged.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Janggi, Xiangqi9x10};

use crate::uci::Engine;

/// One Janggi corpus position, in the **mcr dialect**.
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Janggi comparison corpus: the FSF-confirmed startpos plus one position per
/// distinctive mechanic. Depths are modest by default; `full` deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rjxu1uxjr/4k4/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/4K4/RJXU1UXJR w - - 0 1",
        depth: 4,
    },
    Case {
        label: "screen-cannon",
        fen: "9/1k7/r1r3c2/9/9/9/J1C3J2/9/4K4/C1C3C2 w - - 0 1",
        depth: 3,
    },
    Case {
        label: "cannon-palace-diag",
        fen: "9/1k7/9/9/9/9/9/3K1r3/4J4/3C5 w - - 0 1",
        depth: 4,
    },
    Case {
        label: "palace-diagonals",
        fen: "9/4k4/9/9/9/9/9/3U5/9/3K1R3 w - - 0 1",
        depth: 4,
    },
    Case {
        label: "long-elephant",
        fen: "9/4k4/9/9/4Z4/3ZX4/9/9/9/1K7 w - - 0 1",
        depth: 4,
    },
    Case {
        label: "soldier-side-diag",
        fen: "5k3/9/3Z5/9/9/4Z4/9/9/9/1K7 w - - 0 1",
        depth: 4,
    },
    Case {
        label: "pass-legal",
        fen: "9/1k7/9/9/9/9/4z4/9/4K4/9 w - - 0 1",
        depth: 3,
    },
    Case {
        label: "in-check-no-pass",
        fen: "9/1k7/9/9/9/9/9/4z4/4K4/9 w - - 0 1",
        depth: 3,
    },
];

/// Rewrite an mcr-dialect Janggi FEN into the FSF dialect: the Guard `u`/`U`,
/// Horse `j`/`J`, Elephant `x`/`X`, and Soldier `z`/`Z` become `a n b p` (with
/// case preserved) in the *placement* field only. The chariot `r`/`R` and cannon
/// `c`/`C` are unchanged.
pub fn fen_to_fsf(fen: &str) -> String {
    let map = |c: char| match c {
        'u' => 'a',
        'U' => 'A',
        'j' => 'n',
        'J' => 'N',
        'x' => 'b',
        'X' => 'B',
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

/// A measured Janggi comparison row.
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

/// Run the Janggi corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all positions matched, or FSF lacks `janggi` and the suite is skipped).
/// Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!(
        "Janggi (9x10, u128, screen-cannon + palace diagonals + pass) — generic \
engine vs FSF UCI_Variant janggi (issue #205):"
    );
    println!("  (requires an FSF built with largeboards=yes)");

    if !engine.has_variant("janggi") {
        println!("  SKIP: this FSF binary has no `janggi` variant (build it largeboards=yes).");
        return 0;
    }

    let head = format!(
        "{:<20} {:>5} {:>14} {:>14} {:>9} {:>10} {:>10} {:>8}",
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
                    "{:<20} {:>5} {:>14} {:>14} {:>9} {:>10.1} {:>10.1} {:>7.2}x",
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
                eprintln!("skip janggi/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "janggi OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Janggi positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Janggi parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH janggi/{} depth {}: mcr={} fsf={}  mcr FEN: {}  FSF FEN: {}",
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

/// Run one Janggi position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Janggi::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Xiangqi9x10, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    let fsf_fen = fen_to_fsf(case.fen);
    engine.set_variant("janggi", false)?;
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

    /// The corpus FENs all parse on the generic Janggi engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_janggi.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 1024u64),
            ("screen-cannon", 660),
            ("cannon-palace-diag", 20),
            ("palace-diagonals", 124),
            ("long-elephant", 87),
            ("soldier-side-diag", 23),
            ("pass-legal", 32),
            ("in-check-no-pass", 21),
        ];
        for case in CASES {
            let pos = Janggi::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Xiangqi9x10, _, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }

    /// The mcr -> FSF dialect rewrite swaps only the four Janggi piece letters in
    /// the placement field and leaves the chariot, cannon, and every other field
    /// intact.
    #[test]
    fn fen_dialect_rewrites_only_the_janggi_pieces() {
        let mcr = "rjxu1uxjr/4k4/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/4K4/RJXU1UXJR w - - 0 1";
        let fsf = "rnba1abnr/4k4/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/4K4/RNBA1ABNR w - - 0 1";
        assert_eq!(fen_to_fsf(mcr), fsf);
        // The cannon `C`/`c` and side-to-move field are untouched.
        let out = fen_to_fsf("9/1k7/9/9/9/9/9/4z4/4K4/9 w - - 1 9");
        assert_eq!(out, "9/1k7/9/9/9/9/9/4p4/4K4/9 w - - 1 9");
    }
}
