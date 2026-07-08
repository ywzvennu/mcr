//! Loop Chess perft validation on the generic engine.
//!
//! Loop Chess is **Crazyhouse with the `dropLoop` rule**: every captured piece
//! banks to the captor's hand and may be dropped, and a captured piece that
//! reached the board **by promotion keeps its promoted role** in hand (`Q~` -> `Q`)
//! rather than demoting to a Pawn. It is an 8x8 full-information position, so
//! Fairy-Stockfish's `UCI_Variant loop` `go perft` is directly meaningful. Every
//! `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::LoopChess::perft` and by FSF running `go perft` on the
//! byte-identical position; the `compare-fairy/` harness re-runs that head-to-head
//! on demand (`compare-fairy/src/loopchess.rs`), and this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## The rule that distinguishes Loop from a demoting Crazyhouse
//!
//! The [`PROMO`] corpus position pins the `dropLoop` divergence: a **promoted**
//! White Queen (`Q~`) stands on a8 where a Black rook can take it. Under Loop the
//! captured Queen banks as a **Queen** (droppable everywhere), so perft(3) = 312;
//! under ordinary demoting Crazyhouse the same capture banks a **Pawn**, giving
//! 271. Matching FSF's 312 confirms `drop_loop = true`. With a *natural* (un-`~`)
//! Queen the two rulesets agree (both bank a Queen), so the promoted marker is the
//! only thing that moves the count.
//!
//! ## FEN dialect
//!
//! Loop uses only **standard chess pieces** (`K Q R B N P`), whose letters are
//! identical in mcr and FSF, so the FEN is passed to FSF unchanged. The trailing
//! `[..]` is the crazyhouse hand (empty `[]` at the start) and a promoted piece
//! carries a `~` suffix (`Q~`); FSF accepts both.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `UCI_Variant loop` start position (inherited from crazyhouse) — the
//! hand starts empty:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1
//! ```
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_loop -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, LoopChess, WideRole};
use mcr::Color;

/// The Loop starting FEN, confirmed against FSF — standard array, empty hand.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1";

/// A developed midgame with a Knight and a Pawn in **each** side's hand
/// (`[NPnp]`), so drops are live alongside ordinary play. Pinned against FSF.
const HAND: &str = "r1bqk2r/ppp2ppp/2n5/3pp3/3PP3/2N5/PPP2PPP/R1BQK2R[NPnp] w KQkq - 0 1";

/// A **promoted** White Queen (`Q~`) on a8 that a Black rook on a1 can capture,
/// pinning the `dropLoop` divergence from demoting Crazyhouse: the captured
/// promoted Queen banks as a **Queen** (Loop), not a Pawn (Crazyhouse), so this
/// diverges at depth 3 (Loop 312 vs Crazyhouse 271). Pinned against FSF.
const PROMO: &str = "Q~6k/8/8/8/8/8/8/r6K b - - 0 1";

/// A bare-kings position with a lone Queen in White's hand, isolating the **drop**
/// generator: at depth 1, 5 king moves + 62 Queen drops (every empty square) = 67.
const QDROP: &str = "4k3/8/8/8/8/8/8/4K3[Q] w - - 0 1";

/// A lone Pawn in White's hand on a bare-kings board, isolating the **pawn drop
/// rank restriction**: a Loop pawn may not be dropped on rank 1 or rank 8, so at
/// depth 1 there are 5 king moves + 48 pawn drops (ranks 2-7) = 53. (Chessgi, which
/// allows first-rank pawn drops, gives 60 here.)
const PAWN: &str = "4k3/8/8/8/8/8/8/4K3[P] w - - 0 1";

/// Both sides hold a full `[QRBNPqrbnp]` reserve over a bare castling skeleton,
/// stressing drops of every role at once (and the pawn rank-restriction). Pinned
/// against FSF.
const RICH: &str = "r3k2r/8/8/8/8/8/8/R3K2R[QRBNPqrbnp] w KQkq - 0 1";

/// `(depth, nodes)` rows confirmed identical between mcr and FSF.
struct Perft {
    fen: &'static str,
    rows: &'static [(u32, u64)],
}

const STARTPOS_PERFT: Perft = Perft {
    fen: STARTPOS,
    rows: &[(1, 20), (2, 400), (3, 8902), (4, 197_281), (5, 4_888_832)],
};

const HAND_PERFT: Perft = Perft {
    fen: HAND,
    rows: &[(1, 101), (2, 9_720), (3, 744_450), (4, 56_336_419)],
};

const PROMO_PERFT: Perft = Perft {
    fen: PROMO,
    rows: &[(1, 3), (2, 9), (3, 312), (4, 3_214)],
};

const QDROP_PERFT: Perft = Perft {
    fen: QDROP,
    rows: &[(1, 67), (2, 230), (3, 7_090)],
};

const PAWN_PERFT: Perft = Perft {
    fen: PAWN,
    rows: &[(1, 53), (2, 255)],
};

const RICH_PERFT: Perft = Perft {
    fen: RICH,
    rows: &[(1, 306), (2, 78_889)],
};

