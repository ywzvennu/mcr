//! Alice chess perft validation on the generic engine (issue #276).
//!
//! Alice chess has **no Fairy-Stockfish oracle** — FSF does not implement
//! `UCI_Variant alice` (confirmed: it is absent from the variant list, silently
//! ignored, and inexpressible via `variants.ini`). The pins here are therefore
//! **rules-validated (no FSF oracle); perft pins hand-derived per the documented
//! Alice ruleset**, and cross-checked by a fully **independent brute-force Alice
//! move generator** written from scratch in this file (a separate array-based
//! position model with its own naive movegen and plane-restricted king-safety).
//! Two independent implementations agreeing on every node count to depth 4 is the
//! substitute for the missing perft oracle.
//!
//! ## How the shallow numbers are derived
//!
//! * **perft(1) = 20.** At the start every piece is on board A and board B is
//!   empty. Each of White's 20 ordinary chess opening moves is legal on board A,
//!   and its transfer square on the empty board B is vacant, so all 20 are legal
//!   Alice moves — identical to standard chess. No check can arise.
//! * **perft(2) = 400.** After White's move exactly one white piece sits on board
//!   B (on rank 3 or 4); Black's 20 replies all land on ranks 5–6, so none is
//!   blocked by a transfer conflict and none gives or evades check. 20 × 20 = 400,
//!   again identical to standard chess.
//! * **perft(3) = 9384, perft(4) = 219236, perft(5) = 5910465.** Here Alice
//!   diverges from standard chess (8902 / 197281 / 4865609): a piece sitting on
//!   the near-empty board B usually has *more* mobility there than on board A,
//!   while a few board-A moves become illegal when their transfer square is
//!   occupied on board B. These counts are produced **identically** by the engine
//!   and by the independent brute-force generator below (depths 1–4 in the cheap
//!   test; the deep layers are `#[ignore]`d).
//!
//! Castling, promotion, and en passant never occur in the start-position tree at
//! these depths, so the brute force — which implements the core two-board
//! movement, transfer, and king-safety rules (no castle / promotion / en passant,
//! matching Alice's documented exclusion of en passant) — enumerates exactly the
//! same moves as the engine there. Those special mechanics are validated
//! separately by the hand-constructed unit tests in `tests/alice_rules.rs`.

use mce::geometry::{perft as gperft, Alice, Chess8x8};

/// The Alice starting FEN — the standard chess array (all pieces begin on board
/// A; the `board_b` plane mask is empty, so the standard FEN reconstructs it).
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// The engine's pinned start-position perft, depths 1–5. Depths 1–2 are
/// hand-derived (see the module docs); depths 3–5 are cross-checked against the
/// independent brute-force generator (depths 1–4) below.
const ENGINE_PINS: &[(u32, u64)] = &[(1, 20), (2, 400), (3, 9384), (4, 219236), (5, 5910465)];

#[test]
fn engine_startpos_perft_pins() {
    let pos = Alice::from_fen(STARTPOS).expect("valid Alice FEN");
    // Cheap layers only (1–4); depth 5 is pinned in the ignored deep test.
    for &(depth, expected) in &ENGINE_PINS[..4] {
        let got = gperft::<Chess8x8, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "engine Alice perft({depth}) mismatch (rules-validated, no FSF oracle)"
        );
    }
}

#[test]
#[ignore = "deep perft; run with --release --test perft_alice -- --include-ignored"]
fn engine_startpos_perft_deep() {
    let pos = Alice::from_fen(STARTPOS).expect("valid Alice FEN");
    let got = gperft::<Chess8x8, _>(&pos, 5);
    assert_eq!(got, 5_910_465, "engine Alice perft(5) mismatch");
}

/// The engine and the **independent** brute-force generator must agree on every
/// node count, depths 1–3 (cheap). This is the no-oracle cross-validation.
#[test]
fn engine_matches_independent_brute_force_shallow() {
    let engine = Alice::from_fen(STARTPOS).expect("valid Alice FEN");
    let bf = brute::Position::startpos();
    for depth in 1..=3 {
        let e = gperft::<Chess8x8, _>(&engine, depth);
        let b = brute::perft(&bf, depth);
        assert_eq!(
            e, b,
            "engine vs independent brute-force Alice perft({depth}) disagree: {e} vs {b}"
        );
    }
    // Spot-check the literal pins too.
    assert_eq!(brute::perft(&bf, 1), 20);
    assert_eq!(brute::perft(&bf, 2), 400);
    assert_eq!(brute::perft(&bf, 3), 9384);
}

