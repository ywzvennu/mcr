//! Almost Chess (8x8) perft validation on the generic engine — standard chess
//! with the Queen replaced by a Chancellor (Rook + Knight).
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mce::geometry::Almost::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant almost`) running `go perft` on the byte-identical position. The
//! `compare-fairy/` differential fuzzer re-runs the head-to-head on demand
//! (`--difffuzz --variant almost`); this test pins the FSF-confirmed numbers so a
//! regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! ```text
//! FSF dialect: rnbckbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBCKBNR w KQkq - 0 1
//! mce dialect: rnbekbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBEKBNR w KQkq - 0 1
//! ```
//!
//! The two differ only in the chancellor's letter (`c` in FSF, `e` in mce, mce's
//! letter for the rook-knight compound [`WideRole::Elephant`]).
//!
//! The deep layer is `#[ignore]`d so `cargo test` stays fast — run it with
//! `cargo test --release --test perft_almost -- --include-ignored`.

use mce::geometry::{perft as gperft, Almost, Chess8x8};

/// The Almost Chess starting FEN (mce dialect), confirmed against FSF's
/// `UCI_Variant almost`.
const STARTPOS: &str = "rnbekbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBEKBNR w KQkq - 0 1";

/// A castling-rich position: both back ranks cleared between the king (e-file) and
/// both rooks, so both sides may castle both ways. It has no chancellor, so the
/// FEN is identical in both dialects.
const CASTLE: &str = "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1";

/// Asserts the generic Almost Chess perft equals each pinned `(depth, nodes)`
/// count. Every number here also matched FSF `almost go perft` on the same
/// position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Almost::from_fen(fen).expect("valid Almost FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Almost perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 22), (2, 484), (3, 11895), (4, 290522)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 7812388)]);
}

#[test]
fn castle_cheap() {
    check(CASTLE, &[(1, 25), (2, 625), (3, 15206), (4, 369906)]);
}
