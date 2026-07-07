//! Paradigm chess differential perft + timing against Fairy-Stockfish.
//!
//! Paradigm chess runs on mcr's **generic** engine (`mcr::geometry::Paradigm`, a
//! `GenericPosition<Chess8x8, ParadigmRules>`), like the other fairy variants, so it
//! has its own corpus and comparison loop here. The FSF side selects
//! `UCI_Variant paradigm` (a built-in — no `variants.ini` needed), sets the FEN,
//! runs `go perft`, asserts the node counts match, and reports mcr-vs-FSF
//! throughput.
//!
//! ## FEN dialect
//!
//! mcr and FSF render the same Paradigm position with **different letters** for the
//! compound piece. FSF spells the Bishop-Horse `b` (its `CUSTOM_PIECE_1`, Betza
//! `BnN`); mcr already names `b` the standard Bishop, and every single-letter base
//! plus the `*` / `**` / `=` / `***` overflow banks are exhausted, so the
//! Bishop-Horse takes the fifth-tier overflow token `****x`. [`to_fsf_dialect`]
//! rewrites the placement field `****x → b` (both cases) so the FSF FEN matches; the
//! side-to-move / castling / clock fields are left intact (they carry no remapped
//! token). The comparison asserts only node counts, so the move-string dialect never
//! matters.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no GPL
//! code is linked, and Paradigm chess needs no INI.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Paradigm};

use crate::uci::Engine;

/// One Paradigm corpus position. The FEN is mcr's dialect; the FSF side translates
/// it via [`to_fsf_dialect`].
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Paradigm comparison corpus (all FSF-confirmed): the startpos (both colours),
/// a lone open-board Bishop-Horse (diagonals + hobbled horse leaps), a horse **check
/// answered by a leg-block**, and a castling middlegame with four active
/// Bishop-Horses firing bishop slides, horse leaps, blocks, and captures on one tree.
const CASES: &[Case] = &[
    Case {
        label: "startpos-w",
        fen: "rn****xqk****xnr/pppppppp/8/8/8/8/PPPPPPPP/RN****XQK****XNR w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "startpos-b",
        fen: "rn****xqk****xnr/pppppppp/8/8/8/8/PPPPPPPP/RN****XQK****XNR b KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "open",
        fen: "4k3/8/8/8/3****X4/8/8/4K3 w - - 0 1",
        depth: 4,
    },
    Case {
        label: "leg-block",
        fen: "4k3/r7/3****X4/8/8/8/8/4K3 b - - 0 1",
        depth: 4,
    },
    Case {
        label: "midgame",
        fen: "rn2k2r/ppp1qppp/3p1n2/2****x****Xp3/2****X****xP3/3P1N2/PPP1QPPP/RN2K2R w KQkq - 0 1",
        depth: 4,
    },
];

/// Translates an mcr-dialect Paradigm FEN to FSF's dialect: rewrite the Bishop-Horse
/// overflow token `****x → b` (both cases). Applied to the **placement field only**
/// (the side-to-move / castling / clock fields are left intact); they carry no
/// `****`-prefixed token, so the swap is unambiguous.
pub(crate) fn to_fsf_dialect(fen: &str) -> String {
    let mut parts = fen.splitn(2, ' ');
    let placement = parts
        .next()
        .unwrap_or("")
        // Bishop-Horse: mcr `****x` → FSF `b` (both cases). The four-star prefix is
        // consumed atomically with its letter.
        .replace("****X", "B")
        .replace("****x", "b");
    match parts.next() {
        Some(rest) => format!("{placement} {rest}"),
        None => placement,
    }
}

/// A measured Paradigm comparison row.
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

/// Run the Paradigm corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all matched). `paradigm` is a FSF built-in, so if this binary does not
/// advertise it the block is skipped cleanly (returns 0).
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Paradigm — generic engine vs FSF UCI_Variant paradigm:");

    if !engine.has_variant("paradigm") {
        println!("  (skipped: this FSF binary does not advertise UCI_Variant paradigm)");
        return 0;
    }

    let head = format!(
        "{:<14} {:>5} {:>14} {:>14} {:>9} {:>10} {:>10} {:>8}",
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
                    "{:<14} {:>5} {:>14} {:>14} {:>9} {:>10.1} {:>10.1} {:>7.2}x",
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
                eprintln!("skip paradigm/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "paradigm OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s \
             ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Paradigm positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Paradigm parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH paradigm/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Paradigm position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Paradigm::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    let fsf_fen = to_fsf_dialect(case.fen);
    engine.set_variant("paradigm", false)?;
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

    /// The corpus FENs all parse on the generic Paradigm engine, round-trip through
    /// mcr's FEN I/O, and the pinned shallow counts match the FSF-confirmed numbers
    /// in `tests/perft_paradigm.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos-w", 3u32, 9062u64),
            ("startpos-b", 3, 9062),
            ("open", 3, 2280),
            ("leg-block", 3, 858),
            ("midgame", 3, 51110),
        ];
        for case in CASES {
            let pos = Paradigm::from_fen(case.fen).expect("corpus FEN parses");
            assert_eq!(pos.to_fen(), case.fen, "{} round-trips", case.label);
            let (_, depth, want) = pinned
                .iter()
                .find(|(l, _, _)| *l == case.label)
                .copied()
                .expect("a pinned count for the case");
            assert_eq!(
                gperft::<Chess8x8, _, _>(&pos, depth),
                want,
                "{} perft",
                case.label
            );
        }
    }

    /// The dialect swap rewrites the Bishop-Horse `****x → b` (both cases) over the
    /// placement field and leaves the structural fields untouched.
    #[test]
    fn dialect_swap_maps_overflow_pieces() {
        let mcr = "rn****xqk****xnr/pppppppp/8/8/8/8/PPPPPPPP/RN****XQK****XNR w KQkq - 0 1";
        assert_eq!(
            to_fsf_dialect(mcr),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
        // An inline `****X` mid-rank and the bare structural letters are handled
        // correctly (no other `****` token to consume).
        assert_eq!(
            to_fsf_dialect("4k3/r7/3****X4/8/8/8/8/4K3 b - - 0 1"),
            "4k3/r7/3B4/8/8/8/8/4K3 b - - 0 1"
        );
    }
}
