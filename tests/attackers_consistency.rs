//! Cross-check `GenericPosition::attackers_to` against the forward attack
//! relation derived from the move generator, across every variant (issue #202).
//!
//! ## Why this exists
//!
//! `attackers_to(t, c)` answers "which pieces of color `c` attack square `t`" by
//! **reverse-projecting** each role's pattern back from `t`. That is only valid
//! when the role's attack set is symmetric (`a` attacks `b` iff `b` attacks `a`
//! under the same occupancy) and color-non-directional. Two latent
//! check-detection bugs of exactly this class shipped undetected before being
//! fixed:
//!
//! * **#198** — the Xiangqi / Minixiangqi **Horse**: its hobbling leg is adjacent
//!   to the *horse* (asymmetric), so a reverse projection tests the wrong leg.
//!   Fixed via [`WideVariant::role_attack_is_leg_asymmetric`].
//! * **#201** — the Xiangqi / Minixiangqi **Soldier**: its forward step is
//!   color-directional, so a reverse projection must use the opposite color.
//!   Fixed via [`WideVariant::role_attack_is_directional`].
//!
//! Per-variant FSF perft caught them only by luck of corpus coverage. This test
//! is the systematic guard so the class cannot recur: it computes the FORWARD
//! attack relation **independently** of `attackers_to` — for every occupied
//! square `s` holding a piece of color `c`, it projects that piece's own
//! [`WideVariant::role_attacks`] set forward and records every target it hits —
//! then asserts, for every square `t` and color `c`, that
//! `attackers_to(t, c)` **exactly** equals `{ s : the piece on s of color c
//! forward-attacks t }`. It also cross-checks the king-safety direction:
//! `is_attacked(king, enemy)` iff some enemy piece forward-attacks the king
//! square (a pseudo-legal capture of the king).
//!
//! Both directions are checked on the start position, every pinned corpus FEN, and
//! a set of pseudo-random reachable positions reached by short deterministic
//! random playouts (a seeded in-test splitmix64 PRNG — no `rand` dependency).
//!
//! Adding a future variant is one `variant_test!` line.
//!
//! [`WideVariant::role_attack_is_leg_asymmetric`]: mce::geometry::WideVariant::role_attack_is_leg_asymmetric
//! [`WideVariant::role_attack_is_directional`]: mce::geometry::WideVariant::role_attack_is_directional
//! [`WideVariant::role_attacks`]: mce::geometry::WideVariant::role_attacks

use mce::geometry::{
    Bitboard, CapablancaRules, DuckRules, GenericPosition, Geometry, GrandRules, JanggiRules,
    MakrukRules, MinishogiRules, MinixiangqiRules, OrdaRules, SeirawanRules, ShakoRules,
    ShinobiRules, ShogiRules, SittuyinRules, SpartanRules, Square, StandardChess, SynochessRules,
    WideRole, WideVariant, XiangqiRules,
};
use mce::geometry::{
    Cap10x8, Chess8x8, Grand10x10, Minishogi5x5, Minixiangqi7x7, Shogi9x9, Xiangqi9x10,
};
use mce::Color;

/// One step of splitmix64 — a tiny, fully deterministic, dependency-free PRNG.
/// Used only to pick move indices during the random playouts, so its quality is
/// ample. Identical generator to the one `tests/properties.rs` uses.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The number of pseudo-random reachable positions to visit per seed, per
/// variant. Each playout walks up to this many plies from the start position,
/// asserting the consistency property at **every** node along the way (not just
/// the leaf), so the effective position count per seed is up to `PLIES + 1`.
const PLIES: usize = 24;

/// The seed list. Varying the seed varies the random line; each is fully
/// deterministic, so a failure always reproduces.
const SEEDS: [u64; 6] = [
    0x0000_0000_0000_0001,
    0xDEAD_BEEF_CAFE_F00D,
    0x1234_5678_9ABC_DEF0,
    0x0F0F_0F0F_0F0F_0F0F,
    0xA5A5_5A5A_C3C3_3C3C,
    0xFEDC_BA98_7654_3210,
];

