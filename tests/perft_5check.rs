//! Five-check (5check, 8x8) perft validation on the generic engine — **standard
//! chess with a five-check win condition** (Fairy-Stockfish `fivecheck_variant()`,
//! `chess_variant_base()` + `checkCounting` with a five-check goal).
//!
//! Five-check's check tally changes **only adjudication**, never the legal-move
//! set, so its movegen — and therefore its perft — is **byte-for-byte standard
//! chess**. Every `(depth, nodes)` pair below was produced identically by
//! `mcr::geometry::FiveCheck::perft` and by Fairy-Stockfish (FSF, `UCI_Variant
//! 5check`) running `go perft` on the same position, and each equals the
//! canonical standard-chess count. The `compare-fairy/` differential fuzzer
//! re-runs that head-to-head on demand; this test pins the FSF-confirmed numbers
//! so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against FSF's `UCI_Variant 5check` (`position startpos`), the standard
//! array with the `5+5` remaining-checks field:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 5+5 0 1
//! ```
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_5check -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, FiveCheck, GameStatus};
use mcr::Color;

/// The five-check starting FEN, confirmed against Fairy-Stockfish `UCI_Variant
/// 5check` — the standard array plus the `5+5` check field.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 5+5 0 1";

/// Kiwipete: the classic castling-rich perft position, here carrying a `5+5`
/// tally. The check counter never changes the move set, so the counts are the
/// standard-chess Kiwipete numbers.
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 5+5 0 1";

/// Standard perft "position 3" — a rook-and-pawn endgame.
const POS3: &str = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 5+5 0 1";

/// Standard perft "position 4" (mirror of a tactical middlegame).
const POS4: &str = "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 5+5 0 1";

/// Standard perft "position 5" — a sharp position with promotions available.
const POS5: &str = "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 5+5 0 1";

/// Standard perft "position 6" — a quiet, symmetric middlegame.
const POS6: &str = "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 5+5 0 1";

/// Asserts the generic five-check perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `5check` `go perft` on the same position,
/// and equals the canonical standard-chess perft.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = FiveCheck::from_fen(fen).expect("valid 5check FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "5check perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed; == standard chess) ----------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 20), (2, 400), (3, 8902), (4, 197281)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 4865609), (6, 119060324)]);
}

// -- Kiwipete (FSF-confirmed; == standard chess) ----------------------------

#[test]
fn kiwipete_cheap() {
    check(KIWIPETE, &[(1, 48), (2, 2039), (3, 97862), (4, 4085603)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn kiwipete_deep() {
    check(KIWIPETE, &[(5, 193690690)]);
}

// -- Standard positions 3-6 (FSF-confirmed; == standard chess) --------------

#[test]
fn pos3_cheap() {
    check(POS3, &[(1, 14), (2, 191), (3, 2812), (4, 43238)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn pos3_deep() {
    check(POS3, &[(5, 674624), (6, 11030083)]);
}

#[test]
fn pos4_cheap() {
    check(POS4, &[(1, 6), (2, 264), (3, 9467), (4, 422333)]);
}

#[test]
fn pos5_cheap() {
    check(POS5, &[(1, 44), (2, 1486), (3, 62379), (4, 2103487)]);
}

#[test]
fn pos6_cheap() {
    check(POS6, &[(1, 46), (2, 2079), (3, 89890), (4, 3894594)]);
}

// -- Rule-level self-checks (independent of FSF) ----------------------------

/// The starting array round-trips through FEN (with the `5+5` field) and opens
/// with the twenty standard-chess moves.
#[test]
fn startpos_fen_round_trips_with_check_field() {
    let pos = FiveCheck::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    assert_eq!(pos.turn(), Color::White);
    assert_eq!(pos.legal_move_count(), 20);
    assert!(pos.outcome().is_none());
}

/// The check tally never changes the legal-move set: a position with checks
/// already delivered has the same perft as the identical placement with a full
/// tally.
#[test]
fn check_tally_does_not_affect_perft() {
    let full = FiveCheck::from_fen("4k3/8/8/8/8/8/8/3QK3 w - - 5+5 0 1").expect("valid FEN");
    let spent = FiveCheck::from_fen("4k3/8/8/8/8/8/8/3QK3 w - - 5+2 0 1").expect("valid FEN");
    assert_eq!(
        gperft::<Chess8x8, _, _>(&full, 3),
        gperft::<Chess8x8, _, _>(&spent, 3),
        "the check counter must not change perft",
    );
}

/// Delivering the fifth check wins immediately — before any checkmate — and the
/// win is credited to the checker.
#[test]
fn fifth_check_wins_before_checkmate() {
    // Four checks already stand against Black (`5+1`); White's next check is the
    // fifth and wins on the spot.
    let pos = FiveCheck::from_fen("4k3/8/8/8/8/8/8/3QK3 w - - 5+1 0 1").expect("valid FEN");
    let qd8 = pos
        .legal_moves()
        .into_iter()
        .find(|m| m.to_uci::<Chess8x8>() == "d1d8")
        .expect("Qd8+ is legal");
    let after = pos.play(&qd8);
    assert!(after.is_check());
    assert!(matches!(
        after.status(),
        GameStatus::VariantWin {
            winner: Color::White,
            ..
        }
    ));
}
