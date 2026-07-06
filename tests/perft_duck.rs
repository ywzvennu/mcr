//! Duck chess (8x8) perft validation on the generic engine (issue #177) — the
//! first variant exercising the neutral-**Duck** blocker and the **two-part
//! move**, validating the duck-blocker / cross-product-movegen / no-check
//! mechanics end-to-end.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Duck::perft` and by Fairy-Stockfish (FSF, `UCI_Variant duck`)
//! running `go perft` on the byte-identical position (the FSF divide matches
//! mcr's move-for-move). The `compare-fairy/` harness re-runs that head-to-head
//! on demand (`compare-fairy/src/duck.rs`); this test pins the FSF-confirmed
//! numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `UCI_Variant duck` start position:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! ```
//!
//! The neutral Duck is **not** on the board at the start — it enters on the very
//! first move — so the opening FEN is the plain chess array. A startpos node has
//! `20 piece moves × 32 duck placements = 640` children (the depth-1 count
//! below). Once on the board the Duck renders as a `*` in the placement field
//! (the FSF dialect), as the midgame FENs here show.
//!
//! ## Confirmed semantics (all pinned move-for-move against FSF)
//!
//! * **Two-part ply.** Each ply is a piece move plus a Duck placement onto any
//!   square empty after the piece move (and different from where the Duck sits).
//!   FSF renders this as `<piecemove>,<duckfrom><duckto>` (e.g. `a2a3,a3a2`);
//!   mcr matches that string.
//! * **Duck blocks everything.** No piece may land on the Duck; it blocks slider
//!   rays; knights jump over it. It is neither side's piece (never captured).
//! * **No check.** The king is not royal: a king may move to / be left on an
//!   attacked square, en passant is never pin-filtered, and *capturing the enemy
//!   king is a legal move* — the resulting position (the captured side to move
//!   with no king) is terminal, so it has no children.
//!
//! The deep (depth-3) layers are `#[ignore]`d so `cargo test` stays fast — run
//! them with `cargo test --release --test perft_duck -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Duck, Square, WideMoveKind};

/// The Duck starting FEN, confirmed against Fairy-Stockfish's `UCI_Variant duck`.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// An open middlegame with the Duck on e5 and both sides able to castle.
/// Pinned against FSF (divide matches move-for-move).
const MID_OPEN: &str = "r1bqk2r/pppp1ppp/2n2n2/2b1*3/2B1P3/2N2N2/PPPP1PPP/R1BQK2R w KQkq - 0 1";

/// A middlegame with an en-passant target (`d6`) and the Duck on a4: exercises
/// en passant under the no-check rule. Pinned against FSF.
const MID_EP: &str = "rnbqkbnr/ppp1pppp/8/3pP3/*7/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1";

/// An en-passant whose capture "exposes" the white king on f5 to the a5 rook —
/// legal in Duck chess (no check), and FSF agrees. Also reaches king-capture
/// terminal nodes a few plies down. Pinned against FSF.
const EP_KING: &str = "4k3/8/8/r2pPK2/8/8/8/8 w - d6 0 1";

/// A king-and-pawn endgame with the Duck blocking on d5. Pinned against FSF.
const ENDGAME: &str = "8/2k5/8/3*4/8/5K2/4P3/8 w - - 0 1";

fn perft_at(fen: &str, depth: u32) -> u64 {
    let pos = Duck::from_fen(fen).expect("valid Duck FEN");
    gperft(&pos, depth)
}

#[test]
fn startpos_round_trips_and_opening_has_no_duck() {
    let pos = Duck::startpos();
    // The Duck is not on the board at the start; the FEN is the plain array.
    assert_eq!(pos.to_fen(), STARTPOS);
    assert_eq!(Duck::from_fen(STARTPOS).unwrap().to_fen(), STARTPOS);
}

#[test]
fn fen_round_trips_with_duck_and_ep() {
    // The `*` placement and the en-passant field survive a parse/render round-trip.
    for fen in [MID_OPEN, MID_EP, EP_KING, ENDGAME] {
        let pos = Duck::from_fen(fen).expect("valid Duck FEN");
        assert_eq!(pos.to_fen(), fen, "round-trip mismatch for {fen}");
    }
}

#[test]
fn first_move_is_a_two_part_ply() {
    // The very first ply enters the Duck; every move carries a duck placement and
    // renders FSF-style as `<piecemove>,<duckfrom><duckto>`.
    let pos = Duck::startpos();
    let moves = pos.legal_moves();
    assert_eq!(moves.len(), 640, "20 piece moves x 32 duck placements");
    for m in &moves {
        assert!(
            m.duck_to_index().is_some(),
            "every Duck move carries a duck placement"
        );
        let uci = m.to_uci::<Chess8x8>();
        assert!(uci.contains(','), "two-part rendering: {uci}");
    }
    // The a2a3 + duck-to-a2 move renders exactly as FSF prints it.
    assert!(moves.iter().any(|m| m.to_uci::<Chess8x8>() == "a2a3,a3a2"));
}

