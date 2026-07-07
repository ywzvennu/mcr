//! Giveaway chess perft validation on the generic engine.
//!
//! Every `(depth, nodes)` pair below was produced identically by
//! `mcr::geometry::Giveaway::perft` and by Fairy-Stockfish's `go perft`
//! (`UCI_Variant giveaway`, a built-in — `giveaway_variant()`, `variant.cpp:402`)
//! on the byte-identical position. The `compare-fairy/` harness re-runs that
//! head-to-head on demand (see `compare-fairy/src/giveaway.rs`); this test pins the
//! confirmed numbers so a regression is caught without FSF present.
//!
//! ## What giveaway is
//!
//! Antichess **with castling**: a non-royal Commoner king (no check), **mandatory
//! captures**, king-promotion, and the inverted "losing wins" terminal. Two things
//! drive the counts away from standard chess: the forced-capture filter prunes every
//! quiet move whenever a capture exists (so `startpos` perft(3) is `8067`, chess
//! `8902`), and the whole-army extinction truncates a subtree the instant a side is
//! stripped of its last piece.
//!
//! mcr and FSF spell giveaway with the identical standard-chess letters, so the FEN
//! is passed through unchanged. The deep layers are `#[ignore]`d so `cargo test`
//! stays fast — run them with
//! `cargo test --release --test perft_giveaway -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Giveaway};

/// The giveaway starting FEN — the standard array **with** castling rights,
/// confirmed byte-for-byte against FSF `UCI_Variant giveaway` / `position startpos`.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// The classic Kiwipete position (both queens, rich tactics). Giveaway's
/// king-promotion lifts its depth-4 count (`3872`) above codrus's (`3836`).
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

/// A castling-reachable position (open back ranks, both sides may castle, no capture
/// forced) — exercises giveaway's non-royal castling.
const CASTLING: &str = "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1";

/// A forced-capture position: White's e4 pawn must take d5 (the only capture), so
/// the whole subtree hangs off that one forced move.
const FORCED: &str = "8/8/8/3p4/4P3/8/8/K6k w - - 0 1";

/// Black (to move) has no pieces at all — it has already won, so the node is
/// terminal and `go perft N` is 0 at every depth.
const BLACK_EXTINCT: &str = "8/8/8/8/8/8/8/K7 b - - 0 1";

fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Giveaway::from_fen(fen).expect("valid giveaway FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "giveaway perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
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
    // Depth 6 is where king-promotion first appears (`46264162`, vs codrus's
    // king-less `46263517`).
    check(STARTPOS, &[(5, 2732672), (6, 46264162)]);
}

#[test]
fn kiwipete_and_castling_cheap() {
    check(KIWIPETE, &[(1, 8), (2, 62), (3, 487)]);
    check(CASTLING, &[(1, 25), (2, 625), (3, 14860)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn kiwipete_and_castling_deep() {
    check(KIWIPETE, &[(4, 3872)]);
    check(CASTLING, &[(4, 352666)]);
}

#[test]
fn forced_capture_prunes_quiets() {
    // exd5 is the only capture; the K/king quiets are all pruned.
    check(FORCED, &[(1, 1), (2, 3), (3, 12), (4, 72)]);
}

#[test]
fn zero_pieces_truncates_to_zero() {
    check(BLACK_EXTINCT, &[(1, 0), (2, 0), (3, 0), (4, 0)]);
}
