//! Okisaki Shogi (王妃将棋, 10x10 Shogi with a Queen and a vertical rook) perft
//! validation (issue #584).
//!
//! Okisaki Shogi is Fairy-Stockfish's built-in `okisakishogi`, but **the FSF binary
//! available here is a non-large-board build and does not implement it** (asked for
//! `UCI_Variant okisakishogi` it silently stays in standard chess), so there is **no
//! live FSF perft oracle**. Like the other oracle-less variants (Wa Shogi, Alice,
//! Tenjiku; see `docs/oracle-less-validation.md`) the pins here are therefore
//! **rules-validated**: the start-position move count is **hand-derived**, and the
//! engine's perft is cross-checked node-for-node against a fully **independent,
//! from-scratch 10x10 move generator** written in this file (its own array-based
//! board model, move tables, hand/drops, nifu, per-piece promotion and king safety,
//! sharing no code with the engine under test). Two independent implementations
//! agreeing on every node count is the substitute for the missing engine oracle.
//!
//! ## How the start count is derived
//!
//! **perft(1) = 37**, hand-derived from the start position
//! `lnsgkqgsnl/1r6b1/pppppppppp/10/10/10/10/PPPPPPPPPP/1B6R1/LNSGQKGSNL[-] w`:
//!
//! | Piece(s) | Moves | Reason |
//! |---|---|---|
//! | 10 Pawns (a3–j3) | 10 | each steps one square forward to an empty rank 4 |
//! | 2 vertical rooks (a1, j1) | 2 | each slides one square up its file to a2/j2 (own pawn blocks beyond; nothing below) |
//! | 2 Knights (b1, i1) | 2 | the chess knight's only unblocked leap is inward to d2 / g2 |
//! | 2 Silvers (c1, h1) | 4 | two each (a forward diagonal + the straight-forward step; the other diagonal is a friendly piece) |
//! | 2 Golds (d1, g1) | 6 | three each (the straight-forward step and both forward diagonals) |
//! | Queen (e1) | 3 | e2, and the two forward diagonals d2 / f2 (all four sideways/backward rays are blocked or off-board) |
//! | Bishop (b2) | 0 | boxed in — all four diagonals meet a friendly piece |
//! | Rook (i2) | 7 | six squares west along rank 2 (stops at the b2 bishop) plus one east to j2 |
//! | King (f1) | 3 | e2, f2, g2 |
//!
//! summing to 10 + 2 + 2 + 4 + 6 + 3 + 0 + 7 + 3 = **37**. There are no promotions
//! (nothing reaches the far three ranks in one move) and no drops (the hand is
//! empty) at the root. **perft(2) = 1369 = 37²**: the armies begin four ranks
//! apart, so no White first move touches any Black piece, and Black keeps all 37 of
//! its mirror-image replies whatever White plays.
//!
//! Depths 2–3 of the start position and depths 1–3 of two midgame positions are
//! produced **identically** by the engine and the independent generator below;
//! depth 4 of the start position is an engine-only regression pin (`#[ignore]`d).

use mcr::geometry::{perft as gperft, Grand10x10, OkisakiShogi};

/// The Okisaki Shogi starting FEN (empty hand).
const STARTPOS: &str =
    "lnsgkqgsnl/1r6b1/pppppppppp/10/10/10/10/PPPPPPPPPP/1B6R1/LNSGQKGSNL[] w - - 0 1";

/// A midgame position (Black to move) exercising the promoted Dragon Horse (`+b`),
/// the Queen, the vertical rooks, and — because every file already carries a Black
/// pawn — **nifu** fully suppressing the held pawn's drops (0 legal drops). Reached
/// by a seeded self-play line; the counts are cross-checked against the independent
/// generator below.
const MIDGAME_NIFU: &str =
    "ln1gks2n1/2rs3g1l/p1p1p3pp/1p1p1p1p2/3q2p3/PP1P2P3/L3P3PP/2+bG1P1P2/1B2QGS1RL/1NS2K2N1[p] b - - 0 22";

/// A midgame position (White to move) with a **Queen in hand**, exercising drops in
/// perft (58 legal drops at the root) alongside board play. Also cross-checked
/// against the independent generator.
const MIDGAME_DROPS: &str =
    "l1sg1sg1nl/2r1k3b1/pp3pppp1/n1p1p4p/3p6/6P3/PPPPP2S2/L4Q1PPP/1B1SR5/3GNK1GNL[Qp] w - - 0 24";

/// Engine perft pins for the start position. Depth 1 is hand-derived (module docs);
/// depths 2–3 are cross-checked against the independent generator.
const START_PINS: &[(u32, u64)] = &[(1, 37), (2, 1369), (3, 48_211)];

