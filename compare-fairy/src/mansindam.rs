//! Mansindam differential perft + timing against Fairy-Stockfish (issue #271).
//!
//! Mansindam runs on mcr's **generic** engine (`mcr::geometry::Mansindam`, a
//! `GenericPosition<Shogi9x9, MansindamRules>`), like the other fairy variants, so
//! it has its own corpus and comparison loop here. The FSF side selects
//! `UCI_Variant mansindam`, sets the FEN, runs `go perft`, asserts the node counts
//! match, and reports mcr-vs-FSF throughput.
//!
//! ## Mansindam is an INI variant
//!
//! Like Orda / Chak / Empire, FSF defines `mansindam` in its `variants.ini` data
//! file, not in the binary. The harness loads the INI
//! (`setoption name VariantPath value <variants.ini>`) before checking
//! [`Engine::has_variant`](crate::uci::Engine::has_variant); the INI path is
//! resolved from `$MCR_FSF_VARIANTS_INI`, then a `variants.ini` beside the FSF
//! binary, then the harness build dir. If none is found (or the loaded INI still
//! lacks `mansindam`), the block is skipped cleanly.
//!
//! ## FEN dialect
//!
//! mcr and FSF render the same Mansindam position with **different piece tokens**.
//! [`to_fsf_dialect`] walks the placement field (including the `[..]` hand bracket)
//! token by token, rewriting each mcr token to FSF's spelling:
//!
//! | piece | mcr token | FSF token |
//! |-------|-----------|-----------|
//! | Cardinal (B+N)  | `a` | `c` |
//! | Marshal (R+N)   | `e` | `m` |
//! | Angel (Q+N)     | `**a` | `a` |
//! | Guard (promoted Pawn)     | `*u` | `+p` |
//! | Centaur (promoted Knight) | `w`  | `+n` |
//! | Archer (promoted Bishop)  | `+b` | `+b` |
//! | Tiger (promoted Rook)     | `+r` | `+r` |
//! | Rhino (promoted Cardinal) | `**i` | `+c` |
//! | Ship (promoted Marshal)   | `**s` | `+m` |
//!
//! The standard `p n b r q k` (and the bare `a`/`e` swaps above) round-trip
//! directly. Case carries colour throughout (uppercase = White), so the rewrite
//! preserves it. The comparison asserts only node counts, so the move-string
//! dialect never matters.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no GPL
//! code is linked, and the INI is a plain data file.

use std::path::{Path, PathBuf};
use std::time::Instant;

use mcr::geometry::{perft as gperft, Mansindam, Shogi9x9};

use crate::uci::Engine;

/// One Mansindam corpus position. The FEN is mcr's dialect; the FSF side translates
/// it via [`to_fsf_dialect`].
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Mansindam comparison corpus (all FSF-confirmed): the startpos, the 1.e4 e5
/// opening, a drop swarm (bare kings, Knight/Bishop/Rook in hand), the nifu pawn
/// filter, two promotion-zone movers, a clean flag race, a board of promoted
/// movers, a lone Angel, and a capture-to-hand midgame.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rnb**akqane/9/ppppppppp/9/9/9/PPPPPPPPP/9/ENAQK**ABNR[] w - - 0 1",
        depth: 4,
    },
    Case {
        label: "e4e5",
        fen: "rnb**akqane/9/pppp1pppp/4p4/9/4P4/PPPP1PPPP/9/ENAQK**ABNR[] w - - 2 2",
        depth: 4,
    },
    Case {
        label: "drops",
        fen: "4k4/9/9/9/9/9/9/9/4K4[NBRnbr] w - - 0 1",
        depth: 3,
    },
    Case {
        label: "nifu",
        fen: "4k4/9/9/9/4P4/9/9/9/4K4[Pp] w - - 0 1",
        depth: 3,
    },
    Case {
        label: "promo-zone",
        fen: "4k4/9/A7E/9/9/9/9/9/4K4[] w - - 0 1",
        depth: 4,
    },
    Case {
        label: "flag-race",
        fen: "9/3K5/9/9/9/9/9/5k3/9[] w - - 0 1",
        depth: 4,
    },
    Case {
        label: "promoted",
        fen: "9/k8/2+B1+R1**I2/9/2**S1W1*U2/9/9/9/4K4[Nn] w - - 0 1",
        depth: 3,
    },
    Case {
        label: "angel",
        fen: "1k7/9/9/9/4**A4/9/9/9/4K4[] w - - 0 1",
        depth: 4,
    },
    Case {
        label: "capture-seq",
        fen: "rnb**akqane/9/ppp2pppp/3pP4/9/9/PPPP1PPPP/9/ENAQK**ABNR[P] b - - 0 3",
        depth: 4,
    },
];

