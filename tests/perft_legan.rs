//! Legan chess perft validation on the generic engine.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Legan::perft` and by Fairy-Stockfish (FSF, `UCI_Variant legan`, a
//! built-in — the Legan pawn is FSF Betza `mflFcflW`) on the byte-identical position.
//! The `compare-fairy/` harness re-runs that head-to-head on demand (see
//! `compare-fairy/src/legan.rs`); this test pins the confirmed numbers so a
//! regression is caught without FSF present.
//!
//! ## What Legan chess is
//!
//! Standard chess pieces arrayed along a corner diagonal (each side attacks toward
//! the opposite corner) with a **directional pawn** and an **L-shaped corner
//! promotion region**. A White pawn *moves* one square diagonally up-left and
//! *captures* one square straight up (north) or straight left (west); Black is
//! mirrored (down-right move, south/east capture). There is no double step and no en
//! passant. Promotion (to Q/R/B/N) happens on the corner squares
//! `{a8,b8,c8,d8,a7,a6,a5}` for White (`{e1,f1,g1,h1,h2,h3,h4}` for Black) — a set of
//! squares, so promotion can occur off the last rank. Because the armies sit on a
//! diagonal, the counts diverge from standard chess immediately (startpos perft
//! 1/2/3 = 8/64/724).
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_legan -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Legan};

/// The Legan starting FEN, confirmed byte-for-byte against Fairy-Stockfish's
/// `UCI_Variant legan` / `position startpos`.
const STARTPOS: &str = "knbrp3/bqpp4/npp5/rp1p3P/p3P1PR/5PPN/4PPQB/3PRBNK w - - 0 1";

/// A White-to-move midgame: the `b7` pawn is one diagonal step from promoting in the
/// corner (`b7-a8`), and the `d4` pawn can advance diagonally, capture north
/// (`d4xd5`), or capture west (`d4xc4`) — the full Legan pawn repertoire in one tree.
const MIDGAME_WHITE: &str = "7k/1P6/8/3n4/2nP4/8/8/7K w - - 0 1";

/// A Black-to-move midgame mirroring the White one: the `g2` pawn is one diagonal
/// step from promoting in the lower-right corner (`g2-h1`), and the `e4` pawn can
/// advance diagonally (`e4-f3`), capture south (`e4xe3`), or capture east (`e4xf4`).
const MIDGAME_BLACK: &str = "7k/8/8/8/4pP2/3PP3/6p1/K7 b - - 0 1";

/// Asserts the generic Legan perft equals each pinned `(depth, nodes)` count. Every
/// number here also matched FSF legan `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Legan::from_fen(fen).expect("valid Legan FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Legan perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 8), (2, 64), (3, 724), (4, 8138)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 112012)]);
}

// -- White midgame: diagonal move, both captures, corner promotion ----------

#[test]
fn midgame_white_cheap() {
    check(MIDGAME_WHITE, &[(1, 10), (2, 140), (3, 1376)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_white_deep() {
    check(MIDGAME_WHITE, &[(4, 20104)]);
}

// -- Black midgame: diagonal move, both captures, corner promotion ----------

#[test]
fn midgame_black_cheap() {
    check(MIDGAME_BLACK, &[(1, 10), (2, 60), (3, 604)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_black_deep() {
    check(MIDGAME_BLACK, &[(4, 4463)]);
}
