//! Shared standard-chess conformance: the concrete engine and the generic engine
//! must never diverge (issue #567).
//!
//! `mcr` keeps **two** standard-chess implementations on purpose — the concrete
//! `Position` (`Variant = Chess`) and the generic
//! `GenericPosition<Chess8x8, StandardChess>`. The concrete one is 30–50× faster
//! (benchmark #563), so neither can be deleted. The cost of two engines is the
//! risk they silently drift apart. This test closes that gap: over a corpus of
//! positions, plus every node of a shallow exhaustive walk, plus seeded random
//! self-play, the two engines must agree on
//!
//!   1. the **legal-move set**, compared as sets of UCI long-algebraic strings
//!      (`e2e4`, `e7e8q`, `e1g1`, …) so the two distinct move types compare on a
//!      common ground, and
//!   2. **perft** to a few depths.
//!
//! Both engines are validated independently against Fairy-Stockfish, so they
//! *should* agree everywhere — a failure here means a genuine bug in one of the
//! two standard-chess implementations, and the assertion prints the FEN and the
//! exact symmetric difference of the two move sets.
//!
//! Everything runs by default (both engines are in-repo; no external engine is
//! needed). Runtime is bounded: the exhaustive per-node walk is capped at a
//! shallow depth per position and the random self-play at a fixed ply/game
//! budget — see `WALK_CAP_NOTE` / `SELFPLAY_*` below.

use std::collections::{BTreeMap, BTreeSet};

use mcr::geometry::{perft as gperft, Chess8x8, GenericPosition, StandardChess, WideMove};
use mcr::{perft as cperft, Move, Position};

type GenPos = GenericPosition<Chess8x8, StandardChess>;

// ---------------------------------------------------------------------------
// Move normalisation: both engines' native move types → the SAME UCI string.
// Standard chess has no drops, no gates, and no overflow tokens, so UCI maps
// cleanly: `<from><to>` plus a lowercase promotion suffix, castling as the
// king's two-square move (`e1g1` / `e1c1`). Both `Move::to_uci` and
// `WideMove::to_uci::<Chess8x8>` are the crate's own canonical renderers.
// ---------------------------------------------------------------------------

/// The concrete engine's legal moves, keyed by canonical UCI (UCI is injective,
/// so the keys form the move *set*).
fn concrete_moves(pos: &Position) -> BTreeMap<String, Move> {
    pos.legal_moves()
        .into_iter()
        .map(|m| (m.to_uci(), m))
        .collect()
}

/// The generic engine's legal moves, keyed by the same canonical UCI.
fn generic_moves(pos: &GenPos) -> BTreeMap<String, WideMove> {
    pos.legal_moves()
        .into_iter()
        .map(|m| (m.to_uci::<Chess8x8>(), m))
        .collect()
}

