//! Fog of War / Dark Chess (8x8) differential perft + timing against
//! Fairy-Stockfish (issue #277).
//!
//! Fog of War runs on mcr's **generic** 8x8 engine
//! (`mcr::geometry::FogOfWar`, a `GenericPosition<Chess8x8, FogOfWarRules>`),
//! like Duck. The FSF side selects `UCI_Variant fogofwar`, sets the FEN, runs
//! `go perft`, asserts the node counts match, and reports throughput.
//!
//! ## Fog of War is an INI variant — bundled here
//!
//! FSF has **no built-in** `fogofwar`, and (unlike Chak / Orda / Empire) it is
//! not in upstream's shipped `variants.ini` either. So this harness **bundles**
//! its own definition ([`FOGOFWAR_INI`], inheriting the built-in `chess`), writes
//! it to a temp file, and loads it with `setoption name VariantPath`. The snippet
//! is plain original data authored here — it links no FSF code, so the GPL fence
//! is unchanged (FSF is still only driven as a subprocess; see `uci.rs`).
//!
//! The definition makes the king a non-royal, capturable `commoner`:
//! `extinctionValue = loss` on it makes its capture terminal, and a commoner is
//! not royal, so FSF applies no check / pin / king-danger filter — exactly Fog of
//! War's "no check, win by capturing the king" rule.
//!
//! ## FEN dialect
//!
//! mcr uses the **same dialect** FSF does — plain chess — so a Fog of War FEN is
//! byte-identical between the two engines; there is no rewrite step.

use std::io::Write;
use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, FogOfWar};

use crate::uci::Engine;

/// The bundled `variants.ini` definition of Fog of War (inherits built-in
/// `chess`). Authored here; contains no Fairy-Stockfish code.
const FOGOFWAR_INI: &str = "\
# Fog of War / Dark Chess - standard moves, non-royal capturable king.
[fogofwar:chess]
king = -
commoner = k
castlingKingPiece = k
extinctionValue = loss
extinctionPieceTypes = k
";

/// One Fog of War corpus position (mcr == FSF dialect).
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Fog of War comparison corpus (all FSF-confirmed): the startpos (counts
/// diverge from chess from depth 4 on), the "Kiwipete" middlegame (captures,
/// castling, pins Fog of War ignores), a position with a king attacked (the
/// "checked" side keeps every pseudo-legal move), and a castling-rich position
/// (castling ignores attacked squares).
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        depth: 5,
    },
    Case {
        label: "kiwipete",
        fen: "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "in_check",
        fen: "rnbqkbnr/ppppp1pp/8/5p1Q/4P3/8/PPPP1PPP/RNB1KBNR b KQkq - 1 2",
        depth: 4,
    },
    Case {
        label: "castling",
        fen: "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        depth: 4,
    },
];

/// Write [`FOGOFWAR_INI`] to a temp file and return its path, so it can be loaded
/// with `setoption name VariantPath`.
fn write_bundled_ini() -> Result<std::path::PathBuf, String> {
    let path = std::env::temp_dir().join("mcr_fogofwar_variants.ini");
    let mut f = std::fs::File::create(&path).map_err(|e| format!("create temp ini: {e}"))?;
    f.write_all(FOGOFWAR_INI.as_bytes())
        .map_err(|e| format!("write temp ini: {e}"))?;
    Ok(path)
}

/// A measured Fog of War comparison row.
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

/// Run the Fog of War corpus through mcr and FSF. Returns the number of
/// mismatches (0 = all matched). Bundles and loads its own `variants.ini`
/// definition (FSF has no built-in `fogofwar`); if FSF still does not advertise
/// the variant, the block is skipped cleanly (returns 0).
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Fog of War — generic engine vs FSF UCI_Variant fogofwar (issue #277):");

    if !engine.has_variant("fogofwar") {
        match write_bundled_ini() {
            Ok(ini) => {
                if let Err(e) = engine.load_variant_path(&ini.to_string_lossy()) {
                    println!("  (skipped: failed to load bundled fogofwar variants.ini: {e})");
                    return 0;
                }
            }
            Err(e) => {
                println!("  (skipped: could not write bundled fogofwar variants.ini: {e})");
                return 0;
            }
        }
    }
    if !engine.has_variant("fogofwar") {
        println!(
            "  (skipped: this FSF binary does not advertise UCI_Variant fogofwar even after \
                  loading the bundled variants.ini)"
        );
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
                eprintln!("skip fogofwar/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "fogofwar OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Fog of War positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Fog of War parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH fogofwar/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Fog of War position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = FogOfWar::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    engine.set_variant("fogofwar", false)?;
    engine.set_position(case.fen)?;
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

    /// The corpus FENs all parse on the generic Fog of War engine, round-trip
    /// through mcr's FEN I/O, and the pinned shallow counts match the
    /// FSF-confirmed numbers in `tests/perft_fogofwar.rs` (this runs without FSF
    /// present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 3u32, 8902u64),
            ("kiwipete", 3, 98903),
            ("in_check", 3, 17817),
            ("castling", 3, 15950),
        ];
        for case in CASES {
            let pos = FogOfWar::from_fen(case.fen).expect("corpus FEN parses");
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

    /// The bundled INI snippet declares the `fogofwar` variant inheriting `chess`.
    #[test]
    fn bundled_ini_declares_fogofwar() {
        assert!(FOGOFWAR_INI.contains("[fogofwar:chess]"));
        assert!(FOGOFWAR_INI.contains("commoner = k"));
        assert!(FOGOFWAR_INI.contains("extinctionValue = loss"));
    }
}