fn check(p: &Perft, max_depth: u32) {
    let pos = LoopChess::from_fen(p.fen).expect("Loop FEN parses");
    for &(depth, nodes) in p.rows {
        if depth > max_depth {
            continue;
        }
        assert_eq!(
            gperft::<Chess8x8, _, _>(&pos, depth),
            nodes,
            "Loop perft depth {depth} for FEN {}",
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
fn promoted_capture_keeps_role_matches_fsf() {
    check(&PROMO_PERFT, 3);
}

#[test]
fn qdrop_shallow_matches_fsf() {
    check(&QDROP_PERFT, 3);
}

#[test]
fn pawn_drop_ranks_match_fsf() {
    check(&PAWN_PERFT, 2);
}

#[test]
fn rich_shallow_matches_fsf() {
    check(&RICH_PERFT, 2);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn startpos_deep_matches_fsf() {
    check(&STARTPOS_PERFT, 5);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn hand_deep_matches_fsf() {
    check(&HAND_PERFT, 4);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn promoted_capture_deep_matches_fsf() {
    check(&PROMO_PERFT, 4);
}

/// The `dropLoop` mechanic, checked directly: capturing a **promoted** Queen banks
/// a **Queen** into the captor's hand (not a Pawn), so the captor can then drop a
/// Queen. This is the one rule that separates Loop from ordinary Crazyhouse.
#[test]
fn capturing_a_promoted_queen_banks_a_queen() {
    let pos = LoopChess::from_fen(PROMO).expect("FEN parses");
    // Black rook a1 x a8 captures the promoted White Queen.
    let rxa8 = pos
        .legal_moves()
        .into_iter()
        .find(|m| m.to_uci::<Chess8x8>() == "a1a8")
        .expect("Rxa8 is legal");
    let after = pos.play(&rxa8);
    assert_eq!(
        after.hand_count(Color::Black, WideRole::Queen),
        1,
        "the captured promoted Queen banks as a Queen (dropLoop), not a Pawn",
    );
    assert_eq!(
        after.hand_count(Color::Black, WideRole::Pawn),
        0,
        "no demotion to a Pawn under dropLoop",
    );
}

/// A **natural** (un-promoted) captured Queen also banks as a Queen — the promoted
/// marker is what the demotion rule keys on, and Loop keeps the role either way, so
/// the promoted and natural captures are indistinguishable in Loop.
#[test]
fn capturing_a_natural_queen_also_banks_a_queen() {
    let pos = LoopChess::from_fen("Q6k/8/8/8/8/8/8/r6K b - - 0 1").expect("FEN parses");
    let rxa8 = pos
        .legal_moves()
        .into_iter()
        .find(|m| m.to_uci::<Chess8x8>() == "a1a8")
        .expect("Rxa8 is legal");
    let after = pos.play(&rxa8);
    assert_eq!(after.hand_count(Color::Black, WideRole::Queen), 1);
    assert_eq!(after.hand_count(Color::Black, WideRole::Pawn), 0);
}

/// A dropped Pawn may not land on the first or last rank (the crazyhouse rule,
/// confirmed against FSF): with only a Pawn in hand on a bare-kings board, every
/// pawn drop is on ranks 2-7.
#[test]
fn pawn_drops_avoid_first_and_last_rank() {
    let pos = LoopChess::from_fen(PAWN).expect("FEN parses");
    let pawn_drops: Vec<_> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| m.is_drop() && m.drop_role() == Some(WideRole::Pawn))
        .collect();
    assert_eq!(pawn_drops.len(), 48, "ranks 2-7 only: 6 ranks * 8 files");
    assert!(
        pawn_drops
            .iter()
            .all(|m| (1..=6).contains(&m.to::<Chess8x8>().rank())),
        "no Loop pawn drop on rank 1 (0) or rank 8 (7)",
    );
}

/// A capture banks the taken piece into the captor's hand (unlike Bughouse): after
/// `e4`x`d5` from a developed opening, White holds a droppable Pawn.
#[test]
fn capture_banks_into_the_hand() {
    let pos = LoopChess::from_fen("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR[] w KQkq - 0 1")
        .expect("FEN parses");
    let exd5 = pos
        .legal_moves()
        .into_iter()
        .find(|m| m.to_uci::<Chess8x8>() == "e4d5")
        .expect("e4xd5 is legal");
    let after = pos.play(&exd5);
    assert_eq!(
        after.hand_count(Color::White, WideRole::Pawn),
        1,
        "a Loop capture banks the taken pawn into the captor's hand",
    );
}

/// The empty crazyhouse hand round-trips through FEN, and a promoted piece keeps
/// its `~` marker across a parse/render cycle.
#[test]
fn fen_round_trips_hand_and_promoted_marker() {
    let pos = LoopChess::startpos();
    assert!(
        pos.to_fen().contains("[]"),
        "the empty hand renders as []: {}",
        pos.to_fen()
    );
    let promo = LoopChess::from_fen(PROMO).expect("FEN parses");
    assert!(
        promo.to_fen().contains("Q~"),
        "the promoted Queen keeps its ~ marker: {}",
        promo.to_fen(),
    );
}
