//! Wolf chess (8x10 compound + rider army) perft validation (issue #585).
//!
//! Wolf chess is Fairy-Stockfish's built-in `wolf`, but **the FSF binary available
//! here is a non-large-board build and does not implement the 10-rank board** (asked
//! for `UCI_Variant wolf` it silently stays in standard chess), so there is **no live
//! FSF perft oracle**. Like the other oracle-less variants (Okisaki Shogi, Gustav 3,
//! Wa Shogi, Alice, Tenjiku; see `docs/oracle-less-validation.md`) the pins here are
//! **rules-validated**: the start-position move count is **hand-derived**, and the
//! engine's perft is cross-checked node-for-node against a fully **independent,
//! from-scratch 8x10 move generator** written in this file (its own array-based board
//! model, move tables, double-step region, en passant, and king safety, sharing no
//! code with the engine under test). Two independent implementations agreeing on
//! every node count is the substitute for the missing engine oracle.
//!
//! ## How the start count is derived
//!
//! **perft(1) = 23**, hand-derived from the start position
//! `qwfrbbnk/pssppssp/1pp2pp1/8/8/8/8/1PP2PP1/PSSPPSSP/KNBBRFWQ w` (FSF dialect):
//!
//! | Piece(s) | Moves | Reason |
//! |---|---|---|
//! | 4 home pawns (a2, d2, e2, h2) | 8 | each: a single and a double step to an empty rank 3 / 4 |
//! | 4 advanced pawns (b3, c3, f3, g3) | 8 | each in the double-step region: a single (rank 4) and a double (rank 5) |
//! | 4 Sergeants (b2, c2, f2, g2) | 4 | each: only its one open forward diagonal (the straight and other diagonal, and the double step, are blocked by the advanced pawns) |
//! | Nightrider (b1) | 1 | rides `(-1,2)` to a3 (the `(1,2)` and `(2,1)` rides hit friendly pieces) |
//! | Fox / Archbishop (f1) | 1 | its only open knight leap, to e3 |
//! | Wolf / Chancellor (g1) | 1 | its only open knight leap, to h3 |
//! | Bishops (c1, d1), Rook (e1), Queen (h1), King (a1) | 0 | all boxed in by the pawn/sergeant wall |
//!
//! summing to 8 + 8 + 4 + 1 + 1 + 1 = **23**. **perft(2) = 529 = 23²**: the armies
//! begin four+ ranks apart, so no White first move touches any Black piece and Black
//! keeps all 23 of its mirror replies whatever White plays. Depths 2–3 (which
//! exercise the file-restricted rank-3 double step, en passant availability, and the
//! rider army) are produced **identically** by the engine and the independent
//! generator below; depth 4 is an engine-only regression pin (`#[ignore]`d).

use mcr::geometry::{perft as gperft, Wolf, Wolf8x10};

/// The Wolf starting FEN in mcr's role dialect (Wolf = `e`, Fox = `a`, Nightrider =
/// `****n`, Sergeant = `****y`).
const STARTPOS: &str = "qearbb****nk/p****y****ypp****y****yp/1pp2pp1/8/8/8/8/1PP2PP1/P****Y****YPP****Y****YP/K****NBBRAEQ w - - 0 1";

/// A crafted position exercising the promotion-only **Wolf Elephant** (`****Z`,
/// Nightrider + Queen) at d4 — its knight-ray rides and Queen slides, a capture of
/// the Black rook along rank 4 — plus a home-rank pawn double step. Cross-checked
/// against the independent generator at depths 1–3.
const WOLF_ELEPHANT_POS: &str = "7k/8/8/7p/8/8/3****Z3r/8/P7/K7 w - - 0 1";

/// Engine perft pins for the start position. Depth 1 is hand-derived (module docs);
/// depths 2–3 are cross-checked against the independent generator.
const START_PINS: &[(u32, u64)] = &[(1, 23), (2, 529), (3, 13_722)];

