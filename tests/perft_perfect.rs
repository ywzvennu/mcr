//! Perfect chess (8x8) perft validation on the generic engine — standard chess
//! with the **Chancellor** (R+N), **Archbishop** (B+N), and **Amazon** (Q+N) added
//! to the back rank, and the queen-side castle performed with the a-file
//! Chancellor (Fairy-Stockfish `castlingRookPieces |= CHANCELLOR`).
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Perfect::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant perfect`) running `go perft` on the byte-identical position. The
//! `compare-fairy/` differential fuzzer re-runs the head-to-head on demand
//! (`--difffuzz --variant perfect`); this test pins the FSF-confirmed numbers so a
//! regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! ```text
//! FSF dialect: cmqgkbnr/pppppppp/8/8/8/8/PPPPPPPP/CMQGKBNR w KQkq - 0 1
//! mcr dialect: eaq**akbnr/pppppppp/8/8/8/8/PPPPPPPP/EAQ**AKBNR w KQkq - 0 1
//! ```
//!
//! The two are the same position; only the compound letters differ (chancellor
//! `c`->`e`, archbishop `m`->`a`, amazon `g`->`**a`).
//!
//! ## Proving the Chancellor castles on the queen side
//!
//! The `CASTLE` position clears the back rank between the king and both castle
//! pieces (the a1 Chancellor and the h1 Rook) and carries `KQkq`, so **both**
//! castles are generated. Its counts match FSF square-for-square, which proves the
//! queen-side `e1c1` — sliding the a1 **Chancellor** (not a Rook) to d1 — generates
//! identically to Fairy-Stockfish.
//!
//! The deep startpos layers are `#[ignore]`d so `cargo test` stays fast — run them
//! with `cargo test --release --test perft_perfect -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Perfect};

/// The Perfect chess starting FEN (mcr dialect), confirmed against FSF's
/// `UCI_Variant perfect`. Chancellor `e`/`E`, archbishop `a`/`A`, amazon `**a`/`**A`.
const STARTPOS: &str = "eaq**akbnr/pppppppp/8/8/8/8/PPPPPPPP/EAQ**AKBNR w KQkq - 0 1";

/// Both castles available: the back rank cleared between the e1 king and the a1
/// Chancellor / h1 Rook, `KQkq` present. Queen-side `e1c1` castles with the
/// **Chancellor** — the count matching FSF proves that castle generates.
const CASTLE: &str = "e3k2r/pppppppp/8/8/8/8/PPPPPPPP/E3K2R w KQkq - 0 1";

/// A developed midgame: both amazons out (c3 / c6), knights on f3 / f6, bishops on
/// c4 / c5, both sides still holding `KQkq`. Exercises the three compounds' slides
/// and leaps together.
const MIDGAME: &str = "eaq1k2r/pppp1ppp/2**a2n2/2b1p3/2B1P3/2**A2N2/PPPP1PPP/EAQ1K2R w KQkq - 6 5";

/// A pawn one step from promotion (c7): its subtree spans all seven promotion
/// targets (Amazon, Chancellor, Archbishop, Queen, Rook, Bishop, Knight).
const PROMO: &str = "4k3/2P5/8/8/8/8/8/4K3 w - - 0 1";

/// Asserts the generic Perfect chess perft equals each pinned `(depth, nodes)`
/// count. Every number here also matched FSF `perfect go perft` on the same
/// position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Perfect::from_fen(fen).expect("valid Perfect chess FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Perfect perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 23), (2, 529), (3, 15082), (4, 423107)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 13914103), (6, 448993901)]);
}

#[test]
fn castle_with_chancellor_on_the_queen_side() {
    check(CASTLE, &[(1, 26), (2, 676), (3, 17500), (4, 452658)]);
}

#[test]
fn midgame_cheap() {
    check(MIDGAME, &[(1, 46), (2, 2019), (3, 88551)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_deep() {
    check(MIDGAME, &[(4, 3849148)]);
}

#[test]
fn promotion_reaches_all_seven_targets() {
    check(PROMO, &[(1, 12), (2, 39), (3, 593), (4, 3011)]);
}