/// Translates an mcr-dialect Mansindam FEN to FSF's dialect over the **placement
/// field only** (which includes the `[..]` hand bracket). Walks token by token so
/// the three prefixes (`**`, `*`, `+`) and the bare-letter swaps never collide;
/// case is preserved (it carries colour). The side-to-move / clock fields are left
/// intact.
pub(crate) fn to_fsf_dialect(fen: &str) -> String {
    let mut parts = fen.splitn(2, ' ');
    let placement = parts.next().unwrap_or("");
    let mut out = String::with_capacity(placement.len());
    let mut chars = placement.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '*' {
            if chars.peek() == Some(&'*') {
                // A second-bank overflow token `**X`.
                chars.next();
                let base = chars.next().unwrap_or('*');
                let upper = base.is_ascii_uppercase();
                // Angel `**a → a` (a bare base piece); Rhino `**i → +c` and Ship
                // `**s → +m` (FSF promoted Cardinal / Marshal, two chars).
                match base.to_ascii_lowercase() {
                    'a' => out.push(if upper { 'A' } else { 'a' }),
                    'i' => push_promoted(&mut out, 'c', upper),
                    's' => push_promoted(&mut out, 'm', upper),
                    other => out.push(if upper {
                        other.to_ascii_uppercase()
                    } else {
                        other
                    }),
                }
            } else {
                // A single-`*` overflow token `*X`: the Guard `*u → +p` (FSF
                // promoted Pawn).
                let base = chars.next().unwrap_or('*');
                let upper = base.is_ascii_uppercase();
                match base.to_ascii_lowercase() {
                    'u' => push_promoted(&mut out, 'p', upper),
                    other => out.push(if upper {
                        other.to_ascii_uppercase()
                    } else {
                        other
                    }),
                }
            }
        } else if c == '+' {
            // A Shogi `+`-promoted token: the Archer `+B` and Tiger `+R` are
            // identical in both dialects (FSF promoted Bishop / Rook).
            let base = chars.next().unwrap_or('+');
            out.push('+');
            out.push(base);
        } else if c == 'w' || c == 'W' {
            // The Centaur (= Kheshig, bare letter `w`) is FSF's promoted Knight.
            push_promoted(&mut out, 'n', c == 'W');
        } else {
            // A bare letter: swap the reused Cardinal (`a → c`) and Marshal
            // (`e → m`); `p n b r q k`, digits and `/`, `[`, `]` pass through.
            let mapped = match c {
                'a' => 'c',
                'A' => 'C',
                'e' => 'm',
                'E' => 'M',
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

/// Pushes FSF's `+`-promoted token (`+<base>`), the base's case carrying the
/// colour: `upper` (a White piece) yields `+B`-style uppercase, else lowercase.
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

/// A measured Mansindam comparison row.
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

/// Run the Mansindam corpus through mcr and FSF. Returns the number of mismatches
/// (0 = all matched). Loads FSF's `variants.ini` first (Mansindam is an INI
/// variant); if the INI cannot be found or still lacks `mansindam`, the block is
/// skipped (returns 0) rather than reporting spurious mismatches.
pub fn run(engine: &mut Engine, fsf_bin: &str, full: bool) -> usize {
    println!();
    println!("Mansindam — generic engine vs FSF UCI_Variant mansindam (issue #271):");

    if !engine.has_variant("mansindam") {
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
                     variants.ini to enable the Mansindam comparison)"
                );
                return 0;
            }
        }
    }
    if !engine.has_variant("mansindam") {
        println!(
            "  (skipped: this FSF binary does not advertise UCI_Variant mansindam even after \
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
                eprintln!("skip mansindam/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "mansindam OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Mansindam positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Mansindam parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH mansindam/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Mansindam position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Mansindam::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Shogi9x9, _, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    let fsf_fen = to_fsf_dialect(case.fen);
    engine.set_variant("mansindam", false)?;
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

    /// The corpus FENs all parse on the generic Mansindam engine, round-trip
    /// through mcr's FEN I/O, and the pinned shallow counts match the FSF-confirmed
    /// numbers in `tests/perft_mansindam.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 3u32, 32238u64),
            ("e4e5", 3, 75941),
            ("drops", 3, 7839499),
            ("nifu", 3, 63334),
            ("promo-zone", 3, 5420),
            ("flag-race", 3, 200),
            ("promoted", 3, 1331492),
            ("angel", 3, 5648),
            ("capture-seq", 3, 77346),
        ];
        for case in CASES {
            let pos = Mansindam::from_fen(case.fen).expect("corpus FEN parses");
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

    /// The dialect swap maps the bare Cardinal / Marshal (`a → c`, `e → m`), the
    /// Angel (`**a → a`), the Guard (`*u → +p`), the Rhino / Ship second-bank
    /// promotions (`**i → +c`, `**s → +m`), and the shared `+B` / `+R`, leaving
    /// structural fields intact.
    #[test]
    fn dialect_swap_maps_mansindam_tokens() {
        let mcr = "rnb**akqane/9/ppppppppp/9/9/9/PPPPPPPPP/9/ENAQK**ABNR[] w - - 0 1";
        assert_eq!(
            to_fsf_dialect(mcr),
            "rnbakqcnm/9/ppppppppp/9/9/9/PPPPPPPPP/9/MNCQKABNR[] w - - 0 1"
        );
        // The promoted board: Archer `+B`, Tiger `+R`, Rhino `**I → +C`, Ship
        // `**S → +M`, Centaur `W → +N`, Guard `*U → +P`; the hand `[Nn]` passes
        // through.
        assert_eq!(
            to_fsf_dialect("9/k8/2+B1+R1**I2/9/2**S1W1*U2/9/9/9/4K4[Nn] w - - 0 1"),
            "9/k8/2+B1+R1+C2/9/2+M1+N1+P2/9/9/9/4K4[Nn] w - - 0 1"
        );
        // The side-to-move `b` is not mangled.
        assert!(to_fsf_dialect("9/9 b - - 0 1").ends_with("b - - 0 1"));
    }
}
