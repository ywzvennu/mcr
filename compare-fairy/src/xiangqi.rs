//! Xiangqi (9x10) differential perft + timing against Fairy-Stockfish (issue
//! #187).
//!
//! Xiangqi runs on mcr's **generic** `u128` engine (`mcr::geometry::Xiangqi`, a
//! `GenericPosition<Xiangqi9x10, XiangqiRules>`), not the concrete 8x8
//! `AnyVariant` layer the rest of this harness drives, so it has its own corpus
//! and comparison loop here (mirroring `shako.rs`). The FSF side selects
//! `UCI_Variant xiangqi`, sets the FEN, runs `go perft`, asserts the node counts
//! match, and reports mcr-vs-FSF throughput. The corpus exercises the **cannon**
//! over-screen captures, the **horse** hobbling-leg, the **elephant** eye blocks,
//! the **soldier** river crossing, the **palace** confinement, and a
//! **flying-general** pin.
//!
//! **FSF must be built with large-board support** (`make ... largeboards=yes`):
//! the default FSF build omits the 9x10 `xiangqi` variant from its `UCI_Variant`
//! list (only the 7x7 `minixiangqi` is present). When the running binary lacks
//! `xiangqi`, this loop skips rather than compare meaningless truncated counts.
//!
//! ## FEN dialect
//!
//! mcr and FSF agree on the position but spell four pieces differently: FSF uses
//! `a n b p` for the Advisor / Horse / Elephant / Soldier, but those letters
//! already name the Hawk / Knight / Bishop / Pawn in mcr's `WideRole`, so mcr
//! spells them `u j o z`. [`fen_to_fsf`] rewrites those letters in the placement
//! field only; the chariots (`r`) and cannons (`c`) are unchanged.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Xiangqi, Xiangqi9x10};

use crate::uci::Engine;

/// One Xiangqi corpus position, in the **mcr dialect**.
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Xiangqi comparison corpus: the FSF-confirmed startpos; a horse/cannon
/// middlegame; a cannon over-screen general capture (kingless enumeration); an
/// elephant-eye + soldier-river clash; and a flying-general pin. Depths are
/// modest by default; `full` deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/9/RJOUKUOJR w - - 0 1",
        depth: 3,
    },
    Case {
        label: "middlegame",
        fen: "r1oukuo1r/9/1cj3jc1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1CJ3JC1/9/R1OUKUO1R w - - 0 1",
        depth: 3,
    },
    Case {
        label: "cannon-cap",
        fen: "rjoukuojr/9/4c4/z1z1C1z1z/9/9/Z1Z3Z1Z/1C5c1/9/RJOUKUOJR b - - 0 1",
        depth: 3,
    },
    Case {
        label: "elephant-eye",
        fen: "r1oukuo1r/9/1c2c4/z3z3z/2z3z2/2Z3Z2/Z3Z3Z/1C2C4/9/R1OUKUO1R w - - 0 1",
        depth: 3,
    },
    Case {
        label: "flying-general",
        fen: "4k4/9/9/9/9/9/9/9/4R4/4K4 w - - 0 1",
        depth: 4,
    },
    // Horse gives check (issue #198): a black horse on e3 checks the white general
    // on d1 with its leg e2 empty. The old `attackers_to` reverse-projection missed
    // this check; FSF and mcr now agree (perft 3 = 14, 4 = 50).
    Case {
        label: "horse-check",
        fen: "4k4/9/9/9/9/9/9/4j4/3U5/3K5 w - - 0 1",
        depth: 4,
    },
    // Soldier guards the square ahead (issue #201): a white soldier on e7 guards
    // e8, so the black general on d8 may step only to d9. The old `attackers_to`
    // reverse-projected the Soldier's color-directional forward attack without the
    // color flip and missed the guard; FSF and mcr now agree (perft 1 = 1, 4 = 53).
    Case {
        label: "soldier-guard-fwd",
        fen: "9/9/3k5/4Z4/9/9/9/9/9/4K4 b - - 0 1",
        depth: 4,
    },
    // Crossed soldier guards sideways (issue #201, post-river): a white soldier on
    // e8 has crossed the river and guards d8/f8 sideways as well as e9 forward, so
    // the black general on d9 may step only to d10. A plain color-flipped reverse-
    // projection flips the color-dependent river threshold and misses the crossed
    // soldier; forward projection via `role_attack_is_leg_asymmetric` fixes it.
    // FSF and mcr now agree (perft 1 = 1, 4 = 26).
    Case {
        label: "soldier-guard-side",
        fen: "9/3k5/4Z4/9/9/9/9/9/9/4K4 b - - 0 1",
        depth: 4,
    },
];

/// Rewrite an mcr-dialect Xiangqi FEN into the FSF dialect: the Advisor `u`/`U`,
/// Horse `j`/`J`, Elephant `o`/`O`, and Soldier `z`/`Z` become `a n b p` (with
/// case preserved) in the *placement* field only. The chariot `r`/`R` and cannon
/// `c`/`C` are unchanged.
pub fn fen_to_fsf(fen: &str) -> String {
    let map = |c: char| match c {
        'u' => 'a',
        'U' => 'A',
        'j' => 'n',
        'J' => 'N',
        'o' => 'b',
        'O' => 'B',
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

/// A measured Xiangqi comparison row.
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

/// Run the Xiangqi corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all positions matched, or FSF lacks `xiangqi` and the suite is skipped).
/// Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!(
        "Xiangqi (9x10, u128, cannons + palace/river) — generic engine vs FSF \
UCI_Variant xiangqi (issue #187):"
    );
    println!("  (requires an FSF built with largeboards=yes)");

    if !engine.has_variant("xiangqi") {
        println!("  SKIP: this FSF binary has no `xiangqi` variant (build it largeboards=yes).");
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
                eprintln!("skip xiangqi/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "xiangqi OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Xiangqi positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Xiangqi parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH xiangqi/{} depth {}: mcr={} fsf={}  mcr FEN: {}  FSF FEN: {}",
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

/// Run one Xiangqi position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Xiangqi::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Xiangqi9x10, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    let fsf_fen = fen_to_fsf(case.fen);
    engine.set_variant("xiangqi", false)?;
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

    /// The corpus FENs all parse on the generic Xiangqi engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_xiangqi.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 1920u64),
            ("middlegame", 1292),
            ("cannon-cap", 373),
            ("elephant-eye", 1066),
            ("flying-general", 16),
            ("horse-check", 5),
            ("soldier-guard-fwd", 5),
            ("soldier-guard-side", 5),
        ];
        for case in CASES {
            let pos = Xiangqi::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Xiangqi9x10, _, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }

    /// The mcr -> FSF dialect rewrite swaps only the four Xiangqi piece letters in
    /// the placement field and leaves the chariot, cannon, and every other field
    /// intact.
    #[test]
    fn fen_dialect_rewrites_only_the_xiangqi_pieces() {
        let mcr = "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/9/RJOUKUOJR w - - 0 1";
        let fsf = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1";
        assert_eq!(fen_to_fsf(mcr), fsf);
        // The cannon `C`/`c` and side-to-move field are untouched.
        let out = fen_to_fsf("4k4/9/9/9/9/9/9/9/4R4/4K4 b - - 1 9");
        assert_eq!(out, "4k4/9/9/9/9/9/9/9/4R4/4K4 b - - 1 9");
    }
}
