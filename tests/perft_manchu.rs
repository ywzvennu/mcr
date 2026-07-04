//! Manchu (yipaisanxianqi, 9x10 / `u128`) perft validation on the generic engine
//! (issue #230) — an **asymmetric Xiangqi** in which one side keeps a full Xiangqi
//! army and the other replaces its rook/cannon/horse cluster with a single
//! SUPER-PIECE, the **Banner** (Rook + Cannon + Horse combined).
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Manchu::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant manchu`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position. The `compare-fairy/` harness re-runs that head-to-head
//! on demand (`compare-fairy/src/manchu.rs`); this test pins the FSF-confirmed
//! numbers so a regression is caught even without FSF present.
//!
//! Manchu **reuses the entire Xiangqi rule layer** (palace, river, horse, cannon,
//! elephant, soldier, advisor, flying-general); the only differences are the
//! starting array and the new Banner super-piece, computed from the live board.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `manchu_variant()` (`startFen`):
//!
//! ```text
//! FSF dialect: rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/9/9/M1BAKAB2 w - - 0 1
//! mcr dialect: rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/9/9/*M1OUKUO2 w - - 0 1
//! ```
//!
//! The two describe the same position; mcr spells the Xiangqi pieces `u j o z`
//! (the FSF letters `a n b p` already name the Hawk / Knight / Bishop / Pawn) and
//! the Banner as the overflow token `*M` (FSF's `m`). The deep layers are
//! `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_manchu -- --include-ignored`.

use mcr::geometry::{perft as gperft, Manchu, Xiangqi9x10};

/// The Manchu starting FEN (mcr dialect): a full Black Xiangqi army, and White's
/// rook/cannon/horse cluster replaced by the single Banner `*M` on a1.
const STARTPOS: &str = "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/9/9/*M1OUKUO2 w - - 0 1";

/// The Banner advanced to the centre (e5), white to move: exercises the Banner's
/// rook slide (move + capture), cannon over-screen captures across the river, and
/// hobbled horse leaps in the open. FSF-confirmed (mcr dialect of FSF's
/// `rnbakabnr/9/1c5c1/p1p1p1p1p/9/4M4/P1P1P1P1P/9/9/2BAKAB2`).
const BANNER_CENTER: &str = "rjoukuojr/9/1c5c1/z1z1z1z1z/9/4*M4/Z1Z1Z1Z1Z/9/9/2OUKUO2 w - - 0 1";

/// The startpos with **Black to move**: exercises the full Black Xiangqi army
/// (rooks, horses, cannons, elephants, advisors, soldiers) against the lone
/// Banner. FSF-confirmed (FSF's startpos, side-to-move flipped to black).
const BLACK_TO_MOVE: &str = "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/9/9/*M1OUKUO2 b - - 0 1";

/// The Banner pushed deep into enemy territory (e9, beside the Black palace),
/// white to move: many Banner captures of Black pieces and rich tactical replies.
/// FSF-confirmed (mcr dialect of FSF's
/// `rnbakabnr/4M4/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/9/9/2BAKAB2`).
const BANNER_DEEP: &str = "rjoukuojr/4*M4/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/9/9/2OUKUO2 w - - 0 1";

/// A **Banner cannon-checkmate**, black to move and mated: a White Banner on a1
/// jumps the screen on a5 to "cannon-capture" the Black general on a10 down the
/// open a-file — a checkmate (no Black reply). Exercises the Banner's over-screen
/// cannon **check** detection (a royal square reached only through the capture
/// portion of the Banner's board-aware set). FSF-confirmed (mate: 0 nodes; the
/// screen `p`->`z` for the mcr dialect).
const CANNON_MATE: &str = "k8/9/9/9/z8/9/9/9/9/*M3K4 b - - 0 1";

/// A **Banner rook-check**, black to move and in check: a White Banner on e1
/// checks the Black general on e10 straight down the open e-file (the Banner's
/// rook part). The Black general has exactly two escape steps. Exercises the
/// Banner's rook-line check detection. FSF-confirmed (the screenless e-file; no
/// dialect rewrite beyond the Banner token).
const ROOK_CHECK: &str = "4k4/9/9/9/9/9/9/9/9/4*M4 b - - 0 1";

/// Asserts the generic Manchu perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF manchu `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Manchu::from_fen(fen).expect("valid Manchu FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Xiangqi9x10, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Manchu perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 18), (2, 860), (3, 17648)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(4, 798554), (5, 17817159)]);
}

// -- Banner in the centre: rook + cannon + horse in the open (FSF-confirmed) --

#[test]
fn banner_center_cheap() {
    check(BANNER_CENTER, &[(1, 26), (2, 684), (3, 19264)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn banner_center_deep() {
    check(BANNER_CENTER, &[(4, 776919)]);
}

// -- Black's full Xiangqi army to move vs the lone Banner (FSF-confirmed) -----

#[test]
fn black_to_move_cheap() {
    check(BLACK_TO_MOVE, &[(1, 48), (2, 855), (3, 39140)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn black_to_move_deep() {
    check(BLACK_TO_MOVE, &[(4, 797108)]);
}

// -- Banner deep in enemy territory: many captures (FSF-confirmed) ------------

#[test]
fn banner_deep_cheap() {
    check(BANNER_DEEP, &[(1, 29), (2, 533), (3, 13749)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn banner_deep_deep() {
    check(BANNER_DEEP, &[(4, 484632)]);
}

// -- Banner cannon-checkmate (FSF-confirmed; over-screen cannon check) --------

#[test]
fn cannon_mate() {
    // The Banner's over-screen cannon "capture" of the general is a checkmate:
    // Black, to move, has no reply. FSF says 0 nodes at every depth.
    check(CANNON_MATE, &[(1, 0), (2, 0), (3, 0)]);
}

// -- Banner rook-check (FSF-confirmed; rook-line check + evasions) ------------

#[test]
fn rook_check_cheap() {
    check(ROOK_CHECK, &[(1, 2), (2, 42), (3, 60)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn rook_check_deep() {
    check(ROOK_CHECK, &[(4, 1278)]);
}
