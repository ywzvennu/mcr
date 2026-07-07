//! Georgian chess differential perft + timing against Fairy-Stockfish.
//!
//! Georgian chess runs on mcr's **generic** engine (`mcr::geometry::Georgian`, a
//! `GenericPosition<Chess8x8, GeorgianRules>`), like the other fairy variants, so
//! it has its own corpus and comparison loop here. `georgian` is an FSF
//! **built-in** (it appears in the `UCI_Variant` combo with no `variants.ini`), so
//! the suite selects `UCI_Variant georgian` directly — no ini load — sets the FEN,
//! runs `go perft`, asserts the node counts match, and reports mcr-vs-FSF
//! throughput.
//!
//! ## FEN dialect
//!
//! Georgian chess is the Amazon army with no castling and no en passant. mcr
//! spells the Amazon (Queen + Knight) with the second-bank overflow token
//! `**a`/`**A`; FSF spells it `a`/`A`. The dialect strips the `**` prefix from
//! that token; every other letter and field is identical. The removed castling
//! and en passant are *rule* differences — the FEN's castling field is `-` and no
//! double step ever sets an ep target — not letter ones.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Georgian};

use crate::uci::Engine;

/// Rewrites an mcr Georgian FEN into the one FSF parses: strip the `**` prefix
/// from the Amazon overflow token `**a`/`**A` in the placement field. Every other
/// field is passed through unchanged.
fn to_fsf_dialect(fen: &str) -> String {
    let (placement, rest) = match fen.split_once(' ') {
        Some((p, r)) => (p, Some(r)),
        None => (fen, None),
    };
    let mut out = String::with_capacity(placement.len());
    let mut chars = placement.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '*' && chars.peek() == Some(&'*') {
            // A second-bank overflow token `**X`: consume the second `*`, then
            // emit the base letter case-preserved (only `**a`/`**A`, the Amazon,
            // occurs).
            chars.next();
            if let Some(base) = chars.next() {
                out.push(base);
            }
        } else {
            out.push(c);
        }
    }
    match rest {
        Some(r) => format!("{out} {r}"),
        None => out,
    }
}

/// One Georgian corpus position (mcr dialect).
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Georgian comparison corpus: the FSF-confirmed startpos (the Amazon army,
/// no castling rights, whose deep count diverges from Amazon Chess only by the
/// removed en passant), a cleared-back-rank rooks-and-kings position carrying
/// `KQkq` (castling-rich in Amazon Chess, so the missing castles show as a lower
/// count here), and a single position carrying **both** `KQkq` rights and an `e3`
/// en-passant target (a would-be castle *and* a would-be `d4xe3` capture, both
/// absent in Georgian).
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnb**akbnr/pppppppp/8/8/8/8/PPPPPPPP/RNB**AKBNR w - - 0 1",
        depth: 5,
    },
    Case {
        label: "would-castle",
        fen: "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "would-castle-ep",
        fen: "r3k2r/pppppppp/8/8/3pP3/8/PPPP1PPP/R3K2R b KQkq e3 0 1",
        depth: 4,
    },
];

/// A measured Georgian comparison row.
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

/// Run the Georgian corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all matched, or the suite was skipped). Skips gracefully when the loaded
/// FSF binary does not advertise the `georgian` built-in.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Georgian chess (8x8) — generic engine vs FSF UCI_Variant georgian:");

    if !engine.has_variant("georgian") {
        println!("  SKIP: the loaded FSF binary does not advertise the `georgian` built-in.");
        return 0;
    }

    let head = format!(
        "{:<16} {:>5} {:>14} {:>14} {:>9} {:>10} {:>10} {:>8}",
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
                    "{:<16} {:>5} {:>14} {:>14} {:>9} {:>10.1} {:>10.1} {:>7.2}x",
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
                eprintln!("skip georgian/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "georgian OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Georgian positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Georgian parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH georgian/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Georgian position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Georgian::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // FSF spells the Amazon `a`/`A`; strip mcr's `**` overflow prefix.
    let fsf_fen = to_fsf_dialect(case.fen);
    engine.set_variant("georgian", false)?;
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

    /// The overflow-token dialect strips the `**` prefix from the Amazon and
    /// leaves every other field untouched.
    #[test]
    fn dialect_strips_amazon_overflow_prefix() {
        assert_eq!(
            to_fsf_dialect("rnb**akbnr/pppppppp/8/8/8/8/PPPPPPPP/RNB**AKBNR w - - 0 1"),
            "rnbakbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBAKBNR w - - 0 1"
        );
        assert_eq!(
            to_fsf_dialect("r3k2r/pppppppp/8/8/3pP3/8/PPPP1PPP/R3K2R b KQkq e3 0 1"),
            "r3k2r/pppppppp/8/8/3pP3/8/PPPP1PPP/R3K2R b KQkq e3 0 1"
        );
    }

    /// The corpus FENs all parse on the generic Georgian engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_georgian.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 3u32, 12483u64),
            ("would-castle", 3, 12035),
            ("would-castle-ep", 3, 12517),
        ];
        for (label, depth, want) in pinned {
            let case = CASES.iter().find(|c| c.label == label).expect("label");
            let pos = Georgian::from_fen(case.fen).expect("corpus FEN parses");
            assert_eq!(
                gperft::<Chess8x8, _, _>(&pos, depth),
                want,
                "{label} perft({depth})"
            );
        }
    }
}
