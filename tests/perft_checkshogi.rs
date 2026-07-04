//! Checkshogi (Check Shogi, 9x9 / `u128`) perft validation on the generic engine
//! (issue #406) — **standard 9x9 Shogi** ([`Shogi`](mcr::geometry::Shogi)) with a
//! single terminal twist: **giving check wins the game**. Every piece, the
//! persistent hand, drops, and the promotion zone are Shogi's; the rule layer
//! delegates every move-generation hook to `ShogiRules` and overrides only the
//! terminal condition.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::CheckShogi::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant checkshogi`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position. The `compare-fairy/` harness re-runs that head-to-head
//! on demand (`--difffuzz --variant checkshogi`, 0 divergences); this test pins
//! the FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `checkshogi` start (`position startpos`):
//!
//! ```text
//! lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL[] w - - 1+1 0 1
//! ```
//!
//! The `1+1` field is FSF's per-side **check counter** (one check to win). Because
//! a single check is terminal, mcr keeps no counter and omits the field; FSF fills
//! the absent field with its `1+1` default, so the two see the byte-identical
//! position and no dialect rewrite is needed. Since no opening move gives check,
//! the startpos perft sequence coincides with Shogi's:
//! `30, 900, 25470, 719408, 19839626` (Shogi's perft-4/5 are `719731`/`19861490` —
//! the check-win truncation of the checking moves at ply 3 accounts for the
//! difference).
//!
//! ## The check-win rule (perft truncation)
//!
//! A move that **gives check wins immediately**, so the checked side has zero
//! legal replies — the move generator truncates that node exactly as FSF's
//! `go perft` does (a checking move lists no successors). The positions below are
//! all clean (no side is in check at the root) and each has moves that *deliver*
//! check within the tree, so their perft counts exercise the truncation. See
//! `check_win_is_terminal` for the terminal reporting.

use mcr::geometry::{perft as gperft, CheckShogi, Shogi9x9, WideEndReason, WideOutcome};
use mcr::Color;

/// The Checkshogi starting FEN (mcr omits FSF's `1+1` check-counter field). The
/// hand is empty (`[]`).
const STARTPOS: &str = "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL[] w - - 0 1";

/// A white Rook on b1 (not yet checking) with the enemy King on the open a-file,
/// white to move and no side in check: sliding the Rook onto the a-file (e.g.
/// `b1a1`) delivers check and wins, so those moves truncate to zero replies. Its
/// perft counts reflect that truncation. FSF-confirmed.
const CHECK_TRUNCATION: &str = "k8/9/9/9/9/9/9/9/1R4K2[] w - - 0 1";

/// A Rook in white's hand with the enemy King on the open e-file, white to move:
/// dropping the Rook onto the e-file (e.g. `R@e5`) delivers check and wins, so
/// those drops truncate. FSF-confirmed.
const ROOK_DROP_CHECK: &str = "4k4/9/9/9/9/9/9/9/6K2[R] w - - 0 1";

/// A Rook on the board plus a Gold and a Pawn in hand, kings apart, white to
/// move: a busier position mixing board moves, drops, and several checking
/// continuations that truncate at depth. FSF-confirmed.
const HANDS_MIDBOARD: &str = "4k4/9/9/9/9/9/1R7/9/6K2[Gp] w - - 0 1";

/// Asserts the generic Checkshogi perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `checkshogi` `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = CheckShogi::from_fen(fen).expect("valid Checkshogi FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Shogi9x9, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Checkshogi perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) ------------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 30), (2, 900), (3, 25470)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(4, 719408), (5, 19839626)]);
}

// -- Check-win truncation, board move (FSF-confirmed) ------------------------

#[test]
fn check_truncation() {
    check(CHECK_TRUNCATION, &[(1, 21), (2, 24), (3, 534)]);
}

// -- Check-win truncation, drop (FSF-confirmed) ------------------------------

#[test]
fn rook_drop_check() {
    check(ROOK_DROP_CHECK, &[(1, 84), (2, 288), (3, 8876)]);
}

// -- Busier hands + board position (FSF-confirmed) ---------------------------

#[test]
fn hands_midboard_cheap() {
    check(HANDS_MIDBOARD, &[(1, 102), (2, 6958)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn hands_midboard_deep() {
    check(HANDS_MIDBOARD, &[(3, 302716)]);
}

// -- The check-win terminal --------------------------------------------------

/// A position whose side to move is in check is terminal: the checker (the side
/// *not* to move) has won, there are zero legal moves, and the outcome is a
/// variant win. (This is mcr's rule read off a single position; FSF's `go perft`
/// on such a *root* resets its check counter and would instead let the king
/// escape, so this exact node is not perft-pinned against FSF — the in-tree
/// truncation the perft cases above exercise is what FSF confirms.)
#[test]
fn check_win_is_terminal() {
    // Black king on the open e-file with a White rook bearing down it: Black, to
    // move, is in check, so White (the checker) has already won.
    let pos =
        CheckShogi::from_fen("4k4/9/9/9/9/9/9/9/4R1K2[] b - - 0 1").expect("valid Checkshogi FEN");
    assert!(pos.is_check(), "Black to move is in check");
    assert!(pos.legal_moves().is_empty(), "a checked side has no reply");
    assert_eq!(
        gperft::<Shogi9x9, _>(&pos, 1),
        0,
        "the node is a perft leaf"
    );
    assert_eq!(pos.end_reason(), Some(WideEndReason::VariantWin));
    assert_eq!(
        pos.outcome(),
        Some(WideOutcome::Decisive {
            winner: Color::White
        }),
        "the checker (White) wins",
    );
}
