//! Fog of War / Dark Chess (8x8) perft validation on the generic engine
//! (issue #277).
//!
//! Fog of War is standard chess movement with **no check**: the king is an
//! ordinary, capturable piece — a side may move into "check", may leave its king
//! attacked, and *capturing the enemy king is a legal move* (the resulting
//! node, the captured side to move with no king, is terminal). Castling is never
//! restricted by attacked squares. The move generator is therefore deterministic
//! and its node counts are perft-validatable.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::FogOfWar::perft` and by Fairy-Stockfish (FSF) running
//! `go perft` on the byte-identical position. FSF has no built-in `fogofwar`, so
//! the harness defines it in `variants.ini` (inheriting the built-in `chess`):
//!
//! ```ini
//! [fogofwar:chess]
//! king = -
//! commoner = k
//! castlingKingPiece = k
//! extinctionValue = loss
//! extinctionPieceTypes = k
//! ```
//!
//! which yields a non-royal, capturable king (`extinctionValue = loss` on the
//! commoner king makes its capture terminal; the `commoner` is not royal, so FSF
//! applies no check/pin/king-danger filter). The `compare-fairy/` harness
//! re-runs the head-to-head on demand (`compare-fairy/src/fogofwar.rs`, which
//! bundles that INI snippet and loads it via `setoption name VariantPath`); this
//! test pins the FSF-confirmed numbers so a regression is caught even without FSF
//! present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `UCI_Variant fogofwar` start position — plain chess:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! ```
//!
//! ## Confirmed semantics (all pinned against FSF)
//!
//! * **No check.** The startpos counts already diverge from standard chess once a
//!   "check" first appears: depth 4 is `197742` (chess: `197281`) and depth 5 is
//!   `4897256` (chess: `4865609`), because a "checked" side keeps every
//!   pseudo-legal move instead of only the evasions.
//! * **King capture is terminal.** From `k7/8/8/8/8/8/8/R6K w - - 0 1`, the move
//!   `Rxa8` (capturing the black king) is legal and contributes `0` at the next
//!   ply (the king-capture test below pins this).
//! * **Castling ignores attacked squares.** The castling-rich position keeps
//!   `O-O` / `O-O-O` even when the king passes through an attacked square.

use mcr::geometry::{perft as gperft, Chess8x8, FogOfWar, Square, WideMoveKind};
use mcr::Color;

/// The Fog of War starting FEN, confirmed against FSF's `UCI_Variant fogofwar`.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// The classic "Kiwipete" middlegame — rich in captures, castling, and pins
/// (which Fog of War does *not* honor). Pinned against FSF.
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

/// Black to move with its king attacked by the white queen on h5 (after
/// `1.e4 f5 2.Qh5+`). In standard chess Black has one legal reply; in Fog of War
/// Black keeps every pseudo-legal move (the king is not royal). Pinned vs FSF.
const IN_CHECK: &str = "rnbqkbnr/ppppp1pp/8/5p1Q/4P3/8/PPPP1PPP/RNB1KBNR b KQkq - 1 2";

/// Both sides may castle either way with no pieces between king and rooks — and
/// no check to restrict it. Exercises every castling target. Pinned vs FSF.
const CASTLING: &str = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";

fn perft_at(fen: &str, depth: u32) -> u64 {
    let pos = FogOfWar::from_fen(fen).expect("valid Fog of War FEN");
    gperft(&pos, depth)
}

#[test]
fn startpos_round_trips() {
    let pos = FogOfWar::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    assert_eq!(FogOfWar::from_fen(STARTPOS).unwrap().to_fen(), STARTPOS);
}

#[test]
fn king_capture_is_legal_and_terminal() {
    // Black king on a8, white rook on a1: Rxa8 captures the king (legal in Fog
    // of War). After it, Black has no king and therefore no legal move.
    let pos = FogOfWar::from_fen("k7/8/8/8/8/8/8/R6K w - - 0 1").expect("valid");
    let a1 = Square::<Chess8x8>::from_file_rank(0, 0).unwrap().index();
    let a8 = Square::<Chess8x8>::from_file_rank(0, 7).unwrap().index();
    let cap = pos
        .legal_moves()
        .into_iter()
        .find(|m| m.from_index() == a1 && m.to_index() == a8)
        .expect("the king capture Rxa8 is a legal move");
    assert!(matches!(cap.kind(), WideMoveKind::Capture));
    let after = pos.play(&cap);
    assert!(
        after.legal_moves().is_empty(),
        "the side whose king was captured has no legal move (terminal)"
    );
}

#[test]
fn may_move_into_and_leave_check() {
    // White king e1, black rook e8 "pins" the rook on e2. In standard chess the
    // e2 rook may not leave the e-file; in Fog of War it moves freely (no pins,
    // no self-check). The off-file move Re2-d2 must be present.
    let pos = FogOfWar::from_fen("k3r3/8/8/8/8/8/4R3/4K3 w - - 0 1").expect("valid");
    let e2 = Square::<Chess8x8>::from_file_rank(4, 1).unwrap().index();
    let d2 = Square::<Chess8x8>::from_file_rank(3, 1).unwrap().index();
    assert!(
        pos.legal_moves()
            .into_iter()
            .any(|m| m.from_index() == e2 && m.to_index() == d2),
        "a 'pinned' piece moves freely in Fog of War"
    );
}