#[test]
#[ignore = "deep brute-force cross-check; run with --release -- --include-ignored"]
fn engine_matches_independent_brute_force_depth4() {
    let engine = Alice::from_fen(STARTPOS).expect("valid Alice FEN");
    let bf = brute::Position::startpos();
    assert_eq!(gperft::<Chess8x8, _>(&engine, 4), 219_236);
    assert_eq!(brute::perft(&bf, 4), 219_236);
}

/// A fully independent, naive, array-based Alice move generator and perft —
/// written from scratch (its own position model and rules, no use of the engine
/// under test) to cross-validate the engine's perft without an FSF oracle.
///
/// It implements the documented Alice rules for the moves that occur in the
/// start-position tree to depth ≥5: ordinary chess movement on a piece's own
/// plane, the transfer to the opposite plane (legal only if that square is vacant
/// there), same-plane-only capture/blocking/check, and the two king-safety
/// conditions (no discovered/transfer check on the plane the king ends on; a king
/// move's destination also unattacked on the plane it leaves). Castling,
/// promotion, and en passant do not arise in that tree and are intentionally
/// omitted (en passant is excluded from Alice anyway).
mod brute {
    // Colors / roles / planes as small integers, deliberately unlike the engine.
    const WHITE: u8 = 0;
    const BLACK: u8 = 1;
    const PAWN: u8 = 0;
    const KNIGHT: u8 = 1;
    const BISHOP: u8 = 2;
    const ROOK: u8 = 3;
    const QUEEN: u8 = 4;
    const KING: u8 = 5;

    #[derive(Clone, Copy, PartialEq, Eq)]
    struct Cell {
        color: u8,
        role: u8,
        plane: u8, // 0 = board A, 1 = board B
    }

    #[derive(Clone)]
    pub(crate) struct Position {
        cells: [Option<Cell>; 64],
        turn: u8,
    }

    #[inline]
    fn sq(file: i32, rank: i32) -> usize {
        (rank * 8 + file) as usize
    }
    #[inline]
    fn file_of(s: usize) -> i32 {
        (s % 8) as i32
    }
    #[inline]
    fn rank_of(s: usize) -> i32 {
        (s / 8) as i32
    }
    #[inline]
    fn on_board(file: i32, rank: i32) -> bool {
        (0..8).contains(&file) && (0..8).contains(&rank)
    }

    impl Position {
        pub(crate) fn startpos() -> Position {
            let mut cells = [None; 64];
            let back = [ROOK, KNIGHT, BISHOP, QUEEN, KING, BISHOP, KNIGHT, ROOK];
            for f in 0..8 {
                cells[sq(f, 0)] = Some(Cell {
                    color: WHITE,
                    role: back[f as usize],
                    plane: 0,
                });
                cells[sq(f, 1)] = Some(Cell {
                    color: WHITE,
                    role: PAWN,
                    plane: 0,
                });
                cells[sq(f, 6)] = Some(Cell {
                    color: BLACK,
                    role: PAWN,
                    plane: 0,
                });
                cells[sq(f, 7)] = Some(Cell {
                    color: BLACK,
                    role: back[f as usize],
                    plane: 0,
                });
            }
            Position { turn: WHITE, cells }
        }

        /// Is `target` occupied on `plane` (a piece whose plane == `plane`)?
        #[inline]
        fn occupied_on(&self, target: usize, plane: u8) -> bool {
            matches!(self.cells[target], Some(c) if c.plane == plane)
        }

        /// Are all squares strictly between `a` and `b` (which must be aligned)
        /// empty on `plane`? Used for slider rays.
        fn ray_clear(&self, a: usize, b: usize, plane: u8) -> bool {
            let (df, dr) = (
                (file_of(b) - file_of(a)).signum(),
                (rank_of(b) - rank_of(a)).signum(),
            );
            let (mut f, mut r) = (file_of(a) + df, rank_of(a) + dr);
            while sq(f, r) != b {
                if self.occupied_on(sq(f, r), plane) {
                    return false;
                }
                f += df;
                r += dr;
            }
            true
        }

