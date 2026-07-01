//! Smoke test for the wasm binding logic.
//!
//! The `Game` type is the exact surface `wasm-bindgen` exports to JS; the crate
//! also builds as an `rlib`, so we exercise that surface natively here. This
//! covers the acceptance checks without needing a wasm runtime: startpos has 20
//! legal moves, perft matches the known counts, and SAN round-trips.

use mce_wasm::{FairyGame, Game};

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
    assert!(
        sans.contains(&"Nf3".to_owned()),
        "Nf3 in SAN list: {sans:?}"
    );
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
    assert_eq!(g.status(), "checkmate");
    let out = g.outcome().expect("game over");
    assert_eq!(out.kind, "decisive");
    assert_eq!(out.winner.as_deref(), Some("black"));
    assert_eq!(out.reason.as_deref(), Some("checkmate"));
}

#[test]
fn status_and_analysis_queries() {
    let g = Game::startpos(None).expect("startpos");
    assert_eq!(g.status(), "ongoing");

    // Analysis (issue #373): White attacks f3 but not e4 in the start position.
    assert!(g.is_attacked("f3", "white").expect("is_attacked f3"));
    assert!(!g.is_attacked("e4", "white").expect("is_attacked e4"));

    // Attackers of f3 by White: g1 knight + e2/g2 pawns.
    let mut attackers = g.attackers("f3", "white").expect("attackers");
    attackers.sort();
    assert_eq!(attackers, vec!["e2", "g1", "g2"]);
    assert!(g
        .attackers("f3", "black")
        .expect("black attackers")
        .is_empty());

    // The g1 knight attacks e2 (own pawn, defended), f3 and h3.
    let mut from = g.attacks_from("g1").expect("attacks_from g1");
    from.sort();
    assert_eq!(from, vec!["e2", "f3", "h3"]);
    assert_eq!(g.mobility("g1").expect("mobility g1"), 3);
    assert_eq!(g.mobility("e4").expect("mobility e4"), 0);

    // Analysis also works for other 8x8 variants via the core position.
    let atomic = Game::startpos(Some("atomic".to_owned())).expect("atomic");
    assert_eq!(atomic.status(), "ongoing");
    assert!(atomic
        .is_attacked("f3", "white")
        .expect("atomic is_attacked"));

    // Stalemate is reported as a draw status.
    let stale = Game::from_fen("k7/2K5/1Q6/8/8/8/8/8 b - - 0 1", None).expect("stalemate fen");
    assert_eq!(stale.status(), "stalemate");
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

#[test]
fn fairy_startpos_and_perft_match_known_counts() {
    // Construct a fairy variant by name and run perft — the acceptance gate.
    // FSF-confirmed Xiangqi startpos counts (tests/perft_xiangqi.rs).
    let xq = FairyGame::startpos("xiangqi").expect("xiangqi startpos");
    assert_eq!(xq.variant(), "xiangqi");
    assert_eq!(xq.turn(), "white");
    assert_eq!(xq.legal_moves().len(), 44, "xiangqi startpos legal moves");
    assert!(!xq.is_check());
    assert!(xq.outcome().is_none());
    assert_eq!(xq.perft(1), "44");
    assert_eq!(xq.perft(2), "1920");
    assert_eq!(xq.perft(3), "79666");
    assert_eq!(xq.status(), "ongoing");

    // A second geometry (9x9 Shogi) via an alias-free name (tests/perft_shogi.rs).
    let shogi = FairyGame::startpos("shogi").expect("shogi startpos");
    assert_eq!(shogi.perft(1), "30");
    assert_eq!(shogi.perft(2), "900");

    // Alias resolution: "cchess" -> xiangqi.
    let alias = FairyGame::startpos("cchess").expect("cchess alias");
    assert_eq!(alias.variant(), "xiangqi");

    // The variant catalogue is exposed and non-trivial.
    let names = FairyGame::variants();
    assert!(names.iter().any(|n| n == "xiangqi"));
    assert!(names.iter().any(|n| n == "shogi"));
}

#[test]
fn fairy_play_advances_and_fen_round_trips() {
    let mut xq = FairyGame::startpos("xiangqi").expect("xiangqi startpos");
    let fen = xq.fen();
    // Re-parse the startpos FEN under the variant.
    let reparsed = FairyGame::from_fen("xiangqi", &fen).expect("parse fen");
    assert_eq!(reparsed.fen(), fen);

    // Play the first legal move; the position advances to black to move.
    let first = xq.legal_moves()[0].clone();
    xq.push(&first).expect("play first legal move");
    assert_eq!(xq.turn(), "black");
    assert_ne!(xq.fen(), fen, "fen changes after a move");
}

// Note on the error paths (bad UCI/SAN/FEN, unknown variant, SAN on a non-SAN
// variant): these are exercised in the browser/Node demo. They cannot be
// asserted in a *native* test because constructing the returned `JsError`
// calls a wasm-imported function that panics off-wasm; the binding code itself
// returns `Result<_, JsError>` everywhere, so a thrown JS exception — never a
// panic across the boundary — is what reaches JS.
