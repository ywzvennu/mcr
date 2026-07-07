//! New Zealand chess (8x8) perft validation on the generic engine — standard chess
//! with the Rook and Knight replaced by two **capture-swap** pieces (FSF
//! `newzealand`): the **ROOKNI** (Betza `mRcN` — moves like a rook, captures like a
//! knight) and the **KNIROO** (Betza `mNcR` — moves like a knight, captures like a
//! rook, the Orda Lancer). Everything else is standard chess (pawns, king, castling
//! with the ROOKNI, en passant, `q r b n` promotion — where `r` is the ROOKNI and
//! `n` the KNIROO).
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Newzealand` perft and by Fairy-Stockfish (FSF,
//! `UCI_Variant newzealand`, a built-in) running `go perft` on the byte-identical
//! position. The corpus deliberately exercises the move≠capture split of both
//! pieces and its consequences for king safety:
//!
//! * the **startpos** (both colours; perft(1) = 20 — 16 pawns + 4 KNIROO knight-hops,
//!   the ROOKNIs boxed in);
//! * a **ROOKNI knight-capture** (the rook-mover taking by a leap, its file slide
//!   never capturing);
//! * a **KNIROO rook-capture** (the knight-mover taking by a slide);
//! * a **KNIROO pin** along a rook line (a bishop frozen between its king and an
//!   enemy KNIROO);
//! * a **ROOKNI knight-check** (a leaper check, answerable only by a king move or
//!   capturing the checker — never interposition);
//! * a **castling** middlegame where the corner **ROOKNI** is the castle piece.
//!
//! The `compare-fairy/` harness re-runs the head-to-head on demand
//! (`compare-fairy/src/newzealand.rs`); this test pins the FSF-confirmed numbers so a
//! regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! ```text
//! FSF dialect: rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! mcr dialect: ****kfbqkbf****k/pppppppp/8/8/8/8/PPPPPPPP/****KFBQKBF****K w KQkq - 0 1
//! ```
//!
//! In FSF the back rank's `r` is the ROOKNI and `n` the KNIROO, so the FSF FEN reads
//! like standard chess. mcr names `r`/`n` the Rook/Knight; the KNIROO reuses the Orda
//! Lancer (`f`) and the ROOKNI takes the fifth-tier overflow token `****k`, turning
//! the standard back rank `r n b q k b n r` into `****k f b q k b f ****k`. The two
//! are the same position; `compare-fairy/` translates `****k → r`, `f → n` when
//! driving FSF. Both sides have full castling rights.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_newzealand -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Newzealand};

/// The New Zealand starting FEN in mcr's dialect, confirmed against FSF's
/// `UCI_Variant newzealand` / `position startpos`.
const STARTPOS: &str = "****kfbqkbf****k/pppppppp/8/8/8/8/PPPPPPPP/****KFBQKBF****K w KQkq - 0 1";

/// The start position with Black to move — symmetric, so its shallow counts mirror
/// White's.
const STARTPOS_BLACK: &str =
    "****kfbqkbf****k/pppppppp/8/8/8/8/PPPPPPPP/****KFBQKBF****K b KQkq - 0 1";

/// A **ROOKNI knight-capture**: a lone White ROOKNI on d4 with a Black pawn on e6 (a
/// knight-hop away). The ROOKNI takes e6 by the leap while its rook slides stay quiet.
const ROOKNI_CAP: &str = "4k3/8/4p3/8/3****K4/8/8/4K3 w - - 0 1";

/// A **KNIROO rook-capture**: a Black KNIROO on e4 with a White ROOKNI on e2 up the
/// file. Black to move; the KNIROO takes e2 by sliding along the file.
const KNIROO_CAP: &str = "4k3/8/8/8/4f3/8/4****K3/4K3 b - - 0 1";

/// A **KNIROO pin** along a rook line: a Black KNIROO on e8, a White bishop on e4, and
/// the White king on e1 (the file e8-e4-e1). The bishop is frozen — only the five king
/// steps are legal (perft(1) = 5), a pin only a rook-line capturer can create. The
/// Black king sits out of the way on a8.
const KNIROO_PIN: &str = "k3f3/8/8/8/4B3/8/8/4K3 w - - 0 1";

/// A **ROOKNI knight-check**: the Black king on e6 is checked by a White ROOKNI on d4
/// (the leap d4-e6). Being a leaper check it cannot be interposed — only a king move
/// answers it (perft(1) = 7).
const ROOKNI_CHECK: &str = "8/8/4k3/8/3****K4/8/8/4K3 b - - 0 1";

/// A castling middlegame: the corner **ROOKNIs** are the castle pieces on both wings,
/// with pawns filling ranks 2/7 — exercises ROOKNI castling together with the pawn
/// double step and en passant on one tree (perft(1) = 25, two castles included).
const CASTLING: &str = "****k3k2****k/pppppppp/8/8/8/8/PPPPPPPP/****K3K2****K w KQkq - 0 1";

/// Asserts the generic New Zealand perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `newzealand go perft` on the same
/// (byte-identical) FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Newzealand::from_fen(fen).expect("valid New Zealand FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "New Zealand perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position, White to move (FSF-confirmed) --------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 20), (2, 400), (3, 8976), (4, 200310)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 4987426)]);
}

// -- Start position, Black to move (FSF-confirmed, mirrors White) ------------

#[test]
fn startpos_black_cheap() {
    check(STARTPOS_BLACK, &[(1, 20), (2, 400), (3, 8976), (4, 200310)]);
}

// -- ROOKNI knight-capture (FSF-confirmed) ----------------------------------

#[test]
fn rookni_capture_cheap() {
    check(ROOKNI_CAP, &[(1, 20), (2, 113), (3, 2037), (4, 13363)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn rookni_capture_deep() {
    check(ROOKNI_CAP, &[(5, 242756)]);
}

// -- KNIROO rook-capture (FSF-confirmed) ------------------------------------

#[test]
fn kniroo_capture_cheap() {
    check(KNIROO_CAP, &[(1, 14), (2, 145), (3, 1735), (4, 22830)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn kniroo_capture_deep() {
    check(KNIROO_CAP, &[(5, 266996)]);
}

// -- KNIROO pin along a rook line (FSF-confirmed) ---------------------------

#[test]
fn kniroo_pin_cheap() {
    check(KNIROO_PIN, &[(1, 5), (2, 15), (3, 194), (4, 1608)]);
}

// -- ROOKNI knight-check (FSF-confirmed) ------------------------------------

#[test]
fn rookni_check_cheap() {
    check(ROOKNI_CHECK, &[(1, 7), (2, 124), (3, 847), (4, 14910)]);
}

// -- Castling with the ROOKNI on both wings (FSF-confirmed) ------------------

#[test]
fn castling_cheap() {
    check(CASTLING, &[(1, 25), (2, 625), (3, 15206), (4, 369904)]);
}

// -- The starting FEN round-trips through mcr's FEN I/O ----------------------

#[test]
fn startpos_fen_round_trips() {
    let pos = Newzealand::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    let reparsed = Newzealand::from_fen(STARTPOS).expect("startpos FEN parses");
    assert_eq!(reparsed.to_fen(), STARTPOS);
}
