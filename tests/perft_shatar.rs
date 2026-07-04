//! Shatar (Mongolian chess) perft validation on the generic engine (issue #229).
//!
//! The node counts below are pinned **and cross-checked against
//! Fairy-Stockfish** (FSF, `UCI_Variant shatar`): every `(depth, nodes)` pair
//! here was produced identically by `mcr::geometry::Shatar::perft` and by FSF's
//! `go perft` on the byte-identical FEN (FSF spells the Bers `j`; mcr spells it
//! `d`, the [`WideRole::General`] letter, since `j` already names the Xiangqi
//! Horse). The `compare-fairy/` harness re-runs that head-to-head on demand (see
//! `compare-fairy/src/main.rs`); this test pins the confirmed numbers so a
//! regression is caught without FSF present.
//!
//! Confirmed Shatar starting FEN (from FSF `position startpos`):
//!   FSF: `rnbjkbnr/ppp1pppp/8/3p4/3P4/8/PPP1PPPP/RNBJKBNR w - - 0 1`
//!   mcr: `rnbdkbnr/ppp1pppp/8/3p4/3P4/8/PPP1PPPP/RNBDKBNR w - - 0 1`
//!
//! The cheap layers run as ordinary tests; the deep layers are `#[ignore]`d so
//! `cargo test` stays fast — run them with
//! `cargo test --release --test perft_shatar -- --include-ignored`.

use mcr::geometry::{
    perft as gperft, Bitboard, Chess8x8, Geometry, Shatar, Square, WideOutcome, WidePiece, WideRole,
};
use mcr::Color;

/// The Shatar starting FEN (mcr dialect), confirmed byte-for-byte against
/// Fairy-Stockfish's `UCI_Variant shatar` / `position startpos`. The centre
/// pawns start pre-advanced (no double step in Shatar).
const STARTPOS: &str = "rnbdkbnr/ppp1pppp/8/3p4/3P4/8/PPP1PPPP/RNBDKBNR w - - 0 1";

/// A Bers-active middlegame (the kiwipete frame with the queen replaced by a
/// Bers, no castling rights — Shatar has no castling).
const MID_BERS: &str = "r3k2r/p1ppdpb1/bn2pnp1/3PN3/1p2P3/2N3p1/PPPBBPPP/R3K2R w - - 0 1";

/// An open middlegame with both Bers on the board and a single pawn each.
const MID_OPEN: &str = "4k3/8/8/3d4/3D4/8/4P3/4K3 w - - 0 1";

/// A position that **exercises the Robado bare-king draw at depth**: Black has
/// only a king and one pawn, so a line that captures the pawn reduces Black to a
/// lone king mid-tree, truncating that subtree to zero (just as FSF does). The
/// counts therefore diverge from a queen-substituted standard-chess tree.
const MID_ROBADO: &str = "4k3/4p3/8/8/8/8/3D4/4K3 w - - 0 1";

/// Asserts the generic Shatar perft equals each pinned `(depth, nodes)` count.
/// Every number here also matched FSF shatar `go perft` on the same FEN.
fn check(fen: &str, cases: &[(u32, u64)]) {
    let pos = Shatar::from_fen(fen).expect("valid Shatar FEN");
    for &(depth, expected) in cases {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "Shatar perft({depth}) for {fen}: expected {expected} (FSF-confirmed), got {got}"
        );
    }
}

// -- Start position (FSF-confirmed) -----------------------------------------

