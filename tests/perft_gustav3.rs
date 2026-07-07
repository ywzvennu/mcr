//! Gustav 3 (10x8 Amazon chess with walled-in corners) perft validation
//! (issue #585).
//!
//! Gustav 3 is Fairy-Stockfish's built-in `gustav3`, but **the FSF binary available
//! here is a non-large-board build and does not implement it** (asked for
//! `UCI_Variant gustav3` it silently stays in standard chess and returns chess-root
//! perft counts), so there is **no live FSF perft oracle**. Like the other
//! oracle-less variants (Okisaki Shogi, Yari Shogi, Wa Shogi, Alice; see
//! `docs/oracle-less-validation.md`) the pins here are therefore **rules-validated**:
//! the start-position move count is **hand-derived**, and the engine's perft is
//! cross-checked node-for-node against a fully **independent, from-scratch 10x8 move
//! generator** written in this file (its own array-based board model, move tables,
//! walls, en passant, promotion and king safety, sharing no code with the engine
//! under test). Two independent implementations agreeing on every node count is the
//! substitute for the missing engine oracle.
//!
//! ## How the start count is derived
//!
//! **perft(1) = 22**, hand-derived from the start position
//! `**arnbqkbnr**a/1pppppppp1/10/10/10/10/1PPPPPPPP1/**ARNBQKBNR**A w KQkq`:
//!
//! | Piece(s) | Moves | Reason |
//! |---|---|---|
//! | 8 Pawns (b2–i2) | 16 | each steps one or two squares forward |
//! | Knight c1 | 2 | inward leaps b3 / d3 (a2 is a wall, e2 a friendly pawn) |
//! | Knight h1 | 2 | inward leaps g3 / i3 (j2 is a wall, f2 a friendly pawn) |
//! | Amazon a1 | 1 | boxed by the a-file wall: only the knight-leap to b3 |
//! | Amazon j1 | 1 | boxed by the j-file wall: only the knight-leap to i3 |
//! | Rooks / Bishops / Queen / King | 0 | every ray is blocked at the first step |
//!
//! summing to 16 + 2 + 2 + 1 + 1 = **22**. **perft(2) = 484 = 22²**: the armies begin
//! four ranks apart, so no White first move touches any Black piece, and Black keeps
//! all 22 of its mirror-image replies whatever White plays.
//!
//! Depths 1–3 of the start position and depths 1–3 of two midgame positions
//! (exercising active Amazons near the walls, captures, en passant and promotion)
//! are produced **identically** by the engine and the independent generator below;
//! depth 4 of the start position is an engine-only regression pin (`#[ignore]`d).

use mcr::geometry::{perft as gperft, Cap10x8, Gustav3};

/// The Gustav 3 starting FEN.
const STARTPOS: &str =
    "**arnbqkbnr**a/1pppppppp1/10/10/10/10/1PPPPPPPP1/**ARNBQKBNR**A w KQkq - 0 1";

/// A midgame position (White to move) with an **en-passant** capture available
/// (`d6`: Black has just played …d7-d5 past White's e5 pawn), an active White Amazon
/// on d4 and a Black Amazon on g6, and no castling rights. Cross-checked against the
/// independent generator.
const MIDGAME_EP: &str = "5k4/10/2n3**a3/3pP5/3**A6/10/10/R4K4 w - d6 0 1";

/// A midgame position (White to move) with a **pawn one step from promotion**
/// (`c7`, which may push to c8 or capture the b8 rook, each promoting to Amazon /
/// Q / R / B / N), plus Amazons off the walls and no castling rights. Cross-checked
/// against the independent generator.
const MIDGAME_PROMO: &str = "1r5k2/2P7/10/2**a7/4**A5/1R8/6p3/3K6 w - - 0 1";

/// Engine perft pins for the start position. Depth 1 is hand-derived (module docs);
/// depths 2–3 are also reproduced by the independent generator.
const START_PINS: &[(u32, u64)] = &[(1, 22), (2, 484), (3, 12_804)];

#[test]
fn engine_startpos_perft_pins() {
    let pos = Gustav3::from_fen(STARTPOS).expect("valid Gustav 3 FEN");
    for &(depth, expected) in START_PINS {
        let got = gperft::<Cap10x8, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "engine Gustav 3 perft({depth}) mismatch (rules-validated, no FSF oracle)"
        );
    }
}

#[test]
#[ignore = "deep perft; run with --release --test perft_gustav3 -- --include-ignored"]
fn engine_startpos_perft_deep() {
    let pos = Gustav3::from_fen(STARTPOS).expect("valid Gustav 3 FEN");
    assert_eq!(gperft::<Cap10x8, _, _>(&pos, 4), 331_659);
}

