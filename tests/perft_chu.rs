//! Chu Shogi (12x12) start-position perft and movement cross-checks.
//!
//! ## Validation status (read `variants::chu` module docs for the full picture)
//!
//! The reference oracle for Chu Shogi is **HaChu** (driven by `compare-fairy`).
//! Two things about that oracle constrain what can be machine-checked here:
//!
//! * HaChu has **no native perft**, and the `ddugovic/hachu` build's CECP
//!   `usermove` handler **segfaults** on a cold move (it parses against a legal-move
//!   list that only a prior search populates), so the intended external tree-walk
//!   (play each mce move, read HaChu's legality verdict) is **not runnable** with
//!   this build. Move **counts** below are therefore mce-derived regression values,
//!   not HaChu-confirmed node counts.
//! * HaChu's `w` / `b` debug commands print its **attack map**, which *is* usable
//!   without `usermove`. The start-position White and Black attack maps produced by
//!   mce match HaChu's **exactly** (both colours), and the isolated Horned Falcon /
//!   Soaring Eagle attack maps match HaChu on every ordinary slide/step — differing
//!   only on the lion-power two-step squares, which HaChu deliberately omits from
//!   its static attack table. So the **movement geometry** of the start army is
//!   HaChu-cross-checked; see `compare-fairy` and the module docs.
//!
//! These tests pin the mce move generator against regressions and assert the
//! documented per-piece movement.

use mce::geometry::{perft, Chu, Chu12x12, GenericPosition, Square};

/// The Chu start position round-trips through mce's FEN I/O in the `***`-dialect.
#[test]
fn startpos_round_trips() {
    let pos = Chu::startpos();
    assert_eq!(
        pos.to_fen(),
        "l***l***csg**ekgs***c***ll/***r1b1***t***p***k***t1b1***r/***i***vr+b+r***nq+r+br***v***i/pppppppppppp/3***g4***g3/12/12/3***G4***G3/PPPPPPPPPPPP/***I***VR+B+RQ***N+R+BR***V***I/***R1B1***T***K***P***T1B1***R/L***L***CSGK**EGS***C***LL w - - 0 1"
    );
}

/// Start-position perft. **mce-derived regression values** (not HaChu-confirmed
/// node counts — see the module docs for why the HaChu tree-walk is not runnable).
/// The depth-1 attack geometry underlying these is cross-checked against HaChu's
/// attack map; the counts guard against move-generator regressions.
#[test]
fn startpos_perft_regression() {
    let pos = Chu::startpos();
    assert_eq!(perft::<Chu12x12, _>(&pos, 1), 36);
    assert_eq!(perft::<Chu12x12, _>(&pos, 2), 1296);
    assert_eq!(perft::<Chu12x12, _>(&pos, 3), 48387);
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