#[test]
fn engine_startpos_perft_pins() {
    let pos = Wolf::from_fen(STARTPOS).expect("valid Wolf FEN");
    for &(depth, expected) in START_PINS {
        let got = gperft::<Wolf8x10, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "engine Wolf perft({depth}) mismatch (rules-validated, no FSF oracle)"
        );
    }
}

#[test]
#[ignore = "deep perft; run with --release --test perft_wolf -- --include-ignored"]
fn engine_startpos_perft_deep() {
    let pos = Wolf::from_fen(STARTPOS).expect("valid Wolf FEN");
    assert_eq!(gperft::<Wolf8x10, _, _>(&pos, 4), 353_804);
}

/// The engine and the **independent** from-scratch generator must agree on every node
/// count, depths 1–3, for the start position and the Wolf-Elephant position. This is
/// the no-oracle cross-validation (issue #500 pattern).
#[test]
fn engine_matches_independent_generator() {
    for fen in [STARTPOS, WOLF_ELEPHANT_POS] {
        let engine = Wolf::from_fen(fen).expect("valid Wolf FEN");
        let bf = brute::Position::parse(fen);
        for depth in 1..=3 {
            let e = gperft::<Wolf8x10, _, _>(&engine, depth);
            let b = brute::perft(&bf, depth);
            assert_eq!(
                e, b,
                "engine vs independent Wolf perft({depth}) disagree for {fen}: {e} vs {b}"
            );
        }
    }
    // The independent generator reproduces the hand-derived root count.
    assert_eq!(brute::perft(&brute::Position::parse(STARTPOS), 1), 23);
}

// --- Targeted piece-behaviour tests (engine) ------------------------------------

/// Collect the destination-square UCI suffixes of every legal move originating from
/// `origin` (e.g. `"e5"`), stripping any promotion suffix.
fn dests_from(pos: &Wolf, origin: &str) -> Vec<String> {
    let mut v: Vec<String> = pos
        .legal_moves()
        .iter()
        .map(|m| m.to_uci::<Wolf8x10>())
        .filter(|u| u.starts_with(origin))
        .map(|u| {
            // Keep the leading destination square (`[a-h][0-9]+`), dropping any
            // promotion suffix that follows it.
            u[origin.len()..]
                .chars()
                .take_while(|c| c.is_ascii_lowercase() && *c <= 'h' || c.is_ascii_digit())
                .collect::<String>()
        })
        .collect();
    v.sort();
    v.dedup();
    v
}

/// A pawn double-steps from its home rank (rank 2) and from the four inner files
/// (b/c/f/g) on rank 3, but **not** from the outer files on rank 3: the a-pawn on a3
/// gets only a single step, while the b-pawn on b3 also gets its double step.
#[test]
fn pawn_double_step_region_is_file_restricted_on_rank_three() {
    let pos = Wolf::from_fen("7k/8/8/8/8/8/8/PP6/8/K7 w - - 0 1").expect("valid FEN");
    let a = dests_from(&pos, "a3");
    let b = dests_from(&pos, "b3");
    assert_eq!(a, vec!["a4"], "a3 (outer file) may not double-step");
    assert_eq!(b, vec!["b4", "b5"], "b3 (inner file) may double-step");
}

/// A Sergeant moves and captures on its three **forward** squares (straight ahead and
/// both forward diagonals) and, from the double-step region, also has its initial
/// two-square straight advance.
#[test]
fn sergeant_moves_forward_king_plus_initial_double() {
    // A lone White Sergeant on its home square b2 (in the double-step region): the
    // three forward steps b3/a3/c3 plus the double step b4 — four quiet moves.
    let pos = Wolf::from_fen("7k/8/8/8/8/8/8/8/1****Y6/K7 w - - 0 1").expect("valid FEN");
    let d = dests_from(&pos, "b2");
    assert_eq!(
        d,
        vec!["a3", "b3", "b4", "c3"],
        "forward King + initial double"
    );

    // Away from the region (e5) the double step is gone: only the three forward steps.
    let pos = Wolf::from_fen("7k/8/8/8/8/4****Y3/8/8/8/K7 w - - 0 1").expect("valid FEN");
    let d = dests_from(&pos, "e5");
    assert_eq!(d, vec!["d6", "e6", "f6"], "no double step off the region");
}