// --- FSF-confirmed perft counts ------------------------------------------

#[test]
fn perft_startpos_shallow() {
    assert_eq!(perft_at(STARTPOS, 1), 20);
    assert_eq!(perft_at(STARTPOS, 2), 400);
    assert_eq!(perft_at(STARTPOS, 3), 8902);
    // Depth 4 already diverges from standard chess (197281): a "checked" side
    // keeps every pseudo-legal move, not just the evasions.
    assert_eq!(perft_at(STARTPOS, 4), 197_742);
}

#[test]
fn perft_midgames_shallow() {
    assert_eq!(perft_at(KIWIPETE, 1), 48);
    assert_eq!(perft_at(KIWIPETE, 2), 2049);
    assert_eq!(perft_at(KIWIPETE, 3), 98903);
    assert_eq!(perft_at(IN_CHECK, 1), 20);
    assert_eq!(perft_at(IN_CHECK, 2), 819);
    assert_eq!(perft_at(IN_CHECK, 3), 17817);
    assert_eq!(perft_at(CASTLING, 1), 26);
    assert_eq!(perft_at(CASTLING, 2), 613);
    assert_eq!(perft_at(CASTLING, 3), 15950);
}

#[test]
#[ignore = "deep perft; run with --include-ignored (use --release)"]
fn perft_startpos_deep() {
    assert_eq!(perft_at(STARTPOS, 5), 4_897_256);
    assert_eq!(perft_at(STARTPOS, 6), 120_909_363);
}

#[test]
#[ignore = "deep perft; run with --include-ignored (use --release)"]
fn perft_midgames_deep() {
    assert_eq!(perft_at(KIWIPETE, 4), 4_206_146);
    assert_eq!(perft_at(IN_CHECK, 4), 701_683);
    assert_eq!(perft_at(CASTLING, 4), 401_191);
}

// --- Visibility (the fog) — a view layer, not part of perft --------------

/// Helper: does `color` see the square at `(file, rank)` in `fen`?
fn sees(fen: &str, color: Color, file: u8, rank: u8) -> bool {
    let pos = FogOfWar::from_fen(fen).expect("valid Fog of War FEN");
    let sq = Square::<Chess8x8>::from_file_rank(file, rank).unwrap();
    pos.visible_squares(color).contains(sq)
}

#[test]
fn visibility_includes_own_pieces_and_attacks_hides_the_rest() {
    // White: Ra1, Ke1. Black: Ke8. White sees its own pieces, the rook's file
    // and rank (up to its own king on e1), and the king's neighborhood — and
    // nothing it does not attack.
    let fen = "4k3/8/8/8/8/8/8/R3K3 w - - 0 1";
    // Own pieces are always visible.
    assert!(sees(fen, Color::White, 0, 0), "own rook a1 visible");
    assert!(sees(fen, Color::White, 4, 0), "own king e1 visible");
    // Rook rays.
    assert!(sees(fen, Color::White, 0, 4), "rook sees a5 up the file");
    assert!(sees(fen, Color::White, 3, 0), "rook sees d1 along the rank");
    // King neighborhood.
    assert!(sees(fen, Color::White, 5, 1), "king sees f2");
    // The far enemy king and empty distant squares are fogged.
    assert!(!sees(fen, Color::White, 4, 7), "black king e8 is fogged");
    assert!(!sees(fen, Color::White, 7, 4), "empty h5 is fogged");
}

#[test]
fn visibility_reveals_a_capturable_enemy_but_not_beyond() {
    // White Ra1; a black knight on a5 the rook attacks through empty a2..a4.
    // White sees a5 (the capturable enemy) but not a6 behind it.
    let fen = "4k3/8/8/n7/8/8/8/R3K3 w - - 0 1";
    assert!(
        sees(fen, Color::White, 0, 4),
        "rook sees the enemy knight a5"
    );
    assert!(
        !sees(fen, Color::White, 0, 5),
        "the square a6 behind the blocker is fogged"
    );
}

#[test]
fn visibility_is_per_side_asymmetric() {
    // The fog is per player: Black, with only its king on e8, cannot see the
    // white rook sitting on a1 (it does not attack a1), while White does.
    let fen = "4k3/8/8/8/8/8/8/R3K3 w - - 0 1";
    assert!(sees(fen, Color::White, 0, 0), "White sees its own rook a1");
    assert!(
        !sees(fen, Color::Black, 0, 0),
        "Black cannot see the white rook on a1 (fog)"
    );
    // Black does see its own king's neighborhood.
    assert!(
        sees(fen, Color::Black, 3, 6),
        "Black sees d7 (king neighbor)"
    );
}

#[test]
fn startpos_visibility_stops_short_of_the_enemy() {
    // At the start White's pieces reach only ranks 1-3; ranks 5-8 are fogged.
    assert!(
        sees(STARTPOS, Color::White, 4, 2),
        "White sees e3 (pawn/knight)"
    );
    assert!(
        !sees(STARTPOS, Color::White, 4, 4),
        "e5 is fogged at the start"
    );
    assert!(
        !sees(STARTPOS, Color::White, 4, 7),
        "the enemy back rank is fogged"
    );
}
