//! Perft for Cannon Shogi (大砲将棋, 9x9), pinned **node-for-node against
//! Fairy-Stockfish** `UCI_Variant cannonshogi` (issue #269).
//!
//! Cannon Shogi is the 9x9 Shogi army with the Pawn replaced by a sideways-stepping
//! Soldier and five CANNON-type pieces added (the Xiangqi rook-cannon plus a
//! rook-hopper, a bishop-cannon and a bishop-hopper, with four promoted forms), all
//! droppable from the hand. It is the first variant to combine the Shogi hand/drops
//! with the cannon pseudo-legal + per-move-verify king-safety path.
//!
//! Every count below was produced by Fairy-Stockfish (`go perft <d>` with
//! `VariantPath` pointing at the bundled `variants.ini`) and re-verified by mcr.
//! The mcr dialect differs from FSF only in the cannon piece letters (Cannon `c`;
//! the three new movers `=a` / `=c` / `=i`; the promoted forms `=u` / `=w` / `=f` /
//! `=e`); see `compare-fairy/src/cannonshogi.rs` for the round-trip harness.

use mcr::geometry::{perft as gperft, CannonShogi, Shogi9x9};

/// The confirmed Cannon Shogi start (FSF `position startpos`), mcr dialect, empty
/// hand. FSF: `lnsgkgsnl/1rci1uab1/p1p1p1p1p/9/9/9/P1P1P1P1P/1BAU1ICR1/LNSGKGSNL[-] w 0 1`.
const STARTPOS: &str =
    "lnsgkgsnl/1r=c=i1c=ab1/p1p1p1p1p/9/9/9/P1P1P1P1P/1B=AC1=I=CR1/LNSGKGSNL[] w - - 0 1";

/// A midgame reached from the start (FSF `moves d2d7 c7c6 d7d9+ b8b9`): White has a
/// promoted Cannon (`+U`, mcr `=U`) on the board and a Gold in hand, Black to move.
/// Exercises a promoted cannon, a hand drop, and an over-screen cannon capture.
const MIDGAME_PROMOTED: &str =
    "lns=Ukgsnl/1r=c=i1c=ab1/p3p1p1p/2p6/9/9/P1P1P1P1P/1B=A2=I=CR1/LNSGKGSNL[G] b - - 0 2";

/// A sparse drop-heavy lab: lone kings with both hands full of every cannon-type
/// piece (and a Rook / Bishop). Stresses drop generation and the cannon hop sets on
/// an open board. FSF `4k4/9/9/9/9/9/9/9/4K4[RBUACIrbuaci] w - - 0 1`, mcr dialect.
const DROP_LAB: &str = "4k4/9/9/9/9/9/9/9/4K4[RBC=A=C=Irbc=a=c=i] w - - 0 1";

fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = CannonShogi::from_fen(fen).expect("valid Cannon Shogi FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Shogi9x9, _, _>(&pos, depth);
        assert_eq!(got, expected, "perft({depth}) mismatch for {fen}");
    }
}

#[test]
fn startpos_round_trips() {
    let pos = CannonShogi::from_fen(STARTPOS).expect("valid start FEN");
    assert_eq!(pos.to_fen(), STARTPOS, "start FEN must round-trip");
}

#[test]
fn startpos_cheap() {
    // FSF-confirmed: perft(1)=60, perft(2)=3447, perft(3)=216600.
    check(STARTPOS, &[(1, 60), (2, 3447), (3, 216600)]);
}

#[test]
fn midgame_promoted_cheap() {
    // FSF-confirmed: perft(1)=59, perft(2)=5665, perft(3)=352012.
    check(MIDGAME_PROMOTED, &[(1, 59), (2, 5665), (3, 352012)]);
}

#[test]
fn drop_lab_cheap() {
    // FSF-confirmed: perft(1)=479, perft(2)=215443.
    check(DROP_LAB, &[(1, 479), (2, 215443)]);
}

// Cannon Shogi is a Tier-D large-board exception to the per-PR depth-4 floor:
// every position's depth-4 perft is large (startpos 13.4M, midgame 28.7M) and
// ~95s in debug, so the depth-4 layer stays `#[ignore]`d for the release sweep.
// The default suite proves depth ≤3 (startpos_cheap / midgame / drop_lab).
#[test]
#[ignore = "deep perft (~13.4M nodes, ~95s in debug); run with --release --include-ignored"]
fn startpos_deep() {
    // FSF-confirmed: perft(4)=13406022, perft(5)=909545896.
    check(STARTPOS, &[(4, 13406022), (5, 909545896)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_promoted_deep() {
    // FSF-confirmed: perft(4)=28652093.
    check(MIDGAME_PROMOTED, &[(4, 28652093)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn drop_lab_deep() {
    // FSF-confirmed: perft(3)=82384934.
    check(DROP_LAB, &[(3, 82384934)]);
}
