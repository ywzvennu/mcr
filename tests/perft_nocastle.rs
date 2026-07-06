//! No-castle chess (8x8) perft validation on the generic engine — standard chess
//! with **castling disabled** and nothing else changed.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Nocastle::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant nocastle`, the built-in `nocastle_variant()`) running `go perft`
//! on the byte-identical position. mcr and FSF spell no-castle chess with the same
//! standard-chess letters, so no dialect rewrite is needed. The `compare-fairy/`
//! differential fuzzer re-runs that head-to-head on demand; this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Where it diverges from standard chess
//!
//! From the opening array the two games are identical until a castle becomes
//! available: the startpos counts (20 / 400 / 8902 / 197281) match standard chess
//! exactly. The divergence appears in castling-rich positions — the classic
//! Kiwipete (standard perft(1) = 48; no-castle = 46, the two castles removed) and
//! a cleared back-rank rooks-and-kings position — where the missing castle moves
//! drop the node counts.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `nocastle_variant()` (`variant.cpp:62` — standard chess with
//! `castling = false`); the castling field is `-`:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1
//! ```
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_nocastle -- --include-ignored`.

use mcr::geometry::{
    perft as gperft, Chess8x8, Nocastle, Square, WideMoveKind, WidePiece, WideRole,
};
use mcr::Color;

/// The no-castle starting FEN, confirmed against Fairy-Stockfish's
/// `UCI_Variant nocastle`.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1";

/// Kiwipete: the classic castling-rich perft position. In standard chess perft(1)
/// is 48; no-castle removes the two castles for 46, and the whole subtree shrinks
/// accordingly.
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w - - 0 1";

/// Cleared back rank with both rooks and both kings on their home squares — the
/// position where standard chess offers castling both ways for both sides; here it
/// offers none, so every move is an ordinary king / rook step.
const ROOKS_KINGS: &str = "r3k2r/8/8/8/8/8/8/R3K2R w - - 0 1";

/// Asserts the generic no-castle perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF nocastle `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Nocastle::from_fen(fen).expect("valid no-castle FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "no-castle perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    // Identical to standard chess: a castle is unreachable within these depths.
    check(STARTPOS, &[(1, 20), (2, 400), (3, 8902), (4, 197281)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 4865609)]);
}

// -- Kiwipete: castling divergence (FSF-confirmed) --------------------------

#[test]
fn kiwipete_cheap() {
    // Standard chess: 48 / 2039 / 97862 / 4085603. No-castle drops the castles.
    check(KIWIPETE, &[(1, 46), (2, 1866), (3, 86677), (4, 3504849)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn kiwipete_deep() {
    check(KIWIPETE, &[(5, 161724713)]);
}

// -- Rooks and kings: castling divergence (FSF-confirmed) -------------------

#[test]
fn rooks_kings_cheap() {
    check(ROOKS_KINGS, &[(1, 24), (2, 482), (3, 11522), (4, 261282)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn rooks_kings_deep() {
    check(ROOKS_KINGS, &[(5, 6326061)]);
}

// -- Rule-level self-checks (independent of FSF) ----------------------------

/// The starting array round-trips through FEN, has 20 opening moves, and carries
/// no castling rights for either side.
#[test]
fn startpos_fen_round_trips_without_castling() {
    let pos = Nocastle::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    assert_eq!(pos.turn(), Color::White);
    assert_eq!(pos.legal_move_count(), 20);
    assert!(!pos.castling().has_any(Color::White));
    assert!(!pos.castling().has_any(Color::Black));

    let board = pos.board();
    // Standard back rank a..h: R N B Q K B N R.
    let back = [
        WideRole::Rook,
        WideRole::Knight,
        WideRole::Bishop,
        WideRole::Queen,
        WideRole::King,
        WideRole::Bishop,
        WideRole::Knight,
        WideRole::Rook,
    ];
    for (file, role) in back.iter().enumerate() {
        let sq = Square::<Chess8x8>::from_file_rank(file as u8, 0).unwrap();
        assert_eq!(
            board.piece_at(sq),
            Some(WidePiece::new(Color::White, *role)),
            "white back-rank file {file}",
        );
    }
}

/// A castling-rich position (both kings home, both rooks on the corner files,
/// empty back rank) emits **no** castle move — the single rule that separates
/// no-castle chess from standard chess.
#[test]
fn no_castle_moves_are_ever_generated() {
    let pos = Nocastle::from_fen(ROOKS_KINGS).expect("valid FEN");
    let castles = pos
        .legal_moves()
        .into_iter()
        .filter(|m| {
            matches!(
                m.kind(),
                WideMoveKind::CastleKingside | WideMoveKind::CastleQueenside
            )
        })
        .count();
    assert_eq!(castles, 0, "no-castle chess never castles");
    // The king on e1 still has ordinary steps; the position is not stuck.
    assert!(pos.legal_move_count() > 0);
}

/// Every non-castling rule is standard chess: a pawn promotes to Queen, Rook,
/// Bishop, or Knight on the last rank.
#[test]
fn pawn_promotes_to_four_standard_roles() {
    let pos = Nocastle::from_fen("4k3/1P6/8/8/8/8/8/4K3 w - - 0 1").expect("valid FEN");
    let mut promo_roles: Vec<WideRole> = pos
        .legal_moves()
        .into_iter()
        .filter_map(|m| m.promotion())
        .collect();
    promo_roles.sort();
    promo_roles.dedup();
    let mut want = vec![
        WideRole::Knight,
        WideRole::Bishop,
        WideRole::Rook,
        WideRole::Queen,
    ];
    want.sort();
    assert_eq!(promo_roles, want, "standard four-role promotion set");
}

/// A double pawn push still sets an en-passant target.
#[test]
fn pawn_double_push_sets_en_passant() {
    let pos = Nocastle::startpos();
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
