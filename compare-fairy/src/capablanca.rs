//! Capablanca chess (10x8) differential perft + timing against Fairy-Stockfish
//! (issue #170).
//!
//! Capablanca runs on mcr's **generic** `u128` engine
//! (`mcr::geometry::Capablanca`, a `GenericPosition<Cap10x8, CapablancaRules>`),
//! not the concrete 8x8 `AnyVariant` layer the rest of this harness drives, so it
//! has its own corpus and comparison loop here (mirroring `makruk.rs`). The FSF
//! side selects `UCI_Variant capablanca`, sets the FEN, runs `go perft`, asserts
//! the node counts match, and reports mcr-vs-FSF throughput.
//!
//! **FSF must be built with large-board support** (`make ... largeboards=yes`):
//! the default FSF build omits the 10x8 `capablanca` variant from its
//! `UCI_Variant` list. When the running binary lacks it, FSF cannot parse the
//! ten-wide FEN and the comparison is skipped for that case.
//!
//! ## FEN dialect
//!
//! mcr and FSF agree on the position but spell the chancellor differently: mcr
//! uses `e`/`E` (its [`WideRole::Elephant`] rook-knight compound), FSF uses
//! `c`/`C`. The archbishop is `a`/`A` in both. [`fen_to_fsf`] rewrites the
//! chancellor's letter; the placement is otherwise byte-identical.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Cap10x8, Capablanca};

use crate::uci::Engine;

/// One Capablanca corpus position, in the **mcr dialect** (chancellor = `e`).
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Capablanca comparison corpus: the FSF-confirmed startpos, a castling-rich
/// position (pins the king f -> i/c, rook j -> h / a -> d geometry), a developed
/// midgame, and a promotion position. Depths are modest by default; `full`
/// deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnabqkbenr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBENR w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "castle",
        fen: "r4k3r/pppppppppp/10/10/10/10/PPPPPPPPPP/R4K3R w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "midgame",
        fen: "1nabqkben1/p1ppppppp1/1r6r1/1p6p1/3PP5/2N4N2/PPP2PPPPP/R1ABQKBE1R w KQ - 0 5",
        depth: 4,
    },
    Case {
        label: "promo",
        fen: "5k4/4P5/10/10/10/10/10/5K4 w - - 0 1",
        depth: 4,
    },
];

/// Rewrite an mcr-dialect Capablanca FEN into the FSF dialect: the chancellor's
/// letter `e`/`E` becomes `c`/`C` in the *placement* field only (the other FEN
/// fields carry no piece letters). The archbishop `a`/`A` is unchanged.
pub fn fen_to_fsf(fen: &str) -> String {
    // Only the placement field (up to the first space) holds piece letters.
    match fen.split_once(' ') {
        Some((placement, rest)) => {
            let mapped: String = placement
                .chars()
                .map(|c| match c {
                    'e' => 'c',
                    'E' => 'C',
                    other => other,
                })
                .collect();
            format!("{mapped} {rest}")
        }
        None => fen
            .chars()
            .map(|c| match c {
                'e' => 'c',
                'E' => 'C',
                other => other,
            })
            .collect(),
    }
}

/// A measured Capablanca comparison row.
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

/// Run the Capablanca corpus through mcr and FSF. Returns the number of
/// mismatches (0 = all positions matched). Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!(
        "Capablanca (10x8, u128) — generic engine vs FSF UCI_Variant capablanca (issue #170):"
    );
    println!("  (requires an FSF built with largeboards=yes)");
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
                eprintln!("skip capablanca/{}: {e}", case.label);
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
            "capablanca OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Capablanca positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Capablanca parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH capablanca/{} depth {}: mcr={} fsf={}  mcr FEN: {}  FSF FEN: {}",
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

/// Run one Capablanca position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mcr side: the generic Capablanca position over the 10x8 u128 geometry.
    let pos = Capablanca::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Cap10x8, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // FSF side: rewrite the chancellor's letter into the FSF dialect.
    let fsf_fen = fen_to_fsf(case.fen);
    engine.set_variant("capablanca", false)?;
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

    /// The corpus FENs all parse on the generic Capablanca engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in
    /// `tests/perft_capablanca.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 784u64),
            ("castle", 961),
            ("midgame", 1866),
            ("promo", 43),
        ];
        for case in CASES {
            let pos = Capablanca::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Cap10x8, _, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }

    /// The mcr -> FSF dialect rewrite swaps only the chancellor's letter and
    /// leaves the archbishop and every other field intact.
    #[test]
    fn fen_dialect_rewrites_only_the_chancellor() {
        let mcr = "rnabqkbenr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBENR w KQkq - 0 1";
        let fsf = "rnabqkbcnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBCNR w KQkq - 0 1";
        assert_eq!(fen_to_fsf(mcr), fsf);
        // The trailing fields are untouched: an `e3` en-passant token stays `e3`
        // (only placement letters are mapped), and the archbishop `a` is left
        // alone while the white chancellor `E` becomes `C`.
        let out = fen_to_fsf("a9/10/10/10/10/10/10/E9 b - e3 1 9");
        assert_eq!(out, "a9/10/10/10/10/10/10/C9 b - e3 1 9");
    }
}
