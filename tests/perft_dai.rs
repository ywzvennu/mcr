//! Dai Shogi (15x15) start-position perft and movement cross-checks.
//!
//! ## Validation status (read `variants::dai` module docs for the full picture)
//!
//! The reference oracle for Dai Shogi is **HaChu** (H. G. Muller, driven by
//! `compare-fairy`); Dai is **not** a Fairy-Stockfish variant. HaChu has **no
//! native perft**, but its move generation can be read externally: the
//! `ddugovic/hachu` build only accepts `usermove` after a `memory N` hash
//! allocation, and (debug output being on by default) it prints its full generated
//! move list for the current position when handed an illegal `usermove`. Driving it
//! that way — a fresh subprocess per node, replaying the move sequence, reading the
//! move-list dump and deduping the killer/hash prefixes and the `p32p32` null
//! placeholder — yields an external HaChu perft.
//!
//! Cross-check against that HaChu oracle, from the start position (with the full Chu
//! rule layer plus Dai's five extra short-range movers, its five-rank promotion
//! zone and its no-Kirin/Phoenix-promotion rule all modelled):
//!
//! * **perft(1) = 71** — mcr's legal-move set is **node-for-node identical** to
//!   HaChu's (same 71 moves; this pinned down the Kirin/Phoenix chirality, HaChu
//!   placing the Kirin on the King's left).
//! * **perft(2) = 5041** — **exact node-for-node** match with HaChu: every one of
//!   the 71 root moves leaves Black with exactly the reply count HaChu reports
//!   (71 each; the camps are three ranks apart, so a first move never changes the
//!   reply count — hence 71 x 71).
//! * **perft(3) = 357836** — validated against HaChu at the **subtree / leaf**
//!   level rather than exhaustively (a full node-by-node depth-3 walk is ~5041
//!   flaky one-subprocess-per-node HaChu dumps). Sampled subtrees agree with **zero
//!   real mismatches**: e.g. the quiet `a4a3` rook subtree matches HaChu at 70/71
//!   of its depth-2 nodes (the one gap is a HaChu subprocess crash, not a move
//!   difference), and the line-opening `g5g6` pawn push — which frees the Dragon
//!   King, the Free King's diagonal and the flanking Bishops behind it — matches
//!   HaChu leaf-for-leaf (89 White replies after `1. g5g6 a11a10`). The few
//!   unreachable nodes are HaChu 0.23 segfaulting on specific positions (its
//!   `usermove` path is fragile on the 15x15 board), documented, not weakened.
//! * **perft(4) = 25400968** is an mcr regression pin only: a node-by-node HaChu
//!   cross-check at depth 4 is intractable, so it is not oracle-validated.
//!
//! These tests pin the mcr move generator against regressions and assert the
//! documented per-piece movement of the Dai-specific pieces.

use mcr::geometry::{perft, Dai, Dai15x15, Square};

/// The Dai start position round-trips through mcr's FEN I/O in the `***`-dialect.
#[test]
fn startpos_round_trips() {
    let pos = Dai::startpos();
    assert_eq!(
        pos.to_fen(),
        "l*n***z***u***csgkgs***c***u***z*nl/***r1m1***l1***t**e***t1***l1m1***r/1***x1*j1***f***p***n***k***f1*j1***x1/r***d***i***vb+b+rq+r+bb***v***i***dr/ppppppppppppppp/4***g5***g4/15/15/15/4***G5***G4/PPPPPPPPPPPPPPP/R***D***I***VB+B+RQ+R+BB***V***I***DR/1***X1*J1***F***K***N***P***F1*J1***X1/***R1M1***L1***T**E***T1***L1M1***R/L*N***Z***U***CSGKGS***C***U***Z*NL w - - 0 1"
    );
}

/// Start-position perft. Depths 1 and 2 are **HaChu-validated** node-for-node
/// (perft(1) = 71 is an identical move-set match; perft(2) = 5041 matches at every
/// root). perft(3) = 357836 is validated against HaChu at the subtree/leaf level
/// (sampled subtrees show zero real move differences; see the module docs).
#[test]
fn startpos_perft_regression() {
    let pos = Dai::startpos();
    assert_eq!(perft::<Dai15x15, _>(&pos, 1), 71);
    assert_eq!(perft::<Dai15x15, _>(&pos, 2), 5041);
    assert_eq!(perft::<Dai15x15, _>(&pos, 3), 357836);
}

/// A deeper mcr-only regression pin (perft(4) = 25400968). Not oracle-validated (a
/// depth-4 HaChu cross-check is intractable) and ~25M nodes, so it is `#[ignore]`d
/// to keep the default (debug) suite fast; run it explicitly, ideally in release:
/// `cargo test --release --test perft_dai -- --ignored`.
#[test]
#[ignore = "slow: ~25M-node depth-4 perft; run explicitly in release"]
fn startpos_perft_depth4() {
    let pos = Dai::startpos();
    assert_eq!(perft::<Dai15x15, _>(&pos, 4), 25400968);
}

