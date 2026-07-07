//! Supply chess (Xiangqi 9x10 with drops) perft validation (issue #585).
//!
//! Supply is Fairy-Stockfish's built-in `supply` (`supply_variant()`), but **the FSF
//! binary available here is a non-large-board build**: it has neither `supply` nor
//! even the 9x10 `xiangqi` variant (its `UCI_Variant` list stops at the 7x7
//! `minixiangqi`), and asked for `UCI_Variant supply` it silently stays in standard
//! chess and returns chess-root perft counts. So there is **no live FSF perft
//! oracle** here (see `docs/oracle-less-validation.md`).
//!
//! Supply is therefore **rules-validated** two ways:
//!
//! 1. **Equivalence to Xiangqi.** FSF `supply` leaves `capturesToHand = false` and
//!    is a two-board (`twoBoards`) game whose hand is fed by the *partner* board,
//!    never by a capture on this board; FSF also excludes its two-board "virtual"
//!    drops from perft. On a single board the hand thus starts empty and stays
//!    empty, so **every** Supply move tree from a normal (empty-hand) position is
//!    Xiangqi's node-for-node. This file pins that `Supply::perft == Xiangqi::perft`
//!    at the FSF-confirmed Xiangqi start and a middlegame — an in-repo cross-check
//!    against mcr's independently-FSF-validated Xiangqi (`perft_xiangqi.rs`), and,
//!    when an FSF built `largeboards=yes` is present, `compare-fairy` drives the same
//!    equivalence live against `UCI_Variant xiangqi` (`compare-fairy/src/supply.rs`).
//!
//! 2. **The drop mechanic** — which the Xiangqi equivalence cannot cover — is
//!    cross-checked node-for-node against a fully **independent, from-scratch 9x10
//!    Xiangqi-with-drops move generator** written in this file (its own array board
//!    model, move tables, hobbled Horse / eye-blocked Elephant / over-screen Cannon /
//!    river-crossing Soldier / palace confinement / flying general, the own-half
//!    per-piece drop region, and the `dropChecks = false` rule), sharing no code with
//!    the engine under test. Two independent implementations agreeing on every node
//!    count of positions that **do** hold pieces in hand is the substitute for the
//!    missing engine oracle (issue #500).
//!
//! ## How the start count is derived
//!
//! **perft(1) = 44**, hand-derived and identical to Xiangqi's FSF-confirmed start
//! count (the empty Supply hand adds no drop): the two Horses reach 2 squares each
//! (4), the two Cannons 12 each region-limited to 17 total after screens… — it is
//! exactly the Xiangqi start move count, pinned node-for-node against FSF in
//! `perft_xiangqi.rs`, and re-derived here by the independent generator below.

use mcr::geometry::{perft as gperft, Supply, Xiangqi, Xiangqi9x10};

/// The Supply starting FEN (mcr dialect, empty hand `[]`).
const STARTPOS: &str = "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/9/RJOUKUOJR[] w - - 0 1";

/// A Xiangqi middlegame (empty hand) — a normal position where Supply and Xiangqi
/// coincide node-for-node.
const MIDGAME: &str = "r1oukuo1r/9/1cj3jc1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1CJ3JC1/9/R1OUKUO1R[] w - - 0 1";

/// A crafted position **with pieces in hand** (White Cannon, Black Cannon) plus a
/// Soldier screen, exercising region-restricted drops and the `dropChecks = false`
/// rule. Cross-checked against the independent generator.
const HAND_DROP: &str = "3k5/9/9/9/2z6/9/9/9/9/2R1K1R2[Cc] w - - 0 1";

/// A second crafted hand position (White Cannon, Black Soldier) with an active
/// Chariot / Horse tangle, again cross-checked against the independent generator.
const HAND_DROP2: &str = "5k2r/9/9/9/9/9/9/9/9/Rj1K5[Cz] w - - 0 1";

/// Engine perft pins for the start position. Depth 1 is hand-derived (module docs)
/// and equal to Xiangqi's; depths 2–3 are also reproduced by the independent
/// generator and equal Xiangqi's.
const START_PINS: &[(u32, u64)] = &[(1, 44), (2, 1920), (3, 79_666)];

#[test]
fn engine_startpos_perft_pins() {
    let pos = Supply::from_fen(STARTPOS).expect("valid Supply FEN");
    for &(depth, expected) in START_PINS {
        assert_eq!(
            gperft::<Xiangqi9x10, _, _>(&pos, depth),
            expected,
            "engine Supply perft({depth}) mismatch (rules-validated, no FSF oracle)"
        );
    }
}

