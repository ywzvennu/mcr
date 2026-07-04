//! Xiang Fu (9x9 Xiangqi-themed drop variant) differential perft + timing against
//! Fairy-Stockfish (issue #274).
//!
//! Xiang Fu runs on mcr's **generic** `u128` engine (`mcr::geometry::Xiangfu`, a
//! `GenericPosition<Shogi9x9, XiangfuRules>`), so it has its own corpus and
//! comparison loop here (mirroring `cannonshogi.rs` / `chak.rs`). It is the first
//! variant to combine the **multi-royal pseudo-royal** path with a Shogi-style
//! **hand**: two ring-confined royal Champions under a duple-check rule, plus
//! captures-to-hand drops. The FSF side selects `UCI_Variant xiangfu` (an **INI
//! variant**: not built in, loaded from FSF's `variants.ini`), sets the FEN, runs
//! `go perft`, and asserts the node counts match.
//!
//! ## FEN dialect
//!
//! mcr and FSF agree on the Chariot (`r`), Bishop (`b`), Cannon (`c`), the digits,
//! and the `[..]` holdings bracket, but spell the rest differently:
//!
//! | mcr | FSF | piece |
//! |-----|-----|-------|
//! | `=k` / `=K` | `+g` / `+G` | Champion (royal, ring-confined) |
//! | `*u` / `*U` | `g` / `G` | Pupil (the plain Commoner; hand only) |
//! | `=m` / `=M` | `m` / `M` | Mahout (`nAnD`) |
//! | `=c` / `=C` | `w` / `W` | Crossbow (bishop-cannon `mBcpB`) |
//! | `j` / `J` | `n` / `N` | Horse (hobbled Xiangqi knight) |
//! | `c` / `C` | `c` / `C` | Cannon (`mRcpR`) |
//! | `b` / `B`, `r` / `R` | `b` / `B`, `r` / `R` | Bishop, Chariot |
//!
//! [`fen_to_fsf`] rewrites the placement and holdings; an empty mcr `[]` becomes
//! FSF's `[-]`. The turn / clock tail is passed through unchanged.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::path::{Path, PathBuf};
use std::time::Instant;

use mcr::geometry::{perft as gperft, Shogi9x9, Xiangfu};

use crate::uci::Engine;

/// One Xiang Fu corpus position.
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Xiang Fu comparison corpus: the FSF-confirmed startpos; a Horse-and-cannon
/// midgame; a drops lab with a captured Pupil in each hand; and a duple-check
/// position with the Champions advanced so one may legally step beside the enemy
/// Champion. Depths are modest by default; `full` deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "2rb=m4/2c=cj4/2=k1=k4/9/9/9/4=K1=K2/4J=CC2/4=MBR2[] w - - 0 1",
        depth: 4,
    },
    Case {
        label: "midgame",
        fen: "2rb=m4/2c=cj4/2=k1=k4/9/9/9/2J1=K1=K2/5=CC2/4=MBR2[] b - - 2 1",
        depth: 4,
    },
    Case {
        label: "drops",
        fen: "2rb=m4/2c=cj4/2=k1=k4/9/9/9/4=K1=K2/4J=CC2/4=MBR2[*U*u] w - - 0 1",
        depth: 3,
    },
    Case {
        label: "duple",
        fen: "2rb=m4/2c=cj4/2=k6/4=k4/9/4=K4/6=K2/4J=CC2/4=MBR2[] w - - 3 2",
        depth: 3,
    },
];

