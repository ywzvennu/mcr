//! Hoppel-Poppel (8x8) perft validation on the generic engine (issue #225) — the
//! variant in which the **knight** and **bishop** swap their *capture* methods:
//! the knight (FSF `KNIBIS`, Betza `mNcB`) **moves like a knight** but **captures
//! like a bishop**, and the bishop (FSF `BISKNI`, Betza `mBcN`) **moves like a
//! bishop** but **captures like a knight**. Everything else is standard chess
//! (pieces, pawns, castling, en passant, a standard king).
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::HoppelPoppel` perft and by Fairy-Stockfish (FSF,
//! `UCI_Variant hoppelpoppel`, a built-in) running `go perft` on the byte-identical
//! position — the FSF divide matches mcr's move-for-move, including each piece's
//! move/capture split (the Knight-Bishop's bishop-diagonal captures vs its quiet
//! knight jumps; the Bishop-Knight's knight-leap captures vs its quiet bishop
//! slides), the `q r b n` pawn promotion (`b` / `n` being the variant pieces, not
//! the ordinary Bishop / Knight), castling, and en passant. The `compare-fairy/`
//! harness re-runs that head-to-head on demand
//! (`compare-fairy/src/hoppelpoppel.rs`); this test pins the FSF-confirmed numbers
//! so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! Hoppel-Poppel is a FSF **built-in** derived from the standard chess base, so its
//! start FEN is the **standard chess start**:
//!
//! ```text
//! FSF dialect: rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! mcr dialect: r*h*bqk*b*hr/pppppppp/8/8/8/8/PPPPPPPP/R*H*BQK*B*HR w KQkq - 0 1
//! ```
//!
//! In FSF the back rank's `n` / `b` are the redefined pieces. mcr already names `n`
//! the standard Knight and `b` the standard Bishop, so the Hoppel-Poppel pieces
//! take `*`-prefixed **overflow** tokens — `*h` (Knight-Bishop, the "Hoppel"
//! mnemonic) and `*b` (Bishop-Knight, recycling the FSF `BISKNI` letter) — turning
//! the standard back rank `r n b q k b n r` into `r *h *b q k *b *h r`. The two are
//! the same position; `compare-fairy/` translates the tokens when driving FSF
//! (`*h → n`, `*b → b`). Both sides have full castling rights.
//!
//! ## Confirmed semantics (all pinned move-for-move against FSF)
//!
//! * **Knight-Bishop** (`*h`): moves like a knight onto empty squares, captures
//!   only along bishop diagonals.
//! * **Bishop-Knight** (`*b`): moves like a bishop onto empty squares, captures
//!   only by knight leaps.
//! * **Promotion.** A pawn of either colour promotes to a Queen, Rook,
//!   Bishop-Knight (`*b`), or Knight-Bishop (`*h`) — never an ordinary Bishop /
//!   Knight.
//! * Castling, the double pawn step, and en passant are standard.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_hoppelpoppel -- --include-ignored`.

use mcr::geometry::{perft as gperft, Chess8x8, HoppelPoppel};

/// The Hoppel-Poppel starting FEN in mcr's dialect, confirmed against FSF's
/// `UCI_Variant hoppelpoppel` / `position startpos`.
const STARTPOS: &str = "r*h*bqk*b*hr/pppppppp/8/8/8/8/PPPPPPPP/R*H*BQK*B*HR w KQkq - 0 1";

/// The start position with Black to move — symmetric, so its shallow counts mirror
/// White's (knight/bishop captures swap identically for both colours).
const STARTPOS_BLACK: &str = "r*h*bqk*b*hr/pppppppp/8/8/8/8/PPPPPPPP/R*H*BQK*B*HR b KQkq - 0 1";