/// Computes the FORWARD attack set of color `c` under the position's current
/// occupancy, **independently** of `attackers_to`: for every occupied square `s`
/// holding a piece of color `c`, it ORs in that piece's own forward attack set.
/// The result is the set of squares some piece of `c` attacks.
///
/// For most roles the forward set is the occupancy-only
/// [`WideVariant::role_attacks`] — the same per-piece projection the move
/// generator uses. But a role whose attack set depends on *which* occupied
/// squares hold *which* pieces (the Janggi screen-cannon: screen ≠ cannon, may
/// not capture a cannon, plus the palace-diagonal jump) is computed from the
/// **whole board** via the board-aware [`WideVariant::role_attacks_board`] hook.
/// When the variant sets [`WideVariant::uses_board_attacks`] and that hook
/// returns `Some` for the role, this uses it — exactly as the generator and the
/// cannon-verify king-safety path do — so the board-aware role is forward-projected
/// correctly. The hook returns `None` for every other role (and the default-off
/// variants never set `uses_board_attacks`), so the 13 existing cases keep using
/// the plain `role_attacks` path unchanged.
///
/// This is the ground truth `attackers_to` (the reverse projection) must reproduce.
fn forward_attacks_to<G: Geometry, V: WideVariant<G>>(
    pos: &GenericPosition<G, V>,
    target: Square<G>,
    by: Color,
    occupied: Bitboard<G>,
) -> Bitboard<G> {
    let board = pos.board();
    let board_aware = V::uses_board_attacks();
    let mut sources = Bitboard::EMPTY;
    for role in WideRole::ALL {
        let pieces = board.pieces(by, role);
        if pieces.is_empty() {
            continue;
        }
        for from in pieces {
            // Prefer the board-aware set for any role the variant computes from the
            // whole board (Janggi's cannon); fall back to the occupancy-only set.
            let attacks = if board_aware {
                V::role_attacks_board(role, by, from, board)
                    .unwrap_or_else(|| V::role_attacks(role, by, from, occupied))
            } else {
                V::role_attacks(role, by, from, occupied)
            };
            if attacks.contains(target) {
                sources |= Bitboard::from_square(from);
            }
        }
    }
    sources
}

/// Renders a bitboard as the sorted list of its set square indices, for failure
/// diagnostics (a `Bitboard<G>` only implements `Debug` when `G::Bits` does, a
/// bound the generic test bodies do not carry).
fn squares_of<G: Geometry>(bb: Bitboard<G>) -> Vec<u8> {
    bb.into_iter().map(|s| s.index()).collect()
}

/// Asserts the full attackers consistency property on one position: for every
/// square `t` and every color `c`, `attackers_to(t, c)` (the reverse projection)
/// must EXACTLY equal the forward attack relation computed independently. Also
/// cross-checks the king-safety direction on each royal king square.
fn assert_consistent<G: Geometry, V: WideVariant<G>>(
    pos: &GenericPosition<G, V>,
    variant: &str,
    fen: &str,
) {
    let board = pos.board();
    // `attackers_to` is defined under the board occupancy; use exactly that, so
    // the forward relation is computed against the same board the reverse
    // projection sees (this matters for sliders, cannons, and the duck — which
    // sits outside `board.occupied()` in both).
    let occupied = board.occupied();

    for index in 0..G::SQUARES {
        let t = Square::<G>::new(index);
        for c in Color::ALL {
            let reverse = pos.attackers_to(t, c, occupied);
            let forward = forward_attacks_to(pos, t, c, occupied);
            assert!(
                reverse == forward,
                "attackers_to mismatch in {variant}: square index {index}, color {c:?}\n  \
                 attackers_to (reverse) = {rev:?}\n  forward relation       = {fwd:?}\n  \
                 FEN: {fen}",
                index = index,
                rev = squares_of(reverse),
                fwd = squares_of(forward),
            );
        }
    }

    // King-safety direction: a royal king of color `who` is attacked per
    // `is_attacked` iff some enemy piece forward-attacks (i.e. could pseudo-
    // legally capture) the king's square. Independent of `attackers_to`.
    for who in Color::ALL {
        let them = who.opposite();
        for king_sq in board.kings_of(who) {
            let is_attacked = pos.is_attacked(king_sq, them);
            let forward_hit = !forward_attacks_to(pos, king_sq, them, occupied).is_empty();
            assert_eq!(
                is_attacked, forward_hit,
                "king-safety mismatch in {variant}: king of {who:?} at index {idx}\n  \
                 is_attacked(king, enemy) = {is_attacked}\n  enemy forward-captures king = {forward_hit}\n  \
                 FEN: {fen}",
                idx = king_sq.index(),
            );
        }
    }
}

