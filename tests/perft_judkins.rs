//! Judkins Shogi (6x6 / `u64`) perft validation on the generic engine (issue
//! #405) — Shogi shrunk onto a six-by-six board, reusing the same persistent
//! capture-fed **hand**, **drops**, and far-two-ranks **promotion zone**
//! machinery with one of every piece per side including the **Knight** but with
//! **no Lance**.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Judkins::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant judkins`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position. This test pins the FSF-confirmed numbers so a
//! regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `judkins` start (`position startpos`):
//!
//! ```text
//! rbnsgk/5p/6/6/P5/KGSNBR[-] w - - 0 1
//! ```
//!
//! mcr renders the same position with an empty `[]` holdings bracket (its hand is
//! empty at the start). The piece letters coincide with FSF's — `n s g k r b p`
//! and the `+`-prefixed promoted forms `+P +N +S +R +B` — so no FEN dialect
//! rewrite is needed. The FSF-confirmed startpos perft sequence is
//! `20, 336, 6183, 118345`.
//!
//! ## Promotion zone
//!
//! On 6x6 the zone is the **furthest two ranks**: a piece starting or ending in it
//! *may* promote, and a Pawn reaching the last rank or a Knight reaching the last
//! two ranks is **forced** to promote (it would otherwise have no move). Confirmed
//! against FSF (the `knight_forced` and `pawn_forced` cases below).
//!
//! ## Note on *uchifuzume* (no pawn-drop mate)
//!
//! As with Shogi (#190), **FSF's `judkins` perft does not enforce uchifuzume** —
//! it lists a pawn drop even when it gives mate — so mcr, validated node-for-node
//! against FSF, does not filter it either. The `nifu` case pins the **nifu** filter
//! (no second unpromoted pawn on a file); a Tokin does not count.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_judkins -- --include-ignored`.

use mcr::geometry::{perft as gperft, Judkins, Judkins6x6};

/// The Judkins Shogi starting FEN, confirmed against Fairy-Stockfish's
/// `UCI_Variant judkins`. The hand is empty (`[]`).
const STARTPOS: &str = "rbnsgk/5p/6/6/P5/KGSNBR[] w - - 0 1";

/// Bare kings with **one of every droppable role in each hand** (no Lance on 6x6),
/// white to move: drops dominate the move set, stressing the dead-piece (Pawn last
/// rank, Knight last two ranks) and nifu drop filters across every role and the
/// check-blocking drop legality at depth. FSF-confirmed.
const MULTI_HAND: &str = "5k/6/6/6/6/K5[RBGSNPrbgsnp] w - - 0 1";

/// A lone white Knight on c3, kings clear, white to move: both of its forward 2-1
/// jumps land on the last two ranks and are therefore **forced** to promote (it
/// would otherwise have no further move) — there is no non-promoting alternative.
/// FSF-confirmed.
const KNIGHT_FORCED: &str = "5k/6/6/2N3/6/5K[] w - - 0 1";

/// A lone white Pawn on a2, one step from the last rank, kings clear, white to
/// move: the pawn push to a6 is **forced** to promote. FSF-confirmed.
const PAWN_FORCED: &str = "5k/6/6/6/P5/5K[] w - - 0 1";

/// A white Pawn already on the a-file plus a Pawn in hand, white to move: **nifu**
/// forbids dropping the held Pawn anywhere on the a-file, so a-file pawn drops are
/// absent from the move set. FSF-confirmed.
const NIFU: &str = "2k3/6/6/P5/6/2K3[P] w - - 0 1";

/// A drop-heavy open middlegame: kings on the bottom rank, the rooks active on
/// rank 4, and a Pawn in each hand — many board moves and many drop squares at
/// depth. FSF-confirmed.
const MIDGAME: &str = "2k3/6/R4r/6/6/2K3[Pp] w - - 0 1";

/// Asserts the generic Judkins perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `judkins` `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Judkins::from_fen(fen).expect("valid Judkins FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Judkins6x6, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Judkins perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) ------------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 20), (2, 336), (3, 6183)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(4, 118345)]);
}

// -- Multi-hand: drops dominate (FSF-confirmed) ------------------------------

#[test]
fn multi_hand_cheap() {
    check(MULTI_HAND, &[(1, 191), (2, 31702)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn multi_hand_deep() {
    check(MULTI_HAND, &[(3, 4452947)]);
}

// -- Forced Knight promotion on the last two ranks (FSF-confirmed) ------------

#[test]
fn knight_forced() {
    check(KNIGHT_FORCED, &[(1, 5), (2, 13), (3, 108)]);
}

// -- Forced Pawn promotion on the last rank (FSF-confirmed) -------------------

#[test]
fn pawn_forced() {
    check(PAWN_FORCED, &[(1, 4), (2, 12), (3, 75)]);
}

// -- Nifu drop filter (FSF-confirmed) ----------------------------------------

#[test]
fn nifu() {
    check(NIFU, &[(1, 30), (2, 145), (3, 1726)]);
}

// -- Drop-heavy middlegame (FSF-confirmed) -----------------------------------

#[test]
fn midgame_cheap() {
    check(MIDGAME, &[(1, 44), (2, 1703)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_deep() {
    check(MIDGAME, &[(3, 41464)]);
}