/// The Wolf (Chancellor) moves as a Rook **plus** a Knight; the Fox (Archbishop) as a
/// Bishop **plus** a Knight.
#[test]
fn wolf_is_rook_plus_knight_and_fox_is_bishop_plus_knight() {
    // Wolf (`E`) on d4 with the White king off its lines (h1): Rook 16 (7 rank + 9
    // file) + 8 knight leaps = 24.
    let pos = Wolf::from_fen("7k/8/8/8/8/8/3E4/8/8/7K w - - 0 1").expect("valid FEN");
    let n = pos
        .legal_moves()
        .iter()
        .filter(|m| m.to_uci::<Wolf8x10>().starts_with("d4"))
        .count();
    assert_eq!(n, 24, "Wolf = Rook (16) + Knight (8)");

    // Fox (`A`) on d4 with the king off its lines (h1): Bishop 13 (NE 4, NW 3, SE 3,
    // SW 3) + 8 knight leaps = 21.
    let pos = Wolf::from_fen("7k/8/8/8/8/8/3A4/8/8/7K w - - 0 1").expect("valid FEN");
    let n = pos
        .legal_moves()
        .iter()
        .filter(|m| m.to_uci::<Wolf8x10>().starts_with("d4"))
        .count();
    assert_eq!(n, 21, "Fox = Bishop (13) + Knight (8)");
}

/// The Nightrider rides its knight-rays until blocked, and a Nightrider check may be
/// answered by interposing on an intermediate landing square (the full-verify path).
#[test]
fn nightrider_rides_and_blocks() {
    // A White Nightrider on a1 rides the (1,2) ray b3/c5/d7/e9 and the (2,1) ray
    // c2/e3/g4 on an empty board.
    let pos = Wolf::from_fen("7k/8/8/8/8/8/8/8/8/****N6K w - - 0 1").expect("valid FEN");
    let d = dests_from(&pos, "a1");
    for sq in ["b3", "c5", "d7", "e9", "c2", "e3", "g4"] {
        assert!(
            d.contains(&sq.to_string()),
            "Nightrider a1 should ride to {sq}"
        );
    }
    // A blocker on c5 stops the (1,2) ride at c5 (a capture) — d7/e9 unreachable.
    let pos = Wolf::from_fen("7k/8/8/8/8/2p5/8/8/8/****N6K w - - 0 1").expect("valid FEN");
    let d = dests_from(&pos, "a1");
    assert!(d.contains(&"b3".to_string()));
    assert!(
        d.contains(&"c5".to_string()),
        "captures the first enemy on the ray"
    );
    assert!(
        !d.contains(&"d7".to_string()),
        "cannot ride past the blocker"
    );
}

/// The Wolf Elephant (`NNQ`) rides the knight-rays **and** slides as a Queen: on an
/// open board from d4 it has the Queen's 30 squares plus the Nightrider's 14 rides.
#[test]
fn wolf_elephant_is_nightrider_plus_queen() {
    let pos = Wolf::from_fen("7k/8/8/8/8/8/3****Z4/8/8/7K w - - 0 1").expect("valid FEN");
    let n = pos
        .legal_moves()
        .iter()
        .filter(|m| m.to_uci::<Wolf8x10>().starts_with("d4"))
        .count();
    // Queen from d4 (king off its lines): rook 16 + bishop 13 = 29; Nightrider rides
    // from d4: 3 + 3 + 1 + 1 + 2 + 1 + 2 + 1 = 14 (each knight ray until the edge).
    // Total 43.
    assert_eq!(
        n, 43,
        "Wolf Elephant = Queen (29) + Nightrider (14) from d4"
    );
}

