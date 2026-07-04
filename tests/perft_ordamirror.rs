//! Ordamirror (8x8) perft validation on the generic engine (issue #220) — the
//! **symmetric** mirror variant where **both** armies are Orda-style horde pieces
//! (knight-move / slider-capture leapers plus the queen-move / knight-capture
//! Falcon), with **pawn promotion to Lancer / Kheshig / Archer / Falcon** and the
//! **flag-win (campmate)** terminal rule.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Ordamirror` perft and by Fairy-Stockfish (FSF,
//! `UCI_Variant ordamirror`) running `go perft` on the byte-identical position —
//! the FSF divide matches mcr's move-for-move, including each horde piece's
//! split: the Lancer captures like a rook and the Archer like a bishop (both
//! *move* like a knight), the Kheshig is a King + Knight leaper, and the **Falcon
//! moves like a queen but captures like a knight** (the inverse of the Lancer /
//! Archer). It also covers `lhaf` promotion for both colours and the flag win (a
//! king on its goal rank ends the game, terminating perft descent). The
//! `compare-fairy/` harness re-runs that head-to-head on demand
//! (`compare-fairy/src/ordamirror.rs`); this test pins the FSF-confirmed numbers
//! so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant ordamirror`, `position startpos`) renders the start as
//!
//! ```text
//! lhafkahl/8/pppppppp/8/8/PPPPPPPP/8/LHAFKAHL w - - 0 1
//! ```
//!
//! with FSF's letters `l h a f k` (Lancer, Kheshig, Archer, Falcon, King). mcr
//! reuses `l`/`h`/`a` for its Lance/Hoplite/Hawk, so the horde pieces take
//! distinct letters — Lancer `f`, Kheshig `w`, Archer `y` — and the Falcon takes
//! the overflow token `*f` (its base letter `f` is the FSF mnemonic; the `*`
//! prefix keeps it distinct from the bare Lancer `f`). mcr's canonical start FEN
//! is
//!
//! ```text
//! fwy*fkywf/8/pppppppp/8/8/PPPPPPPP/8/FWY*FKYWF w - - 0 1
//! ```
//!
//! The two are the same position; `compare-fairy/` translates the letters when
//! driving FSF. Note the **symmetric pawn layout**: both armies' pawns start one
//! rank advanced (White on the 3rd rank, Black on the 6th; the 2nd / 7th ranks
//! are empty), so neither side double-steps and there is no en passant. There is
//! **no castling**.
//!
//! ## Confirmed semantics (all pinned move-for-move against FSF)
//!
//! * **Symmetric armies.** Both sides field the same back rank: Lancer, Kheshig,
//!   Archer, Falcon, King, Archer, Kheshig, Lancer, with pawns one rank advanced.
//! * **Piece movement.** Lancer — knight *move*, rook *capture*. Archer — knight
//!   *move*, bishop *capture*. Kheshig — King + Knight (16 squares), moves and
//!   captures alike. Falcon — queen *move*, knight *capture*.
//! * **Promotion.** A pawn of either colour promotes to a Lancer, Kheshig,
//!   Archer, or Falcon only (never a Queen/Rook/Bishop/Knight).
//! * **Flag win.** White wins on reaching the last rank, Black the first; a node
//!   whose side to move's opponent already stands on its goal rank is terminal.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_ordamirror -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Ordamirror};

/// The Ordamirror starting FEN in mcr's dialect, confirmed against FSF's
/// `UCI_Variant ordamirror` / `position startpos`.
const STARTPOS: &str = "fwy*fkywf/8/pppppppp/8/8/PPPPPPPP/8/FWY*FKYWF w - - 0 1";

/// An opening line after `1. e4` — a White centre pawn advanced, Black to move,
/// both horde armies intact, exercising early development of the mirror horde.
const OPENING: &str = "fwy*fkywf/8/pppppppp/8/4P3/8/PPP1PPPP/FWY*FKYWF b - - 0 1";

