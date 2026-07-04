//! Shogun differential perft + timing against Fairy-Stockfish (issue #227).
//!
//! Shogun runs on mcr's **generic** engine (`mcr::geometry::Shogun`, a
//! `GenericPosition<Chess8x8, ShogunRules>`), like the other fairy variants, so it
//! has its own corpus and comparison loop here. The FSF side selects
//! `UCI_Variant shogun`, sets the FEN, runs `go perft`, asserts the node counts
//! match, and reports mcr-vs-FSF throughput.
//!
//! ## Not a built-in: variants.ini
//!
//! `shogun` is **not** an FSF built-in — it is defined in FSF's `variants.ini`.
//! The suite `load`s a `variants.ini` resolved from `$MCR_FSF_VARIANTS_INI`
//! before checking the variant (mirroring the Shinobi / Synochess suites), after
//! which `has_variant("shogun")` is true. If the env var is unset or the binary
//! still lacks `shogun`, the block skips gracefully.
//!
//! ## FEN dialect
//!
//! mcr and FSF render the same Shogun position with **different tokens for the
//! promoted pieces**. In FSF a promoted piece is a base letter carrying a `+`
//! marker (`+P` Commoner, `+N` Centaur, `+B` Archbishop, `+R` Chancellor, `+F`
//! Queen) and the bare fers is `F`. mcr reuses **existing roles** for those
//! compounds — Commoner `*u` (overflow, recycling the Advisor's `u`), Centaur =
//! Kheshig `w`, Archbishop = Hawk `a`, Chancellor = Elephant `e`, Queen `q`, Met
//! (fers) `m` — so [`to_fsf_dialect`] maps each mcr token to FSF's:
//!
//! | piece      | mcr | FSF |
//! |------------|-----|-----|
//! | Commoner   | `*u`/`*U` | `+p`/`+P` |
//! | Centaur    | `w`/`W`   | `+n`/`+N` |
//! | Archbishop | `a`/`A`   | `+b`/`+B` |
//! | Chancellor | `e`/`E`   | `+r`/`+R` |
//! | Queen      | `q`/`Q`   | `+f`/`+F` |
//! | Met (fers) | `m`/`M`   | `f`/`F`   |
//!
//! The overflow Commoner (`*u`/`*U`) is rewritten first; then the single-letter
//! compounds. The standard chess army (`pnbrk` / `PNBRK`) and the structural
//! fields carry none of these tokens, so the swap is unambiguous over the whole
//! FEN (board and the `[..]` hand bracket). The comparison asserts only node
//! counts, so the move-string dialect never matters.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Shogun};

use crate::uci::Engine;

/// One Shogun corpus position. The FEN is mcr's dialect; the FSF side translates
/// it via [`to_fsf_dialect`].
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Shogun comparison corpus: the FSF-confirmed startpos and three positions
/// exercising the drops + optional promotions, the promotion cap, and the
/// crazyhouse hand fed by captures.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "drops-promos",
        fen: "r3k3/8/4N3/8/8/8/8/3RK3[NPbp] w - - 0 1",
        depth: 3,
    },
    Case {
        label: "promo-cap",
        fen: "6k1/8/4N3/8/8/8/8/W5K1[Nn] w - - 0 1",
        depth: 4,
    },
    Case {
        label: "captures-to-hand",
        fen: "rnbqkbnr/ppp2ppp/8/3pp3/3PP3/8/PPP2PPP/RNBQKBNR[Pp] w KQkq - 0 4",
        depth: 4,
    },
];

