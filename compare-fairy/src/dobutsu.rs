//! Dobutsu (3x4 animal shogi) differential perft + timing against
//! Fairy-Stockfish (#233).
//!
//! Dobutsu runs on mce's **generic** `u64` engine (`mce::geometry::Dobutsu`, a
//! `GenericPosition<Dobutsu3x4, DobutsuRules>`), not the concrete 8x8 `AnyVariant`
//! layer the rest of this harness drives, so it has its own corpus and comparison
//! loop here (mirroring `minishogi.rs` / `shogi.rs`). The FSF side selects
//! `UCI_Variant dobutsu`, sets the FEN, runs `go perft`, asserts the node counts
//! match, and reports mce-vs-FSF throughput. The corpus exercises the **drops**,
//! the **non-royal Lion**, the **forced Chick promotion**, and the **safe-try**
//! flag win.
//!
//! `dobutsu` is a FSF **built-in** (no `variants.ini` needed), but the 3x4 board
//! is only present in an FSF built with large-board support (`make ...
//! largeboards=yes`). When the running binary lacks `dobutsu`, this loop skips
//! rather than compare meaningless truncated counts.
//!
//! ## FEN dialect
//!
//! mce and FSF use **different** Dobutsu piece letters. mce reuses existing
//! `WideRole`s — the Lion is a King (`k`), the Chick a Pawn (`p`), the Elephant a
//! Met (`m`), and the Giraffe the Wazir overflow role (`*j`, the `*` prefix marking
//! the overflow token) — while FSF spells them `l c e g` (Lion, Chick, Elephant,
//! Giraffe). [`fen_to_fsf`] rewrites the placement and the holdings bracket
//! (`k→l`, `p→c`, `m→e`, `*j→g`, case preserved, the `+`-prefixed promoted Chick
//! `+p→+c`) before the FEN is handed to FSF.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mce::geometry::{perft as gperft, Dobutsu, Dobutsu3x4};

use crate::uci::Engine;

/// One Dobutsu corpus position (an mce-dialect FEN).
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Dobutsu comparison corpus, all FSF-confirmed and pinned in
/// `tests/perft_dobutsu.rs`: the startpos; the Chicks-in-hand drop position; bare
/// Lions with one of every droppable role in each hand (drops dominate); a forced
/// Chick promotion; and a Lion try-advance (the safe-try flag win). Depths are
/// modest by default; `full` deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "*jkm/1p1/1P1/MK*J[] w - - 0 1",
        depth: 5,
    },
    Case {
        label: "drops",
        fen: "*jkm/3/1P1/MK*J[p] w - - 0 1",
        depth: 5,
    },
    Case {
        label: "multi-hand",
        fen: "1k1/3/3/1K1[M*JPm*jp] w - - 0 1",
        depth: 4,
    },
    Case {
        label: "forced-promo",
        fen: "1k1/1P1/3/1K1[] w - - 0 1",
        depth: 4,
    },
    Case {
        label: "try-advance",
        fen: "1k1/3/1K1/3[M*JP] w - - 0 1",
        depth: 5,
    },
];

/// Rewrite an mce-dialect Dobutsu FEN into the FSF dialect. The Lion `k`/`K`, the
/// Chick `p`/`P`, the Elephant `m`/`M`, and the Giraffe `*j`/`*J` (overflow token)
/// become `l c e g` (case preserved). The `*` prefix is dropped and its following
/// base letter `j`/`J` is remapped to FSF's Giraffe `g`/`G`. The `+`-prefixed
/// promoted Chick (`+p`/`+P`) maps its base letter to `c`/`C`, staying `+c`/`+C`.
/// Digits, slashes, and the holdings bracket pass through; the rewrite covers the
/// placement *and* the holdings (both hold the same letters).
pub fn fen_to_fsf(fen: &str) -> String {
    let map = |c: char| match c {
        'k' => 'l',
        'K' => 'L',
        'p' => 'c',
        'P' => 'C',
        'm' => 'e',
        'M' => 'E',
        // Giraffe: mce's overflow base letter `j` is FSF's `g`.
        'j' => 'g',
        'J' => 'G',
        other => other,
    };
    let rewrite = |field: &str| -> String {
        // Drop every `*` overflow prefix; the following `j`/`J` (the Giraffe) is
        // remapped to FSF's `g`/`G` by `map`.
        field.chars().filter(|&c| c != '*').map(map).collect()
    };
    match fen.split_once(' ') {
        Some((placement, rest)) => format!("{} {rest}", rewrite(placement)),
        None => rewrite(fen),
    }
}

