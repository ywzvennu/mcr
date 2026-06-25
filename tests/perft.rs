//! Perft (move-generation correctness) tests against published reference node
//! counts.
//!
//! These assert the canonical perft values for the standard starting position,
//! the Kiwipete position, and Chess Programming Wiki positions 3 through 6. A
//! perft count is the number of leaf nodes in the legal-move game tree at a
//! fixed depth; matching the published values is a strong, well-known test that
//! move generation, make-move, and the special rules (castling, en passant,
//! promotion, pins, and check evasion) are all correct.
//!
//! The cheap layers run as ordinary tests. The expensive deep layers are marked
//! `#[ignore]` so `cargo test` stays fast; run them with
//! `cargo test --release -- --ignored` for the full sweep.

use mce::{perft, Position};

const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
const POS3: &str = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1";
const POS4: &str = "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1";
const POS5: &str = "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8";
const POS6: &str = "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10";

/// Asserts every `(depth, expected)` pair for the position parsed from `fen`.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Position::from_fen(fen).expect("valid FEN");
    for &(depth, expected) in cases {
        let got = perft(&pos, depth);
        assert_eq!(
            got, expected,
            "perft({depth}) for {fen}: expected {expected}, got {got}"
        );
    }
}

// -- Start position ---------------------------------------------------------

#[test]
fn startpos_cheap() {
    check(
        STARTPOS,
        &[(1, 20), (2, 400), (3, 8902), (4, 197281), (5, 4865609)],
    );
}

#[test]
#[ignore = "deep perft; run with --release --ignored"]
fn startpos_depth6() {
    check(STARTPOS, &[(6, 119060324)]);
}

// -- Kiwipete ---------------------------------------------------------------

#[test]
fn kiwipete_cheap() {
    check(KIWIPETE, &[(1, 48), (2, 2039), (3, 97862)]);
}

#[test]
#[ignore = "deep perft; run with --release --ignored"]
fn kiwipete_deep() {
    check(KIWIPETE, &[(4, 4085603), (5, 193690690)]);
}

// -- CPW position 3 ---------------------------------------------------------

#[test]
fn pos3_cheap() {
    check(
        POS3,
        &[(1, 14), (2, 191), (3, 2812), (4, 43238), (5, 674624)],
    );
}

#[test]
#[ignore = "deep perft; run with --release --ignored"]
fn pos3_depth6() {
    check(POS3, &[(6, 11030083)]);
}

// -- CPW position 4 ---------------------------------------------------------

#[test]
fn pos4_cheap() {
    check(POS4, &[(1, 6), (2, 264), (3, 9467), (4, 422333)]);
}

#[test]
#[ignore = "deep perft; run with --release --ignored"]
fn pos4_depth5() {
    check(POS4, &[(5, 15833292)]);
}

// -- CPW position 5 ---------------------------------------------------------

#[test]
fn pos5_cheap() {
    check(POS5, &[(1, 44), (2, 1486), (3, 62379)]);
}

#[test]
#[ignore = "deep perft; run with --release --ignored"]
fn pos5_depth5() {
    check(POS5, &[(4, 2103487), (5, 89941194)]);
}

// -- CPW position 6 ---------------------------------------------------------

#[test]
fn pos6_cheap() {
    check(POS6, &[(1, 46), (2, 2079), (3, 89890)]);
}

#[test]
#[ignore = "deep perft; run with --release --ignored"]
fn pos6_depth4() {
    check(POS6, &[(4, 3894594)]);
}
