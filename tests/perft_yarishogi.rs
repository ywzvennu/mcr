//! Yari Shogi ("spear shogi", 9x7) perft validation on the generic engine
//! (issue #584).
//!
//! Yari Shogi has **no live perft oracle**: although Fairy-Stockfish defines a
//! `yarishogi` variant, the project's built FSF binary is compiled without large
//! boards, so it cannot host the 9-rank board (`setoption UCI_Variant yarishogi`
//! silently falls back to chess), and HaChu has no Yari Shogi. The pins here are
//! therefore **rules-validated (no engine perft oracle); perft pins derived from
//! the documented Yari ruleset** (the FSF `yarishogi_variant()` definition —
//! spear pieces `frlR` / `fRffN` / `fFfR` / `WfFbR` / `fKbR`, a Shogi hand with
//! drops, nifu, no uchifuzume, and a far-three-rank promotion zone) and
//! cross-checked by a fully **independent brute-force Yari move generator** written
//! from scratch in this file (a separate array-based 7x9 position model with its
//! own naive movegen, hand/drops, per-piece promotion and King safety). Two
//! independent implementations agreeing on every node count to depth 4 is the
//! substitute for the missing oracle, alongside the crate-wide colour-symmetry,
//! make/unmake, and attacker-consistency invariants.
//!
//! ## How the shallow numbers are derived
//!
//! * **perft(1) = 20.** From the start array the two armies are four ranks apart
//!   (White on ranks 1 and 3, Black on ranks 7 and 9), so there is no contact at
//!   the root. White's moves are: the seven Shogi Pawns one step forward (7); each
//!   Yari Rook one step up its file (the sideways slide is blocked by a friendly
//!   piece, the back rank is full) (2); each Yari Bishop its two forward diagonals
//!   plus one forward file step (6); the King's three forward steps (3); and each
//!   Yari Knight one forward file step — its two `ffN` knight leaps both land on
//!   friendly third-rank Pawns, so are blocked (2). 7 + 2 + 6 + 3 + 2 = 20,
//!   reproduced node-for-node by the independent brute force below.
//! * **perft(2..4)** are produced **identically** by the engine and the
//!   independent generator (the no-oracle cross-validation); the deepest layer is
//!   `#[ignore]`d.

use mcr::geometry::{
    perft as gperft, Bitboard, Square, WideRole, WideVariant, Yari, YariRules, YariShogi7x9,
};
use mcr::Color;

/// The sorted `(file, rank)` target squares of `role`/`color` standing at `(f, r)`
/// with the given `occ` blockers, read straight from the variant's `role_attacks`.
fn attack_set(role: WideRole, color: Color, (f, r): (u8, u8), occ: &[(u8, u8)]) -> Vec<(u8, u8)> {
    let sq = Square::<YariShogi7x9>::from_file_rank(f, r).unwrap();
    let mut occb = Bitboard::<YariShogi7x9>::EMPTY;
    for &(af, ar) in occ {
        occb.set(Square::<YariShogi7x9>::from_file_rank(af, ar).unwrap());
    }
    let bb = <YariRules as WideVariant<YariShogi7x9>>::role_attacks(role, color, sq, occb);
    let mut v: Vec<(u8, u8)> = bb.into_iter().map(|s| (s.file(), s.rank())).collect();
    v.sort_unstable();
    v
}

fn sorted(mut v: Vec<(u8, u8)>) -> Vec<(u8, u8)> {
    v.sort_unstable();
    v
}

/// The Yari Knight (`fRffN`) at d5 reaches its forward file slide plus the two
/// narrow-forward `(±1, +2)` knight leaps.
#[test]
fn yari_knight_move_set() {
    // d5 = (3, 4). Forward slide up the d-file: d6..d9. ffN jumps: c7, e7.
    assert_eq!(
        attack_set(WideRole::YariKnight, Color::White, (3, 4), &[]),
        sorted(vec![(3, 5), (3, 6), (3, 7), (3, 8), (2, 6), (4, 6)])
    );
    // A blocker at d7 stops the forward slide at (and including) it; the jumps are
    // unaffected (they leap over any intervening piece).
    assert_eq!(
        attack_set(WideRole::YariKnight, Color::White, (3, 4), &[(3, 6)]),
        sorted(vec![(3, 5), (3, 6), (2, 6), (4, 6)])
    );
    // Black orientation is the vertical mirror: the slide runs down the d-file and
    // the leaps go two ranks back.
    assert_eq!(
        attack_set(WideRole::YariKnight, Color::Black, (3, 4), &[]),
        sorted(vec![(3, 3), (3, 2), (3, 1), (3, 0), (2, 2), (4, 2)])
    );
}

