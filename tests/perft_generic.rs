//! Generic-engine perft validation: `GenericPosition<Chess8x8, StandardChess>`
//! reproduces the concrete `Position` perft **exactly**.
//!
//! This is the linchpin test of the parallel generic layer (issue #166): the
//! generic engine — a separate `Geometry`-parametrised position, variant trait,
//! legal generator, make-move, and perft — must produce node counts
//! byte-identical to the frozen concrete 8x8 engine. The standard published
//! perft positions (startpos, Kiwipete, CPW 3-6) are checked against both the
//! concrete `mce::perft` and the pinned reference numbers.
//!
//! The cheap layers run as ordinary tests. The deep layers are `#[ignore]`d so
//! `cargo test` stays fast; run them with
//! `cargo test --release --test perft_generic -- --include-ignored`.

use mce::geometry::{perft as gperft, Chess8x8, GenericPosition, StandardChess};
use mce::{perft as cperft, Position};

type GenPos = GenericPosition<Chess8x8, StandardChess>;

const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
const POS3: &str = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1";
const POS4: &str = "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1";
const POS5: &str = "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8";
const POS6: &str = "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10";

/// Asserts, for every `(depth, expected)` pair, that the generic perft equals
/// both the pinned reference number *and* the concrete engine's perft.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let gpos = GenPos::from_fen(fen).expect("valid FEN for the generic engine");
    let cpos = Position::from_fen(fen).expect("valid FEN for the concrete engine");
    for &(depth, expected) in cases {
        let generic = gperft(&gpos, depth);
        let concrete = cperft(&cpos, depth);
        assert_eq!(
            concrete, expected,
            "concrete perft({depth}) for {fen}: expected {expected}, got {concrete}"
        );
        assert_eq!(
            generic, concrete,
            "generic perft({depth}) for {fen}: generic {generic} != concrete {concrete}"
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
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_depth6() {
    check(STARTPOS, &[(6, 119060324)]);
}

// -- Kiwipete ---------------------------------------------------------------

#[test]
fn kiwipete_cheap() {
    check(KIWIPETE, &[(1, 48), (2, 2039), (3, 97862)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn kiwipete_deep() {
    check(KIWIPETE, &[(4, 4085603)]);
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
#[ignore = "deep perft; run with --release --include-ignored"]
fn pos3_depth6() {
    check(POS3, &[(6, 11030083)]);
}

// -- CPW position 4 ---------------------------------------------------------

#[test]
fn pos4_cheap() {
    check(POS4, &[(1, 6), (2, 264), (3, 9467)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn pos4_depth4() {
    check(POS4, &[(4, 422333)]);
}

// -- CPW position 5 ---------------------------------------------------------

#[test]
fn pos5_cheap() {
    check(POS5, &[(1, 44), (2, 1486), (3, 62379)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn pos5_depth4() {
    check(POS5, &[(4, 2103487)]);
}

// -- CPW position 6 ---------------------------------------------------------

#[test]
fn pos6_cheap() {
    check(POS6, &[(1, 46), (2, 2079), (3, 89890)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn pos6_depth4() {
    check(POS6, &[(4, 3894594)]);
}
