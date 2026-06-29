//! Cannon Shogi (9x9) differential perft + timing against Fairy-Stockfish
//! (issue #269).
//!
//! Cannon Shogi runs on mce's **generic** `u128` engine
//! (`mce::geometry::CannonShogi`, a `GenericPosition<Shogi9x9, CannonShogiRules>`),
//! so it has its own corpus and comparison loop here (mirroring `chak.rs` /
//! `shogi.rs`). It is the first variant to combine a Shogi hand with cannon-type
//! movers. The FSF side selects `UCI_Variant cannonshogi` (an **INI variant**: not
//! built in, loaded from FSF's `variants.ini`), sets the FEN, runs `go perft`, and
//! asserts the node counts match.
//!
//! ## FEN dialect
//!
//! mce and FSF agree on the Shogi pieces (`l n s g k r b`), the Soldier (`p`), and
//! the `[..]` holdings bracket, but spell the cannon army differently. mce reuses
//! the [`WideRole::Cannon`] (`c`) for FSF's `u`, and the three new movers plus four
//! promoted forms take the **second** overflow prefix `=`:
//!
//! | mce | FSF | piece |
//! |-----|-----|-------|
//! | `c` / `C` | `u` / `U` | Cannon (rook-cannon `mRcpR`) |
//! | `=a` / `=A` | `a` / `A` | Rook-cannon (`pR`) |
//! | `=c` / `=C` | `c` / `C` | Bishop-cannon (`mBcpB`) |
//! | `=i` / `=I` | `i` / `I` | Bishop-hopper (`pB`) |
//! | `=u` / `=U` | `+u` / `+U` | promoted Cannon |
//! | `=w` / `=W` | `+a` / `+A` | promoted Rook-cannon |
//! | `=f` / `=F` | `+c` / `+C` | promoted Bishop-cannon |
//! | `=e` / `=E` | `+i` / `+I` | promoted Bishop-hopper |
//!
//! [`fen_to_fsf`] rewrites the placement and holdings; an empty mce `[]` becomes
//! FSF's `[-]`. The turn / clock tail is passed through unchanged.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::path::{Path, PathBuf};
use std::time::Instant;

use mce::geometry::{perft as gperft, CannonShogi, Shogi9x9};

use crate::uci::Engine;

/// One Cannon Shogi corpus position.
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Cannon Shogi comparison corpus: the FSF-confirmed startpos; a promoted-Cannon
/// midgame with a Gold in hand (a hand drop, a promoted cannon, an over-screen
/// capture); and a sparse drop lab with both hands full of every cannon-type piece
/// (drops dominate the branching factor). Depths are modest by default; `full`
/// deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "lnsgkgsnl/1r=c=i1c=ab1/p1p1p1p1p/9/9/9/P1P1P1P1P/1B=AC1=I=CR1/LNSGKGSNL[] w - - 0 1",
        depth: 4,
    },
    Case {
        label: "promoted-midgame",
        fen: "lns=Ukgsnl/1r=c=i1c=ab1/p3p1p1p/2p6/9/9/P1P1P1P1P/1B=A2=I=CR1/LNSGKGSNL[G] b - - 0 2",
        depth: 3,
    },
    Case {
        label: "drop-lab",
        fen: "4k4/9/9/9/9/9/9/9/4K4[RBC=A=C=Irbc=a=c=i] w - - 0 1",
        depth: 3,
    },
];

/// Rewrites an mce Cannon Shogi FEN into FSF's `cannonshogi` dialect: the bare
/// Cannon `c`/`C` becomes `u`/`U`, each `=`-prefixed cannon token is mapped via
/// [`fsf_token`], and an empty hand `[]` becomes `[-]`. Every other character —
/// the Shogi pieces, the Soldier `p`, digits, and the turn/clock tail — is copied
/// verbatim.
pub fn fen_to_fsf(fen: &str) -> String {
    let mut out = String::with_capacity(fen.len());
    let mut chars = fen.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '=' => {
                // The next character is the recycled base letter; rewrite the pair.
                if let Some(&base) = chars.peek() {
                    chars.next();
                    out.push_str(fsf_token(base));
                    continue;
                }
                out.push(c);
            }
            // The bare Cannon (mce reuses `WideRole::Cannon`) is FSF's `u`.
            'c' => out.push('u'),
            'C' => out.push('U'),
            // An empty mce hand `[]` is FSF's `[-]`.
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

/// The FSF spelling of an mce Cannon Shogi `=`-overflow base letter (case = colour).
fn fsf_token(base: char) -> &'static str {
    match base {
        'a' => "a",
        'A' => "A",
        'c' => "c",
        'C' => "C",
        'i' => "i",
        'I' => "I",
        'u' => "+u",
        'U' => "+U",
        'w' => "+a",
        'W' => "+A",
        'f' => "+c",
        'F' => "+C",
        'e' => "+i",
        'E' => "+I",
        // Not a Cannon Shogi overflow base — keep the rewrite total. Unreached by
        // the corpus.
        _ => "?",
    }
}

