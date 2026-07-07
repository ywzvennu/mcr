//! Extinction chess perft validation on the generic engine.
//!
//! The node counts below are pinned **and cross-checked against Fairy-Stockfish**
//! (FSF, `UCI_Variant extinction`, a built-in — `extinction_variant()`,
//! `variant.cpp:449`): every `(depth, nodes)` pair here was produced identically
//! by `mcr::geometry::Extinction::perft` and by FSF's `go perft` on the
//! byte-identical position. The `compare-fairy/` harness re-runs that head-to-head
//! on demand (see `compare-fairy/src/extinction.rs`); this test pins the confirmed
//! numbers so a regression is caught without FSF present.
//!
//! ## What Extinction chess is
//!
//! Standard chess movement with a **non-royal Commoner king** (no check, like Fog
//! of War), plus the loss condition that a side is finished the moment any one of
//! its piece *types* is wiped out. Two things follow for perft:
//!
//! * **No check** raises the counts above standard chess: the startpos already
//!   diverges at depth 4 (`197742`, chess `197281`), because a "checked" side keeps
//!   every otherwise-illegal move (a king may step into attack, a pin is ignored,
//!   capturing the enemy king is legal).
//! * **The extinction terminal truncates** the tree at any node where a side has
//!   lost its last piece of a type — so from depth 5 the counts fall **below** the
//!   pure no-check numbers (`4896744`, no-check `4897256`) as the first
//!   type-emptying captures (a last queen, a king) end their subtrees, exactly as
//!   FSF adjudicates.
//!
//! ## FEN dialect
//!
//! mcr and FSF spell Extinction chess with the **identical** standard-chess letters
//! (the king is `k`/`K`; its Commoner demotion is a rule, not a letter), so the
//! FEN is passed through unchanged.
//!
//! The cheap layers run as ordinary tests; the deep layers are `#[ignore]`d so
//! `cargo test` stays fast — run them with
//! `cargo test --release --test perft_extinction -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Extinction};

/// The Extinction starting FEN, confirmed byte-for-byte against Fairy-Stockfish's
/// `UCI_Variant extinction` / `position startpos`.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// The classic Kiwipete position (both queens on the board, rich tactics). Under
/// Extinction chess the no-check movement lifts the counts above standard Kiwipete.
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

/// An already-extinct position: Black is missing its queen (all other types
/// present), so it has already lost — the node is terminal with zero continuations
/// at every depth, exactly as FSF's `go perft` returns 0.
const BLACK_NO_QUEEN: &str = "rnb1kbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// Asserts the generic Extinction perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF extinction `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Extinction::from_fen(fen).expect("valid Extinction FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Extinction perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    // Depths 1-3 equal standard chess (no "check" has appeared yet); depth 4 is the
    // no-check number 197742 (chess 197281) — still above chess, no extinction yet.
    check(STARTPOS, &[(1, 20), (2, 400), (3, 8902), (4, 197742)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    // Depth 5 is the first depth where the extinction truncation bites: 4896744,
    // *below* the pure no-check 4897256, as the first last-of-a-type captures end
    // their subtrees. Depth 6 = 120870859.
    check(STARTPOS, &[(5, 4896744), (6, 120870859)]);
}

// -- Kiwipete (FSF-confirmed) -----------------------------------------------

#[test]
fn kiwipete_cheap() {
    check(KIWIPETE, &[(1, 48), (2, 2049), (3, 98456)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn kiwipete_deep() {
    check(KIWIPETE, &[(4, 4183740)]);
}

// -- Extinction truncation (FSF-confirmed) ----------------------------------

#[test]
fn already_extinct_truncates_to_zero() {
    // Black has lost its queen: the game is over, so FSF reports the node terminal
    // and `go perft N` is 0 at every depth. mcr's extinction truncation matches.
    check(BLACK_NO_QUEEN, &[(1, 0), (2, 0), (3, 0), (4, 0)]);
}