/// The Yari Bishop (`fFfR`) at d5 reaches its two forward diagonals plus the
/// forward file slide.
#[test]
fn yari_bishop_move_set() {
    // d5 = (3, 4). fF: c6, e6. fR: d6..d9.
    assert_eq!(
        attack_set(WideRole::YariBishop, Color::White, (3, 4), &[]),
        sorted(vec![(2, 5), (4, 5), (3, 5), (3, 6), (3, 7), (3, 8)])
    );
}

/// Pieces respect the 7x9 board edges: at the a-file corner a Yari Knight's
/// west-going `(-1, +2)` leap falls off the board and vanishes.
#[test]
fn yari_pieces_respect_board_edges() {
    // a1 = (0, 0). Forward slide up the a-file a2..a9; the only on-board ffN leap is
    // b3 = (1, 2) — the (-1, +2) leap to the "0th" file is off the west edge.
    assert_eq!(
        attack_set(WideRole::YariKnight, Color::White, (0, 0), &[]),
        sorted(vec![
            (0, 1),
            (0, 2),
            (0, 3),
            (0, 4),
            (0, 5),
            (0, 6),
            (0, 7),
            (0, 8),
            (1, 2)
        ])
    );
    // A Yari Rook (`frlR`) on the g-file top corner g9 = (6, 8): no forward slide
    // (off the north edge), no east slide (off the east edge); only the west slide
    // f9..a9 remains.
    assert_eq!(
        attack_set(WideRole::YariRook, Color::White, (6, 8), &[]),
        sorted(vec![(5, 8), (4, 8), (3, 8), (2, 8), (1, 8), (0, 8)])
    );
}

/// A drop from the hand is generated (and the dead-piece / nifu rules are honoured).
#[test]
fn yari_drop_moves_are_generated() {
    // White holds one Pawn; the board is bare but for the two kings.
    let pos = Yari::from_fen("k6/7/7/7/7/7/7/7/3K3[P] w - - 0 1").expect("valid Yari FEN");
    let ucis: Vec<String> = pos
        .legal_moves()
        .into_iter()
        .map(|m| m.to_uci::<YariShogi7x9>())
        .collect();
    // A Pawn drop is offered on an empty non-last-rank square.
    assert!(
        ucis.iter().any(|u| u == "P@d5"),
        "expected a Pawn drop P@d5 among {ucis:?}"
    );
    // The dead-piece rule bars a Pawn drop on the last rank (rank 9): no `P@*9`.
    assert!(
        !ucis.iter().any(|u| u.starts_with("P@") && u.ends_with('9')),
        "a Pawn may not be dropped on the last rank"
    );
}

/// Nifu: a Pawn may not be dropped onto a file that already holds an unpromoted
/// friendly Pawn.
#[test]
fn yari_pawn_drop_obeys_nifu() {
    // A White Pawn already sits on d3; White holds another Pawn in hand.
    let pos = Yari::from_fen("k6/7/7/7/7/7/3P3/7/3K3[P] w - - 0 1").expect("valid Yari FEN");
    let ucis: Vec<String> = pos
        .legal_moves()
        .into_iter()
        .map(|m| m.to_uci::<YariShogi7x9>())
        .collect();
    // No Pawn drop anywhere on the d-file (files are 'd' == the 4th file).
    assert!(
        !ucis.iter().any(|u| u.starts_with("P@d")),
        "nifu: no second Pawn on the d-file, got {ucis:?}"
    );
    // But a Pawn drop on another empty file is fine.
    assert!(ucis.iter().any(|u| u == "P@c5"));
}

/// Promotion is offered (optionally) when a move touches the far three ranks.
#[test]
fn yari_promotion_moves_are_generated() {
    // White Yari Knight on d6 (rank 6 of 9, one rank below the zone). Moving forward
    // into the zone (d7/d8/d9) or leaping into it (c8/e8) offers an optional
    // promotion, so both a plain and a `+` move exist.
    let pos = Yari::from_fen("k6/7/7/3****J3/7/7/7/7/3K3[] w - - 0 1").expect("valid Yari FEN");
    let ucis: Vec<String> = pos
        .legal_moves()
        .into_iter()
        .map(|m| m.to_uci::<YariShogi7x9>())
        .collect();
    assert!(
        ucis.iter().any(|u| u == "d6d7"),
        "the non-promoting move must remain legal (not forced), got {ucis:?}"
    );
    // Yari's promotions are per-piece custom roles (not the `+`-tier Shogi roles),
    // so the promoting move names the target role's token — the Yari Gold `****p`.
    assert!(
        ucis.iter().any(|u| u == "d6d7****p"),
        "the promoting move (to a Yari Gold) must be offered, got {ucis:?}"
    );
}

