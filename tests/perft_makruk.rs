//! Makruk (Thai chess) perft validation on the generic engine (issue #168).
//!
//! The node counts below are pinned **and cross-checked against
//! Fairy-Stockfish** (FSF, `UCI_Variant makruk`): every `(depth, nodes)` pair
//! here was produced identically by `mcr::geometry::Makruk::perft` and by FSF's
//! `go perft` on the byte-identical FEN. The `compare-fairy/` harness re-runs
//! that head-to-head on demand (see `compare-fairy/src/main.rs --makruk`); this
//! test pins the confirmed numbers so a regression is caught without FSF
//! present.
//!
//! Confirmed Makruk starting FEN (from FSF `position startpos`):
//!   `rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w - - 0 1`
//!
//! The cheap layers run as ordinary tests; the deep layers are `#[ignore]`d so
//! `cargo test` stays fast — run them with
//! `cargo test --release --test perft_makruk -- --include-ignored`.

use mcr::geometry::{
    perft as gperft, Bitboard, Chess8x8, Geometry, Makruk, Square, WidePiece, WideRole,
};
use mcr::Color;

/// The Makruk starting FEN, confirmed byte-for-byte against Fairy-Stockfish's
/// `UCI_Variant makruk` / `position startpos`.
const STARTPOS: &str = "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w - - 0 1";

/// A midgame position: an edge pawn pushed, a king-pawn advanced, black to move.
const MID1: &str = "rnsmksnr/8/1ppppppp/p7/4P3/PPPP1PPP/8/RNSKMSNR b - - 0 2";

/// A midgame position with an open centre and both knights developed.
const MID2: &str = "r1smks1r/3n4/ppp1pppp/3p4/3P4/PPP1PPPP/4N3/R1SKMS1R w - - 0 4";

/// Asserts the generic Makruk perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF makruk `go perft` on the same FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Makruk::from_fen(fen).expect("valid Makruk FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Makruk perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(
        STARTPOS,
        &[(1, 23), (2, 529), (3, 12012), (4, 273026), (5, 6223994)],
    );
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_depth6() {
    // FSF makruk `go perft 6` on the startpos: 142_078_049.
    check(STARTPOS, &[(6, 142078049)]);
}

// -- Midgame 1 (FSF-confirmed) ----------------------------------------------

#[test]
fn mid1_cheap() {
    check(
        MID1,
        &[(1, 25), (2, 576), (3, 14290), (4, 329789), (5, 8211263)],
    );
}

// -- Midgame 2 (FSF-confirmed) ----------------------------------------------

#[test]
fn mid2_cheap() {
    check(
        MID2,
        &[(1, 22), (2, 508), (3, 11565), (4, 275266), (5, 6481679)],
    );
}

// -- Rule-level self-checks (independent of FSF) ----------------------------

/// The starting array round-trips through FEN and matches the confirmed string.
#[test]
fn startpos_fen_round_trips() {
    let pos = Makruk::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    assert_eq!(pos.turn(), Color::White);
    // The opening move count: 23 (FSF-confirmed perft(1)).
    assert_eq!(pos.legal_move_count(), 23);
    // No castling rights in Makruk.
    assert!(pos.castling().is_empty());
    assert!(pos.ep_square().is_none());

    // Piece placement: kings face each other (white d1, black e8); each Met
    // sits beside its own king. (file 3 = d, file 4 = e.)
    let board = pos.board();
    assert_eq!(
        board.piece_at(Square::<Chess8x8>::from_file_rank(3, 0).unwrap()),
        Some(WidePiece::new(Color::White, WideRole::King)),
    );
    assert_eq!(
        board.piece_at(Square::<Chess8x8>::from_file_rank(4, 0).unwrap()),
        Some(WidePiece::new(Color::White, WideRole::Met)),
    );
    assert_eq!(
        board.piece_at(Square::<Chess8x8>::from_file_rank(4, 7).unwrap()),
        Some(WidePiece::new(Color::Black, WideRole::King)),
    );
    assert_eq!(
        board.piece_at(Square::<Chess8x8>::from_file_rank(3, 7).unwrap()),
        Some(WidePiece::new(Color::Black, WideRole::Met)),
    );
    // Khon (silver) flanks the Met/King pair; pawns on the third/sixth ranks.
    assert_eq!(board.pieces(Color::White, WideRole::Silver).count(), 2);
    assert_eq!(board.pieces(Color::White, WideRole::Pawn).count(), 8);
}

