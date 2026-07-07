//! Sort-of-almost chess (8x8) differential perft + timing against Fairy-Stockfish.
//!
//! Sort-of-almost chess runs on mcr's **generic** engine
//! (`mcr::geometry::Sortofalmost`, a `GenericPosition<Chess8x8, SortofalmostRules>`),
//! like the other fairy variants, so it has its own corpus and comparison loop here.
//! `sortofalmost` is an FSF **built-in** (it appears in the `UCI_Variant` combo with
//! no `variants.ini`), so the suite selects `UCI_Variant sortofalmost` directly — no
//! ini load — sets the FEN, runs `go perft`, asserts the node counts match, and
//! reports mcr-vs-FSF throughput.
//!
//! ## FEN dialect
//!
//! Only **White's** heavy piece differs from standard chess — a Rook + Knight
//! **Chancellor** on the queen's square — and mcr and FSF spell it differently:
//!
//! | piece      | movement      | mcr letter | FSF letter |
//! |------------|---------------|------------|------------|
//! | Chancellor | Rook + Knight | `e`/`E`    | `c`/`C`    |
//!
//! [`to_fsf_dialect`] rewrites that letter in the *placement* field only; every
//! other field (including any en-passant square whose file letter could be `e`) is
//! passed through unchanged. Black's queen and the rest of the army are standard
//! chess letters shared by both engines.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no GPL
//! code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Sortofalmost};

use crate::uci::Engine;

/// Rewrites an mcr Sort-of-almost FEN into the one FSF parses: in the placement
/// field only, map the chancellor `e`->`c` (case-preserving). Every other field is
/// passed through unchanged (only the placement field holds piece letters, so an
/// en-passant `e`-file square is never touched).
pub fn to_fsf_dialect(fen: &str) -> String {
    let (placement, rest) = match fen.split_once(' ') {
        Some((p, r)) => (p, Some(r)),
        None => (fen, None),
    };
    let out: String = placement
        .chars()
        .map(|c| match c {
            'e' => 'c', // Chancellor (R+N)
            'E' => 'C',
            other => other,
        })
        .collect();
    match rest {
        Some(r) => format!("{out} {r}"),
        None => out,
    }
}

/// One Sort-of-almost corpus position. The FEN is mcr's dialect; the FSF side
/// translates it via [`to_fsf_dialect`].
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Sort-of-almost comparison corpus: the FSF-confirmed startpos, a both-sides-
/// developed middlegame exercising White's Chancellor and Black's Queen with a
/// castling-ready White king, and two one-step-from-promotion positions — White
/// promotes to a Chancellor (never a queen), Black to a Queen (never a chancellor)
/// — that pin the asymmetric promotion sets.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBEKBNR w KQkq - 0 1",
        depth: 5,
    },
    Case {
        label: "midgame",
        fen: "r1bqk1nr/pppp1ppp/2n5/2b1p3/2B1P3/5N2/PPPP1PPP/RNBEK2R w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "white-promo",
        fen: "4k3/1P6/8/8/8/8/8/4K3 w - - 0 1",
        depth: 3,
    },
    Case {
        label: "black-promo",
        fen: "4k3/8/8/8/8/8/1p6/4K3 b - - 0 1",
        depth: 3,
    },
];

/// A measured Sort-of-almost comparison row.
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

/// Run the Sort-of-almost corpus through mcr and FSF. Returns the number of
/// mismatches (0 = all matched, or the suite was skipped). Skips gracefully when the
/// loaded FSF binary does not advertise the `sortofalmost` built-in.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Sort-of-almost (8x8) — generic engine vs FSF UCI_Variant sortofalmost (issue #585):");

    if !engine.has_variant("sortofalmost") {
        println!("  SKIP: the loaded FSF binary does not advertise the `sortofalmost` built-in.");
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
                eprintln!("skip sortofalmost/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "sortofalmost OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Sort-of-almost positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Sort-of-almost parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH sortofalmost/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Sort-of-almost position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Sortofalmost::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    let fsf_fen = to_fsf_dialect(case.fen);
    engine.set_variant("sortofalmost", false)?;
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

    /// The corpus FENs all parse on the generic Sort-of-almost engine, and the
    /// pinned shallow counts match the FSF-confirmed numbers in
    /// `tests/perft_sortofalmost.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 4u32, 239279u64),
            ("midgame", 3, 39396),
            ("white-promo", 3, 463),
            ("black-promo", 3, 497),
        ];
        for (label, depth, want) in pinned {
            let case = CASES.iter().find(|c| c.label == label).expect("label");
            let pos = Sortofalmost::from_fen(case.fen).expect("corpus FEN parses");
            assert_eq!(
                gperft::<Chess8x8, _, _>(&pos, depth),
                want,
                "{label} perft({depth})"
            );
        }
    }

    #[test]
    fn dialect_collapses_chancellor_to_c() {
        assert_eq!(
            to_fsf_dialect("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBEKBNR w KQkq - 0 1"),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBCKBNR w KQkq - 0 1"
        );
    }

    /// The en-passant `e`-file square in the state fields is left untouched — only
    /// the placement field is rewritten.
    #[test]
    fn dialect_leaves_en_passant_file_alone() {
        assert_eq!(
            to_fsf_dialect("rnbqkbnr/pppp1ppp/8/4p3/8/8/PPPPPPPP/RNBEKBNR w KQkq e6 0 1"),
            "rnbqkbnr/pppp1ppp/8/4p3/8/8/PPPPPPPP/RNBCKBNR w KQkq e6 0 1"
        );
    }
}
