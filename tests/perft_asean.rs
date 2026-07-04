//! ASEAN chess perft validation on the generic engine (issue #261).
//!
//! ASEAN chess is a modern Makruk variant: it keeps Makruk's pieces (Khon,
//! Met, etc.) but adopts the symmetric FIDE starting array and FIDE-style
//! promotion — a pawn promotes on the **last rank** to a **Met, Rook, Silver,
//! or Knight** (four targets), where Makruk promotes only to a Met three ranks
//! earlier. Because the start array differs only in the Met/King swap and
//! promotion diverges only once a pawn reaches the far rank, ASEAN's shallow
//! perft from the start position is identical to Makruk's (depths 1–5); the two
//! first diverge at depth 6 (Makruk 142_078_049 vs ASEAN 142_078_057).
//!
//! Every `(depth, nodes)` pair below was produced identically by
//! `mcr::geometry::Asean::perft` and by Fairy-Stockfish (`UCI_Variant asean`,
//! `go perft`) on the byte-identical FEN (rewriting the mcr `s`/`m` letters to
//! FSF's `b`/`q`). The `compare-fairy/` harness re-runs that head-to-head on
//! demand (`compare-fairy/src/main.rs --asean`); this test pins the confirmed
//! numbers so a regression is caught without FSF present.
//!
//! Confirmed ASEAN starting FEN (mcr dialect; FSF reports the same array as
//! `rnbqkbnr/8/pppppppp/8/8/PPPPPPPP/8/RNBQKBNR w - - 0 1`):
//!   `rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSMKSNR w - - 0 1`
//!
//! The cheap layers run as ordinary tests; the deep layers are `#[ignore]`d so
//! `cargo test` stays fast — run them with
//! `cargo test --release --test perft_asean -- --include-ignored`.

use mcr::geometry::{perft as gperft, Asean, Chess8x8};

/// The ASEAN starting FEN, confirmed byte-for-byte against Fairy-Stockfish's
/// `UCI_Variant asean` / `position startpos` (mcr dialect).
const STARTPOS: &str = "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSMKSNR w - - 0 1";

/// A midgame: an edge pawn pushed, a king-pawn advanced, black to move.
const MID1: &str = "rnsmksnr/8/1ppppppp/p7/4P3/PPPP1PPP/8/RNSMKSNR b - - 0 2";

/// A midgame with an open centre and both knights developed.
const MID2: &str = "r1smks1r/3n4/ppp1pppp/3p4/3P4/PPP1PPPP/4N3/R1SMKS1R w - - 0 4";

/// A promotion stress position: white pawns on the seventh rank (a7, c7) and
/// black pawns on the second (a2, c2), each one step or one capture from
/// promotion, exercising ASEAN's four promotion targets. (FSF dialect:
/// `1n2k3/P1P5/8/8/8/8/p1P5/1N2K3 w - - 0 1` — no `s`/`m` letters, so it is the
/// same string for both engines.)
const PROMO: &str = "1n2k3/P1P5/8/8/8/8/p1P5/1N2K3 w - - 0 1";

/// Asserts the generic ASEAN perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `asean go perft` on the same FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Asean::from_fen(fen).expect("valid ASEAN FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "ASEAN perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(
        STARTPOS,
        &[(1, 23), (2, 529), (3, 12012), (4, 273026), (5, 6223994)],
    );
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_depth6() {
    // FSF asean `go perft 6` on the startpos: 142_078_057 (Makruk diverges here
    // at 142_078_049 — the extra promotion targets multiply once a pawn queens).
    check(STARTPOS, &[(6, 142078057)]);
}

// -- Midgame 1 (FSF-confirmed) ----------------------------------------------

#[test]
fn mid1_cheap() {
    check(
        MID1,
        &[(1, 25), (2, 576), (3, 14290), (4, 329238), (5, 8196658)],
    );
}

// -- Midgame 2 (FSF-confirmed) ----------------------------------------------

#[test]
fn mid2_cheap() {
    check(
        MID2,
        &[(1, 21), (2, 485), (3, 10687), (4, 254443), (5, 5829421)],
    );
}

// -- Promotion stress (FSF-confirmed) ---------------------------------------

#[test]
fn promo_cheap() {
    // Pawns one step / one capture from the last rank: depth 1 already emits the
    // four-target promotions (q/r/b/n), which Makruk's single Met target would
    // not. FSF-confirmed counts.
    check(
        PROMO,
        &[(1, 25), (2, 307), (3, 5629), (4, 63373), (5, 1123130)],
    );
}
