//! Ordamirror differential perft + timing against Fairy-Stockfish (issue #220).
//!
//! Ordamirror runs on mcr's **generic** engine (`mcr::geometry::Ordamirror`, a
//! `GenericPosition<Chess8x8, OrdamirrorRules>`), like the other fairy variants,
//! so it has its own corpus and comparison loop here. The FSF side selects
//! `UCI_Variant ordamirror`, sets the FEN, runs `go perft`, asserts the node
//! counts match, and reports mcr-vs-FSF throughput.
//!
//! ## Ordamirror is an INI variant
//!
//! Like `orda`, FSF defines `ordamirror` in its `variants.ini` data file, not in
//! the binary. The harness therefore loads the INI
//! (`setoption name VariantPath value <variants.ini>`) before checking
//! [`Engine::has_variant`](crate::uci::Engine::has_variant); the INI path is
//! resolved from `$MCR_FSF_VARIANTS_INI`, then a `variants.ini` sitting beside the
//! FSF binary, then the harness build dir. If none is found (or the loaded INI
//! still lacks `ordamirror`), the whole block is skipped cleanly.
//!
//! ## FEN dialect
//!
//! mcr and FSF render the same Ordamirror position with **different horde-piece
//! letters**. FSF's `ordamirror` uses `l h a f` (Lancer = kniroo, Kheshig =
//! centaur, Archer = knibis, Falcon = the `mQcN` custom piece); mcr reuses
//! `l`/`h`/`a` for its Lance / Hoplite / Hawk, so the horde pieces take distinct
//! letters: Lancer `f`, Kheshig `w`, Archer `y`, and the Falcon takes the overflow
//! token `*f` / `*F` (its base letter `f` is the FSF mnemonic; the `*` prefix keeps
//! it distinct from the bare Lancer `f`). [`to_fsf_dialect`] maps mcr's letters
//! back to FSF's over the whole placement field — translating the two-character
//! `*f`/`*F` Falcon token to FSF's single `f`/`F` **before** rewriting the bare
//! Lancer `f`→`l`. The King `k`/`K` is shared. The comparison asserts only node
//! counts, so the move-string dialect never matters.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked, and the INI is a plain data file.

use std::path::{Path, PathBuf};
use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, Ordamirror};

use crate::uci::Engine;

/// One Ordamirror corpus position. The FEN is mcr's dialect; the FSF side
/// translates it via [`to_fsf_dialect`].
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Ordamirror comparison corpus (all FSF-confirmed): the startpos, an opening
/// with an advanced centre pawn, a developed middlegame, a two-Falcon tactic
/// (queen-move / knight-capture), a both-Falcons middlegame, a both-sides `lhaf`
/// promotion, and a king-flag race exercising the campmate terminal rule.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "fwy*fkywf/8/pppppppp/8/8/PPPPPPPP/8/FWY*FKYWF w - - 0 1",
        depth: 5,
    },
    Case {
        label: "opening",
        fen: "fwy*fkywf/8/pppppppp/8/4P3/8/PPP1PPPP/FWY*FKYWF b - - 0 1",
        depth: 4,
    },
    Case {
        label: "developed",
        fen: "fwy*fk1wf/8/p1pppppp/8/2y1P3/2P5/PP1P1PPP/FWY*FK1WF w - - 0 1",
        depth: 4,
    },
    Case {
        label: "falcon-tactic",
        fen: "4k3/8/3p1p2/3p*f3/3*F4/2P1P3/8/4K3 w - - 0 1",
        depth: 4,
    },
    Case {
        label: "falcon-mid",
        fen: "fwy1k1wf/8/p1ppp1pp/5p2/2*f5/2*F5/PP1PPPPP/FWY1K1WF w - - 0 1",
        depth: 4,
    },
    Case {
        label: "promotion",
        fen: "7k/P7/8/8/8/8/7p/K7 w - - 0 1",
        depth: 5,
    },
    Case {
        label: "flag-race",
        fen: "8/4K3/8/8/8/8/4k3/8 w - - 0 1",
        depth: 5,
    },
];

