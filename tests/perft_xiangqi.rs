//! Xiangqi (Chinese chess, 9x10 / `u128`) perft validation on the generic engine
//! (issue #187) — the first marquee fairy variant: a new 9x10 geometry, the
//! palace / river / blockable-leg machinery, the reused **cannon** primitive, and
//! the **flying-general** king-safety rule.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Xiangqi::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant xiangqi`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position. The `compare-fairy/` harness re-runs that head-to-head
//! on demand (`compare-fairy/src/xiangqi.rs`); this test pins the FSF-confirmed
//! numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `xiangqi_variant()` (`startFen`):
//!
//! ```text
//! FSF dialect: rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1
//! mcr dialect: rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/9/RJOUKUOJR w - - 0 1
//! ```
//!
//! The two describe the same position; mcr spells four pieces differently because
//! the FSF letters `a n b p` already name the Hawk / Knight / Bishop / Pawn in
//! mcr's [`WideRole`](mcr::geometry::WideRole), so the Advisor is `u`, the Horse
//! `j`, the Elephant `o`, and the Soldier `z`. The chariots (`r`) and cannons
//! (`c`) match. The cannons sit on the third rank (b3/h3, b8/h8), the soldiers on
//! ranks 4 / 7.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_xiangqi -- --include-ignored`.

use mcr::geometry::{perft as gperft, Xiangqi, Xiangqi9x10};

/// The Xiangqi starting FEN (mcr dialect), confirmed against Fairy-Stockfish's
/// `UCI_Variant xiangqi`.
const STARTPOS: &str = "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/9/RJOUKUOJR w - - 0 1";

/// A developed middlegame, white to move: both sides have advanced their horses
/// and cannons off the back rank (the b/h-file horses out, the cannons centred),
/// so the position exercises the **horse hobbling-leg**, the **cannon** quiet
/// rays and over-screen captures, and many ordinary captures at depth.
/// FSF-confirmed move-for-move (mcr dialect of FSF's
/// `r1bakab1r/9/1cn3nc1/p1p1p1p1p/9/9/P1P1P1P1P/1CN3NC1/9/R1BAKAB1R`).
const MID: &str = "r1oukuo1r/9/1cj3jc1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1CJ3JC1/9/R1OUKUO1R w - - 0 1";

/// A **cannon king-capture** position, black to move: a black cannon on e8 sits
/// over a white-cannon screen on e7 and may capture the white general on e1 down
/// the e-file (Fairy-Stockfish generates the general capture as a perft move, and
/// the kingless side then enumerates its pseudo-legal continuations). Exercises
/// the screen-dependent cannon capture and the kingless-side move enumeration.
/// FSF-confirmed (mcr dialect of FSF's
/// `rnbakabnr/9/4c4/p1p1C1p1p/9/9/P1P3P1P/1C5c1/9/RNBAKABNR b`).
const CANNON_CAP: &str = "rjoukuojr/9/4c4/z1z1C1z1z/9/9/Z1Z3Z1Z/1C5c1/9/RJOUKUOJR b - - 0 1";

/// An **elephant-eye / soldier-clash** middlegame, white to move: the central
/// soldiers face off across the river (c4/g4 vs c5/g5 in the mcr dialect) and the
/// elephants' two-diagonal jumps are partly eye-blocked, so the elephant
/// blockable-leap and the river-crossing soldier rule are both exercised.
/// FSF-confirmed (mcr dialect of FSF's
/// `r1bakab1r/9/1c2c4/p3p3p/2p3p2/2P3P2/P3P3P/1C2C4/9/R1BAKAB1R`).
const EYE: &str = "r1oukuo1r/9/1c2c4/z3z3z/2z3z2/2Z3Z2/Z3Z3Z/1C2C4/9/R1OUKUO1R w - - 0 1";

/// A **flying-general pin**, white to move: the two generals stand on the e-file
/// (e1 / e10) with a single white chariot between them on e2. The chariot may not
/// leave the e-file — doing so would leave the generals facing, which is illegal —
/// so it is pinned exactly as by a slider, purely by the flying-general rule.
/// FSF-confirmed (`4k4/9/9/9/9/9/9/9/4R4/4K4 w` — no dialect rewrite needed, it
/// has only chariots and generals).
const FLYING_GENERAL: &str = "4k4/9/9/9/9/9/9/9/4R4/4K4 w - - 0 1";

/// A **horse-gives-check** position, white to move and **in check** (issue #198):
/// a black horse on e3 leaps onto the white general on d1, its hobbling leg e2
/// being empty. The old corpus never exercised a horse check, so `attackers_to`'s
/// reverse-projection bug (it tested the leg adjacent to the *target*, d2, which is
/// occupied by the white advisor, instead of the leg adjacent to the *horse*, e2)
/// went undetected: mcr wrongly read `is_attacked(d1, Black) == false` and let
/// non-evasions through, inflating perft (depth 3 was 17, not 14). White's only
/// replies are the king step d1->e1 and the advisor capture d2xe3.
/// FSF-confirmed (mcr dialect of FSF's `4k4/9/9/9/9/9/9/4n4/3A5/3K5 w`).
const HORSE_CHECK: &str = "4k4/9/9/9/9/9/9/4j4/3U5/3K5 w - - 0 1";

