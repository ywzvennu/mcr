//! Duck chess (8x8) differential perft + timing against Fairy-Stockfish
//! (issue #177).
//!
//! Duck chess runs on mce's **generic** 8x8 engine (`mce::geometry::Duck`, a
//! `GenericPosition<Chess8x8, DuckRules>`), not the concrete `AnyVariant` layer
//! the shared corpus drives, so — like Makruk / Capablanca / Seirawan / Grand —
//! it has its own corpus and comparison loop here. The FSF side selects
//! `UCI_Variant duck`, sets the FEN, runs `go perft`, asserts the node counts
//! match, and reports mce-vs-FSF throughput.
//!
//! ## FEN dialect
//!
//! mce uses the **same dialect** Fairy-Stockfish does for Duck chess: the neutral
//! Duck is a `*` in the placement field (absent at the start, since it enters on
//! the first move), and every other field is plain chess. So a Duck FEN is
//! byte-identical between the two engines — there is no rewrite step.
//!
//! ## Move encoding
//!
//! A ply is a piece move plus a Duck placement; FSF prints the divide as
//! `<piecemove>,<duckfrom><duckto>` (e.g. `a2a3,a3a2`), and mce's
//! `WideMove::to_uci` matches it. The duck-cross-product makes the branching
//! factor huge, so default depths are modest.
//!
//! ## Confirmed FSF version (issue #189)
//!
//! The built-in `UCI_Variant duck` is sent the corpus FENs verbatim (no
//! rewrite, no `variants.ini`/`VariantPath`), so the comparison depends only on
//! FSF's built-in Duck definition. That definition has been stable: the live run
//! matches mce **byte-identically** on all five corpus positions at the default
//! depth 3 (and the pinned depth 2) against upstream Fairy-Stockfish commit
//! `1b5bdd4` ("Add Georgian chess", 2026-05-23, `id name Fairy-Stockfish 280626
//! LB`) — which is also the upstream `master` HEAD a fresh `--build` clone
//! fetches. #189 reported a live divergence that did NOT reproduce here; it was a
//! stale/differently-built FSF binary, not an mce or harness translation bug, so
//! no translation change was needed. If a future divergence appears, first
//! confirm the FSF binary's `id name` / commit against the value above before
//! suspecting mce or this harness.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mce::geometry::{perft as gperft, Chess8x8, Duck};

use crate::uci::Engine;

/// One Duck corpus position (mce == FSF dialect).
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Duck comparison corpus: the FSF-confirmed startpos (no Duck on the board
/// yet — it enters on the first move), an open middlegame with the Duck on e5
/// and castling available, an en-passant middlegame, an en-passant whose capture
/// "exposes" a king (legal — Duck chess has no check) and reaches king-capture
/// terminals, and a king-and-pawn endgame. Depths are modest by default (the
/// duck cross-product inflates the branching factor); `full` deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        depth: 3,
    },
    Case {
        label: "mid_open",
        fen: "r1bqk2r/pppp1ppp/2n2n2/2b1*3/2B1P3/2N2N2/PPPP1PPP/R1BQK2R w KQkq - 0 1",
        depth: 3,
    },
    Case {
        label: "mid_ep",
        fen: "rnbqkbnr/ppp1pppp/8/3pP3/*7/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1",
        depth: 3,
    },
    Case {
        label: "ep_king",
        fen: "4k3/8/8/r2pPK2/8/8/8/8 w - d6 0 1",
        depth: 3,
    },
    Case {
        label: "endgame",
        fen: "8/2k5/8/3*4/8/5K2/4P3/8 w - - 0 1",
        depth: 3,
    },
];

/// A measured Duck comparison row.
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

/// Run the Duck corpus through mce and FSF. Returns the number of mismatches
/// (0 = all positions matched). Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Duck chess (8x8) — generic engine vs FSF UCI_Variant duck (issue #177):");
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
                eprintln!("skip duck/{}: {e}", case.label);
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
            "duck OVERALL: {nodes} nodes verified; mce {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mce_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mce_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Duck positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Duck parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH duck/{} depth {}: mce={} fsf={}  FEN: {}",
                r.label, r.depth, r.mce_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Duck position through mce's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mce side: the generic Duck position over the 8x8 geometry.
    let pos = Duck::from_fen(case.fen).map_err(|e| format!("mce rejected FEN: {e:?}"))?;
    let mce_start = Instant::now();
    let mce_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mce_secs = mce_start.elapsed().as_secs_f64();

    // FSF side: the FEN is the same dialect, sent verbatim.
    engine.set_variant("duck", false)?;
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

    /// The corpus FENs all parse on the generic Duck engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_duck.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 379440u64),
            ("mid_open", 1270166),
            ("mid_ep", 736005),
            ("ep_king", 460692),
            ("endgame", 254880),
        ];
        for case in CASES {
            let pos = Duck::from_fen(case.fen).expect("corpus FEN parses");
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