        /// Does a `by`-colored piece on `plane` attack `target`?
        fn attacked(&self, target: usize, by: u8, plane: u8) -> bool {
            for s in 0..64 {
                let Some(c) = self.cells[s] else { continue };
                if c.plane != plane || c.color != by {
                    continue;
                }
                if self.piece_attacks(s, c, target) {
                    return true;
                }
            }
            false
        }

        fn piece_attacks(&self, from: usize, c: Cell, target: usize) -> bool {
            let (ff, fr) = (file_of(from), rank_of(from));
            let (tf, tr) = (file_of(target), rank_of(target));
            let (adf, adr) = ((tf - ff).abs(), (tr - fr).abs());
            match c.role {
                PAWN => {
                    let dir = if c.color == WHITE { 1 } else { -1 };
                    adf == 1 && tr - fr == dir
                }
                KNIGHT => (adf, adr) == (1, 2) || (adf, adr) == (2, 1),
                KING => adf <= 1 && adr <= 1 && (adf + adr) != 0,
                BISHOP => adf == adr && adf != 0 && self.ray_clear(from, target, c.plane),
                ROOK => {
                    (ff == tf || fr == tr)
                        && from != target
                        && self.ray_clear(from, target, c.plane)
                }
                QUEEN => {
                    ((ff == tf || fr == tr) || (adf == adr && adf != 0))
                        && from != target
                        && self.ray_clear(from, target, c.plane)
                }
                _ => false,
            }
        }

        /// Pseudo-legal (from, to) moves: a chess move on the mover's own plane
        /// whose destination is vacant on the opposite plane.
        fn pseudo_moves(&self) -> Vec<(usize, usize)> {
            let mut mv = Vec::new();
            for from in 0..64 {
                let Some(c) = self.cells[from] else { continue };
                if c.color != self.turn {
                    continue;
                }
                let plane = c.plane;
                let other = 1 - plane;
                // A landing square `to` is reachable if it is empty on the mover's
                // plane (quiet) or holds a same-plane enemy (capture), AND it is
                // vacant on the opposite plane (the transfer target). Equivalent to:
                // `to` is totally empty, or holds a same-plane enemy.
                let try_land =
                    |to: usize, must_be_capture: bool, list: &mut Vec<(usize, usize)>| {
                        match self.cells[to] {
                            None => {
                                if !must_be_capture {
                                    list.push((from, to));
                                }
                            }
                            Some(t) => {
                                // Capture only a same-plane enemy; same-plane friendly
                                // blocks; an opposite-plane piece blocks the transfer.
                                if t.plane == plane && t.color != self.turn {
                                    list.push((from, to));
                                }
                            }
                        }
                    };
                match c.role {
                    PAWN => self.pawn_moves(from, c, &mut mv),
                    KNIGHT => {
                        for (df, dr) in [
                            (1, 2),
                            (2, 1),
                            (-1, 2),
                            (-2, 1),
                            (1, -2),
                            (2, -1),
                            (-1, -2),
                            (-2, -1),
                        ] {
                            let (nf, nr) = (file_of(from) + df, rank_of(from) + dr);
                            if on_board(nf, nr) {
                                try_land(sq(nf, nr), false, &mut mv);
                            }
                        }
                    }
                    KING => {
                        for df in -1..=1 {
                            for dr in -1..=1 {
                                if df == 0 && dr == 0 {
                                    continue;
                                }
                                let (nf, nr) = (file_of(from) + df, rank_of(from) + dr);
                                if on_board(nf, nr) {
                                    try_land(sq(nf, nr), false, &mut mv);
                                }
                            }
                        }
                    }
                    BISHOP | ROOK | QUEEN => {
                        let dirs: &[(i32, i32)] = match c.role {
                            BISHOP => &[(1, 1), (1, -1), (-1, 1), (-1, -1)],
                            ROOK => &[(1, 0), (-1, 0), (0, 1), (0, -1)],
                            _ => &[
                                (1, 1),
                                (1, -1),
                                (-1, 1),
                                (-1, -1),
                                (1, 0),
                                (-1, 0),
                                (0, 1),
                                (0, -1),
                            ],
                        };
                        for &(df, dr) in dirs {
                            let (mut nf, mut nr) = (file_of(from) + df, rank_of(from) + dr);
                            while on_board(nf, nr) {
                                let to = sq(nf, nr);
                                match self.cells[to] {
                                    None => mv.push((from, to)), // transfer ok; ray continues
                                    Some(t) => {
                                        if t.plane == plane {
                                            // Same-plane piece: capture if enemy, then stop.
                                            if t.color != self.turn {
                                                mv.push((from, to));
                                            }
                                            break;
                                        }
                                        // Opposite-plane piece: blocks the transfer
                                        // (not a landing) but does not block the ray.
                                    }
                                }
                                nf += df;
                                nr += dr;
                            }
                        }
                    }
                    _ => {}
                }
                let _ = other;
            }
            mv
        }

