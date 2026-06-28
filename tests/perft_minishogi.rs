//! Minishogi (5x5 / `u64`) perft validation on the generic engine (issue #195)
//! — Shogi (#190) shrunk onto a five-by-five board, reusing the same
//! persistent capture-fed **hand**, **drops**, and far-rank **promotion zone**
//! machinery with one of every piece per side and **no Knight or Lance**.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mce::geometry::Minishogi::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant minishogi`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position. The `compare-fairy/` harness re-runs that
//! head-to-head on demand (`compare-fairy/src/minishogi.rs`); this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `minishogi` start (`position startpos`):
//!
//! ```text
//! rbsgk/4p/5/P4/KGSBR[-] w - - 0 1
//! ```
//!
//! mce renders the same position with an empty `[]` holdings bracket (its hand is
//! empty at the start). The piece letters coincide with FSF's — `s g k r b p`
//! and the `+`-prefixed promoted forms `+P +S +R +B` — so no FEN dialect rewrite
//! is needed. The FSF-confirmed startpos perft sequence is
//! `14, 181, 2512, 35401, 533203`.
//!
//! ## Promotion zone
//!
//! On 5x5 the zone is the **furthest rank only**: a Silver or Rook *entering* it
//! gets both the promoting and non-promoting move; a Pawn reaching it is
//! **forced** to promote. Confirmed against FSF (the `forced-promo` and
//! `promo-choice` cases below).
//!
//! ## Note on *uchifuzume* (no pawn-drop mate)
//!
//! As with Shogi (#190), **FSF's `minishogi` perft does not enforce uchifuzume**
//! — it lists a pawn drop even when it gives mate — so mce, validated
//! node-for-node against FSF, does not filter it either. The `nifu` case pins the
//! **nifu** filter (no second unpromoted pawn on a file); a Tokin does not count.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_minishogi -- --include-ignored`.

use mce::geometry::{perft as gperft, Minishogi, Minishogi5x5};

/// The Minishogi starting FEN, confirmed against Fairy-Stockfish's `UCI_Variant
/// minishogi`. The hand is empty (`[]`).
const STARTPOS: &str = "rbsgk/4p/5/P4/KGSBR[] w - - 0 1";

/// A Pawn **in each hand** (one captured off each side) with the board pawns
/// gone, white to move: drops expand the branching factor sharply and exercise
/// the dead-piece (no last-rank) and nifu drop rules. FSF-confirmed.
const DROPS_IN_HAND: &str = "rbsgk/5/5/5/KGSBR[Pp] w - - 0 1";

/// Bare kings with **one of every droppable role in each hand** (no Knight or
/// Lance on 5x5), white to move: drops *dominate* the move set (114 legal moves
/// at depth 1, almost all drops), stressing the dead-piece and nifu filters
/// across every role and the check-blocking drop legality at depth.
/// FSF-confirmed.
const MULTI_HAND: &str = "k4/5/5/5/4K[RBGSPrbgsp] w - - 0 1";

/// A lone white Pawn on e4, one step from the last rank, kings clear, white to
/// move: the pawn push to e5 is **forced** to promote (it would otherwise have
/// no further move) — there is no non-promoting alternative. FSF-confirmed.
const FORCED_PROMO: &str = "4k/4P/5/5/4K[] w - - 0 1";

/// A lone white Silver on e4 entering the last rank, white to move: the Silver
/// gets **both** the promoting and non-promoting move into the zone (the
/// optional zone-entry promotion path), unlike the forced Pawn above.
/// FSF-confirmed.
const PROMO_CHOICE: &str = "4k/4S/5/5/4K[] w - - 0 1";

/// A drop-heavy open middlegame: kings on the bottom rank, the rooks active on
/// rank 3, and a Pawn in each hand — many board moves and many drop squares at
/// depth. FSF-confirmed.
const MIDGAME: &str = "2k2/5/R3r/5/2K2[Pp] w - - 0 1";

/// A white Pawn already on the a-file plus a Pawn in hand, white to move: **nifu**
/// forbids dropping the held Pawn anywhere on the a-file, so a-file pawn drops
/// are absent from the move set (confirmed against FSF). FSF-confirmed.
const NIFU: &str = "2k2/5/P4/5/2K2[P] w - - 0 1";

/// Asserts the generic Minishogi perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `minishogi` `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Minishogi::from_fen(fen).expect("valid Minishogi FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Minishogi5x5, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Minishogi perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) ------------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 14), (2, 181), (3, 2512)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(4, 35401), (5, 533203)]);
}

// -- Pawn in each hand: drops + dead-piece + nifu (FSF-confirmed) -------------

#[test]
fn drops_in_hand_cheap() {
    check(DROPS_IN_HAND, &[(1, 6), (2, 36), (3, 743)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn drops_in_hand_deep() {
    check(DROPS_IN_HAND, &[(4, 14012)]);
}

// -- Multi-hand: drops dominate (FSF-confirmed) ------------------------------

#[test]
fn multi_hand_cheap() {
    check(MULTI_HAND, &[(1, 114), (2, 10671)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn multi_hand_deep() {
    check(MULTI_HAND, &[(3, 822612)]);
}

// -- Forced promotion (FSF-confirmed) ----------------------------------------

#[test]
fn forced_promo() {
    check(FORCED_PROMO, &[(1, 4), (2, 9), (3, 51), (4, 479)]);
}

// -- Optional zone-entry promotion (FSF-confirmed) ---------------------------

#[test]
fn promo_choice() {
    check(PROMO_CHOICE, &[(1, 8), (2, 11), (3, 69), (4, 751)]);
}

// -- Drop-heavy middlegame (FSF-confirmed) -----------------------------------

#[test]
fn midgame_cheap() {
    check(MIDGAME, &[(1, 31), (2, 808)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_deep() {
    check(MIDGAME, &[(3, 14728)]);
}

// -- Nifu drop filter (FSF-confirmed) ----------------------------------------

#[test]
fn nifu() {
    check(NIFU, &[(1, 21), (2, 100), (3, 1096)]);
}
