//! Coregal chess (8x8) perft validation on the generic engine — standard chess in
//! which the **queen is royal too** (a side loses if *either* its king or its
//! queen is checkmated), and nothing else changed.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Coregal::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant coregal`, the built-in `coregal_variant()`) running `go perft` on
//! the byte-identical position. mcr and FSF spell coregal chess with the same
//! standard-chess letters, so no dialect rewrite is needed. The `compare-fairy/`
//! differential fuzzer re-runs that head-to-head on demand; this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Where it diverges from standard chess
//!
//! Because the royal queen may not move onto — or be left on — an attacked square,
//! coregal forbids moves standard chess allows the instant a queen is exposed. The
//! divergence appears **immediately from the startpos**: standard chess perft(3) is
//! 8902, coregal is 8882 (an early `Qh5`/`Qg4`-into-a-knight sortie is illegal
//! here), and perft(4) is 195896 vs standard 197281. Kiwipete drops from 48 to 42
//! at depth 1 (queen destinations that would hang the queen are removed), and a
//! rook or knight bearing on the queen makes the queen "in check" — the side must
//! respond exactly as if a king were attacked.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `coregal_variant()` (`variant.cpp:1122` — the standard
//! `chess_variant_base()` with `extinctionPieceTypes = QUEEN`,
//! `extinctionPseudoRoyal = true`, `extinctionPieceCount = 64`); castling is the
//! standard `KQkq`:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! ```
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_coregal -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Coregal, Square, WideMoveKind, WideRole};
use mcr::Color;

/// The coregal starting FEN, confirmed against Fairy-Stockfish's
/// `UCI_Variant coregal`.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// Kiwipete: the classic castling-rich perft position, with both queens on the
/// board (white `Qf3`, black `Qe7`). Standard chess perft(1) is 48; coregal removes
/// the queen moves that would hang the royal queen, dropping it to 42.
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

/// The white queen on d1 is attacked by the black rook down the open d-file — the
/// queen is "in check" and the side to move must resolve the threat, exactly as if
/// a king were attacked.
const ROOK_CHECKS_QUEEN: &str = "3rk3/8/8/8/8/8/8/3QK3 w - - 0 1";

/// The white queen on c1 is attacked by a black knight on e4 — it must flee (or the
/// attacker be removed) royally.
const KNIGHT_ATTACKS_QUEEN: &str = "4k3/8/8/8/4n3/8/8/2Q1K3 w - - 0 1";

/// Asserts the generic coregal perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF coregal `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Coregal::from_fen(fen).expect("valid coregal FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "coregal perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    // Diverges from standard chess (8902 / 197281) at depth 3: an exposed royal
    // queen sortie is illegal here.
    check(STARTPOS, &[(1, 20), (2, 400), (3, 8882), (4, 195896)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 4756867)]);
}

// -- Kiwipete: royal-queen divergence (FSF-confirmed) -----------------------

#[test]
fn kiwipete_cheap() {
    // Standard chess: 48 / 2039 / 97862 / 4085603. Coregal drops the moves that
    // would leave a royal queen en prise.
    check(KIWIPETE, &[(1, 42), (2, 1656), (3, 67207), (4, 2553728)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn kiwipete_deep() {
    check(KIWIPETE, &[(5, 100957642)]);
}

// -- Queen "in check" from a rook (FSF-confirmed) ---------------------------

#[test]
fn rook_checks_queen_cheap() {
    check(
        ROOK_CHECKS_QUEEN,
        &[(1, 10), (2, 104), (3, 1854), (4, 24454)],
    );
}

// -- Queen attacked by a knight (FSF-confirmed) -----------------------------

#[test]
fn knight_attacks_queen_cheap() {
    check(
        KNIGHT_ATTACKS_QUEEN,
        &[(1, 16), (2, 168), (3, 3239), (4, 32734)],
    );
}

// -- Rule-level self-checks (independent of FSF) ----------------------------

/// The starting array round-trips through FEN, has 20 opening moves, and carries
/// standard castling rights for both sides.
#[test]
fn startpos_fen_round_trips_with_castling() {
    let pos = Coregal::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    assert_eq!(pos.turn(), Color::White);
    assert_eq!(pos.legal_move_count(), 20);
    assert!(pos.castling().has_any(Color::White));
    assert!(pos.castling().has_any(Color::Black));
}

/// The queen is royal: it may not move onto a square the opponent attacks. With a
/// black knight on f6, the queen may not go to g4 or h5 (it would be en prise),
/// exactly as a king refusing to step into check.
#[test]
fn royal_queen_cannot_step_into_attack() {
    let pos = Coregal::from_fen("rnbqkb1r/pppppppp/5n2/8/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 1")
        .expect("valid FEN");
    let queen_dests: Vec<Square<Chess8x8>> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| pos.board().role_at(m.from::<Chess8x8>()) == Some(WideRole::Queen))
        .map(|m| m.to::<Chess8x8>())
        .collect();
    let g4 = Square::<Chess8x8>::from_file_rank(6, 3).unwrap();
    let h5 = Square::<Chess8x8>::from_file_rank(7, 4).unwrap();
    let f3 = Square::<Chess8x8>::from_file_rank(5, 2).unwrap();
    assert!(
        !queen_dests.contains(&g4),
        "Qg4 hangs the royal queen to Nf6"
    );
    assert!(
        !queen_dests.contains(&h5),
        "Qh5 hangs the royal queen to Nf6"
    );
    assert!(queen_dests.contains(&f3), "Qf3 is safe and legal");
}

/// Every legal coregal move leaves *both* royals (king and queen) unattacked — the
/// strict pseudo-royal rule. Checked on the rook-checks-queen position where the
/// queen is under attack and most king moves are therefore illegal.
#[test]
fn every_move_keeps_both_royals_safe() {
    let pos = Coregal::from_fen(ROOK_CHECKS_QUEEN).expect("valid FEN");
    let moves = pos.legal_moves();
    assert!(!moves.is_empty(), "the side to move is not mated here");
    for mv in moves {
        let next = pos.play(&mv);
        for royal in next.royal_squares(Color::White) {
            assert!(
                next.attackers_of(royal, Color::Black).is_empty(),
                "move {mv:?} left a white royal on {royal:?} attacked",
            );
        }
    }
}

/// A double pawn push still sets an en-passant target — every non-royal rule is
/// standard chess.
#[test]
fn pawn_double_push_sets_en_passant() {
    let pos = Coregal::startpos();
    let dbl = pos
        .legal_moves()
        .into_iter()
        .find(|m| matches!(m.kind(), WideMoveKind::DoublePawnPush))
        .expect("a double pawn push exists at the start");
    let next = pos.play(&dbl);
    assert!(
        next.ep_square().is_some(),
        "a double push creates an en-passant target",
    );
}
