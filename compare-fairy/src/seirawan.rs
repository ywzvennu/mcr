//! Seirawan chess (S-Chess, 8x8) differential perft + timing against
//! Fairy-Stockfish (issue #173).
//!
//! Seirawan runs on mce's **generic** 8x8 engine (`mce::geometry::Seirawan`, a
//! `GenericPosition<Chess8x8, SeirawanRules>`), not the concrete `AnyVariant`
//! layer the shared corpus drives, so — like Makruk and Capablanca — it has its
//! own corpus and comparison loop here. The FSF side selects
//! `UCI_Variant seirawan`, sets the FEN, runs `go perft`, asserts the node counts
//! match, and reports mce-vs-FSF throughput.
//!
//! ## FEN dialect
//!
//! mce uses the **same dialect** Fairy-Stockfish does for S-Chess: the Hawk is
//! `H`/`h`, the Elephant `E`/`e`, the reserves in hand ride in a `[..]` bracket
//! after the placement, and the gating rights fold into the castling field
//! (`KQBCDFGkqbcdfg`-style). So a Seirawan FEN is byte-identical between the two
//! engines — there is no rewrite step (contrast Capablanca's chancellor letter).
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mce::geometry::{perft as gperft, Chess8x8, Seirawan};

use crate::uci::Engine;

/// One Seirawan corpus position (mce == FSF dialect).
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Seirawan comparison corpus: the FSF-confirmed startpos (depth-4 count
/// `782599` is pinned in FSF's own `tests/perft.sh`), a midgame with both
/// reserves still in hand, a position whose castle may itself gate, and a
/// developed position with a partial reserve and pieces already gated in. Depths
/// are modest by default (gating inflates the branching factor); `full` deepens
/// by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[HEhe] w KQBCDFGkqbcdfg - 0 1",
        depth: 4,
    },
    Case {
        label: "mid_gating",
        fen: "r1bqkb1r/pppppppp/2n2n2/8/8/2N2N2/PPPPPPPP/R1BQKB1R[HEhe] w KQBCDEFGkqbcdefg - 4 3",
        depth: 4,
    },
    Case {
        label: "castle_gate",
        fen: "rnbqk2r/pppppppp/8/8/8/5N2/PPPPPPBP/RNBQK2R[HEhe] w KQkqABCDFGabcdfgh - 0 1",
        depth: 3,
    },
    Case {
        label: "partial",
        fen: "reb1k2r/pppp1ppp/2nbqn2/4p3/4P3/2NBQN2/PPPP1PPP/R1B1K2R[Hh] w KQkqABCDFGabcdfg - 8 6",
        depth: 3,
    },
];

/// A measured Seirawan comparison row.
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

/// Run the Seirawan corpus through mce and FSF. Returns the number of mismatches
/// (0 = all positions matched). Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Seirawan / S-Chess (8x8) — generic engine vs FSF UCI_Variant seirawan (issue #173):");
    let head = format!(
        "{:<12} {:>5} {:>14} {:>14} {:>9} {:>10} {:>10} {:>8}",
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
                    "{:<12} {:>5} {:>14} {:>14} {:>9} {:>10.1} {:>10.1} {:>7.2}x",
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
                eprintln!("skip seirawan/{}: {e}", case.label);
            }
        }
    }

    // Node-weighted aggregate throughput.
    let nodes: u64 = rows.iter().map(|r| r.mce_nodes).sum();
    let mce_s: f64 = rows.iter().map(|r| r.mce_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mce_s > 0.0 && fsf_s > 0.0 {
        println!(
            "seirawan OVERALL: {nodes} nodes verified; mce {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mce_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mce_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Seirawan positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Seirawan parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH seirawan/{} depth {}: mce={} fsf={}  FEN: {}",
                r.label, r.depth, r.mce_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Seirawan position through mce's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mce side: the generic Seirawan position over the 8x8 geometry.
    let pos = Seirawan::from_fen(case.fen).map_err(|e| format!("mce rejected FEN: {e:?}"))?;
    let mce_start = Instant::now();
    let mce_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mce_secs = mce_start.elapsed().as_secs_f64();

    // FSF side: the FEN is the same dialect, sent verbatim.
    engine.set_variant("seirawan", false)?;
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

    /// The corpus FENs all parse on the generic Seirawan engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in
    /// `tests/perft_seirawan.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 784u64),
            ("mid_gating", 780),
            ("castle_gate", 1402),
            ("partial", 2151),
        ];
        for case in CASES {
            let pos = Seirawan::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Chess8x8, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }
}
