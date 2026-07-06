//! Grasshopper chess perft validation on the generic engine.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Grasshopper::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant grasshopper`, a built-in — the Grasshopper is FSF Betza `gQ`) on
//! the byte-identical position. The `compare-fairy/` harness re-runs that
//! head-to-head on demand (see `compare-fairy/src/grasshopper.rs`); this test pins
//! the confirmed numbers so a regression is caught without FSF present.
//!
//! ## What Grasshopper chess is
//!
//! Standard chess with a full rank of **Grasshoppers** (a queen-line hopper: it
//! lands on the single square immediately beyond the first piece — the "hurdle" —
//! it meets along any queen direction) in front of the pawns, which start on the
//! third rank. Pawns have **no double step** (hence no en passant). Promotion adds
//! the Grasshopper to the Q/R/B/N set. mcr spells the Grasshopper with its
//! fourth-tier overflow token `***j` / `***J`; FSF spells it `g` / `G`.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_grasshopper -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Grasshopper};

/// The Grasshopper-chess starting FEN in mcr's dialect (`***j`/`***J` grasshoppers
/// on ranks 2 / 7, pawns on ranks 3 / 6), byte-for-byte equivalent to FSF's
/// `UCI_Variant grasshopper` / `position startpos`
/// (`rnbqkbnr/gggggggg/pppppppp/8/8/PPPPPPPP/GGGGGGGG/RNBQKBNR w KQkq - 0 1`).
const STARTPOS: &str = "rnbqkbnr/\
    ***j***j***j***j***j***j***j***j/\
    pppppppp/8/8/PPPPPPPP/\
    ***J***J***J***J***J***J***J***J/\
    RNBQKBNR w KQkq - 0 1";

/// A developed midgame (startpos + `g2e4 g7e5` in FSF terms): the two central
/// grasshoppers have hopped to e4 / e5, so every side has a dense field of
/// grasshopper quiet hops and over-pawn captures. FSF:
/// `rnbqkbnr/gggggg1g/pppppppp/4g3/4G3/PPPPPPPP/GGGGGG1G/RNBQKBNR w KQkq - 2 2`.
const MIDGAME: &str = "rnbqkbnr/\
    ***j***j***j***j***j***j1***j/\
    pppppppp/4***j3/4***J3/PPPPPPPP/\
    ***J***J***J***J***J***J1***J/\
    RNBQKBNR w KQkq - 2 2";

/// A White-to-move **grasshopper check**: the black grasshopper on e4 checks the
/// white king on e1 by hopping over the white pawn on e2. White must move the king,
/// or resolve the check by moving / interposing so the hop no longer lands on e1 —
/// including moving the **hurdle** pawn (`e2e3`), an evasion only the per-move
/// king-safety verify path sees. FSF: `4k3/8/8/8/4g3/8/4P3/4K3 w - - 0 1`.
const CHECK: &str = "4k3/8/8/8/4***j3/8/4P3/4K3 w - - 0 1";

/// A White-to-move **capture beyond a hurdle**: the white grasshopper on d4 hops
/// over the white pawn on d5 (the hurdle) and captures the black rook on d6. FSF:
/// `4k3/8/3r4/3P4/3G4/8/8/4K3 w - - 0 1`.
const CAPTURE: &str = "4k3/8/3r4/3P4/3***J4/8/8/4K3 w - - 0 1";

/// Asserts the generic Grasshopper perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `grasshopper` `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Grasshopper::from_fen(fen).expect("valid Grasshopper FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Grasshopper perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 28), (2, 782), (3, 22314), (4, 635298)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 19157556)]);
}

// -- Midgame: dense grasshopper quiet hops and over-pawn captures -----------

#[test]
fn midgame_cheap() {
    check(MIDGAME, &[(1, 29), (2, 811), (3, 24234)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_deep() {
    check(MIDGAME, &[(4, 725865)]);
}

// -- Grasshopper check: move-king / interpose / move-the-hurdle evasions -----

#[test]
fn check_position() {
    check(CHECK, &[(1, 5), (2, 30), (3, 160), (4, 1261)]);
}

// -- Capture beyond a hurdle ------------------------------------------------

#[test]
fn capture_position() {
    check(CAPTURE, &[(2, 80), (3, 672), (4, 10041)]);
}

// -- Targeted grasshopper-mechanics tests -----------------------------------

/// The UCI strings of every legal move whose origin is `from` (e.g. `"d4"`).
fn moves_from(fen: &str, from: &str) -> Vec<String> {
    let pos = Grasshopper::from_fen(fen).expect("valid Grasshopper FEN");
    pos.legal_moves()
        .iter()
        .map(|m| m.to_uci::<Chess8x8>())
        .filter(|u| u.starts_with(from))
        .collect()
}

#[test]
fn grasshopper_hops_immediately_beyond_a_hurdle_quiet() {
    // White grasshopper d4, hurdle (white pawn) d5, empty beyond: its only move on
    // the file is the single quiet hop to d6 — never d5 (the hurdle) and never d7+.
    let fen = "4k3/8/8/3P4/3***J4/8/8/4K3 w - - 0 1";
    let mvs = moves_from(fen, "d4");
    assert_eq!(mvs, vec!["d4d6".to_string()], "the lone hop is d4-d6");
}

#[test]
fn grasshopper_captures_the_piece_beyond_a_hurdle() {
    // The capture corpus: grasshopper d4 hops the white pawn d5 and takes the black
    // rook on d6. That capture is the grasshopper's only move here.
    let mvs = moves_from(CAPTURE, "d4");
    assert_eq!(
        mvs,
        vec!["d4d6".to_string()],
        "the lone hop-capture is d4xd6"
    );
}

#[test]
fn grasshopper_with_no_hurdle_on_a_ray_has_no_move_there() {
    // A lone grasshopper on d4 with no piece on any of its eight queen rays is
    // completely immobile — a hopper needs a hurdle to move at all.
    let fen = "4k3/8/8/8/3***J4/8/8/4K3 w - - 0 1";
    assert!(
        moves_from(fen, "d4").is_empty(),
        "a hurdle-less grasshopper generates no move"
    );
}

#[test]
fn grasshopper_gives_check_one_square_beyond_a_hurdle() {
    // The black grasshopper e4 checks the white king e1 over the e2 pawn hurdle.
    let pos = Grasshopper::from_fen(CHECK).expect("valid Grasshopper FEN");
    assert!(pos.is_check(), "white is in check from the e4 grasshopper");
    // Moving the hurdle pawn e2-e3 breaks the hop onto e1, so it is a legal evasion
    // the fast capture-or-interpose mask could not express — the verify path finds it.
    assert!(
        moves_from(CHECK, "e2").contains(&"e2e3".to_string()),
        "moving the hurdle (e2-e3) is a legal check evasion"
    );
}
