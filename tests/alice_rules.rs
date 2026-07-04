//! Alice chess rule unit + property tests (issue #276).
//!
//! Alice has no Fairy-Stockfish oracle, so the two-board mechanics are pinned by
//! hand-constructed positions and by invariant/property checks over seeded random
//! playouts (no `rand`, no clock). Together with the independent brute-force perft
//! cross-check in `tests/perft_alice.rs`, this is the no-oracle validation the
//! issue calls for: **rules-validated (no FSF oracle); perft pins hand-derived per
//! the documented Alice ruleset.**

use mcr::geometry::position::{GenericCastling, GenericGating, GenericPlacement, GenericState};
use mcr::geometry::{Alice, Bitboard, Board, Chess8x8, Square, WideMoveKind, WidePiece};
use mcr::geometry::{WideRole, WideVariant};
use mcr::Color;

type Sq = Square<Chess8x8>;

/// A square from file/rank (0-based, rank 0 = white back rank), matching the
/// engine's `index = rank * 8 + file`.
fn sq(file: u8, rank: u8) -> Sq {
    Square::<Chess8x8>::from_file_rank(file, rank).expect("on board")
}

/// Builds an Alice position from explicit placements. Each placement is
/// `(square, color, role, on_plane_b)`. `board_b` is set for the plane-B pieces.
fn build(
    turn: Color,
    castling: GenericCastling,
    ep_square: Option<Sq>,
    pieces: &[(Sq, Color, WideRole, bool)],
) -> Alice {
    let mut board = Board::<Chess8x8>::default();
    let mut board_b = Bitboard::<Chess8x8>::EMPTY;
    for &(s, color, role, plane_b) in pieces {
        board.set_piece(s, WidePiece::new(color, role));
        if plane_b {
            board_b.set(s);
        }
    }
    let state = GenericState {
        turn,
        castling,
        ep_square,
        gating: GenericGating::NONE,
        duck: None,
        placement: GenericPlacement::NONE,
        halfmove_clock: 0,
        fullmove_number: 1,
        consecutive_passes: 0,
        board_b,
    };
    Alice::from_parts(board, state)
}

/// Does the legal-move list contain a move `from -> to` (any kind)?
fn has_move(pos: &Alice, from: Sq, to: Sq) -> bool {
    pos.legal_moves()
        .iter()
        .any(|m| m.from::<Chess8x8>() == from && m.to::<Chess8x8>() == to)
}

// --------------------------------------------------------------------------
// Hand-constructed unit tests for the cross-board mechanics.
// --------------------------------------------------------------------------

/// A piece gives check only from the **same** board as the king.
#[test]
fn cross_board_check_is_same_plane_only() {
    let wk = sq(4, 0); // e1
    let bk = sq(4, 7); // e8 — irrelevant, just needs to exist off the file
    let bk_off = sq(0, 7); // a8
    let rook = sq(4, 6); // e7, down the e-file toward the white king

    // Rook on the king's plane (A): it checks down the open e-file.
    let same = build(
        Color::White,
        GenericCastling::NONE,
        None,
        &[
            (wk, Color::White, WideRole::King, false),
            (bk_off, Color::Black, WideRole::King, false),
            (rook, Color::Black, WideRole::Rook, false),
        ],
    );
    assert!(same.is_check(), "same-plane rook must give check");

    // Same geometry but the rook is on board B: it cannot attack the plane-A king.
    let cross = build(
        Color::White,
        GenericCastling::NONE,
        None,
        &[
            (wk, Color::White, WideRole::King, false),
            (bk_off, Color::Black, WideRole::King, false),
            (rook, Color::Black, WideRole::Rook, true),
        ],
    );
    assert!(
        !cross.is_check(),
        "a rook on the other board must not give check"
    );
    let _ = bk;
}

