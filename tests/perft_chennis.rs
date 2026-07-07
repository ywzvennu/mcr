//! Chennis (7x7 tennis-themed flipping variant / `u128`) perft validation on the
//! generic engine (issue #273) — a reuse of the Kyoto Shogi (#232) **per-move
//! flip** and the Shogi-family persistent **hand** + **dual-form drops**, on a
//! fresh 7x7 geometry, with a **king mobility region** (each side's King is
//! confined to a 5x4 zone).
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Chennis::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant chennis`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position. The `compare-fairy/` harness re-runs that head-to-head
//! on demand (`compare-fairy/src/chennis.rs`); this test pins the FSF-confirmed
//! numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `chennis` start (`position startpos`):
//!
//! ```text
//! 1fkm3/1p1s3/7/7/7/3S1P1/3MKF1[] w - 0 1
//! ```
//!
//! mcr renders the same placement in its own dialect (the Ferz is `m`, the Soldier
//! `z`, the Commoner `*u`, the Pawn `**p`) with an empty `[]` holdings bracket. The
//! FSF-confirmed startpos perft sequence is `10, 100, 1371, 18633, 289367,
//! 4534068`.
//!
//! ## The flip mechanic
//!
//! Each non-royal piece carries two forms and **alternates between them every
//! move**: a base Pawn / Ferz / Soldier / Commoner flips to its (Rook / Cannon /
//! Bishop / Knight)-moving promoted form, and a promoted piece flips back. There is
//! no promotion *zone* — the flip is unconditional. A captured piece banks in its
//! base form, and a held piece may be **dropped in either form** (`dropPromoted`),
//! with no drop restriction. The King has no alternate form, never flips, and may
//! never leave its mobility region.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_chennis -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chennis, Chennis7x7};

/// The Chennis starting FEN, confirmed against FSF's `UCI_Variant chennis`, in mcr
/// dialect (Ferz `m`, Soldier `z`, Commoner `*u`, Pawn `**p`). The hand is empty.
const STARTPOS: &str = "1mk*u3/1**p1z3/7/7/7/3Z1**P1/3*UKM1[] w - - 0 1";

/// A flipping middlegame reached from the start (FSF
/// `1fkm3/3s3/1+p5/7/3+S3/5P+F/3MK2[] b`): a promoted Pawn (Rook), Soldier
/// (Bishop) and Ferz (Cannon) sit on the board, so every move toggles a form and
/// the cannon / rook / bishop geometry is exercised. FSF-confirmed.
const FLIP_MIDGAME: &str = "1mk*u3/3z3/1r5/7/3B3/5**PC/3*UK2[] b - - 4 2";

/// Bare kings with **one of every base droppable role** (Pawn, Ferz, Soldier,
/// Commoner) in each hand, white to move: dual-form drops dominate the move set,
/// stressing every flipping role's base/promoted deployment with no drop
/// restriction. FSF-confirmed.
const DROPS_IN_HAND: &str = "3k3/7/7/7/7/7/3K3[**PMZ*U**pmz*u] w - - 0 1";

/// Bare kings with a single Pawn in each hand, white to move: the minimal
/// dual-form drop position (each empty square admits a base Pawn and a promoted
/// Rook drop). FSF-confirmed.
const ONE_PAWN_HAND: &str = "3k3/7/7/7/7/7/3K3[**P**p] w - - 0 1";

/// Runs `Chennis::perft` to `depth` from `fen`.
fn perft(fen: &str, depth: u32) -> u64 {
    let pos = Chennis::from_fen(fen).expect("the Chennis FEN parses");
    gperft::<Chennis7x7, _, _>(&pos, depth)
}

#[test]
fn startpos_perft_matches_fsf() {
    // FSF `UCI_Variant chennis`, `position startpos`, `go perft 1..=4`.
    assert_eq!(perft(STARTPOS, 1), 10, "startpos depth 1");
    assert_eq!(perft(STARTPOS, 2), 100, "startpos depth 2");
    assert_eq!(perft(STARTPOS, 3), 1371, "startpos depth 3");
    assert_eq!(perft(STARTPOS, 4), 18633, "startpos depth 4");
}

#[test]
fn flip_midgame_perft_matches_fsf() {
    assert_eq!(perft(FLIP_MIDGAME, 1), 21, "flip-midgame depth 1");
    assert_eq!(perft(FLIP_MIDGAME, 2), 491, "flip-midgame depth 2");
    assert_eq!(perft(FLIP_MIDGAME, 3), 8079, "flip-midgame depth 3");
}

#[test]
fn drops_in_hand_perft_matches_fsf() {
    assert_eq!(perft(DROPS_IN_HAND, 1), 381, "drops-in-hand depth 1");
    assert_eq!(perft(DROPS_IN_HAND, 2), 129875, "drops-in-hand depth 2");
}

#[test]
fn one_pawn_hand_perft_matches_fsf() {
    assert_eq!(perft(ONE_PAWN_HAND, 1), 99, "one-pawn-hand depth 1");
    assert_eq!(perft(ONE_PAWN_HAND, 2), 8383, "one-pawn-hand depth 2");
    assert_eq!(perft(ONE_PAWN_HAND, 3), 112710, "one-pawn-hand depth 3");
}

/// The startpos round-trips through FEN and the placement matches the
/// FSF-confirmed start (an empty hand).
#[test]
fn startpos_round_trips_through_fen() {
    let pos = Chennis::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    let parsed = Chennis::from_fen(STARTPOS).expect("startpos FEN parses");
    assert_eq!(parsed.to_fen(), STARTPOS);
}

// --- deep layers (ignored by default) --------------------------------------

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn startpos_perft_deep_matches_fsf() {
    assert_eq!(perft(STARTPOS, 5), 289367, "startpos depth 5");
    assert_eq!(perft(STARTPOS, 6), 4534068, "startpos depth 6");
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn flip_midgame_perft_deep_matches_fsf() {
    assert_eq!(perft(FLIP_MIDGAME, 4), 171533, "flip-midgame depth 4");
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn drops_in_hand_perft_deep_matches_fsf() {
    assert_eq!(perft(DROPS_IN_HAND, 3), 33461481, "drops-in-hand depth 3");
}
