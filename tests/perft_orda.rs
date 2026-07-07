//! Orda (8x8) perft validation on the generic engine (issue #214) — the variant
//! exercising an **asymmetric Mongolian-cavalry army** (knight-move /
//! slider-capture leapers), **pawn promotion to Queen or Kheshig**, and the
//! **flag-win (campmate)** terminal rule.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Orda` perft and by Fairy-Stockfish (FSF, `UCI_Variant orda`)
//! running `go perft` on the byte-identical position — the FSF divide matches
//! mcr's move-for-move, including each Orda piece's knight-move / slider-capture
//! split (the Lancer captures like a rook, the Archer like a bishop, both *move*
//! like a knight), the Kheshig (King + Knight) leaper, the Yurt (silver general),
//! q/Kheshig promotion for both colours, and the flag win (a king on its goal rank
//! ends the game, terminating perft descent). The `compare-fairy/` harness re-runs
//! that head-to-head on demand (`compare-fairy/src/orda.rs`); this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant orda`, `position startpos`) renders the start as
//!
//! ```text
//! lhaykahl/8/pppppppp/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1
//! ```
//!
//! with FSF's Orda letters `l h a y k` (Lancer, Kheshig, Archer, Yurt, King). mcr
//! reuses `l`/`h`/`a` for its Lance/Hoplite/Hawk, so the Orda pieces take distinct
//! letters — Lancer `f`, Kheshig `w`, Archer `y`, Yurt `s` (the existing Silver) —
//! and its canonical start FEN is
//!
//! ```text
//! fwyskywf/8/pppppppp/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1
//! ```
//!
//! The two are the same position; `compare-fairy/` translates the Orda letters when
//! driving FSF. Note the **asymmetry**: the Black Orda pawns start on the 6th rank
//! (one rank advanced, the 7th empty) and never double-step. Only White (the
//! standard army) has castling rights.
//!
//! ## Confirmed semantics (all pinned move-for-move against FSF)
//!
//! * **Asymmetric armies.** White = standard P/N/B/R/Q/K (the only side with
//!   castling). Black = Orda: Lancer, Kheshig, Archer, Yurt, one King, and pawns on
//!   the 6th rank.
//! * **Piece movement.** Lancer — knight *move*, rook *capture*. Archer — knight
//!   *move*, bishop *capture*. Kheshig — King + Knight (16 squares), moves and
//!   captures alike. Yurt — silver general (four diagonals + one straight forward).
//! * **Promotion.** A pawn of either colour promotes to a Queen or a Kheshig only.
//! * **Flag win.** White wins on reaching the last rank, Black the first; a node
//!   whose side to move's opponent already stands on its goal rank is terminal.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_orda -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Orda};

/// The Orda starting FEN in mcr's dialect, confirmed against FSF's
/// `UCI_Variant orda` / `position startpos`.
const STARTPOS: &str = "fwyskywf/8/pppppppp/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1";

/// An opening line after `1. e4` — a White centre pawn advanced, Black to move and
/// the Orda army intact, exercising the asymmetric early development.
const OPENING: &str = "fwyskywf/8/pppppppp/8/4P3/8/PPPP1PPP/RNBQKBNR b KQ - 0 1";

/// A developed middlegame: both sides have moved pieces out, Black has pushed two
/// Lancers (`f`) to the fourth rank where they bear (as rooks) on the half-open
/// files and (as knights) into the centre.
const DEVELOPED: &str = "1wysk1w1/8/p1pppp1p/8/2f2f2/PP4PP/2PPPP2/RNBQKBNR b KQ - 0 1";

/// A tactical position exercising every Orda piece at once: a Black Lancer (`f`,
/// rook-capture), Archer (`y`, bishop-capture), Yurt (`s`, silver), and Kheshig
/// (`w`, King+Knight) loose in the centre against two White pawns.
const TACTIC: &str = "4k3/8/3y4/2f1s3/2P1P3/3w4/8/4K3 b - - 0 1";

/// An endgame where White holds a **promoted Kheshig** (`W`, the only Orda piece
/// White can acquire) against the full Black Orda back rank — exercising the
/// Kheshig's King + Knight leaps for both colours.
const WHITE_KHESHIG: &str = "fwysk1wf/8/8/8/8/8/4W3/4K3 b - - 0 1";

/// A both-sides promotion race: a White pawn one step from the last rank and a
/// Black pawn one step from the first, each promoting to a Queen or a Kheshig.
const PROMOTION: &str = "7k/P7/8/8/8/8/7p/K7 w - - 0 1";

/// A king-flag race: both kings a short walk from their goal ranks, so several
/// lines end by **flag win** (a king reaching its goal rank), terminating perft
/// descent exactly as FSF does.
const FLAG_RACE: &str = "8/4K3/8/8/8/8/4k3/8 w - - 0 1";

/// Asserts the generic Orda perft equals each pinned `(depth, nodes)` count. Every
/// number here also matched FSF orda `go perft` on the same FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Orda::from_fen(fen).expect("valid Orda FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Orda perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 20), (2, 560), (3, 12462), (4, 342351)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 8467638), (6, 234530433)]);
}

// -- Opening line (FSF-confirmed) -------------------------------------------

#[test]
fn opening_cheap() {
    check(OPENING, &[(1, 28), (2, 840), (3, 23084), (4, 711710)]);
}

// -- Developed middlegame (FSF-confirmed) -----------------------------------

#[test]
fn developed_cheap() {
    check(DEVELOPED, &[(1, 39), (2, 700), (3, 27019), (4, 557825)]);
}

// -- Orda-piece tactic (FSF-confirmed) --------------------------------------

#[test]
fn tactic_cheap() {
    check(TACTIC, &[(1, 35), (2, 78), (3, 2713), (4, 9844)]);
}

// -- White promoted Kheshig (FSF-confirmed) ---------------------------------

#[test]
fn white_kheshig_cheap() {
    check(WHITE_KHESHIG, &[(1, 28), (2, 472), (3, 14054), (4, 244291)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn white_kheshig_deep() {
    check(WHITE_KHESHIG, &[(5, 7663419)]);
}

// -- Promotion to Queen / Kheshig (FSF-confirmed) ---------------------------

#[test]
fn promotion_cheap() {
    check(
        PROMOTION,
        &[(1, 5), (2, 22), (3, 191), (4, 1597), (5, 18061)],
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
    let pos = Orda::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    let reparsed = Orda::from_fen(STARTPOS).expect("startpos FEN parses");
    assert_eq!(reparsed.to_fen(), STARTPOS);
}
