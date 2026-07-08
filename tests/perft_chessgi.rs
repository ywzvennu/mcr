//! Chessgi perft validation on the generic engine.
//!
//! Chessgi is **Loop Chess plus `firstRankPawnDrops`**: the crazyhouse hand and the
//! `dropLoop` promoted-piece rule are inherited from Loop, and a Pawn may now also
//! be dropped onto its own **first rank** (only the pawn's promotion rank — the
//! enemy back rank — stays forbidden). It is an 8x8 full-information position, so
//! Fairy-Stockfish's `UCI_Variant chessgi` `go perft` is directly meaningful. Every
//! `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Chessgi::perft` and by FSF running `go perft` on the
//! byte-identical position; the `compare-fairy/` harness re-runs that head-to-head
//! on demand (`compare-fairy/src/chessgi.rs`), and this test pins the FSF-confirmed
//! numbers so a regression is caught even without FSF present.
//!
//! ## The rule that distinguishes Chessgi from Loop
//!
//! The [`PAWN`] corpus position pins the `firstRankPawnDrops` divergence: with a
//! lone Pawn in White's hand on a bare-kings board, Chessgi allows the pawn to drop
//! on rank 1 (7 extra squares, e1 taken by the king), so perft(1) = 60, whereas
//! Loop forbids the first rank and gives 53. Matching FSF's 60 confirms
//! `firstRankPawnDrops = true`. A pawn still may not be dropped on its **promotion
//! rank** (rank 8 for White), so the region is colour-dependent.
//!
//! ## FEN dialect
//!
//! Chessgi uses only **standard chess pieces** (`K Q R B N P`), identical in mcr and
//! FSF, so the FEN is passed to FSF unchanged. The trailing `[..]` is the crazyhouse
//! hand and a promoted piece carries a `~` suffix (`Q~`).
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `UCI_Variant chessgi` start position (inherited from crazyhouse):
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1
//! ```
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_chessgi -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Chessgi, WideRole};
use mcr::Color;

/// The Chessgi starting FEN, confirmed against FSF — standard array, empty hand.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1";

/// A developed midgame with a Knight and a Pawn in **each** side's hand
/// (`[NPnp]`), so drops — including first-rank pawn drops — are live alongside
/// ordinary play. Pinned against FSF (and larger than Loop's counts here, because
/// the pawns gain first-rank drop targets).
const HAND: &str = "r1bqk2r/ppp2ppp/2n5/3pp3/3PP3/2N5/PPP2PPP/R1BQK2R[NPnp] w KQkq - 0 1";

/// A lone Pawn in White's hand on a bare-kings board, isolating the
/// **`firstRankPawnDrops`** relaxation: a Chessgi White pawn may drop on ranks 1-7
/// (not rank 8), so at depth 1 there are 5 king moves + 55 pawn drops = 60. (Loop,
/// which forbids the first rank too, gives 53 here.)
const PAWN: &str = "4k3/8/8/8/8/8/8/4K3[P] w - - 0 1";

/// A **promoted** White Queen (`Q~`) a Black rook can capture, confirming the
/// `dropLoop` rule is inherited from Loop unchanged: the captured Queen banks as a
/// Queen, so perft(3) = 312 (identical to Loop; a demoting Crazyhouse gives 271).
const PROMO: &str = "Q~6k/8/8/8/8/8/8/r6K b - - 0 1";

/// `(depth, nodes)` rows confirmed identical between mcr and FSF.
struct Perft {
    fen: &'static str,
    rows: &'static [(u32, u64)],
}

const STARTPOS_PERFT: Perft = Perft {
    fen: STARTPOS,
    rows: &[(1, 20), (2, 400), (3, 8902), (4, 197_281), (5, 4_889_167)],
};

const HAND_PERFT: Perft = Perft {
    fen: HAND,
    rows: &[(1, 104), (2, 10_320), (3, 808_769)],
};

const PAWN_PERFT: Perft = Perft {
    fen: PAWN,
    rows: &[(1, 60), (2, 290)],
};

const PROMO_PERFT: Perft = Perft {
    fen: PROMO,
    rows: &[(1, 3), (2, 9), (3, 312)],
};

