//! Petrified chess perft and rule validation on the generic engine.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Petrified::perft` and by Fairy-Stockfish (FSF, `UCI_Variant
//! petrified`, a built-in) on the byte-identical position. The `compare-fairy/`
//! harness re-runs that head-to-head on demand (see `compare-fairy/src/petrified.rs`);
//! this test pins the confirmed numbers so a regression is caught without FSF present.
//!
//! ## What petrified chess is
//!
//! Pawn-sideways chess (a pawn may take a single quiet sideways step) with two
//! extra rules:
//!
//! * **Petrify on capture.** A capturing Queen, Rook, Bishop, or Knight is turned
//!   to stone on its destination — removed from the board, the square becoming an
//!   inert colorless **wall** that blocks sliders and can never move, capture, be
//!   captured, or give check. A capturing **pawn** is not petrified.
//! * **Pseudo-royal Commoner.** The king (`k`/`K`) is a Commoner: not checkmated,
//!   but a side loses when its Commoner is captured / goes extinct. The Commoner
//!   may never capture (it would petrify itself), so it never attacks and two
//!   Commoners may stand adjacent.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_petrified -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Petrified, Square, WideOutcome};
use mcr::Color;

/// The petrified starting FEN, confirmed byte-for-byte against Fairy-Stockfish's
/// `UCI_Variant petrified` / `position startpos`. The king letter is the Commoner.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// Both armies developed with bishops and knights poised to capture: the tree
/// exercises Queen/Rook/Bishop/Knight captures that petrify into walls.
const DEVEL_PETRIFY: &str = "r1bqk2r/pppp1ppp/2n2n2/1Bb1p3/4P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 0 1";

/// A denser middlegame (both sides castled-eligible, many captures available),
/// stressing petrification interacting with castling and pins.
const MIXED_MID: &str = "r3k2r/pp3ppp/2n1b3/2bpp3/4P3/2N2N2/PPPP1PPP/R1BQK2R w KQkq - 0 1";

/// Two bare Commoners a knight's-reach apart with a lone pawn: exercises the
/// pseudo-royal Commoner (kept safe, may approach the enemy Commoner, may not
/// capture) with no other pieces to distract the count.
const COMMONER_NEAR: &str = "8/8/3k4/8/3K4/4P3/8/8 w - - 0 1";

/// Interlocked pawn chains: every pawn capture here is a **non**-petrifying
/// capture, so the tree pins that pawns are exempt from the turn-to-stone rule.
const PAWN_CAPS: &str = "4k3/8/8/2ppp3/3PPP2/8/8/4K3 w - - 0 1";

/// Asserts the generic petrified perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `UCI_Variant petrified` `go perft` on the
/// same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Petrified::from_fen(fen).expect("valid petrified FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "petrified perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

fn sq(file: u8, rank: u8) -> Square<Chess8x8> {
    Square::<Chess8x8>::from_file_rank(file, rank).unwrap()
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 20), (2, 400), (3, 10022), (4, 250134)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 7206900)]);
}

// -- Petrifying captures in the tree (FSF-confirmed) ------------------------

