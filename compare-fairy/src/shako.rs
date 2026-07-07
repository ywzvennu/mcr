//! Shako (10x10) differential perft + timing against Fairy-Stockfish (issue
//! #184).
//!
//! Shako runs on mcr's **generic** `u128` engine (`mcr::geometry::Shako`, a
//! `GenericPosition<Grand10x10, ShakoRules>`), not the concrete 8x8 `AnyVariant`
//! layer the rest of this harness drives, so it has its own corpus and comparison
//! loop here (mirroring `grand.rs`). The FSF side selects `UCI_Variant shako`,
//! sets the FEN, runs `go perft`, asserts the node counts match, and reports
//! mcr-vs-FSF throughput. The corpus deliberately exercises the **cannon**:
//! captures over a screen, and checks delivered over a screen (the
//! screen-dependent king-safety path).
//!
//! **FSF must be built with large-board support** (`make ... largeboards=yes`):
//! the default FSF build omits the 10x10 `shako` variant from its `UCI_Variant`
//! list. When the running binary lacks it, FSF parses the ten-wide FEN as plain
//! chess (silently truncating to 8x8) and the counts diverge — meaningless — so
//! this loop checks `shako` is in the variant list first and skips if not.
//!
//! ## FEN dialect
//!
//! mcr and FSF agree on the position but spell the **elephant** differently: FSF
//! uses `e`/`E` (its Fers-Alfil), but mcr already uses `e`/`E` for the Rook+Knight
//! Elephant (the Capablanca/Grand marshal), so the Shako elephant takes the free
//! letter `v`/`V` ([`WideRole::FersAlfil`](mcr::geometry::WideRole::FersAlfil)).
//! The cannon is `c`/`C` in both. [`fen_to_fsf`] rewrites the elephant's letter;
//! the placement is otherwise byte-identical.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Grand10x10, Shako};

use crate::uci::Engine;

/// One Shako corpus position, in the **mcr dialect** (elephant = `v`).
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Shako comparison corpus: the FSF-confirmed startpos; a developed midgame
/// with cannons off the back rank and pawn screens (cannon captures over a screen
/// at depth); a position with **white in check from a black cannon** over a
/// screen; the same kind of check **down a file** (black to move); a
/// castling-legal position on rank 2; and a promotion to the full Shako set
/// (including cannon and elephant). Depths are modest by default; `full` deepens
/// by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "c8c/vrnbqkbnrv/pppppppppp/10/10/10/10/PPPPPPPPPP/VRNBQKBNRV/C8C w KQkq - 0 1",
        depth: 3,
    },
    Case {
        label: "midgame",
        fen: "c8c/vr1bqkbnrv/pp1ppppppp/2p7/2C4c2/3P6/10/PP2PPPPPP/VRNBQKBNRV/C8C w - - 0 1",
        depth: 3,
    },
    Case {
        label: "cannon-check",
        fen: "c8c/vrnbqkbnrv/pp2pppppp/2pp6/10/2C2c4/3P6/PP2PPPPPP/VRNBQKBNRV/C8C w - - 0 1",
        depth: 3,
    },
    Case {
        label: "cannon-check-file",
        fen: "c8c/vrnbqkbnrv/ppppp1pppp/10/10/5p4/5C4/PPPPP1PPPP/VRNBQKBNRV/C8C b KQkq - 0 1",
        depth: 3,
    },
    Case {
        label: "castling",
        fen: "c8c/vr3k2rv/pppppppppp/10/10/10/10/PPPPPPPPPP/VR3K2RV/C8C w KQkq - 0 1",
        depth: 3,
    },
    Case {
        label: "promo",
        fen: "5k4/1P8/10/10/10/10/10/10/10/5K3C w - - 0 1",
        depth: 4,
    },
];

/// Rewrite an mcr-dialect Shako FEN into the FSF dialect: the elephant's letter
/// `v`/`V` becomes `e`/`E` in the *placement* field only (the other FEN fields
/// carry no piece letters). The cannon `c`/`C` is unchanged.
pub fn fen_to_fsf(fen: &str) -> String {
    let map = |c: char| match c {
        'v' => 'e',
        'V' => 'E',
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

/// A measured Shako comparison row.
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

/// Run the Shako corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all positions matched, or FSF lacks `shako` and the suite is skipped).
/// Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!(
        "Shako (10x10, u128, cannons) — generic engine vs FSF UCI_Variant shako (issue #184):"
    );
    println!("  (requires an FSF built with largeboards=yes)");

    if !engine.has_variant("shako") {
        println!("  SKIP: this FSF binary has no `shako` variant (build it largeboards=yes).");
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
                eprintln!("skip shako/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "shako OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Shako positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Shako parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH shako/{} depth {}: mcr={} fsf={}  mcr FEN: {}  FSF FEN: {}",
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

/// Run one Shako position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Shako::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Grand10x10, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    let fsf_fen = fen_to_fsf(case.fen);
    engine.set_variant("shako", false)?;
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

    /// The corpus FENs all parse on the generic Shako engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_shako.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 3364u64),
            ("midgame", 4555),
            ("cannon-check", 134),
            ("cannon-check-file", 398),
            ("castling", 2916),
            ("promo", 111),
        ];
        for case in CASES {
            let pos = Shako::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Grand10x10, _, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }

    /// The mcr -> FSF dialect rewrite swaps only the elephant's letter and leaves
    /// the cannon and every other field intact.
    #[test]
    fn fen_dialect_rewrites_only_the_elephant() {
        let mcr = "c8c/vrnbqkbnrv/pppppppppp/10/10/10/10/PPPPPPPPPP/VRNBQKBNRV/C8C w KQkq - 0 1";
        let fsf = "c8c/ernbqkbnre/pppppppppp/10/10/10/10/PPPPPPPPPP/ERNBQKBNRE/C8C w KQkq - 0 1";
        assert_eq!(fen_to_fsf(mcr), fsf);
        // The cannon `C`/`c` is untouched, and the trailing fields are left alone.
        let out = fen_to_fsf("V9/10/10/10/10/10/10/10/10/c8C b - - 1 9");
        assert_eq!(out, "E9/10/10/10/10/10/10/10/10/c8C b - - 1 9");
    }
}
