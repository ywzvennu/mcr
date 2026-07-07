//! Los Alamos chess (6x6, no bishops) perft validation on the generic engine.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Losalamos::perft` and by Fairy-Stockfish (FSF, `UCI_Variant
//! losalamos`, a built-in) on the byte-identical position. The `compare-fairy/`
//! harness re-runs that head-to-head on demand (see `compare-fairy/src/losalamos.rs`);
//! this test pins the confirmed numbers so a regression is caught without FSF present.
//!
//! ## What Los Alamos chess is
//!
//! The 1956 MANIAC-I 6x6 chess: the standard chess army **minus the Bishop** (back
//! rank R N Q K N R) with six pawns a side, **no castling**, **no pawn double-step**,
//! and **no en passant**. Pawns promote on the far rank (rank 6 / rank 1) to Queen,
//! Rook, or Knight — never a Bishop. Every remaining piece moves as in standard
//! chess on the narrower board.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_losalamos -- --include-ignored`.

use mcr::geometry::{perft as gperft, Losalamos, Losalamos6x6};

/// The Los Alamos starting FEN, confirmed byte-for-byte against Fairy-Stockfish's
/// `UCI_Variant losalamos` / `position startpos`.
const STARTPOS: &str = "rnqknr/pppppp/6/6/PPPPPP/RNQKNR w - - 0 1";

/// A White-to-move midgame exercising the full promotion repertoire: the `b5` pawn
/// can push to promote (`b5b6`) or capture the `c6` rook to promote (`b5c6`), each
/// yielding a Queen, Rook, or Knight (never a Bishop). Kings are placed clear of
/// the pawn so the position is legal and the promotion tree is unobstructed.
const PROMO: &str = "2r2k/1P4/6/6/6/K5 w - - 0 1";

/// Asserts the generic Los Alamos perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `losalamos go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Losalamos::from_fen(fen).expect("valid Los Alamos FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Losalamos6x6, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Los Alamos perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(
        STARTPOS,
        &[(1, 10), (2, 100), (3, 1212), (4, 14332), (5, 191846)],
    );
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(6, 2549164)]);
}

// -- Promotion midgame: push-promotion and capture-promotion, Q/R/N only ----

#[test]
fn promo_cheap() {
    check(PROMO, &[(1, 9), (2, 65), (3, 548), (4, 4937)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn promo_deep() {
    check(PROMO, &[(5, 47878)]);
}
