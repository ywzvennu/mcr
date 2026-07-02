//! Courier chess (medieval 12x8) differential perft + timing against
//! Fairy-Stockfish (issue #376).
//!
//! Courier runs on mce's **generic** engine (`mce::geometry::Courier`, a
//! `GenericPosition<Courier12x8, CourierRules>`), not the concrete `AnyVariant`
//! layer, so it has its own small corpus and comparison loop here (like Shatranj).
//! `courier` is an FSF **built-in** (no `variants.ini` needed, but FSF must be
//! built `largeboards=yes` for the 12-wide board): select `UCI_Variant courier`,
//! set the FEN, run `go perft`, assert the node counts match, and report
//! mce-vs-FSF throughput.
//!
//! ## FEN dialect
//!
//! mce and FSF render the same Courier position with different piece letters. FSF
//! spells the Courier (Alfil) `e`, the Man `m`, the Wazir `w`, and the Ferz `f`;
//! mce reuses those letters (or their bare forms) for other roles, so the Courier
//! pieces take mce's overflow / Met tokens — the Alfil `*x`, the Man `*u`, the
//! Wazir `*j`, and the Ferz the Makruk Met `m`. [`to_fsf_dialect`] rewrites them
//! (`*x → e`, `*u → m`, `*j → w`, `m → f`) over the FEN.
//!
//! Because mce's bare `m` (Ferz) maps to FSF's `f` *and* mce's overflow `*u` (Man)
//! maps to FSF's `m`, a naive two-pass `replace` would re-map the freshly produced
//! `m`. The rewrite is therefore a **single left-to-right scan**: an overflow token
//! (`*` + base) is mapped as a unit, and every other char is mapped once — so no
//! output letter is ever re-considered. The comparison asserts only node counts, so
//! the move-string dialect never matters.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mce::geometry::{perft as gperft, Courier, Courier12x8};

use crate::uci::Engine;

/// One Courier corpus position. The FEN is mce's dialect; the FSF side translates
/// it via [`to_fsf_dialect`].
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Courier comparison corpus: the FSF-confirmed startpos, a developed
/// midgame (Alfil / Ferz / Man / Wazir / Bishop in the open), a Ferz-promotion
/// endgame, and a bared-king endgame exercising the baring-loss truncation (FSF
/// reports it terminal, so `go perft` is 0). Depths are kept modest by default;
/// `full` deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rn*xb*uk1*jb*xnr/1ppppp1pppp1/6m5/p5p4p/P5P4P/6M5/1PPPPP1PPPP1/RN*XB*UK1*JB*XNR w - - 0 1",
        depth: 4,
    },
    Case {
        label: "midgame",
        fen: "r1*xb*uk1*jb*xnr/2ppp2pppp1/1pn2pm5/p5p4p/P5P4P/1P2*XPM5/2PPP2PPPP1/RN1B*UK1*JB*XNR w - - 0 4",
        depth: 4,
    },
    Case {
        label: "promo",
        fen: "3r7k/1P10/12/12/12/12/12/K11 w - - 0 1",
        depth: 4,
    },
    Case {
        label: "bared-loss",
        fen: "4k7/12/12/12/12/12/3*U*U7/4K7 b - - 0 1",
        depth: 4,
    },
];

/// Translates an mce-dialect Courier FEN to FSF's dialect in a single left-to-right
/// scan: the overflow tokens (`*x → e`, `*u → m`, `*j → w`, both cases) are mapped
/// as units, and the bare Ferz (`m → f`, `M → F`) is mapped per char. A one-pass
/// scan is required because `*u` produces an `m` that a second pass would wrongly
/// re-map to `f`.
pub(crate) fn to_fsf_dialect(fen: &str) -> String {
    let mut out = String::with_capacity(fen.len());
    let mut chars = fen.chars();
    while let Some(c) = chars.next() {
        if c == '*' {
            // An overflow token: `*` followed by a recycled base letter.
            let base = chars.next().unwrap_or('*');
            out.push(match base {
                'x' => 'e', // Courier (Alfil)
                'X' => 'E',
                'u' => 'm', // Man (Commoner)
                'U' => 'M',
                'j' => 'w', // Wazir
                'J' => 'W',
                other => other,
            });
        } else {
            out.push(match c {
                'm' => 'f', // Ferz (Met)
                'M' => 'F',
                other => other,
            });
        }
    }
    out
}

/// A measured Courier comparison row.
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

/// Run the Courier corpus through mce and FSF. Returns the number of mismatches
/// (0 = all positions matched). Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!(
        "Courier chess (medieval 12x8) — generic engine vs FSF UCI_Variant courier (issue #376):"
    );
    let head = format!(
        "{:<12} {:>5} {:>14} {:>14} {:>9} {:>10} {:>10} {:>8}",
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
                    "{:<12} {:>5} {:>14} {:>14} {:>9} {:>10.1} {:>10.1} {:>7.2}x",
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
                eprintln!("skip courier/{}: {e}", case.label);
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
            "courier OVERALL: {nodes} nodes verified; mce {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mce_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mce_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Courier positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Courier parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH courier/{} depth {}: mce={} fsf={}  FEN: {}",
                r.label, r.depth, r.mce_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Courier position through mce's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mce side: the generic Courier position.
    let pos = Courier::from_fen(case.fen).map_err(|e| format!("mce rejected FEN: {e:?}"))?;
    let mce_start = Instant::now();
    let mce_nodes = gperft::<Courier12x8, _>(&pos, depth);
    let mce_secs = mce_start.elapsed().as_secs_f64();

    // FSF side: rewrite the mce dialect to FSF's letters.
    let fsf_fen = to_fsf_dialect(case.fen);
    engine.set_variant("courier", false)?;
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

    /// The corpus FENs all parse on the generic Courier engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_courier.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 2u32, 678u64),
            ("midgame", 2, 966),
            ("promo", 2, 79),
            ("bared-loss", 2, 0),
        ];
        for (label, depth, want) in pinned {
            let case = CASES.iter().find(|c| c.label == label).expect("label");
            let pos = Courier::from_fen(case.fen).expect("corpus FEN parses");
            assert_eq!(
                gperft::<Courier12x8, _>(&pos, depth),
                want,
                "{label} perft({depth})"
            );
        }
    }

    #[test]
    fn dialect_round_trips_pieces() {
        assert_eq!(
            to_fsf_dialect(
                "rn*xb*uk1*jb*xnr/1ppppp1pppp1/6m5/p5p4p/P5P4P/6M5/1PPPPP1PPPP1/RN*XB*UK1*JB*XNR w - - 0 1"
            ),
            "rnebmk1wbenr/1ppppp1pppp1/6f5/p5p4p/P5P4P/6F5/1PPPPP1PPPP1/RNEBMK1WBENR w - - 0 1"
        );
    }
}
