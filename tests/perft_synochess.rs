//! Synochess (8x8) perft validation on the generic engine (issue #212) — the
//! variant pitting a standard Western army (White) against a Chinese/Korean-chess
//! amalgamation (Black), exercising an **asymmetric army**, the **Janggi cannon**,
//! a forward/sideways **Soldier** with a fixed **rank-5 drop pocket**, the
//! **campmate** flag-rank win, and a **file-or-rank flying general**.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Synochess::perft` and by Fairy-Stockfish (FSF, `UCI_Variant
//! synochess`, from its `variants.ini`) running `go perft` on the byte-identical
//! position — the FSF divide matches mcr's move-for-move, including the Janggi
//! cannon (which needs a screen to move *and* capture and may neither screen over
//! nor capture another cannon), the Soldier's forward/sideways step, the two
//! Soldier drops onto empty rank-5 squares (never replenished by captures), the
//! Fers-Alfil elephant, the king-commoner "Advisor", campmate truncation (a king
//! on the opponent's far rank is terminal, and a king may not enter a flag rank
//! the enemy king already holds), and the file-or-rank king faceoff. The
//! `compare-fairy/` harness re-runs that head-to-head on demand
//! (`compare-fairy/src/synochess.rs`); this test pins the FSF-confirmed numbers so
//! a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! FSF renders the start (from `variants.ini`, `[synochess:pocketknight]`) as
//!
//! ```text
//! rneakenr/8/1c4c1/1ss2ss1/8/8/PPPPPPPP/RNBQKBNR[ss] w KQ - 0 1
//! ```
//!
//! with FSF's letters `e a s` for the Elephant (Fers-Alfil), Advisor (Commoner),
//! and Soldier. mcr reuses `e`/`a`/`s` for other roles, so its Synochess pieces
//! take distinct letters — Elephant `v`, Advisor `f`, Soldier `z` — giving the
//! canonical mcr start FEN
//!
//! ```text
//! rnv*ukvnr/8/1c4c1/1zz2zz1/8/8/PPPPPPPP/RNBQKBNR[zz] w KQ - 0 1
//! ```
//!
//! The two are the same position; `compare-fairy/` translates the letters (and the
//! `[zz]` pocket) when driving FSF. Only White (the standard army) has castling.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_synochess -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Synochess};

/// The Synochess starting FEN in mcr's dialect, White to move.
const STARTPOS: &str = "rnv*ukvnr/8/1c4c1/1zz2zz1/8/8/PPPPPPPP/RNBQKBNR[zz] w KQ - 0 1";

/// The starting position with **Black** to move: 50 replies — every Black piece
/// move plus the four empty-rank-5 Soldier drops (`S@a5/d5/e5/h5`).
const STARTPOS_BLACK: &str = "rnv*ukvnr/8/1c4c1/1zz2zz1/8/8/PPPPPPPP/RNBQKBNR[zz] b KQ - 0 1";

/// An asymmetric middlegame: White has developed a knight and pushed `d4` (taking
/// a Soldier en prise), Black's Soldiers are advanced to the centre and a cannon
/// is screened — exercising captures of and by the Dynasty pieces. White to move.
const MID_ASYM: &str = "rnv*uk1nr/8/1c4c1/3zz3/2zP4/5N2/PPP1PPPP/RNBQKB1R[zz] w KQ - 0 1";

/// A drop-heavy position: Black still holds **both** pocket Soldiers and all of
/// rank 5 is empty, so eight Soldier drops are available alongside the board
/// moves. Black to move.
const DROP_HEAVY: &str = "rnv*uk1nr/8/1c4c1/8/3PP3/8/PPP2PPP/RNBQKBNR[zz] b KQ - 0 1";

/// A campmate endgame, **Black** to move: the Black king (e2) can step onto rank 1
/// to win (those moves are terminal — perft leaves), while the White king (a4) is
/// off rank 1 so the flag is not contested.
const CAMPMATE_BLACK: &str = "8/8/8/8/K7/8/4k3/8 b - - 0 1";

