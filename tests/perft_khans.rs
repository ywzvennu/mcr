//! Khan's Chess (8x8) perft validation on the generic engine (issue #272) — the
//! Orda-family variant exercising an **asymmetric Khan cavalry army** (knight-move
//! / king-capture and forward-half-knight / forward-step leapers), a **forced
//! soldier promotion to a Khan**, and the **flag-win (campmate)** terminal rule.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Khans` perft and by Fairy-Stockfish (FSF, `UCI_Variant khans`)
//! running `go perft` on the byte-identical position — the FSF divide matches
//! mcr's move-for-move, including each Khan piece's distinctive move/capture split:
//! the Lancer (knight move, rook capture), Archer (knight move, bishop capture),
//! Kheshig (King + Knight), Khan (knight *move*, **king** *capture* — `mNcK`), and
//! Khan soldier (forward *half*-knight move, straight-forward step capture —
//! `mfhNcfW`), the soldier's forced promotion to a Khan on the last rank, standard
//! White pawn promotion, and the flag win. The `compare-fairy/` harness re-runs
//! that head-to-head on demand (`compare-fairy/src/khans.rs`); this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant khans`, `position startpos`) renders the start as
//!
//! ```text
//! lhatkahl/ssssssss/8/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1
//! ```
//!
//! with FSF's letters `l h a t k s` (Lancer, Kheshig, Archer, Khan, King, soldier).
//! mcr reuses `l`/`h`/`a` for its Lance/Hoplite/Hawk, so the shared Orda pieces take
//! the letters Lancer `f`, Kheshig `w`, Archer `y`, and the two new Khan pieces take
//! overflow-3 tokens `=t` (Khan) / `=s` (soldier):
//!
//! ```text
//! fwy=tkywf/=s=s=s=s=s=s=s=s/8/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1
//! ```
//!
//! The two are the same position; `compare-fairy/` translates the Khan letters when
//! driving FSF. Only White (the standard army) has castling rights.
//!
//! ## Confirmed semantics (all pinned move-for-move against FSF)
//!
//! * **Asymmetric armies.** White = standard P/N/B/R/Q/K (the only side with
//!   castling). Black = Khan: Lancer, Kheshig, Archer, Khan, one King, and eight
//!   Khan soldiers on the 7th rank.
//! * **Piece movement.** Lancer — knight *move*, rook *capture*. Archer — knight
//!   *move*, bishop *capture*. Kheshig — King + Knight (16 squares). Khan — knight
//!   *move*, king *capture* (one step). Khan soldier — forward half-knight *move*
//!   (four leaps), straight-forward Wazir *capture* (one step); no double step, no
//!   en passant.
//! * **Promotion.** A White pawn promotes to a Knight/Bishop/Rook/Queen; a Black
//!   Khan soldier reaching the first rank **must** promote to a Khan.
//! * **Flag win.** White wins on reaching the last rank, Black the first; a node
//!   whose side to move's opponent already stands on its goal rank is terminal.
//!   Stalemate is a loss for the stalemated side.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_khans -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Khans};

/// The Khan's Chess starting FEN in mcr's dialect, confirmed against FSF's
/// `UCI_Variant khans` / `position startpos`.
const STARTPOS: &str = "fwy=tkywf/=s=s=s=s=s=s=s=s/8/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1";

/// A tactical position exercising every Khan piece at once: a Black Khan (`=t`,
/// king-capture), Lancer (`f`, rook-capture), soldier (`=s`), Kheshig (`w`,
/// King+Knight) and Archer (`y`, bishop-capture) loose in the centre against two
/// White pawns.
const TACTIC: &str = "4k3/8/3=t4/2f1=s3/2P1P3/3w1y2/8/4K3 b - - 0 1";

/// A soldier-promotion race: two Black Khan soldiers a short hop from the first
/// rank, each forced to promote to a Khan on a knight-leap that lands there.
const SOLDIER_PROMO: &str = "4k3/8/8/8/8/8/2=s1=s3/4K3 b - - 0 1";

/// A king-flag race: both kings a short walk from their goal ranks, so several
/// lines end by **flag win** (a king reaching its goal rank), terminating perft
/// descent exactly as FSF does.
const FLAG_RACE: &str = "8/4K3/8/8/8/8/4k3/8 w - - 0 1";

/// A developed middlegame: the Black Khan army with advanced soldiers and a moved
/// Lancer, against a White knight development — exercising the asymmetric mid-game.
const DEVELOPED: &str = "f1y=tkywf/1=s=s=s=s=s1=s/2=s5/8/2P1P3/5N2/PP1P1PPP/RNBQKB1R b KQ - 0 1";

/// A White-pawn-promotion position: a White pawn one step from the last rank with
/// a Black Khan (`=t`) and soldier (`=s`) on the board — White promotes to
/// N/B/R/Q while the Khan pieces move.
const WHITE_PROMO: &str = "4k3/P7/8/2=t5/8/3=s4/8/4K3 w - - 0 1";

/// A capture-heavy position: a Black Khan (`=t`) surrounded by White pawns it
/// captures one-step (its `cK` set), a Black soldier (`=s`) capturing the White
/// pawn straight ahead (`cfW`) and forced to promote on a leap to the first rank.
const CAPTURES: &str = "4k3/8/8/3=t4/2PPP3/3=s4/3P4/4K3 b - - 0 1";

/// Asserts the generic Khan's Chess perft equals each pinned `(depth, nodes)`
/// count. Every number here also matched FSF khans `go perft` on the same FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Khans::from_fen(fen).expect("valid Khan's Chess FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Khan's Chess perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 20), (2, 760), (3, 16912), (4, 667056)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 16477807), (6, 666829564)]);
}

// -- Khan-piece tactic (FSF-confirmed) --------------------------------------

#[test]
fn tactic_cheap() {
    check(TACTIC, &[(1, 40), (2, 58), (3, 2329), (4, 6518)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn tactic_deep() {
    check(TACTIC, &[(5, 251760), (6, 1053310)]);
}

// -- Forced soldier promotion to a Khan (FSF-confirmed) ---------------------

#[test]
fn soldier_promo_cheap() {
    check(SOLDIER_PROMO, &[(1, 9), (2, 36), (3, 364), (4, 1981)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn soldier_promo_deep() {
    check(SOLDIER_PROMO, &[(5, 20596), (6, 127425)]);
}

// -- Developed middlegame (FSF-confirmed) -----------------------------------

#[test]
fn developed_cheap() {
    check(DEVELOPED, &[(1, 36), (2, 970), (3, 35204)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn developed_deep() {
    check(DEVELOPED, &[(4, 1022851), (5, 37466710)]);
}

// -- White pawn promotion (FSF-confirmed) -----------------------------------

#[test]
fn white_promo_cheap() {
    check(WHITE_PROMO, &[(1, 8), (2, 99), (3, 966), (4, 13194)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn white_promo_deep() {
    check(WHITE_PROMO, &[(5, 156947)]);
}

// -- Khan / soldier captures + promotion (FSF-confirmed) --------------------

#[test]
fn captures_cheap() {
    check(CAPTURES, &[(1, 20), (2, 147), (3, 2470), (4, 18906)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn captures_deep() {
    check(CAPTURES, &[(5, 299782), (6, 2430580)]);
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
    let pos = Khans::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    let reparsed = Khans::from_fen(STARTPOS).expect("startpos FEN parses");
    assert_eq!(reparsed.to_fen(), STARTPOS);
}
