//! Hoppel-Poppel differential perft + timing against Fairy-Stockfish (issue #225).
//!
//! Hoppel-Poppel runs on mcr's **generic** engine (`mcr::geometry::HoppelPoppel`, a
//! `GenericPosition<Chess8x8, HoppelPoppelRules>`), like the other fairy variants,
//! so it has its own corpus and comparison loop here. The FSF side selects
//! `UCI_Variant hoppelpoppel` (a built-in — no `variants.ini` needed), sets the FEN,
//! runs `go perft`, asserts the node counts match, and reports mcr-vs-FSF
//! throughput.
//!
//! ## FEN dialect
//!
//! mcr and FSF render the same Hoppel-Poppel position with **different letters** for
//! the two redefined pieces. FSF keeps the standard letters `n` (its `KNIBIS`,
//! knight-moves / bishop-captures) and `b` (its `BISKNI`, bishop-moves /
//! knight-captures); mcr already names `n` the standard Knight and `b` the standard
//! Bishop, so the Hoppel-Poppel pieces take `*`-prefixed overflow tokens — `*h`
//! (Knight-Bishop) and `*b` (Bishop-Knight). [`to_fsf_dialect`] rewrites the
//! placement field `*h → n`, `*b → b` (both cases) so the FSF FEN matches; the
//! side-to-move / castling / clock fields are left intact (they carry no remapped
//! token). The comparison asserts only node counts, so the move-string dialect never
//! matters.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no GPL
//! code is linked, and Hoppel-Poppel needs no INI.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Chess8x8, HoppelPoppel};

use crate::uci::Engine;

/// One Hoppel-Poppel corpus position. The FEN is mcr's dialect; the FSF side
/// translates it via [`to_fsf_dialect`].
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Hoppel-Poppel comparison corpus (all FSF-confirmed): the startpos (both
/// colours), a developed middlegame, two bishop/knight-rich middlegames that fire
/// the distinctive captures (Knight-Bishop bishop-diagonal, Bishop-Knight
/// knight-leap), and a tactic exercising the `q r b n` promotion set.
const CASES: &[Case] = &[
    Case {
        label: "startpos-w",
        fen: "r*h*bqk*b*hr/pppppppp/8/8/8/8/PPPPPPPP/R*H*BQK*B*HR w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "startpos-b",
        fen: "r*h*bqk*b*hr/pppppppp/8/8/8/8/PPPPPPPP/R*H*BQK*B*HR b KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "midgame-1",
        fen: "r1*bqk*b*hr/pppp1ppp/2*h5/4p3/4P3/2*H5/PPPP1PPP/R1*BQK*B*HR w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "midgame-2",
        fen: "r2qk2r/ppp2ppp/2*hp1*h2/2*b1p1*B1/2*B1P1*b1/2*HP1*H2/PPP2PPP/R2QK2R w KQkq - 0 1",
        depth: 4,
    },
    Case {
        label: "midgame-3",
        fen: "2kr3r/pp1*h1ppp/2p1p*h2/q7/3P4/2*H*BP*H2/PPQ2PPP/2KR3R w - - 0 1",
        depth: 4,
    },
    Case {
        label: "tactic-promo",
        fen: "4k3/Pp4*h1/8/3*b4/3*H4/8/1p2*H3/4K3 w - - 0 1",
        depth: 4,
    },
];

/// Translates an mcr-dialect Hoppel-Poppel FEN to FSF's dialect: rewrite the
/// Knight-Bishop overflow token `*h → n` and the Bishop-Knight `*b → b` (both
/// cases). Applied to the **placement field only** (the side-to-move / castling /
/// clock fields are left intact); they carry no `*`-prefixed token, so the swap is
/// unambiguous. The `*b → b` and `*h → n` order matters only in that the `*`
/// prefix is consumed atomically with its letter.
pub(crate) fn to_fsf_dialect(fen: &str) -> String {
    let mut parts = fen.splitn(2, ' ');
    let placement = parts
        .next()
        .unwrap_or("")
        // Knight-Bishop: mcr `*h` → FSF `n` (both cases).
        .replace("*H", "N")
        .replace("*h", "n")
        // Bishop-Knight: mcr `*b` → FSF `b` (both cases).
        .replace("*B", "B")
        .replace("*b", "b");
    match parts.next() {
        Some(rest) => format!("{placement} {rest}"),
        None => placement,
    }
}

/// A measured Hoppel-Poppel comparison row.
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

/// Run the Hoppel-Poppel corpus through mcr and FSF. Returns the number of
/// mismatches (0 = all matched). `hoppelpoppel` is a FSF built-in, so if this binary
/// does not advertise it the block is skipped cleanly (returns 0).
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!("Hoppel-Poppel — generic engine vs FSF UCI_Variant hoppelpoppel (issue #225):");

    if !engine.has_variant("hoppelpoppel") {
        println!("  (skipped: this FSF binary does not advertise UCI_Variant hoppelpoppel)");
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
                eprintln!("skip hoppelpoppel/{}: {e}", case.label);
            }
        }
    }

    let nodes: u64 = rows.iter().map(|r| r.mcr_nodes).sum();
    let mcr_s: f64 = rows.iter().map(|r| r.mcr_secs).sum();
    let fsf_s: f64 = rows.iter().map(|r| r.fsf_secs).sum();
    println!("{}", "-".repeat(head.len()));
    if mcr_s > 0.0 && fsf_s > 0.0 {
        println!(
            "hoppelpoppel OVERALL: {nodes} nodes verified; mcr {:.1} Mn/s vs fsf {:.1} Mn/s \
             ({:.2}x).",
            nodes as f64 / mcr_s / 1e6,
            nodes as f64 / fsf_s / 1e6,
            fsf_s / mcr_s,
        );
    }

    if mismatches == 0 {
        println!(
            "OK: all {} Hoppel-Poppel positions matched FSF ({nodes} nodes verified).",
            rows.len(),
        );
    } else {
        eprintln!("ERROR: {mismatches} Hoppel-Poppel parity mismatch(es) vs FSF.");
        for r in rows.iter().filter(|r| !r.matched) {
            eprintln!(
                "  MISMATCH hoppelpoppel/{} depth {}: mcr={} fsf={}  FEN: {}",
                r.label, r.depth, r.mcr_nodes, r.fsf_nodes, r.fen,
            );
        }
    }
    mismatches
}

