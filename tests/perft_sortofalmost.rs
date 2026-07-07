//! Sort-of-almost chess (8x8) perft validation on the generic engine — standard
//! chess with **only White's Queen replaced by a Chancellor** (Rook + Knight);
//! Black keeps its Queen.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Sortofalmost::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant sortofalmost`, an FSF built-in) running `go perft` on the
//! byte-identical position — including White's Chancellor moving as Rook + Knight,
//! Black's ordinary Queen, standard castling, and the asymmetric pawn promotion
//! (White to Chancellor / Rook / Bishop / Knight, Black to Queen / Rook / Bishop /
//! Knight). The `compare-fairy/` harness re-runs that head-to-head on demand
//! (`compare-fairy/src/sortofalmost.rs`); this test pins the FSF-confirmed numbers
//! so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! ```text
//! FSF dialect: rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBCKBNR w KQkq - 0 1
//! mcr dialect: rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBEKBNR w KQkq - 0 1
//! ```
//!
//! The two differ only in White's chancellor letter (`C` in FSF, `E` in mcr, mcr's
//! letter for the rook-knight compound [`WideRole::Elephant`](mcr::geometry::WideRole::Elephant)).
//! Black's back rank is standard chess in both dialects.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_sortofalmost -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Sortofalmost};

/// The Sort-of-almost starting FEN (mcr dialect), confirmed against FSF's
/// `UCI_Variant sortofalmost` / `position startpos`.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBEKBNR w KQkq - 0 1";

/// A both-sides-developed middlegame (White bishop and knight out, castling-ready
/// `RNBEK2R`; Black knight and bishop out) exercising White's Chancellor on d1 and
/// Black's Queen together in a live position.
const MIDGAME: &str = "r1bqk1nr/pppp1ppp/2n5/2b1p3/2B1P3/5N2/PPPP1PPP/RNBEK2R w KQkq - 0 1";

/// A White pawn one step from promotion: it may become a Chancellor / Rook / Bishop
/// / Knight — never a Queen (White has no queen).
const WHITE_PROMO: &str = "4k3/1P6/8/8/8/8/8/4K3 w - - 0 1";

/// A Black pawn one step from promotion: it may become a Queen / Rook / Bishop /
/// Knight — never a Chancellor (Black has no chancellor). The depth-2 count (497)
/// differs from White's mirror (463), pinning the asymmetric promotion sets.
const BLACK_PROMO: &str = "4k3/8/8/8/8/8/1p6/4K3 b - - 0 1";

/// Asserts the generic Sort-of-almost perft equals each pinned `(depth, nodes)`
/// count. Every number here also matched FSF `sortofalmost go perft` on the same
/// position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Sortofalmost::from_fen(fen).expect("valid Sort-of-almost FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Sortofalmost perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 22), (2, 440), (3, 10815), (4, 239279)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 6444471), (6, 157277582)]);
}

// -- Developed middlegame (FSF-confirmed) -----------------------------------

#[test]
fn midgame_cheap() {
    check(MIDGAME, &[(1, 34), (2, 1185), (3, 39396), (4, 1349409)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_deep() {
    check(MIDGAME, &[(5, 45270068)]);
}

// -- Asymmetric promotion (FSF-confirmed) -----------------------------------

#[test]
fn white_promotes_to_chancellor() {
    check(WHITE_PROMO, &[(1, 9), (2, 39), (3, 463)]);
}

#[test]
fn black_promotes_to_queen() {
    check(BLACK_PROMO, &[(1, 9), (2, 40), (3, 497)]);
}
