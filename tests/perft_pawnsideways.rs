//! Pawn-sideways chess perft validation on the generic engine.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Pawnsideways::perft` and by Fairy-Stockfish (FSF, `UCI_Variant
//! pawnsideways`, a built-in — its pawn is FSF Betza `fsmWfceFifmnD`) on the
//! byte-identical position. The `compare-fairy/` harness re-runs that head-to-head
//! on demand (see `compare-fairy/src/pawnsideways.rs`); this test pins the confirmed
//! numbers so a regression is caught without FSF present.
//!
//! ## What pawn-sideways chess is
//!
//! Standard chess in which a pawn, besides its ordinary moves, may make a single
//! quiet **sideways** step (one square left or right along its own rank) onto an
//! empty square. The forward push, initial forward double step, diagonal capture,
//! en passant (off the forward double step only), and promotion are all standard.
//! A sideways step never captures, never promotes (it stays on the same rank),
//! gives no check, and creates no en-passant target.
//!
//! At the start no pawn can step sideways (each rank-2 pawn is flanked by another
//! pawn or the board edge), so perft 1/2 equal standard chess's 20/400; the extra
//! moves appear once pawns advance (perft 3 = 10022 vs chess's 8902).
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_pawnsideways -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Pawnsideways, Square, WideMoveKind};

/// The pawn-sideways starting FEN, confirmed byte-for-byte against
/// Fairy-Stockfish's `UCI_Variant pawnsideways` / `position startpos`.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// A midgame with both armies developed — pawns off the back rank now have empty
/// flanks, so sideways steps enter the tree alongside ordinary chess play.
const MIDGAME: &str = "rnbqkbnr/pp1ppppp/8/2p5/3P4/8/PPP1PPPP/RNBQKBNR w KQkq - 0 1";

/// A lone advanced pawn on d4 with both flanks empty: it may step sideways to c4 or
/// e4, or push forward to d5 — the plain sideways step in isolation.
const LONE_PAWN: &str = "4k3/8/8/8/3P4/8/8/4K3 w - - 0 1";

/// Two adjacent pawns c4/d4: the sideways step of one slides it off/onto a file,
/// blocking and unblocking the other's forward push — the block/unblock interaction.
const BLOCK_UNBLOCK: &str = "4k3/8/8/8/2PP4/8/8/4K3 w - - 0 1";

/// A pawn **pinned along its file** (White king e1, pawn e2, Black rook e8): it may
/// push forward along the pin line but **neither** sideways step stays on the file,
/// so no sideways move is generated — the pin forbids it.
const PIN_FILE: &str = "4r2k/8/8/8/8/8/4P3/4K3 w - - 0 1";

/// A pawn **pinned along its rank** (White king a4, pawn d4, Black rook h4): here
/// the forward push would leave the pin line (illegal), but **both** sideways steps
/// stay on the rank — the pin line — so the pinned pawn may step sideways to c4 or
/// e4. The reused `pin_line.contains(target)` guard handles this for free.
const PIN_RANK: &str = "7k/8/8/8/K2P3r/8/8/8 w - - 0 1";

/// A forward double-step en passant is untouched by the sideways rule: White's e2
/// pawn double-steps to e4 beside Black's d4 pawn, which then takes en passant.
const EP_ACTIVE: &str = "4k3/8/8/8/3p4/8/4P3/4K3 w - - 0 1";

/// Asserts the generic pawn-sideways perft equals each pinned `(depth, nodes)`
/// count. Every number here also matched FSF pawnsideways `go perft` on the same
/// position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Pawnsideways::from_fen(fen).expect("valid pawn-sideways FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "pawn-sideways perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

fn sq(file: u8, rank: u8) -> Square<Chess8x8> {
    Square::<Chess8x8>::from_file_rank(file, rank).unwrap()
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 20), (2, 400), (3, 10022), (4, 250145)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 7210366)]);
}

// -- Midgame (FSF-confirmed) ------------------------------------------------

#[test]
fn midgame_cheap() {
    check(MIDGAME, &[(2, 879), (3, 29696)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_deep() {
    check(MIDGAME, &[(4, 880984)]);
}

// -- Sideways step in isolation (FSF-confirmed) -----------------------------

#[test]
fn lone_pawn_sideways() {
    // d1 = 8: five king moves plus the pawn's c4 / d5 / e4.
    check(LONE_PAWN, &[(1, 8), (2, 40), (3, 365), (4, 2424)]);
}

// -- Block / unblock via a sideways step (FSF-confirmed) --------------------

#[test]
fn block_and_unblock() {
    check(BLOCK_UNBLOCK, &[(1, 9), (2, 45), (3, 490)]);
}

// -- A file-pinned pawn may NOT step sideways (FSF-confirmed) ----------------

#[test]
fn pinned_on_file_no_sideways() {
    check(PIN_FILE, &[(1, 6), (2, 87), (3, 632)]);
}

// -- A rank-pinned pawn MAY step sideways along the pin line (FSF-confirmed) -

#[test]
fn pinned_on_rank_may_step_sideways() {
    // d1 = 7: five king moves plus the pawn's two sideways steps (c4 / e4) that
    // stay on the rank — the pin line — while the forward push d5 is forbidden.
    check(PIN_RANK, &[(1, 7), (2, 91), (3, 745)]);
}

// -- Forward double-step en passant still works (FSF-confirmed) -------------

#[test]
fn forward_ep_unaffected() {
    check(EP_ACTIVE, &[(1, 8), (2, 65), (3, 509), (4, 4603)]);
}

/// Direct move-level check that the forward double step sets a standard en-passant
/// target and that a sideways step is a plain quiet move creating none.
#[test]
fn sideways_is_quiet_double_step_sets_ep() {
    let pos = Pawnsideways::from_fen(EP_ACTIVE).expect("valid FEN");
    // The e2-e4 double step is a DoublePawnPush and sets the ep target e3.
    let e2e4 = pos
        .legal_moves()
        .into_iter()
        .find(|m| {
            m.from::<Chess8x8>() == sq(4, 1)
                && m.to::<Chess8x8>() == sq(4, 3)
                && matches!(m.kind(), WideMoveKind::DoublePawnPush)
        })
        .expect("e2-e4 forward double step is legal");
    let after = pos.play(&e2e4);
    assert_eq!(
        after.ep_square(),
        Some(sq(4, 2)),
        "standard ep target on e3"
    );

    // A lone-pawn sideways step is a Quiet move that sets no ep target.
    let lone = Pawnsideways::from_fen(LONE_PAWN).expect("valid FEN");
    let side = lone
        .legal_moves()
        .into_iter()
        .find(|m| m.from::<Chess8x8>() == sq(3, 3) && m.to::<Chess8x8>() == sq(2, 3))
        .expect("d4-c4 sideways step is legal");
    assert!(
        matches!(side.kind(), WideMoveKind::Quiet),
        "sideways is quiet"
    );
    assert_eq!(
        lone.play(&side).ep_square(),
        None,
        "sideways sets no ep target"
    );
}
