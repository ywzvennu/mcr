//! Wa Shogi (11x11 animal shogi) perft validation on the generic engine
//! (issue #324).
//!
//! Wa Shogi has **no perft oracle**: Fairy-Stockfish does not implement it (its
//! shogi family is checkshogi / euroshogi / kyotoshogi / manchu / minishogi /
//! okisakishogi / shogi / shoshogi / torishogi / yarishogi — Wa is absent and is
//! inexpressible via `variants.ini`), and the only engine that does, **HaChu**, has
//! an unreliable perft (its `perft` of standard Shogi already disagrees with the
//! known 30 / 900 / 25470 sequence, returning 30 / 930 / 12891). The pins here are
//! therefore **rules-validated (no perft oracle); perft pins derived from the
//! documented Wa ruleset** (<https://www.chessvariants.com/rules/wa-shogi>,
//! <http://www.shogi.net/rjhare/wa-shogi/>, cross-checked against the HaChu
//! engine's Betza piece definitions), and cross-checked by a fully **independent
//! brute-force Wa move generator** written from scratch in this file (a separate
//! array-based 11x11 position model with its own naive movegen, hand/drops,
//! per-piece promotion and Crane-King safety). Two independent implementations
//! agreeing on every node count to depth 3 is the substitute for the missing
//! oracle.
//!
//! HaChu *was* re-tried on Wa Shogi under issue #500 (it advertises `wa-shogi` and,
//! unlike Tenjiku, does not crash): it generates **51** start-position moves against
//! mcr's **57**, from a different start array and piece set — it ships a *different*
//! Wa Shogi ruleset, so it is not a usable node-for-node oracle here. That probe is
//! recorded in-repo by `compare-fairy`'s `probe_washogi` (`cargo run --release --
//! --hachu`). The independent brute force below remains the second count source.
//!
//! ## How the shallow numbers are derived
//!
//! * **perft(1) = 57.** The two armies start five ranks apart (White on ranks 1-3,
//!   Black on ranks 9-11), so at the root the only contact is along the h-file,
//!   where White's Running Rabbit (h3, a forward rook) can slide up to and capture
//!   Black's Treacherous Fox on h9. Every White piece's forward steps and slides
//!   into the empty centre, plus that one capture and its optional promotion, sum
//!   to 57 — reproduced node-for-node by the independent brute force below.
//! * **perft(2), perft(3)** are produced **identically** by the engine and the
//!   independent generator (the no-oracle cross-validation); the deeper layers are
//!   `#[ignore]`d.

use mcr::geometry::{perft as gperft, Washogi, Washogi11x11};

/// The Wa Shogi starting FEN (mcr overflow spelling; empty hand).
const STARTPOS: &str = "**f**j**h**l**nk**o**k**g**m**d/1**v3**q3**t1/\
**b**b**b**r**b**b**b**u**b**b**b/11/11/11/11/11/\
**B**B**B**U**B**B**B**R**B**B**B/1**T3**Q3**V1/\
**D**M**G**K**OK**N**L**H**J**F[] w - - 0 1";

/// Engine perft pins for the start position. Depth 1 is hand-derived (module docs);
/// depths 2-3 are cross-checked against the independent brute force below.
const ENGINE_PINS: &[(u32, u64)] = &[(1, 57), (2, 3204), (3, 174_579)];

#[test]
fn engine_startpos_perft_pins() {
    let pos = Washogi::from_fen(STARTPOS).expect("valid Wa Shogi FEN");
    for &(depth, expected) in ENGINE_PINS {
        let got = gperft::<Washogi11x11, _>(&pos, depth);
        assert_eq!(
            got, expected,
            "engine Wa Shogi perft({depth}) mismatch (rules-validated, no perft oracle)"
        );
    }
}

#[test]
#[ignore = "deep perft; run with --release --test perft_washogi -- --include-ignored"]
fn engine_startpos_perft_deep() {
    let pos = Washogi::from_fen(STARTPOS).expect("valid Wa Shogi FEN");
    assert_eq!(gperft::<Washogi11x11, _>(&pos, 4), 9_531_440);
}