#[test]
fn engine_startpos_perft_pins() {
    let pos = OkisakiShogi::from_fen(STARTPOS).expect("valid Okisaki Shogi FEN");
    for &(depth, expected) in START_PINS {
        let got = gperft::<Grand10x10, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "engine Okisaki Shogi perft({depth}) mismatch (rules-validated, no FSF oracle)"
        );
    }
}

#[test]
#[ignore = "deep perft; run with --release --test perft_okisakishogi -- --include-ignored"]
fn engine_startpos_perft_deep() {
    let pos = OkisakiShogi::from_fen(STARTPOS).expect("valid Okisaki Shogi FEN");
    assert_eq!(gperft::<Grand10x10, _, _>(&pos, 4), 1_697_913);
}

#[test]
fn engine_midgame_perft_pins() {
    for (fen, pins) in [
        (MIDGAME_NIFU, [(1u32, 67u64), (2, 2705), (3, 171_193)]),
        (MIDGAME_DROPS, [(1, 114), (2, 4135), (3, 369_450)]),
    ] {
        let pos = OkisakiShogi::from_fen(fen).expect("valid Okisaki Shogi FEN");
        for (depth, expected) in pins {
            assert_eq!(
                gperft::<Grand10x10, _, _>(&pos, depth),
                expected,
                "engine midgame perft({depth}) mismatch for {fen}"
            );
        }
    }
}

/// The engine and the **independent** from-scratch generator must agree on every
/// node count, depths 1–3, for the start position and both midgame positions. This
/// is the no-oracle cross-validation (issue #500).
#[test]
fn engine_matches_independent_generator() {
    for fen in [STARTPOS, MIDGAME_NIFU, MIDGAME_DROPS] {
        let engine = OkisakiShogi::from_fen(fen).expect("valid Okisaki Shogi FEN");
        let bf = brute::Position::parse(fen);
        for depth in 1..=3 {
            let e = gperft::<Grand10x10, _, _>(&engine, depth);
            let b = brute::perft(&bf, depth);
            assert_eq!(
                e, b,
                "engine vs independent Okisaki perft({depth}) disagree for {fen}: {e} vs {b}"
            );
        }
    }
    // The independent generator reproduces the hand-derived root count.
    assert_eq!(brute::perft(&brute::Position::parse(STARTPOS), 1), 37);
}

// --- Targeted piece-behaviour tests (engine) ------------------------------------

/// The vertical rook (`l`) slides its whole **file, both directions**, and never
/// sideways or diagonally. A lone White vertical rook on e5 reaches every other
/// square of the e-file (e1–e4 downward, e6–e10 upward) and no other file.
#[test]
fn vertical_rook_slides_full_file_both_ways() {
    let pos =
        OkisakiShogi::from_fen("9k/10/10/10/10/4L5/10/10/10/K9[] w - - 0 1").expect("valid FEN");
    let mut dests: Vec<String> = pos
        .legal_moves()
        .iter()
        .map(|m| m.to_uci::<Grand10x10>())
        .filter(|u| u.starts_with("e5"))
        // strip the origin and any promotion suffix, keep the destination square.
        .map(|u| u[2..].trim_end_matches("+l").to_string())
        .collect();
    dests.sort();
    dests.dedup();
    assert_eq!(
        dests,
        vec!["e1", "e10", "e2", "e3", "e4", "e6", "e7", "e8", "e9"],
        "vertical rook must reach exactly its own file in both directions"
    );
    // Every e5 move stays on the e-file — no sideways / diagonal move exists.
    assert!(pos
        .legal_moves()
        .iter()
        .map(|m| m.to_uci::<Grand10x10>())
        .filter(|u| u.starts_with("e5"))
        .all(|u| u.as_bytes()[2] == b'e'));
}

/// The Queen moves as an orthodox chess queen. On an otherwise-empty board (kings in
/// opposite corners, the a1 king blocking one diagonal ray) a Queen on e5 has 34
/// moves: 9 + 9 rook squares and 16 bishop squares.
#[test]
fn queen_moves_as_a_chess_queen() {
    let pos =
        OkisakiShogi::from_fen("9k/10/10/10/10/4Q5/10/10/10/K9[] w - - 0 1").expect("valid FEN");
    let n = pos
        .legal_moves()
        .iter()
        .filter(|m| m.to_uci::<Grand10x10>().starts_with("e5"))
        .count();
    assert_eq!(n, 34, "queen on e5 should have 34 moves");
}