/// A developed middlegame: both sides have moved pieces out, with a White centre
/// pawn and a Black Archer (`y`, bishop-capture) advanced into the centre.
const DEVELOPED: &str = "fwy*fk1wf/8/p1pppppp/8/2y1P3/2P5/PP1P1PPP/FWY*FK1WF w - - 0 1";

/// A Falcon tactic: a White Falcon (`*F`) and a Black Falcon (`*f`) face off in
/// the centre with pawns on the squares that exercise the Falcon's distinctive
/// queen-*move* / knight-*capture* split.
const FALCON_TACTIC: &str = "4k3/8/3p1p2/3p*f3/3*F4/2P1P3/8/4K3 w - - 0 1";

/// A middlegame with both Falcons developed to the fourth/third ranks where their
/// queen slides and knight captures bear on the centre.
const FALCON_MID: &str = "fwy1k1wf/8/p1ppp1pp/5p2/2*f5/2*F5/PP1PPPPP/FWY1K1WF w - - 0 1";

/// A both-sides promotion race: a White pawn one step from the last rank and a
/// Black pawn one step from the first, each promoting to a Lancer / Kheshig /
/// Archer / Falcon (four targets per pawn — never a Queen).
const PROMOTION: &str = "7k/P7/8/8/8/8/7p/K7 w - - 0 1";

/// A king-flag race: both kings a short walk from their goal ranks, so several
/// lines end by **flag win** (a king reaching its goal rank), terminating perft
/// descent exactly as FSF does.
const FLAG_RACE: &str = "8/4K3/8/8/8/8/4k3/8 w - - 0 1";

/// Asserts the generic Ordamirror perft equals each pinned `(depth, nodes)`
/// count. Every number here also matched FSF `ordamirror` `go perft` on the same
/// FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Ordamirror::from_fen(fen).expect("valid Ordamirror FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Ordamirror perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 28), (2, 784), (3, 22487), (4, 645337)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 19078975), (6, 564100355)]);
}

// -- Opening line (FSF-confirmed) -------------------------------------------

#[test]
fn opening_cheap() {
    check(OPENING, &[(1, 28), (2, 895), (3, 25692), (4, 872506)]);
}

// -- Developed middlegame (FSF-confirmed) -----------------------------------

#[test]
fn developed_cheap() {
    check(DEVELOPED, &[(1, 32), (2, 1190), (3, 39329), (4, 1468391)]);
}

// -- Falcon tactic: queen-move / knight-capture (FSF-confirmed) -------------

#[test]
fn falcon_tactic_cheap() {
    check(FALCON_TACTIC, &[(1, 20), (2, 328), (3, 6959), (4, 134565)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn falcon_tactic_deep() {
    check(FALCON_TACTIC, &[(5, 2829937)]);
}

// -- Both Falcons developed (FSF-confirmed) ---------------------------------

#[test]
fn falcon_mid_cheap() {
    check(FALCON_MID, &[(1, 40), (2, 1605), (3, 60848), (4, 2446169)]);
}

// -- Promotion to Lancer / Kheshig / Archer / Falcon (FSF-confirmed) --------

#[test]
fn promotion_cheap() {
    check(
        PROMOTION,
        &[(1, 7), (2, 44), (3, 427), (4, 4236), (5, 61551)],
    );
}

// -- Flag win / campmate terminal rule (FSF-confirmed) ----------------------

#[test]
fn flag_race_cheap() {
    check(
        FLAG_RACE,
        &[(1, 8), (2, 40), (3, 200), (4, 1309), (5, 8440)],
    );
}

// -- The starting FEN round-trips through mcr's FEN I/O ----------------------

#[test]
fn startpos_fen_round_trips() {
    let pos = Ordamirror::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    let reparsed = Ordamirror::from_fen(STARTPOS).expect("startpos FEN parses");
    assert_eq!(reparsed.to_fen(), STARTPOS);
}
