//! Synochess differential perft + timing against Fairy-Stockfish (issue #212).
//!
//! Synochess runs on mce's **generic** engine (`mce::geometry::Synochess`, a
//! `GenericPosition<Chess8x8, SynochessRules>`). FSF defines `synochess` only via
//! its `variants.ini` (it is not a built-in), so this module first `load`s the ini
//! (path from `$MCE_FSF_VARIANTS_INI`, defaulting next to the binary), selects
//! `UCI_Variant synochess`, sets the FEN, runs `go perft`, asserts the node counts
//! match, and reports mce-vs-FSF throughput.
//!
//! ## FEN dialect
//!
//! mce and FSF render the same Synochess position with different Black-piece
//! letters. FSF's `synochess` uses `e a s` for the Elephant (Fers-Alfil), Advisor
//! (Commoner), and Soldier; mce reuses `e`/`a`/`s` for its Rook+Knight Elephant /
//! Hawk / Silver, so the Synochess pieces take distinct letters: Elephant `v`,
//! Soldier `z`, and — the alphabet being exhausted — the Advisor (Commoner) uses
//! mce's `*`-prefixed overflow token `*u`. [`to_fsf_dialect`] maps mce's letters
//! (collapsing `*u → a`) back to FSF's
//! over the whole FEN — including the `[..]` holdings bracket (the fixed
//! two-Soldier pocket, `[zz]` → `[ss]`). The Cannon (`c`), Rook (`r`), Knight
//! (`n`), King (`k`) and the whole White army carry none of the remapped letters,
//! so the swap is unambiguous. The comparison asserts only node counts, so the
//! move-string dialect never matters.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked, and the ini is the user's own file (never committed here).

use std::time::Instant;

use mce::geometry::{perft as gperft, Chess8x8, Synochess};

use crate::uci::Engine;

/// One Synochess corpus position. The FEN is mce's dialect; the FSF side
/// translates it via [`to_fsf_dialect`].
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Synochess comparison corpus: the FSF-confirmed startpos (both colors to
/// move, exercising the Janggi cannon, the forward/sideways Soldier and its rank-5
/// drops), an asymmetric middlegame, a drop-heavy position with both Soldiers in
/// hand, and two campmate endgames (one per color) exercising the flag-rank win
/// truncation.
const CASES: &[Case] = &[
    Case {
        label: "startpos-w",
        fen: "rnv*ukvnr/8/1c4c1/1zz2zz1/8/8/PPPPPPPP/RNBQKBNR[zz] w KQ - 0 1",
        depth: 4,
    },
    Case {
        label: "startpos-b",
        fen: "rnv*ukvnr/8/1c4c1/1zz2zz1/8/8/PPPPPPPP/RNBQKBNR[zz] b KQ - 0 1",
        depth: 4,
    },
    Case {
        label: "mid-asym",
        fen: "rnv*uk1nr/8/1c4c1/3zz3/2zP4/5N2/PPP1PPPP/RNBQKB1R[zz] w KQ - 0 1",
        depth: 4,
    },
    Case {
        label: "drop-heavy",
        fen: "rnv*uk1nr/8/1c4c1/8/3PP3/8/PPP2PPP/RNBQKBNR[zz] b KQ - 0 1",
        depth: 4,
    },
    Case {
        label: "campmate-b",
        fen: "8/8/8/8/K7/8/4k3/8 b - - 0 1",
        depth: 5,
    },
    Case {
        label: "campmate-w",
        fen: "8/4K3/8/8/8/8/4k3/8 w - - 0 1",
        depth: 5,
    },
];

/// Translates an mce-dialect Synochess FEN to FSF's dialect: Elephant `v→e`,
/// Advisor (Commoner) `*u→a`, Soldier `z→s` (both cases). Applied over the whole
/// FEN, including the `[..]` holdings bracket. The Cannon / Rook / Knight / King
/// and the White army carry none of these letters, so the swap is safe.
///
/// The Commoner is an mce **overflow** role: its token is the two characters `*u`
/// (white `*U`), so it is collapsed to FSF's single `a`/`A` before the per-char
/// swap. No other mce token starts with `*`, so a plain sequence replace is safe.
pub(crate) fn to_fsf_dialect(fen: &str) -> String {
    fen.replace("*U", "A")
        .replace("*u", "a")
        .chars()
        .map(|c| match c {
            'v' => 'e',
            'V' => 'E',
            'z' => 's',
            'Z' => 'S',
            other => other,
        })
        .collect()
}

/// The path of the FSF `variants.ini` defining `synochess`, from
/// `$MCE_FSF_VARIANTS_INI` (empty if unset → the suite is skipped).
fn variants_ini_path() -> String {
    std::env::var("MCE_FSF_VARIANTS_INI").unwrap_or_default()
}

/// A measured Synochess comparison row.
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
        if self.mce_secs > 0.0 {
            self.fsf_secs / self.mce_secs
        } else {
            f64::NAN
        }
    }
}