pub fn run(engine: &mut Engine, fsf_bin: &str, full: bool) -> usize {
    println!();
    println!(
        "Cannon Shogi (9x9, u128, hand + drops + cannon-type movers) — generic engine \
vs FSF UCI_Variant cannonshogi (issue #269):"
    );

    if !engine.has_variant("cannonshogi") {
        match resolve_variants_ini(fsf_bin) {
            Some(ini) => {
                if let Err(e) = engine.load_variant_path(&ini.to_string_lossy()) {
                    println!("  (skipped: failed to load variants.ini: {e})");
                    return 0;
                }
            }
            None => {
                println!(
                    "  (skipped: no variants.ini found; set $MCE_FSF_VARIANTS_INI to FSF's \
                     variants.ini to enable the Cannon Shogi comparison)"
                );
                return 0;
            }
        }
    }
    if !engine.has_variant("cannonshogi") {
        println!(
            "  (skipped: this FSF binary does not advertise UCI_Variant cannonshogi even \
                  after loading variants.ini)"
        );
        return 0;
    }

    let head = format!(
        "{:<18} {:>5} {:>14} {:>14} {:>9} {:>10} {:>10} {:>8}",
        "position", "depth", "mce nodes", "fsf nodes", "match", "mce Mn/s", "fsf Mn/s", "mce/fsf",
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
                    row.mce_nodes,
                    row.fsf_nodes,
                    if row.matched { "ok" } else { "MISMATCH" },
                    row.mce_mnps(),
                    row.fsf_mnps(),
                    row.speedup(),
                );
                rows.push(row);
            }
            Err(e) => {
                eprintln!("skip cannonshogi/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mce_nodes).sum();
    let mce_s: f64 = rows.iter().map(|r| r.mce_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mce_s > 0.0 && fsf_s > 0.0 {
        println!(
            "cannonshogi OVERALL: {nodes} nodes verified; mce {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mce_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mce_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Cannon Shogi positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Cannon Shogi parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH cannonshogi/{} depth {}: mce={} fsf={}  FEN: {}",
                r.label, r.depth, r.mce_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Cannon Shogi position through mce's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = CannonShogi::from_fen(case.fen).map_err(|e| format!("mce rejected FEN: {e:?}"))?;
    let mce_start = Instant::now();
    let mce_nodes = gperft::<Shogi9x9, _>(&pos, depth);
    let mce_secs = mce_start.elapsed().as_secs_f64();

    let fsf_fen = fen_to_fsf(case.fen);
    engine.set_variant("cannonshogi", false)?;
    engine.set_position(&fsf_fen)?;
    let fsf = engine.go_perft(depth, false)?;

    Ok(Row {
        label: case.label,
        fen: case.fen,
        depth,
        mce_nodes,
        fsf_nodes: fsf.nodes,
        matched: mce_nodes == fsf.nodes,
        mce_secs,
        fsf_secs: fsf.elapsed.as_secs_f64(),
    })
}

/// Locate FSF's `variants.ini` (which defines `cannonshogi`): `$MCE_FSF_VARIANTS_INI`
/// first, then a sibling of the FSF binary, then the vendored build tree.
fn resolve_variants_ini(fsf_bin: &str) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MCE_FSF_VARIANTS_INI") {
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

/// A measured Cannon Shogi comparison row.
struct Row {
    label: &'static str,
    fen: &'static str,
    depth: u32,
    mce_nodes: u64,
    fsf_nodes: u64,
    matched: bool,
    mce_secs: f64,
    fsf_secs: f64,
}

impl Row {
    fn mce_mnps(&self) -> f64 {
        if self.mce_secs > 0.0 {
            self.mce_nodes as f64 / self.mce_secs / 1e6
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
        if self.mce_secs > 0.0 {
            self.fsf_secs / self.mce_secs
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The corpus FENs all parse on the generic Cannon Shogi engine, round-trip the
    /// placement through `fen_to_fsf`, and the pinned shallow counts match the
    /// FSF-confirmed numbers in `tests/perft_cannonshogi.rs` (runs without FSF).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 3447u64),
            ("promoted-midgame", 5665),
            ("drop-lab", 215443),
        ];
        for case in CASES {
            let pos = CannonShogi::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Shogi9x9, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }

    /// The FEN rewriter maps the mce cannon dialect to FSF's spelling.
    #[test]
    fn fen_rewrite_matches_fsf_dialect() {
        assert_eq!(
            fen_to_fsf(
                "lnsgkgsnl/1r=c=i1c=ab1/p1p1p1p1p/9/9/9/P1P1P1P1P/1B=AC1=I=CR1/LNSGKGSNL[] w - - 0 1"
            ),
            "lnsgkgsnl/1rci1uab1/p1p1p1p1p/9/9/9/P1P1P1P1P/1BAU1ICR1/LNSGKGSNL[-] w - - 0 1"
        );
        // Promoted forms and a non-empty hand.
        assert_eq!(fen_to_fsf("=U=W=F=E[G]"), "+U+A+C+I[G]");
    }
}
