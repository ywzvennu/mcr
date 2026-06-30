//! Sho Shogi (old 9x9 Shogi **without drops**) differential perft + timing
//! against Fairy-Stockfish (issue #267).
//!
//! Sho Shogi runs on mce's **generic** `u128` engine
//! (`mce::geometry::ShoShogi`, a `GenericPosition<Shogi9x9, ShoShogiRules>`), like
//! Shogi / Chak, so it has its own corpus and comparison loop here. The FSF side
//! selects `UCI_Variant shoshogi`, sets the FEN, runs `go perft`, asserts the node
//! counts match, and reports mce-vs-FSF throughput. The corpus exercises the full
//! Shogi army and its `+`-promotions, the **Drunk Elephant** (seven-direction
//! step), its promotion to a **Crown Prince** (a second royal), and the
//! **count-thresholded** two-royal rule (with both royals a side is never in
//! check; with one, that piece is an ordinary royal).
//!
//! **FSF must be built with large-board support** (`make ... largeboards=yes`):
//! the default FSF build omits the 9x9 `shoshogi` variant from its `UCI_Variant`
//! list. When the running binary lacks `shoshogi`, this loop skips rather than
//! compare meaningless truncated counts. `shoshogi` is a **built-in** FSF variant
//! (not an INI one), so no `variants.ini` is loaded.
//!
//! ## FEN dialect
//!
//! mce and FSF share the Shogi letters — `l n s g k r b p` and the `+`-prefixed
//! promoted forms `+P +L +N +S +R +B` — but spell the two extra pieces
//! differently. FSF uses `e`/`E` for the Drunk Elephant and `+e`/`+E` for the
//! Crown Prince (a promoted Drunk Elephant); mce, the single-`*` overflow alphabet
//! being exhausted, uses the **doubled** overflow prefix `**` — `**e`/`**E` for the
//! Drunk Elephant and `**c`/`**C` for the Crown Prince. [`to_fsf_dialect`] walks
//! the placement field and rewrites `**e → e` and `**c → +E` (case-preserving),
//! leaving every shared Shogi letter, `+`-token, digit and `/` untouched. The
//! comparison asserts only node counts, so the move-string dialect never matters.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mce::geometry::{perft as gperft, ShoShogi, Shogi9x9};

use crate::uci::Engine;

/// One Sho Shogi corpus position. The FEN is mce's dialect; the FSF side
/// translates it via [`to_fsf_dialect`].
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Sho Shogi comparison corpus (all FSF-confirmed): the startpos, a developed
/// middlegame, a two-royal position (King + Crown Prince → never in check), a
/// Drunk Elephant in the promotion zone (each move may make a second royal), and a
/// lone Crown Prince standing in check (an ordinary royal). The same FENs are
/// pinned in `tests/perft_shoshogi.rs`.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "lnsgkgsnl/1r2**e2b1/ppppppppp/9/9/9/PPPPPPPPP/1B2**E2R1/LNSGKGSNL w - - 0 1",
        depth: 4,
    },
    Case {
        label: "midgame",
        fen: "lnsgkgsnl/1r2**e2b1/p1pppp1pp/1p4p2/9/2P3P2/PP1PPP1PP/1B2**E2R1/LNSGKGSNL w - - 0 1",
        depth: 4,
    },
    Case {
        label: "two-royals",
        fen: "4k4/9/9/9/9/9/4**C4/9/4K4 w - - 0 1",
        depth: 4,
    },
    Case {
        label: "de-promote",
        fen: "4k4/9/4**E4/9/9/9/9/9/4K4 w - - 0 1",
        depth: 4,
    },
    Case {
        label: "lone-crown-prince",
        fen: "3k5/9/9/9/9/9/9/r8/4**C4 w - - 0 1",
        depth: 4,
    },
];