/// Translates an mcr-dialect Ordamirror FEN to FSF's dialect. The horde letters
/// map Lancer `f→l`, Kheshig `w→h`, Archer `y→a` (both cases); the Falcon's
/// two-character overflow token `*f`/`*F` maps to FSF's single `f`/`F`. The
/// Falcon token is rewritten **first** (a `*` followed by `f`/`F`) so the bare
/// Lancer `f` is then free to become `l`. The King `k`/`K` is shared.
pub(crate) fn to_fsf_dialect(fen: &str) -> String {
    // Only the placement field (before the first space) holds piece letters; the
    // side-to-move / castling / en-passant fields must be left intact.
    let mut parts = fen.splitn(2, ' ');
    let raw = parts.next().unwrap_or("");

    let mut placement = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '*' {
            // An overflow token: `*f`/`*F` is the Falcon; emit FSF's bare `f`/`F`.
            match chars.next() {
                Some('f') => placement.push('f'),
                Some('F') => placement.push('F'),
                Some(other) => {
                    placement.push('*');
                    placement.push(other);
                }
                None => placement.push('*'),
            }
            continue;
        }
        placement.push(match c {
            'f' => 'l',
            'F' => 'L',
            'w' => 'h',
            'W' => 'H',
            'y' => 'a',
            'Y' => 'A',
            other => other,
        });
    }

    match parts.next() {
        Some(rest) => format!("{placement} {rest}"),
        None => placement,
    }
}

/// Resolve the FSF `variants.ini` path: `$MCR_FSF_VARIANTS_INI`, then a sibling
/// `variants.ini` beside the FSF binary, then the harness build dir's checkout.
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

/// A measured Ordamirror comparison row.
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

/// Run the Ordamirror corpus through mcr and FSF. Returns the number of
/// mismatches (0 = all matched). Loads FSF's `variants.ini` first (Ordamirror is
/// an INI variant); if the INI cannot be found or still lacks `ordamirror`, the
/// block is skipped (returns 0) rather than reporting spurious mismatches.
pub fn run(engine: &mut Engine, fsf_bin: &str, full: bool) -> usize {
    println!();
    println!("Ordamirror — generic engine vs FSF UCI_Variant ordamirror (issue #220):");

    if !engine.has_variant("ordamirror") {
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
                     variants.ini to enable the Ordamirror comparison)"
                );
                return 0;
            }
        }
    }
    if !engine.has_variant("ordamirror") {
        println!(
            "  (skipped: this FSF binary does not advertise UCI_Variant ordamirror even after \
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
                eprintln!("skip ordamirror/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "ordamirror OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Ordamirror positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Ordamirror parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH ordamirror/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Ordamirror position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Ordamirror::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    let fsf_fen = to_fsf_dialect(case.fen);
    engine.set_variant("ordamirror", false)?;
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

    /// The corpus FENs all parse on the generic Ordamirror engine, round-trip
    /// through mcr's FEN I/O, and the pinned shallow counts match the FSF-confirmed
    /// numbers in `tests/perft_ordamirror.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 3u32, 22487u64),
            ("opening", 3, 25692),
            ("developed", 3, 39329),
            ("falcon-tactic", 3, 6959),
            ("falcon-mid", 3, 60848),
            ("promotion", 3, 427),
            ("flag-race", 3, 200),
        ];
        for case in CASES {
            let pos = Ordamirror::from_fen(case.fen).expect("corpus FEN parses");
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

    /// The dialect swap maps mcr's horde letters to FSF's — including the
    /// two-character Falcon token `*f`/`*F` to FSF's bare `f`/`F` — and leaves the
    /// structural fields untouched.
    #[test]
    fn dialect_swap_maps_horde_letters() {
        let mcr = "fwy*fkywf/8/pppppppp/8/8/PPPPPPPP/8/FWY*FKYWF w - - 0 1";
        let fsf = to_fsf_dialect(mcr);
        assert_eq!(fsf, "lhafkahl/8/pppppppp/8/8/PPPPPPPP/8/LHAFKAHL w - - 0 1");
        // The Falcon token alone, both cases.
        assert_eq!(to_fsf_dialect("3*F4 w"), "3F4 w");
        assert_eq!(to_fsf_dialect("3*f4 b"), "3f4 b");
        // The side-to-move `w` is not mangled into the Kheshig's `h`.
        assert!(fsf.ends_with(" w - - 0 1"));
    }
}
