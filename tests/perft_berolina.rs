//! Berolina chess perft validation on the generic engine.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Berolina::perft` and by Fairy-Stockfish (FSF, `UCI_Variant
//! berolina`, a built-in — the Berolina pawn is FSF Betza `mfFcfeWimfnA`) on the
//! byte-identical position. The `compare-fairy/` harness re-runs that head-to-head
//! on demand (see `compare-fairy/src/berolina.rs`); this test pins the confirmed
//! numbers so a regression is caught without FSF present.
//!
//! ## What Berolina chess is
//!
//! Standard chess with an **inverted pawn**: it *moves* one square diagonally
//! forward (two along the diagonal from the second rank — a *lame* jump, blocked if
//! the intervening square is occupied) and *captures* one square straight forward.
//! En passant applies to the diagonal double step, and promotion is standard
//! (Q/R/B/N). Because the pawns move diagonally from the first ply, the counts
//! diverge from standard chess immediately (startpos perft 1/2/3 = 30/900/28328,
//! versus chess's 20/400/8902).
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_berolina -- --include-ignored`.

use mcr::geometry::{perft as gperft, Berolina, Chess8x8, Square, WideMoveKind};

/// The Berolina starting FEN, confirmed byte-for-byte against Fairy-Stockfish's
/// `UCI_Variant berolina` / `position startpos`.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// A midgame position with both armies developed — diagonal advances, straight
/// captures, and knight play in one tree.
const MIDGAME: &str = "rnbqkbnr/pp1ppppp/8/2p5/3P4/8/PPP1PPPP/RNBQKBNR w KQkq - 0 1";

/// A promotion endgame: White's c7 pawn and Black's g2 pawn are each one diagonal
/// step from the last rank, exercising promotion by the diagonal move.
const PROMOTION: &str = "8/2P1k3/8/8/8/8/4K1p1/8 w - - 0 1";

/// An en-passant-active position (reached from `c2` double-stepping diagonally to
/// `e4` through `d3`): the skipped square `d3` is the ep target and Black's `d4`
/// pawn may capture the `e4` pawn en passant. mcr writes the single skipped square;
/// the diagonally offset victim (`e4`) is recovered on parse.
const EP_ACTIVE: &str = "4k3/8/8/8/3pP3/8/8/4K3 b - d3 0 1";

/// Asserts the generic Berolina perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF berolina `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Berolina::from_fen(fen).expect("valid Berolina FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Berolina perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

fn sq(file: u8, rank: u8) -> Square<Chess8x8> {
    Square::<Chess8x8>::from_file_rank(file, rank).unwrap()
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 30), (2, 900), (3, 28328), (4, 882717)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 29119802)]);
}

// -- Midgame (FSF-confirmed) ------------------------------------------------

#[test]
fn midgame_cheap() {
    check(MIDGAME, &[(1, 34), (2, 953), (3, 32601)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_deep() {
    check(MIDGAME, &[(4, 975032)]);
}

// -- Promotion by the diagonal move (FSF-confirmed) -------------------------

#[test]
fn promotion_cheap() {
    check(PROMOTION, &[(1, 16), (2, 215), (3, 2876)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn promotion_deep() {
    check(PROMOTION, &[(4, 37603)]);
}

// -- En passant, parsed from a FEN (FSF-confirmed) --------------------------

#[test]
fn en_passant_from_fen() {
    // The ep victim (e4) is recovered from the single skipped square (d3) on parse;
    // Black's d4 pawn takes it straight-forward en passant onto d3.
    check(EP_ACTIVE, &[(1, 8), (2, 52), (3, 419)]);
}

// -- En passant, reached by the double step (FSF-confirmed) -----------------

/// The ambiguous-victim case: a diagonal double step lands beside a *pre-existing*
/// enemy pawn on the other diagonal of the skipped square, so the exact victim is
/// not derivable from the skipped square alone. Reaching the ep by playing the move
/// records the exact victim, matching FSF move-for-move.
#[test]
fn en_passant_ambiguous_via_move() {
    // Black to move, e7 pawn double-steps diagonally e7-g5 (through f6), landing
    // beside the pre-existing e5 pawn. White's f5 pawn then may take g5 en passant
    // onto f6 — removing g5 (the double-stepper), never e5.
    let base = Berolina::from_fen("4k3/4p3/8/4pP2/8/8/8/4K3 b - - 0 1").expect("valid FEN");
    let e7g5 = base
        .legal_moves()
        .into_iter()
        .find(|m| {
            m.from::<Chess8x8>() == sq(4, 6)
                && m.to::<Chess8x8>() == sq(6, 4)
                && matches!(m.kind(), WideMoveKind::DoublePawnPush)
        })
        .expect("e7-g5 diagonal double step is legal");
    let pos = base.play(&e7g5);
    assert_eq!(
        pos.ep_square(),
        Some(sq(5, 5)),
        "ep target is the skipped f6"
    );
    for &(depth, expected) in &[(1u32, 8u64), (2, 68), (3, 547)] {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Berolina ambiguous-ep perft({depth}): expected {expected} (FSF-confirmed), got {got}"
        );
    }
}
