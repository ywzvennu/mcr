//! Jieqi (揭棋, hidden Xiangqi, 9x10) differential perft against Fairy-Stockfish
//! (issue #278).
//!
//! Jieqi is **not** an FSF variant: its stochastic hidden-identity reveal cannot be
//! expressed in an FSF variant config, and `go perft` is only meaningful for a
//! full-information position — which is exactly standard **Xiangqi**. The reveal
//! model wired into mcr's make-move path is the *identity* baseline (a face-down
//! piece reveals as the Xiangqi piece native to its home square), under which the
//! whole Jieqi game tree is bit-identical to Xiangqi. So this harness validates the
//! Jieqi engine by running its perft on the **mcr side** and comparing against FSF
//! `UCI_Variant xiangqi` perft on the **identity-reveal Xiangqi equivalent** of the
//! same position (every `=d`/`=D` face-down piece rewritten to the Xiangqi piece
//! native to its square). A match confirms the dark movement *and* the reveal
//! transition reproduce Xiangqi node-for-node against an independent engine.
//!
//! The stochastic reveal-from-pool (a random unrevealed identity) is validated
//! separately by the seeded unit/property tests in
//! `mcr::geometry::variants::jieqi`; it has no FSF analogue and is not perft-able.
//!
//! **FSF must be built with large-board support** (`largeboards=yes`) for the 9x10
//! `xiangqi` variant; without it this loop skips.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no GPL
//! code is linked.

use std::time::Instant;

use mcr::geometry::{
    perft as gperft, variants::jieqi::home_role, Geometry, Jieqi, Square, Xiangqi9x10,
};

use crate::uci::Engine;
use crate::xiangqi::fen_to_fsf;

/// One Jieqi corpus position (mcr dialect; face-down pieces as `=d`/`=D`).
struct Case {
    label: &'static str,
    fen: String,
    depth: u32,
}

/// Rewrite a Jieqi FEN (mcr dialect) into its **identity-reveal Xiangqi
/// equivalent** in the mcr Xiangqi dialect: every `=d`/`=D` face-down piece is
/// replaced by the Xiangqi piece native to its square (`home_role`), with the case
/// (colour) preserved; concrete pieces and empty runs pass through. Returns an
/// error if a face-down piece sits off a home square (no `home_role`), which never
/// happens in a legal Jieqi position (a dark piece reveals the instant it moves).
fn jieqi_to_xiangqi_mcr(fen: &str) -> Result<String, String> {
    let (placement, rest) = fen.split_once(' ').ok_or("Jieqi FEN has no fields")?;
    let mut out = String::new();
    for (ri, rank_str) in placement.split('/').enumerate() {
        if ri > 0 {
            out.push('/');
        }
        // FEN ranks are listed top-first (rank index HEIGHT-1) down to 0.
        let rank = (Xiangqi9x10::HEIGHT - 1)
            .checked_sub(ri as u8)
            .ok_or("too many ranks in Jieqi FEN")?;
        let mut file = 0u8;
        let mut chars = rank_str.chars();
        while let Some(c) = chars.next() {
            if let Some(d) = c.to_digit(10) {
                out.push(c);
                file += d as u8;
            } else if c == '=' {
                let base = chars.next().ok_or("dangling `=` in Jieqi FEN")?;
                let sq = Square::<Xiangqi9x10>::from_file_rank(file, rank)
                    .ok_or("face-down piece on an off-board square")?;
                let role = home_role(sq)
                    .ok_or("face-down piece off its home square (no native Xiangqi role)")?;
                let ch = if base.is_ascii_uppercase() {
                    role.char().to_ascii_uppercase()
                } else {
                    role.char()
                };
                out.push(ch);
                file += 1;
            } else {
                out.push(c);
                file += 1;
            }
        }
    }
    Ok(format!("{out} {rest}"))
}

/// Build the Jieqi comparison corpus: the all-dark startpos, a fully-revealed
/// middlegame, two revealed tactical positions, and a **mixed** dark/revealed
/// position reached by playing a fixed seeded line from the start (so every
/// face-down piece provably stays on its home square).
fn corpus() -> Vec<Case> {
    const ALL_DARK: &str =
        "=d=d=d=dk=d=d=d=d/9/1=d5=d1/=d1=d1=d1=d1=d/9/9/=D1=D1=D1=D1=D/1=D5=D1/9/=D=D=D=DK=D=D=D=D w - - 0 1";

    let mut cases = vec![
        Case {
            label: "all-dark-start",
            fen: ALL_DARK.to_string(),
            depth: 3,
        },
        Case {
            label: "revealed-mid",
            fen: "r1oukuo1r/9/1cj3jc1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1CJ3JC1/9/R1OUKUO1R w - - 0 1"
                .to_string(),
            depth: 3,
        },
        Case {
            label: "revealed-horse-check",
            fen: "4k4/9/9/9/9/9/9/4j4/3U5/3K5 w - - 0 1".to_string(),
            depth: 4,
        },
        Case {
            label: "revealed-flying-gen",
            fen: "4k4/9/9/9/9/9/9/9/4R4/4K4 w - - 0 1".to_string(),
            depth: 4,
        },
    ];

    // A mixed dark/revealed position from a fixed seeded line off the all-dark
    // start. Playing reveals pieces (identity baseline), leaving the rest face-down
    // on their home squares — exactly the mid-reveal state worth cross-checking.
    if let Ok(mut pos) = Jieqi::from_fen(ALL_DARK) {
        let mut seed = 0x5EED_1234_ABCD_0001u64;
        for _ in 0..6 {
            let moves = pos.legal_moves();
            if moves.is_empty() {
                break;
            }
            seed = seed
                .wrapping_mul(0x2545_F491_4F6C_DD1D)
                .wrapping_add(0x9E37_79B9_7F4A_7C15);
            pos = pos.play(&moves[(seed >> 33) as usize % moves.len()]);
        }
        cases.push(Case {
            label: "mixed-reveal",
            fen: pos.to_fen(),
            depth: 3,
        });
    }

    cases
}

