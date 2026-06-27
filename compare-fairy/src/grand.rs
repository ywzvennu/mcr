//! Grand chess (10x10) differential perft + timing against Fairy-Stockfish
//! (issue #175).
//!
//! Grand runs on mce's **generic** `u128` engine (`mce::geometry::Grand`, a
//! `GenericPosition<Grand10x10, GrandRules>`), not the concrete 8x8 `AnyVariant`
//! layer the rest of this harness drives, so it has its own corpus and comparison
//! loop here (mirroring `capablanca.rs`). The FSF side selects
//! `UCI_Variant grand`, sets the FEN, runs `go perft`, asserts the node counts
//! match, and reports mce-vs-FSF throughput.
//!
//! **FSF must be built with large-board support** (`make ... largeboards=yes`):
//! the default FSF build omits the 10x10 `grand` variant from its `UCI_Variant`
//! list. When the running binary lacks it, FSF parses the ten-wide FEN as plain
//! chess (silently truncating to 8x8) and the counts diverge — the comparison is
//! then meaningless, so this loop checks `grand` is in the variant list first and
//! skips with a clear message if not.
//!
//! ## FEN dialect
//!
//! mce and FSF agree on the position but spell the marshal differently: mce uses
//! `e`/`E` (its [`WideRole::Elephant`](mce::geometry::WideRole::Elephant)
//! rook-knight compound), FSF uses `c`/`C` (its chancellor). The cardinal is
//! `a`/`A` in both. [`fen_to_fsf`] rewrites the marshal's letter; the placement
//! is otherwise byte-identical.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mce::geometry::{perft as gperft, Grand, Grand10x10};

use crate::uci::Engine;

/// One Grand corpus position, in the **mce dialect** (marshal = `e`).
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Grand comparison corpus: the FSF-confirmed startpos, a developed midgame,
/// and a promote-to-captured position (white restricted to promoting to a rook or
/// bishop, exercising the three-rank zone and the captured-type rule). Depths are
/// modest by default; `full` deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "r8r/1nbqkeabn1/pppppppppp/10/10/10/10/PPPPPPPPPP/1NBQKEABN1/R8R w - - 0 1",
        depth: 3,
    },
    Case {
        label: "midgame",
        fen: "r8r/2bqkeab2/pppp1ppppp/2n4n2/3Np5/3P6/7N2/PPP1PPPPPP/2BQKEAB2/R8R b - - 1 4",
        depth: 3,
    },
    Case {
        label: "promo",
        fen: "4k5/8P1/10/10/10/10/10/10/10/RNBQK1EAN1 w - - 0 1",
        depth: 3,
    },
];

/// Rewrite an mce-dialect Grand FEN into the FSF dialect: the marshal's letter
/// `e`/`E` becomes `c`/`C` in the *placement* field only (the other FEN fields
/// carry no piece letters). The cardinal `a`/`A` is unchanged.
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

/// A measured Grand comparison row.
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

/// Run the Grand corpus through mce and FSF. Returns the number of mismatches
/// (0 = all positions matched, or FSF lacks `grand` and the suite is skipped).
/// Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Grand chess (10x10, u128) — generic engine vs FSF UCI_Variant grand (issue #175):");
    println!("  (requires an FSF built with largeboards=yes)");

    // A default (non-largeboard) FSF build silently treats the ten-wide FEN as
    // 8x8 chess; without `grand` in its variant list a comparison is meaningless,
    // so skip cleanly rather than report spurious mismatches.
    if !engine.has_variant("grand") {
        println!("  SKIP: this FSF binary has no `grand` variant (build it largeboards=yes).");
        return 0;
    }

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
                eprintln!("skip grand/{}: {e}", case.label);
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
            "grand OVERALL: {nodes} nodes verified; mce {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mce_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mce_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Grand positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Grand parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH grand/{} depth {}: mce={} fsf={}  mce FEN: {}  FSF FEN: {}",
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

/// Run one Grand position through mce's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mce side: the generic Grand position over the 10x10 u128 geometry.
    let pos = Grand::from_fen(case.fen).map_err(|e| format!("mce rejected FEN: {e:?}"))?;
    let mce_start = Instant::now();
    let mce_nodes = gperft::<Grand10x10, _>(&pos, depth);
    let mce_secs = mce_start.elapsed().as_secs_f64();

    // FSF side: rewrite the marshal's letter into the FSF dialect.
    let fsf_fen = fen_to_fsf(case.fen);
    engine.set_variant("grand", false)?;
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

    /// The corpus FENs all parse on the generic Grand engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_grand.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [("startpos", 4225u64), ("midgame", 5385), ("promo", 221)];
        for case in CASES {
            let pos = Grand::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Grand10x10, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }

    /// The mce -> FSF dialect rewrite swaps only the marshal's letter and leaves
    /// the cardinal and every other field intact.
    #[test]
    fn fen_dialect_rewrites_only_the_marshal() {
        let mce = "r8r/1nbqkeabn1/pppppppppp/10/10/10/10/PPPPPPPPPP/1NBQKEABN1/R8R w - - 0 1";
        let fsf = "r8r/1nbqkcabn1/pppppppppp/10/10/10/10/PPPPPPPPPP/1NBQKCABN1/R8R w - - 0 1";
        assert_eq!(fen_to_fsf(mce), fsf);
        // The trailing fields are untouched: an `e3` en-passant token stays `e3`
        // (only placement letters are mapped), the cardinal `a` is left alone, and
        // the white marshal `E` becomes `C`.
        let out = fen_to_fsf("a9/10/10/10/10/10/10/10/10/E9 b - e3 1 9");
        assert_eq!(out, "a9/10/10/10/10/10/10/10/10/C9 b - e3 1 9");
    }
}
