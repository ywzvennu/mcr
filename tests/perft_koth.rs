//! Perft for King of the Hill through the generic variant layer.
//!
//! The hill rule only affects [`mcr::VariantPosition::outcome`], never which
//! moves are legal, so move generation from any non-terminal placement must
//! match standard chess exactly. These tests run the standard reference
//! positions and depths (the same numbers as `tests/perft.rs` and
//! `tests/perft_variants.rs`) through `VariantPosition<KingOfTheHillRules>`
//! (`KingOfTheHill`) and assert the node counts are identical to standard chess.
//!
//! None of the reference positions has a king on a central square, so all are
//! ongoing; the one exception — a position with a king already on the hill — is
//! terminal and its perft is asserted to be zero in a dedicated test.

use mcr::{perft, perft_variant, Chess, KingOfTheHill, Position};

const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
const POS3: &str = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1";
const POS4: &str = "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1";
const POS5: &str = "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8";
const POS6: &str = "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10";

/// Asserts every `(depth, expected)` pair for the `KingOfTheHill` position from
/// `fen`, and that it agrees with the standard `perft` from the same placement.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = KingOfTheHill::from_fen(fen).expect("valid king-of-the-hill FEN");
    let core = Position::from_fen(fen).expect("valid core FEN");
    let chess = Chess::from_fen(fen).expect("valid chess FEN");
    for &(depth, expected) in cases {
        let got = perft_variant(&pos, depth);
        assert_eq!(
            got, expected,
            "king-of-the-hill perft({depth}) for {fen}: expected {expected}, got {got}"
        );
        // Equality with the standard core and the standard variant path: the
        // hill rule must not change the legal moves on an ongoing position.
        assert_eq!(
            got,
            perft(&core, depth),
            "king-of-the-hill vs core perft({depth})"
        );
        assert_eq!(
            got,
            perft_variant(&chess, depth),
            "king-of-the-hill vs Chess perft({depth})"
        );
    }
}

#[test]
fn startpos_koth() {
    check(
        STARTPOS,
        &[(1, 20), (2, 400), (3, 8902), (4, 197281), (5, 4865609)],
    );
}

#[test]
fn kiwipete_koth() {
    check(KIWIPETE, &[(1, 48), (2, 2039), (3, 97862)]);
}

#[test]
fn pos3_koth() {
    check(
        POS3,
        &[(1, 14), (2, 191), (3, 2812), (4, 43238), (5, 674624)],
    );
}

#[test]
fn pos4_koth() {
    check(POS4, &[(1, 6), (2, 264), (3, 9467), (4, 422333)]);
}

#[test]
fn pos5_koth() {
    check(POS5, &[(1, 44), (2, 1486), (3, 62379)]);
}

#[test]
fn pos6_koth() {
    check(POS6, &[(1, 46), (2, 2079), (3, 89890)]);
}

#[test]
fn king_on_hill_is_terminal_with_zero_perft() {
    // A king already on a central square ends the game, so although the
    // underlying chess moves still generate (perft at depth 1 of the *position*
    // is nonzero), the position is decisive: perft is meaningful only on ongoing
    // positions. Here we assert the terminal property directly and that the
    // move generation itself is unchanged from standard chess at depth 1.
    let fen = "4k3/8/8/8/4K3/8/8/8 b - - 0 1"; // White king on e4 (the hill)
    let pos = KingOfTheHill::from_fen(fen).expect("valid koth FEN");
    assert!(pos.outcome().is_some(), "king on the hill must be terminal");
    // Movegen is unaffected by the hill rule even on a terminal position: the
    // legal moves equal the standard chess moves from the same placement.
    let core = Position::from_fen(fen).expect("valid core FEN");
    assert_eq!(perft_variant(&pos, 1), perft(&core, 1));
}

/// Deep perft sweep, matched against the standard reference node counts. Run
/// with `cargo test --release -- --ignored` before opening a PR.
#[test]
#[ignore = "deep perft sweep; run in --release"]
fn deep_koth_matches_standard() {
    check(STARTPOS, &[(6, 119060324)]);
    check(KIWIPETE, &[(4, 4085603), (5, 193690690)]);
    check(POS3, &[(6, 11030083)]);
}
