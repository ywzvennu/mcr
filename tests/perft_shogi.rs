//! Shogi (Japanese chess, 9x9 / `u128`) perft validation on the generic engine
//! (issue #190) — the second marquee fairy variant and the most structurally
//! different so far: a new 9x9 geometry, a **persistent capture-fed hand** with
//! **drops**, and a far-board **promotion zone** with optional and forced
//! promotion.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Shogi::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant shogi`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position. The `compare-fairy/` harness re-runs that head-to-head
//! on demand (`compare-fairy/src/shogi.rs`); this test pins the FSF-confirmed
//! numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `shogi` start (`position startpos`):
//!
//! ```text
//! lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL[-] w - - 0 1
//! ```
//!
//! mcr renders the same position with an empty `[]` holdings bracket (its hand is
//! empty at the start). The Shogi piece letters coincide with FSF's — `l n s g k
//! r b p` and the `+`-prefixed promoted forms `+P +L +N +S +R +B` — so no FEN
//! dialect rewrite is needed (unlike Xiangqi). The well-known Shogi startpos
//! perft sequence is `30, 900, 25470, 719731, 19861490`, all confirmed against
//! this FSF binary.
//!
//! ## Note on *uchifuzume* (no pawn-drop mate)
//!
//! Real Shogi forbids a pawn drop that gives immediate checkmate. **FSF's `shogi`
//! perft does not enforce this** — a mating pawn drop is listed as a legal move —
//! so mcr, validated node-for-node against FSF, does not filter it either (the
//! `nifu_mate` case below contains a mating pawn drop that both engines count).
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_shogi -- --include-ignored`.

use mcr::geometry::{perft as gperft, Shogi, Shogi9x9};

/// The Shogi starting FEN, confirmed against Fairy-Stockfish's `UCI_Variant
/// shogi`. The hand is empty (`[]`).
const STARTPOS: &str = "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL[] w - - 0 1";

/// A position with a Pawn **in each hand** (one pawn captured off each side),
/// white to move: drops now expand the branching factor sharply, and the held
/// pawns exercise the dead-piece (no last-rank) and nifu (one pawn per file) drop
/// rules. FSF-confirmed.
const DROPS_IN_HAND: &str =
    "lnsgkgsnl/1r5b1/p1ppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL[Pp] w - - 0 1";

/// A drop-heavy middlegame, black to move, a Pawn in each hand and a hole in each
/// pawn wall — many drop squares and many board moves at depth. FSF-confirmed.
const MIDGAME: &str = "lnsgkgsnl/1r5b1/pppppp1pp/9/9/9/PPPPPP1PP/1B5R1/LNSGKGSNL[Pp] b - - 0 1";

/// A bare-kings position with **one of every droppable role in each hand**, white
/// to move: drops *dominate* the move set (525 legal moves at depth 1, almost all
/// drops), stressing the dead-piece and nifu filters across every role and the
/// check-blocking drop legality at depth. FSF-confirmed.
const MULTI_HAND: &str = "4k4/9/9/9/9/9/9/9/4K4[RBGSNLPrbgsnlp] w - - 0 1";

/// Two full pawn walls one step from their far ranks (white pawns on rank 8,
/// black pawns on rank 2), white to move: **every** pawn push reaches the last
/// rank and must therefore **promote** (forced promotion), with no non-promoting
/// alternative. FSF-confirmed.
const FORCED_PROMO: &str = "4k4/PPPPPPPPP/9/9/9/9/9/ppppppppp/4K4[] w - - 0 1";

/// A lone white Pawn on e8 (one step from the last rank) with the kings clear,
/// white to move: the pawn push to e9 is forced to promote, while a Pawn one rank
/// back would have the **promote / don't-promote choice** — the zone-entry
/// optional-promotion path. FSF-confirmed.
const PROMO_CHOICE: &str = "9/4P4/9/9/9/9/9/9/4k1K2[] w - - 0 1";

/// A position whose only pawn drop onto the a-file is a **checkmating** pawn drop
/// (*uchifuzume*): the black king is boxed in the corner and `P@a8` is mate.
/// Real Shogi forbids it, but FSF's `shogi` perft lists it, so both engines count
/// it — this case pins that FSF-matching behaviour. FSF-confirmed.
const NIFU_MATE: &str = "k8/9/9/9/9/9/9/9/LR2K4[P] w - - 0 1";

/// Asserts the generic Shogi perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF shogi `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Shogi::from_fen(fen).expect("valid Shogi FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Shogi9x9, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Shogi perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed; the well-known Shogi perft sequence) ------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 30), (2, 900), (3, 25470)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    // The canonical published Shogi startpos perft, confirmed against FSF.
    check(STARTPOS, &[(4, 719731), (5, 19861490)]);
}

// -- Pawn in each hand: drops + dead-piece + nifu (FSF-confirmed) -------------

#[test]
fn drops_in_hand_cheap() {
    check(DROPS_IN_HAND, &[(1, 30), (2, 1168), (3, 33290)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn drops_in_hand_deep() {
    check(DROPS_IN_HAND, &[(4, 1209441)]);
}

// -- Drop-heavy middlegame (FSF-confirmed) -----------------------------------

#[test]
fn midgame_cheap() {
    check(MIDGAME, &[(1, 43), (2, 1515), (3, 61802)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_deep() {
    check(MIDGAME, &[(4, 2061521)]);
}

// -- Drops dominate: one of every role in hand (FSF-confirmed) ----------------

#[test]
fn multi_hand_cheap() {
    check(MULTI_HAND, &[(1, 525), (2, 251422)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn multi_hand_deep() {
    check(MULTI_HAND, &[(3, 104100181)]);
}

// -- Forced promotion: every push reaches the last rank (FSF-confirmed) -------

#[test]
fn forced_promotion() {
    check(FORCED_PROMO, &[(1, 3), (2, 9), (3, 180), (4, 3570)]);
}

// -- Optional zone-entry promotion choice (FSF-confirmed) --------------------

#[test]
fn promotion_choice() {
    check(PROMO_CHOICE, &[(1, 4), (2, 16), (3, 112), (4, 695)]);
}

// -- A mating pawn drop is counted (FSF matches; no uchifuzume) ---------------

#[test]
fn nifu_mate_drop_counted() {
    check(NIFU_MATE, &[(1, 97), (2, 26), (3, 1230), (4, 40633)]);
}