#[test]
fn engine_midgame_perft_pins() {
    for (fen, pins) in [
        (MIDGAME_EP, MIDGAME_EP_PINS),
        (MIDGAME_PROMO, MIDGAME_PROMO_PINS),
    ] {
        let pos = Gustav3::from_fen(fen).expect("valid Gustav 3 FEN");
        for &(depth, expected) in pins {
            assert_eq!(
                gperft::<Cap10x8, _, _>(&pos, depth),
                expected,
                "engine midgame perft({depth}) mismatch for {fen}"
            );
        }
    }
}

const MIDGAME_EP_PINS: &[(u32, u64)] = &[(1, MG_EP_1), (2, MG_EP_2), (3, MG_EP_3)];
const MIDGAME_PROMO_PINS: &[(u32, u64)] = &[(1, MG_PR_1), (2, MG_PR_2), (3, MG_PR_3)];

// Filled from the engine and re-derived by the independent generator below.
const MG_EP_1: u64 = 35;
const MG_EP_2: u64 = 1188;
const MG_EP_3: u64 = 31_496;
const MG_PR_1: u64 = 62;
const MG_PR_2: u64 = 2108;
const MG_PR_3: u64 = 74_438;

/// The engine and the **independent** from-scratch generator must agree on every
/// node count, depths 1–3, for the start position and both midgame positions. This
/// is the no-oracle cross-validation (issue #500).
#[test]
fn engine_matches_independent_generator() {
    for fen in [STARTPOS, MIDGAME_EP, MIDGAME_PROMO] {
        let engine = Gustav3::from_fen(fen).expect("valid Gustav 3 FEN");
        let bf = brute::Position::parse(fen);
        for depth in 1..=3 {
            let e = gperft::<Cap10x8, _, _>(&engine, depth);
            let b = brute::perft(&bf, depth);
            assert_eq!(
                e, b,
                "engine vs independent Gustav 3 perft({depth}) disagree for {fen}: {e} vs {b}"
            );
        }
    }
    // The independent generator reproduces the hand-derived root count.
    assert_eq!(brute::perft(&brute::Position::parse(STARTPOS), 1), 22);
}

// --- Targeted piece-behaviour tests (engine) ------------------------------------

/// The Amazon moves as Queen + Knight. A boxed corner Amazon (a1, a2 a wall) has a
/// single opening move — the knight-leap to b3 — while a central Amazon slides.
#[test]
fn amazon_is_queen_plus_knight() {
    let pos = Gustav3::from_fen(STARTPOS).expect("valid FEN");
    let a1_moves: Vec<String> = pos
        .legal_moves()
        .iter()
        .map(|m| m.to_uci::<Cap10x8>())
        .filter(|u| u.starts_with("a1"))
        .collect();
    assert_eq!(
        a1_moves,
        vec!["a1b3".to_string()],
        "cornered amazon: only a1b3"
    );

    // A central Amazon on an open board slides far (queen) and leaps (knight): far
    // more than a lone knight's 8 targets.
    let open = Gustav3::from_fen("9k/10/10/10/4**A5/10/10/K9 w - - 0 1").expect("valid FEN");
    let n = open
        .legal_moves()
        .iter()
        .filter(|m| m.to_uci::<Cap10x8>().starts_with("e4"))
        .count();
    assert!(
        n > 8,
        "a central amazon must slide as a queen plus leap as a knight"
    );
}

/// Castling lands the king on the custom **h**- (kingside) and **d**- (queenside)
/// files, with the rook beside it toward the centre.
#[test]
fn castling_lands_on_custom_files() {
    // Kingside: king f1, rook i1; king goes to h1.
    let ks = Gustav3::from_fen("k9/10/10/10/10/10/10/5K2R1 w K - 0 1").expect("valid FEN");
    assert!(
        ks.legal_moves()
            .iter()
            .any(|m| m.to_uci::<Cap10x8>() == "f1h1"),
        "kingside castle must move the king f1->h1"
    );
    // Queenside: king f1, rook b1; king goes to d1.
    let qs = Gustav3::from_fen("k9/10/10/10/10/10/10/1R3K4 w Q - 0 1").expect("valid FEN");
    assert!(
        qs.legal_moves()
            .iter()
            .any(|m| m.to_uci::<Cap10x8>() == "f1d1"),
        "queenside castle must move the king f1->d1"
    );
}

