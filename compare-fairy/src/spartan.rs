//! Spartan chess differential perft + timing against Fairy-Stockfish (issue
//! #181).
//!
//! Spartan runs on mce's **generic** engine (`mce::geometry::Spartan`, a
//! `GenericPosition<Chess8x8, SpartanRules>`), like the other fairy variants, so
//! it has its own corpus and comparison loop here. The FSF side selects
//! `UCI_Variant spartan`, sets the FEN, runs `go perft`, asserts the node counts
//! match, and reports mce-vs-FSF throughput.
//!
//! ## FEN dialect
//!
//! mce and FSF render the same Spartan position with **different Spartan-piece
//! letters**. FSF's `spartan` uses `l g k c w h` (Lieutenant, General, King,
//! Captain, Warlord, Hoplite); mce reuses `g`, `c`, `l` for its Gold / Cannon /
//! Lance roles, so the Spartan pieces take distinct letters: Lieutenant `t`,
//! General `d`, Captain `i`, Warlord `a` (the Hawk = B+N), Hoplite `h`, King `k`
//! (shared). [`to_fsf_dialect`] maps mce's letters back to FSF's over the whole
//! FEN. Only the placement field carries these letters in Spartan (castling uses
//! `KQ`/`-`, no Spartan letters; the en-passant field is a square or `-`), and
//! Black's Spartan pieces are always lowercase (Black never gives White a Spartan
//! piece, and White's standard promotions never produce one), so the swap is
//! unambiguous. The comparison asserts only node counts, so the move-string
//! dialect never matters.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mce::geometry::{perft as gperft, Chess8x8, Spartan};

use crate::uci::Engine;

/// One Spartan corpus position. The FEN is mce's dialect; the FSF side
/// translates it via [`to_fsf_dialect`].
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Spartan comparison corpus: the FSF-confirmed startpos, an opening line
/// with advanced Hoplites, an asymmetric middlegame, and three two-king /
/// duple-check positions (a forced duple check, a duple Black can break, and a
/// king-walk-into-attack).
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "tdkiikat/hhhhhhhh/8/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1",
        depth: 5,
    },
    Case {
        label: "opening",
        fen: "tdkiikat/hhh1hhhh/2h5/8/4P3/8/PPPP1PPP/RNBQKBNR w KQ - 0 2",
        depth: 4,
    },
    Case {
        label: "mid-asym",
        fen: "tdkiikat/hhh2hhh/8/8/2B5/8/PPPPPPPP/RN1QKBNR w KQ - 0 1",
        depth: 4,
    },
    Case {
        label: "duple-check",
        fen: "2k2k2/8/8/8/2Q5/B7/8/4K3 b - - 0 1",
        depth: 5,
    },
    Case {
        label: "duple-break",
        fen: "2k2k2/8/8/8/8/5d2/8/2R2R1K b - - 0 1",
        depth: 4,
    },
    Case {
        label: "king-walk",
        fen: "8/8/8/8/1R6/8/2k2k2/7K b - - 0 1",
        depth: 5,
    },
];

/// Translates an mce-dialect Spartan FEN to FSF's dialect by mapping the Spartan
/// piece letters: Lieutenant `t→l`, General `d→g`, Captain `i→c`, Warlord
/// `a→w` (both cases). King `k`/`K` and Hoplite `h`/`H` are already shared. The
/// standard White army (`RNBQKBNR`/`P`) carries none of the remapped letters, so
/// the swap is safe over the whole FEN.
pub(crate) fn to_fsf_dialect(fen: &str) -> String {
    fen.chars()
        .map(|c| match c {
            't' => 'l',
            'T' => 'L',
            'd' => 'g',
            'D' => 'G',
            'i' => 'c',
            'I' => 'C',
            'a' => 'w',
            'A' => 'W',
            other => other,
        })
        .collect()
}

/// A measured Spartan comparison row.
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

/// Run the Spartan corpus through mce and FSF. Returns the number of mismatches
/// (0 = all positions matched). Prints a table and a one-line summary. If the FSF
/// binary does not advertise the `spartan` variant, the whole block is skipped
/// (returns 0) rather than reporting spurious mismatches.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Spartan chess — generic engine vs FSF UCI_Variant spartan (issue #181):");
    if !engine.has_variant("spartan") {
        println!("  (skipped: this FSF binary does not advertise UCI_Variant spartan)");
        return 0;
    }
    let head = format!(
        "{:<14} {:>5} {:>14} {:>14} {:>9} {:>10} {:>10} {:>8}",
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
                    "{:<14} {:>5} {:>14} {:>14} {:>9} {:>10.1} {:>10.1} {:>7.2}x",
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
                eprintln!("skip spartan/{}: {e}", case.label);
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
            "spartan OVERALL: {nodes} nodes verified; mce {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mce_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mce_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Spartan positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Spartan parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH spartan/{} depth {}: mce={} fsf={}  FEN: {}",
                r.label, r.depth, r.mce_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Spartan position through mce's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mce side: the generic Spartan position (mce dialect).
    let pos = Spartan::from_fen(case.fen).map_err(|e| format!("mce rejected FEN: {e:?}"))?;
    let mce_start = Instant::now();
    let mce_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mce_secs = mce_start.elapsed().as_secs_f64();

    // FSF side: translate the Spartan piece letters to FSF's dialect.
    let fsf_fen = to_fsf_dialect(case.fen);
    engine.set_variant("spartan", false)?;
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

    /// The corpus FENs all parse on the generic Spartan engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_spartan.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 3u32, 14244u64),
            ("opening", 3, 27867),
            ("mid-asym", 3, 21793),
            ("duple-check", 3, 1964),
            ("duple-break", 3, 6691),
            ("king-walk", 3, 3060),
        ];
        for case in CASES {
            let pos = Spartan::from_fen(case.fen).expect("corpus FEN parses");
            let (_, depth, want) = pinned
                .iter()
                .find(|(l, _, _)| *l == case.label)
                .copied()
                .expect("a pinned count for the case");
            assert_eq!(
                gperft::<Chess8x8, _>(&pos, depth),
                want,
                "{} perft",
                case.label
            );
        }
    }

    /// The dialect swap maps mce's Spartan letters to FSF's and leaves the
    /// standard army and structural fields untouched.
    #[test]
    fn dialect_swap_maps_spartan_letters() {
        let mce = "tdkiikat/hhhhhhhh/8/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1";
        let fsf = to_fsf_dialect(mce);
        assert_eq!(
            fsf,
            "lgkcckwl/hhhhhhhh/8/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1"
        );
        // The standard White array and the castling / clock fields are unchanged.
        assert!(fsf.contains("RNBQKBNR") && fsf.contains("PPPPPPPP") && fsf.contains("w KQ -"));
    }
}
