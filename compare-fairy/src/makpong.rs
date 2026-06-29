//! Makpong ("Defensive Chess") differential perft + timing against
//! Fairy-Stockfish (issue #260).
//!
//! Makpong is Makruk with one extra rule — while in check the king may not flee
//! (it may move only to capture the lone checker) — so it rides the same
//! **generic** engine (`mce::geometry::Makpong`, a
//! `GenericPosition<Chess8x8, MakpongRules>`) as Makruk, not the concrete
//! `AnyVariant` layer the rest of this harness drives. It therefore has its own
//! small corpus and comparison loop here, mirroring `makruk.rs`. The FSF side
//! selects the built-in `UCI_Variant makpong`, sets the FEN, runs `go perft`, and
//! asserts the node counts match.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mce::geometry::{perft as gperft, Chess8x8, Makpong};

use crate::uci::Engine;

/// One Makpong corpus position. The FEN is the same dialect FSF parses
/// (`UCI_Variant makpong`), so no rewrite is needed.
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Makpong comparison corpus: the FSF-confirmed startpos plus two root
/// in-check positions (where the king-may-not-flee rule bites at the first ply)
/// and a quiet midgame (which diverges from Makruk inside the tree). Depths are
/// kept modest by default; `full` deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w - - 0 1",
        depth: 5,
    },
    Case {
        label: "check-rook",
        fen: "rnsmksnr/8/1pp1ppp1/p6p/3r4/PPP1PPPP/8/RNSK1SNR w - - 0 4",
        depth: 5,
    },
    Case {
        label: "check-pawn",
        fen: "rnsmksnr/8/ppp1ppp1/7p/8/PP1PPPPP/2pP4/RNSK1SNR w - - 0 5",
        depth: 4,
    },
    Case {
        label: "midgame",
        fen: "r1smks1r/3n4/ppp1pppp/3p4/3P4/PPP1PPPP/4N3/R1SKMS1R w - - 0 4",
        depth: 5,
    },
];

/// A measured Makpong comparison row.
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

/// Run the Makpong corpus through mce and FSF. Returns the number of mismatches
/// (0 = all positions matched). Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Makpong (Defensive Chess) — generic engine vs FSF UCI_Variant makpong (issue #260):");
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
                eprintln!("skip makpong/{}: {e}", case.label);
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
            "makpong OVERALL: {nodes} nodes verified; mce {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mce_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mce_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Makpong positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Makpong parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH makpong/{} depth {}: mce={} fsf={}  FEN: {}",
                r.label, r.depth, r.mce_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Makpong position through mce's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mce side: the generic Makpong position.
    let pos = Makpong::from_fen(case.fen).map_err(|e| format!("mce rejected FEN: {e:?}"))?;
    let mce_start = Instant::now();
    let mce_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mce_secs = mce_start.elapsed().as_secs_f64();

    // FSF side: makpong uses the same FEN dialect, no rewrite needed.
    engine.set_variant("makpong", false)?;
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

    /// The corpus FENs all parse on the generic Makpong engine, and the pinned
    /// depth-2 counts match the FSF-confirmed numbers in `tests/perft_makpong.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 529u64),
            ("check-rook", 128),
            ("check-pawn", 48),
            ("midgame", 508),
        ];
        for case in CASES {
            let pos = Makpong::from_fen(case.fen).expect("corpus FEN parses");
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
