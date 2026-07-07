//! Gardner minichess (5x5 chess) perft validation on the generic engine —
//! standard chess pieces on the five-by-five [`Minishogi5x5`] board, with no
//! castling, no pawn double step, and no en passant.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Gardner::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant gardner`) running `go perft` on the byte-identical position. The
//! `compare-fairy/` differential fuzzer re-runs the head-to-head on demand
//! (`--difffuzz --variant gardner`); this test pins the FSF-confirmed numbers so a
//! regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! ```text
//! rnbqk/ppppp/5/PPPPP/RNBQK w - - 0 1
//! ```
//!
//! mcr and FSF spell every position with the identical standard-chess letters on a
//! 5x5 grid — no dialect rewrite. The opening `perft(1)` is `7`: the five single
//! pawn pushes plus the two knight hops (no double step, no castle).
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_gardner -- --include-ignored`.

use mcr::geometry::{perft as gperft, Gardner, Minishogi5x5};

/// The Gardner minichess starting FEN, confirmed against FSF's
/// `UCI_Variant gardner`. No castling rights; no en-passant target.
const STARTPOS: &str = "rnbqk/ppppp/5/PPPPP/RNBQK w - - 0 1";

/// A natural midgame position (both knights swapped off, a white b-pawn one step
/// from promotion, open lines) reached from the opening — no double step, no
/// castle, no en passant anywhere in its tree. Its counts were confirmed
/// square-for-square against FSF `gardner go perft`.
const MIDGAME: &str = "1nbqk/rPp1p/2p2/P2PP/RNBQK w - - 0 5";

/// Asserts the generic Gardner perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `gardner go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Gardner::from_fen(fen).expect("valid Gardner FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Minishogi5x5, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Gardner perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

#[test]
fn startpos_cheap() {
    check(
        STARTPOS,
        &[(1, 7), (2, 53), (3, 506), (4, 4775), (5, 52512)],
    );
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(6, 572874)]);
}

#[test]
fn midgame_with_promotion() {
    check(MIDGAME, &[(1, 15), (2, 212), (3, 3152), (4, 39955)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_deep() {
    check(MIDGAME, &[(5, 615631)]);
}
