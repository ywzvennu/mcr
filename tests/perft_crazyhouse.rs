//! Crazyhouse perft regression tests.
//!
//! The node counts are transcribed from the public shakmaty `crazyhouse.perft`
//! fixture (its `tests/crazyhouse.perft` data table). Perft numbers are public
//! facts; only the figures are reused here, none of the engine code.
//!
//! Drops make the branching factor explode, so the CI-default tests stay at
//! shallow depths and the deeper levels are `#[ignore]`d — run them with
//! `cargo test --release -- --ignored`.

use mcr::{perft_variant, Crazyhouse};

/// Parses a crazyhouse FEN (the fixture uses the 4-field EPD form without move
/// clocks; the parser defaults those) and runs perft to `depth`.
fn perft(fen: &str, depth: u32) -> u64 {
    let pos: Crazyhouse = fen.parse().expect("valid crazyhouse FEN");
    perft_variant(&pos, depth)
}

// -- zh-middlegame: a normal middlegame with empty pockets (drops appear once a
// capture fills a pocket). EPD: r1bqk2r/.../R1BQK2R[] b KQkq - --------------

const MIDDLEGAME: &str = "r1bqk2r/pppp1ppp/2n1p3/4P3/1b1Pn3/2NB1N2/PPP2PPP/R1BQK2R[] b KQkq -";

#[test]
fn middlegame_perft_1() {
    assert_eq!(perft(MIDDLEGAME, 1), 42);
}

#[test]
fn middlegame_perft_2() {
    assert_eq!(perft(MIDDLEGAME, 2), 1347);
}

#[test]
#[ignore = "deep crazyhouse perft; run with --release -- --ignored"]
fn middlegame_perft_3() {
    assert_eq!(perft(MIDDLEGAME, 3), 58057);
}

#[test]
#[ignore = "deep crazyhouse perft; run with --release -- --ignored"]
fn middlegame_perft_4() {
    assert_eq!(perft(MIDDLEGAME, 4), 2083382);
}

// -- zh-drops: a sparse position with a white queen and a black knight already
// in pocket, so drops dominate. EPD: 2k5/8/8/8/8/8/8/4K3[Qn] w - - ----------

const DROPS: &str = "2k5/8/8/8/8/8/8/4K3[Qn] w - -";

#[test]
fn drops_perft_1() {
    assert_eq!(perft(DROPS, 1), 67);
}

#[test]
fn drops_perft_2() {
    assert_eq!(perft(DROPS, 2), 3083);
}

#[test]
#[ignore = "deep crazyhouse perft; run with --release -- --ignored"]
fn drops_perft_3() {
    assert_eq!(perft(DROPS, 3), 88634);
}

#[test]
#[ignore = "deep crazyhouse perft; run with --release -- --ignored"]
fn drops_perft_4() {
    assert_eq!(perft(DROPS, 4), 932554);
}

// -- zh-all-drop-types: every droppable role in both pockets, maximizing the
// drop branching factor. EPD: 2k5/8/8/8/8/8/8/4K3[QRBNPqrbnp] w - - ---------

const ALL_DROP_TYPES: &str = "2k5/8/8/8/8/8/8/4K3[QRBNPqrbnp] w - -";

#[test]
fn all_drop_types_perft_1() {
    assert_eq!(perft(ALL_DROP_TYPES, 1), 301);
}

#[test]
#[ignore = "deep crazyhouse perft; run with --release -- --ignored"]
fn all_drop_types_perft_2() {
    assert_eq!(perft(ALL_DROP_TYPES, 2), 75353);
}

// -- zh-promoted: a position with a promoted queen (`Q~`) so the promoted-piece
// bookkeeping is exercised in movegen. EPD: 4k3/1Q~6/8/8/4b3/8/Kpp5/8 b - - --

const PROMOTED: &str = "4k3/1Q~6/8/8/4b3/8/Kpp5/8 b - - 0 1";

#[test]
fn promoted_perft_1() {
    assert_eq!(perft(PROMOTED, 1), 20);
}

#[test]
fn promoted_perft_2() {
    assert_eq!(perft(PROMOTED, 2), 360);
}

#[test]
#[ignore = "deep crazyhouse perft; run with --release -- --ignored"]
fn promoted_perft_3() {
    assert_eq!(perft(PROMOTED, 3), 5445);
}

#[test]
#[ignore = "deep crazyhouse perft; run with --release -- --ignored"]
fn promoted_perft_4() {
    assert_eq!(perft(PROMOTED, 4), 132758);
}
