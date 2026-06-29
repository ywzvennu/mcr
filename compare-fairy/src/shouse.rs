//! S-House / Seirawan-house (8x8) differential perft + timing against
//! Fairy-Stockfish (issue #264) — **Seirawan gating + crazyhouse drops**.
//!
//! S-House runs on mce's **generic** 8x8 engine (`mce::geometry::Shouse`, a
//! `GenericPosition<Chess8x8, ShouseRules>`), like Seirawan. The FSF side selects
//! the built-in `UCI_Variant shouse`, sets the FEN, runs `go perft`, asserts the
//! node counts match, and reports mce-vs-FSF throughput.
//!
//! ## FEN dialect
//!
//! As with Capablanca/Capahouse, mce spells the Hawk (its
//! [`WideRole::Hawk`](mce::geometry::WideRole) bishop-knight compound) `a`/`A`
//! where FSF's S-House uses `H`/`h`; the Elephant is `e`/`E` in both. [`fen_to_fsf`]
//! rewrites the Hawk's letter across the *placement* field only (which carries the
//! crazyhouse `[..]` hand bracket too, so a Hawk in hand is mapped as well); the
//! gating rights in the castling field — whose file letters span `a`..`h` — and
//! every other field are left untouched.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mce::geometry::{perft as gperft, Chess8x8, Shouse};

use crate::uci::Engine;

/// One S-House corpus position, in the **mce dialect** (Hawk = `a`/`A`).
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The S-House comparison corpus: the FSF-confirmed startpos, a midgame
/// exercising both mechanics at once (a captured Knight in each hand alongside the
/// starting Hawk/Elephant, so drops are live and a back-rank piece's first move
/// may gate the Knight/Hawk/Elephant), and a promotion/demotion position (a
/// captured promoted queen banks a Pawn — the `~` mask). Depths are modest by
/// default (the unified hand inflates the branching factor); `full` deepens by one
/// ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[AEae] w KQBCDFGkqbcdfg - 0 1",
        depth: 4,
    },
    Case {
        label: "drops_gates",
        fen: "r1bqk2r/ppp2ppp/2n5/3pp3/3PP3/2N5/PPP2PPP/R1BQK2R[EANean] w KQCDkqcd - 0 1",
        depth: 3,
    },
    Case {
        label: "promoted",
        fen: "Q~r1k4/8/8/8/8/8/8/4K3[] b - - 0 1",
        depth: 4,
    },
];

/// Rewrite an mce-dialect S-House FEN into the FSF dialect: the Hawk's letter
/// `a`/`A` becomes `h`/`H` in the *placement* field only (which includes the
/// crazyhouse `[..]` hand bracket). The Elephant `e`/`E`, the promoted `~` marker,
/// the gating-rights castling field, and every other field are unchanged.
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

/// A measured S-House comparison row.
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

/// Run the S-House corpus through mce and FSF. Returns the number of mismatches
/// (0 = all positions matched). Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!(
        "S-House / Seirawan-house (8x8) — generic engine vs FSF UCI_Variant shouse (issue #264):"
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
                eprintln!("skip shouse/{}: {e}", case.label);
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
            "shouse OVERALL: {nodes} nodes verified; mce {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mce_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mce_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} S-House positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} S-House parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH shouse/{} depth {}: mce={} fsf={}  mce FEN: {}  FSF FEN: {}",
                r.label,
                r.depth,
                r.mce_nodes,
                r.fsf_nodes,
                r.fen,
                fen_to_fsf(r.fen),
            );
        }
    }
    mismatches
}

/// Run one S-House position through mce's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mce side: the generic S-House position over the 8x8 geometry.
    let pos = Shouse::from_fen(case.fen).map_err(|e| format!("mce rejected FEN: {e:?}"))?;
    let mce_start = Instant::now();
    let mce_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mce_secs = mce_start.elapsed().as_secs_f64();

    // FSF side: rewrite the Hawk's letter into the FSF dialect.
    let fsf_fen = fen_to_fsf(case.fen);
    engine.set_variant("shouse", false)?;
    engine.set_position(&fsf_fen)?;
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

    /// The corpus FENs all parse on the generic S-House engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in
    /// `tests/perft_shouse.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 7944u64),
            ("drops_gates", 36942),
            ("promoted", 126),
        ];
        for case in CASES {
            let pos = Shouse::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Chess8x8, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }

    /// The dialect rewrite maps the Hawk and leaves the gating castling field
    /// (whose file letters span `a`..`h`) untouched.
    #[test]
    fn fen_to_fsf_maps_hawk_only() {
        assert_eq!(
            fen_to_fsf("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[AEae] w KQBCDFGkqbcdfg - 0 1"),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[HEhe] w KQBCDFGkqbcdfg - 0 1",
        );
    }
}
