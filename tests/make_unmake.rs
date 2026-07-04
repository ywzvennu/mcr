//! Make/unmake round-trip equality (issue #103).
//!
//! For any legal move, `make` followed by `unmake` must return a position
//! byte-identical to the original — the same board, side to move, castling
//! rights, en-passant target, move clocks, variant state, and Zobrist key. This
//! is checked exhaustively by a perft-style walk that, at every node, makes each
//! legal move, asserts the child equals the independently-computed `play` child,
//! recurses, and then unmakes and asserts the position is byte-identical to what
//! it was before the move.
//!
//! The walk runs for the standard `Position` and for all eight variants, each to
//! a depth that visits hundreds of thousands of nodes covering captures, en
//! passant, promotions, castling, drops (crazyhouse), explosions (atomic), forced
//! captures (antichess), check counters (three-check), and arbitrary-geometry
//! castling (chess960).

use mcr::{
    perft, Antichess, Atomic, Chess, Chess960, Crazyhouse, Horde, KingOfTheHill, Position,
    RacingKings, ThreeCheck, Variant, VariantPosition,
};

/// Walks the standard `Position` tree to `depth`, asserting make/unmake restores
/// every node exactly and that the made child matches `play`. Returns the leaf
/// count so it can be cross-checked against `perft`.
fn walk_core(pos: &mut Position, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    let mut nodes = 0;
    for mv in pos.legal_moves() {
        let before = pos.clone();
        let expected_child = before.play(&mv);

        let undo = pos.make(&mv);
        assert_eq!(
            *pos,
            expected_child,
            "make({mv}) on {} must equal play()",
            before.to_fen()
        );

        nodes += walk_core(pos, depth - 1);

        pos.unmake(&mv, undo);
        // `Position`'s derived equality includes the incremental Zobrist key, so
        // this single assertion also covers byte-identity of the hash.
        assert_eq!(
            *pos,
            before,
            "unmake({mv}) must restore {} byte-for-byte (got {})",
            before.to_fen(),
            pos.to_fen()
        );
    }
    nodes
}

/// The variant analogue of [`walk_core`].
fn walk_variant<V: Variant>(pos: &mut VariantPosition<V>, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    let mut nodes = 0;
    for mv in pos.legal_moves() {
        let before = pos.clone();
        let expected_child = before.play(&mv);

        let undo = pos.make(&mv);
        assert_eq!(
            *pos,
            expected_child,
            "make({mv}) on {} must equal play()",
            before.to_fen()
        );
        assert_eq!(
            pos.zobrist(),
            expected_child.zobrist(),
            "make({mv}) hash must match play() on {}",
            before.to_fen()
        );

        nodes += walk_variant(pos, depth - 1);

        pos.unmake(&mv, undo);
        assert_eq!(
            *pos,
            before,
            "unmake({mv}) must restore {} byte-for-byte (got {})",
            before.to_fen(),
            pos.to_fen()
        );
        assert_eq!(
            pos.zobrist(),
            before.zobrist(),
            "unmake({mv}) must restore the Zobrist key of {}",
            before.to_fen()
        );
    }
    nodes
}

#[test]
fn core_round_trip_and_matches_perft() {
    // A perft-style walk of the standard start position: every node is made and
    // unmade with a byte-identity assertion, and the leaf count is cross-checked
    // against the reference `perft`.
    let mut pos = Position::startpos();
    let nodes = walk_core(&mut pos, 4);
    assert_eq!(nodes, perft(&Position::startpos(), 4));
    // The walk left the position untouched.
    assert_eq!(pos, Position::startpos());
}

#[test]
fn core_round_trip_kiwipete() {
    // The classic "kiwipete" position is dense with captures, castling, and en
    // passant — the move kinds whose reversal is most error-prone.
    const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
    let mut pos = Position::from_fen(KIWIPETE).unwrap();
    let original = pos.clone();
    walk_core(&mut pos, 3);
    assert_eq!(pos, original);
}

#[test]
fn standard_variant_round_trip() {
    let mut pos = Chess::startpos();
    walk_variant(&mut pos, 4);
    assert_eq!(pos, Chess::startpos());
}

#[test]
fn koth_round_trip() {
    let mut pos = KingOfTheHill::startpos();
    walk_variant(&mut pos, 4);
    assert_eq!(pos, KingOfTheHill::startpos());
}

#[test]
fn three_check_round_trip() {
    let mut pos = ThreeCheck::startpos();
    walk_variant(&mut pos, 4);
    assert_eq!(pos, ThreeCheck::startpos());
}

#[test]
fn racing_kings_round_trip() {
    let mut pos = RacingKings::startpos();
    walk_variant(&mut pos, 4);
    assert_eq!(pos, RacingKings::startpos());
}

#[test]
fn horde_round_trip() {
    let mut pos = Horde::startpos();
    walk_variant(&mut pos, 4);
    assert_eq!(pos, Horde::startpos());
}

#[test]
fn antichess_round_trip() {
    let mut pos = Antichess::startpos();
    walk_variant(&mut pos, 4);
    assert_eq!(pos, Antichess::startpos());
}

#[test]
fn atomic_round_trip() {
    // From the start position, plus a tactical position that forces explosions
    // (captures around both kings) so the multi-piece blast reversal is covered.
    let mut pos = Atomic::startpos();
    walk_variant(&mut pos, 4);
    assert_eq!(pos, Atomic::startpos());

    const BLAST: &str = "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2";
    let mut tactical = Atomic::from_fen(BLAST).unwrap();
    let original = tactical.clone();
    walk_variant(&mut tactical, 4);
    assert_eq!(tactical, original);
}

#[test]
fn crazyhouse_round_trip() {
    // From the start position (drops only appear after a capture fills a pocket),
    // and from a position with material already in hand so drops are exercised
    // immediately.
    let mut pos = Crazyhouse::startpos();
    walk_variant(&mut pos, 4);
    assert_eq!(pos, Crazyhouse::startpos());

    const WITH_POCKET: &str = "rnbqkbnr/ppp1pppp/8/8/8/8/PPPPPPPP/RNBQKBNR[QPp] w KQkq - 0 1";
    let mut dropped = Crazyhouse::from_fen(WITH_POCKET).unwrap();
    let original = dropped.clone();
    walk_variant(&mut dropped, 3);
    assert_eq!(dropped, original);
}

#[test]
fn chess960_round_trip() {
    // A Chess960 arrangement whose rooks are off the a-/h-files, exercising the
    // arbitrary-geometry castle and its reversal.
    const FEN: &str = "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w HFhf - 2 9";
    let mut pos = Chess960::from_fen(FEN).unwrap();
    let original = pos.clone();
    walk_variant(&mut pos, 4);
    assert_eq!(pos, original);
}
