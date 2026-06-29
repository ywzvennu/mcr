//! Gorogoro Shogi Plus (5x6 / `u64`) perft validation on the generic engine
//! (issue #268) — Shogi (#190) shrunk onto a five-by-six board, reusing the same
//! persistent capture-fed **hand**, **drops**, far-zone **promotion** machinery,
//! **Lance**, and **Shogi Knight**, with a King, two Gold and two Silver
//! Generals, and a Pawn row per side, plus a **Lance and a Knight in hand**.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mce::geometry::Gorogoro::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant gorogoroplus`, built `largeboards=yes`, with the
//! `gorogoroplus` definition loaded from `variants.ini`) running `go perft` on
//! the byte-identical position. The `compare-fairy/` harness re-runs that
//! head-to-head on demand (`compare-fairy/src/gorogoro.rs`); this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `gorogoroplus` start:
//!
//! ```text
//! sgkgs/5/1ppp1/1PPP1/5/SGKGS[LNln] w 0 1
//! ```
//!
//! mce renders the same position with the `[LNln]` holdings bracket — a Lance
//! and a Knight in each side's hand — and the explicit `- -` castling/ep fields
//! its FEN dialect uses. The FSF-confirmed startpos perft sequence is
//! `39, 1438, 44436, 1330443, 36102221`.
//!
//! ## Promotion zone
//!
//! On 5x6 the zone is the **furthest two ranks**: a Lance or Silver *entering*
//! it gets both the promoting and non-promoting move; a Pawn or Lance reaching
//! the last rank, or a Knight reaching the last two ranks, is **forced** to
//! promote. Confirmed against FSF (the `forced_promo`, `promo_choice`, `lance`,
//! and `knight` cases below).
//!
//! ## Note on *uchifuzume* (no pawn-drop mate)
//!
//! As with Shogi (#190), **FSF's `gorogoroplus` perft does not enforce
//! uchifuzume** — it lists a pawn drop even when it gives mate — so mce,
//! validated node-for-node against FSF, does not filter it either. The `nifu`
//! case pins the **nifu** filter (no second unpromoted pawn on a file); a Tokin
//! does not count.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_gorogoro -- --include-ignored`.

use mce::geometry::{perft as gperft, Gorogoro, Gorogoro5x6};

/// The Gorogoro Shogi Plus starting FEN, confirmed against Fairy-Stockfish's
/// `UCI_Variant gorogoroplus`. Each side holds a Lance and a Knight (`[LNln]`).
const STARTPOS: &str = "sgkgs/5/1ppp1/1PPP1/5/SGKGS[LNln] w - - 0 1";

/// Bare kings with a **Lance, Knight, and Pawn in each hand**, white to move:
/// drops dominate the move set, stressing the dead-piece (no last-rank Pawn /
/// Lance, no last-two-rank Knight) drop filters across every droppable role.
/// FSF-confirmed.
const DROPS_IN_HAND: &str = "2k2/5/5/5/5/2K2[LNPlnp] w - - 0 1";

/// A lone white Pawn on c5, one step from the last rank, kings clear, white to
/// move: the push to c6 is **forced** to promote (it would otherwise have no
/// further move) — there is no non-promoting alternative. FSF-confirmed.
const FORCED_PROMO: &str = "2k2/2P2/5/5/5/2K2[] w - - 0 1";

/// A lone white Silver on c4 stepping into the two-rank promotion zone, white to
/// move: the Silver gets **both** the promoting and non-promoting move into the
/// zone (the optional zone-entry promotion path), unlike the forced Pawn above.
/// FSF-confirmed.
const PROMO_CHOICE: &str = "2k2/5/2S2/5/5/2K2[] w - - 0 1";

/// A lone white Lance on c2 sliding up its file, white to move: it slides into
/// the zone (optional promotion on rank 5) and is forced to promote on the last
/// rank (c6) — exercising the forward slider's zone-entry and forced-promotion
/// paths. FSF-confirmed.
const LANCE: &str = "2k2/5/5/5/2L2/2K2[] w - - 0 1";

/// A white Knight on c3 plus a Knight in hand, white to move: the board Knight
/// jumps two ranks forward (into the zone, optional promotion) and the held
/// Knight may drop anywhere but the last two ranks (dead-piece rule).
/// FSF-confirmed.
const KNIGHT: &str = "2k2/5/5/2N2/5/2K2[N] w - - 0 1";

/// A white Pawn already on the a-file plus a Pawn in hand, white to move:
/// **nifu** forbids dropping the held Pawn anywhere on the a-file, so a-file pawn
/// drops are absent from the move set (confirmed against FSF). FSF-confirmed.
const NIFU: &str = "2k2/5/5/P4/5/2K2[P] w - - 0 1";

/// Asserts the generic Gorogoro perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `gorogoroplus` `go perft` on the same
/// position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Gorogoro::from_fen(fen).expect("valid Gorogoro FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Gorogoro5x6, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Gorogoro perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) ------------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 39), (2, 1438), (3, 44436)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(4, 1330443), (5, 36102221)]);
}

// -- Multi-hand drops: dead-piece filters (FSF-confirmed) --------------------

#[test]
fn drops_in_hand_cheap() {
    check(DROPS_IN_HAND, &[(1, 72), (2, 4561)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn drops_in_hand_deep() {
    check(DROPS_IN_HAND, &[(3, 206726), (4, 8990020)]);
}

// -- Forced promotion (FSF-confirmed) ----------------------------------------

#[test]
fn forced_promo() {
    check(FORCED_PROMO, &[(1, 6), (2, 25), (3, 190), (4, 1864)]);
}

// -- Optional zone-entry promotion (FSF-confirmed) ---------------------------

#[test]
fn promo_choice() {
    check(PROMO_CHOICE, &[(1, 13), (2, 38), (3, 442), (4, 3046)]);
}

// -- Forward-sliding Lance: zone entry + forced last-rank promo (FSF) ---------

#[test]
fn lance() {
    check(LANCE, &[(1, 9), (2, 30), (3, 270), (4, 1472)]);
}

// -- Forward-jumping Knight: zone entry + drop dead-piece filter (FSF) --------

#[test]
fn knight() {
    check(KNIGHT, &[(1, 25), (2, 69), (3, 950)]);
}

// -- Nifu drop filter (FSF-confirmed) ----------------------------------------

#[test]
fn nifu() {
    check(NIFU, &[(1, 25), (2, 120), (3, 1396)]);
}
