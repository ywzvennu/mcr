//! Modern chess (9x9) differential perft + timing against Fairy-Stockfish.
//!
//! Modern runs on mcr's **generic** `u128` engine (`mcr::geometry::Modern`, a
//! `GenericPosition<Chess9x9, ModernRules>`), like the other large-board fairy
//! variants, so it has its own corpus and comparison loop here (mirroring
//! `capablanca.rs`). `modern` is an FSF **built-in** (it appears in the
//! `UCI_Variant` combo with no `variants.ini`), so the suite selects
//! `UCI_Variant modern` directly — no ini load — sets the FEN, runs `go perft`,
//! asserts the node counts match, and reports mcr-vs-FSF throughput.
//!
//! **FSF must be built with large-board support** (`make ... largeboards=yes`):
//! the default FSF build omits the 9x9 `modern` variant from its `UCI_Variant`
//! list. When the running binary lacks it, the comparison is skipped.
//!
//! ## FEN dialect
//!
//! mcr and FSF agree on the position but spell the archbishop differently: mcr
//! uses `a`/`A` (its [`WideRole::Hawk`] bishop-knight compound, the Capablanca /
//! Seirawan / Janus archbishop letter), FSF uses `m`/`M`. [`fen_to_fsf`] rewrites
//! the archbishop's letter; the placement is otherwise byte-identical.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess9x9, Modern};

use crate::uci::Engine;

/// One Modern corpus position, in the **mcr dialect** (archbishop = `a`).
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Modern comparison corpus: the FSF-confirmed startpos, a castling-rich
/// position (pins the king e -> g/c, rook i -> f / a -> d geometry), a developed
/// midgame, and a promotion position. Depths are modest by default; `full`
/// deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnbqkabnr/ppppppppp/9/9/9/9/9/PPPPPPPPP/RNBAKQBNR w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "castle",
        fen: "r3k3r/9/9/9/9/9/9/9/R3K3R w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "midgame",
        fen: "r1bqkab1r/ppp2pppp/2n2n3/3pp4/9/3PP4/2N2N3/PPP2PPPP/R1BAKQB1R w KQkq - 0 5",
        depth: 4,
    },
    Case {
        label: "promo",
        fen: "4k4/P8/9/9/9/9/9/9/4K4 w - - 0 1",
        depth: 4,
    },
];

/// Rewrite an mcr-dialect Modern FEN into the FSF dialect: the archbishop's letter
/// `a`/`A` becomes `m`/`M` in the *placement* field only (the other FEN fields
/// carry no piece letters). Every other letter is unchanged.
pub fn fen_to_fsf(fen: &str) -> String {
    // Only the placement field (up to the first space) holds piece letters.
    match fen.split_once(' ') {
        Some((placement, rest)) => {
            let mapped: String = placement
                .chars()
                .map(|c| match c {
                    'a' => 'm',
                    'A' => 'M',
                    other => other,
                })
                .collect();
            format!("{mapped} {rest}")
        }
        None => fen
            .chars()
            .map(|c| match c {
                'a' => 'm',
                'A' => 'M',
                other => other,
            })
            .collect(),
    }
}

/// A measured Modern comparison row.
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

/// Run the Modern corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all matched, or the suite was skipped). Skips gracefully when the loaded
/// FSF binary does not advertise the `modern` built-in.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Modern chess (9x9, u128) — generic engine vs FSF UCI_Variant modern:");
    println!("  (requires an FSF built with largeboards=yes)");

    if !engine.has_variant("modern") {
        println!("  SKIP: the loaded FSF binary does not advertise the `modern` built-in.");
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
                eprintln!("skip modern/{}: {e}", case.label);
            }
        }
    }

    // Node-weighted aggregate throughput.
    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "modern OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Modern positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Modern parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH modern/{} depth {}: mcr={} fsf={}  mcr FEN: {}  FSF FEN: {}",
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

/// Run one Modern position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mcr side: the generic Modern position over the 9x9 u128 geometry.
    let pos = Modern::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess9x9, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // FSF side: rewrite the archbishop's letter into the FSF dialect.
    let fsf_fen = fen_to_fsf(case.fen);
    engine.set_variant("modern", false)?;
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

    /// The corpus FENs all parse on the generic Modern engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_modern.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 576u64),
            ("castle", 713),
            ("midgame", 1583),
            ("promo", 46),
        ];
        for case in CASES {
            let pos = Modern::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Chess9x9, _, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }

    /// The mcr -> FSF dialect rewrite swaps only the archbishop's letter and
    /// leaves every other field intact.
    #[test]
    fn fen_dialect_rewrites_only_the_archbishop() {
        let mcr = "rnbqkabnr/ppppppppp/9/9/9/9/9/PPPPPPPPP/RNBAKQBNR w KQkq - 0 1";
        let fsf = "rnbqkmbnr/ppppppppp/9/9/9/9/9/PPPPPPPPP/RNBMKQBNR w KQkq - 0 1";
        assert_eq!(fen_to_fsf(mcr), fsf);
        // The trailing fields are untouched: an `a3` en-passant token is not a
        // placement letter, so it stays `a3` while the white archbishop `A`
        // becomes `M`.
        let out = fen_to_fsf("a8/9/9/9/9/9/9/9/A8 b - a3 1 9");
        assert_eq!(out, "m8/9/9/9/9/9/9/9/M8 b - a3 1 9");
    }
}
