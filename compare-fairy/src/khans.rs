//! Khan's Chess differential perft + timing against Fairy-Stockfish (issue #272).
//!
//! Khan's Chess runs on mcr's **generic** engine (`mcr::geometry::Khans`, a
//! `GenericPosition<Chess8x8, KhansRules>`), like the other fairy variants, so it
//! has its own corpus and comparison loop here. The FSF side selects
//! `UCI_Variant khans`, sets the FEN, runs `go perft`, asserts the node counts
//! match, and reports mcr-vs-FSF throughput.
//!
//! ## Khan's Chess is an INI variant
//!
//! Like `orda`, FSF defines `khans` in its `variants.ini` data file, not in the
//! binary. The harness therefore loads the INI
//! (`setoption name VariantPath value <variants.ini>`) before checking
//! [`Engine::has_variant`](crate::uci::Engine::has_variant); the INI path is
//! resolved from `$MCR_FSF_VARIANTS_INI`, then a `variants.ini` sitting beside the
//! FSF binary, then the harness build dir. If none is found (or the loaded INI
//! still lacks `khans`), the whole block is skipped cleanly.
//!
//! ## FEN dialect
//!
//! mcr and FSF render the same Khan's Chess position with **different Khan-piece
//! letters**. FSF's `khans` uses `l h a t k s` (Lancer = kniroo, Kheshig = centaur,
//! Archer = knibis, Khan = `mNcK`, King, soldier = `mfhNcfW`); mcr reuses `l`/`h`/`a`
//! for its Lance / Hoplite / Hawk, so the shared Orda pieces take the letters Lancer
//! `f`, Kheshig `w`, Archer `y`, and the two new Khan pieces take overflow-3 tokens
//! `=t` (Khan) / `=s` (soldier). [`to_fsf_dialect`] maps mcr's letters back to FSF's
//! over the placement field: it drops the `=` prefix of each overflow-3 token
//! (whose recycled base is already FSF's own letter `t` / `s`) and rewrites the
//! shared `f`/`w`/`y`. The comparison asserts only node counts, so the move-string
//! dialect never matters.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked, and the INI is a plain data file.

use std::path::{Path, PathBuf};
use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Khans};

use crate::uci::Engine;

/// One Khan's Chess corpus position. The FEN is mcr's dialect; the FSF side
/// translates it via [`to_fsf_dialect`].
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Khan's Chess comparison corpus (all FSF-confirmed): the startpos, an
/// all-pieces tactic, a captures position (Khan king-capture + soldier
/// forward-capture + forced promotion), a developed middlegame, a soldier-promotion
/// race, a White-pawn-promotion position, and a king-flag race exercising the
/// campmate terminal rule.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "fwy=tkywf/=s=s=s=s=s=s=s=s/8/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1",
        depth: 5,
    },
    Case {
        label: "tactic",
        fen: "4k3/8/3=t4/2f1=s3/2P1P3/3w1y2/8/4K3 b - - 0 1",
        depth: 4,
    },
    Case {
        label: "captures",
        fen: "4k3/8/8/3=t4/2PPP3/3=s4/3P4/4K3 b - - 0 1",
        depth: 4,
    },
    Case {
        label: "developed",
        fen: "f1y=tkywf/1=s=s=s=s=s1=s/2=s5/8/2P1P3/5N2/PP1P1PPP/RNBQKB1R b KQ - 0 1",
        depth: 4,
    },
    Case {
        label: "soldier-promo",
        fen: "4k3/8/8/8/8/8/2=s1=s3/4K3 b - - 0 1",
        depth: 5,
    },
    Case {
        label: "white-promo",
        fen: "4k3/P7/8/2=t5/8/3=s4/8/4K3 w - - 0 1",
        depth: 4,
    },
    Case {
        label: "flag-race",
        fen: "8/4K3/8/8/8/8/4k3/8 w - - 0 1",
        depth: 5,
    },
];