/// A Pawn reaching the last rank is **force-promoted** (the only e9→e10 move carries
/// the promotion), while a drop from hand places a fresh unpromoted piece.
#[test]
fn forced_pawn_promotion_and_a_drop() {
    // Forced promotion: a lone White pawn on e9 has exactly one move, e9e10 promoting.
    let promo =
        OkisakiShogi::from_fen("9k/4P5/10/10/10/10/10/10/10/K9[] w - - 0 1").expect("valid FEN");
    let pawn_moves: Vec<String> = promo
        .legal_moves()
        .iter()
        .map(|m| m.to_uci::<Grand10x10>())
        .filter(|u| u.starts_with("e9"))
        .collect();
    assert_eq!(
        pawn_moves,
        vec!["e9e10+p".to_string()],
        "pawn must promote on the last rank"
    );

    // A drop: White holds a Gold; on a near-empty board it can be dropped onto many
    // empty squares. At least one drop is legal and lands a Gold from hand.
    let drop =
        OkisakiShogi::from_fen("9k/10/10/10/10/10/10/10/10/K8G[G] w - - 0 1").expect("valid FEN");
    let drops: Vec<String> = drop
        .legal_moves()
        .iter()
        .map(|m| m.to_uci::<Grand10x10>())
        .filter(|u| u.contains('@'))
        .collect();
    assert!(
        !drops.is_empty(),
        "a held Gold must be droppable onto empty squares"
    );
    assert!(
        drops.iter().all(|u| u.starts_with("G@")),
        "drops must be Gold drops"
    );
}

/// A fully independent, naive, array-based Okisaki Shogi move generator and perft —
/// written from scratch (its own 10x10 position model, move tables, hand, drops,
/// nifu, per-piece promotion and king safety, with no use of the engine under test)
/// to cross-validate the engine's perft without an external oracle. The piece move
/// descriptors are re-derived directly from the piece definitions in a uniform
/// `(df, dr, range, jump)` encoding deliberately unlike the engine's bitboards.
mod brute {
    const W: u8 = 0;
    const B: u8 = 1;
    const N: i32 = 10; // both width and height

    // Own role enumeration (independent of the engine's `WideRole`).
    const PAWN: u8 = 0;
    const SILVER: u8 = 1;
    const GOLD: u8 = 2;
    const KNIGHT: u8 = 3; // ordinary chess knight
    const VROOK: u8 = 4; // vertical rook (`vR`)
    const BISHOP: u8 = 5;
    const ROOK: u8 = 6;
    const QUEEN: u8 = 7;
    const KING: u8 = 8;
    const PPAWN: u8 = 9; // +P (Tokin), moves as Gold
    const PSILVER: u8 = 10; // +S
    const PKNIGHT: u8 = 11; // +N
    const PVROOK: u8 = 12; // +L
    const DHORSE: u8 = 13; // +B (Dragon Horse)
    const DRAGON: u8 = 14; // +R (Dragon King)