/// A quiet move is blocked when its transfer square is occupied on the **other**
/// board, but an other-board piece does **not** block the slide on the mover's
/// own board.
#[test]
fn transfer_blocked_by_other_plane_occupancy() {
    let wk = sq(7, 0); // h1
    let bk = sq(7, 7); // h8
    let rook = sq(0, 0); // a1 on board A
    let blocker_b = sq(0, 3); // a4 on board B

    let pos = build(
        Color::White,
        GenericCastling::NONE,
        None,
        &[
            (wk, Color::White, WideRole::King, false),
            (bk, Color::Black, WideRole::King, false),
            (rook, Color::White, WideRole::Rook, false),
            (blocker_b, Color::Black, WideRole::Pawn, true),
        ],
    );
    // a1->a4 transfers to a4 on board B, which is occupied → illegal.
    assert!(
        !has_move(&pos, rook, sq(0, 3)),
        "transfer onto an occupied other-plane square must be illegal"
    );
    // a1->a3 (a3 empty on both planes) is legal.
    assert!(
        has_move(&pos, rook, sq(0, 2)),
        "quiet push to empty a3 legal"
    );
    // The board-B blocker is invisible on board A, so the rook still slides past
    // a4 to a5 and beyond (a5 empty on both planes → legal landing).
    assert!(
        has_move(&pos, rook, sq(0, 4)),
        "other-plane piece must not block the same-plane slide"
    );
}

/// A capture happens on the mover's board, then the mover transfers to the other
/// board (the captured square is vacant there by the one-piece invariant).
#[test]
fn capture_then_transfer() {
    let wk = sq(7, 0);
    let bk = sq(7, 7);
    let rook = sq(0, 0); // a1, board A
    let victim = sq(0, 3); // a4, board A (black pawn)

    let pos = build(
        Color::White,
        GenericCastling::NONE,
        None,
        &[
            (wk, Color::White, WideRole::King, false),
            (bk, Color::Black, WideRole::King, false),
            (rook, Color::White, WideRole::Rook, false),
            (victim, Color::Black, WideRole::Pawn, false),
        ],
    );
    assert!(has_move(&pos, rook, victim), "rook captures the a4 pawn");
    let mv = pos
        .legal_moves()
        .into_iter()
        .find(|m| m.from::<Chess8x8>() == rook && m.to::<Chess8x8>() == victim)
        .expect("the capture is legal");
    let after = pos.play(&mv);
    // The rook now stands on a4 on board B; a1 is empty; the pawn is gone.
    assert_eq!(
        after.board().piece_at(victim),
        Some(WidePiece::new(Color::White, WideRole::Rook))
    );
    assert!(
        after.board_b().contains(victim),
        "mover transferred to board B"
    );
    assert!(after.board().piece_at(rook).is_none(), "origin vacated");
}

/// Castling is permitted: king and rook both transfer to the other board, and the
/// destination squares must be vacant there.
#[test]
fn castling_transfers_both_pieces() {
    let wk = sq(4, 0); // e1
    let bk = sq(4, 7); // e8
    let rook_h = sq(7, 0); // h1
    let rook_a = sq(0, 0); // a1

    let pos = build(
        Color::White,
        GenericCastling::standard::<Chess8x8>(),
        None,
        &[
            (wk, Color::White, WideRole::King, false),
            (bk, Color::Black, WideRole::King, false),
            (rook_h, Color::White, WideRole::Rook, false),
            (rook_a, Color::White, WideRole::Rook, false),
        ],
    );
    // Both castles available (back rank clear, nothing attacks the king's path).
    assert!(
        has_move(&pos, wk, sq(6, 0)),
        "kingside castle e1->g1 available"
    );
    assert!(
        has_move(&pos, wk, sq(2, 0)),
        "queenside castle e1->c1 available"
    );

    let ks = pos
        .legal_moves()
        .into_iter()
        .find(|m| matches!(m.kind(), WideMoveKind::CastleKingside))
        .expect("kingside castle present");
    let after = pos.play(&ks);
    // King to g1 and rook to f1, both now on board B.
    assert_eq!(
        after.board().piece_at(sq(6, 0)),
        Some(WidePiece::new(Color::White, WideRole::King))
    );
    assert_eq!(
        after.board().piece_at(sq(5, 0)),
        Some(WidePiece::new(Color::White, WideRole::Rook))
    );
    assert!(
        after.board_b().contains(sq(6, 0)),
        "king transferred to board B"
    );
    assert!(
        after.board_b().contains(sq(5, 0)),
        "rook transferred to board B"
    );
}