/// Engine perft pins for the start position. Depth 1 is hand-derived (module docs);
/// depths 2-4 are cross-checked against the independent brute force below.
const ENGINE_PINS: &[(u32, u64)] = &[(1, 20), (2, 400), (3, 7960)];

#[test]
fn engine_startpos_perft_pins() {
    let pos = Yari::startpos();
    for &(depth, expected) in ENGINE_PINS {
        let got = gperft::<YariShogi7x9, _, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "engine Yari Shogi perft({depth}) mismatch (rules-validated, no perft oracle)"
        );
    }
}

#[test]
#[ignore = "deep perft; run with --release --test perft_yarishogi -- --include-ignored"]
fn engine_startpos_perft_deep() {
    let pos = Yari::startpos();
    assert_eq!(gperft::<YariShogi7x9, _, _>(&pos, 4), DEPTH4);
}

/// The engine and the **independent** brute-force generator must agree on every
/// node count, depths 1-4. This is the no-oracle cross-validation.
#[test]
fn engine_matches_independent_brute_force() {
    let engine = Yari::startpos();
    let bf = brute::Position::startpos();
    for depth in 1..=4 {
        let e = gperft::<YariShogi7x9, _, _>(&engine, depth);
        let b = brute::perft(&bf, depth);
        assert_eq!(
            e, b,
            "engine vs independent brute-force Yari perft({depth}) disagree: {e} vs {b}"
        );
    }
    assert_eq!(brute::perft(&bf, 1), 20);
}

/// Depth-4 pin, shared by the `#[ignore]`d engine test and the brute-force cross-check.
const DEPTH4: u64 = 158_404;

/// A hand-constructed midgame that exercises the mechanics the shallow start
/// position does not: **optional promotion** (a White Yari Knight and Yari Bishop
/// already stand in the far three ranks, so every one of their moves offers a
/// promotion), **captures into hand**, and **drops** (White holds a Pawn and a Yari
/// Bishop, Black a Pawn) including **nifu** (the Pawn drop is barred from the
/// a-file, which already holds a White Pawn). White to move, no side in check.
///
/// `k5****o/4****j2/2****J2****Ap/7/7/7/P6/7/****O2K3[****APp] w - - 0 1`
///
/// White: King d1, Yari Rook a1, Pawn a3, Yari Knight c7, Yari Bishop f7; hand P,
/// Yari Bishop. Black: King a9, Yari Rook g9, Pawn g7, Yari Knight e8; hand p. The
/// engine and the independent brute force agree node-for-node at every depth.
const MIDGAME_FEN: &str = "k5****o/4****j2/2****J2****Ap/7/7/7/P6/7/****O2K3[****APp] w - - 0 1";

/// Engine perft pins for the midgame; cross-checked against the brute force below.
const MIDGAME_PINS: &[(u32, u64)] = &[(1, MID1), (2, MID2), (3, MID3)];
const MID1: u64 = 111;
const MID2: u64 = 6411;
const MID3: u64 = 446_974;

#[test]
fn engine_midgame_matches_independent_brute_force() {
    let engine = Yari::from_fen(MIDGAME_FEN).expect("valid Yari midgame FEN");
    let bf = brute::Position::midgame();
    for depth in 1..=3 {
        let e = gperft::<YariShogi7x9, _, _>(&engine, depth);
        let b = brute::perft(&bf, depth);
        assert_eq!(
            e, b,
            "engine vs independent brute-force Yari midgame perft({depth}) disagree: {e} vs {b}"
        );
    }
}

#[test]
fn engine_midgame_perft_pins() {
    let engine = Yari::from_fen(MIDGAME_FEN).expect("valid Yari midgame FEN");
    for &(depth, expected) in MIDGAME_PINS {
        assert_eq!(
            gperft::<YariShogi7x9, _, _>(&engine, depth),
            expected,
            "engine Yari midgame perft({depth}) mismatch"
        );
    }
}