/// No piece may stand on, land on, or slide through a wall square (a2–a7 / j2–j7).
/// A White rook on b4 sliding west is stopped by nothing on the b-file, but a rook
/// placed to slide toward the a-file cannot reach the walled a-file cells.
#[test]
fn walls_block_and_are_unreachable() {
    // A rook on d4 slides west along rank 4 and must stop at b4 — c4/b4 are open but
    // a4 is a wall, so the rook can reach b4 yet never a4.
    let pos = Gustav3::from_fen("k9/10/10/10/3R6/10/10/5K4 w - - 0 1").expect("valid FEN");
    let dests: Vec<String> = pos
        .legal_moves()
        .iter()
        .map(|m| m.to_uci::<Cap10x8>())
        .filter(|u| u.starts_with("d4"))
        .map(|u| u[2..].to_string())
        .collect();
    assert!(
        dests.contains(&"b4".to_string()),
        "rook reaches the open b4"
    );
    assert!(
        !dests.contains(&"a4".to_string()),
        "rook can never land on the a4 wall"
    );
}

/// A fully independent, naive, array-based Gustav 3 move generator and perft —
/// written from scratch (its own 10x8 position model, move tables, walls, en
/// passant, promotion and king safety, with no use of the engine under test) to
/// cross-validate the engine's perft without an external oracle. Castling is
/// **not** modelled here; every cross-checked position either has no castling rights
/// or is the start position, where castling cannot occur within three plies (the
/// engine's castling is pinned separately by the targeted tests above).
mod brute {
    const N: i32 = 10; // files
    const H: i32 = 8; // ranks
    const W: u8 = 0;
    const B: u8 = 1;

    const PAWN: u8 = 0;
    const KNIGHT: u8 = 1;
    const BISHOP: u8 = 2;
    const ROOK: u8 = 3;
    const QUEEN: u8 = 4;
    const KING: u8 = 5;
    const AMAZON: u8 = 6;

    /// A move descriptor step `(df, dr)` in absolute board orientation, repeated up
    /// to `range` squares (`0` = unlimited) as a blockable slide, or a single leap.
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
    // Amazon = queen slides + knight leaps.
    const AMAZON_STEPS: &[Step] = &[
        s(1, 0),
        s(-1, 0),
        s(0, 1),
        s(0, -1),
        s(1, 1),
        s(1, -1),
        s(-1, 1),
        s(-1, -1),
        j(1, 2),
        j(-1, 2),
        j(1, -2),
        j(-1, -2),
        j(2, 1),
        j(-2, 1),
        j(2, -1),
        j(-2, -1),
    ];

    fn steps_for(role: u8) -> &'static [Step] {
        match role {
            KNIGHT => KNIGHT_STEPS,
            BISHOP => BISHOP_STEPS,
            ROOK => ROOK_STEPS,
            QUEEN => QUEEN_STEPS,
            KING => KING_STEPS,
            AMAZON => AMAZON_STEPS,
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
    /// The permanent walls: a-/j-files (0 / 9) exist only on ranks 1 and 8, so ranks
    /// 2..=7 (0-based 1..=6) of those files are blocked.
    #[inline]
    fn is_wall(f: i32, r: i32) -> bool {
        (f == 0 || f == N - 1) && (1..=6).contains(&r)
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
            'a' => AMAZON,
            other => panic!("unexpected FEN piece letter {other}"),
        }
    }

    impl Position {
        /// Parse a Gustav 3 FEN (placement, turn, castling, ep, …). Independent of the
        /// engine's parser. The Amazon is spelled `**a`/`**A` (two `*` then the base
        /// letter); walls are implicit (rendered as empty squares).
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
                        // Overflow prefix: consume any further '*' then the base
                        // letter; this is one Amazon on one square.
                        while chars.peek() == Some(&'*') {
                            chars.next();
                        }
                        let letter = chars.next().expect("overflow role letter");
                        let color = if letter.is_ascii_uppercase() { W } else { B };
                        cells[idx(file, rank)] = Some(Pc {
                            color,
                            role: AMAZON,
                        });
                        assert_eq!(
                            base_role_of(letter),
                            AMAZON,
                            "overflow token must be amazon"
                        );
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

        /// Non-pawn move / attack targets: reachable squares (empty or enemy),
        /// blocked by pieces and walls, never landing on a wall or own piece.
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

        fn last_rank(color: u8) -> i32 {
            if color == W {
                H - 1
            } else {
                0
            }
        }
        fn start_rank(color: u8) -> i32 {
            if color == W {
                1
            } else {
                H - 2
            }
        }

        fn push_pawn_move(mv: &mut Vec<Mv>, from: usize, to: usize, color: u8) {
            if rank_of(to) == Self::last_rank(color) {
                for role in [AMAZON, QUEEN, ROOK, BISHOP, KNIGHT] {
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
