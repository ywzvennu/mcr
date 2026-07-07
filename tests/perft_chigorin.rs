//! Chigorin Chess (8x8) perft validation on the generic engine — an asymmetric
//! variant: a White knight army (knights + a Chancellor) vs a Black bishop army
//! (bishops + a queen), with colour-restricted pawn promotion.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Chigorin::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant chigorin`) running `go perft` on the byte-identical position. The
//! `compare-fairy/` differential fuzzer re-runs the head-to-head on demand
//! (`--difffuzz --variant chigorin`); this test pins the FSF-confirmed numbers so
//! a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! ```text
//! FSF dialect: rbbqkbbr/pppppppp/8/8/8/8/PPPPPPPP/RNNCKNNR w KQkq - 0 1
//! mcr dialect: rbbqkbbr/pppppppp/8/8/8/8/PPPPPPPP/RNNEKNNR w KQkq - 0 1
//! ```
//!
//! The two differ only in White's chancellor letter (`c` in FSF, `e` in mcr). The
//! deep layer is `#[ignore]`d so `cargo test` stays fast — run it with
//! `cargo test --release --test perft_chigorin -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Chigorin};

/// The Chigorin starting FEN (mcr dialect), confirmed against FSF's
/// `UCI_Variant chigorin`.
const STARTPOS: &str = "rbbqkbbr/pppppppp/8/8/8/8/PPPPPPPP/RNNEKNNR w KQkq - 0 1";

/// A promotion position: White has a pawn on the 7th (b7) and Black one on the
/// 2nd (b2), exercising the colour-restricted promotion sets (White ->
/// Chancellor/Rook/Knight at depth 1, Black -> Queen/Rook/Bishop at depth 2).
const PROMO: &str = "4k3/1P6/8/8/8/8/1p6/4K3 w - - 0 1";

fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Chigorin::from_fen(fen).expect("valid Chigorin FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Chigorin perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 26), (2, 416), (3, 11408), (4, 229973)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 6624527)]);
}

#[test]
fn promo_cheap() {
    check(PROMO, &[(1, 8), (2, 52), (3, 478), (4, 4621)]);
}