    /// A move descriptor: a single step `(df, dr)` in White orientation, repeated up
    /// to `range` squares (`0` = unlimited) as a blockable slide, or — when `jump` —
    /// a single leap to `(df, dr)` ignoring the intervening square.
    #[derive(Clone, Copy)]
    struct Step {
        df: i8,
        dr: i8,
        range: u8,
        jump: bool,
    }
    const fn s(df: i8, dr: i8, range: u8) -> Step {
        Step {
            df,
            dr,
            range,
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

    const GOLD_STEPS: &[Step] = &[
        s(1, 0, 1),
        s(-1, 0, 1),
        s(0, 1, 1),
        s(0, -1, 1),
        s(1, 1, 1),
        s(-1, 1, 1),
    ];
    const SILVER_STEPS: &[Step] = &[
        s(1, 1, 1),
        s(1, -1, 1),
        s(-1, 1, 1),
        s(-1, -1, 1),
        s(0, 1, 1),
    ];
    const PAWN_STEPS: &[Step] = &[s(0, 1, 1)];
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
    const VROOK_STEPS: &[Step] = &[s(0, 1, 0), s(0, -1, 0)];
    const BISHOP_STEPS: &[Step] = &[s(1, 1, 0), s(1, -1, 0), s(-1, 1, 0), s(-1, -1, 0)];
    const ROOK_STEPS: &[Step] = &[s(1, 0, 0), s(-1, 0, 0), s(0, 1, 0), s(0, -1, 0)];
    const QUEEN_STEPS: &[Step] = &[
        s(1, 0, 0),
        s(-1, 0, 0),
        s(0, 1, 0),
        s(0, -1, 0),
        s(1, 1, 0),
        s(1, -1, 0),
        s(-1, 1, 0),
        s(-1, -1, 0),
    ];
    const KING_STEPS: &[Step] = &[
        s(1, 0, 1),
        s(-1, 0, 1),
        s(0, 1, 1),
        s(0, -1, 1),
        s(1, 1, 1),
        s(1, -1, 1),
        s(-1, 1, 1),
        s(-1, -1, 1),
    ];
    // Dragon Horse (+B): bishop slides + one orthogonal step.
    const DHORSE_STEPS: &[Step] = &[
        s(1, 1, 0),
        s(1, -1, 0),
        s(-1, 1, 0),
        s(-1, -1, 0),
        s(1, 0, 1),
        s(-1, 0, 1),
        s(0, 1, 1),
        s(0, -1, 1),
    ];
    // Dragon King (+R): rook slides + one diagonal step.
    const DRAGON_STEPS: &[Step] = &[
        s(1, 0, 0),
        s(-1, 0, 0),
        s(0, 1, 0),
        s(0, -1, 0),
        s(1, 1, 1),
        s(1, -1, 1),
        s(-1, 1, 1),
        s(-1, -1, 1),
    ];

    fn steps_for(role: u8) -> &'static [Step] {
        match role {
            PAWN => PAWN_STEPS,
            SILVER => SILVER_STEPS,
            GOLD | PPAWN | PSILVER | PKNIGHT | PVROOK => GOLD_STEPS,
            KNIGHT => KNIGHT_STEPS,
            VROOK => VROOK_STEPS,
            BISHOP => BISHOP_STEPS,
            ROOK => ROOK_STEPS,
            QUEEN => QUEEN_STEPS,
            KING => KING_STEPS,
            DHORSE => DHORSE_STEPS,
            DRAGON => DRAGON_STEPS,
            _ => &[],
        }
    }

    fn can_promote(role: u8) -> bool {
        matches!(role, PAWN | SILVER | KNIGHT | VROOK | BISHOP | ROOK)
    }
    fn promoted(role: u8) -> u8 {
        match role {
            PAWN => PPAWN,
            SILVER => PSILVER,
            KNIGHT => PKNIGHT,
            VROOK => PVROOK,
            BISHOP => DHORSE,
            ROOK => DRAGON,
            other => other,
        }
    }
    fn hand_base(role: u8) -> u8 {
        match role {
            PPAWN => PAWN,
            PSILVER => SILVER,
            PKNIGHT => KNIGHT,
            PVROOK => VROOK,
            DHORSE => BISHOP,
            DRAGON => ROOK,
            other => other,
        }
    }
    fn last_rank(color: u8) -> i32 {
        if color == W {
            N - 1
        } else {
            0
        }
    }
    /// Forced promotion: only a Pawn reaching the last rank (it would be immobile).
    fn forced(role: u8, color: u8, to_rank: i32) -> bool {
        role == PAWN && to_rank == last_rank(color)
    }
    fn in_zone(color: u8, rank: i32) -> bool {
        if color == W {
            rank >= N - 3
        } else {
            rank < 3
        }
    }

    /// The role and colour of a FEN letter (`+` handled by the caller for promotion).
    fn base_role_of(letter: char) -> u8 {
        match letter.to_ascii_lowercase() {
            'p' => PAWN,
            's' => SILVER,
            'g' => GOLD,
            'n' => KNIGHT,
            'l' => VROOK,
            'b' => BISHOP,
            'r' => ROOK,
            'q' => QUEEN,
            'k' => KING,
            other => panic!("unexpected FEN piece letter {other}"),
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    struct Pc {
        color: u8,
        role: u8,
    }

    #[derive(Clone)]
    pub(crate) struct Position {
        cells: [Option<Pc>; (N * N) as usize],
        turn: u8,
        // Hand counts per [color][base role 0..=7].
        hand: [[u8; 8]; 2],
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
        (0..N).contains(&f) && (0..N).contains(&r)
    }

    #[derive(Clone, Copy)]
    enum Mv {
        Board {
            from: usize,
            to: usize,
            promote: bool,
        },
        Drop {
            to: usize,
            role: u8,
        },
    }

    impl Position {
        /// Parse an Okisaki Shogi FEN (placement `[hand]` turn …). Independent of the
        /// engine's parser.
        pub(crate) fn parse(fen: &str) -> Position {
            let mut cells = [None; (N * N) as usize];
            let placement = fen.split('[').next().expect("placement");
            let after = &fen[placement.len()..];
            // hand is between the first '[' and ']'.
            let hand_str = after
                .split('[')
                .nth(1)
                .and_then(|x| x.split(']').next())
                .unwrap_or("");
            let turn = if after.contains(" b ") { B } else { W };

            // Placement: rows top (rank 9) to bottom (rank 0), files a..j left to right.
            for (row, line) in placement.trim().split('/').enumerate() {
                let rank = (N - 1) - row as i32;
                let mut file = 0i32;
                let mut chars = line.chars().peekable();
                let mut promoted_next = false;
                while let Some(c) = chars.next() {
                    if c == '+' {
                        promoted_next = true;
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
                    let mut role = base_role_of(c);
                    if promoted_next {
                        role = promoted(role);
                        promoted_next = false;
                    }
                    cells[idx(file, rank)] = Some(Pc { color, role });
                    file += 1;
                }
            }

            let mut hand = [[0u8; 8]; 2];
            for c in hand_str.chars() {
                if c == '-' {
                    continue;
                }
                let color = if c.is_ascii_uppercase() { W } else { B };
                let role = base_role_of(c);
                hand[color as usize][role as usize] += 1;
            }
            Position { cells, turn, hand }
        }

        #[inline]
        fn orient(color: u8, df: i8, dr: i8) -> (i32, i32) {
            if color == W {
                (df as i32, dr as i32)
            } else {
                (df as i32, -(dr as i32))
            }
        }

        fn targets(&self, from: usize, color: u8, role: u8, out: &mut Vec<usize>) {
            let (ff, fr) = (file_of(from), rank_of(from));
            for st in steps_for(role) {
                let (df, dr) = Self::orient(color, st.df, st.dr);
                if st.jump {
                    let (nf, nr) = (ff + df, fr + dr);
                    if on_board(nf, nr) {
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

        fn attacked(&self, target: usize, by: u8) -> bool {
            let mut buf = Vec::new();
            for i in 0..self.cells.len() {
                if let Some(p) = self.cells[i] {
                    if p.color == by {
                        buf.clear();
                        self.targets(i, by, p.role, &mut buf);
                        if buf.contains(&target) {
                            return true;
                        }
                    }
                }
            }
            false
        }

        fn pseudo(&self) -> Vec<Mv> {
            let mut mv = Vec::new();
            let us = self.turn;
            for from in 0..self.cells.len() {
                let Some(p) = self.cells[from] else { continue };
                if p.color != us {
                    continue;
                }
                let mut tg = Vec::new();
                self.targets(from, us, p.role, &mut tg);
                for to in tg {
                    let to_r = rank_of(to);
                    let zone = in_zone(us, to_r) || in_zone(us, rank_of(from));
                    if can_promote(p.role) && zone {
                        if forced(p.role, us, to_r) {
                            mv.push(Mv::Board {
                                from,
                                to,
                                promote: true,
                            });
                        } else {
                            mv.push(Mv::Board {
                                from,
                                to,
                                promote: true,
                            });
                            mv.push(Mv::Board {
                                from,
                                to,
                                promote: false,
                            });
                        }
                    } else {
                        mv.push(Mv::Board {
                            from,
                            to,
                            promote: false,
                        });
                    }
                }
            }
            // Drops (base roles 0..=7; the King is never in hand).
            for role in PAWN..=QUEEN {
                if self.hand[us as usize][role as usize] == 0 {
                    continue;
                }
                for to in 0..self.cells.len() {
                    if self.cells[to].is_some() {
                        continue;
                    }
                    if role == PAWN {
                        // Dead-piece rule + nifu.
                        if rank_of(to) == last_rank(us) {
                            continue;
                        }
                        let file = file_of(to);
                        let nifu = (0..self.cells.len()).any(|i| {
                            file_of(i) == file
                                && matches!(self.cells[i], Some(q) if q.color == us && q.role == PAWN)
                        });
                        if nifu {
                            continue;
                        }
                    }
                    mv.push(Mv::Drop { to, role });
                }
            }
            mv
        }

        fn apply(&self, m: Mv) -> Position {
            let mut p = self.clone();
            let us = self.turn;
            match m {
                Mv::Board { from, to, promote } => {
                    let mut mover = p.cells[from].expect("mover");
                    p.cells[from] = None;
                    if let Some(victim) = p.cells[to] {
                        p.hand[us as usize][hand_base(victim.role) as usize] += 1;
                    }
                    if promote {
                        mover.role = promoted(mover.role);
                    }
                    p.cells[to] = Some(mover);
                }
                Mv::Drop { to, role } => {
                    p.hand[us as usize][role as usize] -= 1;
                    p.cells[to] = Some(Pc { color: us, role });
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
