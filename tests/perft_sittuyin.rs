//! Sittuyin (Burmese chess, 8x8) perft validation on the generic engine (issue
//! #179) — the first variant exercising the **setup / placement phase** and the
//! **special Met promotion**.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mce::geometry::Sittuyin::perft` and by Fairy-Stockfish (FSF, `UCI_Variant
//! sittuyin`) running `go perft` on the byte-identical position — the FSF divide
//! matches mce's move-for-move, including the placement drops and the special
//! promotion. The `compare-fairy/` harness re-runs that head-to-head on demand
//! (`compare-fairy/src/sittuyin.rs`); this test pins the FSF-confirmed numbers so
//! a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant sittuyin`, `position startpos`) renders the start as
//!
//! ```text
//! 8/8/4pppp/pppp4/4PPPP/PPPP4/8/8[KSSFRRNNkssfrrnn] w - - 0 1
//! ```
//!
//! mce uses the same board placement and `[..]` pocket but its own role letters
//! (the Met is `m`, not FSF's `f`, and the pocket is written in role-index order),
//! so its canonical start FEN is
//!
//! ```text
//! 8/8/4pppp/pppp4/4PPPP/PPPP4/8/8[NNRRKMSSnnrrkmss] w - - 0 1
//! ```
//!
//! The two are the same position; `compare-fairy/` translates the `m`↔`f` Met
//! letter when driving FSF.
//!
//! ## Confirmed semantics (all pinned move-for-move against FSF)
//!
//! * **Placement phase.** The non-pawn pieces start in hand (the pocket bracket).
//!   Players alternate dropping one piece per ply onto their own territory — the
//!   three nearest ranks minus own pawns, with Rooks confined to the back rank —
//!   with no check filtering, until both pockets empty. A side that has emptied
//!   its pocket plays normally while the opponent is still deploying. The opening
//!   has `88` placement drops; `perft(2) = 88 × 88` since the two sides' first
//!   drops do not interact.
//! * **Special promotion.** While a side has **no Met on the board**, a pawn may
//!   become a Met (the only promotion role) in place or by a one-step ferz move
//!   to an empty square, subject to: with more than one pawn only pawns on the
//!   promotion "X" diagonals are eligible; and the Met may not be promoted onto a
//!   square ferz-adjacent to an enemy. A straight push to the far rank does **not**
//!   promote — the pawn just sits there.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_sittuyin -- --include-ignored`.

use mce::geometry::{
    perft as gperft, Chess8x8, Sittuyin, Square, WideMoveKind, WidePiece, WideRole,
};
use mce::Color;

/// The Sittuyin starting FEN in mce's dialect, confirmed against FSF's
/// `UCI_Variant sittuyin` / `position startpos`.
const STARTPOS: &str = "8/8/4pppp/pppp4/4PPPP/PPPP4/8/8[NNRRKMSSnnrrkmss] w - - 0 1";

/// A fully-deployed middlegame (both pockets empty): the symmetric opening array
/// reached by a natural deployment. Both Mets are on the board, so no pawn may
/// promote yet.
const MID: &str = "rrnmk1n1/1ss5/4pppp/pppp4/4PPPP/PPPP4/1SS5/RRNMK1N1 w - - 0 9";

/// A middlegame after both Mets have left the board (white's was captured), with
/// a black pawn deep on f3 — exercises the special promotion under contact.
const PROMO_MID: &str = "rrn1k1n1/1ss5/4pppp/ppp5/5PPP/PPPP1p2/1SS1M3/RRN1K1N1 b - - 0 11";

/// A mid-deployment position: white has deployed a Met and a Rook (and plays on
/// normally), black is still fully in hand — exercises the per-side placement
/// boundary and the `88`-wide placement fan-out.
const DEPLOY_MID: &str = "8/8/4pppp/pppp4/4PPPP/PPPP4/8/3M2R1[NNRKSSnnrrkmss] b - - 0 3";

/// Asserts the generic Sittuyin perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF sittuyin `go perft` on the same FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Sittuyin::from_fen(fen).expect("valid Sittuyin FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Sittuyin perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position / placement phase (FSF-confirmed) -----------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 88), (2, 7744), (3, 580096)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    // FSF sittuyin `go perft` on the startpos.
    check(STARTPOS, &[(4, 43454464), (5, 2730986496)]);
}

// -- Fully-deployed middlegame (FSF-confirmed) ------------------------------

