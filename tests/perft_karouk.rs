//! Ka Ouk (Kar Ouk) perft validation on the generic engine (issue #404).
//!
//! Ka Ouk is Cambodian chess (Makruk plus the one-time king / Met first-move
//! leaps) with one terminal twist: **giving check wins the game**. The node counts
//! below are pinned **and cross-checked against Fairy-Stockfish** (FSF,
//! `UCI_Variant karouk`): every `(depth, nodes)` pair here was produced identically
//! by `mcr::geometry::Karouk::perft` and by FSF's `go perft` on the byte-identical
//! FEN. The `compare-fairy/` harness re-runs that head-to-head on demand (its
//! `--difffuzz --variant karouk` mode); this test pins the confirmed numbers so a
//! regression is caught without FSF present.
//!
//! Confirmed Ka Ouk starting FEN (from FSF `position startpos`):
//!   `rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w DEde - 1+1 0 1`
//!
//! mcr omits the `1+1` check-counter field (a single check is terminal, so mcr
//! keeps no counter); FSF fills it with its default, so the two see the identical
//! position. The `DEde` leap-rights field is exactly Cambodian's.
//!
//! Because the check-win only prunes a subtree once a check is actually delivered,
//! Ka Ouk's perft equals Cambodian's through depth 5 from the start and first
//! diverges at depth 6 (204_573_392 here vs Cambodian's 204_583_970 — the pruned
//! checking replies).
//!
//! The cheap layers run as ordinary tests; the deep layers are `#[ignore]`d so
//! `cargo test` stays fast — run them with
//! `cargo test --release --test perft_karouk -- --include-ignored`.

use mcr::geometry::{
    perft as gperft, Chess8x8, Karouk, Square, WideMoveKind, WideOutcome, WideRole,
};
use mcr::Color;

/// The Ka Ouk starting FEN, confirmed byte-for-byte against Fairy-Stockfish's
/// `UCI_Variant karouk` / `position startpos`.
const STARTPOS: &str = "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w DEde - 0 1";

/// A midgame position with both leap rights still live and an open centre — the
/// same board as the Cambodian corpus, where no check yet arises, so the counts
/// equal Cambodian's.
const MID_LEAPS: &str = "rnsmksnr/8/pp1ppp1p/2p2p2/2P2P2/PP1PPP1P/8/RNSKMSNR w DEde - 0 3";

/// A rook-endgame position where White's Ra1-a8 delivers check along the eighth
/// rank — exercising the check-win pruning at depth 2 (the checking reply is
/// terminal, so it contributes no grandchildren).
const CHECK_WIN: &str = "4k3/8/8/8/8/8/8/R3K3 w - - 0 1";

/// Asserts the generic Ka Ouk perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF karouk `go perft` on the same FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Karouk::from_fen(fen).expect("valid Ka Ouk FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Karouk perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(
        STARTPOS,
        &[(1, 25), (2, 625), (3, 15031), (4, 361719), (5, 8597966)],
    );
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_depth6() {
    // FSF karouk `go perft 6` on the startpos: 204_573_392 — Cambodian's
    // 204_583_970 minus the checking replies the check-win rule prunes.
    check(STARTPOS, &[(6, 204573392)]);
}

// -- Midgame with both leaps live (FSF-confirmed) ---------------------------

#[test]
fn mid_leaps_cheap() {
    check(MID_LEAPS, &[(1, 23), (2, 532), (3, 12148), (4, 277910)]);
}

// -- The check-win pruning (FSF-confirmed) ----------------------------------

#[test]
fn check_win_prunes_at_depth_two() {
    // FSF karouk `go perft` on the rook endgame: perft(1) = 15, perft(2) = 65 (the
    // Ra8+ reply is terminal, so it contributes no grandchildren).
    check(CHECK_WIN, &[(1, 15), (2, 65)]);
}

// -- Rule-level self-checks (independent of FSF) ----------------------------

/// The starting array round-trips through FEN (including the `DEde` leap-rights
/// field) and matches the confirmed string; the opening move count is 25 (Makruk's
/// 23 plus the king's two forward-knight leaps), exactly as in Cambodian.
#[test]
fn startpos_fen_round_trips() {
    let pos = Karouk::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    assert_eq!(pos.turn(), Color::White);
    assert_eq!(pos.legal_move_count(), 25);
}

/// Delivering check **wins the game**: after White's Ra1-a8+, Black (the side to
/// move) is in check, so the position is terminal — Black has no legal reply and
/// the outcome credits White the win.
#[test]
fn giving_check_wins() {
    let pos = Karouk::from_fen(CHECK_WIN).expect("valid");
    let a1 = Square::from_file_rank(0, 0).unwrap();
    let a8 = Square::from_file_rank(0, 7).unwrap();
    let check_move = pos
        .legal_moves()
        .into_iter()
        .find(|m| m.from::<Chess8x8>() == a1 && m.to::<Chess8x8>() == a8)
        .expect("Ra1-a8 is legal");
    let after = pos.play(&check_move);
    assert_eq!(after.turn(), Color::Black);
    assert!(after.is_check(), "Black is in check after Ra8+");
    assert_eq!(
        after.legal_move_count(),
        0,
        "the checked side has no reply (the game is already won)"
    );
    let outcome = after.outcome().expect("the game is over");
    assert_eq!(
        outcome,
        WideOutcome::Decisive {
            winner: Color::White
        },
        "the checker wins"
    );
}

/// The Ka Ouk king keeps Cambodian's one-time forward-knight leap: from the start
/// it reaches both b2 and f2.
#[test]
fn king_keeps_cambodian_leap() {
    let pos = Karouk::startpos();
    let king_from = Square::from_file_rank(3, 0).unwrap(); // d1
    let b2 = Square::from_file_rank(1, 1).unwrap();
    let f2 = Square::from_file_rank(5, 1).unwrap();
    let dests: Vec<_> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| m.from::<Chess8x8>() == king_from)
        .map(|m| m.to::<Chess8x8>())
        .collect();
    assert!(dests.contains(&b2), "king leaps to b2");
    assert!(dests.contains(&f2), "king leaps to f2");
}

/// A Bia (pawn) makes only a single-step advance — the Makruk rule is unchanged —
/// so no en-passant target is ever created, and it promotes to a Met only.
#[test]
fn pawn_rules_are_makruk() {
    let pos = Karouk::startpos();
    let has_double = pos
        .legal_moves()
        .into_iter()
        .any(|m| matches!(m.kind(), WideMoveKind::DoublePawnPush));
    assert!(!has_double, "Ka Ouk pawns never double-push");

    let promo = Karouk::from_fen("7k/8/8/4P3/8/8/8/K7 w - - 0 1").expect("valid");
    let promos: Vec<_> = promo
        .legal_moves()
        .into_iter()
        .filter_map(|m| m.promotion())
        .collect();
    assert_eq!(promos, vec![WideRole::Met]);
}
