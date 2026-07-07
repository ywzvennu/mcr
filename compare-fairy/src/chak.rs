//! Chak differential perft + timing against Fairy-Stockfish (issue #228).
//!
//! Chak runs on mcr's **generic** engine (`mcr::geometry::Chak`, a
//! `GenericPosition<Shogi9x9, ChakRules>`), like the other fairy variants, so it
//! has its own corpus and comparison loop here. The FSF side selects
//! `UCI_Variant chak`, sets the FEN, runs `go perft`, asserts the node counts
//! match, and reports mcr-vs-FSF throughput.
//!
//! ## Chak is an INI variant
//!
//! Like Orda / Synochess / Empire, FSF defines `chak` in its `variants.ini` data
//! file, not in the binary. The harness therefore loads the INI
//! (`setoption name VariantPath value <variants.ini>`) before checking
//! [`Engine::has_variant`](crate::uci::Engine::has_variant); the INI path is
//! resolved from `$MCR_FSF_VARIANTS_INI`, then a `variants.ini` sitting beside the
//! FSF binary, then the harness build dir. If none is found (or the loaded INI
//! still lacks `chak`), the whole block is skipped cleanly.
//!
//! ## FEN dialect
//!
//! mcr and FSF render the same Chak position with **different piece letters**. FSF
//! spells the eight kinds `r v s q k j o p` (Rook, Vulture, Serpent, Quetzal, King,
//! Jaguar, Temple, Soldier) plus the promotion-only `w` (Shaman) and `d` (Divine
//! Lord). mcr reuses `r`/`n`/`k` (Rook / Knight=Vulture / King) and `w`
//! (Kheshig=Jaguar), and spells the six new pieces with `*`-prefixed overflow
//! tokens (`*s *q *w *l *p *o`). [`to_fsf_dialect`] walks the placement field
//! character by character: a `*`-token maps to its FSF letter (`*s → s`, `*q → q`,
//! `*w → w`, `*l → d`, `*p → p`, `*o → o`) and a bare letter is swapped where it
//! differs (`n → v` Vulture, `w → j` Jaguar; `r`/`k` are shared). Walking
//! `*`-tokens and bare letters together is what disambiguates the collapsed Shaman
//! `*w → w` from the bare Jaguar `w → j`. The side-to-move / clock fields are left
//! intact. The comparison asserts only node counts, so the move-string dialect
//! never matters.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked, and the INI is a plain data file.

use std::path::{Path, PathBuf};
use std::time::Instant;

use mcr::geometry::{perft as gperft, Chak, Shogi9x9};

use crate::uci::Engine;

