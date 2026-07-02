//! Chu Shogi (12x12) start-position perft and movement cross-checks.
//!
//! ## Validation status (read `variants::chu` module docs for the full picture)
//!
//! The reference oracle for Chu Shogi is **HaChu** (H. G. Muller, driven by
//! `compare-fairy`). HaChu has **no native perft**, but its move generation can be
//! read externally: the `ddugovic/hachu 0.23` build only accepts `usermove` after a
//! `memory N` hash allocation (otherwise it segfaults on the first move), and with
//! `debug=1` it prints its full generated move list for a position when handed an
//! illegal `usermove`. Driving it that way (a fresh subprocess per node, replaying
//! the move sequence, reading the move-list dump) yields an external HaChu perft.
//!
//! Cross-check against that HaChu oracle, from the start position:
//!
//! * **perft(1) = 36** — mce's legal-move set is **byte-identical** to HaChu's
//!   (all 36 coordinate moves match exactly).
//! * **perft(2) = 1296** — **exact** match with HaChu.
//! * **perft(3)**: mce = **47955**, HaChu = **47952**. mce over-generates **3**
//!   nodes at depth 3, localized to the `f3f5` (Lion), `e4e5` and `h4h5` root-move
//!   subtrees (each +1). This is consistent with the documented approximations (the
//!   leaper Lion model and the promote-on-any-move-ending-in-zone rule); it is **not
//!   yet resolved**, so perft(3) is pinned as an mce regression value, honestly
//!   noted as 3 above the HaChu count.
//!
//! These tests pin the mce move generator against regressions and assert the
//! documented per-piece movement.

use mce::geometry::{perft, Chu, Chu12x12, Square};

/// The Chu start position round-trips through mce's FEN I/O in the `***`-dialect.
#[test]
fn startpos_round_trips() {
    let pos = Chu::startpos();
    assert_eq!(
        pos.to_fen(),
        "l***l***csg**ekgs***c***ll/***r1b1***t***p***k***t1b1***r/***i***vr+b+rq***n+r+br***v***i/pppppppppppp/3***g4***g3/12/12/3***G4***G3/PPPPPPPPPPPP/***I***VR+B+R***NQ+R+BR***V***I/***R1B1***T***K***P***T1B1***R/L***L***CSGK**EGS***C***LL w - - 0 1"
    );
}

/// Start-position perft. Depths 1 and 2 are **HaChu-validated** (perft(1) = 36 is a
/// byte-identical move-set match; perft(2) = 1296 matches exactly). perft(3) = 47955
/// is mce's value; the external HaChu tree-walk gives 47952, so mce over-generates 3
/// nodes at depth 3 (see the module docs) — pinned here as a regression guard.
#[test]
fn startpos_perft_regression() {
    let pos = Chu::startpos();
    assert_eq!(perft::<Chu12x12, _>(&pos, 1), 36);
    assert_eq!(perft::<Chu12x12, _>(&pos, 2), 1296);
    assert_eq!(perft::<Chu12x12, _>(&pos, 3), 47955);
}

fn targets(fen: &str, file: u8, rank: u8) -> Vec<u8> {
    let pos = Chu::from_fen(fen).expect("valid Chu FEN");
    let from = Square::<Chu12x12>::from_file_rank(file, rank).expect("on board");
    let mut v: Vec<u8> = pos
        .legal_moves()
        .iter()
        .filter(|m| m.from::<Chu12x12>() == from)
        .map(|m| m.to::<Chu12x12>().index())
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

fn idx(coords: &[(u8, u8)]) -> Vec<u8> {
    let mut v: Vec<u8> = coords
        .iter()
        .map(|&(f, r)| {
            Square::<Chu12x12>::from_file_rank(f, r)
                .expect("on board")
                .index()
        })
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// A lone Dragon King (`+R`) slides orthogonally and steps one square diagonally.
#[test]
fn dragon_king_is_rook_plus_ferz() {
    // Place a Dragon King at f6 (file 5, rank 5) with blockers two squares away so
    // the ray length is observable, on an otherwise empty board.
    let got = targets("k11/12/12/12/12/12/5+R6/12/12/12/12/K11 w - - 0 1", 5, 5);
    let mut want = Vec::new();
    // Rook rays to the board edge (unobstructed).
    for f in 0..12u8 {
        if f != 5 {
            want.push((f, 5));
        }
    }
    for r in 0..12u8 {
        if r != 5 {
            want.push((5, r));
        }
    }
    // Ferz one-steps.
    want.extend_from_slice(&[(4, 4), (6, 4), (4, 6), (6, 6)]);
    assert_eq!(got, idx(&want));
}

/// A Phoenix jumps to the second diagonal square and steps one square orthogonally.
#[test]
fn phoenix_jumps_and_steps() {
    let got = targets("k11/12/12/12/12/12/5***P6/12/12/12/12/K11 w - - 0 1", 5, 5);
    let want = idx(&[
        // Diagonal two-square jumps.
        (7, 7),
        (3, 7),
        (7, 3),
        (3, 3),
        // Orthogonal one-steps.
        (5, 6),
        (5, 4),
        (6, 5),
        (4, 5),
    ]);
    assert_eq!(got, want);
}

/// A promoted Bishop is a Dragon Horse (`+B`): diagonal slide + orthogonal step.
#[test]
fn dragon_horse_is_bishop_plus_wazir() {
    // Kings on the b-file (off every ray/step from f6) so the diagonals are clear.
    let got = targets("1k10/12/12/12/12/12/5+B6/12/12/12/12/1K10 w - - 0 1", 5, 5);
    let mut want = Vec::new();
    // Bishop rays.
    for d in 1..12i8 {
        for (df, dr) in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let (f, r) = (5 + df * d, 5 + dr * d);
            if (0..12).contains(&f) && (0..12).contains(&r) {
                want.push((f as u8, r as u8));
            }
        }
    }
    // Wazir one-steps.
    want.extend_from_slice(&[(5, 6), (5, 4), (6, 5), (4, 5)]);
    assert_eq!(got, idx(&want));
}