        fn pawn_moves(&self, from: usize, c: Cell, mv: &mut Vec<(usize, usize)>) {
            let plane = c.plane;
            let dir = if c.color == WHITE { 1 } else { -1 };
            let start_rank = if c.color == WHITE { 1 } else { 6 };
            let (ff, fr) = (file_of(from), rank_of(from));
            // Forward one: needs to be clear on the mover's own plane to slide.
            let one = (ff, fr + dir);
            if on_board(one.0, one.1) {
                let one_s = sq(one.0, one.1);
                let one_clear_plane = !self.occupied_on(one_s, plane);
                if one_clear_plane {
                    // Single push: transfer needs the square vacant on both planes.
                    if self.cells[one_s].is_none() {
                        mv.push((from, one_s));
                    }
                    // Double push: the landing must be totally empty.
                    if fr == start_rank {
                        let two = (ff, fr + 2 * dir);
                        if on_board(two.0, two.1) {
                            let two_s = sq(two.0, two.1);
                            if self.cells[two_s].is_none() {
                                mv.push((from, two_s));
                            }
                        }
                    }
                }
            }
            // Captures: a same-plane enemy on a forward diagonal.
            for df in [-1, 1] {
                let (nf, nr) = (ff + df, fr + dir);
                if on_board(nf, nr) {
                    let to = sq(nf, nr);
                    if let Some(t) = self.cells[to] {
                        if t.plane == plane && t.color != c.color {
                            mv.push((from, to));
                        }
                    }
                }
            }
        }

        /// Applies a pseudo-legal move, transferring the mover to the other plane.
        fn apply(&self, from: usize, to: usize) -> Position {
            let mut p = self.clone();
            let mut moving = p.cells[from].expect("mover present");
            p.cells[from] = None;
            // A capture removes the same-plane victim already standing on `to`.
            moving.plane = 1 - moving.plane; // transfer
            p.cells[to] = Some(moving);
            p.turn = 1 - p.turn;
            p
        }

        fn king_of(&self, color: u8) -> Option<usize> {
            (0..64)
                .find(|&s| matches!(self.cells[s], Some(c) if c.color == color && c.role == KING))
        }

        /// Filters the pseudo-legal moves to the legal ones under Alice king-safety.
        pub(crate) fn legal_moves(&self) -> Vec<(usize, usize)> {
            let us = self.turn;
            let them = 1 - us;
            let mut out = Vec::new();
            for (from, to) in self.pseudo_moves() {
                let moving = self.cells[from].expect("mover");
                // Condition X: a king move's destination must be unattacked on the
                // plane it leaves (pre-transfer), king at `to`, captured removed.
                if moving.role == KING {
                    let mut pre = self.clone();
                    pre.cells[from] = None;
                    pre.cells[to] = Some(Cell {
                        color: us,
                        role: KING,
                        plane: moving.plane, // not yet transferred
                    });
                    if pre.attacked(to, them, moving.plane) {
                        continue;
                    }
                }
                // Condition Y: after the transfer the king must be unattacked on
                // the plane it ends up on.
                let next = self.apply(from, to);
                if let Some(king) = next.king_of(us) {
                    let kplane = next.cells[king].expect("king").plane;
                    if next.attacked(king, them, kplane) {
                        continue;
                    }
                }
                out.push((from, to));
            }
            out
        }
    }

    pub(crate) fn perft(pos: &Position, depth: u32) -> u64 {
        if depth == 0 {
            return 1;
        }
        let moves = pos.legal_moves();
        if depth == 1 {
            return moves.len() as u64;
        }
        let mut total = 0;
        for (from, to) in moves {
            total += perft(&pos.apply(from, to), depth - 1);
        }
        total
    }
}
