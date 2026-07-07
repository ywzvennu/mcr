//! Ten-Cubed chess (10x10) differential perft + timing against Fairy-Stockfish
//! (issue #375).
//!
//! Ten-Cubed runs on mcr's **generic** `u128` engine (`mcr::geometry::Tencubed`, a
//! `GenericPosition<Grand10x10, TencubedRules>`), like Grand, so it has its own
//! corpus and comparison loop here (mirroring `grand.rs`). The FSF side selects
//! `UCI_Variant tencubed`, sets the FEN, runs `go perft`, asserts the node counts
//! match, and reports mcr-vs-FSF throughput.
//!
//! **FSF must be built with large-board support** (`make ... largeboards=yes`):
//! the default FSF build omits the 10x10 `tencubed` variant. When the running
//! binary lacks it, the comparison is meaningless, so this loop checks `tencubed`
//! is in the variant list first and skips with a clear message if not.
//!
//! ## FEN dialect
//!
//! mcr and FSF agree on the position but spell the fairy pieces differently. mcr
//! uses its second-bank overflow tokens `**w`/`**W` (Wizard = Camel + Ferz) and
//! `**x`/`**X` (Champion = Wazir + Alfil + Dabbaba) and its Elephant `e`/`E` for the
//! Rook+Knight Marshal; FSF spells them `w`/`W`, `c`/`C`, and `m`/`M`. The Bishop+Knight
//! Archbishop `a`/`A` is identical in both. [`fen_to_fsf`] rewrites those letters in the
//! placement field; every other field is byte-identical.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Grand10x10, Tencubed};

use crate::uci::Engine;

/// One Ten-Cubed corpus position, in the **mcr dialect** (marshal `e`, wizard
/// `**w`, champion `**x`).
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Ten-Cubed comparison corpus: the FSF-confirmed startpos, a quiet e-pawn
/// midgame, and a last-rank promotion position (pawn on e9 promoting to a Queen,
/// Marshal, or Archbishop only). Depths are modest by default; `full` deepens by
/// one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "2**x**wae**w**x2/1rnbqkbnr1/pppppppppp/10/10/10/10/PPPPPPPPPP/1RNBQKBNR1/2**X**WAE**W**X2 w - - 0 1",
        depth: 3,
    },
    Case {
        label: "midgame",
        fen: "2**x**wae**w**x2/1rnbqkbnr1/pppp1ppppp/10/4p5/4P5/10/PPPP1PPPPP/1RNBQKBNR1/2**X**WAE**W**X2 w - - 0 2",
        depth: 3,
    },
    Case {
        label: "promo",
        fen: "k9/4P5/10/10/10/10/10/10/10/K9 w - - 0 1",
        depth: 3,
    },
];

/// Rewrite an mcr-dialect Ten-Cubed FEN into the FSF dialect: in the *placement*
/// field, the Wizard `**w`/`**W` becomes `w`/`W`, the Champion `**x`/`**X` becomes
/// `c`/`C`, and the Marshal `e`/`E` becomes `m`/`M`. The Archbishop `a`/`A` and
/// every other letter and field are unchanged.
pub fn fen_to_fsf(fen: &str) -> String {
    let (placement, rest) = match fen.split_once(' ') {
        Some((p, r)) => (p, Some(r)),
        None => (fen, None),
    };
    let mut out = String::with_capacity(placement.len());
    let mut chars = placement.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '*' && chars.peek() == Some(&'*') {
            // A second-bank overflow token `**B`: consume the second `*`, then map
            // the base letter to FSF's spelling (Wizard `w`→`w`, Champion `x`→`c`),
            // preserving case (colour).
            chars.next();
            if let Some(base) = chars.next() {
                out.push(match base {
                    'w' => 'w',
                    'W' => 'W',
                    'x' => 'c',
                    'X' => 'C',
                    other => other,
                });
            }
        } else {
            // The Marshal (Elephant) `e`/`E` is FSF's `m`/`M`.
            out.push(match c {
                'e' => 'm',
                'E' => 'M',
                other => other,
            });
        }
    }
    match rest {
        Some(r) => format!("{out} {r}"),
        None => out,
    }
}

/// A measured Ten-Cubed comparison row.
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

/// Run the Ten-Cubed corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all positions matched, or FSF lacks `tencubed` and the suite is skipped).
/// Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!(
        "Ten-Cubed chess (10x10, u128) — generic engine vs FSF UCI_Variant tencubed (issue #375):"
    );
    println!("  (requires an FSF built with largeboards=yes)");

    if !engine.has_variant("tencubed") {
        println!("  SKIP: this FSF binary has no `tencubed` variant (build it largeboards=yes).");
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
                eprintln!("skip tencubed/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "tencubed OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Ten-Cubed positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Ten-Cubed parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH tencubed/{} depth {}: mcr={} fsf={}  mcr FEN: {}  FSF FEN: {}",
                r.label,
                r.depth,
                r.mcr_nodes,
                r.fsf_nodes,
                r.fen,
                fen_to_fsf(r.fen),
            );
        }
    }
    mismatches
}

/// Run one Ten-Cubed position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Tencubed::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Grand10x10, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    let fsf_fen = fen_to_fsf(case.fen);
    engine.set_variant("tencubed", false)?;
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

    /// The corpus FENs all parse on the generic Ten-Cubed engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_tencubed.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [("startpos", 1600u64), ("midgame", 2497), ("promo", 16)];
        for case in CASES {
            let pos = Tencubed::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Grand10x10, _, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }

    /// The mcr -> FSF dialect rewrite maps the Wizard `**w`, Champion `**x`, and
    /// Marshal `e` letters and leaves the Archbishop `a` and every other field intact.
    #[test]
    fn fen_dialect_rewrites_fairy_letters() {
        let mcr = "2**x**wae**w**x2/1rnbqkbnr1/pppppppppp/10/10/10/10/PPPPPPPPPP/1RNBQKBNR1/2**X**WAE**W**X2 w - - 0 1";
        let fsf =
            "2cwamwc2/1rnbqkbnr1/pppppppppp/10/10/10/10/PPPPPPPPPP/1RNBQKBNR1/2CWAMWC2 w - - 0 1";
        assert_eq!(fen_to_fsf(mcr), fsf);
        // Trailing fields (including an `e3`-shaped en-passant token) are untouched:
        // only placement letters map. Here the placement has none of the fairy tokens.
        let out = fen_to_fsf("k9/10/10/10/10/10/10/10/10/K9 b - e3 1 9");
        assert_eq!(out, "k9/10/10/10/10/10/10/10/10/K9 b - e3 1 9");
    }
}