/// A measured Jieqi comparison row.
struct Row {
    label: &'static str,
    jieqi_fen: String,
    xiangqi_fen: String,
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

/// Run the Jieqi corpus: mcr Jieqi perft vs FSF `UCI_Variant xiangqi` perft on the
/// identity-reveal Xiangqi equivalent. Returns the number of mismatches (0 = all
/// matched, or FSF lacks `xiangqi` and the suite is skipped).
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!(
        "Jieqi (揭棋, hidden Xiangqi, 9x10, u128) — generic engine vs FSF \
UCI_Variant xiangqi on the identity-reveal equivalent (issue #278):"
    );
    println!("  (Jieqi is not an FSF variant; its full-information core is Xiangqi)");
    println!("  (requires an FSF built with largeboards=yes)");

    if !engine.has_variant("xiangqi") {
        println!("  SKIP: this FSF binary has no `xiangqi` variant (build it largeboards=yes).");
        return 0;
    }

    let head = format!(
        "{:<22} {:>5} {:>14} {:>14} {:>9} {:>10} {:>10} {:>8}",
        "position", "depth", "mcr nodes", "fsf nodes", "match", "mcr Mn/s", "fsf Mn/s", "mcr/fsf",
    );
    println!("{head}");
    println!("{}", "-".repeat(head.len()));

    let cases = corpus();
    let mut rows: Vec<Row> = Vec::with_capacity(cases.len());
    let mut mismatches = 0usize;

    for case in &cases {
        let depth = if full { case.depth + 1 } else { case.depth };
        match run_case(engine, case, depth) {
            Ok(row) => {
                if !row.matched {
                    mismatches += 1;
                }
                println!(
                    "{:<22} {:>5} {:>14} {:>14} {:>9} {:>10.1} {:>10.1} {:>7.2}x",
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
                eprintln!("skip jieqi/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "jieqi OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Jieqi positions matched FSF xiangqi ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Jieqi parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH jieqi/{} depth {}: mcr={} fsf={}  Jieqi FEN: {}  FSF xiangqi FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.jieqi_fen, r.xiangqi_fen,
            );
        }
    }
    mismatches
}

/// Run one Jieqi position through mcr's Jieqi perft and FSF's `xiangqi go perft` on
/// the identity-reveal equivalent.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Jieqi::from_fen(&case.fen).map_err(|e| format!("mcr rejected Jieqi FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Xiangqi9x10, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    let xiangqi_mcr = jieqi_to_xiangqi_mcr(&case.fen)?;
    let fsf_fen = fen_to_fsf(&xiangqi_mcr);
    engine.set_variant("xiangqi", false)?;
    engine.set_position(&fsf_fen)?;
    let fsf = engine.go_perft(depth, false)?;

    Ok(Row {
        label: case.label,
        jieqi_fen: case.fen.clone(),
        xiangqi_fen: xiangqi_mcr,
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

    /// The all-dark startpos converts to FSF's Xiangqi startpos; a fully-revealed
    /// FEN passes through unchanged but for the dialect rewrite.
    #[test]
    fn conversion_maps_dark_to_home_role() {
        let all_dark =
            "=d=d=d=dk=d=d=d=d/9/1=d5=d1/=d1=d1=d1=d1=d/9/9/=D1=D1=D1=D1=D/1=D5=D1/9/=D=D=D=DK=D=D=D=D w - - 0 1";
        let mcr = jieqi_to_xiangqi_mcr(all_dark).expect("converts");
        assert_eq!(
            mcr,
            "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/9/RJOUKUOJR w - - 0 1"
        );
        assert_eq!(
            fen_to_fsf(&mcr),
            "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1"
        );
    }

    /// Every corpus case parses on the Jieqi engine and converts to a FEN the
    /// Xiangqi engine accepts (so the dark pieces all sit on home squares).
    #[test]
    fn corpus_parses_and_converts() {
        for case in corpus() {
            Jieqi::from_fen(&case.fen).expect("Jieqi FEN parses");
            let mcr = jieqi_to_xiangqi_mcr(&case.fen).expect("converts to Xiangqi");
            mcr::geometry::Xiangqi::from_fen(&mcr).expect("Xiangqi equivalent parses");
        }
    }

    /// mcr's own cross-check (no FSF): Jieqi perft equals the Xiangqi perft of the
    /// converted equivalent for every corpus case, at the corpus depth.
    #[test]
    fn jieqi_perft_equals_xiangqi_equivalent() {
        for case in corpus() {
            let jq = Jieqi::from_fen(&case.fen).expect("Jieqi parses");
            let mcr = jieqi_to_xiangqi_mcr(&case.fen).expect("converts");
            let xq = mcr::geometry::Xiangqi::from_fen(&mcr).expect("Xiangqi parses");
            assert_eq!(
                gperft::<Xiangqi9x10, _>(&jq, case.depth),
                gperft::<Xiangqi9x10, _>(&xq, case.depth),
                "{}: Jieqi vs Xiangqi-equivalent perft({})",
                case.label,
                case.depth,
            );
        }
    }
}
