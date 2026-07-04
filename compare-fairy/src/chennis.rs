//! Chennis (7x7 tennis-themed flipping variant) differential perft + timing
//! against Fairy-Stockfish (issue #273).
//!
//! Chennis runs on mcr's **generic** engine (`mcr::geometry::Chennis`, a
//! `GenericPosition<Chennis7x7, ChennisRules>`), reusing the Kyoto-Shogi per-move
//! flip and the Shogi-family persistent hand + dual-form drops, with a king
//! mobility region. The FSF side selects `UCI_Variant chennis`, sets the FEN, runs
//! `go perft`, asserts the node counts match, and reports mcr-vs-FSF throughput.
//! The corpus exercises the **per-move flip** (base ↔ promoted forms), **dual-form
//! drops** (`dropPromoted`), the **cannon** over-screen captures, and the
//! **king mobility region**.
//!
//! ## Chennis is an INI variant
//!
//! Like Orda / Chak / Mansindam, FSF defines `chennis` in its `variants.ini` data
//! file, not in the binary. The harness loads the INI
//! (`setoption name VariantPath value <variants.ini>`) before checking
//! [`Engine::has_variant`](crate::uci::Engine::has_variant); the INI path is
//! resolved from `$MCR_FSF_VARIANTS_INI`, then a `variants.ini` beside the FSF
//! binary, then the harness build dir. If none is found (or the loaded INI still
//! lacks `chennis`), the block is skipped cleanly.
//!
//! ## FEN dialect
//!
//! mcr and FSF render the same Chennis position with **different piece tokens**.
//! [`to_fsf_dialect`] walks the placement field (including the `[..]` hand bracket)
//! token by token, rewriting each mcr token to FSF's spelling:
//!
//! | piece                    | mcr token | FSF token |
//! |--------------------------|-----------|-----------|
//! | Pawn (base)              | `**p` | `p`  |
//! | Ferz (= Met, base)       | `m`   | `f`  |
//! | Soldier (base)           | `z`   | `s`  |
//! | Commoner (base)          | `*u`  | `m`  |
//! | King                     | `k`   | `k`  |
//! | Rook (= promoted Pawn)   | `r`   | `+p` |
//! | Cannon (= promoted Ferz) | `c`   | `+f` |
//! | Bishop (= promoted Soldier) | `b` | `+s` |
//! | Knight (= promoted Commoner) | `n` | `+m` |
//!
//! The promoted forms are distinct roles in mcr (`r c b n`) but FSF's `+`-prefixed
//! flips of the base pieces; the hand only ever holds base forms (a captured piece
//! banks demoted), so `r c b n` never appear in the bracket. Case carries colour
//! throughout (uppercase = White). The comparison asserts only node counts, so the
//! move-string dialect never matters.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no GPL
//! code is linked, and the INI is a plain data file.

use std::path::{Path, PathBuf};
use std::time::Instant;

use mcr::geometry::{perft as gperft, Chennis, Chennis7x7};

use crate::uci::Engine;

/// One Chennis corpus position. The FEN is mcr's dialect; the FSF side translates
/// it via [`to_fsf_dialect`].
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Chennis comparison corpus (all FSF-confirmed): the startpos, a flipping
/// middlegame with a promoted Rook / Bishop / Cannon on the board, a full
/// dual-form drop swarm (one of every base role in each hand), and the minimal
/// single-pawn-in-hand drop position.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "1mk*u3/1**p1z3/7/7/7/3Z1**P1/3*UKM1[] w - - 0 1",
        depth: 5,
    },
    Case {
        label: "flip-midgame",
        fen: "1mk*u3/3z3/1r5/7/3B3/5**PC/3*UK2[] b - - 4 2",
        depth: 4,
    },
    Case {
        label: "drops-in-hand",
        fen: "3k3/7/7/7/7/7/3K3[**PMZ*U**pmz*u] w - - 0 1",
        depth: 2,
    },
    Case {
        label: "one-pawn-hand",
        fen: "3k3/7/7/7/7/7/3K3[**P**p] w - - 0 1",
        depth: 3,
    },
];

