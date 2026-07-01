//! Courier chess (12x8 / `u128`) perft validation on the generic engine
//! (issue #376) — the medieval German widening of chess on a twelve-files by
//! eight-ranks board, validating the [`Courier12x8`] geometry and the
//! short-range Alfil / Ferz / Wazir / Man army.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mce::geometry::Courier::perft` and by Fairy-Stockfish (FSF, `UCI_Variant
//! courier`, built `largeboards=yes`) running `go perft` on the byte-identical
//! position. The `compare-fairy/` differential fuzzer re-runs that head-to-head
//! on demand (`cargo run -- --difffuzz --variant courier`); this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! Each piece was confirmed **empirically** against FSF `go perft` / divide (never
//! by copying FSF source): the Courier `E` is a two-square diagonal **Alfil** (not
//! the sliding Bishop `B`), the Man `M` a non-royal king, the Wazir `W` an
//! orthogonal one-step, and the Ferz `F` a diagonal one-step. Pawns move one
//! square only (no double push, no en passant) and promote to a Ferz; there is no
//! castling.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `courier` built-in `startFen` (note the **non-standard** array — the
//! a/g/l pawns start advanced and a Ferz sits on g3/g6):
//!
//! ```text
//! FSF dialect: rnebmk1wbenr/1ppppp1pppp1/6f5/p5p4p/P5P4P/6F5/1PPPPP1PPPP1/RNEBMK1WBENR w - - 0 1
//! mce dialect: rn*xb*uk1*jb*xnr/1ppppp1pppp1/6m5/p5p4p/P5P4P/6M5/1PPPPP1PPPP1/RN*XB*UK1*JB*XNR w - - 0 1
//! ```
//!
//! mce reuses `b`/`k`/`r`/`n` for the Bishop/King/Rook/Knight but spells the
//! Courier (Alfil) `*x`, the Man (Commoner) `*u`, the Wazir `*j`, and the Ferz
//! (Met) `m`; the `compare-fairy` harness rewrites those to FSF's `e`/`m`/`w`/`f`.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_courier -- --include-ignored`.

use mce::geometry::{perft as gperft, Courier, Courier12x8};

/// The Courier starting FEN (mce dialect), confirmed against Fairy-Stockfish's
/// `UCI_Variant courier`.
const STARTPOS: &str =
    "rn*xb*uk1*jb*xnr/1ppppp1pppp1/6m5/p5p4p/P5P4P/6M5/1PPPPP1PPPP1/RN*XB*UK1*JB*XNR w - - 0 1";

/// A developed midgame, white to move, reached from the startpos by a legal line
/// (b3, c1-e3 Alfil, f3 for White; b6, Nc6, f6 for Black) and confirmed
/// move-for-move by FSF. Exercises the Alfil, Ferz, Man, Wazir, and Bishop in the
/// open alongside pawn captures.
const MID: &str =
    "r1*xb*uk1*jb*xnr/2ppp2pppp1/1pn2pm5/p5p4p/P5P4P/1P2*XPM5/2PPP2PPPP1/RN1B*UK1*JB*XNR w - - 0 4";

/// A promotion position: a lone white pawn one rank from promotion, with a black
/// rook checking down the a-file so both sides stay armed. Exercises the
/// single-role (Ferz) promotion set, including the capture-promotion onto the
/// rook.
const PROMO: &str = "3r7k/1P10/12/12/12/12/12/K11 w - - 0 1";

/// Asserts the generic Courier perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF courier `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Courier::from_fen(fen).expect("valid Courier FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Courier12x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Courier perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 26), (2, 678), (3, 18406), (4, 500337)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 14144849)]);
}

// -- Midgame (FSF-confirmed) ------------------------------------------------

#[test]
fn midgame_cheap() {
    check(MID, &[(1, 30), (2, 966), (3, 29409)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_deep() {
    check(MID, &[(4, 962337)]);
}

// -- Promotion (FSF-confirmed) ----------------------------------------------

#[test]
fn promo_cheap() {
    check(PROMO, &[(1, 4), (2, 79), (3, 475), (4, 9550)]);
}

// -- Rule-level self-check (independent of FSF) -----------------------------

/// The starting array round-trips through FEN in the mce dialect.
#[test]
fn startpos_fen_round_trips() {
    let pos = Courier::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    assert_eq!(pos.legal_move_count(), 26);
    assert!(pos.ep_square().is_none());
}