/// A **soldier-guards-the-square-ahead** position, black to move (issue #201):
/// a white soldier on e7 guards e8, and the black general on d8 may step only to
/// d9 — the palace square e8 is illegal because it is attacked by the soldier.
/// The old corpus never had an enemy king try to step in front of a soldier, so
/// `attackers_to`'s reverse-projection bug (the Soldier's forward attack is
/// color-directional, but the default `role_attack_is_directional` covers only
/// Pawn/Hoplite) went undetected: mcr wrongly read `is_attacked(e8, White) ==
/// false` and let the general step into capture, inflating perft (depth 1 was 2,
/// not 1). FSF-confirmed (`9/9/3k5/4P4/9/9/9/9/9/4K4 b` — only general + soldier,
/// soldier rewritten `P`->`Z` for the mcr dialect).
const SOLDIER_GUARD_FWD: &str = "9/9/3k5/4Z4/9/9/9/9/9/4K4 b - - 0 1";

/// A **crossed-soldier-guards-sideways** position, black to move (issue #201):
/// a white soldier on e8 has crossed the river, so besides e9 (forward) it also
/// guards d8 and f8 (sideways). The black general on d9 may step only to d10 —
/// both e9 (forward-guarded) and d8 (sideways-guarded) are illegal. This is the
/// post-river case a plain color-flipped reverse-projection cannot catch (the
/// river threshold is color-dependent, so flipping the color flips the crossing
/// state and misses the crossed soldier); forward projection from the soldier via
/// `role_attack_is_leg_asymmetric` is required. Without the fix mcr let the
/// general step to d8, inflating perft (depth 1 was 2, not 1). FSF-confirmed
/// (`9/3k5/4P4/9/9/9/9/9/9/4K4 b`, soldier `P`->`Z`).
const SOLDIER_GUARD_SIDE: &str = "9/3k5/4Z4/9/9/9/9/9/9/4K4 b - - 0 1";

/// Asserts the generic Xiangqi perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF xiangqi `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Xiangqi::from_fen(fen).expect("valid Xiangqi FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Xiangqi9x10, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Xiangqi perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed; the well-known Xiangqi perft sequence) ---

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 44), (2, 1920), (3, 79666)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    // FSF xiangqi `go perft` on the startpos — the canonical published counts.
    check(STARTPOS, &[(4, 3290240), (5, 133312995)]);
}

// -- Horse hobbling + cannon middlegame (FSF-confirmed) ----------------------

#[test]
fn middlegame_cheap() {
    check(MID, &[(1, 36), (2, 1292), (3, 47994)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn middlegame_deep() {
    check(MID, &[(4, 1777662), (5, 67407683)]);
}

// -- Cannon over-screen general capture + kingless enumeration (FSF) ---------

#[test]
fn cannon_capture_cheap() {
    check(CANNON_CAP, &[(1, 11), (2, 373), (3, 13581)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn cannon_capture_deep() {
    check(CANNON_CAP, &[(4, 518407), (5, 18737285)]);
}

// -- Elephant-eye blocks + soldier river crossing (FSF-confirmed) ------------

#[test]
fn elephant_eye_cheap() {
    check(EYE, &[(1, 33), (2, 1066), (3, 36542)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn elephant_eye_deep() {
    check(EYE, &[(4, 1259434)]);
}

// -- Flying-general pin (FSF-confirmed) --------------------------------------

#[test]
fn flying_general_pin() {
    check(
        FLYING_GENERAL,
        &[(1, 10), (2, 16), (3, 290), (4, 262), (5, 4734)],
    );
}

// -- Horse gives check (FSF-confirmed; the corpus blind spot of #198) ---------

#[test]
fn horse_check_cheap() {
    // Depth 3 was 17 before the #198 fix (missed horse check); FSF says 14.
    check(HORSE_CHECK, &[(1, 2), (2, 5), (3, 14)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn horse_check_deep() {
    check(HORSE_CHECK, &[(4, 50), (5, 175), (6, 786)]);
}

// -- Soldier guards the square ahead (FSF-confirmed; the blind spot of #201) ---

#[test]
fn soldier_guard_forward_cheap() {
    // Depth 1 was 2 before the #201 fix (missed the soldier guarding e8); FSF
    // says 1 — the black general may step only to d9.
    check(SOLDIER_GUARD_FWD, &[(1, 1), (2, 5), (3, 10)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn soldier_guard_forward_deep() {
    check(SOLDIER_GUARD_FWD, &[(4, 53), (5, 88)]);
}

// -- Crossed soldier guards sideways too (FSF-confirmed; post-river case) ------

#[test]
fn soldier_guard_sideways_cheap() {
    // Depth 1 was 2 before the #201 fix (missed the crossed soldier guarding d8
    // sideways); FSF says 1 — the black general may step only to d10.
    check(SOLDIER_GUARD_SIDE, &[(1, 1), (2, 5), (3, 5)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn soldier_guard_sideways_deep() {
    check(SOLDIER_GUARD_SIDE, &[(4, 26)]);
}
