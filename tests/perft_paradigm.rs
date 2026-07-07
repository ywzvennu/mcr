//! Paradigm chess (8x8) perft validation on the generic engine — standard chess
//! with **both bishops replaced by a Bishop + Xiangqi-Horse compound** (FSF
//! `paradigm`: `remove_piece(BISHOP)` + `add_piece(CUSTOM_PIECE_1, 'b', "BnN")`).
//! The Betza `BnN` is a Bishop slide plus a **lame/hobbled Knight** (`nN`, FSF's
//! `n` being the lame-leaper flag — the Xiangqi Horse), so on an open board the
//! piece reaches its bishop diagonals **and** all eight knight squares, each leap
//! blocked when its "leg" (the orthogonal step toward the leap's long axis) is
//! occupied. Everything else is standard chess (pawns, king, castling, en passant,
//! `q b r n` promotion — where `b` is the Bishop-Horse, never an ordinary Bishop).
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Paradigm` perft and by Fairy-Stockfish (FSF,
//! `UCI_Variant paradigm`, a built-in) running `go perft` on the byte-identical
//! position. The corpus exercises the Bishop-Horse's **bishop slide** and its
//! **hobbled horse leaps** together, and — because the lame Horse's king-safety
//! breaks the generic line-based machinery — mcr routes the variant through the
//! per-move full-verify path (`WideVariant::needs_full_verify`):
//!
//! * **startpos** (both colours) — identical to standard chess at the root
//!   (perft(1) = 20; the home Bishop-Horses are hemmed in and every horse leg is
//!   hobbled by an adjacent piece);
//! * a **lone open-board Bishop-Horse** — 13 diagonal squares + 8 knight leaps;
//! * a **horse check answered by a leg-block** — the defender interposes on the
//!   horse's leg, a square off every king line the line-based check mask cannot
//!   offer (perft(1) = 3);
//! * a **castling middlegame** with four active Bishop-Horses firing bishop slides,
//!   horse leaps, blocks, and end-of-slide captures on one tree.
//!
//! The `compare-fairy/` harness re-runs the head-to-head on demand
//! (`compare-fairy/src/paradigm.rs`); this test pins the FSF-confirmed numbers so a
//! regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! ```text
//! FSF dialect: rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! mcr dialect: rn****xqk****xnr/pppppppp/8/8/8/8/PPPPPPPP/RN****XQK****XNR w KQkq - 0 1
//! ```
//!
//! In FSF the back rank's `b` is the Bishop-Horse. mcr already names `b` the
//! standard Bishop, and every single-letter base plus the `*` / `**` / `=` / `***`
//! overflow banks are exhausted, so the Bishop-Horse takes the fifth-tier
//! **overflow** token `****x` (the free base `x`, distinct by the `****` prefix),
//! turning the standard back rank `r n b q k b n r` into `r n ****x q k ****x n r`.
//! The two are the same position; `compare-fairy/` translates `****x → b` when
//! driving FSF. Both sides have full castling rights.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_paradigm -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Paradigm};

/// The Paradigm starting FEN in mcr's dialect, confirmed against FSF's
/// `UCI_Variant paradigm` / `position startpos`.
const STARTPOS: &str = "rn****xqk****xnr/pppppppp/8/8/8/8/PPPPPPPP/RN****XQK****XNR w KQkq - 0 1";

/// The start position with Black to move — the position is mirror-symmetric, so its
/// counts equal White's.
const STARTPOS_BLACK: &str =
    "rn****xqk****xnr/pppppppp/8/8/8/8/PPPPPPPP/RN****XQK****XNR b KQkq - 0 1";

/// A lone White Bishop-Horse on d4 with both bare kings — the un-obstructed piece
/// reaches its 13 bishop diagonal squares **and** all 8 knight squares (the Horse
/// un-hobbled): perft(1) = 21 Bishop-Horse moves + 5 king moves = 26.
const OPEN: &str = "4k3/8/8/8/3****X4/8/8/4K3 w - - 0 1";

/// A **horse check answered by hobbling its leg**: the Black king on e8 is checked
/// by a White Bishop-Horse on d6 (the (1,2) leap, leg d7); a Black rook on a7 can
/// block on d7 to hobble the horse. Legal replies are the two king steps (d8, d7)
/// plus Ra7-d7 (perft(1) = 3) — the leg-block the line-based check mask cannot see.
const LEG_BLOCK: &str = "4k3/r7/3****X4/8/8/8/8/4K3 b - - 0 1";

/// A castling-rights middlegame with four active Bishop-Horses (White on c4 / d5,
/// Black on c5 / d4) on open central diagonals and within horse-leap range:
/// exercises bishop slides that end in captures / blocks together with horse leaps,
/// castling, the double pawn step, and en passant on the same tree (perft(1) = 38).
const MIDGAME: &str =
    "rn2k2r/ppp1qppp/3p1n2/2****x****Xp3/2****X****xP3/3P1N2/PPP1QPPP/RN2K2R w KQkq - 0 1";

/// Asserts the generic Paradigm perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `paradigm go perft` on the same
/// (byte-identical) FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Paradigm::from_fen(fen).expect("valid Paradigm FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Paradigm perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position, White to move (FSF-confirmed) --------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 20), (2, 400), (3, 9062), (4, 204459)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 5217019)]);
}

// -- Start position, Black to move (FSF-confirmed, mirrors White) ------------

#[test]
fn startpos_black_cheap() {
    check(STARTPOS_BLACK, &[(1, 20), (2, 400), (3, 9062), (4, 204459)]);
}

// -- Lone open-board Bishop-Horse: diagonals + hobbled horse leaps (FSF) -----

#[test]
fn open_cheap() {
    check(OPEN, &[(1, 26), (2, 107), (3, 2280)]);
}

// -- Horse check answered by a leg-block (FSF-confirmed) --------------------

#[test]
fn leg_block_cheap() {
    check(LEG_BLOCK, &[(1, 3), (2, 68), (3, 858)]);
}

// -- Middlegame: bishop slides + horse leaps + castling + en passant (FSF) --

#[test]
fn midgame_cheap() {
    check(MIDGAME, &[(1, 38), (2, 1375), (3, 51110)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_deep() {
    check(MIDGAME, &[(4, 1924570)]);
}

// -- The starting FEN round-trips through mcr's FEN I/O ----------------------

#[test]
fn startpos_fen_round_trips() {
    let pos = Paradigm::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    let reparsed = Paradigm::from_fen(STARTPOS).expect("startpos FEN parses");
    assert_eq!(reparsed.to_fen(), STARTPOS);
}