/// A pawn reaching the last rank promotes to one of **six** targets: Queen, Wolf
/// (Chancellor), Fox (Archbishop), Rook, Bishop, or Wolf Elephant (`NNQ`).
#[test]
fn pawn_promotes_to_six_targets() {
    let pos = Wolf::from_fen("7k/P7/8/8/8/8/8/8/8/2K5 w - - 0 1").expect("valid FEN");
    let promos: Vec<mcr::geometry::WideRole> = pos
        .legal_moves()
        .iter()
        .filter(|m| m.to_uci::<Wolf8x10>().starts_with("a9a10"))
        .filter_map(|m| m.promotion())
        .collect();
    use mcr::geometry::WideRole;
    for r in [
        WideRole::Queen,
        WideRole::Elephant,
        WideRole::Hawk,
        WideRole::Rook,
        WideRole::Bishop,
        WideRole::WolfElephant,
    ] {
        assert!(
            promos.contains(&r),
            "pawn should be able to promote to {r:?}"
        );
    }
    assert_eq!(promos.len(), 6, "exactly six promotion targets");
    assert!(
        !promos.contains(&WideRole::Knight),
        "there is no Knight in this army"
    );
}

/// A fully independent, naive, array-based Wolf move generator and perft — written
/// from scratch (its own 8x10 position model, move tables, double-step region, en
/// passant, and king safety, with no use of the engine under test) to cross-validate
/// the engine's perft without an external oracle. The piece descriptors are
/// re-derived directly from the FSF Betza definitions in a uniform `(df, dr, range,
/// jump)` encoding deliberately unlike the engine's bitboards.
mod brute {
    const W: u8 = 0;
    const B: u8 = 1;
    const NF: i32 = 8; // files
    const NR: i32 = 10; // ranks

    // Own role enumeration (independent of the engine's `WideRole`).
    const PAWN: u8 = 0;
    const SERGEANT: u8 = 1;
    const NIGHT: u8 = 2; // Nightrider (NN)
    const WOLF: u8 = 3; // Chancellor (R + N)
    const FOX: u8 = 4; // Archbishop (B + N)
    const ELEPHANT: u8 = 5; // Wolf Elephant (NN + Q)
    const BISHOP: u8 = 6;
    const ROOK: u8 = 7;
    const QUEEN: u8 = 8;
    const KING: u8 = 9;

    /// A move descriptor: a single step `(df, dr)` repeated up to `range` squares
    /// (`0` = unlimited) as a blockable slide, or — when `jump` — a single leap
    /// ignoring the intervening squares.
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
    const ROOK_STEPS: &[Step] = &[s(1, 0, 0), s(-1, 0, 0), s(0, 1, 0), s(0, -1, 0)];
    const BISHOP_STEPS: &[Step] = &[s(1, 1, 0), s(1, -1, 0), s(-1, 1, 0), s(-1, -1, 0)];
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
    // Nightrider: each knight direction ridden as an unlimited blockable slide.
    const NIGHT_STEPS: &[Step] = &[
        s(1, 2, 0),
        s(-1, 2, 0),
        s(1, -2, 0),
        s(-1, -2, 0),
        s(2, 1, 0),
        s(-2, 1, 0),
        s(2, -1, 0),
        s(-2, -1, 0),
    ];
    // Wolf (Chancellor): rook slides + knight leaps.
    const WOLF_STEPS: &[Step] = &[
        s(1, 0, 0),
        s(-1, 0, 0),
        s(0, 1, 0),
        s(0, -1, 0),
        j(1, 2),
        j(-1, 2),
        j(1, -2),
        j(-1, -2),
        j(2, 1),
        j(-2, 1),
        j(2, -1),
        j(-2, -1),
    ];
    // Fox (Archbishop): bishop slides + knight leaps.
    const FOX_STEPS: &[Step] = &[
        s(1, 1, 0),
        s(1, -1, 0),
        s(-1, 1, 0),
        s(-1, -1, 0),
        j(1, 2),
        j(-1, 2),
        j(1, -2),
        j(-1, -2),
        j(2, 1),
        j(-2, 1),
        j(2, -1),
        j(-2, -1),
    ];
    // Wolf Elephant (NNQ): nightrider rides + queen slides.
    const ELEPHANT_STEPS: &[Step] = &[
        s(1, 2, 0),
        s(-1, 2, 0),
        s(1, -2, 0),
        s(-1, -2, 0),
        s(2, 1, 0),
        s(-2, 1, 0),
        s(2, -1, 0),
        s(-2, -1, 0),
        s(1, 0, 0),
        s(-1, 0, 0),
        s(0, 1, 0),
        s(0, -1, 0),
        s(1, 1, 0),
        s(1, -1, 0),
        s(-1, 1, 0),
        s(-1, -1, 0),
    ];

