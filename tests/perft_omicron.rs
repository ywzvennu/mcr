//! Omicron (12x10 Omega chess on a walled board) perft validation (issue #585).
//!
//! Omicron is Fairy-Stockfish's built-in `omicron`, but **the FSF binary available
//! here is a non-large-board build and does not implement it** (asked for
//! `UCI_Variant omicron` it silently stays in standard chess and returns chess-root
//! perft counts), so there is **no live FSF perft oracle**. Like the other
//! oracle-less variants (Gustav 3, Okisaki Shogi, Yari Shogi, Wa Shogi, Alice; see
//! `docs/oracle-less-validation.md`) the pins here are therefore **rules-validated**:
//! the start-position move count is **hand-derived**, and the engine's perft is
//! cross-checked node-for-node against a fully **independent, from-scratch 12x10 move
//! generator** written in this file (its own array-based board model, move tables,
//! walls, en passant, promotion and king safety, sharing no code with the engine
//! under test). Two independent implementations agreeing on every node count is the
//! substitute for the missing engine oracle.
//!
//! ## How the start count is derived
//!
//! **perft(1) = 30**, hand-derived from the start position
//! `**w10**w/1**xrnbqkbnr**x1/1pppppppppp1/12/12/12/12/1PPPPPPPPPP1/1**XRNBQKBNR**X1/**W10**W`:
//!
//! | Piece(s) | Moves | Reason |
//! |---|---|---|
//! | 10 Pawns (b3–k3) | 20 | each steps one or two squares forward |
//! | Knight d2 | 2 | inward leaps c4 / e4 (b3/f3 pawns, b1/f1 walls) |
//! | Knight i2 | 2 | inward leaps h4 / j4 (g3/k3 pawns, g1/k1 walls) |
//! | Champion b2 | 2 | Dabbaba b4 + Alfil d4 (Wazir/other leaps hit walls/friends) |
//! | Champion k2 | 2 | Dabbaba k4 + Alfil i4 |
//! | Wizard a1 | 1 | boxed corner: only the Camel leap a1–b4 |
//! | Wizard l1 | 1 | boxed corner: only the Camel leap l1–k4 |
//! | Rooks / Bishops / Queen / King | 0 | every ray is blocked at the first step |
//!
//! summing to 20 + 2 + 2 + 2 + 2 + 1 + 1 = **30**. **perft(2) = 900 = 30²**: the
//! armies begin four ranks apart, so no White first move touches any Black piece, and
//! Black keeps all 30 of its mirror-image replies whatever White plays.
//!
//! Depths 1–3 of the start position and depths 1–3 of two midgame positions
//! (exercising active Champion/Wizard leapers near the walls, captures, en passant and
//! promotion) are produced **identically** by the engine and the independent generator
//! below; depth 4 of the start position is an engine-only regression pin (`#[ignore]`d).

use mcr::geometry::{perft as gperft, Omicron, Omicron12x10};

/// The Omicron starting FEN.
const STARTPOS: &str =
    "**w10**w/1**xrnbqkbnr**x1/1pppppppppp1/12/12/12/12/1PPPPPPPPPP1/1**XRNBQKBNR**X1/**W10**W w KQkq - 0 1";

/// A midgame position (White to move) with an **en-passant** capture available
/// (`d6`: Black has just played …d7–d5 past White's e5 pawn), an active White Champion
/// on c4, a Black Wizard on h7, and no castling rights. Cross-checked against the
/// independent generator.
const MIDGAME_EP: &str = "12/6k5/12/7**w4/12/3pP7/2**X9/6K5/12/12 w - d6 0 1";

/// A midgame position (White to move) with a **pawn one step from promotion** (`e8`,
/// which may push to e9 or capture the f9 rook, each promoting to Wizard / Champion /
/// Q / R / B / N), plus a White Champion off the walls and no castling rights.
/// Cross-checked against the independent generator.
const MIDGAME_PROMO: &str = "12/5r4k1/4P7/12/12/7**X4/12/1K10/12/12 w - - 0 1";

