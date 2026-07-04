//! Cross-checks the public **analysis primitives** (issue #373) against both the
//! validated reverse attack query and forward move generation, across a
//! representative variant set.
//!
//! ## What this guards
//!
//! The primitives in `src/geometry/analysis.rs` expose existing internals as a
//! clean public surface. They must stay mutually consistent and consistent with
//! the move generator:
//!
//! * **Forward == reverse.** `attack_map(side)` (the forward union of every
//!   piece's threat set) must agree, square for square, with `attackers_of(sq,
//!   side)` being non-empty and with `is_attacked(sq, side)`. This is the public
//!   mirror of the internal `attackers_to`-vs-forward guard in
//!   `tests/attackers_consistency.rs`.
//! * **Per-piece decomposition.** Every occupied square yields a `piece_attacks`
//!   set whose population count equals `piece_mobility`, and that set is a subset
//!   of its owner's `attack_map`.
//! * **Defense = attack ∩ own pieces**, and `attack_count = attack_map.count()`.
//! * **Move-generation tie.** For the side to move, every *legal capture*'s
//!   target square must be in the mover's `attack_map`, and the capturing piece's
//!   origin must be among that target's `attackers_of` — i.e. a square the
//!   primitives report attacked really is reachable by an enemy capture. Quiet
//!   moves (pawn pushes, cannon slides, drops) are excluded because they are
//!   moves, not attacks.
//!
//! The property is asserted on the start position and at **every node** of
//! deterministic `splitmix64` random playouts (no `rand` dependency), so a
//! failure always reproduces. Adding a variant is one `variant_test!` line.

use mce::geometry::{
    Bitboard, CannonShogiRules, CapablancaRules, ChennisRules, DobutsuRules, DuckRules,
    EmpireRules, GenericPosition, Geometry, JanggiRules, JieqiRules, MakrukRules, MinishogiRules,
    MinixiangqiRules, SeirawanRules, ShakoRules, ShinobiRules, ShogiRules, SittuyinRules,
    SpartanRules, Square, StandardChess, SynochessRules, WideVariant, XiangqiRules,
};
use mce::geometry::{
    Cap10x8, Chennis7x7, Chess8x8, Dobutsu3x4, Grand10x10, Minishogi5x5, Minixiangqi7x7, Shogi9x9,
    Xiangqi9x10,
};
use mce::Color;

/// One step of splitmix64 — a tiny, fully deterministic, dependency-free PRNG,
/// matching the generator used by the sibling attack/perft suites.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Plies walked per seed; the property is asserted at every node along the way.
const PLIES: usize = 24;

/// Deterministic seed list; each fixes a reproducible random line.
const SEEDS: [u64; 5] = [
    0x0000_0000_0000_0001,
    0xDEAD_BEEF_CAFE_F00D,
    0x1234_5678_9ABC_DEF0,
    0xA5A5_5A5A_C3C3_3C3C,
    0xFEDC_BA98_7654_3210,
];

/// Renders a bitboard as its sorted set-square indices, for diagnostics.
fn squares_of<G: Geometry>(bb: Bitboard<G>) -> Vec<u8> {
    bb.into_iter().map(|s| s.index()).collect()
}

/// Asserts the internal consistency of the analysis primitives on one position.
fn assert_primitives<G: Geometry, V: WideVariant<G>>(
    pos: &GenericPosition<G, V>,
    variant: &str,
    fen: &str,
) {
    let board = pos.board();

    for c in Color::ALL {
        let map = pos.attack_map(c);

        // Forward union (attack_map) == reverse query (attackers_of) == is_attacked,
        // square for square. This is the public mirror of the attackers_to guard.
        for index in 0..G::SQUARES {
            let sq = Square::<G>::new(index as u8);
            let by_map = map.contains(sq);
            let by_attackers = !pos.attackers_of(sq, c).is_empty();
            let by_flag = pos.is_attacked(sq, c);
            assert!(
                by_map == by_attackers && by_map == by_flag,
                "attack disagreement in {variant}: square {index}, color {c:?}\n  \
                 attack_map={by_map} attackers_of.nonempty={by_attackers} is_attacked={by_flag}\n  \
                 attack_map={m:?}\n  FEN: {fen}",
                m = squares_of(map),
            );
        }

        // attack_count is the map's population count.
        assert_eq!(
            pos.attack_count(c),
            map.count(),
            "attack_count mismatch in {variant}, color {c:?}, FEN: {fen}",
        );

        // defense_map is exactly the attacked own pieces.
        assert!(
            pos.defense_map(c) == (map & board.by_color(c)),
            "defense_map mismatch in {variant}, color {c:?}, FEN: {fen}",
        );
    }

    // Per-piece decomposition: every occupied square has a piece_attacks set whose
    // count is piece_mobility and which is a subset of its owner's attack_map.
    for index in 0..G::SQUARES {
        let sq = Square::<G>::new(index as u8);
        match board.color_at(sq) {
            None => assert!(
                pos.piece_attacks(sq).is_none() && pos.piece_mobility(sq) == 0,
                "empty square {index} reported attacks in {variant}, FEN: {fen}",
            ),
            Some(owner) => {
                let pa = pos
                    .piece_attacks(sq)
                    .expect("occupied square yields a piece_attacks set");
                assert_eq!(
                    pos.piece_mobility(sq),
                    pa.count(),
                    "piece_mobility != piece_attacks.count at {index} in {variant}, FEN: {fen}",
                );
                assert!(
                    (pa & !pos.attack_map(owner)).is_empty(),
                    "piece_attacks at {index} not a subset of attack_map({owner:?}) in {variant}\n  \
                     piece_attacks={pa:?}\n  FEN: {fen}",
                    pa = squares_of(pa),
                );
            }
        }
    }
}