    /// The step table for the symmetric (non-pawn, non-sergeant) roles.
    fn steps_for(role: u8) -> &'static [Step] {
        match role {
            KING => KING_STEPS,
            ROOK => ROOK_STEPS,
            BISHOP => BISHOP_STEPS,
            QUEEN => QUEEN_STEPS,
            NIGHT => NIGHT_STEPS,
            WOLF => WOLF_STEPS,
            FOX => FOX_STEPS,
            ELEPHANT => ELEPHANT_STEPS,
            _ => &[],
        }
    }

    fn last_rank(color: u8) -> i32 {
        if color == W {
            NR - 1
        } else {
            0
        }
    }
    fn forward(color: u8) -> i32 {
        if color == W {
            1
        } else {
            -1
        }
    }
    /// The double-step region: the home rank, plus the inner b/c/f/g files one rank
    /// forward.
    fn in_region(color: u8, f: i32, r: i32) -> bool {
        let inner = matches!(f, 1 | 2 | 5 | 6);
        if color == W {
            r == 1 || (r == 2 && inner)
        } else {
            r == NR - 2 || (r == NR - 3 && inner)
        }
    }

    /// The role of a FEN letter (base letter for a `****`-prefixed token already
    /// stripped by the caller); `prefixed` distinguishes the overflow roles.
    fn role_of(letter: char, prefixed: bool) -> u8 {
        match (letter.to_ascii_lowercase(), prefixed) {
            ('n', true) => NIGHT,
            ('y', true) => SERGEANT,
            ('z', true) => ELEPHANT,
            ('p', false) => PAWN,
            ('e', false) => WOLF, // Wolf = Rook+Knight Elephant, FEN letter `e`
            ('a', false) => FOX,  // Fox = Bishop+Knight Hawk, FEN letter `a`
            ('b', false) => BISHOP,
            ('r', false) => ROOK,
            ('q', false) => QUEEN,
            ('k', false) => KING,
            other => panic!("unexpected FEN piece token {other:?}"),
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    struct Pc {
        color: u8,
        role: u8,
    }

    #[derive(Clone)]
    pub(crate) struct Position {
        cells: [Option<Pc>; (NF * NR) as usize],
        turn: u8,
        /// The square a pawn may capture onto by en passant (the skipped square).
        ep: Option<usize>,
    }

    #[inline]
    fn idx(f: i32, r: i32) -> usize {
        (r * NF + f) as usize
    }
    #[inline]
    fn file_of(i: usize) -> i32 {
        (i as i32) % NF
    }
    #[inline]
    fn rank_of(i: usize) -> i32 {
        (i as i32) / NF
    }
    #[inline]
    fn on_board(f: i32, r: i32) -> bool {
        (0..NF).contains(&f) && (0..NR).contains(&r)
    }

    #[derive(Clone, Copy)]
    enum Flag {
        Normal,
        Double,
        Ep,
        Promo(u8),
    }

    #[derive(Clone, Copy)]
    struct Mv {
        from: usize,
        to: usize,
        flag: Flag,
    }

    impl Position {
        /// Parse a Wolf FEN (mcr dialect). Independent of the engine's parser.
        pub(crate) fn parse(fen: &str) -> Position {
            let mut cells = [None; (NF * NR) as usize];
            let placement = fen.split(' ').next().expect("placement");
            let turn = if fen.contains(" b ") { B } else { W };
            for (row, line) in placement.trim().split('/').enumerate() {
                let rank = (NR - 1) - row as i32;
                let mut file = 0i32;
                let mut chars = line.chars().peekable();
                while let Some(c) = chars.next() {
                    if c == '*' {
                        // consume the remaining prefix stars, then the base letter.
                        while chars.peek() == Some(&'*') {
                            chars.next();
                        }
                        let letter = chars.next().expect("base letter after `*` prefix");
                        let color = if letter.is_ascii_uppercase() { W } else { B };
                        cells[idx(file, rank)] = Some(Pc {
                            color,
                            role: role_of(letter, true),
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
                        role: role_of(c, false),
                    });
                    file += 1;
                }
            }
            // The test FENs all use `-` for the ep field; start with no ep target.
            Position {
                cells,
                turn,
                ep: None,
            }
        }

        /// Move/capture destinations for a symmetric (slider/leaper) role.
        fn sym_targets(&self, from: usize, color: u8, role: u8, out: &mut Vec<usize>) {
            let (ff, fr) = (file_of(from), rank_of(from));
            for st in steps_for(role) {
                if st.jump {
                    let (nf, nr) = (ff + st.df as i32, fr + st.dr as i32);
                    if on_board(nf, nr) {
                        let t = idx(nf, nr);
                        if self.cells[t].is_none_or(|p| p.color != color) {
                            out.push(t);
                        }
                    }
                    continue;
                }
                let (mut nf, mut nr) = (ff + st.df as i32, fr + st.dr as i32);
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
                    nf += st.df as i32;
                    nr += st.dr as i32;
                }
            }
        }

        /// The squares a piece **attacks** (for check detection) — capture squares
        /// only. Pawns attack their two forward diagonals; Sergeants their three
        /// forward King squares; everyone else attacks where they move.
        fn attacks_into(&self, from: usize, color: u8, role: u8, out: &mut Vec<usize>) {
            let (ff, fr) = (file_of(from), rank_of(from));
            match role {
                PAWN => {
                    let fwd = forward(color);
                    for df in [-1i32, 1] {
                        let (nf, nr) = (ff + df, fr + fwd);
                        if on_board(nf, nr) {
                            out.push(idx(nf, nr));
                        }
                    }
                }
                SERGEANT => {
                    let fwd = forward(color);
                    for df in [-1i32, 0, 1] {
                        let (nf, nr) = (ff + df, fr + fwd);
                        if on_board(nf, nr) {
                            out.push(idx(nf, nr));
                        }
                    }
                }
                _ => self.sym_targets(from, color, role, out),
            }
        }

        fn attacked(&self, target: usize, by: u8) -> bool {
            let mut buf = Vec::new();
            for i in 0..self.cells.len() {
                if let Some(p) = self.cells[i] {
                    if p.color == by {
                        buf.clear();
                        self.attacks_into(i, by, p.role, &mut buf);
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
            let fwd = forward(us);
            for from in 0..self.cells.len() {
                let Some(p) = self.cells[from] else { continue };
                if p.color != us {
                    continue;
                }
                let (ff, fr) = (file_of(from), rank_of(from));
                match p.role {
                    PAWN => {
                        // Single push.
                        let one_r = fr + fwd;
                        if on_board(ff, one_r) && self.cells[idx(ff, one_r)].is_none() {
                            let to = idx(ff, one_r);
                            if one_r == last_rank(us) {
                                for r in [QUEEN, WOLF, FOX, ROOK, BISHOP, ELEPHANT] {
                                    mv.push(Mv {
                                        from,
                                        to,
                                        flag: Flag::Promo(r),
                                    });
                                }
                            } else {
                                mv.push(Mv {
                                    from,
                                    to,
                                    flag: Flag::Normal,
                                });
                                // Double push.
                                let two_r = fr + 2 * fwd;
                                if in_region(us, ff, fr)
                                    && on_board(ff, two_r)
                                    && self.cells[idx(ff, two_r)].is_none()
                                {
                                    mv.push(Mv {
                                        from,
                                        to: idx(ff, two_r),
                                        flag: Flag::Double,
                                    });
                                }
                            }
                        }
                        // Diagonal captures + en passant.
                        for df in [-1i32, 1] {
                            let (nf, nr) = (ff + df, fr + fwd);
                            if !on_board(nf, nr) {
                                continue;
                            }
                            let to = idx(nf, nr);
                            let enemy = matches!(self.cells[to], Some(q) if q.color != us);
                            if enemy {
                                if nr == last_rank(us) {
                                    for r in [QUEEN, WOLF, FOX, ROOK, BISHOP, ELEPHANT] {
                                        mv.push(Mv {
                                            from,
                                            to,
                                            flag: Flag::Promo(r),
                                        });
                                    }
                                } else {
                                    mv.push(Mv {
                                        from,
                                        to,
                                        flag: Flag::Normal,
                                    });
                                }
                            } else if self.ep == Some(to) {
                                mv.push(Mv {
                                    from,
                                    to,
                                    flag: Flag::Ep,
                                });
                            }
                        }
                    }
                    SERGEANT => {
                        for df in [-1i32, 0, 1] {
                            let (nf, nr) = (ff + df, fr + fwd);
                            if on_board(nf, nr)
                                && self.cells[idx(nf, nr)].is_none_or(|q| q.color != us)
                            {
                                mv.push(Mv {
                                    from,
                                    to: idx(nf, nr),
                                    flag: Flag::Normal,
                                });
                            }
                        }
                        // Initial double step (straight, both squares empty).
                        let two_r = fr + 2 * fwd;
                        if in_region(us, ff, fr)
                            && on_board(ff, fr + fwd)
                            && self.cells[idx(ff, fr + fwd)].is_none()
                            && on_board(ff, two_r)
                            && self.cells[idx(ff, two_r)].is_none()
                        {
                            mv.push(Mv {
                                from,
                                to: idx(ff, two_r),
                                flag: Flag::Normal,
                            });
                        }
                    }
                    role => {
                        let mut tg = Vec::new();
                        self.sym_targets(from, us, role, &mut tg);
                        for to in tg {
                            mv.push(Mv {
                                from,
                                to,
                                flag: Flag::Normal,
                            });
                        }
                    }
                }
            }
            mv
        }

        fn apply(&self, m: Mv) -> Position {
            let mut p = self.clone();
            let us = self.turn;
            let mut mover = p.cells[m.from].expect("mover");
            p.cells[m.from] = None;
            p.ep = None;
            match m.flag {
                Flag::Normal => {}
                Flag::Double => {
                    // The skipped square becomes the en-passant target.
                    let mid = idx(file_of(m.from), (rank_of(m.from) + rank_of(m.to)) / 2);
                    p.ep = Some(mid);
                }
                Flag::Ep => {
                    // Remove the pawn that double-stepped past (same file as the
                    // destination, on the moving pawn's origin rank).
                    let cap = idx(file_of(m.to), rank_of(m.from));
                    p.cells[cap] = None;
                }
                Flag::Promo(r) => mover.role = r,
            }
            p.cells[m.to] = Some(mover);
            let _ = us;
            p.turn = 1 - self.turn;
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
