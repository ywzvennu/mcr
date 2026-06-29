//! ASEAN chess differential perft + timing against Fairy-Stockfish (issue #261).
//!
//! ASEAN runs on mce's **generic** engine (`mce::geometry::Asean`, a
//! `GenericPosition<Chess8x8, AseanRules>`), not the concrete `AnyVariant`
//! layer the rest of this harness drives, so it has its own small corpus and
//! comparison loop here (mirroring `makruk.rs`). The FSF side selects
//! `UCI_Variant asean`, sets the FEN, runs `go perft`, and the node counts are
//! asserted equal.
//!
//! Unlike Makruk — whose mce and FSF FEN dialects coincide — ASEAN labels its
//! pieces with the international letters: FSF spells the Khon `b` and the Met
//! `q`, while mce names them `s` (Silver) and `m` (Met). [`fen_to_fsf`] rewrites
//! the placement (`s→b`, `m→q`, case-preserving) before handing the FEN to FSF.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mce::geometry::{perft as gperft, Asean, Chess8x8};

use crate::uci::Engine;

/// Rewrite an mce ASEAN FEN into the Fairy-Stockfish `asean` dialect: the
/// placement field's Khon (`s`/`S`) becomes `b`/`B` and the Met (`m`/`M`)
/// becomes `q`/`Q`. Only the placement field is touched; the rest passes through
/// (ASEAN has no en-passant target and no castling rights, so nothing else needs
/// rewriting).
fn fen_to_fsf(fen: &str) -> String {
    let rewrite_placement = |placement: &str| -> String {
        placement
            .chars()
            .map(|c| match c {
                's' => 'b',
                'S' => 'B',
                'm' => 'q',
                'M' => 'Q',
                other => other,
            })
            .collect()
    };
    match fen.split_once(' ') {
        Some((placement, rest)) => format!("{} {rest}", rewrite_placement(placement)),
        None => rewrite_placement(fen),
    }
}

/// One ASEAN corpus position. `fen` is the mce dialect (`s`/`m`); the FSF side
/// is fed `fen_to_fsf(fen)`.
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The ASEAN comparison corpus: the FSF-confirmed startpos, two midgames, and a
/// promotion-stress position. Depths are modest by default; `full` adds a ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSMKSNR w - - 0 1",
        depth: 5,
    },
    Case {
        label: "midgame-1",
        fen: "rnsmksnr/8/1ppppppp/p7/4P3/PPPP1PPP/8/RNSMKSNR b - - 0 2",
        depth: 5,
    },
    Case {
        label: "midgame-2",
        fen: "r1smks1r/3n4/ppp1pppp/3p4/3P4/PPP1PPPP/4N3/R1SMKS1R w - - 0 4",
        depth: 5,
    },
    Case {
        // Pawns one step / one capture from the last rank: exercises ASEAN's
        // four promotion targets (FSF q/r/b/n) every ply. No `s`/`m` letters, so
        // the FSF rewrite is the identity here.
        label: "promotion",
        fen: "1n2k3/P1P5/8/8/8/8/p1P5/1N2K3 w - - 0 1",
        depth: 5,
    },
];

/// A measured ASEAN comparison row.
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

/// Run the ASEAN corpus through mce and FSF. Returns the number of mismatches
/// (0 = all positions matched). Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("ASEAN chess — generic engine vs FSF UCI_Variant asean (issue #261):");
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
                eprintln!("skip asean/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mce_nodes).sum();
    let mce_s: f64 = rows.iter().map(|r| r.mce_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mce_s > 0.0 && fsf_s > 0.0 {
        println!(
            "asean OVERALL: {nodes} nodes verified; mce {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mce_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mce_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} ASEAN positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} ASEAN parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH asean/{} depth {}: mce={} fsf={}  FEN: {}",
                r.label, r.depth, r.mce_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one ASEAN position through mce's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mce side: the generic ASEAN position (mce `s`/`m` dialect).
    let pos = Asean::from_fen(case.fen).map_err(|e| format!("mce rejected FEN: {e:?}"))?;
    let mce_start = Instant::now();
    let mce_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mce_secs = mce_start.elapsed().as_secs_f64();

    // FSF side: asean is a FSF built-in; rewrite the FEN to FSF's `b`/`q` dialect.
    engine.set_variant("asean", false)?;
    engine.set_position(&fen_to_fsf(case.fen))?;
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

    /// `fen_to_fsf` rewrites the Khon/Met letters and leaves the rest intact.
    #[test]
    fn fen_to_fsf_rewrites_khon_and_met() {
        assert_eq!(
            fen_to_fsf("rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSMKSNR w - - 0 1"),
            "rnbqkbnr/8/pppppppp/8/8/PPPPPPPP/8/RNBQKBNR w - - 0 1"
        );
        // A position with no Khon/Met passes through unchanged.
        assert_eq!(
            fen_to_fsf("1n2k3/P1P5/8/8/8/8/p1P5/1N2K3 w - - 0 1"),
            "1n2k3/P1P5/8/8/8/8/p1P5/1N2K3 w - - 0 1"
        );
    }

    /// The corpus FENs all parse on the generic ASEAN engine, and the pinned
    /// depth-2 counts match the FSF-confirmed numbers in `tests/perft_asean.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 529u64),
            ("midgame-1", 576),
            ("midgame-2", 485),
            ("promotion", 307),
        ];
        for case in CASES {
            let pos = Asean::from_fen(case.fen).expect("corpus FEN parses");
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
