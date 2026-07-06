//! Kinglet chess perft validation on the generic engine.
//!
//! The node counts below are pinned **and cross-checked against Fairy-Stockfish**
//! (FSF, `UCI_Variant kinglet`, a built-in — `extinction_variant()` base): every
//! `(depth, nodes)` pair here was produced identically by
//! `mcr::geometry::Kinglet::perft` and by FSF's `go perft` on the byte-identical
//! position. The `compare-fairy/` harness re-runs that head-to-head on demand (see
//! `compare-fairy/src/kinglet.rs`); this test pins the confirmed numbers so a
//! regression is caught without FSF present.
//!
//! ## What Kinglet chess is
//!
//! Standard chess movement with a **non-royal Commoner king** (no check, like
//! Extinction / Fog of War), **Commoner-only pawn promotion** (a pawn may promote
//! *only* to a non-royal King, never to Q/R/B/N), and the loss condition that a side
//! is finished the moment it holds **zero pawns**. Three things follow for perft:
//!
//! * **No check** raises the counts above standard chess exactly as in Extinction:
//!   a "checked" side keeps every otherwise-illegal move (a king may step into
//!   attack, a pin is ignored, capturing the enemy king is legal).
//! * **Commoner-only promotion** *lowers* the counts at any promotion node — a pawn
//!   on the last rank has a single promotion target where standard chess has four.
//!   From the start position this does not bite until depth 6+ (no pawn can reach
//!   the last rank sooner), so startpos depths 1-5 equal the pure no-check chess
//!   numbers.
//! * **The pawn-extinction terminal truncates** the tree at any node where a side
//!   has lost its last pawn — a captured or promoted last pawn ends the subtree,
//!   exactly as FSF adjudicates.
//!
//! Note Kinglet watches **only** pawns: unlike Extinction, losing a whole non-pawn
//! type (the last queen, or even the king) is *not* terminal, so mid-board tactical
//! positions count higher than under Extinction.
//!
//! ## FEN dialect
//!
//! mcr and FSF spell Kinglet chess with the **identical** standard-chess letters
//! (the king is `k`/`K`; its Commoner demotion is a rule, not a letter), so the FEN
//! is passed through unchanged.
//!
//! The cheap layers run as ordinary tests; the deep layers are `#[ignore]`d so
//! `cargo test` stays fast — run them with
//! `cargo test --release --test perft_kinglet -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Kinglet};

/// The Kinglet starting FEN, confirmed byte-for-byte against Fairy-Stockfish's
/// `UCI_Variant kinglet` / `position startpos`.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// The classic Kiwipete position (both queens on the board, rich tactics). Under
/// Kinglet the no-check movement lifts the counts above standard Kiwipete, and
/// unlike Extinction the counts are *not* truncated by non-pawn type losses.
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

/// A promotion position: White's a7 pawn is one step from the last rank. It
/// exercises Kinglet's **Commoner-only** promotion — a single promotion target, not
/// the four of standard chess (perft(1) = 8 here, standard chess = 11) — while both
/// sides still hold pawns so the position is not terminal. (Under Extinction this
/// FEN is instantly terminal: White fields no knight/bishop/rook/queen.)
const PROMOTION: &str = "4k3/P6p/8/8/8/8/p6P/4K3 w - - 0 1";

/// One capture from a pawn-extinction win: White's rook on b1 faces Black's only
/// pawn on b7 up the open b-file. `Rxb7` empties Black's Pawn type and wins.
const PAWN_EN_PRISE: &str = "4k3/1p6/8/8/8/8/P7/1R2K3 w - - 0 1";

/// An already-pawn-extinct position: White has no pawns (rank 2 empty), so it has
/// already lost — the node is terminal with zero continuations at every depth,
/// exactly as FSF's `go perft` returns 0.
const WHITE_NO_PAWNS: &str = "rnbqkbnr/pppppppp/8/8/8/8/8/RNBQKBNR w KQkq - 0 1";

/// Asserts the generic Kinglet perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF kinglet `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Kinglet::from_fen(fen).expect("valid Kinglet FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Kinglet perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    // Depths 1-4 equal standard chess through depth 3 and the no-check number at
    // depth 4 (197742, chess 197281); no pawn can promote or go extinct this
    // shallow, so the Kinglet twists have not yet appeared.
    check(STARTPOS, &[(1, 20), (2, 400), (3, 8902), (4, 197742)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    // Depth 5 is still the pure no-check number (4897256) — no promotion or pawn
    // extinction bites yet (a pawn needs six plies to reach the last rank). Depth 6
    // = 120921506, also the pure no-check count.
    check(STARTPOS, &[(5, 4897256), (6, 120921506)]);
}

// -- Kiwipete (FSF-confirmed) -----------------------------------------------

#[test]
fn kiwipete_cheap() {
    check(KIWIPETE, &[(1, 48), (2, 2049), (3, 98903)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn kiwipete_deep() {
    check(KIWIPETE, &[(4, 4194105)]);
}

// -- Commoner-only promotion (FSF-confirmed) --------------------------------

#[test]
fn promotion_commoner_only_cheap() {
    // perft(1) = 8: the a7 pawn has exactly one promotion (a8=Commoner) plus h2's
    // two pushes and the king's five steps — where standard chess would list four
    // promotions (perft(1) = 11).
    check(PROMOTION, &[(1, 8), (2, 64), (3, 583), (4, 5302)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn promotion_commoner_only_deep() {
    check(PROMOTION, &[(5, 51599)]);
}

// -- Pawn-extinction truncation (FSF-confirmed) -----------------------------

#[test]
fn pawn_en_prise() {
    // Rxb7 (among others) removes Black's last pawn, truncating that subtree; the
    // whole-node counts still match FSF.
    check(PAWN_EN_PRISE, &[(1, 16), (2, 102), (3, 1820), (4, 13807)]);
}

#[test]
fn already_pawn_extinct_truncates_to_zero() {
    // White has no pawns: the game is over, so FSF reports the node terminal and
    // `go perft N` is 0 at every depth. mcr's pawn-extinction truncation matches.
    check(WHITE_NO_PAWNS, &[(1, 0), (2, 0), (3, 0), (4, 0)]);
}
