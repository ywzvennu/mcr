//! Shinobi (8x8) perft validation on the generic engine (issue #213) — the
//! variant exercising a **fixed-reserve hand with drops**, a **mandatory
//! per-piece promotion zone**, asymmetric clan-vs-standard armies, and a **flag
//! win**.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Shinobi::perft` and by Fairy-Stockfish (FSF, `UCI_Variant
//! shinobi`) running `go perft` on the byte-identical position — the FSF divide
//! matches mcr's move-for-move, including the clan piece movements, the drop
//! reserve, the mandatory promotion of a piece entering (or leaving) the far
//! zone, and the flag-win terminal node. The `compare-fairy/` harness re-runs
//! that head-to-head on demand (`compare-fairy/src/shinobi.rs`); this test pins
//! the FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant shinobi`, `position startpos`) renders the start as
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/LH1CK1HL[LHMMDJ] w kq - 0 1
//! ```
//!
//! with FSF's clan letters `c d h j` (Commoner, Bers, Shogi Knight, Archbishop).
//! mcr uses the same board but its own role letters — the Commoner is `f`, the
//! Bers `d` (= Spartan General, Rook + Ferz), the Shogi Knight `*n` (an overflow
//! role recycling the Knight's `n`), and the Archbishop `a` (= Hawk, Bishop +
//! Knight); the Commoner `*u` (overflow, recycling the Advisor's `u`), the Fers
//! `m` (= Met) and Lance `l` round it out — so its canonical start FEN is
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/L*N1*UK1*NL[L*NMMDA] w kq - 0 1
//! ```
//!
//! The two are the same position; `compare-fairy/` translates the clan letters
//! when driving FSF. Only Black (the standard army) has castling rights.
//!
//! ## Confirmed semantics (all pinned move-for-move against FSF)
//!
//! * **Asymmetric armies.** Black = the standard P/N/B/R/Q/K (the only side with
//!   castling). White = the clan: Lance, Shogi Knight, Commoner, King on the back
//!   rank, standard pawns, and a drop reserve.
//! * **Clan movement.** Commoner — king's eight one-steps, non-royal. Bers —
//!   Rook + Ferz. Archbishop — Bishop + Knight. Fers — one diagonal step. Shogi
//!   Knight — forward-only 2-1 leap. Lance — forward-only rook slider.
//! * **Fixed-reserve hand + drops.** A capture does **not** feed the hand; the
//!   reserve is consumed only by drops. A held piece drops onto an empty square in
//!   the dropping side's own half (ranks 1-4 for White), a Pawn never on the last
//!   rank. Drop-check and drop-mate are legal (no *uchifuzume*).
//! * **Mandatory promotion zone.** The far two ranks (7-8 for White). A Pawn, Fers,
//!   Shogi Knight, or Lance whose move starts or ends in the zone **must** promote:
//!   Pawn → Commoner, Fers → Bishop, Shogi Knight → Knight, Lance → Rook. There is
//!   never a non-promoting alternative on a zone move.
//! * **Flag win.** A king reaching the opponent's back rank (White on rank 8,
//!   Black on rank 1) wins immediately; the resulting node is terminal — the
//!   opponent has zero replies, exactly as FSF's perft counts it.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_shinobi -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Shinobi};

/// The Shinobi starting FEN in mcr's dialect, confirmed against FSF's
/// `UCI_Variant shinobi` / `position startpos`.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/L*N1*UK1*NL[L*NMMDA] w kq - 0 1";

/// An early middlegame after both sides develop: Black has played ...d5 and
/// ...Nc6, White has dropped/advanced a Lance to c4 and developed a Fers and a
/// Shogi Knight; hand `[AM]`. Exercises clan-vs-standard tactics and a depleted
/// reserve.
const MID_A: &str = "r1bqkbnr/ppp2ppp/2n5/3p4/2Lp4/3M*N3/PPPPPPPP/L*N1*UK1*NL[AM] w kq - 0 5";

/// A White Shogi Knight on f5 (rank 5, one forward leap from the rank-7 zone, so
/// its next jump is a forced promotion) with a Fers on d4; hand `[LAM]`. Exercises
/// the mandatory per-piece promotion path.
const MID_B: &str = "r1bqk2r/ppp1bppp/2n1p2n/2p2*N2/3M4/8/PPPPPPPP/L*N1*UK1*NL[LAM] w kq - 0 7";