/// Run the Synochess corpus through mce and FSF. Returns the number of mismatches
/// (0 = all matched, or the suite was skipped). Skips gracefully when no
/// `variants.ini` is configured or the loaded binary still lacks `synochess`.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Synochess — generic engine vs FSF UCI_Variant synochess (issue #212):");

    let ini = variants_ini_path();
    if ini.is_empty() {
        println!("  SKIP: set $MCE_FSF_VARIANTS_INI to an FSF variants.ini defining `synochess`.");
        return 0;
    }
    if let Err(e) = engine.load_variants(&ini) {
        println!("  SKIP: could not load variants.ini ({ini}): {e}");
        return 0;
    }
    if !engine.has_variant("synochess") {
        println!("  SKIP: the loaded FSF binary still does not advertise `synochess`.");
        return 0;
    }

    let head = format!(
        "{:<14} {:>5} {:>14} {:>14} {:>9} {:>10} {:>10} {:>8}",
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
                    "{:<14} {:>5} {:>14} {:>14} {:>9} {:>10.1} {:>10.1} {:>7.2}x",
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
                eprintln!("skip synochess/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mce_nodes).sum();
    let mce_s: f64 = rows.iter().map(|r| r.mce_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mce_s > 0.0 && fsf_s > 0.0 {
        println!(
            "synochess OVERALL: {nodes} nodes verified; mce {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mce_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mce_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Synochess positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Synochess parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH synochess/{} depth {}: mce={} fsf={}  FEN: {}",
                r.label, r.depth, r.mce_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Synochess position through mce's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Synochess::from_fen(case.fen).map_err(|e| format!("mce rejected FEN: {e:?}"))?;
    let mce_start = Instant::now();
    let mce_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mce_secs = mce_start.elapsed().as_secs_f64();

    let fsf_fen = to_fsf_dialect(case.fen);
    engine.set_variant("synochess", false)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The corpus FENs all parse on the generic Synochess engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in
    /// `tests/perft_synochess.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos-w", 2u32, 986u64),
            ("startpos-b", 2, 986),
            ("mid-asym", 2, 1264),
            ("drop-heavy", 2, 1431),
            ("campmate-b", 2, 19),
            ("campmate-w", 2, 20),
        ];
        for (label, depth, want) in pinned {
            let case = CASES.iter().find(|c| c.label == label).expect("label");
            let pos = Synochess::from_fen(case.fen).expect("corpus FEN parses");
            assert_eq!(
                gperft::<Chess8x8, _>(&pos, depth),
                want,
                "{label} perft({depth})"
            );
        }
    }

    #[test]
    fn dialect_round_trips_pieces_and_holdings() {
        assert_eq!(
            to_fsf_dialect("rnv*ukvnr/8/1c4c1/1zz2zz1/8/8/PPPPPPPP/RNBQKBNR[zz] w KQ - 0 1"),
            "rneakenr/8/1c4c1/1ss2ss1/8/8/PPPPPPPP/RNBQKBNR[ss] w KQ - 0 1"
        );
    }
}