/// Translates an mcr-dialect Khan's Chess FEN to FSF's dialect over the placement
/// field. Each overflow-3 token `=t` / `=s` (Khan / soldier) drops its `=` prefix —
/// the recycled base letter is already FSF's own `t` / `s` — and the shared Orda
/// letters map Lancer `f→l`, Kheshig `w→h`, Archer `y→a` (both cases). The King
/// `k`/`K` and the standard White army are unchanged. Only the placement field
/// (before the first space) holds piece letters; the side-to-move / castling /
/// en-passant fields are left intact (otherwise a white-to-move `w` would be
/// mangled into the Kheshig's `h`).
pub(crate) fn to_fsf_dialect(fen: &str) -> String {
    let mut parts = fen.splitn(2, ' ');
    let placement = parts.next().unwrap_or("");
    let mut out = String::with_capacity(placement.len());
    let mut chars = placement.chars();
    while let Some(c) = chars.next() {
        if c == '=' {
            // An overflow-3 token: the next char is the recycled base letter, which
            // is already FSF's own letter (`t` for the Khan, `s` for the soldier),
            // so emit it without the `=` prefix.
            if let Some(base) = chars.next() {
                out.push(base);
            }
        } else {
            out.push(match c {
                'f' => 'l',
                'F' => 'L',
                'w' => 'h',
                'W' => 'H',
                'y' => 'a',
                'Y' => 'A',
                other => other,
            });
        }
    }
    match parts.next() {
        Some(rest) => format!("{out} {rest}"),
        None => out,
    }
}

/// Resolve the FSF `variants.ini` path: `$MCR_FSF_VARIANTS_INI`, then a sibling
/// `variants.ini` beside the FSF binary (the upstream layout `…/src/stockfish` +
/// `…/src/variants.ini`), then the harness build dir's checkout.
fn resolve_variants_ini(fsf_bin: &str) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MCR_FSF_VARIANTS_INI") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(dir) = Path::new(fsf_bin).parent() {
        let sib = dir.join("variants.ini");
        if sib.is_file() {
            return Some(sib);
        }
    }
    let build = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("build")
        .join("Fairy-Stockfish")
        .join("src")
        .join("variants.ini");
    if build.is_file() {
        return Some(build);
    }
    None
}

/// A measured Khan's Chess comparison row.
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

/// Run the Khan's Chess corpus through mcr and FSF. Returns the number of
/// mismatches (0 = all matched). Loads FSF's `variants.ini` first (Khan's Chess is
/// an INI variant); if the INI cannot be found or still lacks `khans`, the block is
/// skipped (returns 0) rather than reporting spurious mismatches.
pub fn run(engine: &mut Engine, fsf_bin: &str, full: bool) -> usize {
    println!();
    println!("Khan's Chess — generic engine vs FSF UCI_Variant khans (issue #272):");

    if !engine.has_variant("khans") {
        match resolve_variants_ini(fsf_bin) {
            Some(ini) => {
                if let Err(e) = engine.load_variant_path(&ini.to_string_lossy()) {
                    println!("  (skipped: failed to load variants.ini: {e})");
                    return 0;
                }
            }
            None => {
                println!(
                    "  (skipped: no variants.ini found; set $MCR_FSF_VARIANTS_INI to FSF's \
                     variants.ini to enable the Khan's Chess comparison)"
                );
                return 0;
            }
        }
    }
    if !engine.has_variant("khans") {
        println!(
            "  (skipped: this FSF binary does not advertise UCI_Variant khans even after \
                  loading variants.ini)"
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
                eprintln!("skip khans/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "khans OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Khan's Chess positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Khan's Chess parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH khans/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Khan's Chess position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Khans::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    let fsf_fen = to_fsf_dialect(case.fen);
    engine.set_variant("khans", false)?;
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

    /// The corpus FENs all parse on the generic Khan's Chess engine, round-trip
    /// through mcr's FEN I/O, and the pinned shallow counts match the FSF-confirmed
    /// numbers in `tests/perft_khans.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 3u32, 16912u64),
            ("tactic", 3, 2329),
            ("captures", 3, 2470),
            ("developed", 3, 35204),
            ("soldier-promo", 3, 364),
            ("white-promo", 3, 966),
            ("flag-race", 3, 200),
        ];
        for case in CASES {
            let pos = Khans::from_fen(case.fen).expect("corpus FEN parses");
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

    /// The dialect swap maps mcr's Khan letters to FSF's and leaves the structural
    /// fields untouched.
    #[test]
    fn dialect_swap_maps_khan_letters() {
        let mcr = "fwy=tkywf/=s=s=s=s=s=s=s=s/8/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1";
        let fsf = to_fsf_dialect(mcr);
        assert_eq!(
            fsf,
            "lhatkahl/ssssssss/8/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1"
        );
        // A White Khan (mcr `=T`) maps to FSF's `T`; a White soldier `=S` to `S`.
        assert_eq!(to_fsf_dialect("4=T3"), "4T3");
        assert_eq!(to_fsf_dialect("4=S3"), "4S3");
        // The standard White army and the clock fields are unchanged.
        assert!(fsf.contains("RNBQKBNR") && fsf.contains("PPPPPPPP") && fsf.contains("w KQ -"));
    }
}
