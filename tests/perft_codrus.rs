//! Codrus perft validation on the generic engine.
//!
//! Every `(depth, nodes)` pair below was produced identically by
//! `mcr::geometry::Codrus::perft` and by Fairy-Stockfish's `go perft`
//! (`UCI_Variant codrus`, a built-in — `codrus_variant()`, `variant.cpp:440`) on the
//! byte-identical position. The `compare-fairy/` harness re-runs that head-to-head
//! on demand (see `compare-fairy/src/codrus.rs`).
//!
//! ## What codrus is
//!
//! Giveaway restricted so that only the **king** is the watched extinction
//! type, and pawns promote to Q/R/B/N only (no king-promotion). Movement is
//! byte-identical to giveaway until a pawn reaches the last rank: giveaway offers a
//! fifth (king) promotion there, codrus does not — so the two diverge only where a
//! promotion is reachable (Kiwipete depth 4: codrus `3836`, giveaway `3872`;
//! startpos depth 6: codrus `46263517`, giveaway `46264162`).
//!
//! The deep layers are `#[ignore]`d — run with
//! `cargo test --release --test perft_codrus -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Codrus};

/// The codrus starting FEN — the standard array with castling rights (codrus keeps
/// giveaway's castling), confirmed against FSF `UCI_Variant codrus`.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// Kiwipete — a pawn promotes within depth 4, so codrus (`3836`) undercuts giveaway
/// (`3872`) by exactly the king-promotion moves.
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

/// Black (to move) has lost its king — in codrus that is a win, so the node is
/// terminal and `go perft N` is 0 at every depth.
const BLACK_NO_KING: &str = "7R/8/8/8/8/8/8/7K b - - 0 1";

fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Codrus::from_fen(fen).expect("valid codrus FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "codrus perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
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
    check(STARTPOS, &[(5, 2732672), (6, 46263517)]);
}

#[test]
fn kiwipete_cheap() {
    check(KIWIPETE, &[(1, 8), (2, 62), (3, 487)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn kiwipete_deep() {
    // Depth 4: codrus 3836, exactly giveaway's 3872 minus the king-promotions.
    check(KIWIPETE, &[(4, 3836)]);
}

#[test]
fn lost_king_truncates_to_zero() {
    check(BLACK_NO_KING, &[(1, 0), (2, 0), (3, 0), (4, 0)]);
}
