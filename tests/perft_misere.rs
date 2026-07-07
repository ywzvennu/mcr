//! Misère chess perft validation on the generic engine.
//!
//! Every `(depth, nodes)` pair below was produced identically by
//! `mcr::geometry::Misere::perft` and by Fairy-Stockfish's `go perft`
//! (`UCI_Variant misere`, a built-in — `misere_variant()`, `variant.cpp:381`) on the
//! byte-identical position. The `compare-fairy/` harness re-runs that head-to-head
//! on demand (see `compare-fairy/src/misere.rs`).
//!
//! ## What misère is
//!
//! Ordinary chess in every respect — royal king, check, checkmate, castling, en
//! passant, standard promotion, **no** forced captures — except that a checkmate is a
//! win for the *mated* side. Because nothing about movement changes, **misère perft
//! is byte-identical to standard chess** (startpos `20/400/8902/197281/4865609`, the
//! standard-chess numbers); only the reported outcome of a checkmate differs.
//!
//! The deep layers are `#[ignore]`d — run with
//! `cargo test --release --test perft_misere -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Misere};

/// The misère starting FEN — the standard array with castling rights.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// Kiwipete — misère's move set is standard chess, so these are the standard
/// Kiwipete counts.
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

/// A fool's-mate checkmate (White to move, mated): the game is over, so `go perft N`
/// is 0 at every depth — but in misère White *wins* (see the variant's adjudication
/// test `checkmated_side_wins`).
const WHITE_MATED: &str = "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3";

fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Misere::from_fen(fen).expect("valid misère FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "misère perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

#[test]
fn startpos_is_standard_chess() {
    check(STARTPOS, &[(1, 20), (2, 400), (3, 8902), (4, 197281)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 4865609), (6, 119060324)]);
}

#[test]
fn kiwipete_cheap() {
    check(KIWIPETE, &[(1, 48), (2, 2039), (3, 97862)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn kiwipete_deep() {
    check(KIWIPETE, &[(4, 4085603)]);
}

#[test]
fn checkmate_truncates_to_zero() {
    check(WHITE_MATED, &[(1, 0), (2, 0), (3, 0), (4, 0)]);
}