/// The engine and the **independent** brute-force generator must agree on every
/// node count, depths 1-3. This is the no-oracle cross-validation.
#[test]
fn engine_matches_independent_brute_force() {
    let engine = Washogi::from_fen(STARTPOS).expect("valid Wa Shogi FEN");
    let bf = brute::Position::startpos();
    for depth in 1..=3 {
        let e = gperft::<Washogi11x11, _>(&engine, depth);
        let b = brute::perft(&bf, depth);
        assert_eq!(
            e, b,
            "engine vs independent brute-force Wa perft({depth}) disagree: {e} vs {b}"
        );
    }
    assert_eq!(brute::perft(&bf, 1), 57);
}

/// Regression for the pinned-leaper jump bug (issue #426, the Wa analogue of the
/// Tori Pheasant bug #416): a **Treacherous Fox** (`FAvWvD`, whose vertical Dabbaba
/// step jumps two squares straight) pinned to its own Crane King along a file may
/// **not** take that two-square jump, which would leap over the pinning slider and
/// vacate the shielding square, exposing the king.
///
/// Here a Black Oxcart on d3 pins the White Fox on d2 to the White King on d1 along
/// the d-file. The Fox's only legal move is to capture the pinning Oxcart (`d2d3`);
/// its vertical Dabbaba jump `d2d4` — over the Oxcart, off the king-to-pinner
/// segment — would leave the king in check and is illegal. mcr previously emitted
/// `d2d4` under the default full-line pin mask (perft(1) = 6); confining a pinned
/// piece to the king-to-pinner segment drops it, giving the correct perft(1) = 5
/// (four King steps + the Fox's capture of the pinner). Both the engine and the
/// independent brute-force generator (whose legality is make-move + Crane-King
/// safety) agree on 5.
#[test]
fn pinned_fox_cannot_take_its_forward_jump() {
    const PIN_FEN: &str = "10k/11/11/11/11/11/11/11/3**d7/3**U7/3K7[] w - - 0 1";
    let pos = Washogi::from_fen(PIN_FEN).expect("valid pinned-Fox Wa FEN");

    // Correct perft(1) = 5: the four Crane-King steps (c1, e1, c2, e2) plus the
    // Fox's `d2d3` capture of the pinning Oxcart. The illegal `d2d4` jump is gone.
    assert_eq!(
        gperft::<Washogi11x11, _>(&pos, 1),
        5,
        "pinned Fox must not keep its illegal d2d4 jump"
    );

    // No generated move may leave the moving side's own king attacked (mcr's own
    // self-consistency: the pin mask must never admit a king-exposing move).
    for m in pos.legal_moves() {
        let next = pos.play(&m);
        if let Some(k) = next.board().king_of(mcr::Color::White) {
            assert!(
                !next.is_attacked(k, mcr::Color::Black),
                "move {} leaves the White king in check",
                m.to_uci::<Washogi11x11>()
            );
        }
    }
}

/// A fully independent, naive, array-based Wa Shogi move generator and perft —
/// written from scratch (its own 11x11 position model, move tables, hand, drops,
/// promotion and Crane-King safety, with no use of the engine under test) to
/// cross-validate the engine's perft without an oracle. The piece move descriptors
/// are re-derived directly from the documented Betza notation, in a uniform
/// `(df, dr, range, jump)` encoding deliberately unlike the engine's bitboard
/// helpers.
mod brute {
    const W: u8 = 0;
    const B: u8 = 1;
    const WIDTH: i32 = 11;
    const HEIGHT: i32 = 11;

    // Role indices (own enumeration). 0..=15 base, 16..=29 promoted, 30 = Crane King.
    const KING: u8 = 30;

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

