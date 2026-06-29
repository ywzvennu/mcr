//! Makpong ("Defensive Chess") perft validation on the generic engine
//! (issue #260).
//!
//! Makpong is Makruk with one extra rule: while the side to move is in check,
//! its king may not flee — it may move **only to capture the lone checker**, and
//! the check must otherwise be answered by another piece (a block or a capture).
//! It reuses the entire Makruk rule layer; the sole delta is the default-off
//! [`king_may_only_capture_checker`] hook.
//!
//! The node counts below are pinned **and cross-checked against Fairy-Stockfish**
//! (FSF, `UCI_Variant makpong`): every `(depth, nodes)` pair here was produced
//! identically by `mce::geometry::Makpong::perft` and by FSF's `go perft` on the
//! byte-identical FEN. The `compare-fairy/` harness re-runs that head-to-head on
//! demand (see `compare-fairy/src/main.rs --makpong`); this test pins the
//! confirmed numbers so a regression is caught without FSF present.
//!
//! Confirmed Makpong starting FEN (from FSF `position startpos`) — identical to
//! Makruk's:
//!   `rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w - - 0 1`
//!
//! The cheap layers run as ordinary tests; the deep layers are `#[ignore]`d so
//! `cargo test` stays fast — run them with
//! `cargo test --release --test perft_makpong -- --include-ignored`.
//!
//! [`king_may_only_capture_checker`]: mce::geometry::WideVariant::king_may_only_capture_checker

use mce::geometry::{perft as gperft, Chess8x8, Makpong, Makruk};

/// The Makpong starting FEN, confirmed byte-for-byte against Fairy-Stockfish's
/// `UCI_Variant makpong` / `position startpos`. It equals Makruk's start array.
const STARTPOS: &str = "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w - - 0 1";

/// A root **in-check** position: a black Rua (rook) on d4 checks the white Khun
/// (king) on d1 down the open d-file. Makpong forbids the three king-flee moves
/// Makruk would allow, so perft(1) is 4 here versus Makruk's 7 — the variant's
/// defining delta, exercised from the very first ply.
const CHECK_ROOK: &str = "rnsmksnr/8/1pp1ppp1/p6p/3r4/PPP1PPPP/8/RNSK1SNR w - - 0 4";

/// A second root in-check position: a black Bia (pawn) on c2 gives a diagonal
/// check to the white Khun on d1, with both a king-capture-of-the-checker and a
/// piece-capture available. Makpong removes the king-flee replies, so perft(1) is
/// 2 versus Makruk's 4.
const CHECK_PAWN: &str = "rnsmksnr/8/ppp1ppp1/7p/8/PP1PPPPP/2pP4/RNSK1SNR w - - 0 5";

/// A quiet developed midgame (the same FEN as the Makruk corpus). No king is in
/// check at the root, but checks arise within the tree, so Makpong and Makruk
/// already diverge by depth 4 (275260 vs 275266).
const MID: &str = "r1smks1r/3n4/ppp1pppp/3p4/3P4/PPP1PPPP/4N3/R1SKMS1R w - - 0 4";

/// Asserts the generic Makpong perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF makpong `go perft` on the same FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Makpong::from_fen(fen).expect("valid Makpong FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Makpong perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(
        STARTPOS,
        &[(1, 23), (2, 529), (3, 12012), (4, 273026), (5, 6223994)],
    );
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_depth6() {
    // FSF makpong `go perft 6` on the startpos: 142_072_366. This is where the
    // rule first bites in the opening tree — Makruk's depth-6 count is the larger
    // 142_078_049, the difference being the king-flee evasions Makpong forbids.
    check(STARTPOS, &[(6, 142072366)]);
}

// -- In check at the root: rook check (FSF-confirmed) -----------------------

#[test]
fn check_rook_cheap() {
    check(
        CHECK_ROOK,
        &[(1, 4), (2, 128), (3, 2640), (4, 83079), (5, 1760926)],
    );
}

// -- In check at the root: pawn (Bia) check (FSF-confirmed) -----------------

#[test]
fn check_pawn_cheap() {
    check(CHECK_PAWN, &[(1, 2), (2, 48), (3, 864), (4, 20744)]);
}

// -- Quiet midgame; diverges from Makruk inside the tree (FSF-confirmed) -----

#[test]
fn mid_cheap() {
    check(
        MID,
        &[(1, 22), (2, 508), (3, 11565), (4, 275260), (5, 6480806)],
    );
}

// -- Rule-level self-checks (independent of FSF) ----------------------------

/// The starting array round-trips through FEN and is byte-identical to Makruk's
/// — Makpong changes only king-in-check legality, never the start position.
#[test]
fn startpos_matches_makruk_and_round_trips() {
    let mp = Makpong::startpos();
    assert_eq!(mp.to_fen(), STARTPOS);
    // The opening is not in check, so Makpong's legal-move count equals Makruk's.
    assert_eq!(mp.legal_move_count(), 23);
    assert_eq!(mp.to_fen(), Makruk::startpos().to_fen());
}

/// The defining rule: while in check the king may move **only to capture the
/// lone checker**, never to a safe flee square. In `CHECK_ROOK` the checking
/// rook on d4 is not reachable by the king, so Makpong drops every king move
/// while Makruk keeps three flee squares.
#[test]
fn king_may_not_flee_check() {
    let mp = Makpong::from_fen(CHECK_ROOK).expect("valid");
    let mr = Makruk::from_fen(CHECK_ROOK).expect("valid");
    assert!(mp.is_check(), "the position is in check");

    // No Makpong move starts from the king square here — the rook checker on d4
    // is not on a square the king can reach, so the king cannot legally move.
    let king_sq = mp.board().king_of(mce::Color::White).expect("a king");
    let mp_king_moves = mp
        .legal_moves()
        .into_iter()
        .filter(|m| m.from::<Chess8x8>() == king_sq)
        .count();
    assert_eq!(
        mp_king_moves, 0,
        "Makpong king cannot flee or block-by-moving"
    );

    // Makruk, by contrast, lets the king flee — so its total exceeds Makpong's.
    assert_eq!(mp.legal_move_count(), 4);
    assert_eq!(mr.legal_move_count(), 7);
}

/// When the lone checker sits on a square the king itself can capture, Makpong
/// permits exactly that king move (and only it, among king moves). In
/// `CHECK_PAWN` the checking Bia on c2 is diagonally adjacent to the king on d1,
/// so the king may capture it; the other reply is a piece capturing c2.
#[test]
fn king_may_capture_the_lone_checker() {
    let mp = Makpong::from_fen(CHECK_PAWN).expect("valid");
    assert!(mp.is_check());
    let king_sq = mp.board().king_of(mce::Color::White).expect("a king");
    let king_dests: Vec<_> = mp
        .legal_moves()
        .into_iter()
        .filter(|m| m.from::<Chess8x8>() == king_sq)
        .map(|m| m.to::<Chess8x8>())
        .collect();
    // Exactly one king move, and it lands on the checker's square (c2).
    assert_eq!(
        king_dests.len(),
        1,
        "only the capture-the-checker king move"
    );
    let c2 = mce::geometry::Square::<Chess8x8>::from_file_rank(2, 1).unwrap();
    assert_eq!(
        king_dests[0], c2,
        "the king captures the lone checker on c2"
    );
    // Two legal moves total: the king capture and a piece capture of the Bia.
    assert_eq!(mp.legal_move_count(), 2);
}
