//! Perfect chess (8x8) differential perft + timing against Fairy-Stockfish.
//!
//! Perfect chess runs on mcr's **generic** engine (`mcr::geometry::Perfect`, a
//! `GenericPosition<Chess8x8, PerfectRules>`), like the other fairy variants, so it
//! has its own corpus and comparison loop here. `perfect` is an FSF **built-in** (it
//! appears in the `UCI_Variant` combo with no `variants.ini`), so the suite selects
//! `UCI_Variant perfect` directly — no ini load — sets the FEN, runs `go perft`,
//! asserts the node counts match, and reports mcr-vs-FSF throughput.
//!
//! ## FEN dialect
//!
//! Perfect chess adds three compound pieces mcr and FSF spell differently:
//!
//! | piece      | movement      | mcr letter | FSF letter |
//! |------------|---------------|------------|------------|
//! | Chancellor | Rook + Knight | `e`/`E`    | `c`/`C`    |
//! | Archbishop | Bishop + Knight | `a`/`A`  | `m`/`M`    |
//! | Amazon     | Queen + Knight | `**a`/`**A` | `g`/`G`  |
//!
//! [`to_fsf_dialect`] rewrites those letters in the *placement* field (stripping the
//! `**` overflow prefix from the Amazon); every other field is passed through
//! unchanged.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no GPL
//! code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Perfect};

use crate::uci::Engine;

/// Rewrites an mcr Perfect-chess FEN into the one FSF parses: in the placement
/// field, map the chancellor `e`->`c`, the archbishop `a`->`m`, and the amazon
/// overflow token `**a`->`g` (case-preserving). Every other field is passed through
/// unchanged (only the placement field holds piece letters).
pub fn to_fsf_dialect(fen: &str) -> String {
    let (placement, rest) = match fen.split_once(' ') {
        Some((p, r)) => (p, Some(r)),
        None => (fen, None),
    };
    let mut out = String::with_capacity(placement.len());
    let mut chars = placement.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '*' && chars.peek() == Some(&'*') {
            // A second-bank overflow token `**a`/`**A`, the Amazon: consume the
            // second `*`, then emit FSF's `g`/`G`, case-preserved.
            chars.next();
            match chars.next() {
                Some('a') => out.push('g'),
                Some('A') => out.push('G'),
                Some(base) => out.push(base),
                None => {}
            }
        } else {
            out.push(match c {
                'e' => 'c', // Chancellor (R+N)
                'E' => 'C',
                'a' => 'm', // Archbishop (B+N)
                'A' => 'M',
                other => other,
            });
        }
    }
    match rest {
        Some(r) => format!("{out} {r}"),
        None => out,
    }
}

/// One Perfect-chess corpus position (mcr dialect).
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Perfect-chess comparison corpus: the FSF-confirmed startpos, a position with
/// **both** castles available (proving the queen-side Chancellor castle generates
/// identically), a developed midgame exercising the three compounds, and a
/// promotion position spanning all seven promotion targets.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "eaq**akbnr/pppppppp/8/8/8/8/PPPPPPPP/EAQ**AKBNR w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "castle",
        fen: "e3k2r/pppppppp/8/8/8/8/PPPPPPPP/E3K2R w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "midgame",
        fen: "eaq1k2r/pppp1ppp/2**a2n2/2b1p3/2B1P3/2**A2N2/PPPP1PPP/EAQ1K2R w KQkq - 6 5",
        depth: 3,
    },
    Case {
        label: "promo",
        fen: "4k3/2P5/8/8/8/8/8/4K3 w - - 0 1",
        depth: 4,
    },
];

/// A measured Perfect-chess comparison row.
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

/// Run the Perfect-chess corpus through mcr and FSF. Returns the number of
/// mismatches (0 = all matched, or the suite was skipped). Skips gracefully when the
/// loaded FSF binary does not advertise the `perfect` built-in.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Perfect chess (8x8) — generic engine vs FSF UCI_Variant perfect:");

    if !engine.has_variant("perfect") {
        println!("  SKIP: the loaded FSF binary does not advertise the `perfect` built-in.");
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
                eprintln!("skip perfect/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "perfect OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Perfect chess positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Perfect chess parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH perfect/{} depth {}: mcr={} fsf={}  mcr FEN: {}  FSF FEN: {}",
                r.label,
                r.depth,
                r.mcr_nodes,
                r.fsf_nodes,
                r.fen,
                to_fsf_dialect(r.fen),
            );
        }
    }
    mismatches
}

/// Run one Perfect-chess position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Perfect::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    let fsf_fen = to_fsf_dialect(case.fen);
    engine.set_variant("perfect", false)?;
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

    /// The dialect rewrites the three compound letters (chancellor, archbishop, and
    /// the amazon overflow token) and leaves every other field untouched.
    #[test]
    fn dialect_rewrites_the_three_compounds() {
        assert_eq!(
            to_fsf_dialect("eaq**akbnr/pppppppp/8/8/8/8/PPPPPPPP/EAQ**AKBNR w KQkq - 0 1"),
            "cmqgkbnr/pppppppp/8/8/8/8/PPPPPPPP/CMQGKBNR w KQkq - 0 1"
        );
        // The trailing fields carry no piece letters: a `KQkq` castling field and an
        // `e3` en-passant token are left intact (only placement letters are mapped).
        assert_eq!(
            to_fsf_dialect("e3k2r/pppppppp/8/8/4P3/8/PPPP1PPP/E3K2R b KQkq e3 0 1"),
            "c3k2r/pppppppp/8/8/4P3/8/PPPP1PPP/C3K2R b KQkq e3 0 1"
        );
    }

    /// The corpus FENs all parse on the generic Perfect engine, and the pinned
    /// depth-2 counts match the FSF-confirmed numbers in `tests/perft_perfect.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 529u64),
            ("castle", 676),
            ("midgame", 2019),
            ("promo", 39),
        ];
        for case in CASES {
            let pos = Perfect::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Chess8x8, _, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }
}
