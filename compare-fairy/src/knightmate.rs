//! Knightmate differential perft + timing against Fairy-Stockfish (issue #224).
//!
//! Knightmate runs on mcr's **generic** engine (`mcr::geometry::Knightmate`, a
//! `GenericPosition<Chess8x8, KnightmateRules>`), like the other fairy variants, so
//! it has its own corpus and comparison loop here. `knightmate` is an FSF
//! **built-in** (it appears in the `UCI_Variant` combo with no `variants.ini`), so
//! the suite selects `UCI_Variant knightmate` directly — no ini load — sets the
//! FEN, runs `go perft`, asserts the node counts match, and reports mcr-vs-FSF
//! throughput.
//!
//! ## FEN dialect
//!
//! mcr and FSF render the same Knightmate position with **different Commoner
//! tokens**. FSF spells the non-royal Commoner (the Mann replacing the opening
//! knights) `m`/`M`; mcr spells it with its `*`-prefixed overflow token `*u`/`*U`
//! (the Commoner shares the Advisor's recycled base letter `u`, as in Synochess and
//! Shinobi). The royal Knight keeps the king's letter `k`/`K` in **both** engines,
//! and every other piece is a standard chess letter, so [`to_fsf_dialect`] simply
//! collapses `*U → M` / `*u → m` over the whole FEN. The comparison asserts only
//! node counts, so the move-string dialect never matters.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Knightmate};

use crate::uci::Engine;

/// One Knightmate corpus position. The FEN is mcr's dialect; the FSF side
/// translates it via [`to_fsf_dialect`].
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Knightmate comparison corpus: the FSF-confirmed startpos, a closed-pawn
/// middlegame and a both-sides-castling-ready middlegame (each exercising the
/// Commoners, the royal Knights, and — for the second — both castles), and a
/// royal-Knight-in-check position with two pawns one step from promotion.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "r*ubqkb*ur/pppppppp/8/8/8/8/PPPPPPPP/R*UBQKB*UR w KQkq - 0 1",
        depth: 5,
    },
    Case {
        label: "mid-closed",
        fen: "r*ubqkb*ur/pp2pppp/2pp4/8/2PP4/8/PP2PPPP/R*UBQKB*UR w KQkq - 0 1",
        depth: 5,
    },
    Case {
        label: "mid-castle",
        fen: "r3k2r/pppq1ppp/2*up1*u2/2b1p3/2B1P3/2*UP1*U2/PPPQ1PPP/R3K2R w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "check-promo",
        fen: "4k3/1P3P2/8/8/3*u4/8/4r3/4K3 w - - 0 1",
        depth: 5,
    },
];

/// Translates an mcr-dialect Knightmate FEN to FSF's dialect by collapsing the
/// Commoner's overflow token `*U`/`*u` to FSF's single letter `M`/`m`. The royal
/// Knight (`k`/`K`) and the standard army carry no remapped token, so the swap is
/// safe over the whole FEN. No other mcr token starts with `*`, so a plain sequence
/// replace is unambiguous.
pub(crate) fn to_fsf_dialect(fen: &str) -> String {
    fen.replace("*U", "M").replace("*u", "m")
}

/// A measured Knightmate comparison row.
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

/// Run the Knightmate corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all matched, or the suite was skipped). Skips gracefully when the loaded
/// FSF binary does not advertise the `knightmate` built-in.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Knightmate (8x8) — generic engine vs FSF UCI_Variant knightmate (issue #224):");

    if !engine.has_variant("knightmate") {
        println!("  SKIP: the loaded FSF binary does not advertise the `knightmate` built-in.");
        return 0;
    }

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
                eprintln!("skip knightmate/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "knightmate OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Knightmate positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Knightmate parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH knightmate/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Knightmate position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Knightmate::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    let fsf_fen = to_fsf_dialect(case.fen);
    engine.set_variant("knightmate", false)?;
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

    /// The corpus FENs all parse on the generic Knightmate engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in
    /// `tests/perft_knightmate.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 3u32, 6765u64),
            ("mid-closed", 3, 20840),
            ("mid-castle", 3, 48290),
            ("check-promo", 4, 969),
        ];
        for (label, depth, want) in pinned {
            let case = CASES.iter().find(|c| c.label == label).expect("label");
            let pos = Knightmate::from_fen(case.fen).expect("corpus FEN parses");
            assert_eq!(
                gperft::<Chess8x8, _>(&pos, depth),
                want,
                "{label} perft({depth})"
            );
        }
    }

    #[test]
    fn dialect_collapses_commoner_to_m() {
        assert_eq!(
            to_fsf_dialect("r*ubqkb*ur/pppppppp/8/8/8/8/PPPPPPPP/R*UBQKB*UR w KQkq - 0 1"),
            "rmbqkbmr/pppppppp/8/8/8/8/PPPPPPPP/RMBQKBMR w KQkq - 0 1"
        );
    }
}