/// Translates an mcr-dialect Shogun FEN to FSF's dialect by mapping the
/// promoted-piece tokens to FSF's `+base` markers (and the bare Met to FSF's
/// fers): Commoner `*u`/`*U → +p`/`+P`, Centaur `w`/`W → +n`/`+N`, Archbishop
/// `a`/`A → +b`/`+B`, Chancellor `e`/`E → +r`/`+R`, Queen `q`/`Q → +f`/`+F`, Met
/// `m`/`M → f`/`F`. The overflow Commoner (`*u`/`*U`) is rewritten first; the
/// single-letter compounds follow. The standard army (`pnbrk`/`PNBRK`) and the
/// structural fields carry no remapped token, so the swap is safe over the whole
/// FEN (including the `[..]` hand bracket).
pub(crate) fn to_fsf_dialect(fen: &str) -> String {
    // Only the placement field (the first whitespace-delimited token, board plus
    // its `[..]` holdings bracket) carries piece tokens. The structural fields —
    // the side-to-move `w`/`b`, the castling rights `KQkq`, the en-passant square,
    // and the clocks — must be left untouched, since several map-source letters
    // (`w`, `q`, `Q`) collide with them.
    let mut it = fen.splitn(2, ' ');
    let placement = it.next().unwrap_or("");
    let mapped = placement
        .replace("*U", "+P")
        .replace("*u", "+p")
        .replace('W', "+N")
        .replace('w', "+n")
        .replace('A', "+B")
        .replace('a', "+b")
        .replace('E', "+R")
        .replace('e', "+r")
        .replace('Q', "+F")
        .replace('q', "+f")
        .replace('M', "F")
        .replace('m', "f");
    match it.next() {
        Some(rest) => format!("{mapped} {rest}"),
        None => mapped,
    }
}

/// A measured Shogun comparison row.
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

/// Run the Shogun corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all positions matched). Prints a table and a one-line summary. If the FSF
/// binary does not advertise the `shogun` variant (no `variants.ini` loaded), the
/// whole block is skipped (returns 0) rather than reporting spurious mismatches.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Shogun — generic engine vs FSF UCI_Variant shogun (issue #227):");

    // Shogun is an INI variant (not an FSF built-in): load FSF's variants.ini from
    // `$MCR_FSF_VARIANTS_INI` before checking for the variant, mirroring the
    // Shinobi suite. Skip gracefully if it is unset or still lacks `shogun`.
    let ini = std::env::var("MCR_FSF_VARIANTS_INI").unwrap_or_default();
    if ini.is_empty() {
        println!("  SKIP: set $MCR_FSF_VARIANTS_INI to an FSF variants.ini defining `shogun`.");
        return 0;
    }
    if let Err(e) = engine.load_variants(&ini) {
        println!("  SKIP: could not load variants.ini ({ini}): {e}");
        return 0;
    }
    if !engine.has_variant("shogun") {
        println!("  SKIP: the loaded FSF binary still does not advertise `shogun`.");
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
                eprintln!("skip shogun/{}: {e}", case.label);
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
            "shogun OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Shogun positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Shogun parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH shogun/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Shogun position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mcr side: the generic Shogun position (mcr dialect).
    let pos = Shogun::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // FSF side: translate the promoted-piece tokens to FSF's dialect.
    let fsf_fen = to_fsf_dialect(case.fen);
    engine.set_variant("shogun", false)?;
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

    /// The corpus FENs all parse on the generic Shogun engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_shogun.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 3u32, 8978u64),
            ("drops-promos", 3, 682066),
            ("promo-cap", 3, 77223),
            ("captures-to-hand", 3, 195868),
        ];
        for case in CASES {
            let pos = Shogun::from_fen(case.fen).expect("corpus FEN parses");
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

    /// The dialect swap rewrites mcr's promoted-piece tokens to FSF's `+base`
    /// markers (and the bare Met to FSF's fers), and leaves the standard army and
    /// the structural fields untouched.
    #[test]
    fn dialect_swap_maps_promoted_tokens() {
        // The start array: the d-file Queen (`q`/`Q`) is FSF's promoted fers `+f`/`+F`.
        let mcr = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1";
        let fsf = to_fsf_dialect(mcr);
        assert_eq!(
            fsf,
            "rnb+fkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNB+FKBNR[] w KQkq - 0 1"
        );
        // The Centaur (`W`) maps to `+N`, and the bare hand pieces stay standard.
        let cap = to_fsf_dialect("6k1/8/4N3/8/8/8/8/W5K1[Nn] w - - 0 1");
        assert_eq!(cap, "6k1/8/4N3/8/8/8/8/+N5K1[Nn] w - - 0 1");
    }
}