#[test]
fn startpos_cheap() {
    check(
        STARTPOS,
        &[(1, 20), (2, 400), (3, 8426), (4, 177344), (5, 3969485)],
    );
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn startpos_depth6() {
    // FSF shatar `go perft 6` on the startpos: 88_684_255.
    check(STARTPOS, &[(6, 88684255)]);
}

// -- Bers middlegame (FSF-confirmed) ----------------------------------------

#[test]
fn mid_bers_cheap() {
    check(MID_BERS, &[(1, 41), (2, 1701), (3, 65231), (4, 2617661)]);
}

#[test]
#[ignore = "deep perft; run with --release --include-ignored"]
fn mid_bers_depth5() {
    // FSF shatar `go perft 5` on MID_BERS: 97_528_101.
    check(MID_BERS, &[(5, 97528101)]);
}

// -- Open middlegame (FSF-confirmed) ----------------------------------------

#[test]
fn mid_open_cheap() {
    check(MID_OPEN, &[(1, 20), (2, 366), (3, 6676), (4, 119700)]);
}

// -- Robado-exercising middlegame (FSF-confirmed) ---------------------------

#[test]
fn mid_robado_cheap() {
    check(
        MID_ROBADO,
        &[(1, 21), (2, 81), (3, 1667), (4, 9054), (5, 191077)],
    );
}

// -- Rule-level self-checks (independent of FSF) ----------------------------

/// The starting array round-trips through FEN and matches the confirmed string.
#[test]
fn startpos_fen_round_trips() {
    let pos = Shatar::startpos();
    assert_eq!(pos.to_fen(), STARTPOS);
    assert_eq!(pos.turn(), Color::White);
    // The opening move count: 20 (FSF-confirmed perft(1)).
    assert_eq!(pos.legal_move_count(), 20);
    // No castling rights in Shatar; no en-passant target (no double step).
    assert!(pos.castling().is_empty());
    assert!(pos.ep_square().is_none());

    // Piece placement: kings face each other (white e1, black e8); each Bers
    // sits on its own queen's file (file 3 = d) beside the king (file 4 = e).
    let board = pos.board();
    assert_eq!(
        board.piece_at(Square::<Chess8x8>::from_file_rank(4, 0).unwrap()),
        Some(WidePiece::new(Color::White, WideRole::King)),
    );
    assert_eq!(
        board.piece_at(Square::<Chess8x8>::from_file_rank(3, 0).unwrap()),
        Some(WidePiece::new(Color::White, WideRole::General)),
    );
    assert_eq!(
        board.piece_at(Square::<Chess8x8>::from_file_rank(4, 7).unwrap()),
        Some(WidePiece::new(Color::Black, WideRole::King)),
    );
    assert_eq!(
        board.piece_at(Square::<Chess8x8>::from_file_rank(3, 7).unwrap()),
        Some(WidePiece::new(Color::Black, WideRole::General)),
    );
    // One Bers per side; the centre d-pawns start on the fourth/fifth ranks.
    assert_eq!(board.pieces(Color::White, WideRole::General).count(), 1);
    assert_eq!(board.pieces(Color::Black, WideRole::General).count(), 1);
    assert_eq!(board.pieces(Color::White, WideRole::Pawn).count(), 8);
    // White d-pawn is on d4 (file 3, rank 3), not d2.
    assert_eq!(
        board.piece_at(Square::<Chess8x8>::from_file_rank(3, 3).unwrap()),
        Some(WidePiece::new(Color::White, WideRole::Pawn)),
    );
}

/// The Bers moves as Rook + Ferz: from an open central square it reaches every
/// rook square (14 on an empty board) plus the four single diagonal steps — 18
/// destinations, and crucially **not** the long diagonals beyond one step.
#[test]
fn bers_moves_as_rook_plus_ferz() {
    // A lone white Bers on d4 (file 3, rank 3) with kings off in opposite corners
    // and a black pawn so neither side is bare-king (Robado would zero the moves).
    let fen = "7k/7p/8/8/3D4/8/8/K7 w - - 0 1";
    let pos = Shatar::from_fen(fen).expect("valid");
    let bers_sq = Square::<Chess8x8>::from_file_rank(3, 3).unwrap();
    let dests: Vec<_> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| m.from::<Chess8x8>() == bers_sq)
        .map(|m| m.to::<Chess8x8>())
        .collect();
    // 14 rook squares + 4 ferz steps = 18.
    assert_eq!(dests.len(), 18, "Bers has 18 destinations from an open d4");
    // The single diagonal steps are reachable.
    let c5 = Square::<Chess8x8>::from_file_rank(2, 4).unwrap();
    let e3 = Square::<Chess8x8>::from_file_rank(4, 2).unwrap();
    assert!(
        dests.contains(&c5) && dests.contains(&e3),
        "Bers steps one diagonally"
    );
    // But a **two-step** diagonal square (f6) is NOT reachable — the Bers is not
    // a bishop.
    let f6 = Square::<Chess8x8>::from_file_rank(5, 5).unwrap();
    assert!(!dests.contains(&f6), "Bers cannot slide along a diagonal");
}

