//! Knightmate (8x8) perft validation on the generic engine (issue #224) — the
//! variant whose **Knight is royal** and whose king is replaced by a non-royal
//! **Commoner** (a Mann).
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mce::geometry::Knightmate::perft` and by Fairy-Stockfish (FSF, `UCI_Variant
//! knightmate`, an FSF built-in) running `go perft` on the byte-identical
//! position — the FSF divide matches mce's move-for-move, including the royal
//! Knight's leaps and check/mate, the Commoner's king-steps, standard castling for
//! the royal Knight, and pawn promotion to a Commoner / Bishop / Rook / Queen
//! (never a Knight). The `compare-fairy/` harness re-runs that head-to-head on
//! demand (`compare-fairy/src/knightmate.rs`); this test pins the FSF-confirmed
//! numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant knightmate`, `position startpos`) renders the start as
//!
//! ```text
//! rmbqkbmr/pppppppp/8/8/8/8/PPPPPPPP/RMBQKBMR w KQkq - 0 1
//! ```
//!
//! with FSF's Commoner letter `m`/`M` on the knight files (`b` and `g`) and the
//! royal Knight on the king square (`k`/`K`). mce uses the same board but spells
//! the Commoner with its overflow token `*u` / `*U`:
//!
//! ```text
//! r*ubqkb*ur/pppppppp/8/8/8/8/PPPPPPPP/R*UBQKB*UR w KQkq - 0 1
//! ```
//!
//! The two are the same position; `compare-fairy/` rewrites `*u → m` when driving
//! FSF. No new role or letter is introduced — the royal Knight reuses
//! `WideRole::King` (given the knight attack set) and the Commoner reuses
//! `WideRole::Commoner`.
//!
//! ## Confirmed semantics (all pinned move-for-move against FSF)
//!
//! * **Royal Knight.** The piece on the king's square moves and gives check as a
//!   Knight; it is the side's royal piece (its capture ends the game). Check,
//!   king-danger, pins, and checkmate all track it. Two opposing royal Knights are
//!   never mutually attacking-adjacent (knights do not attack adjacent squares).
//! * **Commoner.** A non-royal king-stepper on the knight files; freely capturable,
//!   never defines check.
//! * **Castling.** Standard king-and-rook castling, both colours; the royal Knight
//!   castles to g/c exactly as a king does.
//! * **Promotion.** A pawn promotes to a Commoner, Bishop, Rook, or Queen — never a
//!   Knight (the bare Knight role does not exist) and never the royal King.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_knightmate -- --include-ignored`.

use mce::geometry::{perft as gperft, Chess8x8, Knightmate};

/// The Knightmate starting FEN in mce's dialect, confirmed against FSF's
/// `UCI_Variant knightmate` / `position startpos`.
const STARTPOS: &str = "r*ubqkb*ur/pppppppp/8/8/8/8/PPPPPPPP/R*UBQKB*UR w KQkq - 0 1";

/// A closed-pawn middlegame (both sides have played c- and d-pawn advances): the
/// Commoners and royal Knights jostle behind a locked centre. Exercises the
/// Commoner steps and royal-Knight leaps in a crowded board.
const MID_CLOSED: &str = "r*ubqkb*ur/pp2pppp/2pp4/8/2PP4/8/PP2PPPP/R*UBQKB*UR w KQkq - 0 1";

/// A both-sides-castling-ready middlegame with the back ranks cleared between the
/// rooks and the royal Knight (`R3K2R` / `r3k2r`), developed Commoners on the third
/// ranks, bishops and queens out. Exercises both castles, Commoner and bishop play,
/// and royal-Knight tactics simultaneously.
const MID_CASTLE: &str = "r3k2r/pppq1ppp/2*up1*u2/2b1p3/2B1P3/2*UP1*U2/PPPQ1PPP/R3K2R w KQkq - 0 1";

/// The White royal Knight on e1 is in check from a rook on e2 (and must leap off
/// both the e-file and rank 2), while two White pawns sit one step from promotion
/// and a Black Commoner roams the centre. Exercises royal-Knight check evasion
/// *and* the four-way promotion set in one tree.
const CHECK_PROMO: &str = "4k3/1P3P2/8/8/3*u4/8/4r3/4K3 w - - 0 1";

/// Asserts the generic Knightmate perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF knightmate `go perft` on the same FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Knightmate::from_fen(fen).expect("valid Knightmate FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Knightmate perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 18), (2, 324), (3, 6765), (4, 139774)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 3249033), (6, 74568983)]);
}

// -- Closed-pawn middlegame (FSF-confirmed) ---------------------------------

#[test]
fn mid_closed_cheap() {
    check(MID_CLOSED, &[(1, 28), (2, 723), (3, 20840), (4, 560385)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn mid_closed_deep() {
    check(MID_CLOSED, &[(5, 16837135)]);
}

// -- Castling-ready middlegame (FSF-confirmed) ------------------------------

#[test]
fn mid_castle_cheap() {
    check(MID_CASTLE, &[(1, 37), (2, 1321), (3, 48290), (4, 1748572)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn mid_castle_deep() {
    check(MID_CASTLE, &[(5, 64361285)]);
}

// -- Royal-Knight check evasion + promotion (FSF-confirmed) -----------------

#[test]
fn check_promo() {
    check(
        CHECK_PROMO,
        &[(1, 1), (2, 4), (3, 48), (4, 969), (5, 13591)],
    );
}