/// Engine perft pins for the start position. Depth 1 is hand-derived (module docs);
/// depths 2–3 are also reproduced by the independent generator.
const START_PINS: &[(u32, u64)] = &[(1, 30), (2, 900), (3, 29_540)];

#[test]
fn engine_startpos_perft_pins() {
    let pos = Omicron::from_fen(STARTPOS).expect("valid Omicron FEN");
    for &(depth, expected) in START_PINS {
        let got = gperft::<Omicron12x10, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "engine Omicron perft({depth}) mismatch (rules-validated, no FSF oracle)"
        );
    }
}

#[test]
#[ignore = "deep perft; run with --release --test perft_omicron -- --include-ignored"]
fn engine_startpos_perft_deep() {
    let pos = Omicron::from_fen(STARTPOS).expect("valid Omicron FEN");
    assert_eq!(gperft::<Omicron12x10, _, _>(&pos, 4), 967_381);
}

const MIDGAME_EP_PINS: &[(u32, u64)] = &[(1, 18), (2, 294), (3, 4617)];
const MIDGAME_PROMO_PINS: &[(u32, u64)] = &[(1, 29), (2, 414), (3, 9159)];

#[test]
fn engine_midgame_perft_pins() {
    for (fen, pins) in [
        (MIDGAME_EP, MIDGAME_EP_PINS),
        (MIDGAME_PROMO, MIDGAME_PROMO_PINS),
    ] {
        let pos = Omicron::from_fen(fen).expect("valid Omicron FEN");
        for &(depth, expected) in pins {
            assert_eq!(
                gperft::<Omicron12x10, _, _>(&pos, depth),
                expected,
                "engine midgame perft({depth}) mismatch for {fen}"
            );
        }
    }
}

/// The engine and the **independent** from-scratch generator must agree on every node
/// count, depths 1–3, for the start position and both midgame positions. This is the
/// no-oracle cross-validation (issue #500).
#[test]
fn engine_matches_independent_generator() {
    for fen in [STARTPOS, MIDGAME_EP, MIDGAME_PROMO] {
        let engine = Omicron::from_fen(fen).expect("valid Omicron FEN");
        let bf = brute::Position::parse(fen);
        for depth in 1..=3 {
            let e = gperft::<Omicron12x10, _, _>(&engine, depth);
            let b = brute::perft(&bf, depth);
            assert_eq!(
                e, b,
                "engine vs independent Omicron perft({depth}) disagree for {fen}: {e} vs {b}"
            );
        }
    }
    // The independent generator reproduces the hand-derived root count.
    assert_eq!(brute::perft(&brute::Position::parse(STARTPOS), 1), 30);
}

/// A fully independent, naive, array-based Omicron move generator and perft — written
/// from scratch (its own 12x10 position model, move tables, walls, en passant,
/// promotion and king safety, with no use of the engine under test) to cross-validate
/// the engine's perft without an external oracle. Castling is **not** modelled here;
/// every cross-checked position either has no castling rights or is the start position,
/// where castling cannot occur within three plies (the engine's castling is pinned
/// separately by the variant module's targeted tests).
mod brute {
    const N: i32 = 12; // files
    const H: i32 = 10; // ranks
    const W: u8 = 0;
    const B: u8 = 1;

    const PAWN: u8 = 0;
    const KNIGHT: u8 = 1;
    const BISHOP: u8 = 2;
    const ROOK: u8 = 3;
    const QUEEN: u8 = 4;
    const KING: u8 = 5;
    const CHAMPION: u8 = 6; // FSF `DAW` = Wazir + Alfil + Dabbaba
    const WIZARD: u8 = 7; // FSF `CF` = Camel + Ferz

    /// A move descriptor step `(df, dr)` in absolute board orientation, repeated up to
    /// `range` squares (`0` = unlimited) as a blockable slide, or a single leap.
    #[derive(Clone, Copy)]
    struct Step {
        df: i8,
        dr: i8,
        range: u8,
        jump: bool,
    }
    const fn s(df: i8, dr: i8) -> Step {
        Step {
            df,
            dr,
            range: 0,
            jump: false,
        }
    }
    const fn one(df: i8, dr: i8) -> Step {
        Step {
            df,
            dr,
            range: 1,
            jump: false,
        }
    }
    const fn j(df: i8, dr: i8) -> Step {
        Step {
            df,
            dr,
            range: 1,
            jump: true,
        }
    }