/// Castling is illegal when a destination square is occupied on the other board
/// (the transfer is blocked).
#[test]
fn castling_blocked_by_other_plane_destination() {
    let wk = sq(4, 0);
    let bk = sq(4, 7);
    let rook_h = sq(7, 0);
    let g1_b = sq(6, 0); // king's kingside destination, occupied on board B

    let pos = build(
        Color::White,
        GenericCastling::standard::<Chess8x8>(),
        None,
        &[
            (wk, Color::White, WideRole::King, false),
            (bk, Color::Black, WideRole::King, false),
            (rook_h, Color::White, WideRole::Rook, false),
            (g1_b, Color::Black, WideRole::Pawn, true),
        ],
    );
    assert!(
        !has_move(&pos, wk, sq(6, 0)),
        "kingside castle blocked: g1 occupied on board B"
    );
}

/// The king "may not transfer out of check": a king move whose destination is
/// still attacked on the board it **leaves** is illegal even if it would be safe
/// on the board it lands on.
#[test]
fn king_may_not_transfer_out_of_check() {
    // White king on e1 (board A), black rook on e7 (board A) checking down the
    // e-file. Moving the king to e2 stays on the e-file: attacked on board A.
    let wk = sq(4, 0);
    let rook = sq(4, 6);
    let bk = sq(0, 7);
    let pos = build(
        Color::White,
        GenericCastling::NONE,
        None,
        &[
            (wk, Color::White, WideRole::King, false),
            (rook, Color::Black, WideRole::Rook, false),
            (bk, Color::Black, WideRole::King, false),
        ],
    );
    assert!(pos.is_check());
    // e1->e2 stays on the attacked e-file on board A (the leaving board) → illegal,
    // even though e2 on board B is empty.
    assert!(
        !has_move(&pos, wk, sq(4, 1)),
        "king may not slide along the checking file and transfer away"
    );
    // e1->d1 / e1->f1 leave the file and are legal (d1/f1 unattacked on board A,
    // empty on board B).
    assert!(
        has_move(&pos, wk, sq(3, 0)),
        "step off the file to d1 is legal"
    );
    assert!(
        has_move(&pos, wk, sq(5, 0)),
        "step off the file to f1 is legal"
    );
}

/// A piece transferring onto the king's board may **interpose** to block a check
/// there — the post-move position is what is judged.
#[test]
fn transfer_interpose_blocks_check() {
    // Black king on e8 board A, white rook on e1 board A checks up the e-file.
    // White's other rook on a4 board B can move to e4 (board B) and transfer to
    // e4 board A, interposing on the e-file — but that is White's move; instead
    // test from Black's side: a black piece on board B interposing to save its
    // king. Black bishop on c6 board B moves to e4? Simpler: set it so a black
    // knight on board B can land on e-file board A to block.
    let bk = sq(4, 7); // e8 board A
    let wr = sq(4, 0); // e1 board A, checks up the file
    let wk = sq(0, 0); // a1 board A
    let blocker = sq(3, 5); // d6 board B knight -> e4 (board A) blocks? knight d6->e4 yes
    let pos = build(
        Color::Black,
        GenericCastling::NONE,
        None,
        &[
            (bk, Color::Black, WideRole::King, false),
            (wr, Color::White, WideRole::Rook, false),
            (wk, Color::White, WideRole::King, false),
            (blocker, Color::Black, WideRole::Knight, true),
        ],
    );
    assert!(
        pos.is_check(),
        "black king is checked on board A up the e-file"
    );
    // The knight d6 (board B) -> e4 transfers to e4 on board A, interposing.
    assert!(
        has_move(&pos, blocker, sq(4, 3)),
        "an interposing transfer onto the king's board resolves the check"
    );
}

