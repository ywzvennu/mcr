//! Pawn back chess perft validation on the generic engine.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Pawnback::perft` and by Fairy-Stockfish (FSF, `UCI_Variant
//! pawnback`, a built-in — the pawn is FSF Betza `fbmWfceFifmnD` with a
//! `mobilityRegion` cap and an empty `nMoveRuleTypes`) on the byte-identical
//! position. The `compare-fairy/` harness re-runs that head-to-head on demand (see
//! `compare-fairy/src/pawnback.rs`); this test pins the confirmed numbers so a
//! regression is caught without FSF present.
//!
//! ## What pawn back chess is
//!
//! Standard chess with a pawn that may also step **backward**: besides the ordinary
//! forward push (double from the second rank), diagonal-forward capture, en passant,
//! and last-rank promotion, a pawn may make a single quiet step straight backward
//! (same file). It may never retreat onto its own first rank (White ranks 2..8,
//! Black ranks 1..7), so a home-rank pawn cannot step back. Because pawns can move
//! backward, a pawn move is not irreversible and does **not** reset the halfmove
//! clock — only captures and promotions do — so pawn shuffling can reach the
//! fifty-move draw.
//!
//! From the start the home-rank mobility cap forbids the only backward step
//! available, so startpos perft 1/2 equals standard chess's 20/400; the counts
//! diverge at depth 3 (9222 vs chess's 8902) once a pawn advances and gains a legal
//! retreat.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_pawnback -- --include-ignored`.

use mcr::geometry::{
    perft as gperft, Chess8x8, Pawnback, Square, WideEndReason, WideMoveKind, WideRole,
};

/// The pawn back starting FEN, confirmed byte-for-byte against Fairy-Stockfish's
/// `UCI_Variant pawnback` / `position startpos`.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// A midgame position with both armies developed — advanced pawns that may retreat,
/// home-rank pawns that may not, captures, and knight play in one tree.
const MIDGAME: &str = "rnbqkbnr/pp1ppppp/8/2p5/3P4/8/PPP1PPPP/RNBQKBNR w KQkq - 0 1";

/// A backward-move endgame: White's e4 pawn is advanced (it may retreat to e3),
/// White's c2 pawn is on its home rank (it may NOT retreat off rank 1), and Black's
/// d5 pawn is advanced (it may retreat to d6). Exercises the backward step and the
/// mobility cap together.
const BACKWARD: &str = "4k3/8/8/3p4/4P3/8/2P5/4K3 w - - 0 1";

/// A promotion endgame: White's c7 pawn and Black's g2 pawn are each one step from
/// the last rank, exercising promotion (which — unlike an ordinary pawn move — still
/// resets the halfmove clock).
const PROMOTION: &str = "8/2P1k3/8/8/8/8/4K1p1/8 w - - 0 1";

/// Both-armies pawn tangle: every pawn starts on its home rank (none can retreat
/// yet), so the tree opens like standard chess and diverges as pawns advance.
const PAWN_TANGLE: &str = "4k3/pppppppp/8/8/8/8/PPPPPPPP/4K3 w - - 0 1";

/// A forward double-step / en-passant position: White's e2 pawn double-steps to e4,
/// and Black's d4 pawn may capture it en passant onto e3 — the backward step never
/// affects en passant.
const EP: &str = "4k3/8/8/8/3p4/8/4P3/4K3 w - - 0 1";

/// Asserts the generic pawn back perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF pawnback `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Pawnback::from_fen(fen).expect("valid pawn back FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "pawn back perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

fn sq(file: u8, rank: u8) -> Square<Chess8x8> {
    Square::<Chess8x8>::from_file_rank(file, rank).unwrap()
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 20), (2, 400), (3, 9222), (4, 211739)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 5482869)]);
}

// -- Midgame (FSF-confirmed) ------------------------------------------------

