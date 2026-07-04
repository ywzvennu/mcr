//! Micro Shogi (4x5 / `u64`) perft validation on the generic engine (issue #405)
//! — Shogi shrunk onto a four-by-five board with one each of King, Rook, Bishop,
//! Lance, and Pawn per side, several starting **pre-promoted**, and a promotion
//! mechanic all its own: a piece **flips form on every capture** (there is no
//! promotion zone).
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Micro::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant micro`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position. This test pins the FSF-confirmed numbers so a
//! regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `micro` start (`position startpos`):
//!
//! ```text
//! kb+r+l/p3/4/3P/+L+RBK[] w - - 0 1
//! ```
//!
//! mcr renders the same position with an empty `[]` holdings bracket (its hand is
//! empty at the start). The piece letters coincide with FSF's — `k b r l p` and
//! the `+`-prefixed promoted forms `+R +L +P` — so no FEN dialect rewrite is
//! needed. The FSF-confirmed startpos perft sequence is
//! `9, 80, 767, 7256, 71328`.
//!
//! ## The capture flip
//!
//! There is **no promotion zone**. Instead a piece toggles between its base and
//! promoted form **whenever — and only when — it captures**: `Pawn ↔ +P` (Knight),
//! `Lance ↔ +L` (Silver), `Rook ↔ +R` (Gold), `Bishop ↔ +B` (Gold). A base piece
//! promotes on its capturing move, a promoted piece demotes; a quiet move never
//! flips. The `capture_flip` case exercises this: a Rook slides up and captures,
//! flipping to a Gold-moving `+R`.
//!
//! ## Drops (no restrictions)
//!
//! A captured piece banks **unpromoted** into the captor's hand and may be dropped
//! onto **any empty square** — Micro Shogi has no nifu, no dead-piece rule, and no
//! `dropPromoted` (a held piece deploys only in its base form). The `multi_hand`
//! case, with every base role in each hand, pins the drop-heavy move set.
//!
//! As with Shogi (#190), FSF's `micro` perft does **not** enforce *uchifuzume*, so
//! mcr does not filter a mating pawn drop either.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_micro -- --include-ignored`.

use mcr::geometry::{perft as gperft, Micro, Micro4x5};

/// The Micro Shogi starting FEN, confirmed against Fairy-Stockfish's
/// `UCI_Variant micro`. The Rook and Lance start pre-promoted; the hand is empty.
const STARTPOS: &str = "kb+r+l/p3/4/3P/+L+RBK[] w - - 0 1";

/// A white Rook on b2 that can slide up and capture a black Lance on b4, **flipping
/// to a Gold-moving `+R`** on the capture — the Micro Shogi capture-flip mechanic.
/// FSF-confirmed.
const CAPTURE_FLIP: &str = "3k/1l2/4/1R2/K3[] w - - 0 1";

/// Bare kings with **one of every droppable base role in each hand** (Rook, Bishop,
/// Lance, Pawn), white to move: drops dominate the move set with no nifu / dead-piece
/// filter, stressing every drop square and the check-blocking drop legality at depth.
/// FSF-confirmed.
const MULTI_HAND: &str = "3k/4/4/4/K3[RBLPrblp] w - - 0 1";

/// White's three promoted forms on the board — a Gold-moving `+R` on b3, a
/// Silver-moving `+L` on a2, and a Knight-moving `+P` on c2 — exercising each
/// promoted attack set (and, on capture, the demotion back to base). FSF-confirmed.
const PROMOTED_FORMS: &str = "3k/4/1+R2/+L1+P1/K3[] w - - 0 1";

/// Asserts the generic Micro perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `micro` `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Micro::from_fen(fen).expect("valid Micro FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Micro4x5, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Micro perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) ------------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 9), (2, 80), (3, 767), (4, 7256)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 71328)]);
}

// -- Capture flip: Rook captures and promotes to Gold (FSF-confirmed) --------

#[test]
fn capture_flip() {
    check(CAPTURE_FLIP, &[(1, 8), (2, 33), (3, 248), (4, 1346)]);
}

// -- Multi-hand: drops dominate, no restrictions (FSF-confirmed) -------------

#[test]
fn multi_hand_cheap() {
    check(MULTI_HAND, &[(1, 147), (2, 17254)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn multi_hand_deep() {
    check(MULTI_HAND, &[(3, 1511573)]);
}

// -- Promoted-form movement (Gold / Silver / Knight) (FSF-confirmed) ---------

#[test]
fn promoted_forms() {
    check(PROMOTED_FORMS, &[(1, 12), (2, 16), (3, 168)]);
}
