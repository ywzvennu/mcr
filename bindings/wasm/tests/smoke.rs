//! Smoke test for the wasm binding logic.
//!
//! The `Game` type is the exact surface `wasm-bindgen` exports to JS; the crate
//! also builds as an `rlib`, so we exercise that surface natively here. This
//! covers the acceptance checks without needing a wasm runtime: startpos has 20
//! legal moves, perft matches the known counts, and SAN round-trips.

use mce_wasm::Game;

#[test]
fn startpos_has_twenty_legal_moves() {
    let g = Game::startpos(None).expect("standard startpos");
    assert_eq!(g.legal_moves().len(), 20, "startpos legal moves");
    assert_eq!(g.turn(), "white");
    assert_eq!(g.variant(), "standard");
    assert!(!g.is_check());
    assert!(!g.is_checkmate());
    assert!(g.outcome().is_none());
}

#[test]
fn perft_matches_known_counts() {
    let g = Game::startpos(None).expect("startpos");
    // Canonical standard-chess perft node counts from the start position.
    assert_eq!(g.perft(1), "20");
    assert_eq!(g.perft(2), "400");
    assert_eq!(g.perft(3), "8902");
}

#[test]
fn san_round_trips() {
    let g = Game::startpos(None).expect("startpos");
    // UCI -> SAN -> UCI for a knight develop.
    let san = g.san("g1f3").expect("san of g1f3");
    assert_eq!(san, "Nf3");
    let uci = g.parse_san("Nf3").expect("parse Nf3");
    assert_eq!(uci, "g1f3");

    // legalMovesSan should contain the same SAN.
    let sans = g.legal_moves_san().expect("legal SAN list");
    assert!(sans.contains(&"Nf3".to_owned()), "Nf3 in SAN list: {sans:?}");
    assert_eq!(sans.len(), 20);
}

#[test]
fn play_advances_and_reaches_mate() {
    // Fool's mate: 1. f3 e5 2. g4 Qh4#.
    let mut g = Game::startpos(None).expect("startpos");
    g.push("f2f3").unwrap();
    g.push("e7e5").unwrap();
    g.push("g2g4").unwrap();
    assert!(g.outcome().is_none());
    g.push("d8h4").unwrap();

    assert!(g.is_check());
    assert!(g.is_checkmate());
    let out = g.outcome().expect("game over");
    assert_eq!(out.kind, "decisive");
    assert_eq!(out.winner.as_deref(), Some("black"));
    assert_eq!(out.reason.as_deref(), Some("checkmate"));
}

#[test]
fn variants_and_fen_round_trip() {
    let atomic = Game::startpos(Some("atomic".to_owned())).expect("atomic startpos");
    assert_eq!(atomic.variant(), "atomic");
    assert_eq!(atomic.legal_moves().len(), 20);

    // FEN parse and round-trip for a custom position.
    let fen = "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq e6 0 2";
    let g = Game::from_fen(fen, None).expect("parse fen");
    assert_eq!(g.fen(), fen);
}

#[test]
fn zobrist_is_stable_hex() {
    // zobrist is a stable 16-hex string; it changes as the position changes.
    let mut g = Game::startpos(None).expect("startpos");
    let z0 = g.zobrist();
    assert_eq!(z0.len(), 16);
    assert!(z0.chars().all(|c| c.is_ascii_hexdigit()));
    g.push("e2e4").unwrap();
    assert_ne!(g.zobrist(), z0, "hash changes after a move");
}

// Note on the error paths (bad UCI/SAN/FEN, unknown variant, SAN on a non-SAN
// variant): these are exercised in the browser/Node demo. They cannot be
// asserted in a *native* test because constructing the returned `JsError`
// calls a wasm-imported function that panics off-wasm; the binding code itself
// returns `Result<_, JsError>` everywhere, so a thrown JS exception — never a
// panic across the boundary — is what reaches JS.