#[test]
#[ignore = "deep perft; run with --release --test perft_supply -- --include-ignored"]
fn engine_startpos_perft_deep() {
    let pos = Supply::from_fen(STARTPOS).expect("valid Supply FEN");
    assert_eq!(gperft::<Xiangqi9x10, _, _>(&pos, 4), 3_290_240);
}

/// The Xiangqi equivalence: with an empty hand and `capturesToHand = false`, Supply
/// generates no drop in ordinary play, so its move tree is Xiangqi's node-for-node.
/// mcr's Xiangqi is itself pinned against FSF (`perft_xiangqi.rs`), so this is a live
/// FSF-backed oracle for every empty-hand Supply position.
#[test]
fn engine_matches_xiangqi_on_empty_hand() {
    for fen in [STARTPOS, MIDGAME] {
        let supply = Supply::from_fen(fen).expect("valid Supply FEN");
        // The Xiangqi engine reads the same placement without the holdings bracket.
        let xq_fen = fen.replace("[]", "");
        let xiangqi = Xiangqi::from_fen(&xq_fen).expect("valid Xiangqi FEN");
        for depth in 1..=4 {
            assert_eq!(
                gperft::<Xiangqi9x10, _, _>(&supply, depth),
                gperft::<Xiangqi9x10, _, _>(&xiangqi, depth),
                "Supply vs Xiangqi perft({depth}) disagree for {fen}"
            );
        }
    }
}

/// Engine perft pins for the crafted hand positions (drops active). Re-derived by the
/// independent generator below.
const HAND_PINS: &[(u32, u64)] = &[(1, 64), (2, 2756), (3, 125_863)];
const HAND2_PINS: &[(u32, u64)] = &[(1, 53), (2, 1333), (3, 39_404)];

#[test]
fn engine_hand_perft_pins() {
    for (fen, pins) in [(HAND_DROP, HAND_PINS), (HAND_DROP2, HAND2_PINS)] {
        let pos = Supply::from_fen(fen).expect("valid Supply FEN");
        for &(depth, expected) in pins {
            assert_eq!(
                gperft::<Xiangqi9x10, _, _>(&pos, depth),
                expected,
                "engine Supply hand perft({depth}) mismatch for {fen}"
            );
        }
    }
}

/// The engine and the **independent** from-scratch generator must agree on every
/// node count, depths 1–3, for the start position, the middlegame, and both crafted
/// hand positions (the drop mechanic). This is the no-oracle cross-validation (issue
/// #500).
#[test]
fn engine_matches_independent_generator() {
    for fen in [STARTPOS, MIDGAME, HAND_DROP, HAND_DROP2] {
        let engine = Supply::from_fen(fen).expect("valid Supply FEN");
        let bf = brute::Position::parse(fen);
        for depth in 1..=3 {
            let e = gperft::<Xiangqi9x10, _, _>(&engine, depth);
            let b = brute::perft(&bf, depth);
            assert_eq!(
                e, b,
                "engine vs independent Supply perft({depth}) disagree for {fen}: {e} vs {b}"
            );
        }
    }
    // The independent generator reproduces the hand-derived / Xiangqi-equal root.
    assert_eq!(brute::perft(&brute::Position::parse(STARTPOS), 1), 44);
}

/// A fully independent, naive, array-based Supply (Xiangqi-with-drops) move generator
/// and perft — written from scratch (its own 9x10 board model, hobbled Horse,
/// eye-blocked Elephant, over-screen Cannon, river-crossing Soldier, palace / river
/// confinement, flying general, the own-half per-piece drop region, and the
/// `dropChecks = false` rule) with no use of the engine under test, to cross-validate
/// the engine's perft without an external oracle.
mod brute {
    const N: i32 = 9; // files a..i
    const H: i32 = 10; // ranks 1..10
    const W: u8 = 0;
    const B: u8 = 1;

    const ROOK: u8 = 0;
    const HORSE: u8 = 1;
    const ELEPHANT: u8 = 2;
    const ADVISOR: u8 = 3;
    const KING: u8 = 4;
    const CANNON: u8 = 5;
    const SOLDIER: u8 = 6;