/// Run one Hoppel-Poppel position through mcr's generic perft and FSF's `go perft`.
fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = HoppelPoppel::from_fen(case.fen).map_err(|e| format!("mcr rejected FEN: {e:?}"))?;
    let mcr_start = Instant::now();
    let mcr_nodes = gperft::<Chess8x8, _>(&pos, depth);
    let mcr_secs = mcr_start.elapsed().as_secs_f64();

    let fsf_fen = to_fsf_dialect(case.fen);
    engine.set_variant("hoppelpoppel", false)?;
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

    /// The corpus FENs all parse on the generic Hoppel-Poppel engine, round-trip
    /// through mcr's FEN I/O, and the pinned shallow counts match the FSF-confirmed
    /// numbers in `tests/perft_hoppelpoppel.rs` (this runs without FSF present).
    #[test]
    fn corpus_fens_parse_and_match_pinned_shallow_counts() {
        let pinned = [
            ("startpos-w", 3u32, 9034u64),
            ("startpos-b", 3, 9034),
            ("midgame-1", 3, 32815),
            ("midgame-2", 3, 89256),
            ("midgame-3", 3, 87234),
            ("tactic-promo", 3, 8732),
        ];
        for case in CASES {
            let pos = HoppelPoppel::from_fen(case.fen).expect("corpus FEN parses");
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

    /// The dialect swap rewrites the Knight-Bishop `*h → n` and Bishop-Knight
    /// `*b → b` (both cases) over the placement field and leaves the structural
    /// fields untouched.
    #[test]
    fn dialect_swap_maps_overflow_pieces() {
        let mcr = "r*h*bqk*b*hr/pppppppp/8/8/8/8/PPPPPPPP/R*H*BQK*B*HR w KQkq - 0 1";
        assert_eq!(
            to_fsf_dialect(mcr),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
        // A castling/queen-bearing position: the bare `q`/`k`/`r` and the `b`/`h`
        // structural-field letters are untouched (no `*` prefix to consume).
        assert_eq!(
            to_fsf_dialect("2kr3r/pp1*h1ppp/2p1p*h2/q7/3P4/2*H*BP*H2/PPQ2PPP/2KR3R w - - 0 1"),
            "2kr3r/pp1n1ppp/2p1pn2/q7/3P4/2NBPN2/PPQ2PPP/2KR3R w - - 0 1"
        );
    }
}