/// Ties the primitives to forward move generation: every legal *capture* for the
/// side to move lands on a square that side attacks, and the capturing piece is
/// among that square's attackers.
fn assert_move_gen<G: Geometry, V: WideVariant<G>>(
    pos: &GenericPosition<G, V>,
    variant: &str,
    fen: &str,
) {
    let us = pos.turn();
    let map = pos.attack_map(us);
    for mv in pos.legal_moves() {
        if !mv.is_capture() {
            continue;
        }
        let to = mv.to::<G>();
        assert!(
            map.contains(to),
            "legal capture target {t} not in attack_map({us:?}) in {variant}\n  \
             move={mv:?}\n  FEN: {fen}",
            t = to.index(),
        );
        let from = mv.from::<G>();
        assert!(
            pos.attackers_of(to, us).contains(from),
            "capturing piece at {f} not among attackers_of({t}, {us:?}) in {variant}\n  \
             move={mv:?}\n  FEN: {fen}",
            f = from.index(),
            t = to.index(),
        );
    }
}

/// Cross-checks the check / pin / per-piece-move query helpers against the
/// validated primitives and forward move generation.
fn assert_queries<G: Geometry, V: WideVariant<G>>(
    pos: &GenericPosition<G, V>,
    variant: &str,
    fen: &str,
) {
    let board = pos.board();
    let us = pos.turn();

    // is_in_check(turn) is exactly the proven is_check; and a legal position never
    // leaves the side that just moved (turn.opposite()) in check.
    assert_eq!(
        pos.is_in_check(us),
        pos.is_check(),
        "is_in_check(turn) != is_check in {variant}, FEN: {fen}",
    );
    assert!(
        !pos.is_in_check(us.opposite()),
        "the side that just moved is in check in {variant}, FEN: {fen}",
    );

    for c in Color::ALL {
        let royals = pos.royal_squares(c);
        let checkers = pos.checkers_of(c);
        // Every checker is an enemy piece whose threat set actually covers a royal
        // of c — reverse (attackers) and forward (piece_attacks) agree per piece.
        for chk in checkers {
            assert!(
                board.color_at(chk) == Some(c.opposite()),
                "checker {i} of {c:?} is not an enemy piece in {variant}, FEN: {fen}",
                i = chk.index(),
            );
            let threats = pos
                .piece_attacks(chk)
                .expect("a checker occupies its square");
            assert!(
                !(threats & royals).is_empty(),
                "checker {i} of {c:?} does not attack a royal in {variant}, FEN: {fen}",
                i = chk.index(),
            );
        }
        // A single-royal side is in check iff it has a piece checker (the
        // flying-general term aside, which only adds check); so a non-empty checker
        // set on a single royal always means in check.
        if royals.count() == 1 && !checkers.is_empty() {
            assert!(
                pos.is_in_check(c),
                "checkers present but not in check in {variant}, color {c:?}, FEN: {fen}",
            );
        }

        // Pins are friendly, non-royal, and each has a ray through a king that the
        // pin query reports; unpinned friendly pieces report no ray.
        let pinned = pos.pinned_pieces(c);
        assert!(
            (pinned & !board.by_color(c)).is_empty(),
            "pinned_pieces({c:?}) includes a non-{c:?} piece in {variant}, FEN: {fen}",
        );
        assert!(
            (pinned & royals).is_empty(),
            "a royal is reported pinned in {variant}, color {c:?}, FEN: {fen}",
        );
        for index in 0..G::SQUARES {
            let sq = Square::<G>::new(index as u8);
            let ray = pos.pin_ray_of(c, sq);
            if pinned.contains(sq) {
                let ray = ray.expect("a pinned piece has a pin ray");
                assert!(
                    ray.contains(sq),
                    "pin ray of {index} excludes the pinned piece in {variant}, FEN: {fen}",
                );
            } else {
                assert!(
                    ray.is_none(),
                    "unpinned square {index} reports a pin ray in {variant}, color {c:?}, FEN: {fen}",
                );
            }
        }
    }

    // checkers() is checkers_of(turn).
    assert!(
        pos.checkers() == pos.checkers_of(us),
        "checkers() != checkers_of(turn) in {variant}, FEN: {fen}",
    );

    // legal_moves_from partitions legal_moves by origin, and every pinned piece of
    // the side to move keeps its legal moves on its pin ray.
    let all = pos.legal_moves();
    let mut counted = 0usize;
    let pinned_us = pos.pinned_pieces(us);
    // A geometric pin to one royal confines movement only under single-royal
    // legality; a multi-king side (Spartan) may legally expose one king (it is in
    // check only under *duple* attack), so that tie is asserted only when the side
    // to move has exactly one royal.
    let confines = pos.royal_squares(us).count() == 1;
    for index in 0..G::SQUARES {
        let sq = Square::<G>::new(index as u8);
        let from_here = pos.legal_moves_from(sq);
        for mv in &from_here {
            assert_eq!(
                mv.from::<G>(),
                sq,
                "legal_moves_from({index}) returned a foreign-origin move in {variant}, FEN: {fen}",
            );
        }
        counted += from_here.len();
        if confines && pinned_us.contains(sq) {
            let ray = pos
                .pin_ray_of(us, sq)
                .expect("a pinned piece of the mover has a ray");
            for mv in &from_here {
                assert!(
                    ray.contains(mv.to::<G>()),
                    "pinned piece at {index} moves off its pin ray in {variant}\n  \
                     move={mv:?}\n  FEN: {fen}",
                );
            }
        }
    }
    assert_eq!(
        counted,
        all.len(),
        "legal_moves_from does not partition legal_moves in {variant}, FEN: {fen}",
    );
}

