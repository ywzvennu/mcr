//! Ten-Cubed chess (10x10 / `u128`) perft validation on the generic engine
//! (issue #375) — an Omega-family 10x10 variant on the [`Grand10x10`] geometry,
//! adding the Wizard (Camel + Ferz) and Champion (Wazir + Alfil + Dabbaba)
//! leapers to the Grand army.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mce::geometry::Tencubed::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant tencubed`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position. The `compare-fairy/` harness re-runs that
//! head-to-head on demand (`compare-fairy/src/tencubed.rs`); this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `tencubed_variant()` (`startFen`):
//!
//! ```text
//! FSF dialect: 2cwamwc2/1rnbqkbnr1/pppppppppp/10/10/10/10/PPPPPPPPPP/1RNBQKBNR1/2CWAMWC2 w - - 0 1
//! mce dialect: 2**x**wae**w**x2/1rnbqkbnr1/pppppppppp/10/10/10/10/PPPPPPPPPP/1RNBQKBNR1/2**X**WAE**W**X2 w - - 0 1
//! ```
//!
//! The dialects differ only in the fairy letters: FSF's marshal `m`/`M`
//! (Rook+Knight) is mce's [`WideRole::Elephant`](mce::geometry::WideRole::Elephant)
//! `e`/`E`; FSF's champion `c`/`C` (Wazir+Alfil+Dabbaba) is mce's
//! [`WideRole::TencubedChampion`](mce::geometry::WideRole::TencubedChampion) second-bank
//! token `**x`/`**X`; FSF's wizard `w`/`W` (Camel+Ferz) is mce's
//! [`WideRole::Wizard`](mce::geometry::WideRole::Wizard) `**w`/`**W`; the archbishop
//! `a`/`A` (Bishop+Knight, mce [`WideRole::Hawk`](mce::geometry::WideRole::Hawk)) is
//! spelled identically in both.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_tencubed -- --include-ignored`.

use mce::geometry::{perft as gperft, Grand10x10, Tencubed};

/// The Ten-Cubed starting FEN (mce dialect), confirmed against Fairy-Stockfish's
/// `UCI_Variant tencubed`.
const STARTPOS: &str =
    "2**x**wae**w**x2/1rnbqkbnr1/pppppppppp/10/10/10/10/PPPPPPPPPP/1RNBQKBNR1/2**X**WAE**W**X2 w - - 0 1";

/// A quiet midgame, white to move: each side has advanced its e-pawn (white to e4,
/// black to e6 in `a..j` files) and traded no material. Reached from the startpos by
/// `e3e5 e8e6` and confirmed move-for-move by FSF (mce dialect, marshal `e`, champion
/// `**x`, wizard `**w`).
const MID: &str =
    "2**x**wae**w**x2/1rnbqkbnr1/pppp1ppppp/10/4p5/4P5/10/PPPP1PPPPP/1RNBQKBNR1/2**X**WAE**W**X2 w - - 0 2";

/// A promotion position, white to move: a lone white pawn on e9 sits one step from
/// the (single-rank) promotion zone, so it promotes on e10 to a Queen, Marshal, or
/// Archbishop only — never to a Rook, Knight, Bishop, Wizard, or Champion. Kings are
/// tucked in the corners. Exercises the restricted, unrestricted-count promotion set.
const PROMO: &str = "k9/4P5/10/10/10/10/10/10/10/K9 w - - 0 1";

/// Asserts the generic Ten-Cubed perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `tencubed` `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Tencubed::from_fen(fen).expect("valid Ten-Cubed FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Grand10x10, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Ten-Cubed perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 40), (2, 1600), (3, 68230)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(4, 2906895), (5, 131575398)]);
}

// -- Midgame (FSF-confirmed) ------------------------------------------------

#[test]
fn midgame_cheap() {
    check(MID, &[(1, 50), (2, 2497), (3, 129558)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_deep() {
    check(MID, &[(4, 6701100)]);
}

// -- Promotion (FSF-confirmed) ----------------------------------------------

#[test]
fn promotion_cheap() {
    check(PROMO, &[(1, 6), (2, 16), (3, 239)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn promotion_deep() {
    check(PROMO, &[(4, 1132)]);
}
