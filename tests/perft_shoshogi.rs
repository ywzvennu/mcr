//! Sho Shogi (old 9x9 Shogi **without drops**, **with** the Drunk Elephant /
//! Crown Prince) perft validation on the generic engine (issue #267).
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mce::geometry::ShoShogi::perft` and by Fairy-Stockfish (FSF, `UCI_Variant
//! shoshogi`) running `go perft` on the byte-identical position — the FSF divide
//! matches mce's move-for-move, including the full Shogi army and its
//! `+`-promotions, the Drunk Elephant's seven-direction step, the Drunk Elephant →
//! Crown Prince promotion (creating a **second royal**), and the
//! **count-thresholded** two-royal rule (while a side holds both a King and a Crown
//! Prince neither is royal — it may leave either en prise and is never in check;
//! reduced to one, that piece is an ordinary royal). The `compare-fairy/` harness
//! re-runs the head-to-head on demand (`compare-fairy/src/shoshogi.rs`); this test
//! pins the FSF-confirmed numbers so a regression is caught even without FSF.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant shoshogi`, `position startpos`) renders the start as
//!
//! ```text
//! lnsgkgsnl/1r2e2b1/ppppppppp/9/9/9/PPPPPPPPP/1B2E2R1/LNSGKGSNL w 0 1
//! ```
//!
//! with the Drunk Elephant as `e`/`E`. The single-`*` overflow alphabet being
//! exhausted, mce spells the Drunk Elephant and Crown Prince with the **doubled**
//! overflow prefix `**` (`**E`/`**e` Drunk Elephant, `**C`/`**c` Crown Prince), so
//! mce's start FEN is
//!
//! ```text
//! lnsgkgsnl/1r2**e2b1/ppppppppp/9/9/9/PPPPPPPPP/1B2**E2R1/LNSGKGSNL w - - 0 1
//! ```
//!
//! The two are the same position; `compare-fairy/` rewrites `**e → e`, `**c → +E`
//! when driving FSF. The deep layers are `#[ignore]`d so `cargo test` stays fast —
//! run them with `cargo test --release --test perft_shoshogi -- --include-ignored`.

use mce::geometry::{perft as gperft, ShoShogi, Shogi9x9};

/// The Sho Shogi starting FEN in mce's dialect, confirmed against FSF's
/// `UCI_Variant shoshogi` / `position startpos`.
const STARTPOS: &str =
    "lnsgkgsnl/1r2**e2b1/ppppppppp/9/9/9/PPPPPPPPP/1B2**E2R1/LNSGKGSNL w - - 0 1";

/// A developed middlegame (a few pawns advanced on both wings) — exercises the
/// full army's interactions away from the opening.
const MIDGAME: &str =
    "lnsgkgsnl/1r2**e2b1/p1pppp1pp/1p4p2/9/2P3P2/PP1PPP1PP/1B2**E2R1/LNSGKGSNL w - - 0 1";

/// **Two white royals**: a King (e1) and a Crown Prince (e3, a promoted Drunk
/// Elephant). While White holds both, **neither is royal** — White is never in
/// check and may move (or expose) either freely; every pseudo-legal move is legal.
const TWO_ROYALS: &str = "4k4/9/9/9/9/9/4**C4/9/4K4 w - - 0 1";

/// A white **Drunk Elephant in the promotion zone** (e7): each of its moves may
/// promote to a **Crown Prince**, giving White a second royal — a promotion that
/// is always legal (it drops the side's pseudo-royalty), exactly as in FSF.
const DE_PROMOTE: &str = "4k4/9/4**E4/9/9/9/9/9/4K4 w - - 0 1";

/// A **lone Crown Prince** (White's only royal, after the King was lost) standing
/// in check from a Rook on a2: it behaves as an ordinary royal — only the two
/// king-moves that escape the check are legal.
const LONE_CROWN_PRINCE: &str = "3k5/9/9/9/9/9/9/r8/4**C4 w - - 0 1";

/// Asserts the generic Sho Shogi perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `shoshogi` `go perft` on the same FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = ShoShogi::from_fen(fen).expect("valid Sho Shogi FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Shogi9x9, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Sho Shogi perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 26), (2, 676), (3, 17368), (4, 445372)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 11494746), (6, 296171901)]);
}

// -- Developed middlegame (FSF-confirmed) -----------------------------------

#[test]
fn midgame_cheap() {
    check(MIDGAME, &[(1, 36), (2, 1199), (3, 41031), (4, 1358571)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_deep() {
    check(MIDGAME, &[(5, 46281288)]);
}

// -- Two royals: count-thresholded pseudo-royalty (FSF-confirmed) -----------

#[test]
fn two_royals_cheap() {
    // With both a King and a Crown Prince, White is never in check.
    check(TWO_ROYALS, &[(1, 13), (2, 65), (3, 830), (4, 5644)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn two_royals_deep() {
    check(TWO_ROYALS, &[(5, 75616)]);
}

// -- Drunk Elephant → Crown Prince promotion (FSF-confirmed) ----------------

#[test]
fn de_promote_cheap() {
    check(DE_PROMOTE, &[(1, 19), (2, 56), (3, 838), (4, 3950)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn de_promote_deep() {
    check(DE_PROMOTE, &[(5, 57356)]);
}

// -- Lone Crown Prince behaves as a royal king (FSF-confirmed) --------------

#[test]
fn lone_crown_prince_cheap() {
    check(LONE_CROWN_PRINCE, &[(1, 2), (2, 74), (3, 232), (4, 5999)]);
}