/// The Met moves as a ferz: exactly the four diagonal one-steps from a central
/// square, color-independent.
#[test]
fn met_moves_as_ferz() {
    // A lone white Met on d4 (file 3, rank 3) with kings off in the corners.
    let fen = "7k/8/8/8/3M4/8/8/K7 w - - 0 1";
    let pos = Makruk::from_fen(fen).expect("valid");
    let met_sq = Square::<Chess8x8>::from_file_rank(3, 3).unwrap();
    let from = pos
        .legal_moves()
        .into_iter()
        .filter(|m| m.from::<Chess8x8>() == met_sq)
        .count();
    // Four diagonals, all empty and not adjacent to the enemy king.
    assert_eq!(from, 4);
}

/// The Khon moves as a silver general: four diagonals plus one straight
/// *forward* step — five from an open central square, and the forward step is
/// color-relative.
#[test]
fn khon_moves_as_silver_general() {
    // White Khon on d4 (file 3, rank 3): 4 diagonals + 1 straight forward (d5).
    let fen = "7k/8/8/8/3S4/8/8/K7 w - - 0 1";
    let pos = Makruk::from_fen(fen).expect("valid");
    let sq = Square::<Chess8x8>::from_file_rank(3, 3).unwrap();
    let moves: Vec<_> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| m.from::<Chess8x8>() == sq)
        .map(|m| m.to::<Chess8x8>())
        .collect();
    assert_eq!(moves.len(), 5, "silver general has five destinations");
    // The straight-forward square (d5) must be among them; the straight-back
    // (d3) must NOT.
    let d5 = Square::<Chess8x8>::from_file_rank(3, 4).unwrap();
    let d3 = Square::<Chess8x8>::from_file_rank(3, 2).unwrap();
    assert!(moves.contains(&d5), "white silver steps forward to d5");
    assert!(!moves.contains(&d3), "silver does not step straight back");

    // A black Khon's forward is the other way: from d4 it reaches d3, not d5.
    let fen_b = "7k/8/8/8/3s4/8/8/K7 b - - 0 1";
    let pos_b = Makruk::from_fen(fen_b).expect("valid");
    let moves_b: Vec<_> = pos_b
        .legal_moves()
        .into_iter()
        .filter(|m| m.from::<Chess8x8>() == sq)
        .map(|m| m.to::<Chess8x8>())
        .collect();
    assert!(moves_b.contains(&d3), "black silver steps forward to d3");
    assert!(!moves_b.contains(&d5), "black silver does not step to d5");
}

/// A Bia (pawn) makes only a single-step advance — never a double push — so no
/// en-passant target is ever created.
#[test]
fn pawn_has_no_double_push_or_en_passant() {
    let pos = Makruk::startpos();
    // No double-pawn-push move kind appears in the opening list.
    let has_double = pos
        .legal_moves()
        .into_iter()
        .any(|m| matches!(m.kind(), mcr::geometry::WideMoveKind::DoublePawnPush));
    assert!(!has_double, "Makruk pawns never double-push");

    // After a pawn single-step, the resulting position has no ep square.
    let single = pos
        .legal_moves()
        .into_iter()
        .find(|m| m.from::<Chess8x8>().rank() == 2)
        .expect("a pawn push exists");
    let next = pos.play(&single);
    assert!(
        next.ep_square().is_none(),
        "no ep target after a single push"
    );
}

/// A Bia promotes to a Met (only) on reaching the sixth rank (white) / third
/// rank (black).
#[test]
fn pawn_promotes_to_met_only() {
    // White pawn on e5 (file 4, rank 4) one step from the promotion rank (5).
    let fen = "7k/8/8/4P3/8/8/8/K7 w - - 0 1";
    let pos = Makruk::from_fen(fen).expect("valid");
    let promos: Vec<_> = pos
        .legal_moves()
        .into_iter()
        .filter_map(|m| m.promotion())
        .collect();
    // Exactly one promotion move, to a Met.
    assert_eq!(promos, vec![WideRole::Met]);
    // The destination is e6 (rank 5).
    let promo_move = pos
        .legal_moves()
        .into_iter()
        .find(|m| m.promotion().is_some())
        .unwrap();
    assert_eq!(promo_move.to::<Chess8x8>().rank(), 5);
}

/// A sanity check that the off-board edge handling holds: no Met / Khon attack
/// ever leaves the 8x8 board.
#[test]
fn attacks_stay_on_board() {
    for index in 0..64u8 {
        let sq = Square::<Chess8x8>::new(index);
        for color in Color::ALL {
            for role in [WideRole::Met, WideRole::Silver] {
                let att = <mcr::geometry::MakrukRules as mcr::geometry::WideVariant<Chess8x8>>::role_attacks(
                    role, color, sq, Bitboard::EMPTY,
                );
                assert_eq!(
                    att.0 & !Chess8x8::BOARD_MASK,
                    0,
                    "{role} off-board on {index}"
                );
            }
        }
    }
}
