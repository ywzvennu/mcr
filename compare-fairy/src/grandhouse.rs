//! Grandhouse (10x10) differential perft + timing against Fairy-Stockfish
//! (issue #265) — **Grand chess plus crazyhouse drops**.
//!
//! Grandhouse runs on mcr's **generic** `u128` engine (`mcr::geometry::Grandhouse`,
//! a `GenericPosition<Grand10x10, GrandhouseRules>`), like Grand. The FSF side
//! selects `UCI_Variant grandhouse`, sets the FEN, runs `go perft`, asserts the
//! node counts match, and reports mcr-vs-FSF throughput.
//!
//! ## Not a built-in: variants.ini
//!
//! `grandhouse` is **not** an FSF built-in — it is defined in FSF's `variants.ini`
//! as `[grandhouse:grand]`. The suite `load`s a `variants.ini` resolved from
//! `$MCR_FSF_VARIANTS_INI` before checking the variant (mirroring the Shogun /
//! Shinobi suites), after which `has_variant("grandhouse")` is true. If the env
//! var is unset or the binary still lacks `grandhouse`, the block skips gracefully.
//! FSF must also be built with large-board support (`make ... largeboards=yes`) for
//! the 10x10 board.
//!
//! ## FEN dialect
//!
//! As with Grand, mcr spells the marshal `e`/`E` (its
//! [`WideRole::Elephant`](mcr::geometry::WideRole) rook-knight compound) where FSF
//! uses `c`/`C`; the cardinal is `a`/`A` in both. [`fen_to_fsf`] rewrites the
//! marshal's letter across the placement field (which carries the crazyhouse
//! `[..]` hand bracket too, so a marshal *in hand* is mapped as well); the promoted
//! `~` marker and the rest are byte-identical.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Grand10x10, Grandhouse};

use crate::uci::Engine;

/// One Grandhouse corpus position, in the **mcr dialect** (marshal = `e`).
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Grandhouse comparison corpus: the FSF-confirmed startpos, a drop-heavy
/// position (both sides hold a queen + pawn in hand, exercising the colour-aware
/// pawn drop region), a promotion/demotion position (exercising the promoted mask
/// — a captured promoted piece banks a Pawn), and a developed midgame with a
/// knight in each hand. Depths are modest by default; `full` deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "r8r/1nbqkeabn1/pppppppppp/10/10/10/10/PPPPPPPPPP/1NBQKEABN1/R8R[] w - - 0 1",
        depth: 3,
    },
    Case {
        label: "hands",
        fen: "r8r/1nbqkeabn1/pppppppppp/10/10/10/10/PPPPPPPPPP/1NBQKEABN1/R8R[QPqp] w - - 0 1",
        depth: 2,
    },
    Case {
        label: "promo",
        fen: "1rk7/P9/10/10/10/10/10/10/10/5K4[] w - - 0 1",
        depth: 4,
    },
    Case {
        label: "midgame",
        fen: "r8r/2bqkeab2/pppp1ppppp/2n4n2/3Np5/3P6/7N2/PPP1PPPPPP/2BQKEAB2/R8R[Nn] w - - 1 4",
        depth: 3,
    },
];

/// Rewrite an mcr-dialect Grandhouse FEN into the FSF dialect: the marshal's
/// letter `e`/`E` becomes `c`/`C` in the *placement* field only (which includes
/// the crazyhouse `[..]` hand bracket). The cardinal `a`/`A`, the promoted `~`
/// marker, and every other field are unchanged.
pub fn fen_to_fsf(fen: &str) -> String {
    let map = |c| match c {
        'e' => 'c',
        'E' => 'C',
        other => other,
    };
    // Only the placement field (up to the first space) holds piece letters.
    match fen.split_once(' ') {
        Some((placement, rest)) => {
            let mapped: String = placement.chars().map(map).collect();
            format!("{mapped} {rest}")
        }
        None => fen.chars().map(map).collect(),
    }
}

/// A measured Grandhouse comparison row.
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

/// Run the Grandhouse corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all positions matched). Prints a table and a one-line summary. If the FSF
/// binary does not advertise the `grandhouse` variant (no `variants.ini` loaded),
/// the whole block is skipped (returns 0) rather than reporting spurious
/// mismatches.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!(
        "Grandhouse (10x10, u128) — generic engine vs FSF UCI_Variant grandhouse (issue #265):"
    );
    println!(
        "  (requires an FSF built with largeboards=yes + a variants.ini defining `grandhouse`)"
    );

    // Grandhouse is an INI variant (not an FSF built-in): load FSF's variants.ini
    // from `$MCR_FSF_VARIANTS_INI` before checking for the variant, mirroring the
    // Shogun suite. Skip gracefully if it is unset or still lacks `grandhouse`.
    let ini = std::env::var("MCR_FSF_VARIANTS_INI").unwrap_or_default();
    if ini.is_empty() {
        println!("  SKIP: set $MCR_FSF_VARIANTS_INI to an FSF variants.ini defining `grandhouse`.");
        return 0;
    }
    if let Err(e) = engine.load_variants(&ini) {
        println!("  SKIP: could not load variants.ini ({ini}): {e}");
        return 0;
    }
    if !engine.has_variant("grandhouse") {
        println!("  SKIP: the loaded FSF binary still does not advertise `grandhouse`.");
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
                eprintln!("skip grandhouse/{}: {e}", case.label);
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
            "grandhouse OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Grandhouse positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Grandhouse parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH grandhouse/{} depth {}: mcr={} fsf={}  mcr FEN: {}  FSF FEN: {}",
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

/// Run one Grandhouse position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mcr side: the generic Grandhouse position over the 10x10 u128 geometry.
    let pos = Grandhouse::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Grand10x10, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // FSF side: rewrite the marshal's letter into the FSF dialect.
    let fsf_fen = fen_to_fsf(case.fen);
    engine.set_variant("grandhouse", false)?;
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

    /// The corpus FENs all parse on the generic Grandhouse engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in
    /// `tests/perft_grandhouse.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 4225u64),
            ("hands", 27108),
            ("promo", 139),
            ("midgame", 16941),
        ];
        for case in CASES {
            let pos = Grandhouse::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Grand10x10, _, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }

    /// The mcr -> FSF dialect rewrite swaps only the marshal's letter (including a
    /// marshal held in the crazyhouse hand) and leaves the cardinal, the promoted
    /// `~` marker, and every other field intact.
    #[test]
    fn fen_dialect_rewrites_only_the_marshal() {
        let mcr = "r8r/1nbqkeabn1/pppppppppp/10/10/10/10/PPPPPPPPPP/1NBQKEABN1/R8R[Ee] w - - 0 1";
        let fsf = "r8r/1nbqkcabn1/pppppppppp/10/10/10/10/PPPPPPPPPP/1NBQKCABN1/R8R[Cc] w - - 0 1";
        assert_eq!(fen_to_fsf(mcr), fsf);
        // A promoted queen's `~` and an `e3` en-passant token survive untouched.
        let out = fen_to_fsf("5k4/4Q~5/10/10/10/10/10/10/10/5K4[] b - e3 1 9");
        assert_eq!(out, "5k4/4Q~5/10/10/10/10/10/10/10/5K4[] b - e3 1 9");
    }
}
