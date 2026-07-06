//! Grasshopper chess differential perft + timing against Fairy-Stockfish.
//!
//! Grasshopper chess runs on mcr's **generic** engine (`mcr::geometry::Grasshopper`,
//! a `GenericPosition<Chess8x8, GrasshopperRules>`), not the concrete `AnyVariant`
//! layer the rest of this harness drives, so it has its own small corpus and
//! comparison loop here (mirroring `legan.rs`). The FSF side selects
//! `UCI_Variant grasshopper` (a built-in), sets the FEN, runs `go perft`, and the
//! node counts are asserted equal. The corpus deliberately exercises the
//! grasshopper: quiet hops over a hurdle, captures beyond a hurdle, and a check
//! delivered over a hurdle (the hurdle-dependent king-safety verify path).
//!
//! ## FEN dialect
//!
//! mcr and FSF agree on the position but spell the **grasshopper** differently: FSF
//! uses `g`/`G`, but mcr's bare `g`/`G` names the Gold and every overflow `g` slot
//! is taken, so mcr spells the grasshopper with the fourth-tier overflow token
//! `***j`/`***J` ([`WideRole::Grasshopper`](mcr::geometry::WideRole::Grasshopper)).
//! [`fen_to_fsf`] rewrites that token to `g`/`G`; the placement is otherwise
//! byte-identical, and Grasshopper chess has no double step / en passant so the ep
//! field is always `-`.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no GPL
//! code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Grasshopper};

use crate::uci::Engine;

/// One Grasshopper corpus position, in the **mcr dialect** (grasshopper = `***j`).
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Grasshopper comparison corpus: the FSF-confirmed startpos; a developed
/// midgame with the central grasshoppers hopped to e4 / e5 (dense quiet hops and
/// over-pawn captures); a White-in-check position (a black grasshopper checks over
/// a pawn hurdle, exercising the move-king / interpose / move-the-hurdle evasions);
/// and a capture-beyond-a-hurdle position. Depths are modest by default; `full`
/// deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnbqkbnr/***j***j***j***j***j***j***j***j/pppppppp/8/8/PPPPPPPP/***J***J***J***J***J***J***J***J/RNBQKBNR w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "midgame",
        fen: "rnbqkbnr/***j***j***j***j***j***j1***j/pppppppp/4***j3/4***J3/PPPPPPPP/***J***J***J***J***J***J1***J/RNBQKBNR w KQkq - 2 2",
        depth: 3,
    },
    Case {
        label: "check",
        fen: "4k3/8/8/8/4***j3/8/4P3/4K3 w - - 0 1",
        depth: 4,
    },
    Case {
        label: "capture",
        fen: "4k3/8/3r4/3P4/3***J4/8/8/4K3 w - - 0 1",
        depth: 4,
    },
];

/// Rewrite an mcr-dialect Grasshopper FEN into the FSF dialect: the grasshopper's
/// fourth-tier overflow token `***j` / `***J` becomes the bare `g` / `G` in the
/// *placement* field (the only field carrying piece letters; `***` never occurs
/// elsewhere in these FENs). Every other letter and field is left intact.
pub fn fen_to_fsf(fen: &str) -> String {
    let (placement, rest) = match fen.split_once(' ') {
        Some((p, r)) => (p, Some(r)),
        None => (fen, None),
    };
    let mut out = String::with_capacity(placement.len());
    let mut chars = placement.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '*' && chars.peek() == Some(&'*') {
            // A `***X` overflow token: consume the two remaining `*`, then the base
            // letter `j`/`J` (the Grasshopper) becomes FSF's `g`/`G`, case-preserved.
            chars.next();
            chars.next();
            match chars.next() {
                Some('j') => out.push('g'),
                Some('J') => out.push('G'),
                Some(base) => out.push(base),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    match rest {
        Some(r) => format!("{out} {r}"),
        None => out,
    }
}

/// A measured Grasshopper comparison row.
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

/// Run the Grasshopper corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all positions matched). Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Grasshopper chess — generic engine vs FSF UCI_Variant grasshopper:");
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
                eprintln!("skip grasshopper/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "grasshopper OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Grasshopper positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Grasshopper parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH grasshopper/{} depth {}: mcr={} fsf={}  mcr FEN: {}  FSF FEN: {}",
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

/// Run one Grasshopper position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Grasshopper::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    let fsf_fen = fen_to_fsf(case.fen);
    engine.set_variant("grasshopper", false)?;
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

    /// The corpus FENs all parse on the generic Grasshopper engine, and the pinned
    /// depth-2 counts match the FSF-confirmed numbers in `tests/perft_grasshopper.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 782u64),
            ("midgame", 811),
            ("check", 30),
            ("capture", 80),
        ];
        for case in CASES {
            let pos = Grasshopper::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Chess8x8, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }

    /// The mcr -> FSF dialect rewrite swaps only the grasshopper's `***j`/`***J`
    /// token for `g`/`G` and leaves every other letter and field intact.
    #[test]
    fn fen_dialect_rewrites_only_the_grasshopper() {
        let mcr = "4k3/8/8/8/4***j3/8/4P3/4K3 w - - 0 1";
        let fsf = "4k3/8/8/8/4g3/8/4P3/4K3 w - - 0 1";
        assert_eq!(fen_to_fsf(mcr), fsf);
        // Uppercase (white) grasshopper, and the trailing fields, are handled too.
        assert_eq!(
            fen_to_fsf("3***J4/8/8/8/8/8/8/4K1k1 b - - 1 9"),
            "3G4/8/8/8/8/8/8/4K1k1 b - - 1 9"
        );
    }
}