    const BISHOP_STEPS: &[Step] = &[s(1, 1), s(1, -1), s(-1, 1), s(-1, -1)];
    const ROOK_STEPS: &[Step] = &[s(1, 0), s(-1, 0), s(0, 1), s(0, -1)];
    const QUEEN_STEPS: &[Step] = &[
        s(1, 0),
        s(-1, 0),
        s(0, 1),
        s(0, -1),
        s(1, 1),
        s(1, -1),
        s(-1, 1),
        s(-1, -1),
    ];
    const KING_STEPS: &[Step] = &[
        one(1, 0),
        one(-1, 0),
        one(0, 1),
        one(0, -1),
        one(1, 1),
        one(1, -1),
        one(-1, 1),
        one(-1, -1),
    ];
    const KNIGHT_STEPS: &[Step] = &[
        j(1, 2),
        j(-1, 2),
        j(1, -2),
        j(-1, -2),
        j(2, 1),
        j(-2, 1),
        j(2, -1),
        j(-2, -1),
    ];
    // Champion (FSF `DAW`): Wazir one-steps + Dabbaba two-orthogonal jumps + Alfil
    // two-diagonal jumps (all jumps over any intervening piece).
    const CHAMPION_STEPS: &[Step] = &[
        j(1, 0),
        j(-1, 0),
        j(0, 1),
        j(0, -1),
        j(2, 0),
        j(-2, 0),
        j(0, 2),
        j(0, -2),
        j(2, 2),
        j(2, -2),
        j(-2, 2),
        j(-2, -2),
    ];
    // Wizard (FSF `CF`): Camel jumps + Ferz one-diagonal steps.
    const WIZARD_STEPS: &[Step] = &[
        j(1, 3),
        j(1, -3),
        j(-1, 3),
        j(-1, -3),
        j(3, 1),
        j(3, -1),
        j(-3, 1),
        j(-3, -1),
        j(1, 1),
        j(1, -1),
        j(-1, 1),
        j(-1, -1),
    ];

