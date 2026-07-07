//! Nightrider chess (8x8) perft validation on the generic engine — standard chess
//! with the knights replaced by **Nightriders** (FSF `nightrider`, Betza `NN`): a
//! knight that may *ride*, repeating its leap in the same knight-direction over
//! empty squares until blocked, capturing the first piece on each ray. Everything
//! else is standard chess (pawns, king, castling, en passant, `q r b n` promotion —
//! where `n` is the Nightrider).
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Nightrider` perft and by Fairy-Stockfish (FSF,
//! `UCI_Variant nightrider`, a built-in) running `go perft` on the byte-identical
//! position. The corpus deliberately exercises the features unique to a
//! riding-leaper's king safety — which mcr routes through the per-move full-verify
//! path (`WideVariant::needs_full_verify`) because the Nightrider rides
//! **knight-rays**, not board lines:
//!
//! * **long rides** off the home squares (startpos already differs from chess:
//!   perft(1) = 24, not 20);
//! * **rides blocked** by an intervening piece and **captures at the end of a ride**;
//! * a **pin** along a knight-ray (a piece frozen between its king and a Nightrider);
//! * a **check** answered by **interposing** on an intermediate knight-ray landing
//!   square — the move the line-based interposition mask cannot see.
//!
//! The `compare-fairy/` harness re-runs the head-to-head on demand
//! (`compare-fairy/src/nightrider.rs`); this test pins the FSF-confirmed numbers so
//! a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! ```text
//! FSF dialect: rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! mcr dialect: r****nbqkb****nr/pppppppp/8/8/8/8/PPPPPPPP/R****NBQKB****NR w KQkq - 0 1
//! ```
//!
//! In FSF the back rank's `n` is the Nightrider. mcr already names `n` the standard
//! Knight, and every single-letter base plus the `*` / `**` / `=` / `***` overflow
//! banks are exhausted, so the Nightrider takes the fifth-tier **overflow** token
//! `****n` (recycling the FSF mnemonic `n`, distinct by the `****` prefix), turning
//! the standard back rank `r n b q k b n r` into `r ****n b q k b ****n r`. The two
//! are the same position; `compare-fairy/` translates `****n → n` when driving FSF.
//! Both sides have full castling rights.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_nightrider -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Nightrider};

/// The Nightrider starting FEN in mcr's dialect, confirmed against FSF's
/// `UCI_Variant nightrider` / `position startpos`.
const STARTPOS: &str = "r****nbqkb****nr/pppppppp/8/8/8/8/PPPPPPPP/R****NBQKB****NR w KQkq - 0 1";

/// The start position with Black to move — symmetric, so its shallow counts mirror
/// White's.
const STARTPOS_BLACK: &str =
    "r****nbqkb****nr/pppppppp/8/8/8/8/PPPPPPPP/R****NBQKB****NR b KQkq - 0 1";

/// A lone White Nightrider on d4 with both bare kings — long open-board rides in
/// every knight direction, no obstruction.
const OPEN_RIDES: &str = "4k3/8/8/8/3****N4/8/8/4K3 w - - 0 1";

/// A **pin** along a knight-ray: the White king on e1, a White rook on d3, and a
/// Black Nightrider on c5 (the ray e1-d3-c5). The rook is frozen — only the five
/// king steps are legal (perft(1) = 5), which the line-based pin machinery could
/// not enforce for a knight-ray.
const PIN: &str = "4k3/8/8/2****n5/8/3R4/8/4K3 w - - 0 1";

/// A **check answered by interposition** along a knight-ray: the White king on a1
/// is checked by a Black Nightrider on c5 (ray a1-b3-c5); a White rook on h3 can
/// block on b3. Legal replies are the three king escapes plus Rh3-b3
/// (perft(1) = 4) — the interposition the line-based check mask cannot see.
const INTERPOSE: &str = "4k3/8/8/2****n5/8/7R/8/K7 w - - 0 1";

/// A castling-rights middlegame with a White Nightrider on d4: exercises rides that
/// end in captures / blocks together with castling, the double pawn step, and en
/// passant on the same tree (perft(1) = 33).
const CASTLING: &str = "r3k2r/pppp1ppp/8/8/3****N4/8/PPPP1PPP/R3K2R w KQkq - 0 1";

/// Asserts the generic Nightrider perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `nightrider go perft` on the same
/// (byte-identical) FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Nightrider::from_fen(fen).expect("valid Nightrider FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Nightrider perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position, White to move (FSF-confirmed) --------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 24), (2, 576), (3, 15586), (4, 419019)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 12273896)]);
}

// -- Start position, Black to move (FSF-confirmed, mirrors White) ------------

#[test]
fn startpos_black_cheap() {
    check(
        STARTPOS_BLACK,
        &[(1, 24), (2, 576), (3, 15586), (4, 419019)],
    );
}

// -- Long open-board rides (FSF-confirmed) ----------------------------------

#[test]
fn open_rides_cheap() {
    check(OPEN_RIDES, &[(1, 17), (2, 67), (3, 1052), (4, 6046)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn open_rides_deep() {
    check(OPEN_RIDES, &[(5, 94520)]);
}

// -- Pin along a knight-ray (FSF-confirmed) ---------------------------------

#[test]
fn pin_cheap() {
    check(PIN, &[(1, 5), (2, 70), (3, 1150), (4, 14732)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn pin_deep() {
    check(PIN, &[(5, 243871)]);
}

// -- Check answered by interposition on a knight-ray (FSF-confirmed) --------

#[test]
fn interpose_cheap() {
    check(INTERPOSE, &[(1, 4), (2, 67), (3, 1059), (4, 14490)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn interpose_deep() {
    check(INTERPOSE, &[(5, 242559)]);
}

// -- Rides + castling + en passant on one tree (FSF-confirmed) ---------------

#[test]
fn castling_cheap() {
    check(CASTLING, &[(1, 33), (2, 731), (3, 23517), (4, 509391)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn castling_deep() {
    check(CASTLING, &[(5, 15965207)]);
}

// -- The starting FEN round-trips through mcr's FEN I/O ----------------------

#[test]
fn startpos_fen_round_trips() {
    let pos = Nightrider::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    let reparsed = Nightrider::from_fen(STARTPOS).expect("startpos FEN parses");
    assert_eq!(reparsed.to_fen(), STARTPOS);
}
