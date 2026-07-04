//! Shatar (Mongolian chess) differential perft + timing against Fairy-Stockfish
//! (issue #229).
//!
//! Shatar runs on mcr's **generic** engine (`mcr::geometry::Shatar`, a
//! `GenericPosition<Chess8x8, ShatarRules>`), like the other fairy variants, so
//! it has its own corpus and comparison loop here. The FSF side selects
//! `UCI_Variant shatar`, sets the FEN, runs `go perft`, asserts the node counts
//! match, and reports mcr-vs-FSF throughput.
//!
//! ## FEN dialect
//!
//! mcr and FSF render the same Shatar position with **different Bers letters**.
//! FSF's `shatar` spells the Bers `j`; mcr reuses `j` for its Xiangqi Horse role,
//! so the Bers takes the [`mcr::geometry::WideRole::General`] letter `d` (the
//! Rook + Ferz role it shares with Spartan / Shinobi). [`to_fsf_dialect`] maps
//! mcr's `d` / `D` back to FSF's `j` / `J` over the whole FEN. Only the placement
//! field carries the Bers letter (castling is `-`, no Shatar letters; the
//! en-passant field is always `-`), and `d` / `D` name nothing else in a Shatar
//! position, so the swap is unambiguous. The comparison asserts only node counts,
//! so the move-string dialect never matters.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Shatar};

use crate::uci::Engine;

/// One Shatar corpus position. The FEN is mcr's dialect; the FSF side translates
/// it via [`to_fsf_dialect`].
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Shatar comparison corpus: the FSF-confirmed startpos (centre pawns
/// pre-advanced, no double step), a Bers-active middlegame, an open two-Bers
/// middlegame, and a Robado-exercising position (a line that captures Black's
/// lone pawn reduces it to a bare king, truncating that subtree to zero — the
/// distinctive Shatar perft effect). Depths are kept modest by default; `full`
/// deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnbdkbnr/ppp1pppp/8/3p4/3P4/8/PPP1PPPP/RNBDKBNR w - - 0 1",
        depth: 5,
    },
    Case {
        label: "mid-bers",
        fen: "r3k2r/p1ppdpb1/bn2pnp1/3PN3/1p2P3/2N3p1/PPPBBPPP/R3K2R w - - 0 1",
        depth: 4,
    },
    Case {
        label: "mid-open",
        fen: "4k3/8/8/3d4/3D4/8/4P3/4K3 w - - 0 1",
        depth: 4,
    },
    Case {
        label: "robado",
        fen: "4k3/4p3/8/8/8/8/3D4/4K3 w - - 0 1",
        depth: 5,
    },
];

/// Translates an mcr-dialect Shatar FEN to FSF's dialect by mapping the Bers
/// letter `d → j` (both cases). Every other letter — the standard army and the
/// structural fields — is FSF-identical, so the swap is safe over the whole FEN.
pub(crate) fn to_fsf_dialect(fen: &str) -> String {
    fen.chars()
        .map(|c| match c {
            'd' => 'j',
            'D' => 'J',
            other => other,
        })
        .collect()
}

/// A measured Shatar comparison row.
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

/// Run the Shatar corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all positions matched). Prints a table and a one-line summary. If the FSF
/// binary does not advertise the `shatar` variant, the whole block is skipped
/// (returns 0) rather than reporting spurious mismatches.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Shatar (Mongolian) — generic engine vs FSF UCI_Variant shatar (issue #229):");
    if !engine.has_variant("shatar") {
        println!("  (skipped: this FSF binary does not advertise UCI_Variant shatar)");
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
                eprintln!("skip shatar/{}: {e}", case.label);
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
            "shatar OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Shatar positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Shatar parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH shatar/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Shatar position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    // mcr side: the generic Shatar position (mcr dialect).
    let pos = Shatar::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    // FSF side: translate the Bers letter to FSF's dialect.
    let fsf_fen = to_fsf_dialect(case.fen);
    engine.set_variant("shatar", false)?;
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

    /// The corpus FENs all parse on the generic Shatar engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_shatar.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 3u32, 8426u64),
            ("mid-bers", 3, 65231),
            ("mid-open", 3, 6676),
            ("robado", 3, 1667),
        ];
        for case in CASES {
            let pos = Shatar::from_fen(case.fen).expect("corpus FEN parses");
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

    /// The dialect swap maps mcr's Bers letter `d` to FSF's `j` and leaves the
    /// standard army and structural fields untouched.
    #[test]
    fn dialect_swap_maps_bers_letter() {
        let mcr = "rnbdkbnr/ppp1pppp/8/3p4/3P4/8/PPP1PPPP/RNBDKBNR w - - 0 1";
        let fsf = to_fsf_dialect(mcr);
        assert_eq!(
            fsf,
            "rnbjkbnr/ppp1pppp/8/3p4/3P4/8/PPP1PPPP/RNBJKBNR w - - 0 1"
        );
        // The structural fields are unchanged.
        assert!(fsf.contains("w - -"));
    }
}
