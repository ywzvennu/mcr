//! Losers chess perft validation on the generic engine.
//!
//! Every `(depth, nodes)` pair below was produced identically by
//! `mcr::geometry::Losers::perft` and by Fairy-Stockfish's `go perft`
//! (`UCI_Variant losers`, a built-in — `losers_variant()`, `variant.cpp:389`) on the
//! byte-identical position. The `compare-fairy/` harness re-runs that head-to-head
//! on demand (see `compare-fairy/src/losers.rs`).
//!
//! ## What losers is
//!
//! Unlike the giveaway family, losers keeps a **royal** king — so king safety
//! applies and the counts sit *below* giveaway's (startpos depth 4: losers `152955`,
//! giveaway `153299`). It adds **mandatory captures** (the forced-capture filter runs
//! on the king-safe move set) and a **bare-king** terminal: a side reduced to its
//! lone king has won (total piece count `<= 1`), truncating the subtree.
//!
//! The deep layers are `#[ignore]`d — run with
//! `cargo test --release --test perft_losers -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Losers};

/// The losers starting FEN — the standard array with castling rights, confirmed
/// against FSF `UCI_Variant losers` / `position startpos`.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// Kiwipete: the royal-king safety prunes more than the giveaway family (depth 4
/// `3498`, vs giveaway `3872`).
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

/// A forced capture (exd5) that immediately bares Black to a lone king — so the
/// subtree truncates at depth 2 (Black has won by being bared).
const FORCED_TO_BARE: &str = "8/8/8/3p4/4P3/8/8/K6k w - - 0 1";

/// Black (to move) is already a bare king — it has won, so `go perft N` is 0.
const BLACK_BARE: &str = "7k/8/8/3P4/8/8/8/K7 b - - 0 1";

fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Losers::from_fen(fen).expect("valid losers FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "losers perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 20), (2, 400), (3, 8067), (4, 152955)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 2723795), (6, 46038682)]);
}

#[test]
fn kiwipete_cheap() {
    check(KIWIPETE, &[(1, 8), (2, 62), (3, 487)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn kiwipete_deep() {
    check(KIWIPETE, &[(4, 3498)]);
}

#[test]
fn bare_king_truncations() {
    // exd5 is forced and bares Black -> depth 2 truncates to 0.
    check(FORCED_TO_BARE, &[(1, 1), (2, 0), (3, 0), (4, 0)]);
    // An already-bare king is terminal at every depth.
    check(BLACK_BARE, &[(1, 0), (2, 0), (3, 0), (4, 0)]);
}
