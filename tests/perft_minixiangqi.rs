//! Minixiangqi (7x7 / `u128`) perft validation on the generic engine (issue
//! #196) — a compact reduction of Xiangqi (#187): a new 7x7 (49-square) geometry
//! reusing the palace / blockable-leg / cannon / flying-general machinery, with no
//! river, advisors, or elephants.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Minixiangqi::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant minixiangqi`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position. The `compare-fairy/` harness re-runs that head-to-head
//! on demand (`compare-fairy/src/minixiangqi.rs`); this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `minixiangqi` variant (`startFen`):
//!
//! ```text
//! FSF dialect: rcnkncr/p1ppp1p/7/7/7/P1PPP1P/RCNKNCR w - - 0 1
//! mcr dialect: rcjkjcr/z1zzz1z/7/7/7/Z1ZZZ1Z/RCJKJCR w - - 0 1
//! ```
//!
//! The two describe the same position; mcr spells the Horse `j` and the Soldier
//! `z` because the FSF letters `n p` already name the Knight / Pawn in mcr's
//! [`WideRole`](mcr::geometry::WideRole). The chariots (`r`), cannons (`c`), and
//! king (`k`) match. The cannons sit on the b/f files of rank 2, the soldiers on
//! ranks 2 / 6 (files a, c, d, e, g).
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_minixiangqi -- --include-ignored`.

use mcr::geometry::{perft as gperft, Minixiangqi, Minixiangqi7x7};

/// The Minixiangqi starting FEN (mcr dialect), confirmed against
/// Fairy-Stockfish's `UCI_Variant minixiangqi`.
const STARTPOS: &str = "rcjkjcr/z1zzz1z/7/7/7/Z1ZZZ1Z/RCJKJCR w - - 0 1";

/// A developed **horse / cannon middlegame**, white to move: both sides have a
/// horse and a cannon advanced off the back rank, so the position exercises the
/// **horse hobbling-leg**, the **cannon** quiet rays and over-screen captures, and
/// many ordinary captures at depth. FSF-confirmed move-for-move (mcr dialect of
/// FSF's `r1nkncr/p1ppp1p/2c4/2N4/7/P1PPP1P/R1CKNCR w`).
const HORSE_CANNON_MID: &str = "r1jkjcr/z1zzz1z/2c4/2J4/7/Z1ZZZ1Z/R1CKJCR w - - 0 1";

/// A **cannon over-screen capture** position, black to move: a white cannon on c4
/// sees the black cannon on c6 over the soldier screen on c5, and the cannon quiet
/// rays / over-screen captures are exercised throughout. FSF-confirmed (mcr
/// dialect of FSF's `r1nkn1r/p1p1p1p/2c4/2C4/7/P2P2P/R2KN1R b`).
const CANNON_CAP: &str = "r1jkj1r/z1z1z1z/2c4/2C4/7/Z2Z2Z/R2KJ1R b - - 0 1";

/// A **horse-hobble** position, white to move: the white horse on d5 leaps around
/// the soldier blockers on c4/e4 toward the black king, so its per-leg hobbling is
/// exercised together with the king-safety scan that must register the horse's
/// (and the soldiers') attacks on the squares around the enemy king.
///
/// This is the position that **previously mismatched FSF** (mcr reported 290 at
/// depth 4, FSF 248): the king-safety scan did not detect a soldier's *forward*
/// attack on the square directly ahead of it, so it let the enemy king step into
/// that square. Marking the Soldier's attack as color-directional (so
/// `attackers_to` reverse-projects the opposing-color soldier pattern) fixes it.
/// Pinned here as a regression. FSF-confirmed (mcr dialect of FSF's
/// `3k3/7/3N3/2P1P2/7/7/3K3 w`).
const HORSE_HOBBLE: &str = "3k3/7/3J3/2Z1Z2/7/7/3K3 w - - 0 1";

/// The **minimal soldier-forward-attack regression**, black to move: the black
/// king on d6 stands beside the white soldiers on c5/e5 and ahead of the white
/// horse on d5. A white soldier on c5 attacks c6 (its forward step), and a soldier
/// on e5 attacks e6, so the black king may step to neither — it has only d6d7.
///
/// Before the directional-soldier-attack fix, mcr's `attackers_to` projected the
/// *same*-color soldier pattern back from c6 (which points to c7, not c5) and so
/// missed the c5 soldier, wrongly allowing d6c6 (and d6e6) and inflating perft.
/// FSF-confirmed (mcr dialect of FSF's `7/3k3/2PNP2/7/7/7/3K3 b`).
const SOLDIER_GUARD: &str = "7/3k3/2ZJZ2/7/7/7/3K3 b - - 0 1";

/// A **flying-general pin**, white to move: the two generals stand on the d-file
/// (d1 / d7) with a single white chariot between them on d4. The chariot may not
/// leave the d-file — doing so would leave the generals facing, which is illegal —
/// so it is pinned exactly as by a slider, purely by the flying-general rule.
/// FSF-confirmed (`3k3/7/7/3R3/7/7/3K3 w` — no dialect rewrite needed, it has only
/// chariots and generals).
const FLYING_GENERAL: &str = "3k3/7/7/3R3/7/7/3K3 w - - 0 1";

/// Asserts the generic Minixiangqi perft equals each pinned `(depth, nodes)`
/// count. Every number here also matched FSF minixiangqi `go perft` on the same
/// position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Minixiangqi::from_fen(fen).expect("valid Minixiangqi FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Minixiangqi7x7, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Minixiangqi perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) ------------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 19), (2, 331), (3, 6664)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(4, 127164), (5, 2666905)]);
}

// -- Horse hobbling + cannon middlegame (FSF-confirmed) ----------------------

#[test]
fn horse_cannon_middlegame_cheap() {
    check(HORSE_CANNON_MID, &[(1, 21), (2, 429), (3, 9032)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn horse_cannon_middlegame_deep() {
    check(HORSE_CANNON_MID, &[(4, 194413), (5, 4304380)]);
}

// -- Cannon over-screen capture (FSF-confirmed) ------------------------------

#[test]
fn cannon_capture_cheap() {
    check(CANNON_CAP, &[(1, 18), (2, 359), (3, 7719)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn cannon_capture_deep() {
    check(CANNON_CAP, &[(4, 163522), (5, 3652502)]);
}

// -- Horse hobble + the previously-failing soldier-guard mismatch (FSF) ------

#[test]
fn horse_hobble_cheap() {
    // Depth 4 was 290 before the directional-soldier-attack fix; FSF says 248.
    check(HORSE_HOBBLE, &[(1, 9), (2, 9), (3, 92), (4, 248)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn horse_hobble_deep() {
    check(HORSE_HOBBLE, &[(5, 3026)]);
}

#[test]
fn soldier_guard_regression() {
    // The minimal reproducer of the king-safety soldier-attack bug: the black
    // king may not step onto c6/e6 (guarded by the c5/e5 soldiers' forward step).
    check(SOLDIER_GUARD, &[(1, 1), (2, 7), (3, 5), (4, 40), (5, 62)]);
}

// -- Flying-general pin (FSF-confirmed) --------------------------------------

#[test]
fn flying_general_pin() {
    check(
        FLYING_GENERAL,
        &[(1, 8), (2, 12), (3, 156), (4, 136), (5, 1780)],
    );
}
