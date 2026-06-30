//! Shatranj (medieval chess) differential perft + timing against Fairy-Stockfish
//! (issue #262).
//!
//! Shatranj runs on mce's **generic** engine (`mce::geometry::Shatranj`, a
//! `GenericPosition<Chess8x8, ShatranjRules>`), not the concrete `AnyVariant`
//! layer the rest of this harness drives, so it has its own small corpus and
//! comparison loop here (like Makruk). `shatranj` is an FSF **built-in** (no
//! `variants.ini` needed): select `UCI_Variant shatranj`, set the FEN, run `go
//! perft`, assert the node counts match, and report mce-vs-FSF throughput.
//!
//! ## FEN dialect
//!
//! mce and FSF render the same Shatranj position with different piece letters.
//! FSF's `shatranj` uses `b` for the Alfil (elephant) and `q` for the Ferz
//! (counselor); mce reuses `b`/`q` for its Bishop/Queen, so the Ferz takes the
//! Makruk Met `m` and the Alfil — past the exhausted single-letter alphabet — the
//! `*`-prefixed overflow token `*x`. [`to_fsf_dialect`] maps mce's letters
//! (`*x → b`, `m → q`, both cases) back to FSF's over the whole FEN. The Rook /
//! Knight / King / Pawn carry none of the remapped letters, so the swap is
//! unambiguous. The comparison asserts only node counts, so the move-string
//! dialect never matters.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mce::geometry::{perft as gperft, Chess8x8, Shatranj};

use crate::uci::Engine;

/// One Shatranj corpus position. The FEN is mce's dialect; the FSF side
/// translates it via [`to_fsf_dialect`].
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Shatranj comparison corpus: the FSF-confirmed startpos, an all-Alfils
/// middlegame (the two-diagonal jump), a Ferz-and-knight middlegame, and a
/// bared-king endgame exercising the baring-loss truncation (FSF reports it
/// terminal, so `go perft` is 0). Depths are kept modest by default; `full`
/// deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rn*xkm*xnr/pppppppp/8/8/8/8/PPPPPPPP/RN*XKM*XNR w - - 0 1",
        depth: 5,
    },
    Case {
        label: "mid-alfils",
        fen: "rn1km1nr/pppppppp/3*x*x3/8/8/3*X*X3/PPPPPPPP/RN1KM1NR w - - 4 3",
        depth: 4,
    },
    Case {
        label: "mid-ferzes",
        fen: "r1*xk1*x1r/pppmpppp/2np1n2/8/8/2NPP3/PPPM1PPP/R1*XK1*XNR w - - 3 5",
        depth: 4,
    },
    Case {
        label: "bared-loss",
        fen: "4k3/8/8/2P1P3/3*X4/2P1P3/8/4K3 w - - 0 1",
        depth: 4,
    },
];

/// Translates an mce-dialect Shatranj FEN to FSF's dialect: Alfil `*x → b`, Ferz
/// `m → q` (both cases), over the whole FEN. The Rook / Knight / King / Pawn carry
/// none of these letters, so the swap is safe.
///
/// The Alfil is an mce **overflow** role: its token is the two characters `*x`
/// (white `*X`), so it is collapsed to FSF's single `b`/`B` before the per-char
/// swap. No other mce token starts with `*`, so a plain sequence replace is safe.
pub(crate) fn to_fsf_dialect(fen: &str) -> String {
    fen.replace("*X", "B")
        .replace("*x", "b")
        .chars()
        .map(|c| match c {
            'm' => 'q',
            'M' => 'Q',
            other => other,
        })
        .collect()
}

/// A measured Shatranj comparison row.
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

/// Run the Shatranj corpus through mce and FSF. Returns the number of mismatches
/// (0 = all positions matched). Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!(
        "Shatranj (medieval chess) — generic engine vs FSF UCI_Variant shatranj (issue #262):"
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
                eprintln!("skip shatranj/{}: {e}", case.label);
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
            "shatranj OVERALL: {nodes} nodes verified; mce {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mce_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mce_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Shatranj positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Shatranj parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH shatranj/{} depth {}: mce={} fsf={}  FEN: {}",
                r.label, r.depth, r.mce_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Shatranj position through mce's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mce side: the generic Shatranj position.
    let pos = Shatranj::from_fen(case.fen).map_err(|e| format!("mce rejected FEN: {e:?}"))?;
    let mce_start = Instant::now();
    let mce_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mce_secs = mce_start.elapsed().as_secs_f64();

    // FSF side: rewrite the mce dialect to FSF's `b`/`q` letters.
    let fsf_fen = to_fsf_dialect(case.fen);
    engine.set_variant("shatranj", false)?;
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

    /// The corpus FENs all parse on the generic Shatranj engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in
    /// `tests/perft_shatranj.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 2u32, 256u64),
            ("mid-alfils", 2, 289),
            ("mid-ferzes", 2, 549),
            ("bared-loss", 2, 0),
        ];
        for (label, depth, want) in pinned {
            let case = CASES.iter().find(|c| c.label == label).expect("label");
            let pos = Shatranj::from_fen(case.fen).expect("corpus FEN parses");
            assert_eq!(
                gperft::<Chess8x8, _>(&pos, depth),
                want,
                "{label} perft({depth})"
            );
        }
    }

    #[test]
    fn dialect_round_trips_pieces() {
        assert_eq!(
            to_fsf_dialect("rn*xkm*xnr/pppppppp/8/8/8/8/PPPPPPPP/RN*XKM*XNR w - - 0 1"),
            "rnbkqbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBKQBNR w - - 0 1"
        );
    }
}