/// A fully independent, naive, array-based Yari Shogi move generator and perft —
/// written from scratch (its own 7x9 position model, move tables, hand, drops,
/// promotion and King safety, with no use of the engine under test) to
/// cross-validate the engine's perft without an oracle. The piece move descriptors
/// are re-derived directly from the documented Betza notation, in a uniform
/// `(df, dr, range, jump)` encoding deliberately unlike the engine's bitboard
/// helpers.
mod brute {
    const W: u8 = 0;
    const B: u8 = 1;
    const WIDTH: i32 = 7;
    const HEIGHT: i32 = 9;

    // Role indices (own enumeration).
    //  0 = Shogi Pawn (fW)         5 = Rook (promoted Yari Rook, full R)
    //  1 = Yari Rook (frlR)        6 = Yari Gold (promoted Knight/Bishop, WfFbR)
    //  2 = Yari Knight (fRffN)     7 = Yari Silver (promoted Pawn, fKbR)
    //  3 = Yari Bishop (fFfR)
    //  4 = King
    const KING: u8 = 4;

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

    // Move sets in White orientation, re-derived from the documented Betza.
    const PAWN: &[Step] = &[s(0, 1, 1)]; // fW
    const YARI_ROOK: &[Step] = &[s(0, 1, 0), s(1, 0, 0), s(-1, 0, 0)]; // frlR
    const YARI_KNIGHT: &[Step] = &[s(0, 1, 0), j(1, 2), j(-1, 2)]; // fRffN
    const YARI_BISHOP: &[Step] = &[s(0, 1, 0), s(1, 1, 1), s(-1, 1, 1)]; // fFfR
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
    const ROOK: &[Step] = &[s(0, 1, 0), s(0, -1, 0), s(1, 0, 0), s(-1, 0, 0)]; // R
    const YARI_GOLD: &[Step] = &[
        s(1, 0, 1),
        s(-1, 0, 1),
        s(0, 1, 1),
        s(1, 1, 1),
        s(-1, 1, 1),
        s(0, -1, 0),
    ]; // WfFbR
    const YARI_SILVER: &[Step] = &[s(0, 1, 1), s(1, 1, 1), s(-1, 1, 1), s(0, -1, 0)]; // fKbR

