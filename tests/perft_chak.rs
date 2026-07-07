//! Chak (9x9 Mayan chess) perft validation on the generic engine (issue #228) —
//! the variant exercising the [`Shogi9x9`](mcr::geometry::Shogi9x9) geometry with
//! six new pieces (Serpent, Quetzal, Shaman, Divine Lord, Soldier, Temple), the
//! **King→Divine-Lord / Soldier→Shaman promotion on reaching one's own half**, the
//! **region-confined** Shaman and Divine Lord, the **eight-direction cannon** (the
//! Quetzal), the strict **pseudo-royal** rule (royals are the King *and* the Divine
//! Lord, neither may be left en prise), and the **temple-square win** (a Divine
//! Lord reaching the enemy temple ends the game).
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Chak` perft and by Fairy-Stockfish (FSF, `UCI_Variant chak`,
//! from its `variants.ini`) running `go perft` on the byte-identical position — the
//! FSF divide matches mcr's move-for-move, including each new piece's movement, the
//! Quetzal's screen-hop captures, the mandatory King/Soldier promotion (triggered
//! when the origin *or* destination is in the far half), the region confinement of
//! the Shaman / Divine Lord, the all-royals-safe pseudo-royal legality, and the
//! flag-win subtree truncation. The `compare-fairy/` harness re-runs that
//! head-to-head on demand (`compare-fairy/src/chak.rs`); this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant chak`, `position startpos`) renders the start as
//!
//! ```text
//! rvsqkjsvr/4o4/p1p1p1p1p/9/9/9/P1P1P1P1P/4O4/RVSJKQSVR w - - 0 1
//! ```
//!
//! with FSF's Chak letters `r v s q k j o p` (Rook, Vulture, Serpent, Quetzal,
//! King, Jaguar, Temple, Soldier; the Shaman `w` and Divine Lord `d` are
//! promotion-only). mcr reuses `r`/`n`/`k` (Rook / Knight=Vulture / King) and `w`
//! (Kheshig=Jaguar), and spells the six new pieces with `*`-prefixed overflow
//! tokens (`*s *q *w *l *p *o`); its canonical start FEN is
//!
//! ```text
//! rn*s*qkw*snr/4*o4/*p1*p1*p1*p1*p/9/9/9/*P1*P1*P1*P1*P/4*O4/RN*SWK*Q*SNR w - - 0 1
//! ```
//!
//! The two are the same position (note the **asymmetric** back ranks: White's
//! Jaguar/Quetzal sit on opposite sides of the King from Black's); `compare-fairy/`
//! translates the tokens when driving FSF (`*s → s`, `*q → q`, `*w → w`, `*l → d`,
//! `*p → p`, `*o → o`, plus the reused `n → v` and `w → j`).
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_chak -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chak, Shogi9x9};

/// The Chak starting FEN in mcr's dialect, confirmed against FSF's
/// `UCI_Variant chak` / `position startpos`.
const STARTPOS: &str =
    "rn*s*qkw*snr/4*o4/*p1*p1*p1*p1*p/9/9/9/*P1*P1*P1*P1*P/4*O4/RN*SWK*Q*SNR w - - 0 1";

/// A King walked to the centre (e5, in White's promotion half): every King move
/// now **promotes to a Divine Lord** (the origin is in the far half), exercising
/// the mandatory origin-in-zone promotion of the King.
const KING_CENTER: &str =
    "rn*s*qkw*snr/4*o4/*p1*p1*p1*p1*p/9/9/4K4/*P1*P1*P1*P1*P/4*O4/RN*S*Q1*Q*SNR w - - 0 1";

/// A developed middlegame: both sides have pushed a c-pawn (Soldier) one and two
/// steps, opening lines for the back-rank pieces. Exercises ordinary development of
/// the Serpent / Quetzal / Jaguar against an intact opposing army.
const MIDGAME: &str =
    "rn*s*qkw*snr/4*o4/*p3*p1*p1*p/2*p6/9/2*P6/*P3*P1*P1*P/4*O4/RN*SWK*Q*SNR w - - 0 1";