/// One Chak corpus position. The FEN is mcr's dialect; the FSF side translates it
/// via [`to_fsf_dialect`].
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Chak comparison corpus (all FSF-confirmed): the startpos, a King walked to
/// its own half (every King move promotes to a Divine Lord), a developed
/// middlegame, a Soldier in its promotion half (promotes to a Shaman), a
/// Quetzal-active position (open screens for the eight-direction cannon), an
/// (artificial) two-royal position with the strict pseudo-royal rule pinning a
/// Divine Lord, and a Divine Lord beside the enemy temple exercising the temple
/// win (subtree truncation).
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rn*s*qkw*snr/4*o4/*p1*p1*p1*p1*p/9/9/9/*P1*P1*P1*P1*P/4*O4/RN*SWK*Q*SNR w - - 0 1",
        depth: 4,
    },
    Case {
        label: "king-center",
        fen: "rn*s*qkw*snr/4*o4/*p1*p1*p1*p1*p/9/9/4K4/*P1*P1*P1*P1*P/4*O4/RN*S*Q1*Q*SNR w - - 0 1",
        depth: 4,
    },
    Case {
        label: "midgame",
        fen: "rn*s*qkw*snr/4*o4/*p3*p1*p1*p/2*p6/9/2*P6/*P3*P1*P1*P/4*O4/RN*SWK*Q*SNR w - - 0 1",
        depth: 4,
    },
    Case {
        label: "soldier-zone",
        fen: "rn*s*qkw*snr/4*o4/*p1*p1*p1*p1*p/9/4*P4/9/*P1*P1*P3*P/4*O4/RN*SWK*Q*SNR b - - 0 1",
        depth: 4,
    },
    Case {
        label: "quetzal",
        fen: "rn*s*qkw*snr/4*o4/*p1*p1*p1*p1*p/9/9/2*P3*P2/*P1*P1*P1*P1*P/4*O4/RN*SWK*Q*SNR b - - 0 1",
        depth: 4,
    },
    Case {
        label: "lord-pinned",
        fen: "rn*s*qk1*snr/4*o4/*p1*p1*p1*p1*p/3*L5/9/9/*P1*P1*P1*P1*P/4*O4/RN*S1K*Q*SNR w - - 0 1",
        depth: 4,
    },
    Case {
        label: "temple-win",
        fen: "rn*s*qk1*snr/4*o4/*p1*p1*L1*p1*p/9/9/9/*P1*P1*P1*P1*P/4*O4/RN*S1K*Q*SNR w - - 0 1",
        depth: 4,
    },
];

/// Translates an mcr-dialect Chak FEN to FSF's dialect over the **placement field
/// only**. Walking character by character disambiguates the collapsed Shaman
/// (`*w → w`) from the bare Jaguar (`w → j`): a `*`-prefixed overflow token maps to
/// its FSF letter, and a bare letter is swapped where mcr and FSF differ. The
/// side-to-move / clock fields are left intact.
pub(crate) fn to_fsf_dialect(fen: &str) -> String {
    let mut parts = fen.splitn(2, ' ');
    let placement = parts.next().unwrap_or("");
    let mut out = String::with_capacity(placement.len());
    let mut chars = placement.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '*' {
            // An overflow token `*X`: map the recycled base letter to FSF's letter.
            let base = chars.next().unwrap_or('*');
            let upper = base.is_ascii_uppercase();
            let fsf = match base.to_ascii_lowercase() {
                's' => 's',     // Serpent
                'q' => 'q',     // Quetzal
                'w' => 'w',     // Shaman
                'l' => 'd',     // Divine Lord
                'p' => 'p',     // Soldier
                'o' => 'o',     // Temple
                other => other, // (unreachable for a valid Chak FEN)
            };
            out.push(if upper { fsf.to_ascii_uppercase() } else { fsf });
        } else {
            // A bare letter: swap the reused Vulture (`n → v`) and Jaguar (`w → j`);
            // `r`/`k` are shared, digits and `/` pass through.
            let mapped = match c {
                'n' => 'v',
                'N' => 'V',
                'w' => 'j',
                'W' => 'J',
                other => other,
            };
            out.push(mapped);
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

/// A measured Chak comparison row.
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

/// Run the Chak corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all matched). Loads FSF's `variants.ini` first (Chak is an INI variant);
/// if the INI cannot be found or still lacks `chak`, the block is skipped
/// (returns 0) rather than reporting spurious mismatches.
pub fn run(engine: &mut Engine, fsf_bin: &str, full: bool) -> usize {
    println!();
    println!("Chak — generic engine vs FSF UCI_Variant chak (issue #228):");

    if !engine.has_variant("chak") {
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
                     variants.ini to enable the Chak comparison)"
                );
                return 0;
            }
        }
    }
    if !engine.has_variant("chak") {
        println!(
            "  (skipped: this FSF binary does not advertise UCI_Variant chak even after \
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
                eprintln!("skip chak/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "chak OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Chak positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Chak parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH chak/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Chak position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Chak::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Shogi9x9, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    let fsf_fen = to_fsf_dialect(case.fen);
    engine.set_variant("chak", false)?;
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

    /// The corpus FENs all parse on the generic Chak engine, round-trip through
    /// mcr's FEN I/O, and the pinned shallow counts match the FSF-confirmed numbers
    /// in `tests/perft_chak.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 3u32, 37526u64),
            ("king-center", 3, 46618),
            ("midgame", 3, 37772),
            ("soldier-zone", 3, 38719),
            ("quetzal", 3, 38903),
            ("lord-pinned", 3, 7091),
            ("temple-win", 3, 33816),
        ];
        for case in CASES {
            let pos = Chak::from_fen(case.fen).expect("corpus FEN parses");
            assert_eq!(pos.to_fen(), case.fen, "{} round-trips", case.label);
            let (_, depth, want) = pinned
                .iter()
                .find(|(l, _, _)| *l == case.label)
                .copied()
                .expect("a pinned count for the case");
            assert_eq!(
                gperft::<Shogi9x9, _, _>(&pos, depth),
                want,
                "{} perft",
                case.label
            );
        }
    }

    /// The dialect swap maps `*`-tokens to FSF letters and the reused bare letters
    /// (Vulture `n → v`, Jaguar `w → j`), disambiguating the collapsed Shaman
    /// (`*w → w`) from the bare Jaguar (`w → j`), and leaves structural fields intact.
    #[test]
    fn dialect_swap_maps_chak_letters() {
        let mcr =
            "rn*s*qkw*snr/4*o4/*p1*p1*p1*p1*p/9/9/9/*P1*P1*P1*P1*P/4*O4/RN*SWK*Q*SNR w - - 0 1";
        assert_eq!(
            to_fsf_dialect(mcr),
            "rvsqkjsvr/4o4/p1p1p1p1p/9/9/9/P1P1P1P1P/4O4/RVSJKQSVR w - - 0 1"
        );
        // A promotion-only Shaman (`*W → W`) and Divine Lord (`*L → D`) on the board.
        assert_eq!(to_fsf_dialect("4*W4/4*L4 b - - 0 1"), "4W4/4D4 b - - 0 1");
        // The side-to-move `b` is not mangled.
        assert!(to_fsf_dialect("9/9 b - - 0 1").ends_with("b - - 0 1"));
    }
}