/// A pawn makes only a single-step advance — never a double push — so no
/// en-passant target is ever created.
#[test]
fn pawn_has_no_double_push_or_en_passant() {
    let pos = Shatar::startpos();
    let has_double = pos
        .legal_moves()
        .into_iter()
        .any(|m| matches!(m.kind(), mcr::geometry::WideMoveKind::DoublePawnPush));
    assert!(!has_double, "Shatar pawns never double-push");

    // After a pawn single-step from the second rank, the result has no ep square.
    let single = pos
        .legal_moves()
        .into_iter()
        .find(|m| m.from::<Chess8x8>().rank() == 1)
        .expect("a second-rank pawn push exists");
    let next = pos.play(&single);
    assert!(
        next.ep_square().is_none(),
        "no ep target after a single push"
    );
}

/// A pawn promotes to a Bers (only) on reaching the last rank.
#[test]
fn pawn_promotes_to_bers_only() {
    // White pawn on a7 one step from the promotion rank; a black rook keeps Black
    // from being a bare king.
    let fen = "7k/P6r/8/8/8/8/8/K7 w - - 0 1";
    let pos = Shatar::from_fen(fen).expect("valid");
    let promos: Vec<_> = pos
        .legal_moves()
        .into_iter()
        .filter_map(|m| m.promotion())
        .collect();
    // Exactly one promotion move, to a Bers (= General).
    assert_eq!(promos, vec![WideRole::General]);
    let promo_move = pos
        .legal_moves()
        .into_iter()
        .find(|m| m.promotion().is_some())
        .unwrap();
    assert_eq!(promo_move.to::<Chess8x8>().rank(), 7);
}

/// The Robado rule: a side reduced to a lone king ends the game in an immediate
/// **draw**, generating **zero** legal moves regardless of whose turn it is.
#[test]
fn bare_king_is_a_robado_draw() {
    // Black is a lone king; White still has pieces. White to move: zero moves.
    let white_to_move = "7k/8/8/8/3D4/8/8/K7 w - - 0 1";
    let pos = Shatar::from_fen(white_to_move).expect("valid");
    assert_eq!(pos.legal_move_count(), 0, "bare-king node has no moves");
    assert_eq!(pos.outcome(), Some(WideOutcome::Draw), "Robado is a draw");
    assert_eq!(gperft::<Chess8x8, _>(&pos, 1), 0);
    assert_eq!(gperft::<Chess8x8, _>(&pos, 2), 0);

    // Same board, black to move: still a terminal draw with no continuation.
    let black_to_move = "7k/8/8/8/3D4/8/8/K7 b - - 0 1";
    let pos_b = Shatar::from_fen(black_to_move).expect("valid");
    assert_eq!(pos_b.legal_move_count(), 0);
    assert_eq!(pos_b.outcome(), Some(WideOutcome::Draw));

    // A position where BOTH sides still have a non-king piece is NOT Robado.
    let armed = "7k/7p/8/8/3D4/8/8/K7 w - - 0 1";
    let pos_armed = Shatar::from_fen(armed).expect("valid");
    assert!(
        pos_armed.legal_move_count() > 0,
        "armed position is playable"
    );
    assert_eq!(pos_armed.outcome(), None);
}

/// A sanity check that the Bers's attack set never leaves the 8x8 board.
#[test]
fn attacks_stay_on_board() {
    for index in 0..64u8 {
        let sq = Square::<Chess8x8>::new(index);
        for color in Color::ALL {
            let att =
                <mcr::geometry::ShatarRules as mcr::geometry::WideVariant<Chess8x8>>::role_attacks(
                    WideRole::General,
                    color,
                    sq,
                    Bitboard::EMPTY,
                );
            assert_eq!(
                att.0 & !Chess8x8::BOARD_MASK,
                0,
                "Bers off-board on {index}"
            );
        }
    }
}
