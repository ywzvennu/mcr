//! Amazon Chess (8x8) perft validation on the generic engine — standard chess
//! with the Queen replaced by an Amazon (Queen + Knight).
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mce::geometry::Amazon::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant amazon`) running `go perft` on the byte-identical position. The
//! `compare-fairy/` differential fuzzer re-runs the head-to-head on demand
//! (`--difffuzz --variant amazon`); this test pins the FSF-confirmed numbers so a
//! regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! ```text
//! FSF dialect: rnbakbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBAKBNR w KQkq - 0 1
//! mce dialect: rnb**akbnr/pppppppp/8/8/8/8/PPPPPPPP/RNB**AKBNR w KQkq - 0 1
//! ```
//!
//! The two are the same position; the amazon is `a` in FSF and the second-bank
//! overflow `**a` (the [`WideRole::Angel`] Queen + Knight compound) in mce.
//!
//! The deep layer is `#[ignore]`d so `cargo test` stays fast — run it with
//! `cargo test --release --test perft_amazon -- --include-ignored`.

use mce::geometry::{perft as gperft, Amazon, Chess8x8};

/// The Amazon Chess starting FEN (mce dialect), confirmed against FSF's
/// `UCI_Variant amazon`.
const STARTPOS: &str = "rnb**akbnr/pppppppp/8/8/8/8/PPPPPPPP/RNB**AKBNR w KQkq - 0 1";

/// A castling-rich position (standard army, no amazon): both back ranks cleared
/// between the king and both rooks, so both sides may castle both ways.
const CASTLE: &str = "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1";

/// Asserts the generic Amazon Chess perft equals each pinned `(depth, nodes)`
/// count. Every number here also matched FSF `amazon go perft` on the same
/// position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Amazon::from_fen(fen).expect("valid Amazon FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Amazon perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 22), (2, 484), (3, 12483), (4, 318185)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 9319911)]);
}

#[test]
fn castle_cheap() {
    check(CASTLE, &[(1, 25), (2, 625), (3, 15206), (4, 369906)]);
}
