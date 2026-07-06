//! Torpedo chess (8x8) perft validation on the generic engine — standard chess in
//! which a pawn may make its **two-square advance from any rank** (not only its
//! starting rank), and nothing else changed.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Torpedo::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant torpedo`, the built-in `chess_variant_base()` with the pawn
//! double-step region set to every square) running `go perft` on the byte-identical
//! position. mcr and FSF spell torpedo chess with the same standard-chess letters,
//! so no dialect rewrite is needed. The `compare-fairy/` differential fuzzer
//! re-runs that head-to-head on demand; this test pins the FSF-confirmed numbers so
//! a regression is caught even without FSF present.
//!
//! ## Where it diverges from standard chess
//!
//! From the opening array the two games are identical only at depths 1-2 (every
//! pawn already stands on its start rank, so both games' first double-steps
//! coincide): the startpos counts 20 / 400 match standard chess. By **depth 3** the
//! torpedo count is 9194 (standard chess: 8902) — mid-board pawns have gained their
//! extra double-step — and the two diverge ever more with depth (depth 4: 209719 vs
//! 197281; depth 5: 5402551 vs 4865609).
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `torpedo` built-in (standard chess with `doubleStepRegion` = all
//! squares for both colours); every field is the standard-chess start:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! ```
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_torpedo -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Torpedo, WideMove, WideMoveKind};
use mcr::Color;

/// The generic `Square` renders as `(file,rank)` zero-based coordinates, so a move
/// is identified by its `(from, to)` file/rank rather than an algebraic string.
/// Files/ranks are 0-based: e = file 4, and rank N is index N-1.
fn is_move(m: &WideMove, ff: u8, fr: u8, tf: u8, tr: u8) -> bool {
    let from = m.from::<Chess8x8>();
    let to = m.to::<Chess8x8>();
    from.file() == ff && from.rank() == fr && to.file() == tf && to.rank() == tr
}

/// The torpedo starting FEN, confirmed against Fairy-Stockfish's
/// `UCI_Variant torpedo`.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// Kiwipete: the classic castling-rich perft position. In torpedo chess its many
/// mid-board pawns each gain a double-step, so the counts run **above** standard
/// chess (standard perft(1) = 48 coincides at depth 1, then torpedo pulls ahead).
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

/// A middlegame position with both central white pawns already on the **fourth
/// rank** (and black pawns on the fifth): rich in mid-board double-steps and the
/// captures they open, well beyond any position standard chess and torpedo share.
const MIDBOARD: &str = "rnbqkbnr/pp2pppp/8/2pp4/3PP3/8/PPP2PPP/RNBQKBNR w KQkq - 0 1";

/// A bare-pawn position exercising **en passant off a mid-board double-step**:
/// white's e4 pawn leaps to e6, and black's d6 pawn may then capture it en passant
/// onto e5 — a capture that only ever arises in torpedo, off a non-starting-rank
/// double-step.
const EP_MIDBOARD: &str = "4k3/8/3p4/8/4P3/8/8/4K3 w - - 0 1";

/// Asserts the generic torpedo perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF torpedo `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Torpedo::from_fen(fen).expect("valid torpedo FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "torpedo perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    // Depths 1-2 match standard chess; depth 3 already diverges (std: 8902) as
    // mid-board pawns gain their double-step; depth 4 (std: 197281) diverges more.
    check(STARTPOS, &[(1, 20), (2, 400), (3, 9194), (4, 209719)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    // Standard chess perft(5) is 4865609; torpedo's extra double-steps lift it.
    check(STARTPOS, &[(5, 5402551)]);
}

// -- Kiwipete: mid-board double-step divergence (FSF-confirmed) --------------

#[test]
fn kiwipete_cheap() {
    check(KIWIPETE, &[(1, 48), (2, 2082), (3, 100003)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn kiwipete_deep() {
    check(KIWIPETE, &[(4, 4255783)]);
}

// -- Middlegame with fourth-rank pawns (FSF-confirmed) ----------------------

#[test]
fn midboard_cheap() {
    check(MIDBOARD, &[(1, 40), (2, 1231), (3, 47926)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midboard_deep() {
    check(MIDBOARD, &[(4, 1516195)]);
}

// -- En passant off a mid-board double-step (FSF-confirmed) -----------------

#[test]
fn ep_midboard_cheap() {
    check(EP_MIDBOARD, &[(1, 7), (2, 49), (3, 414), (4, 3161)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn ep_midboard_deep() {
    check(EP_MIDBOARD, &[(5, 27344), (6, 214394)]);
}

// -- Rule-level self-checks (independent of FSF) ----------------------------

/// The starting array round-trips through FEN, has 20 opening moves (identical to
/// standard chess at the start), and carries the standard castling rights.
#[test]
fn startpos_fen_round_trips() {
    let pos = Torpedo::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    assert_eq!(pos.turn(), Color::White);
    assert_eq!(pos.legal_move_count(), 20);
    assert!(pos.castling().has_any(Color::White));
    assert!(pos.castling().has_any(Color::Black));
}

/// A pawn on a **non-starting** rank double-steps when the two squares ahead are
/// empty — the defining torpedo rule. The move is a `DoublePawnPush` and sets the
/// en-passant target on the intermediate square.
#[test]
fn pawn_double_steps_from_a_non_starting_rank() {
    let pos = Torpedo::from_fen("4k3/8/8/8/4P3/8/8/4K3 w - - 0 1").expect("valid FEN");
    // e4 = (4, 3) -> e6 = (4, 5).
    let leap = pos
        .legal_moves()
        .into_iter()
        .find(|m| matches!(m.kind(), WideMoveKind::DoublePawnPush) && is_move(m, 4, 3, 4, 5))
        .expect("a torpedo pawn on e4 leaps to e6");
    let next = pos.play(&leap);
    let ep = next.ep_square().expect("the leap sets an ep target");
    assert_eq!(
        (ep.file(), ep.rank()),
        (4, 4),
        "the ep target of an e4->e6 leap is the intermediate square e5",
    );
}

/// The opponent may capture **en passant** a pawn that double-stepped from a
/// mid-board rank, landing on the intermediate square and removing the leaping
/// pawn. This exercises the full ep round-trip off a non-starting double-step.
#[test]
fn en_passant_captures_a_mid_board_double_stepper() {
    let pos = Torpedo::from_fen(EP_MIDBOARD).expect("valid FEN");
    // e4 = (4, 3) -> e6 = (4, 5).
    let leap = pos
        .legal_moves()
        .into_iter()
        .find(|m| matches!(m.kind(), WideMoveKind::DoublePawnPush) && is_move(m, 4, 3, 4, 5))
        .expect("white leaps e4->e6");
    let after = pos.play(&leap);
    let ep_sq = after.ep_square().expect("the leap sets an ep target");
    assert_eq!((ep_sq.file(), ep_sq.rank()), (4, 4), "ep target e5");

    // d6 = (3, 5) captures en passant onto e5 = (4, 4).
    let ep = after
        .legal_moves()
        .into_iter()
        .find(|m| matches!(m.kind(), WideMoveKind::EnPassant) && is_move(m, 3, 5, 4, 4))
        .expect("black captures en passant d6xe5");
    let done = after.play(&ep);
    assert_eq!(
        done.to_fen(),
        "4k3/8/8/4p3/8/8/8/4K3 w - - 0 2",
        "en passant off a mid-board double-step removes the leaping e6 pawn",
    );
}