#[test]
fn duck_blocks_landing_and_is_not_captured() {
    // Duck on a6: the b8 knight may not land on it (FSF excludes b8a6).
    let pos = Duck::from_fen("rnbqkbnr/pppppppp/*7/8/8/P7/1PPPPPPP/RNBQKBNR b KQkq - 0 1")
        .expect("valid");
    let landed_on_duck = pos
        .legal_moves()
        .into_iter()
        .any(|m| m.to_index() == Square::<Chess8x8>::from_file_rank(0, 5).unwrap().index());
    assert!(!landed_on_duck, "no piece may land on the Duck's square");
}

#[test]
fn king_capture_is_legal_and_terminal() {
    // Black rook on a5, white king on f5: a5xf5 captures the king (legal in Duck
    // chess). After it, white has no king and therefore no legal move.
    let pos = Duck::from_fen("4k3/8/3P4/r4K2/8/8/8/*7 b - - 0 1").expect("valid");
    let cap = pos
        .legal_moves()
        .into_iter()
        .find(|m| {
            m.from_index() == Square::<Chess8x8>::from_file_rank(0, 4).unwrap().index()
                && m.to_index() == Square::<Chess8x8>::from_file_rank(5, 4).unwrap().index()
        })
        .expect("the king capture a5f5 is a legal move");
    assert!(matches!(cap.kind(), WideMoveKind::Capture));
    let after = pos.play(&cap);
    assert!(
        after.legal_moves().is_empty(),
        "the side whose king was captured has no legal move (terminal)"
    );
}

/// A **dense** placement position — only four empty squares (g5/h5/g4/h4) — whose
/// small duck-placement fan keeps the two-part cross-product tractable to depth 3
/// **without** `#[ignore]`, so the per-PR floor exercises the placement mechanic at
/// depth 3 (issue #501). The corpus's FSF-confirmed depth-3 counts are inherently
/// heavy (tens of millions of nodes) and stay release-gated below (#499); this
/// light case is an mcr self-consistent regression pin — the installed FSF build
/// enumerates only the base piece move (`go perft` depth 1 = 20, not 640), so it
/// cannot re-confirm duck-placement node counts in-tree (the placement counts were
/// FSF-confirmed against upstream `280626`; see `compare-fairy/src/duck.rs`).
const DENSE: &str =
    "rnbqkbnr/pppppppp/pppppppp/pppppp2/PPPPPP2/PPPPPPPP/PPPPPPPP/RNBQKBNR w - - 0 1";

#[test]
fn perft_dense_placement_to_depth_3() {
    assert_eq!(perft_at(DENSE, 1), 58);
    assert_eq!(perft_at(DENSE, 2), 3177);
    assert_eq!(perft_at(DENSE, 3), 206_355);
}

// --- FSF-confirmed perft counts ------------------------------------------

#[test]
fn perft_startpos_shallow() {
    assert_eq!(perft_at(STARTPOS, 1), 640);
    assert_eq!(perft_at(STARTPOS, 2), 379440);
}

#[test]
fn perft_midgames_shallow() {
    assert_eq!(perft_at(MID_OPEN, 1), 1121);
    assert_eq!(perft_at(MID_OPEN, 2), 1270166);
    assert_eq!(perft_at(MID_EP, 1), 931);
    assert_eq!(perft_at(MID_EP, 2), 736005);
    assert_eq!(perft_at(EP_KING, 1), 532);
    assert_eq!(perft_at(EP_KING, 2), 460692);
    assert_eq!(perft_at(ENDGAME, 1), 540);
    assert_eq!(perft_at(ENDGAME, 2), 254880);
}

#[test]
#[ignore = "deep perft; run with --include-ignored (use --release)"]
fn perft_startpos_deep() {
    assert_eq!(perft_at(STARTPOS, 3), 249921262);
}

#[test]
#[ignore = "deep perft; run with --include-ignored (use --release)"]
fn perft_midgames_deep() {
    assert_eq!(perft_at(MID_OPEN, 3), 1416644732);
    assert_eq!(perft_at(MID_EP, 3), 676408832);
    assert_eq!(perft_at(EP_KING, 3), 216775430);
    assert_eq!(perft_at(ENDGAME, 3), 135240480);
}
