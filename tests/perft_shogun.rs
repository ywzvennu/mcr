//! Shogun (8x8 shogi-chess hybrid) perft validation on the generic engine (issue
//! #227) — the variant combining a **crazyhouse hand with drops**, a
//! **shogi-style optional promotion zone**, and a **per-piece promotion cap**
//! over the standard chess army.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Shogun::perft` and by Fairy-Stockfish (FSF, `UCI_Variant
//! shogun`, with FSF's `variants.ini` loaded) running `go perft` on the
//! byte-identical position — the FSF divide matches mcr's move-for-move, including
//! the optional `+` promotions on a move starting or ending in the far three-rank
//! zone, the promotion cap (one Centaur / Archbishop / Chancellor / Queen apiece),
//! the crazyhouse hand fed by captures (a captured promoted piece banked as its
//! base), and the rank-1-5 (White) / rank-4-8 (Black) drop region. The
//! `compare-fairy/` harness re-runs that head-to-head on demand
//! (`compare-fairy/src/shogun.rs`); this test pins the FSF-confirmed numbers so a
//! regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant shogun`, `position startpos`) renders the start as
//!
//! ```text
//! rnb+fkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNB+FKBNR[] w KQkq - 0 1
//! ```
//!
//! where `+f` / `+F` is a **promoted Fers** that moves as a Queen. mcr represents
//! it with the [`Queen`](mcr::geometry::WideRole::Queen) token, so its canonical
//! start FEN is exactly the standard chess array with an empty holdings bracket:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1
//! ```
//!
//! The two are the same position; `compare-fairy/` translates the promoted-piece
//! tokens when driving FSF.
//!
//! ## Confirmed semantics (all pinned move-for-move against FSF)
//!
//! * **Standard chess movement** for the full army (the d-file "queen" is a
//!   promoted Fers, moving as a Queen).
//! * **Per-piece promotion zone (optional).** The far three ranks (6-8 for White).
//!   A Pawn → Commoner, Knight → Centaur, Bishop → Archbishop, Rook → Chancellor,
//!   or Fers → Queen whose move starts or ends in the zone *may* promote; the
//!   non-promoting move is also legal — except a Pawn reaching the last rank, which
//!   is forced (an unpromoted pawn there would be immobile).
//! * **Promotion cap (FSF `promotionLimit`).** A side may hold at most one Centaur,
//!   Archbishop, Chancellor, and Queen; while at the cap the corresponding
//!   promotion is suppressed (only the plain move remains). The Commoner is
//!   uncapped.
//! * **Crazyhouse hand + drops.** A capture banks the taken piece (reverted to its
//!   base) to the captor's hand; a held piece drops onto an empty square in the
//!   drop region — ranks 1-5 (White) / ranks 4-8 (Black). No nifu; pawns may drop
//!   on the first rank; drop-check / drop-mate are legal.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_shogun -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Shogun};

/// The Shogun starting FEN in mcr's dialect, confirmed against FSF's
/// `UCI_Variant shogun` / `position startpos`.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1";

/// Drops + promotions: a White Knight on e6 (in the zone) and a Rook on d1, with a
/// Knight and Pawn in White's hand and a Bishop and Pawn in Black's. Exercises the
/// optional `+` promotion on a zone move alongside drops from both hands.
const POS_A: &str = "r3k3/8/4N3/8/8/8/8/3RK3[NPbp] w - - 0 1";

/// The promotion cap: a White Centaur (promoted Knight) on a1 already fills the
/// `g:1` cap, while a White Knight sits on e6 inside the promotion zone. Because
/// the cap is reached, that Knight's zone moves offer **only** the non-promoting
/// form — no second Centaur. mcr writes the Centaur with its Kheshig token `W`. A
/// companion Knight rides each hand to keep the drop region live.
const POS_B: &str = "6k1/8/4N3/8/8/8/8/W5K1[Nn] w - - 0 1";

/// Captures feeding the hand: a natural-game midgame after `1.e4 e5 2.d4 d5` with
/// the centre pawns in mutual contact and a Pawn already in each hand. The d-file
/// "queens" are the start array's promoted Fers (mcr `q`), so a queen capture banks
/// a Met (fers) — exercising the crazyhouse hand together with the live drops.
const POS_C: &str = "rnbqkbnr/ppp2ppp/8/3pp3/3PP3/8/PPP2PPP/RNBQKBNR[Pp] w KQkq - 0 4";

/// Asserts the generic Shogun perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF shogun `go perft` on the same FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Shogun::from_fen(fen).expect("valid Shogun FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Shogun perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 20), (2, 400), (3, 8978), (4, 200537)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 5081766)]);
}

// -- Position A: drops + optional promotions (FSF-confirmed) -----------------

#[test]
fn pos_a_cheap() {
    check(POS_A, &[(1, 109), (2, 8824), (3, 682066)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn pos_a_deep() {
    check(POS_A, &[(4, 37705701)]);
}

// -- Position B: promotion cap (FSF-confirmed) ------------------------------

#[test]
fn pos_b_cheap() {
    check(POS_B, &[(1, 56), (2, 2289), (3, 77223)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn pos_b_deep() {
    check(POS_B, &[(4, 948009), (5, 27226968)]);
}

// -- Position C: captures feeding the hand (FSF-confirmed) ------------------

#[test]
fn pos_c_cheap() {
    check(POS_C, &[(1, 62), (2, 3738), (3, 195868)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn pos_c_deep() {
    check(POS_C, &[(4, 10145583)]);
}
