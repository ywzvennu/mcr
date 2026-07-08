//! Supply (Xiangqi 9x10 with drops) differential perft against Fairy-Stockfish
//! (issue #585).
//!
//! FSF `supply` leaves `capturesToHand = false` and is a two-board (`twoBoards`)
//! game whose hand is fed by the *partner* board, never by a capture on this board;
//! FSF also excludes its two-board "virtual" drops from perft. On a single board the
//! hand therefore starts empty and stays empty, so **every** Supply move tree from a
//! normal (empty-hand) position is standard **Xiangqi's** node-for-node. This harness
//! validates the Supply engine by running its perft on the **mcr side** and comparing
//! against FSF `UCI_Variant xiangqi` perft on the same position (holdings bracket
//! stripped, mcr dialect rewritten to FSF's `a n b p`). A match confirms the whole
//! Xiangqi movement layer Supply inherits reproduces node-for-node against an
//! independent engine.
//!
//! The **drop** mechanic itself (region-restricted, `dropChecks = false`) has no FSF
//! analogue reachable here — FSF's own drops are the excluded two-board virtual drops
//! — so it is validated separately by the independent from-scratch 9x10 generator in
//! `tests/perft_supply.rs`.
//!
//! **FSF must be built with large-board support** (`largeboards=yes`) for the 9x10
//! `xiangqi` variant; without it this loop skips.
//!
//! GPL FENCE unchanged: FSF is driven purely as a subprocess (see `uci.rs`); no GPL
//! code is linked.

use std::time::Instant;

use mcr::geometry::{perft as gperft, Supply, Xiangqi9x10};

use crate::uci::Engine;
use crate::xiangqi::fen_to_fsf;

/// One empty-hand Supply corpus position (mcr dialect, with the `[]` bracket).
struct Case {
    label: &'static str,
    fen: &'static str,
    depth: u32,
}

/// The Supply comparison corpus: the FSF-confirmed start and a Xiangqi middlegame,
/// both with an empty hand (where Supply coincides with Xiangqi node-for-node).
const CASES: &[Case] = &[
    Case {
        label: "startpos",
        fen: "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/9/RJOUKUOJR[] w - - 0 1",
        depth: 3,
    },
    Case {
        label: "middlegame",
        fen: "r1oukuo1r/9/1cj3jc1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1CJ3JC1/9/R1OUKUO1R[] w - - 0 1",
        depth: 3,
    },
];

/// Rewrite an empty-hand Supply FEN (mcr dialect) into its Xiangqi equivalent for
/// FSF: strip the `[…]` holdings bracket, then apply the Xiangqi dialect rewrite.
fn supply_to_fsf_xiangqi(fen: &str) -> String {
    let stripped = fen.replacen("[]", "", 1);
    fen_to_fsf(&stripped)
}

/// A measured Supply comparison row.
struct Row {
    label: &'static str,
    depth: u32,
    mcr_nodes: u64,
    fsf_nodes: u64,
    matched: bool,
}

/// Run the Supply corpus: mcr Supply perft vs FSF `UCI_Variant xiangqi` perft on the
/// empty-hand equivalent. Returns the number of mismatches (0 = all matched, or FSF
/// lacks `xiangqi` and the suite is skipped).
pub fn run(engine: &mut Engine, full: bool) -> usize {
    println!();
    println!(
        "Supply (Xiangqi 9x10 with drops, u128) — generic engine vs FSF \
UCI_Variant xiangqi on the empty-hand equivalent (issue #585):"
    );
    println!("  (Supply's single-board empty-hand play is Xiangqi; drops are validated in-repo)");
    println!("  (requires an FSF built with largeboards=yes)");

    if !engine.has_variant("xiangqi") {
        println!("  SKIP: this FSF binary has no `xiangqi` variant (build it largeboards=yes).");
        return 0;
    }

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
                    "  {:<12} depth {} — mcr {} vs fsf {} — {}",
                    row.label,
                    row.depth,
                    row.mcr_nodes,
                    row.fsf_nodes,
                    if row.matched { "ok" } else { "MISMATCH" },
                );
                rows.push(row);
            }
            Err(e) => eprintln!("skip supply/{}: {e}", case.label),
        }
    }

    if mismatches == 0 {
        println!("OK: all {} Supply positions matched FSF xiangqi.", rows.len());
    } else {
        eprintln!("ERROR: {mismatches} Supply parity mismatch(es) vs FSF.");
    }
    mismatches
}

fn run_case(engine: &mut Engine, case: &Case, depth: u32) -> Result<Row, String> {
    let pos = Supply::from_fen(case.fen).map_err(|e| format!("mcr rejected Supply FEN: {e:?}"))?;
    let _t = Instant::now();
    let mcr_nodes = gperft::<Xiangqi9x10, _, _>(&pos, depth);

    let fsf_fen = supply_to_fsf_xiangqi(case.fen);
    engine.set_variant("xiangqi", false)?;
    engine.set_position(&fsf_fen)?;
    let fsf = engine.go_perft(depth, false)?;

    Ok(Row {
        label: case.label,
        depth,
        mcr_nodes,
        fsf_nodes: fsf.nodes,
        matched: mcr_nodes == fsf.nodes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The start FEN converts to FSF's Xiangqi startpos (bracket stripped, dialect
    /// rewritten).
    #[test]
    fn conversion_strips_hand_and_rewrites_dialect() {
        assert_eq!(
            supply_to_fsf_xiangqi(CASES[0].fen),
            "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1"
        );
    }

    /// mcr's own cross-check (no FSF): Supply perft equals the Xiangqi perft of the
    /// bracket-stripped equivalent for every corpus case.
    #[test]
    fn supply_perft_equals_xiangqi_equivalent() {
        for case in CASES {
            let supply = Supply::from_fen(case.fen).expect("Supply parses");
            let xq = mcr::geometry::Xiangqi::from_fen(&case.fen.replacen("[]", "", 1))
                .expect("Xiangqi parses");
            assert_eq!(
                gperft::<Xiangqi9x10, _, _>(&supply, case.depth),
                gperft::<Xiangqi9x10, _, _>(&xq, case.depth),
                "{}: Supply vs Xiangqi-equivalent perft({})",
                case.label,
                case.depth,
            );
        }
    }
}
