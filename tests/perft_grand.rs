//! Grand chess (10x10 / `u128`) perft validation on the generic engine
//! (issue #175) — the first **10x10** variant, validating a **second** `u128`
//! geometry ([`Grand10x10`], 100 squares) after Capablanca proved the 10x8 path.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Grand::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant grand`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position. The `compare-fairy/` harness re-runs that
//! head-to-head on demand (`compare-fairy/src/grand.rs`); this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `grand_variant()` (`startFen`):
//!
//! ```text
//! FSF dialect: r8r/1nbqkcabn1/pppppppppp/10/10/10/10/PPPPPPPPPP/1NBQKCABN1/R8R w - - 0 1
//! mcr dialect: r8r/1nbqkeabn1/pppppppppp/10/10/10/10/PPPPPPPPPP/1NBQKEABN1/R8R w - - 0 1
//! ```
//!
//! The two differ only in the marshal's letter (`c` in FSF — its chancellor —
//! and `e` in mcr, mcr's letter for the rook-knight compound
//! [`WideRole::Elephant`](mcr::geometry::WideRole::Elephant)). The cardinal is `a`
//! ([`WideRole::Hawk`](mcr::geometry::WideRole::Hawk)) in both.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_grand -- --include-ignored`.

use mcr::geometry::{perft as gperft, Grand, Grand10x10};

/// The Grand starting FEN (mcr dialect), confirmed against Fairy-Stockfish's
/// `UCI_Variant grand`.
const STARTPOS: &str = "r8r/1nbqkeabn1/pppppppppp/10/10/10/10/PPPPPPPPPP/1NBQKEABN1/R8R w - - 0 1";

/// A developed midgame, black to move: both knights out for each side, a marshal
/// and cardinal still home, and a pawn structure that admits captures and en
/// passant at depth. Reached from the startpos by a sequence of legal moves and
/// confirmed move-for-move by FSF.
const MID1: &str = "r8r/2bqkeab2/pppp1ppppp/2n4n2/3Np5/3P6/7N2/PPP1PPPPPP/2BQKEAB2/R8R b - - 1 4";

/// A promote-to-captured position, white to move: white has had one rook, one
/// knight, the queen, the marshal, and the cardinal of its army reduced to the
/// limit, so only the **rook** and **bishop** are legal promotion targets (white
/// holds one of each, below the starting count of two). A white pawn on i9 sits in
/// the optional part of the promotion zone (it may promote there or push to the
/// forced last rank). This exercises the three-rank zone *and* the
/// promote-only-to-a-captured-type rule together.
const PROMO: &str = "4k5/8P1/10/10/10/10/10/10/10/RNBQK1EAN1 w - - 0 1";

/// Asserts the generic Grand perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF grand `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Grand::from_fen(fen).expect("valid Grand FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Grand10x10, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Grand perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 65), (2, 4225), (3, 259514)]);
}

#[test]
#[ignore = "deep perft (~16M nodes, slow in debug); run with --release --include-ignored"]
fn startpos_deep() {
    // FSF grand `go perft` on the startpos.
    check(STARTPOS, &[(4, 15921643)]);
}

// -- Midgame (FSF-confirmed) ------------------------------------------------

#[test]
fn midgame_cheap() {
    check(MID1, &[(1, 70), (2, 5385), (3, 353340)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_deep() {
    check(MID1, &[(4, 25876782)]);
}

// -- Promote-to-captured (FSF-confirmed) ------------------------------------

#[test]
fn promote_to_captured_cheap() {
    check(PROMO, &[(1, 75), (2, 221), (3, 17459)]);
}

/// The per-PR depth-4 floor for Grand: the promote-to-captured position is only
/// 72_092 nodes at depth 4, so it proves depth ≥4 in the default (debug) suite
/// while the far heavier `startpos_deep` (~16M nodes) stays `#[ignore]`d.
#[test]
fn promote_to_captured_depth4() {
    // One ply deeper, still exercising the captured-restricted promotion set.
    check(PROMO, &[(4, 72092)]);
}