/// Rewrites an mce-dialect Sho Shogi FEN to FSF's dialect: the doubled-overflow
/// Drunk Elephant `**e → e` and Crown Prince `**c → +E` (case-preserving); every
/// shared Shogi letter, `+`-token, digit and `/` passes through. Only the
/// placement field is rewritten; the side-to-move / clock fields are left intact.
pub(crate) fn to_fsf_dialect(fen: &str) -> String {
    let mut parts = fen.splitn(2, ' ');
    let placement = parts.next().unwrap_or("");
    let mut out = String::with_capacity(placement.len());
    let mut chars = placement.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '*' {
            // A doubled prefix `**` introduces a second-bank overflow token; the
            // base letter after it is the recycled letter whose case carries the
            // colour.
            let _second = chars.next(); // the second '*'
            let base = chars.next().unwrap_or('*');
            let upper = base.is_ascii_uppercase();
            match base.to_ascii_lowercase() {
                // Drunk Elephant: `**e → e`.
                'e' => out.push(if upper { 'E' } else { 'e' }),
                // Crown Prince: `**c → +E` (a promoted Drunk Elephant in FSF).
                'c' => {
                    out.push('+');
                    out.push(if upper { 'E' } else { 'e' });
                }
                // (Unreachable for a valid Sho Shogi FEN.)
                other => out.push(if upper {
                    other.to_ascii_uppercase()
                } else {
                    other
                }),
            }
        } else {
            // Shared Shogi letters, `+`-promotion tokens, digits and `/` are
            // identical in both dialects.
            out.push(c);
        }
    }
    match parts.next() {
        Some(rest) => format!("{out} {rest}"),
        None => out,
    }
}

/// A measured Sho Shogi comparison row.
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

/// Run the Sho Shogi corpus through mce and FSF. Returns the number of mismatches
/// (0 = all matched, or FSF lacks `shoshogi` and the suite is skipped). Prints a
/// table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!(
        "Sho Shogi (9x9, u128, no drops; Drunk Elephant / Crown Prince) — generic engine vs FSF \
UCI_Variant shoshogi (issue #267):"
    );
    println!("  (requires an FSF built with largeboards=yes)");

    if !engine.has_variant("shoshogi") {
        println!("  SKIP: this FSF binary has no `shoshogi` variant (build it largeboards=yes).");
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
                eprintln!("skip shoshogi/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mce_nodes).sum();
    let mce_s: f64 = rows.iter().map(|r| r.mce_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mce_s > 0.0 && fsf_s > 0.0 {
        println!(
            "shoshogi OVERALL: {nodes} nodes verified; mce {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mce_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mce_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Sho Shogi positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Sho Shogi parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH shoshogi/{} depth {}: mce={} fsf={}  FEN: {}",
                r.label, r.depth, r.mce_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Sho Shogi position through mce's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = ShoShogi::from_fen(case.fen).map_err(|e| format!("mce rejected FEN: {e:?}"))?;
    let mce_start = Instant::now();
    let mce_nodes = gperft::<Shogi9x9, _>(&pos, depth);
    let mce_secs = mce_start.elapsed().as_secs_f64();

    engine.set_variant("shoshogi", false)?;
    engine.set_position(&to_fsf_dialect(case.fen))?;
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

    /// The corpus FENs all parse on the generic Sho Shogi engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in `tests/perft_shoshogi.rs`
    /// (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 676u64),
            ("midgame", 1199),
            ("two-royals", 65),
            ("de-promote", 56),
            ("lone-crown-prince", 74),
        ];
        for case in CASES {
            let pos = ShoShogi::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Shogi9x9, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }

    /// The dialect rewrite maps the doubled-overflow tokens to FSF's spellings and
    /// leaves the shared Shogi letters intact.
    #[test]
    fn dialect_rewrites_royals() {
        assert_eq!(
            to_fsf_dialect(
                "lnsgkgsnl/1r2**e2b1/ppppppppp/9/9/9/PPPPPPPPP/1B2**E2R1/LNSGKGSNL w - - 0 1"
            ),
            "lnsgkgsnl/1r2e2b1/ppppppppp/9/9/9/PPPPPPPPP/1B2E2R1/LNSGKGSNL w - - 0 1"
        );
        assert_eq!(
            to_fsf_dialect("4k4/9/9/9/9/9/4**C4/9/4K4 w - - 0 1"),
            "4k4/9/9/9/9/9/4+E4/9/4K4 w - - 0 1"
        );
    }
}
