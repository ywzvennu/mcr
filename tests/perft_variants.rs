//! Perft through the generic variant layer for standard chess.
//!
//! These run the same reference positions and depths as `tests/perft.rs`, but
//! through `VariantPosition<ChessRules>` (`Chess`) and the generic
//! [`perft_variant`] counter. They assert the variant path reproduces the
//! standard node counts exactly, which is the acceptance criterion that the
//! variant abstraction does not regress the standard chess path.

use mce::{perft_variant, Chess};

const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
const POS3: &str = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1";
const POS4: &str = "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1";
const POS5: &str = "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8";
const POS6: &str = "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10";

/// Asserts every `(depth, expected)` pair for the `Chess` position from `fen`.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Chess::from_fen(fen).expect("valid FEN");
    for &(depth, expected) in cases {
        let got = perft_variant(&pos, depth);
        assert_eq!(
            got, expected,
            "variant perft({depth}) for {fen}: expected {expected}, got {got}"
        );
    }
}

#[test]
fn startpos_through_variant() {
    check(
        STARTPOS,
        &[(1, 20), (2, 400), (3, 8902), (4, 197281), (5, 4865609)],
    );
}

#[test]
fn kiwipete_through_variant() {
    check(KIWIPETE, &[(1, 48), (2, 2039), (3, 97862)]);
}

#[test]
fn pos3_through_variant() {
    check(
        POS3,
        &[(1, 14), (2, 191), (3, 2812), (4, 43238), (5, 674624)],
    );
}

#[test]
fn pos4_through_variant() {
    check(POS4, &[(1, 6), (2, 264), (3, 9467), (4, 422333)]);
}

#[test]
fn pos5_through_variant() {
    check(POS5, &[(1, 44), (2, 1486), (3, 62379)]);
}

#[test]
fn pos6_through_variant() {
    check(POS6, &[(1, 46), (2, 2079), (3, 89890)]);
}