    // Shared move sets (White orientation), re-derived from the documented Betza.
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
    // WfF — four orthogonal + two forward diagonal (Violent Wolf / Golden Bird / Promoted Blind Dog).
    const WOLF: &[Step] = &[
        s(1, 0, 1),
        s(-1, 0, 1),
        s(0, 1, 1),
        s(0, -1, 1),
        s(1, 1, 1),
        s(-1, 1, 1),
    ];
    // FfW — four diagonal + one forward (Violent Stag / Promoted Climbing Monkey).
    const STAG: &[Step] = &[
        s(1, 1, 1),
        s(1, -1, 1),
        s(-1, 1, 1),
        s(-1, -1, 1),
        s(0, 1, 1),
    ];
    // FAvWvD — Treacherous Fox / Promoted Running Rabbit (one/two diagonal & vertical, jumping the two-step).
    const FOX: &[Step] = &[
        s(1, 1, 1),
        s(1, -1, 1),
        s(-1, 1, 1),
        s(-1, -1, 1),
        j(2, 2),
        j(2, -2),
        j(-2, 2),
        j(-2, -2),
        s(0, 1, 1),
        s(0, -1, 1),
        j(0, 2),
        j(0, -2),
    ];
    // BfW — bishop + one forward (Flying Falcon / Promoted Strutting Crow).
    const FALCON: &[Step] = &[
        s(1, 1, 0),
        s(1, -1, 0),
        s(-1, 1, 0),
        s(-1, -1, 0),
        s(0, 1, 1),
    ];
    // vRsWfF3bF — Cloud Eagle / Promoted Swooping Owl.
    const EAGLE: &[Step] = &[
        s(0, 1, 0),
        s(0, -1, 0),
        s(1, 0, 1),
        s(-1, 0, 1),
        s(1, 1, 3),
        s(-1, 1, 3),
        s(1, -1, 1),
        s(-1, -1, 1),
    ];
    // sRvW — Swallow's Wings / Promoted Flying Goose.
    const SWINGS: &[Step] = &[s(1, 0, 0), s(-1, 0, 0), s(0, 1, 1), s(0, -1, 1)];

    const SPARROW: &[Step] = &[s(0, 1, 1)]; // fW
    const OXCART: &[Step] = &[s(0, 1, 0)]; // fR
    const LIBHORSE: &[Step] = &[s(0, 1, 0), s(0, -1, 2)]; // fRbW2
    const CROW_OWL: &[Step] = &[s(0, 1, 1), s(1, -1, 1), s(-1, -1, 1)]; // fWbF
    const MONKEY_GOOSE: &[Step] = &[s(0, 1, 1), s(0, -1, 1), s(1, 1, 1), s(-1, 1, 1)]; // vWfF
    const COCK: &[Step] = &[s(1, 0, 1), s(-1, 0, 1), s(1, 1, 1), s(-1, 1, 1)]; // sWfF
    const DOG: &[Step] = &[
        s(1, 1, 1),
        s(-1, 1, 1),
        s(1, 0, 1),
        s(-1, 0, 1),
        s(0, -1, 1),
    ]; // fFsbW
    const RABBIT: &[Step] = &[
        s(0, 1, 0),
        s(1, 1, 1),
        s(1, -1, 1),
        s(-1, 1, 1),
        s(-1, -1, 1),
        s(0, -1, 1),
    ]; // fRFbW
    const HHORSE: &[Step] = &[j(1, 2), j(-1, 2), j(1, -2), j(-1, -2)]; // vN
    const RAIDING: &[Step] = &[
        s(0, 1, 0),
        s(0, -1, 0),
        s(1, 0, 1),
        s(-1, 0, 1),
        s(1, 1, 1),
        s(-1, 1, 1),
    ]; // vRsWfF
    const BOAR: &[Step] = &[
        s(1, 1, 1),
        s(1, -1, 1),
        s(-1, 1, 1),
        s(-1, -1, 1),
        s(0, 1, 1),
        s(1, 0, 1),
        s(-1, 0, 1),
    ]; // FfsW
    const GLIDING: &[Step] = &[s(0, 1, 0), s(0, -1, 0), s(1, 0, 0), s(-1, 0, 0)]; // R
    const TENACIOUS: &[Step] = &[
        s(1, 1, 0),
        s(1, -1, 0),
        s(-1, 1, 0),
        s(-1, -1, 0),
        s(0, 1, 0),
        s(0, -1, 0),
        s(1, 0, 1),
        s(-1, 0, 1),
    ]; // BvRsW

