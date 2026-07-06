//! New Zealand chess differential perft + timing against Fairy-Stockfish.
//!
//! New Zealand chess runs on mcr's **generic** engine (`mcr::geometry::Newzealand`, a
//! `GenericPosition<Chess8x8, NewzealandRules>`), like the other fairy variants, so it
//! has its own corpus and comparison loop here. The FSF side selects
//! `UCI_Variant newzealand` (a built-in — no `variants.ini` needed), sets the FEN,
//! runs `go perft`, asserts the node counts match, and reports mcr-vs-FSF throughput.
//!
//! ## FEN dialect
//!
//! mcr and FSF render the same New Zealand position with **different letters** for the
//! two capture-swap pieces. FSF spells the ROOKNI `r` and the KNIROO `n` (so its FEN
//! reads like standard chess); mcr already names `r`/`n` the Rook / Knight, so the
//! KNIROO reuses the Orda Lancer `f` and the ROOKNI takes the fifth-tier overflow
//! token `****k`. [`to_fsf_dialect`] rewrites the placement field `****k → r` and
//! `f → n` (both cases) so the FSF FEN matches; the side-to-move / castling / clock
//! fields are left intact (they carry no remapped token). The comparison asserts only
//! node counts, so the move-string dialect never matters.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no GPL
//! code is linked, and New Zealand chess needs no INI.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Newzealand};

use crate::uci::Engine;

/// One New Zealand corpus position. The FEN is mcr's dialect; the FSF side translates
/// it via [`to_fsf_dialect`].
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The New Zealand comparison corpus (all FSF-confirmed): the startpos (both colours),
/// a ROOKNI knight-capture, a KNIROO rook-capture, a KNIROO rook-line **pin**, a
/// ROOKNI knight-**check**, and a castling middlegame that fires ROOKNI castles,
/// double steps, and en passant on one tree.
const CASES: &[Case] = &[
    Case {
        label: "startpos-w",
        fen: "****kfbqkbf****k/pppppppp/8/8/8/8/PPPPPPPP/****KFBQKBF****K w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "startpos-b",
        fen: "****kfbqkbf****k/pppppppp/8/8/8/8/PPPPPPPP/****KFBQKBF****K b KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "rookni-cap",
        fen: "4k3/8/4p3/8/3****K4/8/8/4K3 w - - 0 1",
        depth: 4,
    },
    Case {
        label: "kniroo-cap",
        fen: "4k3/8/8/8/4f3/8/4****K3/4K3 b - - 0 1",
        depth: 4,
    },
    Case {
        label: "kniroo-pin",
        fen: "k3f3/8/8/8/4B3/8/8/4K3 w - - 0 1",
        depth: 4,
    },
    Case {
        label: "rookni-check",
        fen: "8/8/4k3/8/3****K4/8/8/4K3 b - - 0 1",
        depth: 4,
    },
    Case {
        label: "castling",
        fen: "****k3k2****k/pppppppp/8/8/8/8/PPPPPPPP/****K3K2****K w KQkq - 0 1",
        depth: 4,
    },
];

/// Translates an mcr-dialect New Zealand FEN to FSF's dialect: rewrite the ROOKNI
/// overflow token `****k → r` and the KNIROO `f → n` (both cases). Applied to the
/// **placement field only** (the side-to-move / castling / clock fields are left
/// intact); they carry no `****`-prefixed token and no bare `f`, so the swap is
/// unambiguous. The bare `k`/`K` king and the other structural letters are untouched.
pub(crate) fn to_fsf_dialect(fen: &str) -> String {
    let mut parts = fen.splitn(2, ' ');
    let placement = parts
        .next()
        .unwrap_or("")
        // ROOKNI: mcr `****k` → FSF `r` (both cases). The four-star prefix is consumed
        // atomically with its letter, so the bare `k`/`K` king is left alone.
        .replace("****K", "R")
        .replace("****k", "r")
        // KNIROO: mcr `f` → FSF `n` (both cases).
        .replace('F', "N")
        .replace('f', "n");
    match parts.next() {
        Some(rest) => format!("{placement} {rest}"),
        None => placement,
    }
}

/// A measured New Zealand comparison row.
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

/// Run the New Zealand corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all matched). `newzealand` is a FSF built-in, so if this binary does not
/// advertise it the block is skipped cleanly (returns 0).
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("New Zealand — generic engine vs FSF UCI_Variant newzealand:");

    if !engine.has_variant("newzealand") {
        println!("  (skipped: this FSF binary does not advertise UCI_Variant newzealand)");
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
                eprintln!("skip newzealand/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "newzealand OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s \
             ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} New Zealand positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} New Zealand parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH newzealand/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one New Zealand position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Newzealand::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    let fsf_fen = to_fsf_dialect(case.fen);
    engine.set_variant("newzealand", false)?;
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

    /// The corpus FENs all parse on the generic New Zealand engine, round-trip through
    /// mcr's FEN I/O, and the pinned shallow counts match the FSF-confirmed numbers in
    /// `tests/perft_newzealand.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos-w", 3u32, 8976u64),
            ("startpos-b", 3, 8976),
            ("rookni-cap", 3, 2037),
            ("kniroo-cap", 3, 1735),
            ("kniroo-pin", 3, 194),
            ("rookni-check", 3, 847),
            ("castling", 3, 15206),
        ];
        for case in CASES {
            let pos = Newzealand::from_fen(case.fen).expect("corpus FEN parses");
            assert_eq!(pos.to_fen(), case.fen, "{} round-trips", case.label);
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

    /// The dialect swap rewrites the ROOKNI `****k → r` and KNIROO `f → n` (both cases)
    /// over the placement field and leaves the structural fields — including the bare
    /// `k`/`K` king — untouched.
    #[test]
    fn dialect_swap_maps_capture_swap_pieces() {
        let mcr = "****kfbqkbf****k/pppppppp/8/8/8/8/PPPPPPPP/****KFBQKBF****K w KQkq - 0 1";
        assert_eq!(
            to_fsf_dialect(mcr),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
        // Inline `****K` / `f` tokens and the bare `k`/`K` king are handled correctly.
        assert_eq!(
            to_fsf_dialect("4k3/8/8/8/4f3/8/4****K3/4K3 b - - 0 1"),
            "4k3/8/8/8/4n3/8/4R3/4K3 b - - 0 1"
        );
    }
}