/// An advanced White Lance on a4 with a pawn on a5, knights and a Fers developed;
/// hand `[LADM]` (Lance, Archbishop, Bers, Fers all droppable). Exercises drops
/// alongside open-board play.
const MID_C: &str = "r1bqk2r/1pppbppp/p1n1pn2/P7/L7/1*NM5/1PPPPPPP/1*N1*UK1*NL[LADM] w kq - 2 6";

/// A kings-only race position (White king e7, Black king e3, White to move): the
/// flagging move `e7e8` is a terminal leaf with zero children, and a position with
/// a king already on its flag rank yields no moves — both pinned against FSF.
const FLAG_RACE: &str = "8/4K3/8/8/8/4k3/8/8[] w - - 0 1";

/// Asserts the generic Shinobi perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF shinobi `go perft` on the same FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Shinobi::from_fen(fen).expect("valid Shinobi FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Shinobi perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 112), (2, 2238), (3, 224144), (4, 4959812)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 445160174)]);
}

/// Regression for the `Shinobi::startpos()` constructor itself (issue #239): it
/// must build the **FSF-confirmed** start, not merely parse a hand-written FEN.
///
/// The latent bug: `SHINOBI_START_PLACEMENT` spelled the clan back rank with bare
/// `Y`/`F`, which parse to the Orda **Archer** / **Lancer** — not the Shogi Knight
/// (`*N`) and Commoner (`*U`). It evaded every test because `perft_shinobi` and the
/// `compare-fairy` harness both drove hand-written, correctly-spelled FENs and
/// never the constructor, so `startpos()` quietly returned a different position
/// (perft 107 / 2138 / 202874) from the pinned FSF-correct one (112 / 2238 /
/// 224144). This test closes the gap by exercising the constructor directly.
#[test]
fn startpos_constructor_matches_fsf_confirmed_start() {
    let start = Shinobi::startpos();
    // The board placement (the clan back rank especially) must equal the
    // FSF-confirmed `STARTPOS` byte for byte. The `[..]` hand holds the same
    // multiset either way but may serialize in a different role order, so compare
    // the placement prefix here and let the perft counts below pin the holdings.
    assert_eq!(
        start.to_fen().split('[').next(),
        STARTPOS.split('[').next(),
        "Shinobi::startpos() board placement must equal the FSF-confirmed start FEN"
    );
    assert!(
        start.to_fen().contains("L*N1*UK1*NL"),
        "the clan back rank must be Lance / Shogi Knight / Commoner / King (`L*N1*UK1*NL`), \
not the Orda Archer/Lancer (`LY1FK1YL`)"
    );
    // The constructor's own perft must hit the FSF-confirmed counts, independent of
    // any FEN round trip.
    for &(depth, expected) in &[(1u32, 112u64), (2, 2238), (3, 224144)] {
        assert_eq!(
            gperft::<Chess8x8, _, _>(&start, depth),
            expected,
            "Shinobi::startpos() perft({depth}) must equal the FSF-confirmed {expected}"
        );
    }
}

// -- Middlegame A: developed, depleted reserve (FSF-confirmed) --------------

#[test]
fn mid_a_cheap() {
    check(MID_A, &[(1, 50), (2, 1927), (3, 82712), (4, 3160671)]);
}

// -- Middlegame B: Shogi Knight near the promotion zone (FSF-confirmed) -----

#[test]
fn mid_b_cheap() {
    check(MID_B, &[(1, 78), (2, 2692), (3, 176397), (4, 6210643)]);
}

// -- Middlegame C: drops available (FSF-confirmed) --------------------------

#[test]
fn mid_c_cheap() {
    check(MID_C, &[(1, 86), (2, 2659), (3, 193211), (4, 6099333)]);
}

// -- Flag-win terminal race (FSF-confirmed) ---------------------------------

#[test]
fn flag_race() {
    // The flagging king move is a leaf: depth 2 = 40, depth 3 = 301 (FSF).
    check(FLAG_RACE, &[(1, 8), (2, 40), (3, 301)]);
    // A king already on its flag rank ends the game: the opponent has no reply.
    let terminal = Shinobi::from_fen("4K3/8/8/8/8/4k3/8/8[] b - - 0 1").expect("valid FEN");
    assert_eq!(
        gperft::<Chess8x8, _, _>(&terminal, 1),
        0,
        "a king on its flag rank is a terminal node with zero children"
    );
    // The same board with the flagged side to move is NOT terminal (it has not yet
    // been the opponent's turn to register the loss): five king moves (FSF).
    let not_yet = Shinobi::from_fen("4K3/8/8/8/8/4k3/8/8[] w - - 0 1").expect("valid FEN");
    assert_eq!(gperft::<Chess8x8, _, _>(&not_yet, 1), 5);
}
