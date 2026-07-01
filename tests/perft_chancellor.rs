//! Chancellor chess (9x9 / `u128`) perft validation on the generic engine
//! (issue #378) — standard western chess widened to a nine-files by nine-ranks
//! board with a Rook + Knight Chancellor, validating the [`Chess9x9`] geometry.
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mce::geometry::Chancellor::perft` and by Fairy-Stockfish (FSF,
//! `UCI_Variant chancellor`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position. The `compare-fairy/` differential fuzzer re-runs that
//! head-to-head on demand (`cargo run -- --difffuzz --variant chancellor`); this
//! test pins the FSF-confirmed numbers so a regression is caught even without FSF
//! present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `chancellor_variant()` (`startFen`, `castlingKingsideFile = FILE_G`,
//! `castlingQueensideFile = FILE_C`, `doubleStepRegion = Rank2/Rank8`,
//! `promotionRegion = Rank9/Rank1`):
//!
//! ```text
//! FSF dialect: rnbqkcnbr/ppppppppp/9/9/9/9/9/PPPPPPPPP/RNBQKCNBR w KQkq - 0 1
//! mce dialect: rnbqkenbr/ppppppppp/9/9/9/9/9/PPPPPPPPP/RNBQKENBR w KQkq - 0 1
//! ```
//!
//! The two differ only in the chancellor's letter (`c` in FSF, `e` in mce, mce's
//! letter for the rook-knight compound [`WideRole::Elephant`]).
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_chancellor -- --include-ignored`.

use mce::geometry::{
    perft as gperft, Chancellor, Chess9x9, Square, WideMoveKind, WidePiece, WideRole,
};
use mce::Color;

/// The Chancellor starting FEN (mce dialect), confirmed against Fairy-Stockfish's
/// `UCI_Variant chancellor`.
const STARTPOS: &str = "rnbqkenbr/ppppppppp/9/9/9/9/9/PPPPPPPPP/RNBQKENBR w KQkq - 0 1";

/// A castling-rich position: the back ranks are cleared between each king (on the
/// e-file) and both rooks (a/i files), so both sides may castle both ways. It pins
/// the standard-on-9x9 castle geometry (king e -> g/c, rook i -> f / a -> d).
const CASTLE: &str = "r3k3r/9/9/9/9/9/9/9/R3K3R w KQkq - 0 1";

/// A developed midgame, white to move: knights out for both sides, the chancellor
/// still home, and adjacent central pawns admitting captures and en passant at
/// depth. Reached from the startpos by a sequence of legal moves and confirmed
/// move-for-move by FSF.
const MID1: &str = "r1bqke1br/ppp2pppp/2n2n3/3pp4/9/3PP4/2N2N3/PPP2PPPP/R1BQKE1BR w KQkq - 0 5";

/// A promotion position: a lone white pawn one rank from promotion on the 9-wide
/// board, exercising the five-role Chancellor promotion set (Q/R/B/N + Chancellor).
const PROMO: &str = "4k4/P8/9/9/9/9/9/9/4K4 w - - 0 1";

/// Asserts the generic Chancellor perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF chancellor `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Chancellor::from_fen(fen).expect("valid Chancellor FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess9x9, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Chancellor perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 24), (2, 576), (3, 15896), (4, 436656)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    // FSF chancellor `go perft` on the startpos.
    check(STARTPOS, &[(5, 13466196)]);
}

// -- Castling position (FSF-confirmed) --------------------------------------

