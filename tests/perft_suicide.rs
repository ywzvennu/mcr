//! Suicide chess perft validation on the generic engine.
//!
//! Every `(depth, nodes)` pair below was produced identically by
//! `mcr::geometry::Suicide::perft` and by Fairy-Stockfish's `go perft`
//! (`UCI_Variant suicide`, a built-in — `suicide_variant()`, `variant.cpp:430`) on
//! the byte-identical position. The `compare-fairy/` harness re-runs that
//! head-to-head on demand (see `compare-fairy/src/suicide.rs`).
//!
//! ## What suicide is
//!
//! Antichess: giveaway **without castling** (and with a piece-count stalemate rule
//! that affects only adjudication, not perft). Its startpos has no castling rights,
//! so from that position the counts coincide with giveaway's; a FEN that *grants*
//! rights still yields no castling move (suicide never castles), which is where it
//! parts from giveaway.
//!
//! The deep layers are `#[ignore]`d — run with
//! `cargo test --release --test perft_suicide -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Suicide};

/// The suicide starting FEN — the standard array with **no** castling rights,
/// confirmed against FSF `UCI_Variant suicide` / `position startpos`.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1";

/// A position whose FEN grants KQkq: suicide must still produce **no** castling move
/// (23/529/11717), where giveaway would castle (25/625/14860).
const RIGHTS_BUT_NO_CASTLING: &str = "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1";

/// Black (to move) has no pieces — it has won, so `go perft N` is 0 at every depth.
const BLACK_EXTINCT: &str = "8/8/8/8/8/8/8/K7 b - - 0 1";

fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Suicide::from_fen(fen).expect("valid suicide FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "suicide perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 20), (2, 400), (3, 8067), (4, 153299)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 2732672), (6, 46264162)]);
}

#[test]
fn no_castling_even_with_rights() {
    // KQkq in the FEN, but suicide never castles: 23 moves, not giveaway's 25.
    check(RIGHTS_BUT_NO_CASTLING, &[(1, 23), (2, 529), (3, 11717)]);
}

#[test]
fn zero_pieces_truncates_to_zero() {
    check(BLACK_EXTINCT, &[(1, 0), (2, 0), (3, 0), (4, 0)]);
}