/// A developed middlegame — White and Black have each played `Nc3` / `...Nc6` and
/// `e4` / `...e5`. The Knight-Bishops (`*h`/`*H`) are developed but, with no diagonal
/// captures yet available, move exactly as ordinary knights, so the tree only
/// diverges from standard chess once a capture appears deeper down.
const MIDGAME_1: &str = "r1*bqk*b*hr/pppp1ppp/2*h5/4p3/4P3/2*H5/PPPP1PPP/R1*BQK*B*HR w KQkq - 0 1";

/// A bishop-and-knight-rich middlegame (an Italian-style position with both
/// Bishop-Knights pinning-by-knight and both Knight-Bishops eyeing diagonals):
/// exercises the distinctive captures heavily — `*B`/`*b` (Bishop-Knight) threats
/// are knight leaps, `*H`/`*h` (Knight-Bishop) threats are bishop diagonals.
const MIDGAME_2: &str =
    "r2qk2r/ppp2ppp/2*hp1*h2/2*b1p1*B1/2*B1P1*b1/2*HP1*H2/PPP2PPP/R2QK2R w KQkq - 0 1";

/// A Sicilian-style middlegame with an exposed Black queen on a5 and a White
/// Bishop-Knight (`*B`) on d3 whose knight-leap capture set covers central squares —
/// exercises checks / pins delivered by the move≠capture pieces.
const MIDGAME_3: &str = "2kr3r/pp1*h1ppp/2p1p*h2/q7/3P4/2*H*BP*H2/PPQ2PPP/2KR3R w - - 0 1";

/// A tactic + promotion position: White pawns on a7 / about to promote, a Black
/// Bishop-Knight (`*b`) on d5 and Knight-Bishop (`*h`) on g7, two White Knight-Bishops
/// (`*H`) on d4 / e2, and a Black passed pawn on b2. Exercises the `q r b n`
/// promotion set (`a8` promotes to Queen / Rook / Bishop-Knight / Knight-Bishop) and
/// the distinctive capture patterns of both piece types at once.
const TACTIC: &str = "4k3/Pp4*h1/8/3*b4/3*H4/8/1p2*H3/4K3 w - - 0 1";

/// Asserts the generic Hoppel-Poppel perft equals each pinned `(depth, nodes)`
/// count. Every number here also matched FSF `hoppelpoppel go perft` on the same
/// (byte-identical) FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = HoppelPoppel::from_fen(fen).expect("valid Hoppel-Poppel FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Hoppel-Poppel perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position, White to move (FSF-confirmed) --------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 20), (2, 400), (3, 9034), (4, 202459)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(5, 5056643), (6, 125120759)]);
}

// -- Start position, Black to move (FSF-confirmed) --------------------------

#[test]
fn startpos_black_cheap() {
    check(STARTPOS_BLACK, &[(1, 20), (2, 400), (3, 9034)]);
}

// -- Developed middlegame (FSF-confirmed) -----------------------------------

#[test]
fn midgame_1_cheap() {
    check(MIDGAME_1, &[(1, 32), (2, 1018), (3, 32815), (4, 1041323)]);
}

// -- Bishop/knight-rich middlegame, distinctive captures (FSF-confirmed) ----

#[test]
fn midgame_2_cheap() {
    check(MIDGAME_2, &[(1, 46), (2, 2052), (3, 89256), (4, 3840231)]);
}

#[test]
fn midgame_3_cheap() {
    check(MIDGAME_3, &[(1, 46), (2, 1979), (3, 87234), (4, 3553994)]);
}

// -- Tactic + `q r b n` promotion (FSF-confirmed) ---------------------------

#[test]
fn tactic_cheap() {
    check(TACTIC, &[(1, 22), (2, 463), (3, 8732), (4, 186494)]);
}

// -- The starting FEN round-trips through mcr's FEN I/O ----------------------

#[test]
fn startpos_fen_round_trips() {
    let pos = HoppelPoppel::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    let reparsed = HoppelPoppel::from_fen(STARTPOS).expect("startpos FEN parses");
    assert_eq!(reparsed.to_fen(), STARTPOS);
}
