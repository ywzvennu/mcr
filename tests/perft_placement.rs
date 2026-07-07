//! Placement (Pre-Chess, 8x8) perft validation on the generic engine (issue
//! #266) — standard chess preceded by a **deployment phase** in which each side
//! places its eight back-rank pieces onto its own first rank.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Placement::perft` and by Fairy-Stockfish (FSF, `UCI_Variant
//! placement`) running `go perft` on the byte-identical position — the FSF divide
//! matches mcr's move-for-move, including the deployment drops, the bishop
//! opposite-color constraint, and the deployment-conferred castling rights. The
//! `compare-fairy/` harness re-runs that head-to-head on demand
//! (`compare-fairy/src/placement.rs`); this test pins the FSF-confirmed numbers so
//! a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant placement`, `position startpos`) renders the start as
//!
//! ```text
//! 8/pppppppp/8/8/8/8/PPPPPPPP/8[KQRRBBNNkqrrbbnn] w - - 0 1
//! ```
//!
//! mcr uses the same board and `[..]` pocket but writes the pocket in role-index
//! order (Knights, Bishops, Rooks, Queen, King), so its canonical start FEN is
//!
//! ```text
//! 8/pppppppp/8/8/8/8/PPPPPPPP/8[NNBBRRQKnnbbrrqk] w - - 0 1
//! ```
//!
//! The two are the same position; the standard piece letters (`K Q R B N`) are
//! shared with FSF, so `compare-fairy/` drives FSF with mcr's FEN unchanged.
//!
//! ## Confirmed semantics (all pinned move-for-move against FSF)
//!
//! * **Deployment phase.** The eight non-pawn pieces per side start in hand (the
//!   pocket bracket). Players alternate dropping one piece per ply onto an empty
//!   square of their own first rank, with no check filtering, until both pockets
//!   empty. The opening has `40` drops (8 squares × the 5 distinct held roles);
//!   `perft(2) = 40 × 40` since the two sides' first drops do not interact.
//! * **Bishop opposite colors.** Once a side has placed one bishop, its second
//!   bishop may only drop onto a square of the opposite color (the mid-deployment
//!   `perft(1) = 23` counts only the 3 opposite-color bishop squares).
//! * **Deployment-conferred castling.** A king dropped on its e-file gains the
//!   queenside right with an a-file corner rook and the kingside right with an
//!   h-file corner rook, assigned incrementally (FSF renders `KQq` while one side
//!   is still deploying its last rook). After deployment, normal chess — castling,
//!   en passant, promotion — is played.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_placement -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, Placement};

/// The Placement starting FEN in mcr's dialect, confirmed against FSF's
/// `UCI_Variant placement` / `position startpos`.
const STARTPOS: &str = "8/pppppppp/8/8/8/8/PPPPPPPP/8[NNBBRRQKnnbbrrqk] w - - 0 1";

/// A mid-deployment position: three pieces deployed per side (Rook, Knight,
/// Bishop on a1/b1/c1 and a8/b8/c8), white to drop. The Bishop already on the
/// dark c1 square forces white's held bishop to the light squares.
const MID_DEPLOY: &str = "rnb5/pppppppp/8/8/8/8/PPPPPPPP/RNB5[NBRQKnbrqk] w - - 0 4";

/// A mid-deployment position exercising **incremental castling**: white fully
/// deployed (king e1, corner rooks → `KQ`), black with only its kingside rook
/// still in hand (king e8, a8 rook → `q`), so the rights read `KQq`. Black's only
/// legal drop is `R@h8`, after which the position is the standard chess startpos.
const MID_CASTLING: &str = "rnbqkbn1/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[r] b KQq - 0 8";

/// A fully-deployed back rank that is **not** the standard array — `RBNQKNBR`
/// (bishops on the light b-file and dark g-file, opposite colors) with the king on
/// e1 and corner rooks, so both sides keep full `KQkq` castling.
const CUSTOM_ARRAY: &str = "rbnqknbr/pppppppp/8/8/8/8/PPPPPPPP/RBNQKNBR w KQkq - 0 9";

/// A developed middlegame from a non-standard deployment with castling rights and
/// an en-passant target available (white may play `exd6` e.p.).
const DEV_MIDGAME: &str = "rbnqk1br/ppp1pppp/5n2/3pP3/8/2N5/PPPP1PPP/R1BQKBNR w KQkq d6 0 9";

/// Asserts the generic Placement perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF placement `go perft` on the same FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Placement::from_fen(fen).expect("the corpus FEN parses");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Placement perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 40), (2, 1600), (3, 50560)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    // FSF placement `go perft` on the startpos.
    check(STARTPOS, &[(4, 1597696), (5, 38587392)]);
}

#[test]
fn mid_deploy_cheap() {
    check(MID_DEPLOY, &[(1, 23), (2, 529), (3, 7728)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn mid_deploy_deep() {
    check(MID_DEPLOY, &[(4, 112896), (5, 870912)]);
}

#[test]
fn mid_castling_cheap() {
    check(MID_CASTLING, &[(1, 1), (2, 20), (3, 400), (4, 8902)]);
}

#[test]
fn custom_array_cheap() {
    check(CUSTOM_ARRAY, &[(1, 20), (2, 400), (3, 9010)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn custom_array_deep() {
    check(CUSTOM_ARRAY, &[(4, 202002), (5, 5029728)]);
}

#[test]
fn dev_midgame_cheap() {
    check(DEV_MIDGAME, &[(1, 35), (2, 751), (3, 25568)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn dev_midgame_deep() {
    check(DEV_MIDGAME, &[(4, 598896)]);
}

#[test]
fn startpos_fen_round_trips() {
    let pos = Placement::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
}