/// Rewrites an mcr-dialect Chennis FEN's placement field to FSF's spelling. See the
/// [module docs](self) for the token table.
pub(crate) fn to_fsf_dialect(fen: &str) -> String {
    let mut parts = fen.splitn(2, ' ');
    let placement = parts.next().unwrap_or("");
    let mut out = String::with_capacity(placement.len());
    let mut chars = placement.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '*' {
            if chars.peek() == Some(&'*') {
                // A second-bank overflow token `**X`: the Pawn `**p → p` (a base
                // piece in both dialects).
                chars.next();
                let base = chars.next().unwrap_or('*');
                let upper = base.is_ascii_uppercase();
                match base.to_ascii_lowercase() {
                    'p' => out.push(if upper { 'P' } else { 'p' }),
                    other => out.push(if upper {
                        other.to_ascii_uppercase()
                    } else {
                        other
                    }),
                }
            } else {
                // A single-`*` overflow token `*X`: the Commoner `*u → m`.
                let base = chars.next().unwrap_or('*');
                let upper = base.is_ascii_uppercase();
                match base.to_ascii_lowercase() {
                    'u' => out.push(if upper { 'M' } else { 'm' }),
                    other => out.push(if upper {
                        other.to_ascii_uppercase()
                    } else {
                        other
                    }),
                }
            }
        } else {
            // A bare letter. The base pieces swap to FSF's letters (Ferz `m → f`,
            // Soldier `z → s`); the promoted roles become FSF's `+`-flips of the
            // base piece (Rook `r → +p`, Cannon `c → +f`, Bishop `b → +s`, Knight
            // `n → +m`); `k`, digits, `/`, `[`, `]` pass through.
            let upper = c.is_ascii_uppercase();
            match c.to_ascii_lowercase() {
                'm' => out.push(if upper { 'F' } else { 'f' }),
                'z' => out.push(if upper { 'S' } else { 's' }),
                'r' => push_promoted(&mut out, 'p', upper),
                'c' => push_promoted(&mut out, 'f', upper),
                'b' => push_promoted(&mut out, 's', upper),
                'n' => push_promoted(&mut out, 'm', upper),
                _ => out.push(c),
            }
        }
    }
    match parts.next() {
        Some(rest) => format!("{out} {rest}"),
        None => out,
    }
}

/// Pushes FSF's `+`-promoted token (`+<base>`), the base's case carrying the
/// colour: `upper` (a White piece) yields `+P`-style uppercase, else lowercase.
fn push_promoted(out: &mut String, base: char, upper: bool) {
    out.push('+');
    out.push(if upper {
        base.to_ascii_uppercase()
    } else {
        base
    });
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

/// A measured Chennis comparison row.
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

/// Run the Chennis corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all matched). Loads FSF's `variants.ini` first (Chennis is an INI
/// variant); if the INI cannot be found or still lacks `chennis`, the block is
/// skipped (returns 0) rather than reporting spurious mismatches.
pub fn run(engine: &mut Engine, fsf_bin: &str, full: bool) -> usize {
    println!();
    println!("Chennis — generic engine vs FSF UCI_Variant chennis (issue #273):");

    if !engine.has_variant("chennis") {
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
                     variants.ini to enable the Chennis comparison)"
                );
                return 0;
            }
        }
    }
    if !engine.has_variant("chennis") {
        println!(
            "  (skipped: this FSF binary does not advertise UCI_Variant chennis even after \
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
                eprintln!("skip chennis/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "chennis OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Chennis positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Chennis parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH chennis/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Chennis position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Chennis::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chennis7x7, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    let fsf_fen = to_fsf_dialect(case.fen);
    engine.set_variant("chennis", false)?;
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

    /// The corpus FENs all parse on the generic Chennis engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_chennis.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 100u64),
            ("flip-midgame", 491),
            ("drops-in-hand", 129875),
            ("one-pawn-hand", 8383),
        ];
        for case in CASES {
            let pos = Chennis::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Chennis7x7, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }

    /// The dialect swap maps the base pieces (Ferz `m → f`, Soldier `z → s`,
    /// Commoner `*u → m`, Pawn `**p → p`) and the promoted roles to FSF's `+`-flips
    /// (Rook `r → +p`, Cannon `c → +f`, Bishop `b → +s`, Knight `n → +m`), case
    /// preserved; `k`, digits and separators pass through.
    #[test]
    fn dialect_swap_maps_chennis_tokens() {
        assert_eq!(
            to_fsf_dialect("1mk*u3/1**p1z3/7/7/7/3Z1**P1/3*UKM1[] w - - 0 1"),
            "1fkm3/1p1s3/7/7/7/3S1P1/3MKF1[] w - - 0 1"
        );
        assert_eq!(
            to_fsf_dialect("1mk*u3/3z3/1r5/7/3B3/5**PC/3*UK2[] b - - 4 2"),
            "1fkm3/3s3/1+p5/7/3+S3/5P+F/3MK2[] b - - 4 2"
        );
        assert_eq!(
            to_fsf_dialect("3k3/7/7/7/7/7/3K3[**PMZ*U**pmz*u] w - - 0 1"),
            "3k3/7/7/7/7/7/3K3[PFSMpfsm] w - - 0 1"
        );
        // The trailing fields are preserved verbatim.
        assert!(to_fsf_dialect("7/7 b - - 0 1").ends_with("b - - 0 1"));
    }
}