/// Asserts every property on one position.
fn assert_all<G: Geometry, V: WideVariant<G>>(
    pos: &GenericPosition<G, V>,
    variant: &str,
    fen: &str,
) {
    assert_primitives(pos, variant, fen);
    assert_move_gen(pos, variant, fen);
    assert_queries(pos, variant, fen);
}

/// Runs the properties over the start position and `SEEDS`-driven playouts.
fn run_variant<G: Geometry, V: WideVariant<G>>(variant: &str) {
    let start = GenericPosition::<G, V>::startpos();
    assert_all(&start, variant, "<startpos>");

    for &seed in &SEEDS {
        let mut state = seed;
        let mut pos = GenericPosition::<G, V>::startpos();
        for _ply in 0..PLIES {
            let moves = pos.legal_moves();
            if moves.is_empty() {
                break;
            }
            let pick = (splitmix64(&mut state) as usize) % moves.len();
            pos = pos.play(&moves[pick]);
            let fen = pos.to_fen();
            assert_all(&pos, variant, &fen);
        }
    }
}

macro_rules! variant_test {
    ($name:ident, $geom:ty, $rules:ty, $label:literal) => {
        #[test]
        fn $name() {
            run_variant::<$geom, $rules>($label);
        }
    };
}

// A representative spread: standard chess; the directional-Pawn / leg-asymmetric-
// Horse / cannon Xiangqi family (Xiangqi, Minixiangqi, Janggi board-aware cannon,
// Jieqi); Shogi-family drops (Shogi, Minishogi, Cannon Shogi board-aware hoppers);
// wide boards (Capablanca 10x8, Shako 10x10); a tiny board (Dobutsu 3x4, Chennis
// 7x7); multi-royal (Spartan); the duck; gating (Seirawan); directional Sittuyin;
// and the fairy-army variants (Makruk, Empire capture-short, Shinobi, Synochess).
variant_test!(standard, Chess8x8, StandardChess, "standard");
variant_test!(makruk, Chess8x8, MakrukRules, "makruk");
variant_test!(sittuyin, Chess8x8, SittuyinRules, "sittuyin");
variant_test!(spartan, Chess8x8, SpartanRules, "spartan");
variant_test!(duck, Chess8x8, DuckRules, "duck");
variant_test!(seirawan, Chess8x8, SeirawanRules, "seirawan");
variant_test!(synochess, Chess8x8, SynochessRules, "synochess");
variant_test!(shinobi, Chess8x8, ShinobiRules, "shinobi");
variant_test!(empire, Chess8x8, EmpireRules, "empire");
variant_test!(capablanca, Cap10x8, CapablancaRules, "capablanca");
variant_test!(shako, Grand10x10, ShakoRules, "shako");
variant_test!(xiangqi, Xiangqi9x10, XiangqiRules, "xiangqi");
variant_test!(janggi, Xiangqi9x10, JanggiRules, "janggi");
variant_test!(jieqi, Xiangqi9x10, JieqiRules, "jieqi");
variant_test!(minixiangqi, Minixiangqi7x7, MinixiangqiRules, "minixiangqi");
variant_test!(shogi, Shogi9x9, ShogiRules, "shogi");
variant_test!(cannonshogi, Shogi9x9, CannonShogiRules, "cannonshogi");
variant_test!(minishogi, Minishogi5x5, MinishogiRules, "minishogi");
variant_test!(dobutsu, Dobutsu3x4, DobutsuRules, "dobutsu");
variant_test!(chennis, Chennis7x7, ChennisRules, "chennis");