    fn steps_for(role: u8) -> &'static [Step] {
        match role {
            0 => PAWN,
            1 => YARI_ROOK,
            2 => YARI_KNIGHT,
            3 => YARI_BISHOP,
            KING => KING_STEPS,
            5 => ROOK,
            6 => YARI_GOLD,
            7 => YARI_SILVER,
            _ => &[],
        }
    }

    /// The promotable base pieces: Pawn, Yari Rook, Yari Knight, Yari Bishop.
    fn can_promote(role: u8) -> bool {
        role <= 3
    }
    fn promoted(role: u8) -> u8 {
        match role {
            0 => 7,     // Pawn -> Yari Silver
            1 => 5,     // Yari Rook -> Rook
            2 | 3 => 6, // Yari Knight / Yari Bishop -> Yari Gold
            other => other,
        }
    }
    /// A captured promoted piece sheds its promotion for the hand (matching FSF's
    /// canonical last-assignment demotion: a Yari Gold banks as a Yari Bishop).
    fn hand_base(role: u8) -> u8 {
        match role {
            5 => 1, // Rook -> Yari Rook
            6 => 3, // Yari Gold -> Yari Bishop
            7 => 0, // Yari Silver -> Pawn
            other => other,
        }
    }
    /// Forced promotion: a Pawn / Yari Knight / Yari Bishop reaching the last rank
    /// has no further move.
    fn forced(role: u8, color: u8, to_rank: i32) -> bool {
        matches!(role, 0 | 2 | 3) && to_rank == last_rank(color)
    }
    fn last_rank(color: u8) -> i32 {
        if color == W {
            HEIGHT - 1
        } else {
            0
        }
    }
    fn in_zone(color: u8, rank: i32) -> bool {
        if color == W {
            rank >= HEIGHT - 3
        } else {
            rank < 3
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    struct Pc {
        color: u8,
        role: u8,
    }

    #[derive(Clone)]
    pub(crate) struct Position {
        cells: [Option<Pc>; (WIDTH * HEIGHT) as usize],
        turn: u8,
        // Hand counts per [color][base role 0..=3].
        hand: [[u8; 4]; 2],
    }

    #[inline]
    fn idx(f: i32, r: i32) -> usize {
        (r * WIDTH + f) as usize
    }
    #[inline]
    fn file_of(i: usize) -> i32 {
        (i as i32) % WIDTH
    }
    #[inline]
    fn rank_of(i: usize) -> i32 {
        (i as i32) / WIDTH
    }
    #[inline]
    fn on_board(f: i32, r: i32) -> bool {
        (0..WIDTH).contains(&f) && (0..HEIGHT).contains(&r)
    }

    /// A move: a board move `(from, to, promote)` or a drop `(to, base_role)`.
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
        pub(crate) fn startpos() -> Position {
            // White back rank (rank 1, index 0), files a..g: Yari Rook, Yari Bishop,
            // Yari Bishop, King, Yari Knight, Yari Knight, Yari Rook. Black (ranks 7
            // and 9) is the 180-degree reflection.
            let back = [1u8, 3, 3, KING, 2, 2, 1];
            let mut cells = [None; (WIDTH * HEIGHT) as usize];
            for f in 0..WIDTH {
                let role = back[f as usize];
                cells[idx(f, 0)] = Some(Pc { color: W, role });
                // Black back rank 9 is the file-reversed reflection.
                cells[idx(WIDTH - 1 - f, HEIGHT - 1)] = Some(Pc { color: B, role });
                // Pawns on the third rank from each side.
                cells[idx(f, 2)] = Some(Pc { color: W, role: 0 });
                cells[idx(f, HEIGHT - 3)] = Some(Pc { color: B, role: 0 });
            }
            Position {
                cells,
                turn: W,
                hand: [[0; 4]; 2],
            }
        }

        /// The hand-constructed midgame mirrored by `MIDGAME_FEN` in the parent
        /// module (White to move). Base role indices: 0 Pawn, 1 Yari Rook, 2 Yari
        /// Knight, 3 Yari Bishop, 4 King.
        pub(crate) fn midgame() -> Position {
            let mut cells = [None; (WIDTH * HEIGHT) as usize];
            let put = |f: i32, r: i32, color: u8, role: u8, cells: &mut [Option<Pc>]| {
                cells[idx(f, r)] = Some(Pc { color, role });
            };
            // White: King d1, Yari Rook a1, Pawn a3, Yari Knight c7, Yari Bishop f7.
            put(3, 0, W, KING, &mut cells);
            put(0, 0, W, 1, &mut cells);
            put(0, 2, W, 0, &mut cells);
            put(2, 6, W, 2, &mut cells);
            put(5, 6, W, 3, &mut cells);
            // Black: King a9, Yari Rook g9, Pawn g7, Yari Knight e8.
            put(0, 8, B, KING, &mut cells);
            put(6, 8, B, 1, &mut cells);
            put(6, 6, B, 0, &mut cells);
            put(4, 7, B, 2, &mut cells);
            let mut hand = [[0u8; 4]; 2];
            hand[W as usize][0] = 1; // White Pawn
            hand[W as usize][3] = 1; // White Yari Bishop
            hand[B as usize][0] = 1; // Black Pawn
            Position {
                cells,
                turn: W,
                hand,
            }
        }

        #[inline]
        fn orient(color: u8, df: i8, dr: i8) -> (i32, i32) {
            if color == W {
                (df as i32, dr as i32)
            } else {
                (df as i32, -(dr as i32))
            }
        }

        /// Target squares (empty or enemy-occupied) a piece of `role`/`color` at
        /// `from` reaches, respecting blocking.
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

        /// Is `target` attacked by any piece of color `by`?
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
            // Board moves.
            for from in 0..self.cells.len() {
                let Some(p) = self.cells[from] else { continue };
                if p.color != us {
                    continue;
                }
                let mut tg = Vec::new();
                self.targets(from, us, p.role, &mut tg);
                for to in tg {
                    let to_r = rank_of(to);
                    let from_r = rank_of(from);
                    let zone = in_zone(us, to_r) || in_zone(us, from_r);
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
            // Drops.
            for role in 0u8..4 {
                if self.hand[us as usize][role as usize] == 0 {
                    continue;
                }
                // Nifu: a Pawn may not be dropped on a file already holding an
                // unpromoted friendly Pawn.
                let mut pawn_files = [false; WIDTH as usize];
                if role == 0 {
                    for i in 0..self.cells.len() {
                        if matches!(self.cells[i], Some(pc) if pc.color == us && pc.role == 0) {
                            pawn_files[file_of(i) as usize] = true;
                        }
                    }
                }
                for to in 0..self.cells.len() {
                    if self.cells[to].is_some() {
                        continue;
                    }
                    // Dead-piece drop: Pawn / Yari Knight / Yari Bishop not on the last rank.
                    if matches!(role, 0 | 2 | 3) && rank_of(to) == last_rank(us) {
                        continue;
                    }
                    if role == 0 && pawn_files[file_of(to) as usize] {
                        continue;
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
