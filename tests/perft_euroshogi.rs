//! EuroShogi (European Shogi, 8x8 / `u64`) perft validation on the generic
//! engine (issue #406) — Shogi (#190) adapted onto the standard 8x8 board with a
//! reduced army (**no Silver, no Lance**), a **modified Knight**, and
//! **mandatory** in-zone promotion, reusing the same persistent capture-fed
//! **hand**, **drops**, and far-three-rank **promotion zone** machinery.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mce::geometry::EuroShogi::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant euroshogi`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position — the start-position perft-2 divide matches FSF
//! move-for-move. The `compare-fairy/` harness re-runs that head-to-head on
//! demand (`--difffuzz --variant euroshogi`, 0 divergences); this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `euroshogi` start (`position startpos`):
//!
//! ```text
//! 1nbgkgn1/1r4b1/pppppppp/8/8/PPPPPPPP/1B4R1/1NGKGBN1[] w - - 0 1
//! ```
//!
//! mce renders the same position with an empty `[]` holdings bracket; the piece
//! letters coincide with FSF's — `k g n b r p` and the `+`-prefixed promoted
//! forms — so no FEN dialect rewrite is needed. The FSF-confirmed startpos perft
//! sequence is `25, 625, 15424, 380499, 9451149`.
//!
//! ## The modified Knight and mandatory promotion
//!
//! The EuroShogi Knight adds the two straight **sideways** steps to the Shogi
//! Knight's two forward 2-1 jumps, so it is never immobile: it may be dropped on
//! any empty square (even the last rank, unlike a Shogi Knight — see
//! `knight_in_hand`). Promotion is **compulsory** in the far three ranks: a Rook
//! that merely *starts* in the zone promotes on every move (see `rook_in_zone`),
//! and there is never a non-promoting zone alternative.
//!
//! ## Note on *uchifuzume* (no pawn-drop mate)
//!
//! As with Shogi (#190), **FSF's `euroshogi` perft does not enforce uchifuzume**,
//! so mce, validated node-for-node against FSF, does not filter it either. The
//! `drops` case exercises the dead-Pawn (no last-rank) and nifu Pawn-drop rules.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_euroshogi -- --include-ignored`.

use mce::geometry::{perft as gperft, Chess8x8, EuroShogi};

/// The EuroShogi starting FEN, confirmed against Fairy-Stockfish's `UCI_Variant
/// euroshogi`. The hand is empty (`[]`).
const STARTPOS: &str = "1nbgkgn1/1r4b1/pppppppp/8/8/PPPPPPPP/1B4R1/1NGKGBN1[] w - - 0 1";

/// Bare kings with **one of every droppable role in each hand**, white to move:
/// drops dominate the move set (308 legal moves at depth 1), stressing the
/// dead-Pawn (no last-rank) and nifu filters and the unrestricted (last-rank-OK)
/// modified-Knight drop. FSF-confirmed.
const DROPS: &str = "4k3/8/8/8/8/8/8/4K3[RBGNPrbgnp] w - - 0 1";

/// A lone white Knight on d4, kings clear: exercises the modified Knight's two
/// forward 2-1 jumps (each mandatory-promoting into the zone) **and** its two
/// sideways steps. FSF-confirmed.
const LONE_KNIGHT: &str = "4k3/8/8/8/3N4/8/8/4K3[] w - - 0 1";

/// A white Rook standing **in** the promotion zone (c6), kings clear: because
/// EuroShogi promotion is compulsory, every Rook move — even one leaving the zone
/// — is the promoting (Dragon) form, with no non-promoting alternative.
/// FSF-confirmed.
const ROOK_IN_ZONE: &str = "4k3/8/2R5/8/8/8/8/4K3[] w - - 0 1";

/// A single modified Knight in white's hand, kings clear: unlike a Shogi Knight
/// it may be dropped on **any** empty square, including the last two ranks (it
/// always keeps a sideways move), so its drop set is unrestricted. FSF-confirmed.
const KNIGHT_IN_HAND: &str = "4k3/8/8/8/8/8/8/4K3[N] w - - 0 1";

/// An open middlegame off the start position (a few pawns advanced), white to
/// move: many board moves across the whole army. FSF-confirmed.
const MIDGAME: &str = "1nbgkgn1/1r4b1/pp1ppppp/2p5/5P2/PPPPP1PP/1B4R1/1NGKGBN1[] w - - 0 1";

/// Asserts the generic EuroShogi perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `euroshogi` `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = EuroShogi::from_fen(fen).expect("valid EuroShogi FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "EuroShogi perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) ------------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 25), (2, 625), (3, 15424)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(4, 380499), (5, 9451149)]);
}

// -- Drops: dead-Pawn + nifu + unrestricted Knight (FSF-confirmed) ------------

#[test]
fn drops_cheap() {
    check(DROPS, &[(1, 308), (2, 84453)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn drops_deep() {
    check(DROPS, &[(3, 18866429)]);
}

// -- The modified Knight (FSF-confirmed) -------------------------------------

#[test]
fn lone_knight() {
    check(LONE_KNIGHT, &[(1, 9), (2, 41), (3, 426), (4, 2415)]);
}

// -- Mandatory in-zone promotion (FSF-confirmed) -----------------------------

#[test]
fn rook_in_zone() {
    check(ROOK_IN_ZONE, &[(1, 19), (2, 78), (3, 1683), (4, 8164)]);
}

// -- Unrestricted Knight drop (FSF-confirmed) --------------------------------

#[test]
fn knight_in_hand() {
    check(KNIGHT_IN_HAND, &[(1, 67), (2, 317), (3, 4029)]);
}

// -- Open middlegame (FSF-confirmed) -----------------------------------------

#[test]
fn midgame() {
    check(MIDGAME, &[(1, 26), (2, 676), (3, 17519)]);
}