/// Rewrites an mcr Xiang Fu FEN into FSF's `xiangfu` dialect: each `=`-prefixed
/// overflow token (`=k → +g`, `=m → m`, `=c → w`) and the `*u`/`*U` Pupil
/// (`*u → g`) are mapped via [`fsf_token`], the bare Horse `j`/`J` becomes `n`/`N`,
/// and an empty hand `[]` becomes `[-]`. Every other character — the Chariot,
/// Bishop, Cannon, digits, and the turn/clock tail — is copied verbatim.
pub fn fen_to_fsf(fen: &str) -> String {
    let mut out = String::with_capacity(fen.len());
    let mut chars = fen.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // A two-character overflow token: the prefix plus the recycled base
            // letter (case = colour).
            '=' | '*' => {
                if let Some(&base) = chars.peek() {
                    chars.next();
                    out.push_str(fsf_token(c, base));
                    continue;
                }
                out.push(c);
            }
            // The bare Horse (mcr reuses `WideRole::Horse`, spelled `j`) is FSF's `n`.
            'j' => out.push('n'),
            'J' => out.push('N'),
            // An empty mcr hand `[]` is FSF's `[-]`.
            '[' => {
                out.push('[');
                if chars.peek() == Some(&']') {
                    out.push('-');
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// The FSF spelling of an mcr Xiang Fu overflow token: `prefix` is `=` (Champion /
/// Mahout / Crossbow) or `*` (Pupil), `base` the recycled base letter (case =
/// colour).
fn fsf_token(prefix: char, base: char) -> &'static str {
    match (prefix, base) {
        // Champion: the promoted commoner `+g`.
        ('=', 'k') => "+g",
        ('=', 'K') => "+G",
        // Mahout.
        ('=', 'm') => "m",
        ('=', 'M') => "M",
        // Crossbow (bishop-cannon).
        ('=', 'c') => "w",
        ('=', 'C') => "W",
        // Pupil: the plain commoner `g`.
        ('*', 'u') => "g",
        ('*', 'U') => "G",
        // Not a Xiang Fu overflow base — keep the rewrite total. Unreached by the
        // corpus.
        _ => "?",
    }
}

pub fn run(engine: &mut Engine, fsf_bin: &str, full: bool) -> usize {
    println!();
    println!(
        "Xiang Fu (9x9, u128, ring-confined royal Champions + duple check + hand drops) — \
generic engine vs FSF UCI_Variant xiangfu (issue #274):"
    );

    if !engine.has_variant("xiangfu") {
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
                     variants.ini to enable the Xiang Fu comparison)"
                );
                return 0;
            }
        }
    }
    if !engine.has_variant("xiangfu") {
        println!(
            "  (skipped: this FSF binary does not advertise UCI_Variant xiangfu even \
                  after loading variants.ini)"
        );
        return 0;
    }

    let head = format!(
        "{:<18} {:>5} {:>14} {:>14} {:>9} {:>10} {:>10} {:>8}",
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
                    "{:<18} {:>5} {:>14} {:>14} {:>9} {:>10.1} {:>10.1} {:>7.2}x",
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
                eprintln!("skip xiangfu/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "xiangfu OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Xiang Fu positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Xiang Fu parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH xiangfu/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Xiang Fu position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Xiangfu::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Shogi9x9, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    let fsf_fen = fen_to_fsf(case.fen);
    engine.set_variant("xiangfu", false)?;
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

/// Locate FSF's `variants.ini` (which defines `xiangfu`): `$MCR_FSF_VARIANTS_INI`
/// first, then a sibling of the FSF binary, then the vendored build tree.
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

/// A measured Xiang Fu comparison row.
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
            0.0
        }
    }
    fn fsf_mnps(&self) -> f64 {
        if self.fsf_secs > 0.0 {
            self.fsf_nodes as f64 / self.fsf_secs / 1e6
        } else {
            0.0
        }
    }
    fn speedup(&self) -> f64 {
        if self.mcr_secs > 0.0 {
            self.fsf_secs / self.mcr_secs
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The corpus FENs all parse on the generic Xiang Fu engine and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_xiangfu.rs`
    /// (runs without FSF).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 260u64),
            ("midgame", 439),
            ("drops", 790),
            ("duple", 678),
        ];
        for case in CASES {
            let pos = Xiangfu::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Shogi9x9, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }

    /// The FEN rewriter maps the mcr Xiang Fu dialect to FSF's spelling.
    #[test]
    fn fen_rewrite_matches_fsf_dialect() {
        assert_eq!(
            fen_to_fsf("2rb=m4/2c=cj4/2=k1=k4/9/9/9/4=K1=K2/4J=CC2/4=MBR2[] w - - 0 1"),
            "2rbm4/2cwn4/2+g1+g4/9/9/9/4+G1+G2/4NWC2/4MBR2[-] w - - 0 1"
        );
        // The captured Pupils in hand (`*U`/`*u`) become FSF's `G`/`g`.
        assert_eq!(fen_to_fsf("[*U*u]"), "[Gg]");
    }
}
