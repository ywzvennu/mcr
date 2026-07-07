//! WASM-safe surface smoke test.
//!
//! This test exercises only the parts of the public API that compile to
//! `wasm32-unknown-unknown` under the wasm-safe feature set
//! (`--no-default-features`, optionally `+serde`): FEN parsing and legal move
//! generation for standard chess and for a wide (large-board) variant. It
//! deliberately touches nothing that needs `std` — no filesystem book loader
//! (`book`), no rayon (`parallel`), no magic table (`magic`).
//!
//! The test runs on the native target (where `cargo test` has the default
//! features on), but every path it drives is present in the `no_std`/wasm
//! build. Its purpose is to prove the wasm-safe surface is not merely
//! *compilable* but *functional*: parse a position, enumerate legal moves, and
//! get sensible answers. Compilation to wasm itself is verified out-of-band via
//!
//! ```text
//! cargo build --lib --target wasm32-unknown-unknown --no-default-features
//! cargo build --lib --target wasm32-unknown-unknown --no-default-features --features serde
//! ```

use mcr::geometry::{AnyWideVariant, WideVariantId};
use mcr::Position;

/// Standard chess: parse the start position from FEN and enumerate its legal
/// moves. The opening position has exactly 20 legal moves.
#[test]
fn standard_chess_fen_and_legal_moves() {
    const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    let pos = Position::from_fen(STARTPOS).expect("start FEN parses");
    assert_eq!(pos.legal_moves().len(), 20, "20 legal moves at the start");

    // The FEN reader agrees with the built-in start position.
    assert_eq!(pos, Position::startpos());

    // A quiet middlegame FEN parses and yields at least one legal move.
    let mid =
        Position::from_fen("r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3")
            .expect("middlegame FEN parses");
    assert!(!mid.legal_moves().is_empty(), "middlegame has legal moves");
}

/// A wide variant (Shogi, a 9x9 board): start from the initial position and
/// enumerate legal moves. Shogi's opening position has 30 legal moves.
#[test]
fn wide_variant_startpos_and_legal_moves() {
    let id: WideVariantId = "shogi".parse().expect("shogi is a known variant");
    let pos = AnyWideVariant::startpos(id);

    assert_eq!(pos.variant_id(), WideVariantId::Shogi);
    assert_eq!(pos.dimensions(), (9, 9));
    assert_eq!(pos.legal_moves().len(), 30, "30 legal moves at Shogi start");
}

/// A wide variant round-trips through FEN: render the start position, re-parse
/// it, and confirm the legal-move surface is unchanged.
#[test]
fn wide_variant_fen_roundtrip() {
    let id: WideVariantId = "shogi".parse().expect("shogi is a known variant");
    let start = AnyWideVariant::startpos(id);
    let fen = start.to_fen();

    let reparsed = AnyWideVariant::from_fen(id, &fen).expect("rendered Shogi FEN re-parses");
    assert_eq!(
        reparsed.legal_moves().len(),
        start.legal_moves().len(),
        "FEN round-trip preserves the legal-move count",
    );
}
