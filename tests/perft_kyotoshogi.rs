//! Kyoto Shogi (5x5 flipping Shogi / `u64`) perft validation on the generic
//! engine (issue #232) — Minishogi (#195) / Shogi (#190) shrunk onto a 5x5 board,
//! reusing the same persistent capture-fed **hand** and **drops**, with one
//! distinctive new mechanic: **every piece flips to its alternate form after each
//! move it makes**.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Kyotoshogi::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant kyotoshogi`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position. The `compare-fairy/` harness re-runs that
//! head-to-head on demand (`compare-fairy/src/kyotoshogi.rs`); this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `kyotoshogi` start (`position startpos`):
//!
//! ```text
//! p+nks+l/5/5/5/+LSK+NP[-] w 0 1
//! ```
//!
//! mcr renders the same position with an empty `[]` holdings bracket (its hand is
//! empty at the start) and the shared `+`-prefixed promoted-token board FEN. The
//! FSF-confirmed startpos perft sequence is `12, 137, 1636, 18268, 225903,
//! 2829234`.
//!
//! ## The flip mechanic
//!
//! Each non-royal piece carries two forms and **alternates between them every
//! move**: a base Pawn/Silver/Lance/Knight flips to its (Rook / Bishop / Gold /
//! Gold)-moving promoted form, and a promoted piece flips back to its base. There
//! is no promotion *zone* — the flip is unconditional. A captured piece banks
//! **unpromoted**, and a held piece may be **dropped in either form**
//! (`dropPromoted`). Kyoto imposes **no** drop restriction (`immobilityIllegal`
//! off, `dropNoDoubled` none): a Pawn may be dropped on the last rank and a file
//! may hold any number of Pawns. The King has no alternate form and never flips.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_kyotoshogi -- --include-ignored`.

use mcr::geometry::{perft as gperft, Kyotoshogi, Minishogi5x5};

/// The Kyoto Shogi starting FEN, confirmed against Fairy-Stockfish's `UCI_Variant
/// kyotoshogi`. The hand is empty (`[]`); the `+`-prefixed back-rank tokens are
/// the Gold-moving promoted Lance / Knight.
const STARTPOS: &str = "p+nks+l/5/5/5/+LSK+NP[] w - - 0 1";

/// Each side holds a **Silver and a Pawn** in hand on a nearly bare board, white
/// to move: dual-form drops (base + promoted) expand the branching factor sharply
/// and exercise the `dropPromoted` rule with no drop restriction. FSF-confirmed.
const DROPS_IN_HAND: &str = "2k2/5/5/5/2K2[SPsp] w - - 0 1";

/// One of **every base droppable role** (Pawn, Silver, Lance, Knight) in each
/// hand on a bare board, white to move: dual-form drops *dominate* the move set,
/// stressing every flipping role's base/promoted deployment. FSF-confirmed.
const MULTI_HAND: &str = "2k2/5/5/5/2K2[PSLNpsln] w - - 0 1";

/// A middlegame with a promoted Silver (`+S`, a Bishop) already on the board, both
/// back ranks otherwise as at the start: exercises the per-move flip of board
/// pieces (every move toggles a form) alongside the promoted-form movement.
/// FSF-confirmed.
const FLIP_MIDGAME: &str = "p+nks+l/5/2+S2/5/+LSK+NP[] w - - 0 1";

/// Both promoted **sliders** on the board — a white `+P` (a Rook) and a black
/// `+s` (a Bishop) — white to move: a slider move flips it back to a (stepping)
/// base, so the next ply sees a different piece, exercising the flip of slider
/// pieces and the resulting check / pin geometry. FSF-confirmed.
const PROMOTED_SLIDERS: &str = "1k3/5/1+s3/5/1K2+P[] w - - 0 1";

/// Runs `Kyotoshogi::perft` to `depth` from `fen`.
fn perft(fen: &str, depth: u32) -> u64 {
    let pos = Kyotoshogi::from_fen(fen).expect("the Kyoto Shogi FEN parses");
    gperft::<Minishogi5x5, _, _>(&pos, depth)
}

#[test]
fn startpos_perft_matches_fsf() {
    // FSF `UCI_Variant kyotoshogi`, `position startpos`, `go perft 1..=4`.
    assert_eq!(perft(STARTPOS, 1), 12, "startpos depth 1");
    assert_eq!(perft(STARTPOS, 2), 137, "startpos depth 2");
    assert_eq!(perft(STARTPOS, 3), 1636, "startpos depth 3");
    assert_eq!(perft(STARTPOS, 4), 18268, "startpos depth 4");
}

#[test]
fn drops_in_hand_perft_matches_fsf() {
    assert_eq!(perft(DROPS_IN_HAND, 1), 97, "drops-in-hand depth 1");
    assert_eq!(perft(DROPS_IN_HAND, 2), 7665, "drops-in-hand depth 2");
    assert_eq!(perft(DROPS_IN_HAND, 3), 346353, "drops-in-hand depth 3");
}

#[test]
fn multi_hand_perft_matches_fsf() {
    assert_eq!(perft(MULTI_HAND, 1), 189, "multi-hand depth 1");
    assert_eq!(perft(MULTI_HAND, 2), 28889, "multi-hand depth 2");
}

#[test]
fn flip_midgame_perft_matches_fsf() {
    assert_eq!(perft(FLIP_MIDGAME, 1), 18, "flip-midgame depth 1");
    assert_eq!(perft(FLIP_MIDGAME, 2), 171, "flip-midgame depth 2");
    assert_eq!(perft(FLIP_MIDGAME, 3), 3228, "flip-midgame depth 3");
    assert_eq!(perft(FLIP_MIDGAME, 4), 33823, "flip-midgame depth 4");
}

#[test]
fn promoted_sliders_perft_matches_fsf() {
    assert_eq!(perft(PROMOTED_SLIDERS, 1), 9, "promoted-sliders depth 1");
    assert_eq!(perft(PROMOTED_SLIDERS, 2), 99, "promoted-sliders depth 2");
    assert_eq!(perft(PROMOTED_SLIDERS, 3), 617, "promoted-sliders depth 3");
    assert_eq!(perft(PROMOTED_SLIDERS, 4), 5391, "promoted-sliders depth 4");
}

/// The startpos round-trips through FEN and the placement matches the
/// FSF-confirmed start (an empty hand, the `+`-prefixed back ranks).
#[test]
fn startpos_round_trips_through_fen() {
    let pos = Kyotoshogi::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    let parsed = Kyotoshogi::from_fen(STARTPOS).expect("startpos FEN parses");
    assert_eq!(parsed.to_fen(), STARTPOS);
}

// --- deep layers (ignored by default) --------------------------------------

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn startpos_perft_deep_matches_fsf() {
    assert_eq!(perft(STARTPOS, 5), 225903, "startpos depth 5");
    assert_eq!(perft(STARTPOS, 6), 2829234, "startpos depth 6");
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn drops_in_hand_perft_deep_matches_fsf() {
    assert_eq!(perft(DROPS_IN_HAND, 4), 15136979, "drops-in-hand depth 4");
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn multi_hand_perft_deep_matches_fsf() {
    assert_eq!(perft(MULTI_HAND, 3), 3315266, "multi-hand depth 3");
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn promoted_sliders_perft_deep_matches_fsf() {
    assert_eq!(
        perft(PROMOTED_SLIDERS, 5),
        37149,
        "promoted-sliders depth 5"
    );
}