/// A measured Dobutsu comparison row.
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

/// Run the Dobutsu corpus through mce and FSF. Returns the number of mismatches
/// (0 = all matched, or FSF lacks `dobutsu` and the suite is skipped). Prints a
/// table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!(
        "Dobutsu (3x4, u64, hand + drops + non-royal Lion + safe-try flag win) — generic engine \
vs FSF UCI_Variant dobutsu (issue #233):"
    );
    println!("  (requires an FSF built with largeboards=yes)");

    if !engine.has_variant("dobutsu") {
        println!("  SKIP: this FSF binary has no `dobutsu` variant (build it largeboards=yes).");
        return 0;
    }

    let head = format!(
        "{:<18} {:>5} {:>14} {:>14} {:>9} {:>10} {:>10} {:>8}",
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
                    "{:<18} {:>5} {:>14} {:>14} {:>9} {:>10.1} {:>10.1} {:>7.2}x",
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
                eprintln!("skip dobutsu/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mce_nodes).sum();
    let mce_s: f64 = rows.iter().map(|r| r.mce_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mce_s > 0.0 && fsf_s > 0.0 {
        println!(
            "dobutsu OVERALL: {nodes} nodes verified; mce {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mce_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mce_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Dobutsu positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Dobutsu parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH dobutsu/{} depth {}: mce={} fsf={}  FEN: {}",
                r.label, r.depth, r.mce_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Dobutsu position through mce's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Dobutsu::from_fen(case.fen).map_err(|e| format!("mce rejected FEN: {e:?}"))?;
    let mce_start = Instant::now();
    let mce_nodes = gperft::<Dobutsu3x4, _>(&pos, depth);
    let mce_secs = mce_start.elapsed().as_secs_f64();

    let fsf_fen = fen_to_fsf(case.fen);
    engine.set_variant("dobutsu", false)?;
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

    /// The corpus FENs all parse on the generic Dobutsu engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_dobutsu.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 123u64),
            ("drops", 218),
            ("multi-hand", 1135),
            ("forced-promo", 128),
            ("try-advance", 190),
        ];
        for case in CASES {
            let pos = Dobutsu::from_fen(case.fen).expect("corpus FEN parses");
            let depth = if case.label == "multi-hand" || case.label == "try-advance" {
                2
            } else {
                3
            };
            let n = gperft::<Dobutsu3x4, _>(&pos, depth);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned shallow count for the case");
            assert_eq!(n, want, "{} shallow perft", case.label);
        }
    }

    /// The FSF rewrite maps mce's letters to FSF's dialect, dropping the `*`
    /// overflow prefix and preserving case and the holdings bracket.
    #[test]
    fn fen_rewrite_matches_fsf_dialect() {
        assert_eq!(
            fen_to_fsf("*jkm/1p1/1P1/MK*J[] w - - 0 1"),
            "gle/1c1/1C1/ELG[] w - - 0 1"
        );
        assert_eq!(
            fen_to_fsf("1k1/3/3/1K1[M*JPm*jp] w - - 0 1"),
            "1l1/3/3/1L1[EGCegc] w - - 0 1"
        );
        // The promoted Chick (Hen) keeps its `+` prefix, base letter mapped.
        assert_eq!(
            fen_to_fsf("1+P1/3/3/1k1[] b - - 0 1"),
            "1+C1/3/3/1l1[] b - - 0 1"
        );
    }
}
