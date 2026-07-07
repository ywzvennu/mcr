//! Chaturanga perft validation on the generic engine.
//!
//! The node counts below are pinned **and cross-checked against Fairy-Stockfish**
//! (FSF, `UCI_Variant chaturanga`, a built-in): every `(depth, nodes)` pair here
//! was produced identically by `mcr::geometry::Chaturanga::perft` and by FSF's `go
//! perft` on the byte-identical position. The `compare-fairy/` harness re-runs that
//! head-to-head on demand (see `compare-fairy/src/chaturanga.rs`); this test pins
//! the confirmed numbers so a regression is caught without FSF present.
//!
//! ## Relationship to Shatranj
//!
//! Chaturanga is Shatranj with the baring-the-king loss removed and the standard
//! chess starting array (King on e, Ferz on d — the left-right mirror of
//! Shatranj's). Because perft is reflection-invariant, the **startpos** counts are
//! identical to Shatranj's at every depth. They **diverge** only where a baring
//! claim would fire: in the bared-king endgame below, Shatranj truncates the node
//! to a zero-move terminal leaf (`go perft` = 0), while chaturanga has no baring
//! rule and plays on, so its counts are non-zero.
//!
//! ## FEN dialect
//!
//! As in Shatranj, mcr spells the Ferz `m` and the Alfil `*x`, which FSF spells `q`
//! and `b`; the harness rewrites these (`*x → b`, `m → q`) when driving FSF.
//!
//! Confirmed Chaturanga starting FEN (from FSF `position startpos`, mcr dialect):
//!   `rn*xmk*xnr/pppppppp/8/8/8/8/PPPPPPPP/RN*XMK*XNR w - - 0 1`
//!
//! The cheap layers run as ordinary tests; the deep layers are `#[ignore]`d so
//! `cargo test` stays fast — run them with
//! `cargo test --release --test perft_chaturanga -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chaturanga, Chess8x8};

/// The Chaturanga starting FEN (mcr dialect), confirmed byte-for-byte against
/// Fairy-Stockfish's `UCI_Variant chaturanga` / `position startpos` — the standard
/// chess array (King on e, Ferz on d).
const STARTPOS: &str = "rn*xmk*xnr/pppppppp/8/8/8/8/PPPPPPPP/RN*XMK*XNR w - - 0 1";

/// A quiet middlegame with a knight and a centre pawn developed each side, full
/// material — a contested position with the same movegen as Shatranj.
const MID: &str = "r1*xmk*xnr/pppp1ppp/2n5/4p3/4P3/2N5/PPPP1PPP/R1*XMK*XNR w - - 0 1";

/// A bared-king endgame: Black is reduced to its lone king while White keeps its
/// king, an Alfil, and four pawns. In Shatranj this is a baring loss (a zero-move
/// terminal leaf); in **chaturanga there is no baring rule**, so the node plays on
/// and `go perft` is non-zero — exactly as FSF chaturanga reports.
const BARED: &str = "4k3/8/8/2P1P3/3*X4/2P1P3/8/4K3 w - - 0 1";

/// Asserts the generic Chaturanga perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF chaturanga `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Chaturanga::from_fen(fen).expect("valid Chaturanga FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Chaturanga perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    // No double pawn push: depth 1 is 8 single pawn pushes + 4 knight moves + 4
    // Alfil jumps = 16. These equal Shatranj's startpos counts (mirror image).
    check(
        STARTPOS,
        &[(1, 16), (2, 256), (3, 4176), (4, 68122), (5, 1164248)],
    );
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    // FSF chaturanga `go perft` on the startpos (equal to Shatranj's).
    check(STARTPOS, &[(6, 19864709), (7, 357218656)]);
}

// -- Middlegame (FSF-confirmed) ---------------------------------------------

#[test]
fn mid_cheap() {
    check(MID, &[(1, 21), (2, 440), (3, 9241), (4, 192519)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn mid_deep() {
    check(MID, &[(5, 4081638)]);
}

// -- No baring truncation (FSF-confirmed) -----------------------------------

#[test]
fn bared_king_plays_on() {
    // Chaturanga has no baring rule, so the bared node is not terminal: `go perft`
    // is non-zero at every depth. (Shatranj truncates the same position to 0.)
    check(BARED, &[(1, 13), (2, 60), (3, 751), (4, 4300)]);
}