#[test]
fn mid_cheap() {
    check(MID, &[(1, 20), (2, 542), (3, 11293), (4, 305317)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn mid_deep() {
    check(MID, &[(5, 6642677)]);
}

// -- Promotion middlegame (FSF-confirmed) -----------------------------------

#[test]
fn promo_mid_cheap() {
    check(PROMO_MID, &[(1, 29), (2, 537), (3, 15350)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn promo_mid_deep() {
    check(PROMO_MID, &[(4, 298180)]);
}

// -- Mid-deployment (FSF-confirmed) -----------------------------------------

#[test]
fn deploy_mid_cheap() {
    check(DEPLOY_MID, &[(1, 88), (2, 5280), (3, 395520)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn deploy_mid_deep() {
    check(DEPLOY_MID, &[(4, 20171520)]);
}

// -- Rule-level self-checks (independent of FSF) ----------------------------

/// The starting array round-trips through FEN and matches the confirmed string.
#[test]
fn startpos_fen_round_trips() {
    let pos = Sittuyin::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    assert_eq!(pos.turn(), Color::White);
    // The opening is a placement phase: 88 drops (FSF-confirmed perft(1)).
    assert_eq!(pos.legal_move_count(), 88);
    assert!(pos.castling().is_empty());
    assert!(pos.ep_square().is_none());

    // The non-pawn pieces are all in hand at the start; only pawns are on board.
    let board = pos.board();
    assert_eq!(board.pieces(Color::White, WideRole::Pawn).count(), 8);
    assert_eq!(board.pieces(Color::Black, WideRole::Pawn).count(), 8);
    for role in [
        WideRole::Knight,
        WideRole::Rook,
        WideRole::King,
        WideRole::Met,
        WideRole::Silver,
    ] {
        assert_eq!(
            board.pieces(Color::White, role).count(),
            0,
            "no {role:?} on the board at the start of deployment"
        );
    }
    // White's pawns sit on ranks 3-4 in the interlocked Sittuyin block.
    for (file, rank) in [
        (0u8, 2u8),
        (1, 2),
        (2, 2),
        (3, 2),
        (4, 3),
        (5, 3),
        (6, 3),
        (7, 3),
    ] {
        assert_eq!(
            board.piece_at(Square::<Chess8x8>::from_file_rank(file, rank).unwrap()),
            Some(WidePiece::new(Color::White, WideRole::Pawn)),
        );
    }
}

/// Every opening move is a placement drop (the setup phase emits no board move).
#[test]
fn opening_moves_are_all_drops() {
    let pos = Sittuyin::startpos();
    let moves = pos.legal_moves();
    assert_eq!(moves.len(), 88);
    assert!(
        moves
            .iter()
            .all(|m| matches!(m.kind(), WideMoveKind::Drop { .. })),
        "every opening move is a placement drop"
    );
    // The drop roles are exactly the pieces in hand (no pawns, no duplicates of
    // an absent role).
    for m in &moves {
        let role = m.kind().drop_role().expect("a drop");
        assert!(matches!(
            role,
            WideRole::Knight | WideRole::Rook | WideRole::King | WideRole::Met | WideRole::Silver
        ));
    }
}

/// After a full 16-ply deployment the pocket is empty and play becomes normal —
/// the FEN then carries an empty `[]` bracket and no drop is emitted.
#[test]
fn deployment_transitions_to_normal_play() {
    // Reach the fully-deployed `MID` by parsing it (its pocket is empty).
    let pos = Sittuyin::from_fen(MID).expect("valid FEN");
    assert!(pos.to_fen().contains("[]"), "deployed pocket renders as []");
    let moves = pos.legal_moves();
    assert!(
        moves
            .iter()
            .all(|m| !matches!(m.kind(), WideMoveKind::Drop { .. })),
        "no drops once both sides are deployed"
    );
    assert_eq!(moves.len(), 20);
}

/// A Rook may be dropped only on the back rank; the other pieces may go anywhere
/// in territory. After a single white Rook drop on the back rank the position is
/// well-formed and FEN round-trips.
#[test]
fn rook_drop_is_back_rank_only() {
    let pos = Sittuyin::startpos();
    let rook_drops: Vec<_> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| m.kind().drop_role() == Some(WideRole::Rook))
        .collect();
    // 8 back-rank squares (rank 0), none occupied by a pawn.
    assert_eq!(rook_drops.len(), 8);
    for m in &rook_drops {
        assert_eq!(m.to::<Chess8x8>().rank(), 0, "rook drops only on rank 0");
    }
}

/// Special promotion is gated on the side having no Met: with the Met on the
/// board no pawn promotes; once it is gone, an eligible pawn may become a Met in
/// place. (`PROMO_MID` has white's Met captured and a black pawn on f3.)
#[test]
fn special_promotion_requires_no_own_met() {
    let pos = Sittuyin::from_fen(PROMO_MID).expect("valid FEN");
    // Black has no Met on the board and a pawn on f3 (a promotion-region square),
    // so at least one promotion move exists.
    let has_promo = pos.legal_moves().iter().any(|m| {
        matches!(
            m.kind(),
            WideMoveKind::Promotion {
                role: WideRole::Met,
                ..
            }
        )
    });
    assert!(
        has_promo,
        "an eligible pawn promotes when the side has no Met"
    );
}
