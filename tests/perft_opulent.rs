//! Opulent chess (10x10 / `u128`) perft validation on the generic engine
//! (issue #375) — an Omega-family 10x10 variant on the [`Grand10x10`] geometry,
//! adding the Wizard (Camel + Ferz) and Lion (Ferz + Dabbaba + Threeleaper)
//! leapers and an **augmented Knight** (Knight + Wazir) to a Grand-style army.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mce::geometry::Opulent::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant opulent`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position. The `compare-fairy/` harness re-runs that
//! head-to-head on demand (`compare-fairy/src/opulent.rs`); this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `opulent_variant()` (`startFen`):
//!
//! ```text
//! FSF dialect: rw6wr/clbnqknbla/pppppppppp/10/10/10/10/PPPPPPPPPP/CLBNQKNBLA/RW6WR w - - 0 1
//! mce dialect: r**w6**wr/e**yb**zqk**zb**ya/pppppppppp/10/10/10/10/PPPPPPPPPP/E**YB**ZQK**ZB**YA/R**W6**WR w - - 0 1
//! ```
//!
//! The dialects differ only in the fairy letters: FSF's chancellor `c`/`C`
//! (Rook+Knight) is mce's [`WideRole::Elephant`](mce::geometry::WideRole::Elephant)
//! `e`/`E`; FSF's lion `l`/`L` (Ferz+Dabbaba+Threeleaper) is mce's
//! [`WideRole::OpulentLion`](mce::geometry::WideRole::OpulentLion) second-bank token
//! `**y`/`**Y`; FSF's knight `n`/`N` — an **augmented** Knight+Wazir, not the plain
//! knight — is mce's [`WideRole::OpulentKnight`](mce::geometry::WideRole::OpulentKnight)
//! `**z`/`**Z`; FSF's wizard `w`/`W` (Camel+Ferz) is mce's
//! [`WideRole::Wizard`](mce::geometry::WideRole::Wizard) `**w`/`**W`; the archbishop
//! `a`/`A` (Bishop+Knight, mce [`WideRole::Hawk`](mce::geometry::WideRole::Hawk)) is
//! spelled identically in both.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_opulent -- --include-ignored`.

use mce::geometry::{perft as gperft, Grand10x10, Opulent};

/// The Opulent starting FEN (mce dialect), confirmed against Fairy-Stockfish's
/// `UCI_Variant opulent`.
const STARTPOS: &str =
    "r**w6**wr/e**yb**zqk**zb**ya/pppppppppp/10/10/10/10/PPPPPPPPPP/E**YB**ZQK**ZB**YA/R**W6**WR w - - 0 1";

/// A quiet midgame, white to move: each side has advanced its e-pawn (white to e4,
/// black to e6 in `a..j` files) and traded no material. Reached from the startpos by
/// `e3e5 e8e6` and confirmed move-for-move by FSF (mce dialect: chancellor `e`, lion
/// `**y`, wizard `**w`, augmented knight `**z`).
const MID: &str =
    "r**w6**wr/e**yb**zqk**zb**ya/pppp1ppppp/10/4p5/4P5/10/PPPP1PPPPP/E**YB**ZQK**ZB**YA/R**W6**WR w - - 0 2";

/// A promotion position, white to move: a lone white pawn on e9 sits inside the
/// far three-rank promotion zone with an empty board (so it may promote to any of
/// the eight army roles — the captured-type limit is slack here) and is forced to
/// promote on the last rank e10. Kings are tucked in the corners. Exercises the
/// three-rank zone and the full Opulent promotion vocabulary (Wizard and Lion
/// included).
const PROMO: &str = "k9/4P5/10/10/10/10/10/10/10/K9 w - - 0 1";

/// Asserts the generic Opulent perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `opulent` `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Opulent::from_fen(fen).expect("valid Opulent FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Grand10x10, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Opulent perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 50), (2, 2500), (3, 133829)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(4, 7147971), (5, 402780823)]);
}

// -- Midgame (FSF-confirmed) ------------------------------------------------

#[test]
fn midgame_cheap() {
    check(MID, &[(1, 52), (2, 2705), (3, 150474)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_deep() {
    check(MID, &[(4, 8344752)]);
}

// -- Promotion (FSF-confirmed) ----------------------------------------------

#[test]
fn promotion_cheap() {
    check(PROMO, &[(1, 11), (2, 28), (3, 432)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn promotion_deep() {
    check(PROMO, &[(4, 2099)]);
}
