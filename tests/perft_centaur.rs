//! Centaur Chess (10x8 / `u128`) perft validation on the generic engine
//! (issue #403) — the Capablanca board and castling, but with the
//! Archbishop/Chancellor compounds replaced by two **Centaurs** (King + Knight,
//! mcr [`WideRole::Kheshig`]).
//!
//! Every `(depth, nodes)` pair below was produced **identically** by
//! `mcr::geometry::Centaur::perft` and by Fairy-Stockfish (FSF, the INI
//! `centaur` variant — a `capablanca` descendant with the compounds removed and
//! `centaur = c`, built `largeboards=yes`) running `go perft` on the
//! byte-identical position. The `compare-fairy/` harness re-runs that
//! head-to-head on demand (`compare-fairy/src/difffuzz.rs`); this test pins the
//! FSF-confirmed numbers so a regression is caught even without FSF present.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's INI `centaur` variant (Capablanca `startFen` with the compounds
//! swapped for centaurs, `castlingKingsideFile = FILE_I`,
//! `castlingQueensideFile = FILE_C`):
//!
//! ```text
//! FSF dialect: rcnbqkbncr/pppppppppp/10/10/10/10/PPPPPPPPPP/RCNBQKBNCR w KQkq - 0 1
//! mcr dialect: rwnbqkbnwr/pppppppppp/10/10/10/10/PPPPPPPPPP/RWNBQKBNWR w KQkq - 0 1
//! ```
//!
//! The two differ only in the centaur's letter (`c` in FSF, `w` in mcr, the Orda
//! [`WideRole::Kheshig`] letter).
//!
//! The deep layers are `#[ignore]`d so `cargo test` stays fast — run them with
//! `cargo test --release --test perft_centaur -- --include-ignored`.

use mcr::geometry::{perft as gperft, Cap10x8, Centaur, Square, WideMoveKind, WidePiece, WideRole};
use mcr::Color;

/// The Centaur Chess starting FEN (mcr dialect), confirmed against
/// Fairy-Stockfish's INI `centaur` variant.
const STARTPOS: &str = "rwnbqkbnwr/pppppppppp/10/10/10/10/PPPPPPPPPP/RWNBQKBNWR w KQkq - 0 1";

/// A castling-rich position: the back ranks are cleared between each king (on the
/// f-file) and both rooks (a/j files), so both sides may castle both ways. It
/// pins the Capablanca castle geometry (king f -> i/c, rook j -> h / a -> d).
/// (No centaurs, so byte-identical to the Capablanca castle FEN.)
const CASTLE: &str = "r4k3r/pppppppppp/10/10/10/10/PPPPPPPPPP/R4K3R w KQkq - 0 1";

/// A midgame position (derived from FSF by playing a real move sequence): both
/// sides developed, both centaurs (`w`) out on the c-file, knights on d3/d6, and
/// a f-file pawn advanced.
const MID1: &str = "r2bqkbnwr/pppp1ppppp/2wn6/4p5/4PP4/2WN6/PPPP2PPPP/R2BQKBNWR b KQkq - 0 4";

/// A promotion position: a lone white pawn one rank from promotion on the 10-wide
/// board, exercising the five-role Centaur promotion set (Q/R/B/N + Centaur).
const PROMO: &str = "5k4/4P5/10/10/10/10/10/5K4 w - - 0 1";

/// Asserts the generic Centaur perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF `centaur` `go perft` on the same position.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Centaur::from_fen(fen).expect("valid Centaur FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Cap10x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Centaur perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(STARTPOS, &[(1, 28), (2, 784), (3, 24490), (4, 763180)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_deep() {
    // FSF `centaur` `go perft` on the startpos.
    check(STARTPOS, &[(5, 26221598), (6, 896745154)]);
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
    check(MID1, &[(1, 44), (2, 2451), (3, 112684)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn mid1_deep() {
    check(MID1, &[(4, 6311935)]);
}

// -- Promotion (FSF-confirmed) ----------------------------------------------

#[test]
fn promo_cheap() {
    check(PROMO, &[(1, 15), (2, 40), (3, 425), (4, 2213)]);
}

// -- Rule-level self-checks (independent of FSF) ----------------------------

/// The starting array round-trips through FEN and matches the confirmed string,
/// with the king on the f-file and the back rank in the Centaur order.
#[test]
fn startpos_fen_round_trips() {
    let pos = Centaur::startpos();
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
    // Back rank a..j: R W N B Q K B N W R (centaur W = Kheshig).
    let back = [
        WideRole::Rook,
        WideRole::Kheshig, // Centaur
        WideRole::Knight,
        WideRole::Bishop,
        WideRole::Queen,
        WideRole::King,
        WideRole::Bishop,
        WideRole::Knight,
        WideRole::Kheshig, // Centaur
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
    // Two centaurs per side; ten pawns.
    assert_eq!(board.pieces(Color::White, WideRole::Kheshig).count(), 2);
    assert_eq!(board.pieces(Color::White, WideRole::Pawn).count(), 10);
}

/// The Centaur moves as King + Knight: a lone one on an open interior square
/// reaches all sixteen leaper targets.
#[test]
fn centaur_moves_as_king_plus_knight() {
    // A lone white centaur on e4 (file 4, rank 3); kings tucked in corners.
    let cen = "9k/10/10/10/4W5/10/10/K9 w - - 0 1";
    let pos = Centaur::from_fen(cen).expect("valid");
    let sq = Square::<Cap10x8>::from_file_rank(4, 3).unwrap();
    let cen_moves = pos
        .legal_moves()
        .into_iter()
        .filter(|m| m.from::<Cap10x8>() == sq)
        .count();
    // King 8 + knight 8 (all on board from an interior square) = 16.
    assert_eq!(cen_moves, 16, "centaur = king + knight");
}

/// Castling lands the king and rook on the Capablanca files: kingside king f -> i
/// (file 8) with rook j -> h (file 7); queenside king f -> c (file 2) with rook
/// a -> d (file 3).
#[test]
fn castling_uses_capablanca_files() {
    let pos = Centaur::from_fen(CASTLE).expect("valid");
    let mut saw_kingside = false;
    let mut saw_queenside = false;
    for mv in pos.legal_moves() {
        match mv.kind() {
            WideMoveKind::CastleKingside => {
                saw_kingside = true;
                assert_eq!(mv.to_uci::<Cap10x8>(), "f1i1");
            }
            WideMoveKind::CastleQueenside => {
                saw_queenside = true;
                assert_eq!(mv.to_uci::<Cap10x8>(), "f1c1");
            }
            _ => {}
        }
    }
    assert!(saw_kingside, "kingside castle available");
    assert!(saw_queenside, "queenside castle available");
}

/// A pawn promotes to any of the five Centaur roles — including the Centaur — on
/// the last rank.
#[test]
fn pawn_promotes_to_five_roles() {
    let pos = Centaur::from_fen(PROMO).expect("valid");
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
        WideRole::Kheshig,
    ];
    want.sort();
    assert_eq!(promo_roles, want, "all five promotion roles, incl. Centaur");
}