    fn steps_for(role: u8) -> &'static [Step] {
        match role {
            0 => SPARROW,
            1 => OXCART,
            2 => LIBHORSE,
            3 | 4 => CROW_OWL,
            5 | 6 => MONKEY_GOOSE,
            7 => COCK,
            8 => DOG,
            9 | 21 => STAG,
            10 | 16 | 24 => WOLF,
            11 | 22 => SWINGS,
            12 => RABBIT,
            13 | 19 => FALCON,
            14 | 28 => FOX,
            15 | 20 => EAGLE,
            17 | 26 | KING => KING_STEPS,
            18 => HHORSE,
            23 => RAIDING,
            25 => BOAR,
            27 => GLIDING,
            29 => TENACIOUS,
            _ => &[],
        }
    }

    fn can_promote(role: u8) -> bool {
        role <= 13
    }
    fn promoted(role: u8) -> u8 {
        16 + role
    }
    fn hand_base(role: u8) -> u8 {
        if (16..=29).contains(&role) {
            role - 16
        } else {
            role
        }
    }
    /// Forced promotion: a Sparrow Pawn (0) or Oxcart (1) reaching the last rank.
    fn forced(role: u8, color: u8, to_rank: i32) -> bool {
        matches!(role, 0 | 1) && to_rank == last_rank(color)
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
        // Hand counts per [color][base role 0..=15].
        hand: [[u8; 16]; 2],
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

    /// A move: a board move `(from, to, promote)` or a drop `(!0, to, base_role)`.
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
            // Reading each back rank a..k. White ranks 1-3; Black is the 180°
            // reflection on ranks 9-11. Base role indices match `steps_for`.
            const OX: u8 = 1;
            const DOG: u8 = 8;
            const CROW: u8 = 3;
            const GOOSE: u8 = 6;
            const WOLF_R: u8 = 10;
            const STAG_R: u8 = 9;
            const COCK: u8 = 7;
            const OWL: u8 = 4;
            const MONKEY: u8 = 5;
            const LIBH: u8 = 2;
            const FALC: u8 = 13;
            const SWINGS_R: u8 = 11;
            const EAGLE_R: u8 = 15;
            const SPAR: u8 = 0;
            const FOX_R: u8 = 14;
            const RABBIT: u8 = 12;

            let back = [
                OX, DOG, CROW, GOOSE, WOLF_R, KING, STAG_R, COCK, OWL, MONKEY, LIBH,
            ];
            let mut cells = [None; (WIDTH * HEIGHT) as usize];
            let put = |f: i32, r: i32, color: u8, role: u8, cells: &mut [Option<Pc>]| {
                cells[idx(f, r)] = Some(Pc { color, role });
            };
            for f in 0..WIDTH {
                let role = back[f as usize];
                put(f, 0, W, role, &mut cells); // White back rank 1
                                                // Black back rank 11 is the file-reversed reflection.
                put(WIDTH - 1 - f, HEIGHT - 1, B, role, &mut cells);
            }
            // Rank 2 (White): Flying Falcon @ b, Swallow's Wings @ f, Cloud Eagle @ j.
            put(1, 1, W, FALC, &mut cells);
            put(5, 1, W, SWINGS_R, &mut cells);
            put(9, 1, W, EAGLE_R, &mut cells);
            // Black rank 10 (reflection): files 10-1=9(j)->FALC, 5->SWINGS, 1->EAGLE... reflect each.
            put(WIDTH - 1 - 1, HEIGHT - 2, B, FALC, &mut cells);
            put(WIDTH - 1 - 5, HEIGHT - 2, B, SWINGS_R, &mut cells);
            put(WIDTH - 1 - 9, HEIGHT - 2, B, EAGLE_R, &mut cells);
            // Rank 3 (White): Sparrows a-c,e-g,i-k; Fox @ d; Running Rabbit @ h.
            for f in 0..WIDTH {
                let role = if f == 3 {
                    FOX_R
                } else if f == 7 {
                    RABBIT
                } else {
                    SPAR
                };
                put(f, 2, W, role, &mut cells);
                put(WIDTH - 1 - f, HEIGHT - 3, B, role, &mut cells);
            }
            Position {
                cells,
                turn: W,
                hand: [[0; 16]; 2],
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
            for role in 0u8..16 {
                if self.hand[us as usize][role as usize] == 0 {
                    continue;
                }
                for to in 0..self.cells.len() {
                    if self.cells[to].is_some() {
                        continue;
                    }
                    if matches!(role, 0 | 1) && rank_of(to) == last_rank(us) {
                        continue; // dead-piece drop
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