    fn steps_for(role: u8) -> &'static [Step] {
        match role {
            KNIGHT => KNIGHT_STEPS,
            BISHOP => BISHOP_STEPS,
            ROOK => ROOK_STEPS,
            QUEEN => QUEEN_STEPS,
            KING => KING_STEPS,
            CHAMPION => CHAMPION_STEPS,
            WIZARD => WIZARD_STEPS,
            _ => &[],
        }
    }

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
    /// The permanent walls: the a-/l-files (0 / 11) exist only on ranks 1 and 10, so
    /// ranks 2..=9 (0-based 1..=8) of those files are blocked; the top/bottom ranks
    /// (0-based 0 / 9) exist only on the a/l corners, so files b..k (0-based 1..=10) of
    /// those ranks are blocked.
    #[inline]
    fn is_wall(f: i32, r: i32) -> bool {
        ((f == 0 || f == N - 1) && (1..=H - 2).contains(&r))
            || ((r == 0 || r == H - 1) && (1..=N - 2).contains(&f))
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
        ep: Option<usize>, // en-passant target square, if any
    }

    #[derive(Clone, Copy)]
    enum Mv {
        Board {
            from: usize,
            to: usize,
            promote: Option<u8>,
        },
        Ep {
            from: usize,
            to: usize,
            captured: usize,
        },
        Double {
            from: usize,
            to: usize,
            ep: usize,
        },
    }

    fn base_role_of(letter: char) -> u8 {
        match letter.to_ascii_lowercase() {
            'p' => PAWN,
            'n' => KNIGHT,
            'b' => BISHOP,
            'r' => ROOK,
            'q' => QUEEN,
            'k' => KING,
            'x' => CHAMPION, // `**x`/`**X` overflow token
            'w' => WIZARD,   // `**w`/`**W` overflow token
            other => panic!("unexpected FEN piece letter {other}"),
        }
    }

    impl Position {
        /// Parse an Omicron FEN (placement, turn, castling, ep, …). Independent of the
        /// engine's parser. The Champion is spelled `**x`/`**X` and the Wizard
        /// `**w`/`**W` (two `*` then the base letter); walls are implicit (rendered as
        /// empty squares).
        pub(crate) fn parse(fen: &str) -> Position {
            let mut fields = fen.split_whitespace();
            let placement = fields.next().expect("placement");
            let turn = if fields.next() == Some("b") { B } else { W };
            let _castling = fields.next();
            let ep_field = fields.next().unwrap_or("-");

            let mut cells = [None; (N * H) as usize];
            for (row, line) in placement.split('/').enumerate() {
                let rank = (H - 1) - row as i32;
                let mut file = 0i32;
                let mut chars = line.chars().peekable();
                while let Some(c) = chars.next() {
                    if c == '*' {
                        // Overflow prefix: consume any further '*' then the base letter;
                        // this is one fairy piece on one square.
                        while chars.peek() == Some(&'*') {
                            chars.next();
                        }
                        let letter = chars.next().expect("overflow role letter");
                        let color = if letter.is_ascii_uppercase() { W } else { B };
                        cells[idx(file, rank)] = Some(Pc {
                            color,
                            role: base_role_of(letter),
                        });
                        file += 1;
                        continue;
                    }
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
                        role: base_role_of(c),
                    });
                    file += 1;
                }
            }

            let ep = if ep_field == "-" {
                None
            } else {
                let b = ep_field.as_bytes();
                let f = (b[0] - b'a') as i32;
                let r: i32 = ep_field[1..].parse::<i32>().expect("ep rank") - 1;
                Some(idx(f, r))
            };
            Position { cells, turn, ep }
        }

        /// Non-pawn move / attack targets: reachable squares (empty or enemy), blocked
        /// by pieces and walls, never landing on a wall or own piece.
        fn piece_targets(&self, from: usize, color: u8, role: u8, out: &mut Vec<usize>) {
            let (ff, fr) = (file_of(from), rank_of(from));
            for st in steps_for(role) {
                let (df, dr) = (st.df as i32, st.dr as i32);
                if st.jump {
                    let (nf, nr) = (ff + df, fr + dr);
                    if on_board(nf, nr) && !is_wall(nf, nr) {
                        let t = idx(nf, nr);
                        if self.cells[t].is_none_or(|p| p.color != color) {
                            out.push(t);
                        }
                    }
                    continue;
                }
                let (mut nf, mut nr) = (ff + df, fr + dr);
                let mut steps = 0u8;
                while on_board(nf, nr) {
                    if is_wall(nf, nr) {
                        break; // a wall blocks the slide like an occupied square
                    }
                    let t = idx(nf, nr);
                    match self.cells[t] {
                        None => out.push(t),
                        Some(p) => {
                            if p.color != color {
                                out.push(t);
                            }
                            break;
                        }
                    }
                    steps += 1;
                    if st.range != 0 && steps >= st.range {
                        break;
                    }
                    nf += df;
                    nr += dr;
                }
            }
        }

        /// The two diagonal-forward squares a pawn of `color` attacks from `from`.
        fn pawn_attacks(color: u8, from: usize, out: &mut Vec<usize>) {
            let (ff, fr) = (file_of(from), rank_of(from));
            let dr = if color == W { 1 } else { -1 };
            for df in [-1i32, 1] {
                let (nf, nr) = (ff + df, fr + dr);
                if on_board(nf, nr) && !is_wall(nf, nr) {
                    out.push(idx(nf, nr));
                }
            }
        }

        fn attacked(&self, target: usize, by: u8) -> bool {
            let mut buf = Vec::new();
            for i in 0..self.cells.len() {
                let Some(p) = self.cells[i] else { continue };
                if p.color != by {
                    continue;
                }
                buf.clear();
                if p.role == PAWN {
                    Self::pawn_attacks(by, i, &mut buf);
                } else {
                    self.piece_targets(i, by, p.role, &mut buf);
                }
                if buf.contains(&target) {
                    return true;
                }
            }
            false
        }

        /// The last (promotion) rank for `color`: rank 9 (0-based 8) for white, rank 2
        /// (0-based 1) for black — the last rank a pawn (confined to files b..k) can
        /// reach, rank 10 / rank 1 being walls on those files.
        fn last_rank(color: u8) -> i32 {
            if color == W {
                H - 2
            } else {
                1
            }
        }
        fn start_rank(color: u8) -> i32 {
            if color == W {
                2
            } else {
                H - 3
            }
        }

        fn push_pawn_move(mv: &mut Vec<Mv>, from: usize, to: usize, color: u8) {
            if rank_of(to) == Self::last_rank(color) {
                for role in [WIZARD, CHAMPION, QUEEN, ROOK, BISHOP, KNIGHT] {
                    mv.push(Mv::Board {
                        from,
                        to,
                        promote: Some(role),
                    });
                }
            } else {
                mv.push(Mv::Board {
                    from,
                    to,
                    promote: None,
                });
            }
        }

        fn pseudo(&self) -> Vec<Mv> {
            let mut mv = Vec::new();
            let us = self.turn;
            for from in 0..self.cells.len() {
                let Some(p) = self.cells[from] else { continue };
                if p.color != us {
                    continue;
                }
                if p.role == PAWN {
                    let (ff, fr) = (file_of(from), rank_of(from));
                    let dr = if us == W { 1 } else { -1 };
                    // Single push.
                    let one_r = fr + dr;
                    if on_board(ff, one_r)
                        && !is_wall(ff, one_r)
                        && self.cells[idx(ff, one_r)].is_none()
                    {
                        Self::push_pawn_move(&mut mv, from, idx(ff, one_r), us);
                        // Double push from the start rank.
                        if fr == Self::start_rank(us) {
                            let two_r = fr + 2 * dr;
                            if on_board(ff, two_r)
                                && !is_wall(ff, two_r)
                                && self.cells[idx(ff, two_r)].is_none()
                            {
                                mv.push(Mv::Double {
                                    from,
                                    to: idx(ff, two_r),
                                    ep: idx(ff, one_r),
                                });
                            }
                        }
                    }
                    // Diagonal captures (incl. en passant).
                    for df in [-1i32, 1] {
                        let (nf, nr) = (ff + df, fr + dr);
                        if !on_board(nf, nr) || is_wall(nf, nr) {
                            continue;
                        }
                        let t = idx(nf, nr);
                        if let Some(q) = self.cells[t] {
                            if q.color != us {
                                Self::push_pawn_move(&mut mv, from, t, us);
                            }
                        } else if self.ep == Some(t) {
                            let captured = idx(nf, fr); // the pawn that double-stepped
                            mv.push(Mv::Ep {
                                from,
                                to: t,
                                captured,
                            });
                        }
                    }
                    continue;
                }
                // Non-pawn.
                let mut tg = Vec::new();
                self.piece_targets(from, us, p.role, &mut tg);
                for to in tg {
                    mv.push(Mv::Board {
                        from,
                        to,
                        promote: None,
                    });
                }
            }
            mv
        }

        fn apply(&self, m: Mv) -> Position {
            let mut p = self.clone();
            let us = self.turn;
            p.ep = None;
            match m {
                Mv::Board { from, to, promote } => {
                    let mut mover = p.cells[from].expect("mover");
                    p.cells[from] = None;
                    if let Some(role) = promote {
                        mover.role = role;
                    }
                    p.cells[to] = Some(mover);
                }
                Mv::Double { from, to, ep } => {
                    let mover = p.cells[from].expect("mover");
                    p.cells[from] = None;
                    p.cells[to] = Some(mover);
                    p.ep = Some(ep);
                }
                Mv::Ep { from, to, captured } => {
                    let mover = p.cells[from].expect("mover");
                    p.cells[from] = None;
                    p.cells[captured] = None;
                    p.cells[to] = Some(mover);
                }
            }
            p.turn = 1 - us;
            p
        }

        fn king_sq(&self, color: u8) -> Option<usize> {
            (0..self.cells.len())
                .find(|&i| matches!(self.cells[i], Some(p) if p.color == color && p.role == KING))
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
