//! Tori Shogi (bird shogi, 7x7) differential perft + timing against
//! Fairy-Stockfish `UCI_Variant torishogi` (issue #231).
//!
//! Tori Shogi runs on mce's **generic** `u128` engine (`mce::geometry::Tori`, a
//! `GenericPosition<Tori7x7, ToriRules>`), not the concrete 8x8 `AnyVariant`
//! layer the rest of this harness drives, so it has its own corpus and comparison
//! loop here (mirroring `shogi.rs` / `minishogi.rs`). The FSF side selects
//! `UCI_Variant torishogi`, sets the FEN, runs `go perft`, asserts the node counts
//! match, and reports mce-vs-FSF throughput. The corpus exercises the startpos,
//! **drops** with Swallows in hand, a **promotion** midgame (forced promotion in
//! the two-rank zone), a **promoted-piece** midgame (Goose / Eagle on the board,
//! each reverting on capture), and the **asymmetric quails**.
//!
//! **FSF must be built with large-board support** (`make ... largeboards=yes`):
//! the default FSF build omits the 7x7 `torishogi` variant from its `UCI_Variant`
//! list. When the running binary lacks `torishogi`, this loop skips rather than
//! compare meaningless truncated counts.
//!
//! ## FEN dialect
//!
//! Every Tori bird is an mce **overflow role** spelled with the `*` prefix and a
//! recycled base letter (`*y` swallow, `*g` goose, `*a` falcon, `*i` eagle, `*k`
//! crane, `*v`/`*r` quails, `*z` pheasant), the case carrying the colour. The
//! swallow / crane / left-quail / pheasant take `y`/`k`/`v`/`z` because the Chak
//! army already recycles the `s`/`o`/`l`/`p` overflow bases. FSF spells them with
//! bare letters and the Shogi `+`-promotion tokens. So the mce FEN is rewritten to
//! FSF's dialect before driving FSF:
//!
//! | mce | FSF | piece |
//! |-----|-----|-------|
//! | `*y` / `*Y` | `s` / `S` | Swallow |
//! | `*g` / `*G` | `+s` / `+S` | Goose (promoted Swallow) |
//! | `*a` / `*A` | `f` / `F` | Falcon |
//! | `*i` / `*I` | `+f` / `+F` | Eagle (promoted Falcon) |
//! | `*k` / `*K` | `c` / `C` | Crane |
//! | `*v` / `*V` | `l` / `L` | Left Quail |
//! | `*r` / `*R` | `r` / `R` | Right Quail |
//! | `*z` / `*Z` | `p` / `P` | Pheasant |
//!
//! The King (`k` / `K`) is a plain role in both engines, and the `[..]`
//! holdings-bracket convention for the hand is shared, so only the `*`-tokens are
//! rewritten (in both the board and the hand).
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no
//! GPL code is linked.

use std::time::Instant;

use mce::geometry::{perft as gperft, Tori, Tori7x7};

use crate::uci::Engine;

/// One Tori Shogi corpus position (in mce's `*`-overflow spelling).
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Tori Shogi comparison corpus: the FSF-confirmed startpos; a Swallow-drop
/// midgame; a forced-promotion midgame (Swallows / Falcon near the zone); a
/// promoted-piece midgame (Goose / Eagle on the board); and a quail-active
/// midgame (both asymmetric quails plus a crane of each colour). Depths are
/// modest by default; `full` deepens by one ply.
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "*r*z*kk*k*z*v/3*a3/*y*y*y*y*y*y*y/2*y1*Y2/*Y*Y*Y*Y*Y*Y*Y/3*A3/*V*Z*KK*K*Z*R[] w - - 0 1",
        depth: 4,
    },
    Case {
        label: "drops-in-hand",
        fen: "*r*z*kk*k*z*v/3*a3/*y*y*y*y*y*y*y/7/*Y*Y*Y*Y*Y*Y*Y/3*A3/*V*Z*KK*K*Z*R[*Y*y] w - - 0 1",
        depth: 3,
    },
    Case {
        label: "promo",
        fen: "2k4/1*Y5/7/3*A3/7/5*y1/4K2[*Y*A*y*a] w - - 0 1",
        depth: 3,
    },
    Case {
        label: "promoted",
        fen: "3k3/2*G4/7/3*I3/7/2*i4/3K3[*Y*y] w - - 0 1",
        depth: 3,
    },
    Case {
        label: "quails",
        fen: "3k3/7/2*v*r3/7/3*V*R2/7/3K3[*Y*y*A*a] w - - 0 1",
        depth: 3,
    },
];