/// A campmate endgame, **White** to move: the White king (e7) can step onto rank 8
/// to win (terminal), with the Black king (e2) off rank 8.
const CAMPMATE_WHITE: &str = "8/4K3/8/8/8/8/4k3/8 w - - 0 1";

/// Asserts the generic Synochess perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF synochess `go perft` on the same FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Synochess::from_fen(fen).expect("valid Synochess FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Synochess perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position, White to move (FSF-confirmed) --------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 20), (2, 986), (3, 21646)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(4, 1065188), (5, 25681932)]);
}

// -- Start position, Black to move: cannons, soldiers, and drops -------------

#[test]
fn startpos_black_cheap() {
    check(STARTPOS_BLACK, &[(1, 50), (2, 986), (3, 49300)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_black_deep() {
    check(STARTPOS_BLACK, &[(4, 1070367)]);
}

// -- Asymmetric middlegame (FSF-confirmed) ----------------------------------

#[test]
fn mid_asym_cheap() {
    check(MID_ASYM, &[(1, 29), (2, 1264), (3, 36059)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn mid_asym_deep() {
    check(MID_ASYM, &[(4, 1598096)]);
}

// -- Drop-heavy: both Soldiers in hand, rank 5 open (FSF-confirmed) ----------

#[test]
fn drop_heavy_cheap() {
    check(DROP_HEAVY, &[(1, 39), (2, 1431), (3, 56888)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn drop_heavy_deep() {
    check(DROP_HEAVY, &[(4, 2087370)]);
}

// -- Campmate flag-rank truncation, both colors (FSF-confirmed) --------------

#[test]
fn campmate_black_cheap() {
    check(CAMPMATE_BLACK, &[(1, 8), (2, 19), (3, 127), (4, 539)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn campmate_black_deep() {
    check(CAMPMATE_BLACK, &[(5, 3701)]);
}

#[test]
fn campmate_white_cheap() {
    check(CAMPMATE_WHITE, &[(1, 6), (2, 20), (3, 84), (4, 458)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn campmate_white_deep() {
    check(CAMPMATE_WHITE, &[(5, 2726)]);
}

// -- Flying-general blocker (issue #435, FSF-confirmed) ----------------------

/// Issue #435: the file-**or-rank** flying general must respect an intervening
/// blocker. Black king a8, a Black elephant on **d8** breaking the rank-8
/// king-to-king line, White king h7. The elephant breaks the faceoff, so the
/// White king may step to g8 / h8 and perft(1) = 21 (FSF-confirmed). The coarse
/// contested-flag-rank ban used to drop those two king moves, under-generating to
/// 19 and mis-detecting the faceoff as unbroken.
const FLYING_GENERAL_BLOCKER: &str = "k2v4/7K/8/8/8/8/PPPPPPPP/r6r[] w - - 0 1";

/// The control for `FLYING_GENERAL_BLOCKER`: with d8 empty the two kings genuinely
/// face down rank 8, so g8 / h8 are correctly forbidden and perft(1) = 19
/// (FSF-confirmed). Pins that the fix does **not** over-generate.
const FLYING_GENERAL_CONTROL: &str = "k7/7K/8/8/8/8/PPPPPPPP/r6r[] w - - 0 1";

#[test]
fn flying_general_respects_blocker() {
    check(FLYING_GENERAL_BLOCKER, &[(1, 21)]);
    check(FLYING_GENERAL_CONTROL, &[(1, 19)]);
}

// -- The starting FEN round-trips through mcr's FEN I/O ----------------------

#[test]
fn startpos_fen_round_trips() {
    let pos = Synochess::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    let reparsed = Synochess::from_fen(STARTPOS).expect("startpos FEN parses");
    assert_eq!(reparsed.to_fen(), STARTPOS);
}
