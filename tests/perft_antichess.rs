//! Antichess perft regression.
//!
//! The expected node counts are transcribed verbatim from the public shakmaty
//! `antichess.perft` data table (a published table of facts; the generator code
//! here is original). The start position uses the antichess start FEN
//! `rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1` (standard placement,
//! no castling rights). The cheap depths run in CI; the deeper ones are
//! `#[ignore]`d and meant for a `--release` sweep:
//!
//! ```text
//! cargo test --release --test perft_antichess -- --ignored
//! ```
//!
//! The two pawn-endgame positions are taken from the same table and exercise the
//! forced-capture rule and the inverted win/no-move terminations (their deepest
//! plies reach `perft = 0`, i.e. a finished game).

use mce::{perft_variant, Antichess};

/// The antichess starting position (standard placement, no castling rights).
const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1";

fn perft_from(fen: &str, depth: u32) -> u64 {
    let pos: Antichess = fen.parse().expect("valid antichess FEN");
    perft_variant(&pos, depth)
}

#[test]
fn start_perft_cheap() {
    // shakmaty antichess.perft, id "antichess-start".
    assert_eq!(perft_from(START_FEN, 1), 20);
    assert_eq!(perft_from(START_FEN, 2), 400);
    assert_eq!(perft_from(START_FEN, 3), 8067);
}

#[test]
fn a_pawn_vs_b_pawn_full_table() {
    // shakmaty antichess.perft, id "a-pawn-vs-b-pawn": the line is forced to a
    // finish, so the deepest plies are 1 then 0 (game over).
    let fen = "8/1p6/8/8/8/8/P7/8 w - - 0 1";
    assert_eq!(perft_from(fen, 1), 2);
    assert_eq!(perft_from(fen, 2), 4);
    assert_eq!(perft_from(fen, 3), 4);
    assert_eq!(perft_from(fen, 4), 3);
    assert_eq!(perft_from(fen, 5), 1);
    assert_eq!(perft_from(fen, 6), 0);
}

#[test]
fn a_pawn_vs_c_pawn_cheap() {
    // shakmaty antichess.perft, id "a-pawn-vs-c-pawn" (cheap depths).
    let fen = "8/2p5/8/8/8/8/P7/8 w - - 0 1";
    assert_eq!(perft_from(fen, 1), 2);
    assert_eq!(perft_from(fen, 2), 4);
    assert_eq!(perft_from(fen, 3), 4);
    assert_eq!(perft_from(fen, 4), 4);
    assert_eq!(perft_from(fen, 5), 4);
    assert_eq!(perft_from(fen, 6), 4);
}

#[test]
#[ignore = "deep antichess start perft; run with --release --ignored"]
fn start_perft_deep() {
    // shakmaty antichess.perft, id "antichess-start" (deepest published depth).
    assert_eq!(perft_from(START_FEN, 4), 153_299);
}

#[test]
#[ignore = "deep antichess pawn-endgame perft; run with --release --ignored"]
fn a_pawn_vs_c_pawn_deep() {
    // shakmaty antichess.perft, id "a-pawn-vs-c-pawn" (deeper published depths).
    let fen = "8/2p5/8/8/8/8/P7/8 w - - 0 1";
    assert_eq!(perft_from(fen, 7), 4);
    assert_eq!(perft_from(fen, 8), 4);
    assert_eq!(perft_from(fen, 9), 12);
    assert_eq!(perft_from(fen, 10), 36);
    assert_eq!(perft_from(fen, 11), 312);
    assert_eq!(perft_from(fen, 12), 2557);
    assert_eq!(perft_from(fen, 13), 30873);
}