/// Asserts the two engines expose the identical legal-move set at `fen`. On a
/// mismatch, fails with the FEN and the symmetric difference (which side has the
/// extra / missing move) — the precise shape of any real divergence.
fn assert_move_sets_agree(
    cmap: &BTreeMap<String, Move>,
    gmap: &BTreeMap<String, WideMove>,
    fen: &str,
) {
    let cset: BTreeSet<&str> = cmap.keys().map(String::as_str).collect();
    let gset: BTreeSet<&str> = gmap.keys().map(String::as_str).collect();
    if cset != gset {
        let concrete_only: Vec<&str> = cset.difference(&gset).copied().collect();
        let generic_only: Vec<&str> = gset.difference(&cset).copied().collect();
        panic!(
            "legal-move-set divergence between concrete and generic standard chess\n  \
             FEN: {fen}\n  concrete-only moves: {concrete_only:?}\n  \
             generic-only moves:  {generic_only:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Corpus.
// ---------------------------------------------------------------------------

/// `(fen, label, exhaustive-walk depth, perft depth)`. The walk checks the
/// move-set equality at *every* node down to its depth; perft checks the node
/// counts. Depths are kept shallow to bound the debug-build runtime while still
/// visiting the interesting tactics (castling, en passant, promotion, pins,
/// checks) many times over.
const CORPUS: &[(&str, &str, u32, u32)] = &[
    // Published perft positions (CPW): startpos, Kiwipete, positions 3–6. The
    // high-branching ones use a depth-2 exhaustive walk (the random self-play
    // reaches far deeper along sampled lines); the sparser ones use depth 3–4.
    (
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "startpos",
        3,
        4,
    ),
    (
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        "kiwipete",
        2,
        3,
    ),
    (
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        "cpw-pos3",
        4,
        4,
    ),
    (
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        "cpw-pos4",
        3,
        3,
    ),
    (
        "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        "cpw-pos5",
        2,
        3,
    ),
    (
        "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
        "cpw-pos6",
        2,
        3,
    ),
    // Castling available both sides, both colours to move.
    (
        "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
        "castle-open-w",
        3,
        4,
    ),
    (
        "r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1",
        "castle-open-b",
        3,
        4,
    ),
    // En passant available (black just played f7f5; white can take e5xf6 e.p.).
    (
        "rnbqkbnr/ppp1p1pp/8/3pPp2/8/8/PPPP1PPP/RNBQKBNR w KQkq f6 0 3",
        "en-passant",
        3,
        4,
    ),
    // Promotion (white a7 promotes; black h2 promotes) — both sides.
    ("8/Pk6/8/8/8/8/6Kp/8 w - - 0 1", "promotion-w", 4, 5),
    ("8/Pk6/8/8/8/8/6Kp/8 b - - 0 1", "promotion-b", 4, 5),
    // Absolute pin: black queen a5 pins the c3 knight to the e1 king.
    (
        "rnb1kbnr/pp1ppppp/8/q1p5/8/2N5/PPPPPPPP/R1BQKBNR w KQkq - 2 3",
        "pin",
        3,
        4,
    ),
    // In check: white Bb5+ gives check; black to move must respond.
    (
        "rnbqkbnr/ppp2ppp/8/1B1pp3/4P3/8/PPPP1PPP/RNBQK1NR b KQkq - 1 3",
        "in-check",
        3,
        4,
    ),
];

/// Bookkeeping note: the exhaustive walk depth per corpus entry is capped in
/// [`CORPUS`]. Anything deeper is reached only along random self-play lines.
const WALK_CAP_NOTE: &str = "exhaustive per-node walk depth is capped per corpus entry (2–4)";

// ---------------------------------------------------------------------------
// Exhaustive shallow walk: check the move-set at every node down to `depth`,
// driving both engines in lockstep by playing the UCI-matched move on each.
// ---------------------------------------------------------------------------

/// Visits every node of the game tree of `cpos`/`gpos` down to `depth`, checking
/// move-set agreement at each. Returns the number of nodes checked.
fn walk(cpos: &Position, gpos: &GenPos, depth: u32, nodes: &mut u64) {
    let cmap = concrete_moves(cpos);
    let gmap = generic_moves(gpos);
    assert_move_sets_agree(&cmap, &gmap, &cpos.to_fen());
    // The generic FEN must match the concrete FEN too: a board/state drift would
    // otherwise let equal move *sets* mask a divergent position.
    debug_assert_eq!(cpos.to_fen(), gpos.to_fen());
    *nodes += 1;
    if depth == 0 {
        return;
    }
    for (uci, cmv) in &cmap {
        let gmv = gmap
            .get(uci)
            .expect("move sets already asserted equal, so the UCI key exists on both sides");
        walk(&cpos.play(cmv), &gpos.play(gmv), depth - 1, nodes);
    }
}

// ---------------------------------------------------------------------------
// Seeded random self-play: walk N random games to a fixed ply depth, checking
// move-set equality at every node. A tiny dependency-free splitmix64 PRNG picks
// the move index; both engines play the same UCI-sorted index, so they stay on
// the exact same line.
// ---------------------------------------------------------------------------

/// One step of splitmix64 — deterministic, dependency-free (mirrors the PRNG the
/// other seeded tests use).
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

const SELFPLAY_SEEDS: &[u64] = &[
    0x0000_0000_0000_0001,
    0xDEAD_BEEF_CAFE_F00D,
    0x1234_5678_9ABC_DEF0,
    0x0F0F_0F0F_0F0F_0F0F,
    0xA5A5_A5A5_A5A5_A5A5,
    0x9E37_79B9_7F4A_7C15,
    0x0BAD_C0DE_DEAD_10CC,
    0xFEED_FACE_8BAD_F00D,
];
const SELFPLAY_PLIES: u32 = 80;

/// Plays one seeded random game from `fen`, up to `SELFPLAY_PLIES`, checking
/// move-set agreement at every node. Returns the number of nodes checked.
fn self_play(fen: &str, seed: u64) -> u64 {
    let mut cpos = Position::from_fen(fen).expect("valid corpus FEN (concrete)");
    let mut gpos = GenPos::from_fen(fen).expect("valid corpus FEN (generic)");
    let mut state = seed;
    let mut nodes = 0u64;
    for _ in 0..SELFPLAY_PLIES {
        let cmap = concrete_moves(&cpos);
        let gmap = generic_moves(&gpos);
        assert_move_sets_agree(&cmap, &gmap, &cpos.to_fen());
        nodes += 1;
        if cmap.is_empty() {
            break; // checkmate or stalemate — both agree there are no moves.
        }
        // Both maps are BTreeMaps keyed by the identical UCI set, so their key
        // orders coincide; the same index selects the same move on both engines.
        let idx = (splitmix64(&mut state) as usize) % cmap.len();
        let uci = cmap.keys().nth(idx).expect("index < len").clone();
        cpos = cpos.play(&cmap[&uci]);
        gpos = gpos.play(&gmap[&uci]);
    }
    nodes
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

/// Perft agreement: the two engines return identical node counts on every
/// corpus position, up to that entry's perft depth.
#[test]
fn perft_agrees_on_corpus() {
    for &(fen, label, _walk_depth, perft_depth) in CORPUS {
        let cpos = Position::from_fen(fen).expect("valid corpus FEN (concrete)");
        let gpos = GenPos::from_fen(fen).expect("valid corpus FEN (generic)");
        for depth in 1..=perft_depth {
            let concrete = cperft(&cpos, depth);
            let generic = gperft(&gpos, depth);
            assert_eq!(
                concrete, generic,
                "perft({depth}) divergence at {label} ({fen}): concrete {concrete} != generic {generic}"
            );
        }
    }
}

/// Move-set agreement at every node of a shallow exhaustive walk of each corpus
/// position. This is the strong claim: not just equal node *counts*, but the
/// identical *set of moves* at every reachable node down to the walk depth.
#[test]
fn move_sets_agree_over_shallow_walk() {
    assert!(!WALK_CAP_NOTE.is_empty());
    let mut total = 0u64;
    for &(fen, label, walk_depth, _perft_depth) in CORPUS {
        let cpos = Position::from_fen(fen).expect("valid corpus FEN (concrete)");
        let gpos = GenPos::from_fen(fen).expect("valid corpus FEN (generic)");
        let mut nodes = 0u64;
        walk(&cpos, &gpos, walk_depth, &mut nodes);
        assert!(nodes > 0, "walk visited at least the root for {label}");
        total += nodes;
    }
    // Sanity: the walk did real work (tens of thousands of nodes checked).
    assert!(
        total > 20_000,
        "shallow walk should cover many nodes, got {total}"
    );
}

/// Move-set agreement along seeded random self-play games from the corpus
/// positions — reaches far deeper (up to `SELFPLAY_PLIES`) than the exhaustive
/// walk, along random lines, catching divergence that only appears in deep or
/// unusual middlegame/endgame states.
#[test]
fn move_sets_agree_over_random_self_play() {
    let mut total_nodes = 0u64;
    let mut games = 0u64;
    for &(fen, _label, _wd, _pd) in CORPUS {
        for &seed in SELFPLAY_SEEDS {
            total_nodes += self_play(fen, seed);
            games += 1;
        }
    }
    assert!(games >= 100, "ran many self-play games, got {games}");
    assert!(
        total_nodes > 1_000,
        "self-play visited many nodes, got {total_nodes}"
    );
}
