//! Capablanca-Random (`caparandom`, 10x8 / `u128`) perft validation on the generic
//! engine (issue #377) — the Capablanca army shuffled on the back rank with
//! Chess960-style castling on arbitrary king/rook start files.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Caparandom::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant caparandom`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position (its chancellor `e`/`E` spelled `c`/`C` in the FSF
//! dialect). The `compare-fairy/` differential fuzzer re-runs the head-to-head on
//! demand; this test pins the FSF-confirmed numbers so a regression is caught even
//! without FSF present.
//!
//! ## Confirmed startpos (FSF `caparandom` `startpos`)
//!
//! ```text
//! FSF dialect: rnabqkbcnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBCNR w JAja - 0 1
//! mcr dialect: rnabqkbenr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBENR w JAja - 0 1
//! ```
//!
//! The castling field uses **file letters** (`JAja`) — the a-file (`A`/`a`) and
//! j-file (`J`/`j`) castling rooks — the Shredder form FSF emits for `caparandom`
//! and mcr's [`Caparandom`] matches.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_caparandom -- --include-ignored`.

use mcr::geometry::{perft as gperft, Cap10x8, Caparandom, WideMoveKind};

/// Asserts the generic Caparandom perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `caparandom` `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Caparandom::from_fen(fen).expect("valid Caparandom FEN");
    // The FEN round-trips (Shredder castling field preserved).
    assert_eq!(pos.to_fen(), fen, "Caparandom FEN round trip for {fen}");
    for &(depth, expected) in cases {
        let got = gperft::<Cap10x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Caparandom perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Startpos (the canonical Capablanca array; `JAja` file-letter rights) --------

/// The FSF-confirmed Caparandom startpos (mcr dialect, chancellor `e`).
const STARTPOS: &str = "rnabqkbenr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBENR w JAja - 0 1";

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 28), (2, 784), (3, 25228)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    check(STARTPOS, &[(4, 805128)]);
}

// -- Shuffled full-army starts (rooks off the a/j files) -------------------------

/// King on c1 with **adjacent** rooks b1/d1 (rights `DBdb`), Archbishop a1,
/// Chancellor j1: pins the arbitrary-file castling-rights notation and geometry.
const SHUF_B: &str = "arkrbqnbne/pppppppppp/10/10/10/10/PPPPPPPPPP/ARKRBQNBNE w DBdb - 0 1";

/// King d1, rooks b1/f1 (rights `FBfb`), bishops on opposite colours.
const SHUF_C: &str = "nrbkqrnbae/pppppppppp/10/10/10/10/PPPPPPPPPP/NRBKQRNBAE w FBfb - 0 1";

/// King e1, rooks b1/i1 (rights `IBib`), Chancellor a1, Archbishop j1.
const SHUF_D: &str = "ernbkqbnra/pppppppppp/10/10/10/10/PPPPPPPPPP/ERNBKQBNRA w IBib - 0 1";

#[test]
fn shuffled_full_cheap() {
    check(SHUF_B, &[(2, 676), (3, 19931)]);
    check(SHUF_C, &[(2, 676), (3, 20056)]);
    check(SHUF_D, &[(2, 676), (3, 19992)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn shuffled_full_deep() {
    check(SHUF_B, &[(4, 584320)]);
    check(SHUF_C, &[(4, 589822)]);
    check(SHUF_D, &[(4, 586637)]);
}

// -- Castling edge cases (cleared back ranks) ------------------------------------

/// King c1 with adjacent rooks b1/d1 (rights `DBdb`), back rank otherwise clear.
/// Only the **kingside** castle is legal: the queenside castle would move the b1
/// rook to d1, which is occupied by the (kingside) d1 rook — the king itself
/// stays on c1 (the queenside king destination), so the block is on the rook path.
const ADJ_QUEENSIDE: &str = "1rkr6/pppppppppp/10/10/10/10/PPPPPPPPPP/1RKR6 w DBdb - 0 1";

/// King h1 with adjacent kingside rook i1 and queenside rook b1 (rights `IBib`).
const ADJ_KINGSIDE: &str = "1r5kr1/pppppppppp/10/10/10/10/PPPPPPPPPP/1R5KR1 w IBib - 0 1";

/// King e1, rooks b1/i1 (rights `IBib`), both castles available on a clear rank.
const SPREAD: &str = "1r2k3r1/pppppppppp/10/10/10/10/PPPPPPPPPP/1R2K3R1 w IBib - 0 1";

#[test]
fn castle_edges_cheap() {
    check(ADJ_QUEENSIDE, &[(1, 28), (2, 784), (3, 22044)]);
    check(ADJ_KINGSIDE, &[(1, 30), (2, 900), (3, 26588)]);
    check(SPREAD, &[(1, 31), (2, 961), (3, 29148)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn castle_edges_deep() {
    check(SPREAD, &[(4, 884030)]);
}

/// The adjacency position offers exactly one castle — kingside — landing the king
/// on i1 (file 8) with the rook on h1 (file 7), matching FSF's Capablanca castle
/// geometry; the queenside castle is suppressed (rook destination d1 occupied).
#[test]
fn adjacent_rook_offers_only_kingside_castle() {
    let pos = Caparandom::from_fen(ADJ_QUEENSIDE).expect("valid");
    let castles: Vec<WideMoveKind> = pos
        .legal_moves()
        .into_iter()
        .map(|m| m.kind())
        .filter(|k| {
            matches!(
                k,
                WideMoveKind::CastleKingside | WideMoveKind::CastleQueenside
            )
        })
        .collect();
    assert_eq!(castles, [WideMoveKind::CastleKingside]);

    let mv = pos
        .legal_moves()
        .into_iter()
        .find(|m| m.kind() == WideMoveKind::CastleKingside)
        .expect("a kingside castle");
    // king c1 -> i1 (file 8); the move renders king-to-destination in mcr's dialect.
    assert_eq!(mv.to_uci::<Cap10x8>(), "c1i1");
    let after = pos.play(&mv);
    // Resulting rank 1: rook b1 stays, king i1, rook h1 -> `1R5RK1`.
    assert_eq!(
        after.to_fen(),
        "1rkr6/pppppppppp/10/10/10/10/PPPPPPPPPP/1R5RK1 b db - 1 1"
    );
}