/// Runs the consistency property over the start position, the pinned corpus FENs,
/// and `SEEDS`-driven random playouts of `PLIES` plies, for one variant.
fn run_variant<G: Geometry, V: WideVariant<G>>(variant: &str, corpus: &[&str]) {
    // Start position.
    let start = GenericPosition::<G, V>::startpos();
    assert_consistent(&start, variant, "<startpos>");

    // Pinned corpus FENs (reused from the perft suites).
    for &fen in corpus {
        let pos = GenericPosition::<G, V>::from_fen(fen)
            .unwrap_or_else(|e| panic!("{variant}: corpus FEN failed to parse: {fen}: {e:?}"));
        assert_consistent(&pos, variant, fen);
    }

    // Deterministic random playouts: assert the property at every node visited.
    for &seed in &SEEDS {
        let mut state = seed;
        let mut pos = GenericPosition::<G, V>::startpos();
        assert_consistent(&pos, variant, "<playout startpos>");
        for _ply in 0..PLIES {
            let moves = pos.legal_moves();
            if moves.is_empty() {
                break;
            }
            let pick = (splitmix64(&mut state) as usize) % moves.len();
            pos = pos.play(&moves[pick]);
            // The reached position is always legal and reachable; assert there.
            // Use the running FEN for diagnostics so a failure is reproducible.
            let fen = pos.to_fen();
            assert_consistent(&pos, variant, &fen);
        }
    }
}

/// Defines one `#[test]` per variant. Adding a future variant is a single line
/// here (its geometry, rules marker, and corpus FEN list).
macro_rules! variant_test {
    ($name:ident, $geom:ty, $rules:ty, $label:literal, [$($fen:expr),* $(,)?]) => {
        #[test]
        fn $name() {
            run_variant::<$geom, $rules>($label, &[$($fen),*]);
        }
    };
}

// -- Standard chess ---------------------------------------------------------

variant_test!(
    standard,
    Chess8x8,
    StandardChess,
    "standard",
    [
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
    ]
);

// -- Makruk -----------------------------------------------------------------

variant_test!(
    makruk,
    Chess8x8,
    MakrukRules,
    "makruk",
    [
        "rnsmksnr/8/1ppppppp/p7/4P3/PPPP1PPP/8/RNSKMSNR b - - 0 2",
        "r1smks1r/3n4/ppp1pppp/3p4/3P4/PPP1PPPP/4N3/R1SKMS1R w - - 0 4",
    ]
);

// -- Capablanca (10x8) ------------------------------------------------------

variant_test!(
    capablanca,
    Cap10x8,
    CapablancaRules,
    "capablanca",
    [
        "r4k3r/pppppppppp/10/10/10/10/PPPPPPPPPP/R4K3R w KQkq - 0 1",
        "1nabqkben1/p1ppppppp1/1r6r1/1p6p1/3PP5/2N4N2/PPP2PPPPP/R1ABQKBE1R w KQ - 0 5",
        "5k4/4P5/10/10/10/10/10/5K4 w - - 0 1",
    ]
);

// -- Grand (10x10) ----------------------------------------------------------

variant_test!(
    grand,
    Grand10x10,
    GrandRules,
    "grand",
    [
        "r8r/2bqkeab2/pppp1ppppp/2n4n2/3Np5/3P6/7N2/PPP1PPPPPP/2BQKEAB2/R8R b - - 1 4",
        "4k5/8P1/10/10/10/10/10/10/10/RNBQK1EAN1 w - - 0 1",
    ]
);

// -- Seirawan (gating, 8x8) -------------------------------------------------

variant_test!(
    seirawan,
    Chess8x8,
    SeirawanRules,
    "seirawan",
    [
        "r1bqkb1r/pppppppp/2n2n2/8/8/2N2N2/PPPPPPPP/R1BQKB1R[HEhe] w KQBCDEFGkqbcdefg - 4 3",
        "rnbqk2r/pppppppp/8/8/8/5N2/PPPPPPBP/RNBQK2R[HEhe] w KQkqABCDFGabcdfgh - 0 1",
        "reb1k2r/pppp1ppp/2nbqn2/4p3/4P3/2NBQN2/PPPP1PPP/R1B1K2R[Hh] w KQkqABCDFGabcdfg - 8 6",
    ]
);

