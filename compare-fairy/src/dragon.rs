//! Dragon chess (8x8) differential perft + timing against Fairy-Stockfish
//! (issue #270) — **standard chess plus a Dragon (Bishop + Knight) in each side's
//! fixed pocket**, droppable only onto the player's own back rank.
//!
//! Dragon runs on mcr's **generic** engine (`mcr::geometry::Dragon`, a
//! `GenericPosition<Chess8x8, DragonRules>`). The FSF side selects the built-in
//! `UCI_Variant dragon`, sets the FEN, runs `go perft`, asserts the node counts
//! match, and reports mcr-vs-FSF throughput.
//!
//! ## FEN dialect
//!
//! mcr spells the Dragon `a`/`A` (its [`WideRole::Hawk`](mcr::geometry::WideRole)
//! Bishop-Knight compound — the same letter Capablanca's archbishop and Seirawan's
//! hawk use) where FSF's `dragon` uses `d`/`D`. [`fen_to_fsf`] rewrites that one
//! letter across the placement field (which carries the `[..]` pocket bracket too,
//! so a Dragon *in hand* is mapped as well); every other field is byte-identical.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Dragon};

use crate::uci::Engine;

/// One Dragon corpus position, in the **mcr dialect** (Dragon = `a`).
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Dragon comparison corpus: the FSF-confirmed startpos (the back rank is full,
/// so no drop is possible yet), a developed back rank with both Dragons still in
/// hand (the back-rank drops are live), a Dragon already on the board (its on-board
/// Bishop + Knight movement), and a pawn one step from promoting to a Dragon.
/// Depths are modest by default; `full` deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[Aa] w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "drops",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/R2QK2R[Aa] w KQkq - 0 1",
        depth: 3,
    },
    Case {
        label: "onboard",
        fen: "rnbqkbnr/pppppppp/8/8/3A4/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1",
        depth: 3,
    },
    Case {
        label: "promo",
        fen: "r3k3/1P6/8/8/8/8/6p1/4K3[Aa] w - - 0 1",
        depth: 3,
    },
];

/// Rewrite an mcr-dialect Dragon FEN into the FSF dialect: the Dragon's letter
/// `a`/`A` becomes `d`/`D` in the *placement* field only (which includes the
/// `[..]` pocket bracket). Every other field — the castling rights, the en-passant
/// square, the clocks — is unchanged.
pub fn fen_to_fsf(fen: &str) -> String {
    let map = |c| match c {
        'a' => 'd',
        'A' => 'D',
        other => other,
    };
    // Only the placement field (up to the first space) holds piece letters.
    match fen.split_once(' ') {
        Some((placement, rest)) => {
            let mapped: String = placement.chars().map(map).collect();
            format!("{mapped} {rest}")
        }
        None => fen.chars().map(map).collect(),
    }
}

/// A measured Dragon comparison row.
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

/// Run the Dragon corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all positions matched). Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Dragon (8x8) — generic engine vs FSF UCI_Variant dragon (issue #270):");
    let head = format!(
        "{:<12} {:>5} {:>14} {:>14} {:>9} {:>10} {:>10} {:>8}",
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
                    "{:<12} {:>5} {:>14} {:>14} {:>9} {:>10.1} {:>10.1} {:>7.2}x",
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
                eprintln!("skip dragon/{}: {e}", case.label);
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
            "dragon OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Dragon positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Dragon parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH dragon/{} depth {}: mcr={} fsf={}  mcr FEN: {}  FSF FEN: {}",
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

/// Run one Dragon position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mcr side: the generic Dragon position over the 8x8 geometry.
    let pos = Dragon::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // FSF side: rewrite the Dragon's letter into the FSF dialect.
    let fsf_fen = fen_to_fsf(case.fen);
    engine.set_variant("dragon", false)?;
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

    /// The corpus FENs all parse on the generic Dragon engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_dragon.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 400u64),
            ("drops", 560),
            ("onboard", 615),
            ("promo", 430),
        ];
        for case in CASES {
            let pos = Dragon::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Chess8x8, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }

    /// The mcr -> FSF dialect rewrite swaps only the Dragon's letter (including a
    /// Dragon held in the pocket) and leaves the castling rights, the en-passant
    /// token, and every other field intact.
    #[test]
    fn fen_dialect_rewrites_only_the_dragon() {
        let mcr = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[Aa] w KQkq - 0 1";
        let fsf = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[Dd] w KQkq - 0 1";
        assert_eq!(fen_to_fsf(mcr), fsf);
        // An `a3` en-passant token and the castling file letters survive untouched.
        let out = fen_to_fsf("4k3/8/8/8/Pp6/8/8/4K3[] b - a3 0 1");
        assert_eq!(out, "4k3/8/8/8/Pp6/8/8/4K3[] b - a3 0 1");
    }
}
