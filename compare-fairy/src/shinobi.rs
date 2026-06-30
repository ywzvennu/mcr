//! Shinobi differential perft + timing against Fairy-Stockfish (issue #213).
//!
//! Shinobi runs on mce's **generic** engine (`mce::geometry::Shinobi`, a
//! `GenericPosition<Chess8x8, ShinobiRules>`), like the other fairy variants, so
//! it has its own corpus and comparison loop here. The FSF side selects
//! `UCI_Variant shinobi`, sets the FEN, runs `go perft`, asserts the node counts
//! match, and reports mce-vs-FSF throughput.
//!
//! ## Not a built-in: variants.ini
//!
//! Unlike the other variants here, `shinobi` is **not** an FSF built-in — it is
//! defined in FSF's `variants.ini`. The suite `load`s a `variants.ini` resolved
//! from `$MCE_FSF_VARIANTS_INI` before checking the variant (mirroring the
//! Synochess / Orda suites), after which `has_variant("shinobi")` is true. If the
//! env var is unset or the binary still lacks `shinobi`, the block skips
//! gracefully.
//!
//! ## FEN dialect
//!
//! mce and FSF render the same Shinobi position with **different clan-piece
//! tokens**. FSF's `shinobi` uses `c d h j` (Commoner, Bers, Shogi Knight,
//! Archbishop). mce reuses `c`/`h` for its Cannon / Hoplite roles, so the Commoner
//! and Shogi Knight are **overflow** roles spelled with the `*` prefix and a
//! recycled base letter: Commoner `*u` (recycling the Advisor's `u`), Shogi Knight
//! `*n` (recycling the Knight's `n`); the Archbishop is the Hawk `a` (= B+N). The
//! Bers `d` (= Spartan General, Rook + Ferz), the Fers `m` (= Met), the Lance `l`,
//! and every standard piece already share FSF's letters. [`to_fsf_dialect`] maps
//! mce's `*u → c`, `*n → h`, `a → j` over the whole FEN (board and the `[..]` hand
//! bracket). The standard Black army and the structural fields carry none of the
//! remapped tokens, so the swap is unambiguous. The comparison asserts only node
//! counts, so the move-string dialect never matters.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mce::geometry::{perft as gperft, Chess8x8, Shinobi};

use crate::uci::Engine;

/// One Shinobi corpus position. The FEN is mce's dialect; the FSF side translates
/// it via [`to_fsf_dialect`].
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Shinobi comparison corpus: the FSF-confirmed startpos and three
/// middlegames exercising the drop reserve, the clan pieces, and the mandatory
/// promotion zone, plus a flag-win race whose terminal node has zero children.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/L*N1*UK1*NL[L*NMMDA] w kq - 0 1",
        depth: 4,
    },
    Case {
        label: "mid-developed",
        fen: "r1bqkbnr/ppp2ppp/2n5/3p4/2Lp4/3M*N3/PPPPPPPP/L*N1*UK1*NL[AM] w kq - 0 5",
        depth: 4,
    },
    Case {
        label: "mid-promo-zone",
        fen: "r1bqk2r/ppp1bppp/2n1p2n/2p2*N2/3M4/8/PPPPPPPP/L*N1*UK1*NL[LAM] w kq - 0 7",
        depth: 4,
    },
    Case {
        label: "mid-drops",
        fen: "r1bqk2r/1pppbppp/p1n1pn2/P7/L7/1*NM5/1PPPPPPP/1*N1*UK1*NL[LADM] w kq - 2 6",
        depth: 4,
    },
    Case {
        label: "flag-race",
        fen: "8/4K3/8/8/8/4k3/8/8[] w - - 0 1",
        depth: 3,
    },
];

/// Translates an mce-dialect Shinobi FEN to FSF's dialect by mapping the clan
/// piece tokens to FSF's letters: Commoner `*U`/`*u → C`/`c`, Shogi Knight
/// `*N`/`*n → H`/`h`, Archbishop `A`/`a → J`/`j`. The Commoner and Shogi Knight
/// are mce **overflow** roles, written with the `*` prefix and a recycled base
/// letter (`u` / `n`); FSF spells them as single letters, so the `*`-prefixed
/// two-char tokens are rewritten first, then the single-letter Archbishop. Bers
/// `d`/`D`, Fers `m`/`M`, Lance `l`/`L`, King `k`/`K` and the standard army carry
/// no remapped letter, so the swap is safe over the whole FEN (including the
/// `[..]` hand bracket).
pub(crate) fn to_fsf_dialect(fen: &str) -> String {
    let stage = fen
        .replace("*U", "C")
        .replace("*u", "c")
        .replace("*N", "H")
        .replace("*n", "h");
    stage
        .chars()
        .map(|c| match c {
            'a' => 'j',
            'A' => 'J',
            other => other,
        })
        .collect()
}

