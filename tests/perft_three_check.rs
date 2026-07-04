//! Perft for three-check through the generic variant layer.
//!
//! The three-check check counter never alters the legal-move set, so movegen
//! from any placement must match standard chess exactly. These tests run the
//! standard reference positions and depths (the same numbers as `tests/perft.rs`
//! and `tests/perft_variants.rs`) through `VariantPosition<ThreeCheckRules>`
//! (`ThreeCheck`) and assert the node counts are identical to standard chess.
//!
//! Each FEN carries the seventh `3+3` remaining-checks field so it parses as a
//! three-check position; the counts are irrelevant to movegen, which is exactly
//! the property under test.

use mcr::{perft, perft_variant, Chess, Position, ThreeCheck};

const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 3+3";
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1 3+3";
const POS3: &str = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1 3+3";
const POS4: &str = "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1 3+3";
const POS5: &str = "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8 3+3";
const POS6: &str = "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10 3+3";

/// The same FEN with the trailing check field stripped, for the standard core.
fn strip_check_field(fen: &str) -> String {
    let mut fields: Vec<&str> = fen.split_whitespace().collect();
    fields.pop();
    fields.join(" ")
}

/// Asserts every `(depth, expected)` pair for the `ThreeCheck` position from
/// `fen`, and that it agrees with the standard `perft` from the same placement.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = ThreeCheck::from_fen(fen).expect("valid three-check FEN");
    let core_fen = strip_check_field(fen);
    let core = Position::from_fen(&core_fen).expect("valid core FEN");
    let chess = Chess::from_fen(&core_fen).expect("valid chess FEN");
    for &(depth, expected) in cases {
        let got = perft_variant(&pos, depth);
        assert_eq!(
            got, expected,
            "three-check perft({depth}) for {fen}: expected {expected}, got {got}"
        );
        // Equality with the standard core and the standard variant path: the
        // check counter must not change the legal moves.
        assert_eq!(
            got,
            perft(&core, depth),
            "three-check vs core perft({depth})"
        );
        assert_eq!(
            got,
            perft_variant(&chess, depth),
            "three-check vs Chess perft({depth})"
        );
    }
}

#[test]
fn startpos_three_check() {
    check(
        STARTPOS,
        &[(1, 20), (2, 400), (3, 8902), (4, 197281), (5, 4865609)],
    );
}

#[test]
fn kiwipete_three_check() {
    check(KIWIPETE, &[(1, 48), (2, 2039), (3, 97862)]);
}

#[test]
fn pos3_three_check() {
    check(
        POS3,
        &[(1, 14), (2, 191), (3, 2812), (4, 43238), (5, 674624)],
    );
}

#[test]
fn pos4_three_check() {
    check(POS4, &[(1, 6), (2, 264), (3, 9467), (4, 422333)]);
}

#[test]
fn pos5_three_check() {
    check(POS5, &[(1, 44), (2, 1486), (3, 62379)]);
}

#[test]
fn pos6_three_check() {
    check(POS6, &[(1, 46), (2, 2079), (3, 89890)]);
}

/// Deep perft sweep, matched against the standard reference node counts. Run
/// with `cargo test --release -- --ignored` before opening a PR.
#[test]
#[ignore = "deep perft sweep; run in --release"]
fn deep_three_check_matches_standard() {
    check(STARTPOS, &[(6, 119060324)]);
    check(KIWIPETE, &[(4, 4085603), (5, 193690690)]);
    check(POS3, &[(6, 11030083)]);
}
