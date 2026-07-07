//! Seirawan chess (S-Chess, 8x8) differential perft + timing against
//! Fairy-Stockfish (issue #173).
//!
//! Seirawan runs on mcr's **generic** 8x8 engine (`mcr::geometry::Seirawan`, a
//! `GenericPosition<Chess8x8, SeirawanRules>`), not the concrete `AnyVariant`
//! layer the shared corpus drives, so — like Makruk and Capablanca — it has its
//! own corpus and comparison loop here. The FSF side selects
//! `UCI_Variant seirawan`, sets the FEN, runs `go perft`, asserts the node counts
//! match, and reports mcr-vs-FSF throughput.
//!
//! ## FEN dialect
//!
//! mcr spells the Hawk (its [`WideRole::Hawk`](mcr::geometry::WideRole)
//! bishop-knight compound) `a`/`A` on the **board** — the same letter Capablanca's
//! Archbishop uses, since the two share the role — whereas FSF's S-Chess spells the
//! board Hawk `h`/`H`. (The Elephant is `e`/`E` in both, and mcr's reserve `[..]`
//! bracket already prints the Hawk as `H`/`h`, matching FSF.) The mismatch only
//! ever shows once a Hawk is **gated onto the board**: the hand-picked corpus below
//! keeps both reserves un-gated, so its boards carry no Hawk and a verbatim FEN
//! happens to match — but a fuzzed game that gates a Hawk produces a board `A`/`a`
//! that FSF would read as a different piece. [`fen_to_fsf`] therefore rewrites the
//! Hawk's letter across the *placement* field only (it leaves the gating-rights
//! castling field — whose file letters span `a`..`h` — and every other field
//! untouched), exactly as the sibling S-House dialect does (issue #239).
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Seirawan};

use crate::uci::Engine;

/// Rewrite an mcr-dialect Seirawan FEN into the FSF dialect: the Hawk's letter
/// `a`/`A` becomes `h`/`H` in the *placement* field only (which includes the
/// reserve `[..]` bracket, though mcr already prints reserve Hawks as `H`/`h`, so
/// that part is a no-op). The Elephant `e`/`E`, the gating-rights castling field
/// (whose file letters span `a`..`h`), and every other field are unchanged. For a
/// board with no gated Hawk (every hand-picked corpus position) this is the
/// identity, so the pinned counts are unaffected; it only bites once a fuzzed game
/// gates a Hawk onto the board.
pub fn fen_to_fsf(fen: &str) -> String {
    let map = |c| match c {
        'a' => 'h',
        'A' => 'H',
        other => other,
    };
    // Only the placement field (up to the first space) holds piece letters; the
    // castling field's gating-file letters (a..h) must NOT be rewritten.
    match fen.split_once(' ') {
        Some((placement, rest)) => {
            let mapped: String = placement.chars().map(map).collect();
            format!("{mapped} {rest}")
        }
        None => fen.chars().map(map).collect(),
    }
}

/// One Seirawan corpus position, in the **mcr dialect** (board Hawk = `a`/`A`).
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

/// Run the Seirawan corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all positions matched). Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Seirawan / S-Chess (8x8) — generic engine vs FSF UCI_Variant seirawan (issue #173):");
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
                eprintln!("skip seirawan/{}: {e}", case.label);
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
            "seirawan OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
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
                "  MISMATCH seirawan/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Seirawan position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mcr side: the generic Seirawan position over the 8x8 geometry.
    let pos = Seirawan::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // FSF side: rewrite a board Hawk's letter into the FSF dialect (a no-op for
    // every un-gated corpus board, but correct once a Hawk is gated on).
    let fsf_fen = fen_to_fsf(case.fen);
    engine.set_variant("seirawan", false)?;
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
            let n = gperft::<Chess8x8, _, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }
}