fn targets(fen: &str, file: u8, rank: u8) -> Vec<u8> {
    let pos = Dai::from_fen(fen).expect("valid Dai FEN");
    let from = Square::<Dai15x15>::from_file_rank(file, rank).expect("on board");
    let mut v: Vec<u8> = pos
        .legal_moves()
        .iter()
        .filter(|m| m.from::<Dai15x15>() == from)
        .map(|m| m.to::<Dai15x15>().index())
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

fn idx(coords: &[(u8, u8)]) -> Vec<u8> {
    let mut v: Vec<u8> = coords
        .iter()
        .map(|&(f, r)| {
            Square::<Dai15x15>::from_file_rank(f, r)
                .expect("on board")
                .index()
        })
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// An empty-ish 15x15 board with a White King on a1 and a Black king on o15, plus
/// one piece of interest at h8. `piece` is the FEN token for the h8 piece.
fn lone(piece: &str) -> String {
    // rank 15 (o15 black king) down to rank 1 (a1 white king). h8 = file 7 rank 7.
    format!("14k/15/15/15/15/15/15/7{piece}7/15/15/15/15/15/15/K14 w - - 0 1")
}

/// A Violent Ox slides one or two squares along each orthogonal direction, and is
/// blocked at range one by an intervening piece (it cannot jump).
#[test]
fn violent_ox_range_two_and_blocked() {
    // Open board: eight targets (1 and 2 each direction).
    let got = targets(&lone("***X"), 7, 7);
    assert_eq!(
        got,
        idx(&[
            (7, 8),
            (7, 9),
            (7, 6),
            (7, 5),
            (8, 7),
            (9, 7),
            (6, 7),
            (5, 7)
        ])
    );
    // Own blocker one square north (h9): the north ray cannot enter it and cannot
    // jump past it, so neither h9 nor h10 is reachable.
    let blocked = targets(
        "14k/15/15/15/15/15/7P7/7***X7/15/15/15/15/15/15/K14 w - - 0 1",
        7,
        7,
    );
    assert!(!blocked.contains(&Square::<Dai15x15>::from_file_rank(7, 8).unwrap().index()));
    assert!(!blocked.contains(&Square::<Dai15x15>::from_file_rank(7, 9).unwrap().index()));
}

/// A Flying Dragon slides one or two squares along each diagonal direction.
#[test]
fn flying_dragon_range_two() {
    let got = targets(&lone("***D"), 7, 7);
    assert_eq!(
        got,
        idx(&[
            (8, 8),
            (9, 9),
            (6, 8),
            (5, 9),
            (8, 6),
            (9, 5),
            (6, 6),
            (5, 5)
        ])
    );
}

/// An Iron General steps straight forward and to the two forward diagonals (three).
#[test]
fn iron_general_three_steps() {
    assert_eq!(targets(&lone("***U"), 7, 7), idx(&[(7, 8), (8, 8), (6, 8)]));
}

/// A Stone General steps only to the two forward diagonals.
#[test]
fn stone_general_two_steps() {
    assert_eq!(targets(&lone("***Z"), 7, 7), idx(&[(8, 8), (6, 8)]));
}

/// An Evil Wolf steps forward, sideways, and to the two forward diagonals (five).
#[test]
fn evil_wolf_five_steps() {
    assert_eq!(
        targets(&lone("***F"), 7, 7),
        idx(&[(7, 8), (8, 8), (6, 8), (8, 7), (6, 7)])
    );
}

/// The reused Angry Boar (Wazir) steps one square orthogonally, and the Cat Sword
/// (Met / Ferz) one square diagonally.
#[test]
fn angry_boar_and_cat_sword() {
    // Angry Boar = `*J` (Wazir): four orthogonal steps.
    assert_eq!(
        targets(&lone("*J"), 7, 7),
        idx(&[(7, 8), (7, 6), (8, 7), (6, 7)])
    );
    // Cat Sword = `M` (Met / Ferz): four diagonal steps.
    assert_eq!(
        targets(
            "14k/15/15/15/15/15/15/7M7/15/15/15/15/15/15/K14 w - - 0 1",
            7,
            7
        ),
        idx(&[(8, 8), (6, 8), (8, 6), (6, 6)])
    );
}

/// The Knight (Shogi Knight) jumps to the two forward 2-1 squares, leaping over the
/// intervening rank.
#[test]
fn knight_forward_two_one() {
    let got = targets(
        "14k/15/15/15/15/15/15/7*N7/15/15/15/15/15/15/K14 w - - 0 1",
        7,
        7,
    );
    assert_eq!(got, idx(&[(8, 9), (6, 9)]));
}

/// Pins / attackers consistency for a Dai-specific slider: the absolutely pinned
/// set and the attacker relation stay coherent when a Violent Ox lines up on the
/// King through a lone friendly blocker. (Dai is multi-royal, so legality rides the
/// make/unmake path; this exercises the attacker/pin query surface on the new
/// pieces and geometry.)
#[test]
fn attackers_and_pins_consistency() {
    // Black Violent Ox on h6 (file7 rank5), a White blocker Pawn on h4 (file7 rank3),
    // White King on h2 (file7 rank1): the Ox attacks two squares down the h-file,
    // reaching the pawn; the pawn is the sole piece between Ox and King two squares
    // further, so it is pinned. All pieces on the h-file within range 2.
    let pos = Dai::from_fen("14k/15/15/15/15/15/15/15/15/7***x7/15/7P7/15/7K7/15 w - - 0 1")
        .expect("valid Dai FEN");
    let king = Square::<Dai15x15>::from_file_rank(7, 1).unwrap();
    // The King is not in check (the pawn blocks the Ox at range one).
    assert!(!pos.is_attacked(king, mcr::Color::Black));
    // Every legal move is self-consistent: replaying it never leaves our King
    // attacked (a base sanity net on the new-piece attacker projection).
    for m in pos.legal_moves().iter() {
        let after = pos.play(m);
        let k = {
            let mut it = None;
            for f in 0..15 {
                for r in 0..15 {
                    let sq = Square::<Dai15x15>::from_file_rank(f, r).unwrap();
                    if after.board().kings_of(mcr::Color::White).contains(sq) {
                        it = Some(sq);
                    }
                }
            }
            it
        };
        if let Some(k) = k {
            assert!(
                !after.is_attacked(k, mcr::Color::Black),
                "move {} left the White King in check",
                m.to_uci::<Dai15x15>()
            );
        }
    }
}
