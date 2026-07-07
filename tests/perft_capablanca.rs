//! Capablanca chess (10x8 / `u128`) perft validation on the generic engine
//! (issue #170) — the first **larger-board** variant, validating the `u128`
//! [`Cap10x8`] path end-to-end.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Capablanca::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant capablanca`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position. The `compare-fairy/` harness re-runs that
//! head-to-head on demand (`compare-fairy/src/capablanca.rs`); this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `capablanca_variant()` (`startFen`, `castlingKingsideFile = FILE_I`,
//! `castlingQueensideFile = FILE_C`):
//!
//! ```text
//! FSF dialect: rnabqkbcnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBCNR w KQkq - 0 1
//! mcr dialect: rnabqkbenr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBENR w KQkq - 0 1
//! ```
//!
//! The two differ only in the chancellor's letter (`c` in FSF, `e` in mcr, mcr's
//! letter for the rook-knight compound [`WideRole::Elephant`]). The archbishop is
//! `a` ([`WideRole::Hawk`]) in both.
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_capablanca -- --include-ignored`.

use mcr::geometry::{
    perft as gperft, Cap10x8, Capablanca, Square, WideMoveKind, WidePiece, WideRole,
};
use mcr::Color;

/// The Capablanca starting FEN (mcr dialect), confirmed against Fairy-Stockfish's
/// `UCI_Variant capablanca`.
const STARTPOS: &str = "rnabqkbenr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBENR w KQkq - 0 1";

/// A castling-rich position: the back ranks are cleared between each king (on the
/// f-file) and both rooks (a/j files), so both sides may castle both ways. It
/// pins the Capablanca castle geometry (king f -> i/c, rook j -> h / a -> d).
const CASTLE: &str = "r4k3r/pppppppppp/10/10/10/10/PPPPPPPPPP/R4K3R w KQkq - 0 1";

/// A midgame position: both sides developed, the archbishop (`a`) and chancellor
/// (`e`) active, knights out, and a pawn structure admitting en passant.
const MID1: &str = "1nabqkben1/p1ppppppp1/1r6r1/1p6p1/3PP5/2N4N2/PPP2PPPPP/R1ABQKBE1R w KQ - 0 5";

/// A promotion position: a lone white pawn one rank from promotion on the 10-wide
/// board, exercising the six-role Capablanca promotion set (Q/R/B/N + Archbishop
/// + Chancellor).
const PROMO: &str = "5k4/4P5/10/10/10/10/10/5K4 w - - 0 1";

/// Asserts the generic Capablanca perft equals each pinned `(depth, nodes)`
/// count. Every number here also matched FSF capablanca `go perft` on the same
/// position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Capablanca::from_fen(fen).expect("valid Capablanca FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Cap10x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Capablanca perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 28), (2, 784), (3, 25228), (4, 805128)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    // FSF capablanca `go perft` on the startpos.
    check(STARTPOS, &[(5, 28741319), (6, 1015802437)]);
}

// -- Castling position (FSF-confirmed) --------------------------------------

#[test]
fn castle_cheap() {
    check(CASTLE, &[(1, 31), (2, 961), (3, 29210), (4, 887784)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn castle_deep() {
    check(CASTLE, &[(5, 26548436)]);
}

// -- Midgame (FSF-confirmed) ------------------------------------------------

#[test]
fn mid1_cheap() {
    check(MID1, &[(1, 47), (2, 1866), (3, 89790)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn mid1_deep() {
    check(MID1, &[(4, 3514971)]);
}

// -- Promotion (FSF-confirmed) ----------------------------------------------

#[test]
fn promo_cheap() {
    check(PROMO, &[(1, 17), (2, 43), (3, 492), (4, 2486)]);
}

// -- Rule-level self-checks (independent of FSF) ----------------------------

/// The starting array round-trips through FEN and matches the confirmed string,
/// with the king on the f-file and the back rank in the Capablanca order.
#[test]
fn startpos_fen_round_trips() {
    let pos = Capablanca::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    assert_eq!(pos.turn(), Color::White);
    // The opening move count: 28 (FSF-confirmed perft(1)).
    assert_eq!(pos.legal_move_count(), 28);
    assert!(pos.ep_square().is_none());
    assert!(pos.castling().has_any(Color::White));

    let board = pos.board();
    // King on the f-file (file 5), rank 1.
    assert_eq!(
        board.king_of(Color::White),
        Square::<Cap10x8>::from_file_rank(5, 0),
    );
    // Back rank a..j: R N A B Q K B C N R (chancellor C = Elephant).
    let back = [
        WideRole::Rook,
        WideRole::Knight,
        WideRole::Hawk, // Archbishop
        WideRole::Bishop,
        WideRole::Queen,
        WideRole::King,
        WideRole::Bishop,
        WideRole::Elephant, // Chancellor
        WideRole::Knight,
        WideRole::Rook,
    ];
    for (file, role) in back.iter().enumerate() {
        let sq = Square::<Cap10x8>::from_file_rank(file as u8, 0).unwrap();
        assert_eq!(
            board.piece_at(sq),
            Some(WidePiece::new(Color::White, *role)),
            "white back-rank file {file}",
        );
    }
    // One archbishop and one chancellor per side; ten pawns.
    assert_eq!(board.pieces(Color::White, WideRole::Hawk).count(), 1);
    assert_eq!(board.pieces(Color::White, WideRole::Elephant).count(), 1);
    assert_eq!(board.pieces(Color::White, WideRole::Pawn).count(), 10);
}

/// The Archbishop moves as Bishop + Knight; the Chancellor as Rook + Knight,
/// confirming the generic compound defaults are the Capablanca pieces.
#[test]
fn compounds_move_as_bishop_knight_and_rook_knight() {
    // A lone white archbishop on e4 (file 4, rank 3); kings tucked in corners.
    let arch = "9k/10/10/10/4A5/10/10/K9 w - - 0 1";
    let pos = Capablanca::from_fen(arch).expect("valid");
    let sq = Square::<Cap10x8>::from_file_rank(4, 3).unwrap();
    let arch_moves = pos
        .legal_moves()
        .into_iter()
        .filter(|m| m.from::<Cap10x8>() == sq)
        .count();
    // Bishop rays on an open 10x8 board from e4 + 8 knight hops (all on board).
    // Diagonals from e4: NE 4, NW 4, SE 3, SW 3 = 14; knight = 8 -> 22.
    assert_eq!(arch_moves, 22, "archbishop = bishop + knight");

    // A lone white chancellor on e4: rook rays + knight.
    let chan = "9k/10/10/10/4E5/10/10/K9 w - - 0 1";
    let pos = Capablanca::from_fen(chan).expect("valid");
    let chan_moves = pos
        .legal_moves()
        .into_iter()
        .filter(|m| m.from::<Cap10x8>() == sq)
        .count();
    // Rook from e4 on a 10x8 board: file e has 7 other ranks; rank 4 has 9 other
    // files = 16; knight = 8 -> 24.
    assert_eq!(chan_moves, 24, "chancellor = rook + knight");
}

/// Castling lands the king and rook on the Capablanca files: kingside king f -> i
/// (file 8) with rook j -> h (file 7); queenside king f -> c (file 2) with rook
/// a -> d (file 3).
#[test]
fn castling_uses_capablanca_files() {
    let pos = Capablanca::from_fen(CASTLE).expect("valid");
    let mut saw_kingside = false;
    let mut saw_queenside = false;
    for mv in pos.legal_moves() {
        match mv.kind() {
            WideMoveKind::CastleKingside => {
                saw_kingside = true;
                assert_eq!(mv.to_uci::<Cap10x8>(), "f1i1");
                let next = pos.play(&mv);
                assert_eq!(
                    next.board().king_of(Color::White),
                    Square::<Cap10x8>::from_file_rank(8, 0),
                );
                assert_eq!(
                    next.board()
                        .piece_at(Square::<Cap10x8>::from_file_rank(7, 0).unwrap()),
                    Some(WidePiece::new(Color::White, WideRole::Rook)),
                );
            }
            WideMoveKind::CastleQueenside => {
                saw_queenside = true;
                assert_eq!(mv.to_uci::<Cap10x8>(), "f1c1");
                let next = pos.play(&mv);
                assert_eq!(
                    next.board().king_of(Color::White),
                    Square::<Cap10x8>::from_file_rank(2, 0),
                );
                assert_eq!(
                    next.board()
                        .piece_at(Square::<Cap10x8>::from_file_rank(3, 0).unwrap()),
                    Some(WidePiece::new(Color::White, WideRole::Rook)),
                );
            }
            _ => {}
        }
    }
    assert!(saw_kingside, "kingside castle available");
    assert!(saw_queenside, "queenside castle available");
}

/// A pawn promotes to any of the six Capablanca roles — including Archbishop and
/// Chancellor — on the last rank.
#[test]
fn pawn_promotes_to_six_roles() {
    let pos = Capablanca::from_fen(PROMO).expect("valid");
    let mut promo_roles: Vec<WideRole> = pos
        .legal_moves()
        .into_iter()
        .filter_map(|m| m.promotion())
        .collect();
    promo_roles.sort();
    promo_roles.dedup();
    let mut want = vec![
        WideRole::Knight,
        WideRole::Bishop,
        WideRole::Rook,
        WideRole::Queen,
        WideRole::Hawk,
        WideRole::Elephant,
    ];
    want.sort();
    assert_eq!(
        promo_roles, want,
        "all six promotion roles, incl. compounds"
    );
}

/// A double pawn push from the second rank sets an en-passant target, exactly as
/// standard chess; the generic double-push/ep machinery works on the wide board.
#[test]
fn pawn_double_push_sets_en_passant() {
    let pos = Capablanca::startpos();
    let dbl = pos
        .legal_moves()
        .into_iter()
        .find(|m| matches!(m.kind(), WideMoveKind::DoublePawnPush))
        .expect("a double pawn push exists at the start");
    let next = pos.play(&dbl);
    assert!(
        next.ep_square().is_some(),
        "a double push creates an en-passant target",
    );
}