fn check(p: &Perft, max_depth: u32) {
    let pos = Chessgi::from_fen(p.fen).expect("Chessgi FEN parses");
    for &(depth, nodes) in p.rows {
        if depth > max_depth {
            continue;
        }
        assert_eq!(
            gperft::<Chess8x8, _, _>(&pos, depth),
            nodes,
            "Chessgi perft depth {depth} for FEN {}",
            p.fen,
        );
    }
}

#[test]
fn startpos_shallow_matches_fsf() {
    check(&STARTPOS_PERFT, 4);
}

#[test]
fn hand_shallow_matches_fsf() {
    check(&HAND_PERFT, 3);
}

#[test]
fn first_rank_pawn_drops_match_fsf() {
    check(&PAWN_PERFT, 2);
}

#[test]
fn promoted_capture_keeps_role_matches_fsf() {
    check(&PROMO_PERFT, 3);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn startpos_deep_matches_fsf() {
    check(&STARTPOS_PERFT, 5);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn hand_deep_matches_fsf() {
    check(&HAND_PERFT, 3);
}

/// The `firstRankPawnDrops` mechanic, checked directly: a White pawn in hand may be
/// dropped on rank 1 (its own back rank) as well as ranks 2-7, but **not** on rank
/// 8 (its promotion rank). This is the one rule that separates Chessgi from Loop.
#[test]
fn white_pawn_may_drop_on_the_first_rank_but_not_the_eighth() {
    let pos = Chessgi::from_fen(PAWN).expect("FEN parses");
    let pawn_drops: Vec<_> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| m.is_drop() && m.drop_role() == Some(WideRole::Pawn))
        .collect();
    // Ranks 1-7 (7 ranks * 8 files = 56) minus the king's square e1 = 55.
    assert_eq!(
        pawn_drops.len(),
        55,
        "ranks 1-7 (not 8), minus the king square"
    );
    assert!(
        pawn_drops.iter().any(|m| m.to::<Chess8x8>().rank() == 0),
        "a Chessgi White pawn may drop on rank 1",
    );
    assert!(
        pawn_drops.iter().all(|m| m.to::<Chess8x8>().rank() != 7),
        "no Chessgi White pawn drop on its promotion rank (rank 8)",
    );
}

/// The forbidden rank is **colour-dependent**: a Black pawn in hand may drop on
/// rank 8 (its own back rank) but not on rank 1 (its promotion rank) — the mirror
/// of the White case.
#[test]
fn black_pawn_may_drop_on_the_eighth_rank_but_not_the_first() {
    let pos = Chessgi::from_fen("4k3/8/8/8/8/8/8/4K3[p] b - - 0 1").expect("FEN parses");
    let pawn_drops: Vec<_> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| m.is_drop() && m.drop_role() == Some(WideRole::Pawn))
        .collect();
    // Ranks 2-8 (7 ranks * 8 files = 56) minus the king's square e8 = 55.
    assert_eq!(
        pawn_drops.len(),
        55,
        "ranks 2-8 (not 1), minus the king square"
    );
    assert!(
        pawn_drops.iter().any(|m| m.to::<Chess8x8>().rank() == 7),
        "a Chessgi Black pawn may drop on rank 8",
    );
    assert!(
        pawn_drops.iter().all(|m| m.to::<Chess8x8>().rank() != 0),
        "no Chessgi Black pawn drop on its promotion rank (rank 1)",
    );
}

/// The `dropLoop` rule is inherited from Loop unchanged: capturing a promoted Queen
/// banks a Queen, not a Pawn.
#[test]
fn droploop_is_inherited_from_loop() {
    let pos = Chessgi::from_fen(PROMO).expect("FEN parses");
    let rxa8 = pos
        .legal_moves()
        .into_iter()
        .find(|m| m.to_uci::<Chess8x8>() == "a1a8")
        .expect("Rxa8 is legal");
    let after = pos.play(&rxa8);
    assert_eq!(after.hand_count(Color::Black, WideRole::Queen), 1);
    assert_eq!(after.hand_count(Color::Black, WideRole::Pawn), 0);
}