/// A measured Shinobi comparison row.
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

/// Run the Shinobi corpus through mce and FSF. Returns the number of mismatches
/// (0 = all positions matched). Prints a table and a one-line summary. If the FSF
/// binary does not advertise the `shinobi` variant (no `variants.ini` loaded), the
/// whole block is skipped (returns 0) rather than reporting spurious mismatches.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Shinobi — generic engine vs FSF UCI_Variant shinobi (issue #213):");

    // Shinobi is an INI variant (not an FSF built-in): load FSF's variants.ini
    // from `$MCE_FSF_VARIANTS_INI` before checking for the variant, mirroring the
    // Synochess suite. Skip gracefully if it is unset or still lacks `shinobi`.
    let ini = std::env::var("MCE_FSF_VARIANTS_INI").unwrap_or_default();
    if ini.is_empty() {
        println!("  SKIP: set $MCE_FSF_VARIANTS_INI to an FSF variants.ini defining `shinobi`.");
        return 0;
    }
    if let Err(e) = engine.load_variants(&ini) {
        println!("  SKIP: could not load variants.ini ({ini}): {e}");
        return 0;
    }
    if !engine.has_variant("shinobi") {
        println!("  SKIP: the loaded FSF binary still does not advertise `shinobi`.");
        return 0;
    }
    let head = format!(
        "{:<16} {:>5} {:>14} {:>14} {:>9} {:>10} {:>10} {:>8}",
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
                    "{:<16} {:>5} {:>14} {:>14} {:>9} {:>10.1} {:>10.1} {:>7.2}x",
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
                eprintln!("skip shinobi/{}: {e}", case.label);
            }
        }
    }

    // Node-weighted aggregate throughput.
    let nodes: u64 = rows.iter().map(|r| r.mce_nodes).sum();
    let mce_s: f64 = rows.iter().map(|r| r.mce_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mce_s > 0.0 && fsf_s > 0.0 {
        println!(
            "shinobi OVERALL: {nodes} nodes verified; mce {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mce_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mce_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Shinobi positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Shinobi parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH shinobi/{} depth {}: mce={} fsf={}  FEN: {}",
                r.label, r.depth, r.mce_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Shinobi position through mce's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mce side: the generic Shinobi position (mce dialect).
    let pos = Shinobi::from_fen(case.fen).map_err(|e| format!("mce rejected FEN: {e:?}"))?;
    let mce_start = Instant::now();
    let mce_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mce_secs = mce_start.elapsed().as_secs_f64();

    // FSF side: translate the clan piece letters to FSF's dialect.
    let fsf_fen = to_fsf_dialect(case.fen);
    engine.set_variant("shinobi", false)?;
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

    /// The corpus FENs all parse on the generic Shinobi engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_shinobi.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 3u32, 224144u64),
            ("mid-developed", 3, 82712),
            ("mid-promo-zone", 3, 176397),
            ("mid-drops", 3, 193211),
            ("flag-race", 3, 301),
        ];
        for case in CASES {
            let pos = Shinobi::from_fen(case.fen).expect("corpus FEN parses");
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

    /// The dialect swap rewrites mce's overflow clan tokens (`*u`/`*n`) and the
    /// Archbishop (`a`) to FSF's single letters, and leaves the standard army, the
    /// shared clan letters (`d m l`), and the structural fields untouched.
    #[test]
    fn dialect_swap_maps_clan_letters() {
        let mce = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/L*N1*UK1*NL[L*NMMDA] w kq - 0 1";
        let fsf = to_fsf_dialect(mce);
        assert_eq!(
            fsf,
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/LH1CK1HL[LHMMDJ] w kq - 0 1"
        );
        // The standard Black army, the shared Lance/Bers/Fers letters, and the
        // castling / clock fields are unchanged.
        assert!(fsf.contains("rnbqkbnr") && fsf.contains("pppppppp") && fsf.contains("w kq -"));
    }
}
