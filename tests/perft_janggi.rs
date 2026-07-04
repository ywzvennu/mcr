//! Janggi (Korean chess, 9x10 / `u128`) perft validation on the generic engine
//! (issue #205) — the third marquee fairy variant: it shares Xiangqi's
//! [`Xiangqi9x10`] geometry and palace but adds **palace diagonals**, a
//! **screen-mandatory cannon** (screen ≠ cannon, may not capture a cannon, may
//! jump the palace diagonal), a **long blockable elephant**, a **sideways-always
//! soldier**, and a legal **pass** move.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Janggi::perft` and by Fairy-Stockfish (FSF, `UCI_Variant
//! janggi`, built `largeboards=yes`) running `go perft` on the byte-identical
//! position. The `compare-fairy/` harness re-runs that head-to-head on demand
//! (`compare-fairy/src/janggi.rs`); this test pins the FSF-confirmed numbers so a
//! regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's janggi `startFen`:
//!
//! ```text
//! FSF dialect: rnba1abnr/4k4/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/4K4/RNBA1ABNR w - - 0 1
//! mcr dialect: rjxu1uxjr/4k4/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/4K4/RJXU1UXJR w - - 0 1
//! ```
//!
//! The two describe the same position; mcr spells four pieces differently because
//! the FSF letters `a n b p` already name the Hawk / Knight / Bishop / Pawn in
//! mcr's [`WideRole`](mcr::geometry::WideRole), so the Guard is `u`, the Horse
//! `j`, the Elephant `x` (the Xiangqi elephant already took `o`), and the Soldier
//! `z`. The chariots (`r`) and cannons (`c`) match. The generals start on the
//! palace **centre** (e2 / e9), the guards/elephants/horses/chariots on the back
//! rank, the cannons on rank 3 (b3/h3, b8/h8), the soldiers on ranks 4 / 7.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_janggi -- --include-ignored`.

use mcr::geometry::{perft as gperft, Janggi, Xiangqi9x10};

/// The Janggi starting FEN (mcr dialect), confirmed against Fairy-Stockfish's
/// `UCI_Variant janggi`.
const STARTPOS: &str = "rjxu1uxjr/4k4/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/4K4/RJXU1UXJR w - - 0 1";

/// **Screen-cannon** corpus, white to move: a-file cannon jumps a horse-screen and
/// captures the rook beyond (allowed); c-file cannon's only screen is another
/// **cannon** (forbidden — no vertical move); g-file cannon has a horse-screen but
/// the target g8 is a **cannon** (forbidden capture). FSF-confirmed (mcr dialect
/// of FSF's `9/1k7/r1r3c2/9/9/9/N1C3N2/9/4K4/C1C3C2`).
const SCREEN_CANNON: &str = "9/1k7/r1r3c2/9/9/9/J1C3J2/9/4K4/C1C3C2 w - - 0 1";

/// **Cannon palace-diagonal jump**, white to move: a cannon on d1 jumps the
/// (non-cannon) horse-screen on the palace centre e2 to the opposite corner f3,
/// capturing the chariot there. FSF-confirmed (mcr dialect of FSF's
/// `9/1k7/9/9/9/9/9/3K1r3/4N4/3C5`).
const CANNON_PALACE_DIAG: &str = "9/1k7/9/9/9/9/9/3K1r3/4J4/3C5 w - - 0 1";

/// **Palace diagonals** for general, guard, and chariot at once, white to move:
/// the general d1, guard d3, and chariot f1 all reach the palace centre e2 along
/// the diagonal. FSF-confirmed (mcr dialect of FSF's
/// `9/4k4/9/9/9/9/9/3A5/9/3K1R3`).
const PALACE_DIAG: &str = "9/4k4/9/9/9/9/9/3U5/9/3K1R3 w - - 0 1";

/// **Long blockable elephant**, white to move: the elephant e5 has its two up-leaps
/// blocked (a soldier on e6) and its two left-leaps blocked (a soldier on d5),
/// leaving the four right/down leaps open. FSF-confirmed (mcr dialect of FSF's
/// `9/4k4/9/9/4P4/3PB4/9/9/9/1K7`).
const LONG_ELEPHANT: &str = "9/4k4/9/9/4Z4/3ZX4/9/9/9/1K7 w - - 0 1";

/// **Sideways + forward-diagonal soldier**, white to move: an open soldier e5 steps
/// forward or sideways, and a soldier on the black palace corner d8 steps forward,
/// sideways, and along the forward palace diagonal into the centre e9.
/// FSF-confirmed (mcr dialect of FSF's `5k3/9/3P5/9/9/4P4/9/9/9/1K7`).
const SOLDIER_SIDE_DIAG: &str = "5k3/9/3Z5/9/9/4Z4/9/9/9/1K7 w - - 0 1";