#[test]
fn castle_cheap() {
    check(CASTLE, &[(1, 29), (2, 713), (3, 19562)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn castle_deep() {
    check(CASTLE, &[(4, 509836)]);
}

// -- Midgame (FSF-confirmed) ------------------------------------------------

#[test]
fn midgame_cheap() {
    check(MID1, &[(1, 38), (2, 1435), (3, 56044)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn midgame_deep() {
    check(MID1, &[(4, 2170556)]);
}

// -- Promotion (FSF-confirmed) ----------------------------------------------

#[test]
fn promo_cheap() {
    check(PROMO, &[(1, 10), (2, 44), (3, 614), (4, 3573)]);
}

// -- Rule-level self-checks (independent of FSF) ----------------------------

/// The starting array round-trips through FEN and matches the confirmed string,
/// with the king on the e-file and the chancellor beside it on the f-file.
#[test]
fn startpos_fen_round_trips() {
    let pos = Chancellor::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    assert_eq!(pos.turn(), Color::White);
    // The opening move count: 24 (FSF-confirmed perft(1)).
    assert_eq!(pos.legal_move_count(), 24);
    assert!(pos.ep_square().is_none());
    assert!(pos.castling().has_any(Color::White));

    let board = pos.board();
    // King on the e-file (file 4), rank 1.
    assert_eq!(
        board.king_of(Color::White),
        Square::<Chess9x9>::from_file_rank(4, 0),
    );
    // Back rank a..i: R N B Q K E N B R (chancellor E = Elephant).
    let back = [
        WideRole::Rook,
        WideRole::Knight,
        WideRole::Bishop,
        WideRole::Queen,
        WideRole::King,
        WideRole::Elephant, // Chancellor
        WideRole::Knight,
        WideRole::Bishop,
        WideRole::Rook,
    ];
    for (file, role) in back.iter().enumerate() {
        let sq = Square::<Chess9x9>::from_file_rank(file as u8, 0).unwrap();
        assert_eq!(
            board.piece_at(sq),
            Some(WidePiece::new(Color::White, *role)),
            "white back-rank file {file}",
        );
    }
    // One chancellor per side; nine pawns.
    assert_eq!(board.pieces(Color::White, WideRole::Elephant).count(), 1);
    assert_eq!(board.pieces(Color::White, WideRole::Pawn).count(), 9);
}

/// The Chancellor moves as Rook + Knight, confirming the generic compound default
/// is the Chancellor-chess piece.
#[test]
fn chancellor_moves_as_rook_knight() {
    // A lone white chancellor on e5 (file 4, rank 4); kings tucked in corners.
    let chan = "8k/9/9/9/4E4/9/9/9/K8 w - - 0 1";
    let pos = Chancellor::from_fen(chan).expect("valid");
    let sq = Square::<Chess9x9>::from_file_rank(4, 4).unwrap();
    let chan_moves = pos
        .legal_moves()
        .into_iter()
        .filter(|m| m.from::<Chess9x9>() == sq)
        .count();
    // Rook from e5 on a 9x9 board: file e has 8 other ranks; rank 5 has 8 other
    // files = 16; knight = 8 -> 24.
    assert_eq!(chan_moves, 24, "chancellor = rook + knight");
}

/// Castling lands the king and rook on the standard files: kingside king e -> g
/// (file 6) with rook i -> f (file 5); queenside king e -> c (file 2) with rook
/// a -> d (file 3).
#[test]
fn castling_uses_standard_files() {
    let pos = Chancellor::from_fen(CASTLE).expect("valid");
    let mut saw_kingside = false;
    let mut saw_queenside = false;
    for mv in pos.legal_moves() {
        match mv.kind() {
            WideMoveKind::CastleKingside => {
                saw_kingside = true;
                assert_eq!(mv.to_uci::<Chess9x9>(), "e1g1");
                let next = pos.play(&mv);
                assert_eq!(
                    next.board().king_of(Color::White),
                    Square::<Chess9x9>::from_file_rank(6, 0),
                );
                assert_eq!(
                    next.board()
                        .piece_at(Square::<Chess9x9>::from_file_rank(5, 0).unwrap()),
                    Some(WidePiece::new(Color::White, WideRole::Rook)),
                );
            }
            WideMoveKind::CastleQueenside => {
                saw_queenside = true;
                assert_eq!(mv.to_uci::<Chess9x9>(), "e1c1");
                let next = pos.play(&mv);
                assert_eq!(
                    next.board().king_of(Color::White),
                    Square::<Chess9x9>::from_file_rank(2, 0),
                );
                assert_eq!(
                    next.board()
                        .piece_at(Square::<Chess9x9>::from_file_rank(3, 0).unwrap()),
                    Some(WidePiece::new(Color::White, WideRole::Rook)),
                );
            }
            _ => {}
        }
    }
    assert!(saw_kingside, "kingside castle available");
    assert!(saw_queenside, "queenside castle available");
}

/// A pawn promotes to any of the five Chancellor-chess roles — including the
/// Chancellor — on the last rank.
#[test]
fn pawn_promotes_to_five_roles() {
    let pos = Chancellor::from_fen(PROMO).expect("valid");
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
        WideRole::Elephant,
    ];
    want.sort();
    assert_eq!(
        promo_roles, want,
        "all five promotion roles, incl. chancellor"
    );
}

/// A double pawn push from the second rank sets an en-passant target, exactly as
/// standard chess; the generic double-push/ep machinery works on the 9x9 board.
#[test]
fn pawn_double_push_sets_en_passant() {
    let pos = Chancellor::startpos();
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
