//! Spartan chess (8x8) perft validation on the generic engine (issue #181) — the
//! variant exercising **asymmetric armies**, **two royal kings + duple check**,
//! and the **Berolina Hoplite** pawn.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mce::geometry::Spartan::perft` and by Fairy-Stockfish (FSF, `UCI_Variant
//! spartan`) running `go perft` on the byte-identical position — the FSF divide
//! matches mce's move-for-move, including the Lieutenant's move-only sideways
//! step, the Berolina Hoplite's jumping double advance, multi-king / duple-check
//! legality (a king may be left en prise while another survives), and Hoplite
//! promotion (to King only while the side has a single king). The
//! `compare-fairy/` harness re-runs that head-to-head on demand
//! (`compare-fairy/src/spartan.rs`); this test pins the FSF-confirmed numbers so
//! a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant spartan`, `position startpos`) renders the start as
//!
//! ```text
//! lgkcckwl/hhhhhhhh/8/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1
//! ```
//!
//! with FSF's Spartan letters `l g k c w h` (Lieutenant, General, King, Captain,
//! Warlord, Hoplite). mce uses the same board but its own role letters — the
//! Lieutenant is `t`, the General `d`, the Captain `i`, the Warlord `a` (Hawk),
//! the Hoplite `h` — so its canonical start FEN is
//!
//! ```text
//! tdkiikat/hhhhhhhh/8/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1
//! ```
//!
//! The two are the same position; `compare-fairy/` translates the Spartan letters
//! when driving FSF. Only White (the standard Persian army) has castling rights.
//!
//! ## Confirmed semantics (all pinned move-for-move against FSF)
//!
//! * **Asymmetric armies.** White = standard P/N/B/R/Q/K (the only side with
//!   castling). Black = Spartans: Lieutenant, General, Captain, Warlord (= Hawk,
//!   B+N), two Kings, and Hoplite pawns.
//! * **Piece movement.** Lieutenant — diagonal one-step + diagonal two-jump
//!   (capturing), plus a move-only sideways step (no sideways capture). General —
//!   Rook + Ferz. Captain — Wazir + Dabbaba. Warlord — Bishop + Knight. Hoplite —
//!   diagonal advance (a jumping double from the start rank), straight-forward
//!   capture, no en passant.
//! * **Two kings + duple check.** A side is in check only when *every* king is
//!   attacked at once; otherwise it may move freely, even leaving a king en prise
//!   to be lost while the survivor plays on. Legality is exactly "after the move,
//!   at least one of my kings is unattacked."
//! * **Hoplite promotion.** On the last rank a Hoplite becomes a Lieutenant,
//!   General, Captain, or Warlord — and a King too, but only while its side has a
//!   single king (regaining the lost second one).
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_spartan -- --include-ignored`.

use mce::geometry::{perft as gperft, Chess8x8, Spartan};

/// The Spartan starting FEN in mce's dialect, confirmed against FSF's
/// `UCI_Variant spartan` / `position startpos`.
const STARTPOS: &str = "tdkiikat/hhhhhhhh/8/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1";

/// An opening line after `1. e4 (Hoplite) d7c6` — Hoplites advanced, exercising
/// the Berolina diagonal advance and the asymmetric mid-development tactics.
const OPENING: &str = "tdkiikat/hhh1hhhh/2h5/8/4P3/8/PPPP1PPP/RNBQKBNR w KQ - 0 2";

/// An asymmetric middlegame: White has developed a bishop to c4 and opened lines;
/// Black's Spartan back rank (two kings, Lieutenants, General, Captains, Warlord)
/// is intact behind partly-advanced Hoplites.
const MID_ASYM: &str = "tdkiikat/hhh2hhh/8/8/2B5/8/PPPPPPPP/RN1QKBNR w KQ - 0 1";

/// A **duple-check** position: Black (two kings on c8/f8) is in check from both a
/// Queen (on the c-file) and a Bishop (on the a3–f8 diagonal) at once, so it must
/// leave at least one king unattacked. Six legal replies.
const DUPLE: &str = "2k2k2/8/8/8/2Q5/B7/8/4K3 b - - 0 1";

/// A duple check Black can break by **blocking or capturing** with its General
/// (on f3) — interposing on either checking line resolves the duple to a single
/// (ignorable) check.
const DUPLE_BREAK: &str = "2k2k2/8/8/8/8/5d2/8/2R2R1K b - - 0 1";

/// A two-king position where Black may legally **walk a king into attack**
/// (a Rook covers the b-file) because its other king stays safe — the en-prise
/// king is simply lost if taken, and the survivor plays on.
const KING_WALK: &str = "8/8/8/8/1R6/8/2k2k2/7K b - - 0 1";

/// A Hoplite one step from promotion with Black holding **one** king: it may
/// promote to a Lieutenant, General, Captain, Warlord, **or King** (regaining the
/// second king).
const PROMO_ONE_KING: &str = "k7/8/8/8/8/8/4h3/4K3 b - - 0 1";

/// The same Hoplite promotion with Black holding **two** kings: King is *not* a
/// legal promotion target (only Lieutenant/General/Captain/Warlord).
const PROMO_TWO_KINGS: &str = "k6k/8/8/8/8/8/4h3/4K3 b - - 0 1";

/// A **White** (Persian) pawn one step from promotion. White is the standard
/// army, so it promotes to exactly `N/B/R/Q` — never to a Spartan piece and never
/// to a King. Regression guard for issue #336: the generic engine routes White's
/// promotion through Spartan's `promotion_targets` too, so a colour-blind target
/// set would (wrongly) hand White the four Spartan promotions plus an illegal King
/// (perft(1) here would read 11, not 9). FSF `spartan` confirms the counts below.
const WHITE_PROMO: &str = "k7/4P3/8/8/8/8/8/4K3 w - - 0 1";

/// The exact midgame the #239 differential fuzzer surfaced the White-promotion bug
/// on: a White pawn on e7 with both sides developed. FSF `spartan` `go perft` pins
/// 39 / 1262 / 49801; the pre-fix engine over-counted (perft(1) 41) by giving the
/// White pawn the Spartan promotion set (Warlord/General/Captain/Lieutenant/King).
const WHITE_PROMO_MIDGAME: &str =
    "t1k2kt1/h1h1Pi2/5hh1/1h3hh1/d1P2PP1/3hP1QN/2P1BK1P/R1B3R1 w - - 1 23";

/// Asserts the generic Spartan perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF spartan `go perft` on the same FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Spartan::from_fen(fen).expect("valid Spartan FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Spartan perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 20), (2, 640), (3, 14244), (4, 473282)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 11712515), (6, 406273143)]);
}

// -- Opening line: advanced Hoplites (FSF-confirmed) ------------------------

#[test]
fn opening_cheap() {
    check(OPENING, &[(1, 30), (2, 896), (3, 27867), (4, 869389)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn opening_deep() {
    check(OPENING, &[(5, 28094514)]);
}

// -- Asymmetric middlegame (FSF-confirmed) ----------------------------------

#[test]
fn mid_asym_cheap() {
    check(MID_ASYM, &[(1, 27), (2, 778), (3, 21793), (4, 688062)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn mid_asym_deep() {
    check(MID_ASYM, &[(5, 19976620)]);
}

// -- Two-king / duple-check positions (FSF-confirmed) -----------------------

#[test]
fn duple_check_cheap() {
    // Black is in duple check: only the six replies that free a king are legal.
    check(DUPLE, &[(1, 6), (2, 222), (3, 1964), (4, 66843)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn duple_check_deep() {
    check(DUPLE, &[(5, 653945)]);
}

#[test]
fn duple_break_cheap() {
    // The General can interpose on (or capture down) either checking line.
    check(DUPLE_BREAK, &[(1, 17), (2, 301), (3, 6691), (4, 139767)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn duple_break_deep() {
    check(DUPLE_BREAK, &[(5, 3384300)]);
}

#[test]
fn king_walk_cheap() {
    // A king may step into attack while the other king stays safe.
    check(KING_WALK, &[(1, 16), (2, 216), (3, 3060), (4, 42851)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn king_walk_deep() {
    check(KING_WALK, &[(5, 569457)]);
}

// -- Hoplite promotion (FSF-confirmed) --------------------------------------

#[test]
fn hoplite_promo_one_king() {
    // With one king, a promoting Hoplite may become a King as well.
    check(PROMO_ONE_KING, &[(1, 18), (2, 45), (3, 426), (4, 2160)]);
}

#[test]
fn hoplite_promo_two_kings() {
    // With two kings, King is not a promotion target.
    check(PROMO_TWO_KINGS, &[(2, 54), (3, 694), (4, 3632)]);
}

// -- White (Persian) promotion = standard N/B/R/Q only (issue #336) ----------

#[test]
fn white_pawn_promotes_to_standard_set_only() {
    // White promotes only to N/B/R/Q (four targets, no Spartan piece, no King):
    // perft(1) = 8 pawn promotions + 1 king step. A colour-blind target set would
    // give 10 pawn moves (the four Spartan pieces + an illegal King) plus the king
    // step, i.e. 11.
    check(WHITE_PROMO, &[(1, 9), (2, 25), (3, 311), (4, 1577)]);
}

#[test]
fn white_promotion_midgame_matches_fsf() {
    // The #239 fuzzer repro, pinned to FSF `spartan` go perft.
    check(WHITE_PROMO_MIDGAME, &[(1, 39), (2, 1262), (3, 49801)]);
}

// -- The starting FEN round-trips through mce's FEN I/O ----------------------

#[test]
fn startpos_fen_round_trips() {
    let pos = Spartan::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    let reparsed = Spartan::from_fen(STARTPOS).expect("startpos FEN parses");
    assert_eq!(reparsed.to_fen(), STARTPOS);
}