// -- Grand chess uses one corpus already; Duck (no check) -------------------

variant_test!(
    duck,
    Chess8x8,
    DuckRules,
    "duck",
    [
        "r1bqk2r/pppp1ppp/2n2n2/2b1*3/2B1P3/2N2N2/PPPP1PPP/R1BQK2R w KQkq - 0 1",
        "rnbqkbnr/ppp1pppp/8/3pP3/*7/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 1",
        "4k3/8/8/r2pPK2/8/8/8/8 w - d6 0 1",
        "8/2k5/8/3*4/8/5K2/4P3/8 w - - 0 1",
    ]
);

// -- Sittuyin (placement phase, 8x8) ----------------------------------------

variant_test!(
    sittuyin,
    Chess8x8,
    SittuyinRules,
    "sittuyin",
    [
        "rrnmk1n1/1ss5/4pppp/pppp4/4PPPP/PPPP4/1SS5/RRNMK1N1 w - - 0 9",
        "rrn1k1n1/1ss5/4pppp/ppp5/5PPP/PPPP1p2/1SS1M3/RRN1K1N1 b - - 0 11",
        "8/8/4pppp/pppp4/4PPPP/PPPP4/8/3M2R1[NNRKSSnnrrkmss] b - - 0 3",
    ]
);

// -- Spartan (multi-king / Berolina, 8x8) -----------------------------------

variant_test!(
    spartan,
    Chess8x8,
    SpartanRules,
    "spartan",
    [
        "tdkiikat/hhh1hhhh/2h5/8/4P3/8/PPPP1PPP/RNBQKBNR w KQ - 0 2",
        "tdkiikat/hhh2hhh/8/8/2B5/8/PPPPPPPP/RN1QKBNR w KQ - 0 1",
        "2k2k2/8/8/8/2Q5/B7/8/4K3 b - - 0 1",
        "2k2k2/8/8/8/8/5d2/8/2R2R1K b - - 0 1",
        "8/8/8/8/1R6/8/2k2k2/7K b - - 0 1",
        "k7/8/8/8/8/8/4h3/4K3 b - - 0 1",
        "k6k/8/8/8/8/8/4h3/4K3 b - - 0 1",
    ]
);

// -- Orda (asymmetric cavalry leapers + flag win, 8x8) ----------------------
//
// The Orda Lancer / Archer **move** like a knight but **capture** along a slider
// line, so their `role_attacks` set (rook / bishop) is the attack relation while
// their knight jumps are quiet-only (and never enter it). The Yurt is a forward-
// biased silver general, so it is reverse-projected with the opposite colour
// (`role_attack_is_directional`). The corpus is reused from `tests/perft_orda.rs`
// (each FSF-confirmed): the startpos, the developed middlegame, the all-pieces
// tactic, the White-promoted-Kheshig endgame, and a flag-race position.

variant_test!(
    orda,
    Chess8x8,
    OrdaRules,
    "orda",
    [
        "fwyskywf/8/pppppppp/8/4P3/8/PPPP1PPP/RNBQKBNR b KQ - 0 1",
        "1wysk1w1/8/p1pppp1p/8/2f2f2/PP4PP/2PPPP2/RNBQKBNR b KQ - 0 1",
        "4k3/8/3y4/2f1s3/2P1P3/3w4/8/4K3 b - - 0 1",
        "fwysk1wf/8/8/8/8/8/4W3/4K3 b - - 0 1",
        "8/4K3/8/8/8/8/4k3/8 w - - 0 1",
    ]
);

// -- Shako (cannons + Fers-Alfil elephant, 10x10) ---------------------------