/// A White Soldier standing on e5 (its own half): from there a sideways or forward
/// move promotes it to a Shaman, while a backward move is impossible. Black to move,
/// so the promotion is exercised from Black's replies down the tree.
const SOLDIER_ZONE: &str =
    "rn*s*qkw*snr/4*o4/*p1*p1*p1*p1*p/9/4*P4/9/*P1*P1*P3*P/4*O4/RN*SWK*Q*SNR b - - 0 1";

/// A position with the c- and g-file Soldiers advanced for both sides, opening
/// files and diagonals so the **Quetzal** has screens to hop over (its only way to
/// move/capture). Black to move.
const QUETZAL: &str =
    "rn*s*qkw*snr/4*o4/*p1*p1*p1*p1*p/9/9/2*P3*P2/*P1*P1*P1*P1*P/4*O4/RN*SWK*Q*SNR b - - 0 1";

/// A position with a White **Divine Lord** on d6 attacked by a Black Soldier on e7:
/// White has *both* a King (e1) and a Lord, and the strict pseudo-royal rule forces
/// White to resolve the attack on the Lord — only the seven Lord moves are legal.
/// Exercises the all-royals-must-survive legality on an (artificial) two-royal
/// position, matching FSF's `extinctionPseudoRoyal`.
const LORD_PINNED: &str =
    "rn*s*qk1*snr/4*o4/*p1*p1*p1*p1*p/3*L5/9/9/*P1*P1*P1*P1*P/4*O4/RN*S1K*Q*SNR w - - 0 1";

/// A White **Divine Lord** on e7, one step from the Black temple on e8: capturing
/// the temple (`e7e8`) reaches the flag square and **ends the game**, truncating
/// that subtree to a perft leaf exactly as FSF does. Exercises the temple-win.
const TEMPLE_WIN: &str =
    "rn*s*qk1*snr/4*o4/*p1*p1*L1*p1*p/9/9/9/*P1*P1*P1*P1*P/4*O4/RN*S1K*Q*SNR w - - 0 1";

/// Asserts the generic Chak perft equals each pinned `(depth, nodes)` count. Every
/// number here also matched FSF chak `go perft` on the same FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Chak::from_fen(fen).expect("valid Chak FEN");
    // The FEN round-trips through mcr's overflow-token I/O.
    assert_eq!(pos.to_fen(), fen, "Chak FEN round-trips: {fen}");
    for &(depth, expected) in cases {
        let got = gperft::<Shogi9x9, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Chak perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 33), (2, 1092), (3, 37526), (4, 1294328)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 46112201)]);
}

// -- King promotion to the Divine Lord (origin-in-zone, mandatory) ----------

#[test]
fn king_center_cheap() {
    check(KING_CENTER, &[(1, 37), (2, 1219), (3, 46618), (4, 1590477)]);
}

// -- Developed middlegame ---------------------------------------------------

#[test]
fn midgame_cheap() {
    check(MIDGAME, &[(1, 32), (2, 1092), (3, 37772), (4, 1359077)]);
}

// -- Soldier promotion to the Shaman ----------------------------------------

#[test]
fn soldier_zone_cheap() {
    check(
        SOLDIER_ZONE,
        &[(1, 33), (2, 1124), (3, 38719), (4, 1391939)],
    );
}

// -- Quetzal cannon hops ----------------------------------------------------

#[test]
fn quetzal_cheap() {
    check(QUETZAL, &[(1, 33), (2, 1126), (3, 38903), (4, 1414846)]);
}

// -- Pseudo-royal Divine Lord (all royals must survive) ---------------------

#[test]
fn lord_pinned_cheap() {
    check(LORD_PINNED, &[(1, 7), (2, 217), (3, 7091), (4, 217234)]);
}

// -- Temple win (flag-square subtree truncation) ----------------------------

#[test]
fn temple_win_cheap() {
    check(TEMPLE_WIN, &[(1, 37), (2, 971), (3, 33816), (4, 925030)]);
}

// -- FEN round-trip ---------------------------------------------------------

#[test]
fn startpos_fen_round_trips() {
    let pos = Chak::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
}
