//! Raazuvaa chess (8x8) perft validation on the generic engine — standard chess
//! with **castling and the pawn double-step both disabled** ("the chess of the
//! Maldives") and nothing else changed.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Raazuvaa::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant raazuvaa`, the built-in `raazuvaa_variant()`) running `go perft`
//! on the byte-identical position. mcr and FSF spell raazuvaa with the same
//! standard-chess letters, so no dialect rewrite is needed. The `compare-fairy/`
//! differential fuzzer re-runs that head-to-head on demand; this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Where it diverges from standard chess
//!
//! From the opening array the two games differ immediately: with the double step
//! gone, each pawn has a single push, so the start position has **12** legal moves
//! (eight pawn pushes + four knight hops) rather than standard chess's 20, and the
//! whole tree is much smaller (`12 / 144 / 2124 / 31250` vs `20 / 400 / 8902 /
//! 197281`). Castling-rich positions (Kiwipete, a cleared back-rank rooks-and-kings
//! position) diverge further, the castle moves being absent. Because no double step
//! ever occurs, **no en-passant target is ever created** — en passant can never
//! arise in play, confirmed by the `noep` corpus position where the only pawn move
//! is a single step.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `raazuvaa_variant()` (`variant.cpp:147` — standard chess with
//! `castling = false` and `doubleStep = false`); the castling field is `-`:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1
//! ```
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_raazuvaa -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Raazuvaa, WideMoveKind};
use mcr::Color;

/// The raazuvaa starting FEN, confirmed against Fairy-Stockfish's
/// `UCI_Variant raazuvaa`.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1";

/// Kiwipete: the classic castling-rich perft position. Here both castling and the
/// double step are gone, so its node counts drop well below standard chess.
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w - - 0 1";

/// Cleared back rank with both rooks and both kings on their home squares — the
/// position where standard chess offers castling both ways for both sides; here it
/// offers none, so every move is an ordinary king / rook step (no pawns, so the
/// double step is irrelevant, isolating the castling divergence).
const ROOKS_KINGS: &str = "r3k2r/8/8/8/8/8/8/R3K2R w - - 0 1";

/// A no-en-passant proof position: Black to move with a pawn on d7 and a White pawn
/// on e5. In standard chess Black's `d7d5` would create an en-passant target that
/// White could take with `exd6`; in raazuvaa Black can only single-step `d7d6`, so
/// no double step and no en-passant target ever appears. mcr and FSF agree exactly.
const NOEP: &str = "4k3/3p4/8/4P3/8/8/8/4K3 b - - 0 1";

/// A promotion position exercising the standard Queen/Rook/Bishop/Knight set on the
/// far rank (White pawn on b7, plus a Black pawn racing on g2).
const PROMO: &str = "4k3/1P6/8/8/8/8/6p1/4K3 w - - 0 1";

/// Asserts the generic raazuvaa perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF raazuvaa `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Raazuvaa::from_fen(fen).expect("valid raazuvaa FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "raazuvaa perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    // Diverges from standard chess (20 / 400 / 8902 / 197281) at the first ply: no
    // double step, so each pawn has a single push.
    check(STARTPOS, &[(1, 12), (2, 144), (3, 2124), (4, 31250)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 556525), (6, 9826886)]);
}

// -- Kiwipete: castling + double-step divergence (FSF-confirmed) ------------

#[test]
fn kiwipete_cheap() {
    check(KIWIPETE, &[(1, 44), (2, 1740), (3, 77305), (4, 3034989)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn kiwipete_deep() {
    check(KIWIPETE, &[(5, 133836727)]);
}

// -- Rooks and kings: castling divergence, no pawns (FSF-confirmed) ---------

#[test]
fn rooks_kings_cheap() {
    check(ROOKS_KINGS, &[(1, 24), (2, 482), (3, 11522), (4, 261282)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn rooks_kings_deep() {
    check(ROOKS_KINGS, &[(5, 6326061)]);
}

// -- No en passant: no double step means no ep target ever (FSF-confirmed) --

#[test]
fn noep_cheap() {
    check(NOEP, &[(1, 5), (2, 31), (3, 197), (4, 1508)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn noep_deep() {
    check(NOEP, &[(5, 9958)]);
}

// -- Promotion: standard four-role set on the far rank (FSF-confirmed) ------

#[test]
fn promo_cheap() {
    check(PROMO, &[(1, 8), (2, 59), (3, 596), (4, 5911)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn promo_deep() {
    check(PROMO, &[(5, 70681), (6, 815905)]);
}

// -- Rule-level self-checks (independent of FSF) ----------------------------

/// The starting array round-trips through FEN, has 12 opening moves (no double
/// pushes), and carries no castling rights for either side.
#[test]
fn startpos_fen_round_trips_without_castling_or_double_step() {
    let pos = Raazuvaa::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    assert_eq!(pos.turn(), Color::White);
    assert_eq!(pos.legal_move_count(), 12);
    assert!(!pos.castling().has_any(Color::White));
    assert!(!pos.castling().has_any(Color::Black));
    // No double pawn push appears in the opening.
    assert!(
        !pos.legal_moves()
            .into_iter()
            .any(|m| matches!(m.kind(), WideMoveKind::DoublePawnPush)),
        "raazuvaa pawns never double-step",
    );
}

/// The `noep` position offers exactly one pawn move — the single step `d7d6` — and
/// playing it records no en-passant target, so en passant can never arise.
#[test]
fn no_double_step_means_no_en_passant() {
    let pos = Raazuvaa::from_fen(NOEP).expect("valid FEN");
    // The only piece on the d-file is the d7 pawn; collect its moves.
    let pawn_moves: Vec<_> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| m.from::<Chess8x8>().file() == 3)
        .collect();
    assert_eq!(pawn_moves.len(), 1, "the d7 pawn has a single push only");
    assert!(
        !matches!(pawn_moves[0].kind(), WideMoveKind::DoublePawnPush),
        "the d7 pawn cannot double-step",
    );
    let next = pos.play(&pawn_moves[0]);
    assert!(
        next.ep_square().is_none(),
        "a single pawn push sets no en-passant target",
    );
}

/// A castling-rich position emits no castle move — one of the two rules that
/// separate raazuvaa from standard chess.
#[test]
fn no_castle_moves_are_ever_generated() {
    let pos = Raazuvaa::from_fen(ROOKS_KINGS).expect("valid FEN");
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
    assert_eq!(castles, 0, "raazuvaa never castles");
    assert!(pos.legal_move_count() > 0);
}