variant_test!(
    shako,
    Grand10x10,
    ShakoRules,
    "shako",
    [
        "c8c/vr1bqkbnrv/pp1ppppppp/2p7/2C4c2/3P6/10/PP2PPPPPP/VRNBQKBNRV/C8C w - - 0 1",
        "c8c/vrnbqkbnrv/pp2pppppp/2pp6/10/2C2c4/3P6/PP2PPPPPP/VRNBQKBNRV/C8C w - - 0 1",
        "c8c/vrnbqkbnrv/ppppp1pppp/10/10/5p4/5C4/PPPPP1PPPP/VRNBQKBNRV/C8C b KQkq - 0 1",
        "c8c/vr3k2rv/pppppppppp/10/10/10/10/PPPPPPPPPP/VR3K2RV/C8C w KQkq - 0 1",
        "5k4/1P8/10/10/10/10/10/10/10/5K3C w - - 0 1",
    ]
);

// -- Xiangqi (horse leg + soldier direction + cannon + flying general) ------

variant_test!(
    xiangqi,
    Xiangqi9x10,
    XiangqiRules,
    "xiangqi",
    [
        "r1oukuo1r/9/1cj3jc1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1CJ3JC1/9/R1OUKUO1R w - - 0 1",
        "rjoukuojr/9/4c4/z1z1C1z1z/9/9/Z1Z3Z1Z/1C5c1/9/RJOUKUOJR b - - 0 1",
        "r1oukuo1r/9/1c2c4/z3z3z/2z3z2/2Z3Z2/Z3Z3Z/1C2C4/9/R1OUKUO1R w - - 0 1",
        "4k4/9/9/9/9/9/9/9/4R4/4K4 w - - 0 1",
        "4k4/9/9/9/9/9/9/4j4/3U5/3K5 w - - 0 1",
        "9/9/3k5/4Z4/9/9/9/9/9/4K4 b - - 0 1",
        "9/3k5/4Z4/9/9/9/9/9/9/4K4 b - - 0 1",
    ]
);

// -- Minixiangqi (7x7) ------------------------------------------------------

variant_test!(
    minixiangqi,
    Minixiangqi7x7,
    MinixiangqiRules,
    "minixiangqi",
    [
        "r1jkjcr/z1zzz1z/2c4/2J4/7/Z1ZZZ1Z/R1CKJCR w - - 0 1",
        "r1jkj1r/z1z1z1z/2c4/2C4/7/Z2Z2Z/R2KJ1R b - - 0 1",
        "3k3/7/3J3/2Z1Z2/7/7/3K3 w - - 0 1",
        "7/3k3/2ZJZ2/7/7/7/3K3 b - - 0 1",
        "3k3/7/7/3R3/7/7/3K3 w - - 0 1",
    ]
);

// -- Shogi (hand / drops / forward steppers, 9x9) ---------------------------

variant_test!(
    shogi,
    Shogi9x9,
    ShogiRules,
    "shogi",
    [
        "lnsgkgsnl/1r5b1/p1ppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL[Pp] w - - 0 1",
        "lnsgkgsnl/1r5b1/pppppp1pp/9/9/9/PPPPPP1PP/1B5R1/LNSGKGSNL[Pp] b - - 0 1",
        "4k4/9/9/9/9/9/9/9/4K4[RBGSNLPrbgsnlp] w - - 0 1",
        "4k4/PPPPPPPPP/9/9/9/9/9/ppppppppp/4K4[] w - - 0 1",
        "9/4P4/9/9/9/9/9/9/4k1K2[] w - - 0 1",
        "k8/9/9/9/9/9/9/9/LR2K4[P] w - - 0 1",
    ]
);

// -- Minishogi (5x5) --------------------------------------------------------

variant_test!(
    minishogi,
    Minishogi5x5,
    MinishogiRules,
    "minishogi",
    [
        "rbsgk/5/5/5/KGSBR[Pp] w - - 0 1",
        "k4/5/5/5/4K[RBGSPrbgsp] w - - 0 1",
        "4k/4P/5/5/4K[] w - - 0 1",
        "4k/4S/5/5/4K[] w - - 0 1",
        "2k2/5/R3r/5/2K2[Pp] w - - 0 1",
        "2k2/5/P4/5/2K2[P] w - - 0 1",
    ]
);

// -- Janggi (board-aware screen-cannon + palace diagonals + soldier, 9x10) --
//
// The cannon's attack set depends on which occupied squares hold cannons (and on
// the palace geometry), so `forward_attacks_to` projects it from the board-aware
// `role_attacks_board` hook above — without that, the cannon and palace-diagonal
// rows would spuriously mismatch. The corpus FENs are reused from
// `tests/perft_janggi.rs` (each FSF-confirmed): the startpos, the screen-cannon,
// the cannon palace-diagonal jump, the palace diagonals, the long elephant, the
// sideways/diagonal soldier, and the pass / in-check positions.