/// Rewrites an mce Tori FEN (or any field of one) into FSF's dialect: each
/// `*`-prefixed overflow token becomes FSF's letter / `+`-token, the case
/// preserving the colour. Every other character is copied verbatim.
pub fn fen_to_fsf(fen: &str) -> String {
    let mut out = String::with_capacity(fen.len());
    let mut chars = fen.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '*' {
            // The next character is the recycled base letter; rewrite the pair.
            if let Some(&base) = chars.peek() {
                chars.next();
                out.push_str(fsf_token(base));
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// The FSF spelling of an mce Tori overflow base letter (case = colour).
fn fsf_token(base: char) -> &'static str {
    match base {
        'Y' => "S",
        'y' => "s",
        'G' => "+S",
        'g' => "+s",
        'A' => "F",
        'a' => "f",
        'I' => "+F",
        'i' => "+f",
        'K' => "C",
        'k' => "c",
        'V' => "L",
        'v' => "l",
        'R' => "R",
        'r' => "r",
        'Z' => "P",
        'z' => "p",
        // Not a Tori overflow base — emit the `*` and letter back unchanged. This
        // path is unreached by the corpus but keeps the rewrite total.
        _ => "?",
    }
}

/// A measured Tori Shogi comparison row.
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

/// Run the Tori Shogi corpus through mce and FSF. Returns the number of
/// mismatches (0 = all matched, or FSF lacks `torishogi` and the suite is
/// skipped). Prints a table and a one-line summary.
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!(
        "Tori Shogi (7x7, u128, bird army + hand + drops + promotion zone) — generic engine vs \
FSF UCI_Variant torishogi (issue #231):"
    );
    println!("  (requires an FSF built with largeboards=yes)");

    if !engine.has_variant("torishogi") {
        println!("  SKIP: this FSF binary has no `torishogi` variant (build it largeboards=yes).");
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
                eprintln!("skip torishogi/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mce_nodes).sum();
    let mce_s: f64 = rows.iter().map(|r| r.mce_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mce_s > 0.0 && fsf_s > 0.0 {
        println!(
            "torishogi OVERALL: {nodes} nodes verified; mce {:.1} Mn/s vs fsf {:.1} Mn/s ({:.2}x).",
            nodes as f64 / mce_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mce_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Tori Shogi positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Tori Shogi parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH torishogi/{} depth {}: mce={} fsf={}  FEN: {}",
                r.label, r.depth, r.mce_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Tori Shogi position through mce's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Tori::from_fen(case.fen).map_err(|e| format!("mce rejected FEN: {e:?}"))?;
    let mce_start = Instant::now();
    let mce_nodes = gperft::<Tori7x7, _>(&pos, depth);
    let mce_secs = mce_start.elapsed().as_secs_f64();

    // Rewrite the mce `*`-overflow FEN into FSF's bare-letter / `+`-token dialect.
    let fsf_fen = fen_to_fsf(case.fen);
    engine.set_variant("torishogi", false)?;
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

    /// The corpus FENs all parse on the generic Tori engine, and the pinned
    /// shallow counts match the FSF-confirmed numbers in
    /// `tests/perft_torishogi.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos", 288u64),
            ("drops-in-hand", 1269),
            ("promo", 7588),
            ("promoted", 154),
            ("quails", 7697),
        ];
        for case in CASES {
            let pos = Tori::from_fen(case.fen).expect("corpus FEN parses");
            let n = gperft::<Tori7x7, _>(&pos, 2);
            let want = pinned
                .iter()
                .find(|(l, _)| *l == case.label)
                .map(|(_, n)| *n)
                .expect("a pinned depth-2 count for the case");
            assert_eq!(n, want, "{} depth-2 perft", case.label);
        }
    }

    /// The FEN rewrite turns mce's `*`-overflow spelling into FSF's dialect.
    #[test]
    fn fen_rewrite_matches_fsf_dialect() {
        assert_eq!(
            fen_to_fsf(
                "*r*z*kk*k*z*v/3*a3/*y*y*y*y*y*y*y/2*y1*Y2/*Y*Y*Y*Y*Y*Y*Y/3*A3/*V*Z*KK*K*Z*R[] w - - 0 1"
            ),
            "rpckcpl/3f3/sssssss/2s1S2/SSSSSSS/3F3/LPCKCPR[] w - - 0 1"
        );
        assert_eq!(
            fen_to_fsf("3k3/2*G4/7/3*I3/7/2*i4/3K3[*Y*y] w - - 0 1"),
            "3k3/2+S4/7/3+F3/7/2+f4/3K3[Ss] w - - 0 1"
        );
    }
}
