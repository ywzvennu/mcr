//! Manchu (yipaisanxianqi, 9x10) differential perft + timing against
//! Fairy-Stockfish (issue #230).
//!
//! Manchu is an **asymmetric Xiangqi**: one side keeps a full Xiangqi army, the
//! other replaces its rook/cannon/horse cluster with a single SUPER-PIECE — the
//! **Banner** (Rook + Cannon + Horse combined). It runs on mcr's **generic**
//! `u128` engine (`mcr::geometry::Manchu`, a `GenericPosition<Xiangqi9x10,
//! ManchuRules>`), reusing the entire Xiangqi rule layer, so it has its own corpus
//! and comparison loop here (mirroring `xiangqi.rs`). The FSF side selects
//! `UCI_Variant manchu`, sets the FEN, runs `go perft`, asserts the node counts
//! match, and reports mcr-vs-FSF throughput. The corpus exercises the Banner's
//! rook slide, cannon over-screen captures, and hobbled horse leaps, plus the full
//! Black Xiangqi army and the Banner's cannon-check / rook-check detection.
//!
//! **FSF must be built with large-board support** (`make ... largeboards=yes`):
//! the default FSF build omits the 9x10 `manchu` variant from its `UCI_Variant`
//! list. When the running binary lacks `manchu`, this loop skips rather than
//! compare meaningless truncated counts.
//!
//! ## FEN dialect
//!
//! mcr and FSF agree on the position but spell pieces differently: FSF uses
//! `a n b p` for the Advisor / Horse / Elephant / Soldier (mcr spells them
//! `u j o z`, those letters already naming the Hawk / Knight / Bishop / Pawn) and
//! `m` for the Banner (mcr spells it the overflow token `*m`, FSF's `m` already
//! naming the Makruk Met in mcr). [`fen_to_fsf`] rewrites those tokens in the
//! placement field only; the chariots (`r`) and cannons (`c`) are unchanged.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Manchu, Xiangqi9x10};

use crate::uci::Engine;

/// One Manchu corpus position, in the **mcr dialect**.
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Manchu comparison corpus (mcr dialect): the FSF-confirmed startpos; the
/// Banner centred (rook/cannon/horse in the open); Black's full Xiangqi army to
/// move; the Banner deep in enemy territory (many captures); a Banner
/// cannon-checkmate (over-screen cannon check); and a Banner rook-check. Depths
/// are modest by default; `full` deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/9/9/*M1OUKUO2 w - - 0 1",
        depth: 3,
    },
    Case {
        label: "banner-center",
        fen: "rjoukuojr/9/1c5c1/z1z1z1z1z/9/4*M4/Z1Z1Z1Z1Z/9/9/2OUKUO2 w - - 0 1",
        depth: 3,
    },
    Case {
        label: "black-to-move",
        fen: "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/9/9/*M1OUKUO2 b - - 0 1",
        depth: 3,
    },
    Case {
        label: "banner-deep",
        fen: "rjoukuojr/4*M4/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/9/9/2OUKUO2 w - - 0 1",
        depth: 3,
    },
    Case {
        label: "cannon-mate",
        fen: "k8/9/9/9/z8/9/9/9/9/*M3K4 b - - 0 1",
        depth: 3,
    },
    Case {
        label: "rook-check",
        fen: "4k4/9/9/9/9/9/9/9/9/4*M4 b - - 0 1",
        depth: 3,
    },
];

/// Rewrite an mcr-dialect Manchu FEN into the FSF dialect: the Advisor `u`/`U`,
/// Horse `j`/`J`, Elephant `o`/`O`, and Soldier `z`/`Z` become `a n b p` (case
/// preserved), and the Banner overflow token `*M`/`*m` becomes the bare `M`/`m`,
/// in the *placement* field only. The chariot `r`/`R` and cannon `c`/`C` are
/// unchanged.
pub fn fen_to_fsf(fen: &str) -> String {
    let rewrite_placement = |placement: &str| -> String {
        // Strip the `*` off the Banner overflow token first (`*M → M`, `*m → m`),
        // then map the four Xiangqi letters. The Banner's base letter `m` is not
        // one of the mapped Xiangqi letters, so order is independent, but doing the
        // strip first keeps the per-char map simple.
        let stripped = placement.replace("*M", "M").replace("*m", "m");
        stripped
            .chars()
            .map(|c| match c {
                'u' => 'a',
                'U' => 'A',
                'j' => 'n',
                'J' => 'N',
                'o' => 'b',
                'O' => 'B',
                'z' => 'p',
                'Z' => 'P',
                other => other,
            })
            .collect()
    };
    match fen.split_once(' ') {
        Some((placement, rest)) => {
            let mapped = rewrite_placement(placement);
            format!("{mapped} {rest}")
        }
        None => rewrite_placement(fen),
    }
}

/// A measured Manchu comparison row.
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

/// Run the Manchu corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all positions matched, or FSF lacks `manchu` and the suite is skipped).
/// Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!(
        "Manchu (9x10, u128, asymmetric Xiangqi + Banner super-piece) — generic \
engine vs FSF UCI_Variant manchu (issue #230):"
    );
    println!("  (requires an FSF built with largeboards=yes)");

    if !engine.has_variant("manchu") {
        println!("  SKIP: this FSF binary has no `manchu` variant (build it largeboards=yes).");
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
                eprintln!("skip manchu/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "manchu OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Manchu positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Manchu parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH manchu/{} depth {}: mcr={} fsf={}  mcr FEN: {}  FSF FEN: {}",
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

/// Run one Manchu position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Manchu::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Xiangqi9x10, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    let fsf_fen = fen_to_fsf(case.fen);
    engine.set_variant("manchu", false)?;
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

    /// The corpus FENs all parse on the generic Manchu engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_manchu.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 860u64),
            ("banner-center", 684),
            ("black-to-move", 855),
            ("banner-deep", 533),
            ("cannon-mate", 0),
            ("rook-check", 42),
        ];
        for case in CASES {
            let pos = Manchu::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Xiangqi9x10, _, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }

    /// The mcr -> FSF dialect rewrite strips the `*` off the Banner token, maps the
    /// four Xiangqi piece letters, and leaves the chariot, cannon, and every other
    /// field intact.
    #[test]
    fn fen_dialect_rewrites_banner_and_xiangqi_pieces() {
        let mcr = "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/9/9/*M1OUKUO2 w - - 0 1";
        let fsf = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/9/9/M1BAKAB2 w - - 0 1";
        assert_eq!(fen_to_fsf(mcr), fsf);
        // The cannon `C`/`c` and side-to-move field are untouched; a black Banner
        // `*m` strips to `m`.
        let out = fen_to_fsf("4k4/9/9/9/9/9/9/9/9/4*m4 b - - 1 9");
        assert_eq!(out, "4k4/9/9/9/9/9/9/9/9/4m4 b - - 1 9");
    }
}