    #[inline]
    fn idx(f: i32, r: i32) -> usize {
        (r * N + f) as usize
    }
    #[inline]
    fn file_of(i: usize) -> i32 {
        (i as i32) % N
    }
    #[inline]
    fn rank_of(i: usize) -> i32 {
        (i as i32) / N
    }
    #[inline]
    fn on_board(f: i32, r: i32) -> bool {
        (0..N).contains(&f) && (0..H).contains(&r)
    }
    /// The 3x3 palace: files d..f (3..=5), on the near three ranks of `color`.
    fn in_palace(color: u8, f: i32, r: i32) -> bool {
        let rank_ok = if color == W {
            (0..=2).contains(&r)
        } else {
            (7..=9).contains(&r)
        };
        (3..=5).contains(&f) && rank_ok
    }
    /// The own half (a side of the river): White ranks 1..5, Black ranks 6..10.
    fn in_own_half(color: u8, r: i32) -> bool {
        if color == W {
            (0..=4).contains(&r)
        } else {
            (5..=9).contains(&r)
        }
    }
    /// Whether a Soldier of `color` on rank `r` has crossed the river.
    fn soldier_crossed(color: u8, r: i32) -> bool {
        if color == W {
            r >= 5
        } else {
            r <= 4
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    struct Pc {
        color: u8,
        role: u8,
    }

    #[derive(Clone)]
    pub(crate) struct Position {
        cells: [Option<Pc>; (N * H) as usize],
        turn: u8,
        /// hand[color][role] counts (only non-King roles ever occur).
        hand: [[u8; 7]; 2],
    }

    #[derive(Clone, Copy)]
    enum Mv {
        Board { from: usize, to: usize },
        Drop { sq: usize, role: u8 },
    }

    fn role_of(letter: char) -> u8 {
        match letter.to_ascii_lowercase() {
            'r' => ROOK,
            'j' => HORSE,
            'o' => ELEPHANT,
            'u' => ADVISOR,
            'k' => KING,
            'c' => CANNON,
            'z' => SOLDIER,
            other => panic!("unexpected Supply FEN piece letter {other}"),
        }
    }

    impl Position {
        /// Parse a Supply FEN (mcr dialect, with a `[…]` holdings bracket).
        /// Independent of the engine's parser.
        pub(crate) fn parse(fen: &str) -> Position {
            let mut fields = fen.split_whitespace();
            let mut placement = fields.next().expect("placement").to_string();
            let turn = if fields.next() == Some("b") { B } else { W };

            let mut hand = [[0u8; 7]; 2];
            if let Some(open) = placement.find('[') {
                let close = placement.find(']').expect("closing bracket");
                let held = placement[open + 1..close].to_string();
                placement.truncate(open);
                for ch in held.chars() {
                    let color = if ch.is_ascii_uppercase() { W } else { B };
                    hand[color as usize][role_of(ch) as usize] += 1;
                }
            }

            let mut cells = [None; (N * H) as usize];
            for (row, line) in placement.split('/').enumerate() {
                let rank = (H - 1) - row as i32;
                let mut file = 0i32;
                let mut chars = line.chars().peekable();
                while let Some(c) = chars.next() {
                    if c.is_ascii_digit() {
                        let mut num = c.to_digit(10).unwrap() as i32;
                        while let Some(&d) = chars.peek() {
                            if d.is_ascii_digit() {
                                num = num * 10 + d.to_digit(10).unwrap() as i32;
                                chars.next();
                            } else {
                                break;
                            }
                        }
                        file += num;
                        continue;
                    }
                    let color = if c.is_ascii_uppercase() { W } else { B };
                    cells[idx(file, rank)] = Some(Pc {
                        color,
                        role: role_of(c),
                    });
                    file += 1;
                }
            }
            Position { cells, turn, hand }
        }

        /// The squares a piece on `from` **attacks** (its check / capture set),
        /// occupancy-aware, ignoring the colour of the target square (a drop-check or
        /// king-safety probe tests membership). The flying-general file attack is
        /// handled by [`attacked`].
        fn attack_set(&self, from: usize, role: u8, color: u8, out: &mut Vec<usize>) {
            let (ff, fr) = (file_of(from), rank_of(from));
            match role {
                ROOK => {
                    for (df, dr) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        let (mut nf, mut nr) = (ff + df, fr + dr);
                        while on_board(nf, nr) {
                            let t = idx(nf, nr);
                            out.push(t);
                            if self.cells[t].is_some() {
                                break;
                            }
                            nf += df;
                            nr += dr;
                        }
                    }
                }
                CANNON => {
                    // Over exactly one screen: skip to the first occupied (screen),
                    // then the next occupied square along the ray is the attack target.
                    for (df, dr) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        let (mut nf, mut nr) = (ff + df, fr + dr);
                        // advance to the screen
                        while on_board(nf, nr) && self.cells[idx(nf, nr)].is_none() {
                            nf += df;
                            nr += dr;
                        }
                        if !on_board(nf, nr) {
                            continue;
                        }
                        // past the screen, find the next occupied square
                        nf += df;
                        nr += dr;
                        while on_board(nf, nr) {
                            if self.cells[idx(nf, nr)].is_some() {
                                out.push(idx(nf, nr));
                                break;
                            }
                            nf += df;
                            nr += dr;
                        }
                    }
                }
                HORSE => {
                    // Knight leaps hobbled by the orthogonal leg adjacent to the horse.
                    const LEAPS: [(i32, i32, i32, i32); 8] = [
                        (1, 2, 0, 1),
                        (-1, 2, 0, 1),
                        (1, -2, 0, -1),
                        (-1, -2, 0, -1),
                        (2, 1, 1, 0),
                        (2, -1, 1, 0),
                        (-2, 1, -1, 0),
                        (-2, -1, -1, 0),
                    ];
                    for (df, dr, lf, lr) in LEAPS {
                        let (nf, nr) = (ff + df, fr + dr);
                        if on_board(nf, nr) && self.cells[idx(ff + lf, fr + lr)].is_none() {
                            out.push(idx(nf, nr));
                        }
                    }
                }
                ELEPHANT => {
                    for (df, dr) in [(2, 2), (2, -2), (-2, 2), (-2, -2)] {
                        let (nf, nr) = (ff + df, fr + dr);
                        let (ef, er) = (ff + df / 2, fr + dr / 2); // the eye
                        if on_board(nf, nr)
                            && in_own_half(color, nr)
                            && self.cells[idx(ef, er)].is_none()
                        {
                            out.push(idx(nf, nr));
                        }
                    }
                }
                ADVISOR => {
                    for (df, dr) in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
                        let (nf, nr) = (ff + df, fr + dr);
                        if on_board(nf, nr) && in_palace(color, nf, nr) {
                            out.push(idx(nf, nr));
                        }
                    }
                }
                KING => {
                    for (df, dr) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        let (nf, nr) = (ff + df, fr + dr);
                        if on_board(nf, nr) && in_palace(color, nf, nr) {
                            out.push(idx(nf, nr));
                        }
                    }
                }
                SOLDIER => {
                    let fwd = if color == W { 1 } else { -1 };
                    if on_board(ff, fr + fwd) {
                        out.push(idx(ff, fr + fwd));
                    }
                    if soldier_crossed(color, fr) {
                        for df in [-1, 1] {
                            if on_board(ff + df, fr) {
                                out.push(idx(ff + df, fr));
                            }
                        }
                    }
                }
                _ => unreachable!(),
            }
        }

        /// Whether `sq` is attacked by any piece of colour `by` (incl. the
        /// flying-general file attack).
        fn attacked(&self, sq: usize, by: u8) -> bool {
            let mut buf = Vec::new();
            for i in 0..self.cells.len() {
                let Some(p) = self.cells[i] else { continue };
                if p.color != by {
                    continue;
                }
                buf.clear();
                self.attack_set(i, p.role, by, &mut buf);
                if buf.contains(&sq) {
                    return true;
                }
            }
            // Flying general: the `by` general attacks `sq` down an open shared file.
            if let Some(k) = self.king_sq(by) {
                if file_of(k) == file_of(sq) && k != sq {
                    let (f, r0, r1) = (
                        file_of(k),
                        rank_of(k).min(rank_of(sq)),
                        rank_of(k).max(rank_of(sq)),
                    );
                    let clear = (r0 + 1..r1).all(|r| self.cells[idx(f, r)].is_none());
                    if clear {
                        return true;
                    }
                }
            }
            false
        }

        /// The own-half drop region test for `role` of `color` on square `sq`
        /// (already known empty): FSF `dropRegion` ∩ the piece's `mobilityRegion`.
        fn in_drop_region(role: u8, color: u8, sq: usize) -> bool {
            let (f, r) = (file_of(sq), rank_of(sq));
            if !in_own_half(color, r) {
                return false;
            }
            match role {
                ROOK | HORSE | CANNON => true,
                ADVISOR => in_palace(color, f, r),
                ELEPHANT => {
                    // The seven Elephant points of `color` (own half).
                    let pts: [(i32, i32); 7] =
                        [(2, 0), (6, 0), (0, 2), (4, 2), (8, 2), (2, 4), (6, 4)];
                    pts.iter()
                        .any(|&(pf, pr)| pf == f && pr == if color == W { r } else { 9 - r })
                }
                SOLDIER => {
                    // Pre-river residences: files a/c/e/g/i (even) on ranks 4..5 of
                    // `color` (0-based normalized ranks 3..4).
                    let rr = if color == W { r } else { 9 - r };
                    f % 2 == 0 && (rr == 3 || rr == 4)
                }
                _ => false,
            }
        }

        fn king_sq(&self, color: u8) -> Option<usize> {
            (0..self.cells.len())
                .find(|&i| matches!(self.cells[i], Some(p) if p.color == color && p.role == KING))
        }

        fn pseudo(&self) -> Vec<Mv> {
            let mut mv = Vec::new();
            let us = self.turn;
            // Board moves: a piece may land on an empty square or capture an enemy.
            for from in 0..self.cells.len() {
                let Some(p) = self.cells[from] else { continue };
                if p.color != us {
                    continue;
                }
                if p.role == CANNON {
                    // Quiet slides (empty) + over-screen captures (enemy).
                    let (ff, fr) = (file_of(from), rank_of(from));
                    for (df, dr) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        let (mut nf, mut nr) = (ff + df, fr + dr);
                        while on_board(nf, nr) && self.cells[idx(nf, nr)].is_none() {
                            mv.push(Mv::Board {
                                from,
                                to: idx(nf, nr),
                            });
                            nf += df;
                            nr += dr;
                        }
                        // at the screen (or off board)
                        if !on_board(nf, nr) {
                            continue;
                        }
                        nf += df;
                        nr += dr;
                        while on_board(nf, nr) {
                            if let Some(q) = self.cells[idx(nf, nr)] {
                                if q.color != us {
                                    mv.push(Mv::Board {
                                        from,
                                        to: idx(nf, nr),
                                    });
                                }
                                break;
                            }
                            nf += df;
                            nr += dr;
                        }
                    }
                    continue;
                }
                let mut buf = Vec::new();
                self.attack_set(from, p.role, us, &mut buf);
                for to in buf {
                    if self.cells[to].is_none_or(|q| q.color != us) {
                        mv.push(Mv::Board { from, to });
                    }
                }
            }
            // Drops: each held role onto an empty own-half region square, unless the
            // drop gives check (FSF `dropChecks = false`).
            let them = 1 - us;
            let enemy_king = self.king_sq(them);
            for role in 0..7u8 {
                if self.hand[us as usize][role as usize] == 0 {
                    continue;
                }
                for sq in 0..self.cells.len() {
                    if self.cells[sq].is_some() {
                        continue;
                    }
                    if !Self::in_drop_region(role, us, sq) {
                        continue;
                    }
                    // dropChecks: the dropped piece must not attack the enemy king on
                    // the post-drop board.
                    if let Some(ek) = enemy_king {
                        let mut tmp = self.clone();
                        tmp.cells[sq] = Some(Pc { color: us, role });
                        let mut buf = Vec::new();
                        tmp.attack_set(sq, role, us, &mut buf);
                        if buf.contains(&ek) {
                            continue;
                        }
                    }
                    mv.push(Mv::Drop { sq, role });
                }
            }
            mv
        }

        fn apply(&self, m: Mv) -> Position {
            let mut p = self.clone();
            let us = self.turn;
            match m {
                Mv::Board { from, to } => {
                    let mover = p.cells[from].expect("mover");
                    p.cells[from] = None;
                    p.cells[to] = Some(mover); // capture (if any) is overwritten; no hand add
                }
                Mv::Drop { sq, role } => {
                    p.cells[sq] = Some(Pc { color: us, role });
                    p.hand[us as usize][role as usize] -= 1;
                }
            }
            p.turn = 1 - us;
            p
        }

        fn legal_moves(&self) -> Vec<Mv> {
            let us = self.turn;
            let them = 1 - us;
            let mut out = Vec::new();
            for m in self.pseudo() {
                let next = self.apply(m);
                if let Some(k) = next.king_sq(us) {
                    if next.attacked(k, them) {
                        continue;
                    }
                }
                out.push(m);
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
        for m in moves {
            total += perft(&pos.apply(m), depth - 1);
        }
        total
    }
}
