//! Cambodian chess / Ouk Chaktrang differential perft + timing against
//! Fairy-Stockfish (issue #234).
//!
//! Cambodian runs on mce's **generic** engine (`mce::geometry::Cambodian`, a
//! `GenericPosition<Chess8x8, CambodianRules>`), not the concrete `AnyVariant`
//! layer the rest of this harness drives, so it has its own small corpus and
//! comparison loop here — the same shape as the Makruk harness. The FSF side
//! selects `UCI_Variant cambodian`, sets the FEN, runs `go perft`, asserts the
//! node counts match, and reports mce-vs-FSF throughput.
//!
//! The corpus exercises the one-time king / Met leaps: the startpos (king leaps
//! live), an open midgame with both leaps reachable, the same board with the
//! leap rights spent (pure Makruk move generation), and a second open midgame.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mce::geometry::{perft as gperft, Cambodian, Chess8x8};

use crate::uci::Engine;

/// One Cambodian corpus position. The FEN is the same dialect FSF parses
/// (`UCI_Variant cambodian`) — including the `DEde` leap-rights field — so no
/// rewrite is needed.
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Cambodian comparison corpus, all FSF-confirmed. Depths are kept modest by
/// default; `full` deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w DEde - 0 1",
        depth: 5,
    },
    Case {
        label: "leaps-mid",
        fen: "rnsmksnr/8/pp1ppp1p/2p2p2/2P2P2/PP1PPP1P/8/RNSKMSNR w DEde - 0 3",
        depth: 4,
    },
    Case {
        label: "leaps-spent",
        fen: "rnsmksnr/8/pp1ppp1p/2p2p2/2P2P2/PP1PPP1P/8/RNSKMSNR w - - 0 3",
        depth: 4,
    },
    Case {
        label: "open-mid",
        fen: "rnsmksnr/8/1ppppppp/p7/7P/PPPPPPP1/8/RNSKMSNR w DEde - 0 2",
        depth: 4,
    },
];

/// A measured Cambodian comparison row.
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

/// Run the Cambodian corpus through mce and FSF. Returns the number of
/// mismatches (0 = all positions matched). Prints a table and a one-line
/// summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!(
        "Cambodian (Ouk Chaktrang) — generic engine vs FSF UCI_Variant cambodian (issue #234):"
    );
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
                eprintln!("skip cambodian/{}: {e}", case.label);
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
            "cambodian OVERALL: {nodes} nodes verified; mce {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mce_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mce_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Cambodian positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Cambodian parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH cambodian/{} depth {}: mce={} fsf={}  FEN: {}",
                r.label, r.depth, r.mce_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Cambodian position through mce's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mce side: the generic Cambodian position.
    let pos = Cambodian::from_fen(case.fen).map_err(|e| format!("mce rejected FEN: {e:?}"))?;
    let mce_start = Instant::now();
    let mce_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mce_secs = mce_start.elapsed().as_secs_f64();

    // FSF side: cambodian uses the same FEN dialect, no rewrite needed.
    engine.set_variant("cambodian", false)?;
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

    /// The corpus FENs all parse on the generic Cambodian engine, and the pinned
    /// depth-2 counts match the FSF-confirmed numbers in
    /// `tests/perft_cambodian.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 625u64),
            ("leaps-mid", 532),
            ("leaps-spent", 444),
            ("open-mid", 729),
        ];
        for case in CASES {
            let pos = Cambodian::from_fen(case.fen).expect("corpus FEN parses");
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