#[test]
fn midgame_cheap() {
    check(MIDGAME, &[(1, 30), (2, 712), (3, 21765), (4, 571715)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_deep() {
    check(MIDGAME, &[(5, 18395494)]);
}

// -- Backward step + mobility cap (FSF-confirmed) ---------------------------

#[test]
fn backward_cheap() {
    check(
        BACKWARD,
        &[(1, 10), (2, 76), (3, 759), (4, 6356), (5, 63894)],
    );
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn backward_deep() {
    check(BACKWARD, &[(6, 538239)]);
}

// -- Promotion (FSF-confirmed) ----------------------------------------------

#[test]
fn promotion_cheap() {
    check(PROMOTION, &[(1, 12), (2, 132), (3, 1599), (4, 18406)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn promotion_deep() {
    check(PROMOTION, &[(5, 247913)]);
}

// -- Both-armies pawn tangle (FSF-confirmed) --------------------------------

#[test]
fn pawn_tangle_cheap() {
    check(PAWN_TANGLE, &[(1, 18), (2, 324), (3, 5946), (4, 109062)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn pawn_tangle_deep() {
    check(PAWN_TANGLE, &[(5, 2039858)]);
}

// -- Forward double step + en passant (FSF-confirmed) -----------------------

#[test]
fn en_passant() {
    check(EP, &[(1, 6), (2, 44), (3, 305), (4, 2575), (5, 19702)]);
}

// -- Targeted rule checks ---------------------------------------------------

/// An advanced pawn generates the empty square directly behind it as a quiet
/// backward move; a home-rank pawn generates none (the mobility cap).
#[test]
fn backward_step_generated_only_off_the_home_rank() {
    // White e4 (advanced) may retreat to e3; White c2 (home rank) may not retreat
    // onto c1.
    let pos = Pawnback::from_fen(BACKWARD).expect("valid FEN");
    let moves = pos.legal_moves();

    let e4 = sq(4, 3);
    assert!(
        moves.iter().any(|m| m.from::<Chess8x8>() == e4
            && m.to::<Chess8x8>() == sq(4, 2)
            && matches!(m.kind(), WideMoveKind::Quiet)),
        "advanced e4 pawn retreats to e3",
    );

    let c2 = sq(2, 1);
    assert!(
        !moves
            .iter()
            .any(|m| m.from::<Chess8x8>() == c2 && m.to::<Chess8x8>() == sq(2, 0)),
        "home-rank c2 pawn cannot retreat onto c1 (mobility cap)",
    );
}

/// Pawn shuffling reaches the fifty-move draw: because a pawn move does **not**
/// reset the halfmove clock in pawn back (only captures and promotions do), moving
/// pawns back and forth runs the clock to 100 plies and draws — something standard
/// chess never does, since there a pawn push zeroes the clock.
#[test]
fn pawnback_pawn_shuffle_reaches_move_rule_draw() {
    // Two facing pawns (blocked on each other) with the clock preloaded to 98: two
    // more pawn moves take it to 100.
    let pos = Pawnback::from_fen("4k3/8/8/3p4/3P4/8/8/4K3 w - - 98 60").expect("valid FEN");
    assert_eq!(pos.halfmove_clock(), 98);
    assert_eq!(pos.end_reason(), None, "not yet a draw at clock 98");

    // White retreats d4-d3: a pawn move, so the clock advances to 99 (a standard
    // pawn push would have reset it to 0 and never reach the draw).
    let d4d3 = pos
        .legal_moves()
        .into_iter()
        .find(|m| m.from::<Chess8x8>() == sq(3, 3) && m.to::<Chess8x8>() == sq(3, 2))
        .expect("d4-d3 backward step is legal");
    let after_white = pos.play(&d4d3);
    assert_eq!(
        after_white.halfmove_clock(),
        99,
        "pawn move did not reset the clock"
    );
    assert_eq!(after_white.end_reason(), None, "not yet a draw at clock 99");

    // Black retreats d5-d6: the clock reaches 100 and the fifty-move rule draws.
    let d5d6 = after_white
        .legal_moves()
        .into_iter()
        .find(|m| m.from::<Chess8x8>() == sq(3, 4) && m.to::<Chess8x8>() == sq(3, 5))
        .expect("d5-d6 backward step is legal");
    let drawn = after_white.play(&d5d6);
    assert_eq!(drawn.halfmove_clock(), 100);
    assert_eq!(
        drawn.end_reason(),
        Some(WideEndReason::MoveRule),
        "pawn shuffling reached the fifty-move draw",
    );
}

/// A promotion still resets the halfmove clock even though ordinary pawn moves do
/// not — matching Fairy-Stockfish, which zeroes the clock on every promotion.
#[test]
fn promotion_resets_the_clock() {
    // White c7 pawn promotes on c8; the clock (preloaded to 40) zeroes.
    let pos = Pawnback::from_fen("4k3/2P5/8/8/8/8/8/4K3 w - - 40 30").expect("valid FEN");
    let promo = pos
        .legal_moves()
        .into_iter()
        .find(|m| {
            m.from::<Chess8x8>() == sq(2, 6)
                && m.to::<Chess8x8>() == sq(2, 7)
                && matches!(
                    m.kind(),
                    WideMoveKind::Promotion {
                        role: WideRole::Queen,
                        ..
                    }
                )
        })
        .expect("c7-c8=Q is legal");
    assert_eq!(
        pos.play(&promo).halfmove_clock(),
        0,
        "promotion zeroes the clock"
    );
}