#[test]
fn devel_petrify_cheap() {
    check(DEVEL_PETRIFY, &[(2, 1402), (3, 51301)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn devel_petrify_deep() {
    check(DEVEL_PETRIFY, &[(4, 1946048)]);
}

#[test]
fn mixed_mid_cheap() {
    check(MIXED_MID, &[(2, 1461), (3, 48695)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn mixed_mid_deep() {
    check(MIXED_MID, &[(4, 2154323)]);
}

// -- Pseudo-royal Commoner (FSF-confirmed) ----------------------------------

#[test]
fn commoner_near() {
    check(COMMONER_NEAR, &[(2, 76), (3, 753), (4, 5536)]);
}

// -- Non-petrifying pawn captures (FSF-confirmed) ---------------------------

#[test]
fn pawn_captures_do_not_petrify() {
    check(PAWN_CAPS, &[(2, 131), (3, 1571), (4, 17909)]);
}

// -- Rule-level checks (petrify wall semantics) -----------------------------

/// A capturing Rook turns to stone: the wall blocks, cannot move, cannot be
/// captured or passed through, and round-trips through the `*` FEN.
#[test]
fn rook_capture_petrifies_into_an_impassable_wall() {
    let start = Petrified::from_fen("r3k3/8/8/b7/8/8/8/R3K3 w - - 0 1").unwrap();
    let a5 = sq(0, 4);
    let ra1xa5 = start
        .legal_moves()
        .into_iter()
        .find(|m| m.from::<Chess8x8>() == sq(0, 0) && m.to::<Chess8x8>() == a5)
        .expect("Ra1xa5 is legal");
    let after = start.play(&ra1xa5);

    // The rook is gone; a5 is a wall recorded in the petrified mask.
    assert!(after.state().petrified.contains(a5), "a5 is petrified");
    assert!(
        after.board().piece_at(a5).is_none(),
        "the petrified rook is removed from the board"
    );

    // The wall serializes as `*` and round-trips.
    let fen = after.to_fen();
    assert!(fen.contains('*'), "wall serializes as `*`: {fen}");
    let round_tripped = Petrified::from_fen(&fen).unwrap();
    assert_eq!(round_tripped.to_fen(), fen, "the wall FEN round-trips");

    // Black to move: no move may land on the wall (a5) nor slide through it (a4).
    for m in round_tripped.legal_moves() {
        let to = m.to::<Chess8x8>();
        assert_ne!(to, a5, "no piece may capture or land on the wall");
        assert_ne!(to, sq(0, 3), "no slider may pass through the wall");
    }

    // After any black reply the petrified rook still generates no move.
    let reply = *round_tripped.legal_moves().first().unwrap();
    let next = round_tripped.play(&reply);
    for m in next.legal_moves() {
        assert_ne!(
            m.from::<Chess8x8>(),
            a5,
            "a petrified piece can never move again"
        );
    }
}

/// A capturing pawn is **not** petrified: it stays on the board and no wall is made.
#[test]
fn pawn_capture_is_not_petrified() {
    let start = Petrified::from_fen("4k3/8/8/4r3/3P4/8/8/4K3 w - - 0 1").unwrap();
    let e5 = sq(4, 4);
    let dxe5 = start
        .legal_moves()
        .into_iter()
        .find(|m| m.from::<Chess8x8>() == sq(3, 3) && m.to::<Chess8x8>() == e5)
        .expect("d4xe5 is legal");
    let after = start.play(&dxe5);
    assert!(
        !after.state().petrified.contains(e5),
        "a capturing pawn is not petrified"
    );
    assert!(after.board().piece_at(e5).is_some(), "the pawn stays on e5");
    assert!(!after.to_fen().contains('*'), "no wall from a pawn capture");
}

/// The pseudo-royal Commoner may never capture (a capture would petrify it).
#[test]
fn commoner_cannot_capture() {
    let pos = Petrified::from_fen("4k3/8/8/8/8/2n5/1K6/8 w - - 0 1").unwrap();
    let b2 = sq(1, 1);
    let c3 = sq(2, 2);
    assert!(
        pos.legal_moves()
            .into_iter()
            .all(|m| !(m.from::<Chess8x8>() == b2 && m.to::<Chess8x8>() == c3)),
        "the Commoner may not capture the adjacent knight"
    );
}

/// Extinction adjudication: a side whose Commoner is gone has lost.
#[test]
fn losing_the_commoner_is_extinction() {
    let pos = Petrified::from_fen("8/8/8/8/8/8/8/4K3 b - - 0 1").unwrap();
    assert_eq!(
        pos.outcome(),
        Some(WideOutcome::Decisive {
            winner: Color::White
        }),
        "a side with no Commoner has lost by extinction"
    );
}
