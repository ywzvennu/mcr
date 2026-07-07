//! Georgian Chess (8x8) perft validation on the generic engine — the Amazon
//! army (Queen + Knight on the queen's square) with **no castling** and **no en
//! passant**.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Georgian::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant georgian`) running `go perft` on the byte-identical position. The
//! `compare-fairy/` differential fuzzer re-runs the head-to-head on demand
//! (`--difffuzz --variant georgian`); this test pins the FSF-confirmed numbers so
//! a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! ```text
//! FSF dialect: rnbakbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBAKBNR w - - 0 1
//! mcr dialect: rnb**akbnr/pppppppp/8/8/8/8/PPPPPPPP/RNB**AKBNR w - - 0 1
//! ```
//!
//! The two are the same position; the amazon is `a` in FSF and the second-bank
//! overflow `**a` (the [`WideRole::Angel`] Queen + Knight compound) in mcr. The
//! castling field is `-` and no double step ever sets an ep target.
//!
//! ## Proving castling and en passant are absent
//!
//! The startpos already diverges from Amazon Chess only by the removed en
//! passant: `perft(5)` is `9319631` here versus Amazon's `9319911` (the missing
//! ep captures deep in the tree). Two further positions pin the absence
//! **directly**, each fed to FSF and mcr with the *would-be* rights present so a
//! stray castle or ep capture would change the count:
//!
//! * `CASTLE` carries `KQkq` and the cleared-back-rank rook/king geometry that
//!   castles in Amazon Chess; both engines ignore the rights (Georgian's
//!   `castling = false`), so no castle is generated.
//! * `CASTLE_AND_EP` carries **both** `KQkq` and an `e3` en-passant target (Black
//!   to move with a `d4` pawn that would take `d4xe3` in Amazon Chess); both
//!   engines ignore the ep target (Georgian's `enPassantRegion = 0`) and the
//!   castling rights, so neither a castle nor an ep capture is generated — its
//!   node counts equal the same FEN written with `- -`.
//!
//! The deep startpos layer is `#[ignore]`d so `cargo test` stays fast — run it
//! with `cargo test --release --test perft_georgian -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Georgian};

/// The Georgian Chess starting FEN (mcr dialect), confirmed against FSF's
/// `UCI_Variant georgian`. No castling rights; the amazon is `**a`/`**A`.
const STARTPOS: &str = "rnb**akbnr/pppppppp/8/8/8/8/PPPPPPPP/RNB**AKBNR w - - 0 1";

/// A castling-rich position (standard army, no amazon): both back ranks cleared
/// between the king and both rooks, and the FEN **carries `KQkq`** so a would-be
/// castle would show up in the count. Georgian ignores the rights, so no castle
/// is ever generated — the count matches FSF, which likewise never castles.
const CASTLE: &str = "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1";

/// A single midgame FEN carrying **both** a would-be castle and a would-be en
/// passant: `KQkq` rights over the cleared-back-rank rook/king geometry, and an
/// `e3` ep target with Black to move and a `d4` pawn that would take `d4xe3` in
/// Amazon Chess. Georgian ignores both, so its node counts equal the same FEN
/// written with `- -` — proving neither the castle nor the ep capture exists.
const CASTLE_AND_EP: &str = "r3k2r/pppppppp/8/8/3pP3/8/PPPP1PPP/R3K2R b KQkq e3 0 1";

/// Asserts the generic Georgian Chess perft equals each pinned `(depth, nodes)`
/// count. Every number here also matched FSF `georgian go perft` on the same
/// position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Georgian::from_fen(fen).expect("valid Georgian FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Georgian perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 22), (2, 484), (3, 12483), (4, 318185)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    // 9319631 here vs Amazon's 9319911: the difference is exactly the en-passant
    // captures Georgian removes.
    check(STARTPOS, &[(5, 9319631)]);
}

#[test]
fn no_castle_from_a_rights_bearing_fen() {
    check(CASTLE, &[(1, 23), (2, 529), (3, 12035), (4, 273751)]);
}

#[test]
fn no_castle_and_no_en_passant_from_a_would_be_fen() {
    check(CASTLE_AND_EP, &[(1, 24), (2, 528), (3, 12517), (4, 276774)]);
}