variant_test!(
    janggi,
    Xiangqi9x10,
    JanggiRules,
    "janggi",
    [
        "rjxu1uxjr/4k4/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/4K4/RJXU1UXJR w - - 0 1",
        "9/1k7/r1r3c2/9/9/9/J1C3J2/9/4K4/C1C3C2 w - - 0 1",
        "9/1k7/9/9/9/9/9/3K1r3/4J4/3C5 w - - 0 1",
        "9/4k4/9/9/9/9/9/3U5/9/3K1R3 w - - 0 1",
        "9/4k4/9/9/4Z4/3ZX4/9/9/9/1K7 w - - 0 1",
        "5k3/9/3Z5/9/9/4Z4/9/9/9/1K7 w - - 0 1",
        "9/1k7/9/9/9/9/4z4/9/4K4/9 w - - 0 1",
        "9/1k7/9/9/9/9/9/4z4/4K4/9 w - - 0 1",
    ]
);

// -- Synochess (asymmetric: standard White vs Janggi-cannon / Soldier / Commoner
//    / Fers-Alfil Black, 8x8) --
//
// The Janggi cannon is board-aware (screen ≠ cannon, may not capture a cannon),
// so `forward_attacks_to` projects it from `role_attacks_board`. The Soldier is
// forward-directional and the cannon occupancy-asymmetric (both flagged via
// `role_attack_is_leg_asymmetric`), so this is the guard that those flags match
// the generator. The corpus FENs are reused from `tests/perft_synochess.rs` (each
// FSF-confirmed): the startpos (both colors), an asymmetric middlegame, the
// drop-heavy position, and the two campmate endgames.

variant_test!(
    synochess,
    Chess8x8,
    SynochessRules,
    "synochess",
    [
        "rnv*ukvnr/8/1c4c1/1zz2zz1/8/8/PPPPPPPP/RNBQKBNR[zz] w KQ - 0 1",
        "rnv*ukvnr/8/1c4c1/1zz2zz1/8/8/PPPPPPPP/RNBQKBNR[zz] b KQ - 0 1",
        "rnv*uk1nr/8/1c4c1/3zz3/2zP4/5N2/PPP1PPPP/RNBQKB1R[zz] w KQ - 0 1",
        "rnv*uk1nr/8/1c4c1/8/3PP3/8/PPP2PPP/RNBQKBNR[zz] b KQ - 0 1",
        "8/8/8/8/K7/8/4k3/8 b - - 0 1",
        "8/4K3/8/8/8/8/4k3/8 w - - 0 1",
    ]
);

// -- Shinobi (fixed-reserve hand + drops, forward Shogi-Knight/Lance, mandatory
// per-piece promotion zone, flag win, 8x8) ---------------------------------
//
// Exercises the forward-directional Shogi Knight and Lance (which must
// reverse-project with the opposite color in `attackers_to`), the Commoner,
// Bers (= General), Archbishop (= Hawk), and Fers (= Met). The corpus FENs are
// reused from `tests/perft_shinobi.rs` (each FSF-confirmed): the startpos and
// three middlegames with drop reserves and pieces near the promotion zone. The
// Shogi Knight and Commoner are overflow roles (tokens `*N` / `*U`, recycling
// `n` / `u`); the board FEN parser resolves the `*` prefix.

variant_test!(
    shinobi,
    Chess8x8,
    ShinobiRules,
    "shinobi",
    [
        "r1bqkbnr/ppp2ppp/2n5/3p4/2Lp4/3M*N3/PPPPPPPP/L*N1*UK1*NL[AM] w kq - 0 5",
        "r1bqk2r/ppp1bppp/2n1p2n/2p2*N2/3M4/8/PPPPPPPP/L*N1*UK1*NL[LAM] w kq - 0 7",
        "r1bqk2r/1pppbppp/p1n1pn2/P7/L7/1*NM5/1PPPPPPP/1*N1*UK1*NL[LADM] w kq - 2 6",
    ]
);
