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
//! Cross-check against that HaChu oracle, from the start position (with the Lion's
//! full move set — igui, double capture, jitto pass — and Chu's exact
//! promote-on-entry rule both modelled, issue #400):
//!
//! * **perft(1) = 36** — mce's legal-move set is **byte-identical** to HaChu's.
//! * **perft(2) = 1296** — **exact** match with HaChu (verified node-for-node).
//! * **perft(3)**: mce = **48319**, HaChu = **48317**. The two differ at exactly
//!   **one** node — after `1. f3f5 d8d7`, where a Black Go-Between sits at
//!   *anti-diagonal* distance two (d7) from the White Lion on f5. mce generates the
//!   two legal captures of it (the jump and the two-step area move); HaChu 0.23
//!   generates **neither**. This is a **HaChu bug**: its Lion captures a
//!   distance-two enemy on the a1–l12 diagonal (and its reflection) but not on the
//!   opposite diagonal — demonstrated in isolation via `setboard` (a lone Lion
//!   captures an enemy at `+2,+2` / `-2,-2` but not at `-2,+2` / `+2,-2`). mce is
//!   correct; every *other* node of the depth-3 tree matches HaChu exactly.
//! * **perft(4) = 1802285** is an mce regression pin only: a node-by-node HaChu
//!   cross-check at depth 4 (~1.8M nodes, one subprocess per node) is intractable,
//!   so it is not oracle-validated.
//!
//! HaChu does **not** enforce the Chu lion-trading restrictions in its move
//! generation (its `setboard` dumps let a Lion capture a *protected* enemy Lion),
//! so — matching the oracle — mce does not either.
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

/// Start-position perft. Depths 1 and 2 are **HaChu-validated** node-for-node
/// (perft(1) = 36 is a byte-identical move-set match; perft(2) = 1296 matches
/// exactly). perft(3) = 48319 matches HaChu at every node **except one**, where
/// HaChu 0.23 misses two legal anti-diagonal Lion captures (a HaChu bug; mce is
/// correct — see the module docs), so HaChu's tree gives 48317. perft(4) is an
/// mce-only regression pin (depth-4 HaChu cross-check is intractable).
#[test]
fn startpos_perft_regression() {
    let pos = Chu::startpos();
    assert_eq!(perft::<Chu12x12, _>(&pos, 1), 36);
    assert_eq!(perft::<Chu12x12, _>(&pos, 2), 1296);
    assert_eq!(perft::<Chu12x12, _>(&pos, 3), 48319);
    assert_eq!(perft::<Chu12x12, _>(&pos, 4), 1802285);
}

/// The UCI strings of every legal move from `from_sq` in `fen`.
fn ucis_from(fen: &str, from_sq: &str) -> Vec<String> {
    let pos = Chu::from_fen(fen).expect("valid Chu FEN");
    let f = from_sq.as_bytes()[0] - b'a';
    let r: u8 = from_sq[1..].parse::<u8>().unwrap() - 1;
    let from = Square::<Chu12x12>::from_file_rank(f, r).expect("on board");
    let mut v: Vec<String> = pos
        .legal_moves()
        .iter()
        .filter(|m| m.from::<Chu12x12>() == from)
        .map(|m| m.to_uci::<Chu12x12>())
        .collect();
    v.sort();
    v
}

/// The Lion's **jitto pass**: with an empty adjacent square it may step there and
/// back, a net-zero `from == to` move that only passes the turn.
#[test]
fn lion_jitto_pass() {
    let ucis = ucis_from("k11/12/12/12/12/12/5***N6/12/12/12/12/K11 w - - 0 1", "f6");
    assert!(ucis.contains(&"f6f6".to_string()), "lion should have a pass: {ucis:?}");
}

/// The Lion's **igui**: it captures an adjacent enemy (here on g6) and stays on its
/// own square, encoded `from == to` with the captured square as the intermediate.
#[test]
fn lion_igui_stationary_capture() {
    let ucis = ucis_from("k11/12/12/12/12/12/5***Np5/12/12/12/12/K11 w - - 0 1", "f6");
    assert!(ucis.contains(&"f6f6*g6".to_string()), "lion should have an igui on g6: {ucis:?}");
    // The ordinary step-capture onto g6 is also available (a distinct move).
    assert!(ucis.contains(&"f6g6".to_string()));
}

/// The Lion's **double capture**: two enemies in a line (g6 then h6) are both taken
/// in one turn, the intermediate g6 riding in the move's addendum.
#[test]
fn lion_double_capture() {
    let ucis = ucis_from("k11/12/12/12/12/12/5***Npp4/12/12/12/12/K11 w - - 0 1", "f6");
    assert!(ucis.contains(&"f6h6*g6".to_string()), "lion should double-capture g6+h6: {ucis:?}");
    // A redundant elbow path to the *adjacent* g6 is NOT generated (matches HaChu):
    // exactly one plain move lands on g6.
    assert_eq!(ucis.iter().filter(|m| m.as_str() == "f6g6").count(), 1);
}

/// The Lion captures a distance-two enemy on **either** diagonal — the jump and the
/// two-step area move. (HaChu 0.23 has a bug here: it misses the anti-diagonal
/// capture; mce is correct and symmetric.)
#[test]
fn lion_captures_both_diagonals_at_distance_two() {
    // Anti-diagonal (-2,+2): enemy on d8 relative to the Lion on f6.
    let anti = ucis_from("k11/12/12/12/3p8/12/5***N6/12/12/12/12/K11 w - - 0 1", "f6");
    assert_eq!(
        anti.iter().filter(|m| m.starts_with("f6d8")).count(),
        2,
        "expected jump + area capture of d8: {anti:?}"
    );
    // Main diagonal (+2,+2): enemy on h8.
    let main = ucis_from("k11/12/12/12/7p4/12/5***N6/12/12/12/12/K11 w - - 0 1", "f6");
    assert_eq!(main.iter().filter(|m| m.starts_with("f6h8")).count(), 2);
}

/// Chu promotion (HaChu's "promote on entry"): promotion is **mandatory** on
/// entering the zone (no non-promoting alternative), and never offered on a move
/// that stays within or leaves the zone.
#[test]
fn chu_promotion_forced_on_entry_only() {
    // Rook on f7 (outside the rank-9..12 zone) sliding up: every move that *enters*
    // the zone promotes, with no plain alternative.
    let enter = ucis_from("k11/12/12/12/12/5R6/12/12/12/12/12/K11 w - - 0 1", "f7");
    assert!(enter.iter().any(|m| m == "f7f9r"), "entry must promote: {enter:?}");
    assert!(!enter.contains(&"f7f9".to_string()), "no non-promoting entry: {enter:?}");
    // Rook already inside the zone (f10) moving within it does not promote.
    let within = ucis_from("k11/12/5R6/12/12/12/12/12/12/12/12/K11 w - - 0 1", "f10");
    assert!(within.contains(&"f10f11".to_string()));
    assert!(!within.contains(&"f10f11r".to_string()), "no promotion within zone: {within:?}");
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