/// En passant is excluded from Alice: even with an en-passant target set, no
/// en-passant capture is generated.
#[test]
fn en_passant_is_excluded() {
    // White pawn e5 (board A), black pawn d5 (board A), ep target d6 — the
    // standard ep capture e5xd6 would be legal in ordinary chess.
    let wp = sq(4, 4); // e5
    let bp = sq(3, 4); // d5
    let ep = sq(3, 5); // d6
    let wk = sq(4, 0);
    let bk = sq(4, 7);
    let pos = build(
        Color::White,
        GenericCastling::NONE,
        Some(ep),
        &[
            (wp, Color::White, WideRole::Pawn, false),
            (bp, Color::Black, WideRole::Pawn, false),
            (wk, Color::White, WideRole::King, false),
            (bk, Color::Black, WideRole::King, false),
        ],
    );
    assert!(
        !has_move(&pos, wp, ep),
        "Alice excludes en passant: e5xd6 must not be generated"
    );
    assert!(
        pos.legal_moves()
            .iter()
            .all(|m| !matches!(m.kind(), WideMoveKind::EnPassant)),
        "no move of kind EnPassant is ever produced"
    );
    // And a normal push e5->e6 (empty both planes) is still legal.
    assert!(has_move(&pos, wp, sq(4, 5)));
}

/// A double pawn push never leaves a usable en-passant target in Alice.
#[test]
fn double_push_clears_ep_target() {
    let wp = sq(4, 1); // e2
    let wk = sq(0, 0);
    let bk = sq(0, 7);
    let pos = build(
        Color::White,
        GenericCastling::NONE,
        None,
        &[
            (wp, Color::White, WideRole::Pawn, false),
            (wk, Color::White, WideRole::King, false),
            (bk, Color::Black, WideRole::King, false),
        ],
    );
    let mv = pos
        .legal_moves()
        .into_iter()
        .find(|m| matches!(m.kind(), WideMoveKind::DoublePawnPush))
        .expect("e2-e4 double push available");
    let after = pos.play(&mv);
    // The pushed pawn transferred to board B; no en-passant is ever offered.
    assert!(
        after.board_b().contains(sq(4, 3)),
        "pawn transferred to e4 on board B"
    );
    assert!(after
        .legal_moves()
        .iter()
        .all(|m| !matches!(m.kind(), WideMoveKind::EnPassant)));
}

// --------------------------------------------------------------------------
// Property / invariant tests over seeded random playouts.
// --------------------------------------------------------------------------

