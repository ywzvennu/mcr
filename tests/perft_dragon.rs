//! Dragon chess (8x8) perft validation on the generic engine (issue #270) —
//! standard chess plus a single **Dragon** (Bishop + Knight compound) held in each
//! side's fixed pocket and droppable onto the player's own back rank.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Dragon::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant dragon`) running `go perft` on the byte-identical position. The
//! `compare-fairy/` harness re-runs that head-to-head on demand
//! (`compare-fairy/src/dragon.rs`); this test pins the FSF-confirmed numbers so a
//! regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `UCI_Variant dragon` start position:
//!
//! ```text
//! FSF dialect: rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[Dd] w KQkq - 0 1
//! mcr dialect: rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[Aa] w KQkq - 0 1
//! ```
//!
//! The `[Aa]` is the fixed Dragon pocket (one Dragon per side). mcr spells the
//! Dragon `a`/`A` (its census/Capablanca/Seirawan letter) where FSF uses `D`/`d`,
//! so the shown FEN is the **mcr dialect** and the `compare-fairy/` harness
//! rewrites the one Dragon letter when driving FSF.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_dragon -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Dragon};

/// The Dragon starting FEN, confirmed against FSF `UCI_Variant dragon`. At the
/// start the back rank is full, so the Dragon cannot yet be dropped — the early
/// node counts equal standard chess until a back-rank square opens up.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[Aa] w KQkq - 0 1";

/// A developed back rank with both Dragons still in hand: White's b1/c1/f1/g1 are
/// empty, so the Dragon's back-rank drops are live (`A@b1`, …). Exercises the drop
/// generator alongside castling. Pinned against FSF.
const DRAGON_DROPS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/R2QK2R[Aa] w KQkq - 0 1";

/// A Dragon (the Hawk `B+N` compound) already on the board at d4, both pockets
/// empty (`[]`): exercises the Dragon's on-board Bishop + Knight movement,
/// captures, and checks. Pinned against FSF.
const DRAGON_ON_BOARD: &str = "rnbqkbnr/pppppppp/8/8/3A4/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1";

/// A White pawn one step from promotion (b7) with both Dragons in hand: exercises
/// promotion to a Dragon (`b7b8a`, `b7a8a`) — FSF lists the Archbishop among the
/// promotion targets — alongside the back-rank drops. Pinned against FSF.
const DRAGON_PROMOTION: &str = "r3k3/1P6/8/8/8/8/6p1/4K3[Aa] w - - 0 1";

/// `(depth, nodes)` rows confirmed identical between mcr and FSF.
struct Perft {
    fen: &'static str,
    rows: &'static [(u32, u64)],
}

const STARTPOS_PERFT: Perft = Perft {
    fen: STARTPOS,
    rows: &[(1, 20), (2, 400), (3, 8_982), (4, 200_857), (5, 5_038_485)],
};

const DRAGON_DROPS_PERFT: Perft = Perft {
    fen: DRAGON_DROPS,
    rows: &[(1, 28), (2, 560), (3, 15_386), (4, 345_057)],
};

const DRAGON_ON_BOARD_PERFT: Perft = Perft {
    fen: DRAGON_ON_BOARD,
    rows: &[(1, 33), (2, 615), (3, 20_462), (4, 421_236)],
};

const DRAGON_PROMOTION_PERFT: Perft = Perft {
    fen: DRAGON_PROMOTION,
    rows: &[(1, 21), (2, 430), (3, 7_854), (4, 180_760)],
};

fn check(p: &Perft, max_depth: u32) {
    let pos = Dragon::from_fen(p.fen).expect("Dragon FEN parses");
    for &(depth, nodes) in p.rows {
        if depth > max_depth {
            continue;
        }
        assert_eq!(
            gperft::<Chess8x8, _, _>(&pos, depth),
            nodes,
            "Dragon perft depth {depth} for FEN {}",
            p.fen,
        );
    }
}

#[test]
fn startpos_shallow_matches_fsf() {
    check(&STARTPOS_PERFT, 4);
}

#[test]
fn dragon_drops_shallow_matches_fsf() {
    check(&DRAGON_DROPS_PERFT, 3);
}

#[test]
fn dragon_on_board_shallow_matches_fsf() {
    check(&DRAGON_ON_BOARD_PERFT, 3);
}

#[test]
fn dragon_promotion_shallow_matches_fsf() {
    check(&DRAGON_PROMOTION_PERFT, 3);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn startpos_deep_matches_fsf() {
    check(&STARTPOS_PERFT, 5);
    // FSF startpos depth 6 = 125_432_340.
    let pos = Dragon::from_fen(STARTPOS).expect("FEN parses");
    assert_eq!(gperft::<Chess8x8, _, _>(&pos, 6), 125_432_340);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn dragon_drops_deep_matches_fsf() {
    check(&DRAGON_DROPS_PERFT, 4);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn dragon_on_board_deep_matches_fsf() {
    check(&DRAGON_ON_BOARD_PERFT, 4);
}

#[test]
#[ignore = "deep perft; run with --include-ignored"]
fn dragon_promotion_deep_matches_fsf() {
    check(&DRAGON_PROMOTION_PERFT, 4);
}

/// The starting pocket makes the Dragon droppable, but only onto an empty back-rank
/// square — and at the start the back rank is full, so the start position has no
/// drops and exactly the 20 standard chess moves.
#[test]
fn startpos_has_no_drops_yet() {
    let pos = Dragon::startpos();
    let moves = pos.legal_moves();
    assert_eq!(moves.len(), 20, "Dragon startpos perft 1 == standard chess");
    assert_eq!(
        moves.iter().filter(|m| m.is_drop()).count(),
        0,
        "the full back rank leaves no drop square"
    );
}

/// With back-rank squares open, the Dragon drops onto each empty own-back-rank
/// square (and nowhere else).
#[test]
fn dragon_drops_only_onto_empty_back_rank() {
    // White: a1/h1 rooks, d1 queen, e1 king; b1/c1/f1/g1 empty -> four drops.
    let pos = Dragon::from_fen(DRAGON_DROPS).expect("FEN parses");
    let drops = pos.legal_moves().iter().filter(|m| m.is_drop()).count();
    assert_eq!(drops, 4, "one Dragon drop per empty back-rank square");
}

/// The canonical start FEN round-trips through the mcr dialect (the Dragon pocket
/// renders as `[Aa]`).
#[test]
fn startpos_fen_round_trips() {
    let pos = Dragon::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    let reparsed = Dragon::from_fen(&pos.to_fen()).expect("re-rendered FEN parses");
    assert_eq!(reparsed.to_fen(), STARTPOS, "FEN round-trips");
}
