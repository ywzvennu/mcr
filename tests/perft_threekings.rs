//! Three kings chess perft validation on the generic engine.
//!
//! The node counts below are pinned **and cross-checked against Fairy-Stockfish**
//! (FSF, `UCI_Variant threekings`, a built-in — `threekings_variant()`): every
//! `(depth, nodes)` pair here was produced identically by
//! `mcr::geometry::Threekings::perft` and by FSF's `go perft` on the byte-identical
//! position. The `compare-fairy/` harness re-runs that head-to-head on demand (see
//! `compare-fairy/src/threekings.rs`); this test pins the confirmed numbers so a
//! regression is caught without FSF present.
//!
//! ## What Three kings chess is
//!
//! Standard chess movement with **three non-royal Commoner kings per side** (no
//! check, like Extinction / Fog of War), plus the loss condition that a side is
//! finished the moment its king count drops to two — i.e. losing **any one** of its
//! three kings is a loss. Two things follow for perft:
//!
//! * **No check** lifts the counts relative to standard chess: a "checked" side
//!   keeps every otherwise-illegal move (a king may step into attack, a pin is
//!   ignored, capturing an enemy king is legal). The start array also differs — no
//!   rooks, three kings — so the counts are unlike standard chess from move 1.
//! * **The extinction terminal truncates** the tree at any node where a side has
//!   been reduced to two kings — a king capture ends that subtree, exactly as FSF
//!   adjudicates.
//!
//! ## Confirmed starting FEN
//!
//! `knbqkbnk/pppppppp/8/8/8/8/PPPPPPPP/KNBQKBNK w - - 0 1` — kings (Commoners
//! spelled `k`/`K`) on files a/e/h, no rooks, no castling rights. mcr and FSF spell
//! every position with the identical standard-chess letters, so the FEN passes
//! through unchanged.
//!
//! The cheap layers run as ordinary tests; the deep layers are `#[ignore]`d so
//! `cargo test` stays fast — run them with
//! `cargo test --release --test perft_threekings -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Threekings};

/// The Three kings starting FEN, confirmed byte-for-byte against Fairy-Stockfish's
/// `UCI_Variant threekings` / `position startpos`.
const STARTPOS: &str = "knbqkbnk/pppppppp/8/8/8/8/PPPPPPPP/KNBQKBNK w - - 0 1";

/// A midgame position one move from a king capture: the white queen on e5 faces the
/// black king on e8 down the open e-file, so `Qxe8` (a legal king capture) reduces
/// Black to two kings and truncates that subtree. Exercises the multi-king movegen
/// and the extinction terminal in the same tree.
const KING_CAPTURE: &str = "kn1qk1nk/pppp1ppp/8/4Q3/8/8/PPPP1PPP/KNB1KBNK w - - 0 1";

/// Asserts the generic Three kings perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `threekings go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Threekings::from_fen(fen).expect("valid Three kings FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Three kings perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 20), (2, 400), (3, 8942), (4, 199514)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 4971357)]);
}

// -- King-capture midgame (FSF-confirmed) -----------------------------------

#[test]
fn king_capture_cheap() {
    // Qxe8 captures a black king (a leaf: Black then has two kings and no moves),
    // so the extinction truncation bites inside this tree.
    check(KING_CAPTURE, &[(1, 47), (2, 1179), (3, 52892)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn king_capture_deep() {
    check(KING_CAPTURE, &[(4, 1407623), (5, 60978533)]);
}