/// **Pass legal**, white to move and **not** in check: the legal pass `e2e2`
/// appears among the moves and is counted in perft. FSF-confirmed (mcr dialect of
/// FSF's `9/1k7/9/9/9/9/4p4/9/4K4/9`).
const PASS_LEGAL: &str = "9/1k7/9/9/9/9/4z4/9/4K4/9 w - - 0 1";

/// **No pass while in check**, white to move and in check from a black soldier on
/// e3: no pass is offered; every move is a check evasion. FSF-confirmed (mcr
/// dialect of FSF's `9/1k7/9/9/9/9/9/4p4/4K4/9`).
const IN_CHECK_NO_PASS: &str = "9/1k7/9/9/9/9/9/4z4/4K4/9 w - - 0 1";

fn perft(fen: &str, depth: u32) -> u64 {
    let pos = Janggi::from_fen(fen).expect("Janggi FEN parses");
    gperft::<Xiangqi9x10, _>(&pos, depth)
}

// ---- Startpos: shallow layers run by default, deep layers ignored -------------

#[test]
fn startpos_perft_shallow() {
    // FSF `UCI_Variant janggi`, `position startpos`, `go perft 1..4`.
    assert_eq!(perft(STARTPOS, 1), 32);
    assert_eq!(perft(STARTPOS, 2), 1024);
    assert_eq!(perft(STARTPOS, 3), 33000);
    assert_eq!(perft(STARTPOS, 4), 1065277);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn startpos_perft_deep() {
    assert_eq!(perft(STARTPOS, 5), 35243995);
}

// ---- Mechanic-isolating corpus (all FSF-confirmed) ----------------------------

#[test]
fn screen_cannon_perft() {
    // Allowed over-screen capture + screen-is-cannon forbidden + target-is-cannon
    // forbidden, all in one position.
    assert_eq!(perft(SCREEN_CANNON, 1), 32);
    assert_eq!(perft(SCREEN_CANNON, 2), 660);
    assert_eq!(perft(SCREEN_CANNON, 3), 17961);
}

#[test]
fn cannon_palace_diagonal_jump_perft() {
    assert_eq!(perft(CANNON_PALACE_DIAG, 1), 2);
    assert_eq!(perft(CANNON_PALACE_DIAG, 2), 20);
    assert_eq!(perft(CANNON_PALACE_DIAG, 3), 260);
    assert_eq!(perft(CANNON_PALACE_DIAG, 4), 4141);
}

#[test]
fn palace_diagonals_perft() {
    assert_eq!(perft(PALACE_DIAG, 1), 21);
    assert_eq!(perft(PALACE_DIAG, 2), 124);
    assert_eq!(perft(PALACE_DIAG, 3), 2522);
    assert_eq!(perft(PALACE_DIAG, 4), 8935);
}

#[test]
fn long_elephant_perft() {
    assert_eq!(perft(LONG_ELEPHANT, 1), 10);
    assert_eq!(perft(LONG_ELEPHANT, 2), 87);
    assert_eq!(perft(LONG_ELEPHANT, 3), 945);
    assert_eq!(perft(LONG_ELEPHANT, 4), 3877);
}

#[test]
fn soldier_sideways_and_palace_diagonal_perft() {
    assert_eq!(perft(SOLDIER_SIDE_DIAG, 1), 8);
    assert_eq!(perft(SOLDIER_SIDE_DIAG, 2), 23);
    assert_eq!(perft(SOLDIER_SIDE_DIAG, 3), 162);
    assert_eq!(perft(SOLDIER_SIDE_DIAG, 4), 512);
}

#[test]
fn pass_legal_when_not_in_check_perft() {
    // The pass (`e2e2`) is offered and counted.
    assert_eq!(perft(PASS_LEGAL, 1), 8);
    assert_eq!(perft(PASS_LEGAL, 2), 32);
    assert_eq!(perft(PASS_LEGAL, 3), 117);
}

#[test]
fn no_pass_while_in_check_perft() {
    // The same shape but the side to move is in check: no pass, only evasions.
    assert_eq!(perft(IN_CHECK_NO_PASS, 1), 6);
    assert_eq!(perft(IN_CHECK_NO_PASS, 2), 21);
    assert_eq!(perft(IN_CHECK_NO_PASS, 3), 50);
}

// ---- Startpos round-trips through the mcr FEN I/O ------------------------------

#[test]
fn startpos_fen_round_trips() {
    let pos = Janggi::from_fen(STARTPOS).expect("startpos parses");
    assert_eq!(pos.to_fen(), STARTPOS);
}