/// Deterministic splitmix64 — no `rand`, no clock.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// An independent (test-side) plane-restricted king-safety check: is `side`'s
/// king unattacked by enemy pieces on its own board? Naive full scan over the
/// engine's `board()` + `board_b()`.
fn side_king_safe(pos: &Alice, side: Color) -> bool {
    let board = pos.board();
    let bb = pos.board_b();
    let Some(king) = board.king_of(side) else {
        return true;
    };
    let kplane = bb.contains(king);
    let occ_on = |s: Sq, plane: bool| board.is_occupied(s) && bb.contains(s) == plane;
    let (kf, kr) = (king.file() as i32, king.rank() as i32);
    let them = side.opposite();
    let mut s = 0u8;
    while (s as usize) < 64 {
        let from = Square::<Chess8x8>::new(s);
        s += 1;
        let Some(p) = board.piece_at(from) else {
            continue;
        };
        if p.color != them || bb.contains(from) != kplane {
            continue;
        }
        let (ff, fr) = (from.file() as i32, from.rank() as i32);
        let (df, dr) = (kf - ff, kr - fr);
        let (adf, adr) = (df.abs(), dr.abs());
        let ray_clear = |sf: i32, sr: i32| {
            let (mut cf, mut cr) = (ff + sf.signum(), fr + dr.signum());
            let _ = sr;
            let stepf = sf.signum();
            let stepr = dr.signum();
            while (cf, cr) != (kf, kr) {
                if occ_on(sq(cf as u8, cr as u8), kplane) {
                    return false;
                }
                cf += stepf;
                cr += stepr;
            }
            true
        };
        let hit = match p.role {
            WideRole::Pawn => {
                let pdir = if them == Color::White { 1 } else { -1 };
                adf == 1 && dr == pdir
            }
            WideRole::Knight => (adf, adr) == (1, 2) || (adf, adr) == (2, 1),
            WideRole::King => adf <= 1 && adr <= 1 && (adf + adr) != 0,
            WideRole::Bishop => adf == adr && adf != 0 && ray_clear(df, dr),
            WideRole::Rook => (df == 0 || dr == 0) && (adf + adr) != 0 && ray_clear(df, dr),
            WideRole::Queen => {
                ((df == 0 || dr == 0) || (adf == adr && adf != 0))
                    && (adf + adr) != 0
                    && ray_clear(df, dr)
            }
            _ => false,
        };
        if hit {
            return false;
        }
    }
    true
}

#[test]
fn random_playout_invariants() {
    for seed in 0..24u64 {
        let mut pos = Alice::startpos();
        let mut rng = Rng(seed.wrapping_mul(0xD1B5_4A32_D192_ED03).wrapping_add(1));
        for _ply in 0..40 {
            let moves = pos.legal_moves();
            if moves.is_empty() {
                break;
            }
            let mover = pos.turn();
            let mv = moves[(rng.next() % moves.len() as u64) as usize];
            let from = mv.from::<Chess8x8>();
            let to = mv.to::<Chess8x8>();
            let from_plane = pos.board_b().contains(from);
            let is_castle = matches!(
                mv.kind(),
                WideMoveKind::CastleKingside | WideMoveKind::CastleQueenside
            );

            // (c) The destination must be vacant on the plane the mover transfers
            // to (checked before the move, for non-castle moves).
            if !is_castle {
                let target_plane = !from_plane;
                let to_blocked =
                    pos.board().is_occupied(to) && pos.board_b().contains(to) == target_plane;
                assert!(
                    !to_blocked,
                    "seed {seed}: legal move lands on an occupied target plane"
                );
            }

            pos = pos.play(&mv);

            // (a) Invariant: every plane-B square is occupied (board_b ⊆ occupied).
            assert!(
                (pos.board_b() & !pos.board().occupied()).is_empty(),
                "seed {seed}: board_b marks an empty square"
            );
            // (b) The mover transferred to the opposite plane (non-castle).
            if !is_castle {
                assert_eq!(
                    pos.board_b().contains(to),
                    !from_plane,
                    "seed {seed}: mover did not transfer to the other board"
                );
            }
            // (d) The side that just moved is not left in check on its own board.
            assert!(
                side_king_safe(&pos, mover),
                "seed {seed}: a legal move left the mover's king in same-board check"
            );
        }
    }
}

/// Perft internal consistency: the depth-2 node count equals the sum over the
/// root's legal moves of each child's depth-1 count.
#[test]
fn perft_divide_consistency() {
    let pos = Alice::startpos();
    let root = pos.legal_moves();
    let sum: usize = root.iter().map(|m| pos.play(m).legal_moves().len()).sum();
    assert_eq!(sum, 400, "sum of child move counts must equal perft(2)");
    assert_eq!(root.len(), 20, "perft(1) == 20");
}

/// `AliceRules::is_alice()` is the only enabled hook, and it is on.
#[test]
fn is_alice_hook_is_on() {
    assert!(<mcr::geometry::AliceRules as WideVariant<Chess8x8>>::is_alice());
}
