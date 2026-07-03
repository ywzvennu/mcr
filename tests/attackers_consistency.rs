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
    AseanRules, Bitboard, BughouseRules, CambodianRules, CannonShogiRules, CapablancaRules,
    CapahouseRules, ChakRules, ChancellorRules, ChennisRules, CourierRules, DobutsuRules,
    DragonRules, DuckRules, EmpireRules, FogOfWarRules, GenericPosition, Geometry, GorogoroRules,
    GrandRules, GrandhouseRules, HoppelPoppelRules, JanggiRules, JieqiRules, JudkinsRules,
    KhansRules, KnightmateRules, KyotoshogiRules, MakpongRules, MakrukRules, ManchuRules,
    MansindamRules, MinishogiRules, MinixiangqiRules, OpulentRules, OrdaRules, OrdamirrorRules,
    PlacementRules, SeirawanRules, ShakoRules, ShatarRules, ShatranjRules, ShinobiRules,
    ShoShogiRules, ShogiRules, ShogunRules, ShouseRules, SittuyinRules, SpartanRules, Square,
    StandardChess, SynochessRules, TencubedRules, ToriRules, WashogiRules, WideRole, WideVariant,
    XiangfuRules, XiangqiRules,
};
use mce::geometry::{
    Cap10x8, Chennis7x7, Chess8x8, Chess9x9, Courier12x8, Dobutsu3x4, Gorogoro5x6, Grand10x10,
    Judkins6x6, Minishogi5x5, Minixiangqi7x7, Shogi9x9, Tori7x7, Washogi11x11, Xiangqi9x10,
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
/// generator uses. But a role whose **threat** set depends on *which* occupied
/// squares hold *which* pieces (the Janggi screen-cannon: screen ≠ cannon, may
/// not capture a cannon, plus the palace-diagonal jump) is computed from the
/// **whole board** via the board-aware [`WideVariant::role_threats_board`] hook —
/// the pure threat set, which for Empire's "move like a Queen, capture short" pieces
/// excludes the quiet Queen slides that are moves but not attacks. When the variant
/// sets [`WideVariant::uses_board_attacks`] and that hook returns `Some` for the
/// role, this uses it — exactly as `attackers_to` and the cannon-verify king-safety
/// path do — so the board-aware role is forward-projected correctly. The hook
/// returns `None` for every other role (and the default-off variants never set
/// `uses_board_attacks`), so the existing cases keep using the plain `role_attacks`
/// path unchanged.
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
            // Prefer the board-aware *threat* set for any role the variant computes
            // from the whole board (Janggi's cannon, Empire's capture-short pieces);
            // fall back to the occupancy-only set. This mirrors `attackers_to`.
            let attacks = if board_aware {
                V::role_threats_board(role, by, from, board)
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

// -- Cannon Shogi (9x9) -----------------------------------------------------
//
// Cannon Shogi adds five occupancy-dependent CANNON-type movers (and four promoted
// forms) to the Shogi army, every one of which is `role_attack_is_leg_asymmetric`
// (its attack set turns on a screen under the live occupancy, so `attackers_to`
// forward-projects it, exactly as the generator and the per-move-verify king-safety
// path do). This guards that the orthogonal/diagonal hop attack relations — the
// Cannon, the rook/bishop hoppers, the diagonal bishop-cannon, and the promoted
// cannons' range-2 hops — match the forward relation. The Soldier (the reused Pawn)
// is forward-directional. The corpus pairs a promoted-cannon midgame with a
// drop-bearing hand and a screen-dense cannon position.

variant_test!(
    cannonshogi,
    Shogi9x9,
    CannonShogiRules,
    "cannonshogi",
    [
        "lns=Ukgsnl/1r=c=i1c=ab1/p3p1p1p/2p6/9/9/P1P1P1P1P/1B=A2=I=CR1/LNSGKGSNL[G] b - - 0 2",
        "4k4/9/9/9/9/9/9/9/4K4[RBC=A=C=Irbc=a=c=i] w - - 0 1",
        "4k4/9/2p1p4/3=U5/2P1P4/9/4=i4/2P1P4/4K4[Pp] w - - 0 1",
        "4k4/9/3p1p3/9/3=FcP3/9/3P1P3/9/4K4[] b - - 0 1",
    ]
);

// -- Bughouse (single-board, 8x8) -------------------------------------------
//
// Single-board Bughouse is standard chess plus crazyhouse-style drops from a hand
// that is fed externally; a capture does not bank into the captor's hand (it
// crosses to the partner board). The pieces and their attacks are the standard
// chess set, so this guards that adding the externally-fed hand left the attacker
// reverse-projection byte-identical. The corpus pairs the empty-hand start with
// hand-bearing positions (a `[NPnp]` midgame and a full `[QRBNPqrbnp]` reserve).

variant_test!(
    bughouse,
    Chess8x8,
    BughouseRules,
    "bughouse",
    [
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1",
        "r1bqk2r/ppp2ppp/2n5/3pp3/3PP3/2N5/PPP2PPP/R1BQK2R[NPnp] w KQkq - 0 1",
        "r3k2r/8/8/8/8/8/8/R3K2R[QRBNPqrbnp] w KQkq - 0 1",
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

// -- ASEAN (8x8) ------------------------------------------------------------
//
// ASEAN reuses Makruk's piece set and `role_attacks` relation — the only
// colour-directional attacker is the Khon (Silver), exactly as Makruk — and
// changes only the start array and the promotion rule. Neither touches the
// forward/reverse attack agreement this test guards, so the consistency check
// is the same shape as Makruk's. The corpus keeps two Khon-bearing midgames.

variant_test!(
    asean,
    Chess8x8,
    AseanRules,
    "asean",
    [
        "rnsmksnr/8/1ppppppp/p7/4P3/PPPP1PPP/8/RNSMKSNR b - - 0 2",
        "r1smks1r/3n4/ppp1pppp/3p4/3P4/PPP1PPPP/4N3/R1SMKS1R w - - 0 4",
    ]
);

// -- Makpong (8x8) ----------------------------------------------------------
//
// Makpong reuses Makruk's entire piece set and `role_attacks` relation — the
// only colour-directional attacker is the Khon (Silver), exactly as Makruk — and
// changes only **king-in-check legality** (the king may not flee). That rule
// filters which king *moves* are emitted; it never touches the forward/reverse
// attack agreement this test guards, so the consistency check is the same shape
// as Makruk's. The corpus keeps two genuinely-in-check FENs (a rook check and a
// pawn check) so `attackers_to(king, enemy)` is exercised on check-bearing nodes
// too, alongside the developed midgame.

variant_test!(
    makpong,
    Chess8x8,
    MakpongRules,
    "makpong",
    [
        "rnsmksnr/8/1pp1ppp1/p6p/3r4/PPP1PPPP/8/RNSK1SNR w - - 0 4",
        "rnsmksnr/8/ppp1ppp1/7p/8/PP1PPPPP/2pP4/RNSK1SNR w - - 0 5",
        "r1smks1r/3n4/ppp1pppp/3p4/3P4/PPP1PPPP/4N3/R1SKMS1R w - - 0 4",
    ]
);

// -- Cambodian / Ouk Chaktrang (8x8) ----------------------------------------
//
// Cambodian shares Makruk's piece set — the only colour-directional attacker is
// the Khon (Silver), exactly as Makruk — and adds one-time king / Met leaps that
// are emitted by the generator, not by the static `role_attacks` relation, so
// they do not affect the forward/reverse attack agreement this test guards. The
// corpus keeps the leap rights live (`DEde`) and develops pieces around the
// kings so the consistency check covers the leap-bearing positions too.

variant_test!(
    cambodian,
    Chess8x8,
    CambodianRules,
    "cambodian",
    [
        "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w DEde - 0 1",
        "rnsmksnr/8/pp1ppp1p/2p2p2/2P2P2/PP1PPP1P/8/RNSKMSNR w DEde - 0 3",
        "r1smks1r/3n4/ppp1pppp/3p4/3P4/PPP1PPPP/4N3/R1SKMS1R w DEde - 0 4",
    ]
);

// -- Shatar (Mongolian, 8x8) ------------------------------------------------
//
// The only non-standard piece is the Bers (`d`, reusing `WideRole::General` =
// Rook + Ferz), whose attack set is geometrically symmetric (rook slide + ferz
// step), so it needs no directional / leg-asymmetric flag — only the pawn is
// colour-directional, exactly as standard chess. This test is the guard that the
// Bers's `role_attacks` set and `attackers_to` reverse-projection agree, so a
// Bers check is detected on both the forward (king-danger) and reverse
// (`attackers_to`) paths. The corpus FENs are reused from `tests/perft_shatar.rs`
// (each FSF-confirmed, in mce dialect with the Bers spelled `d`): the startpos,
// the Bers-active and open middlegames, and the Robado-exercising position. (A
// bare-king node generates zero moves but `attackers_to` is still defined and
// must stay consistent, so the corpus keeps both sides armed.)

variant_test!(
    shatar,
    Chess8x8,
    ShatarRules,
    "shatar",
    [
        "rnbdkbnr/ppp1pppp/8/3p4/3P4/8/PPP1PPPP/RNBDKBNR w - - 0 1",
        "r3k2r/p1ppdpb1/bn2pnp1/3PN3/1p2P3/2N3p1/PPPBBPPP/R3K2R w - - 0 1",
        "4k3/8/8/3d4/3D4/8/4P3/4K3 w - - 0 1",
        "4k3/4p3/8/8/8/8/3D4/4K3 w - - 0 1",
    ]
);

// -- Shatranj (medieval, 8x8) -----------------------------------------------
//
// The two non-standard pieces are the Ferz (`m`, reusing `WideRole::Met` = one
// diagonal step) and the Alfil (`*x`, a pure two-square diagonal jumper). Both
// attack sets are geometrically symmetric leaps, so neither needs a directional /
// leg-asymmetric flag — only the pawn is colour-directional, exactly as standard
// chess. This test is the guard that the Alfil's (and Ferz's) `role_attacks` set
// and the `attackers_to` reverse-projection agree, so an Alfil check — delivered
// by a two-diagonal jump from a square *collinear* with the king — is detected on
// both the forward (king-danger) and reverse (`attackers_to`) paths. The corpus
// reuses the perft suite's startpos and Alfil/Ferz middlegames plus a position
// with an Alfil checking the king along a diagonal it jumps over. (Both sides
// stay armed so no node is a baring leaf, where the move set is empty but
// `attackers_to` is still defined and must stay consistent.)

variant_test!(
    shatranj,
    Chess8x8,
    ShatranjRules,
    "shatranj",
    [
        "rn1km1nr/pppppppp/3*x*x3/8/8/3*X*X3/PPPPPPPP/RN1KM1NR w - - 4 3",
        "r1*xk1*x1r/pppmpppp/2np1n2/8/8/2NPP3/PPPM1PPP/R1*XK1*XNR w - - 3 5",
        "rn*xkm1nr/pppppppp/8/8/1*x6/3P4/PPPKPPPP/RN*X1M*XNR w - - 3 3",
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

// -- Chancellor (9x9) -------------------------------------------------------
//
// Chancellor chess is standard western chess widened to 9x9 with a Rook + Knight
// Chancellor (`e` / `WideRole::Elephant`, geometrically symmetric) added to each
// back rank, so only the pawn is colour-directional, exactly as standard chess.
// This guards that the compound `role_attacks` and the `attackers_to` reverse-
// projection agree on the distinct [`Chess9x9`] geometry. The corpus (mce dialect,
// chancellor `e`) pairs a castling position with a developed midgame and a
// promotion position, reusing the FSF-confirmed FENs from
// `tests/perft_chancellor.rs`.

variant_test!(
    chancellor,
    Chess9x9,
    ChancellorRules,
    "chancellor",
    [
        "r3k3r/9/9/9/9/9/9/9/R3K3R w KQkq - 0 1",
        "r1bqke1br/ppp2pppp/2n2n3/3pp4/9/3PP4/2N2N3/PPP2PPPP/R1BQKE1BR w KQkq - 0 5",
        "4k4/P8/9/9/9/9/9/9/4K4 w - - 0 1",
    ]
);

// -- Courier (12x8) ---------------------------------------------------------
//
// Courier chess is standard western Rook / Knight / Bishop / King plus the
// short-range medieval leapers — the Courier (`*x` / [`WideRole::Alfil`], a
// two-square diagonal jumper), the Ferz (`m` / [`WideRole::Met`], one diagonal
// step), the Wazir (`*j` / [`WideRole::Wazir`], one orthogonal step), and the Man
// (`*u` / [`WideRole::Commoner`], a non-royal king). Every one of those attack
// sets is geometrically symmetric, so only the pawn is colour-directional, exactly
// as standard chess. This guards that each leaper's `role_attacks` set and the
// `attackers_to` reverse-projection agree on the distinct [`Courier12x8`] geometry
// — in particular that an Alfil check (a two-diagonal jump from a square collinear
// with the king) is detected on both the forward (king-danger) and reverse paths.
// The corpus (mce dialect) pairs the FSF-confirmed developed midgame and promotion
// positions from `tests/perft_courier.rs` with an Alfil-check position and a
// Wazir/Man/Ferz endgame (both sides armed so no node is a baring leaf).

variant_test!(
    courier,
    Courier12x8,
    CourierRules,
    "courier",
    [
        "r1*xb*uk1*jb*xnr/2ppp2pppp1/1pn2pm5/p5p4p/P5P4P/1P2*XPM5/2PPP2PPPP1/RN1B*UK1*JB*XNR w - - 0 4",
        "3r7k/1P10/12/12/12/12/12/K11 w - - 0 1",
        "6r4k/12/12/12/5*X6/12/12/K11 w - - 0 1",
        "7r3k/12/12/12/4*J*U6/5m6/12/K11 w - - 0 1",
    ]
);

// -- Capahouse (10x8, crazyhouse drops) -------------------------------------
//
// Capahouse shares Capablanca's pieces (the Archbishop `a` / `WideRole::Hawk` and
// Chancellor `e` / `WideRole::Elephant` compounds, both geometrically symmetric),
// so only the pawn is colour-directional, exactly as Capablanca. The added
// crazyhouse hand (the `[..]` bracket) and the promoted mask (a `~` token) never
// touch the attack sets — a promoted Queen attacks as a Queen — so this guards
// that the compound `role_attacks` and the `attackers_to` reverse-projection agree
// even with a hand in pocket and a promoted piece (`Q~`) on the board. The corpus
// mixes a castling position, a midgame with pieces in hand, and a promoted-queen
// position (all in mce dialect, chancellor `e`).

variant_test!(
    capahouse,
    Cap10x8,
    CapahouseRules,
    "capahouse",
    [
        "r4k3r/pppppppppp/10/10/10/10/PPPPPPPPPP/R4K3R[] w KQkq - 0 1",
        "1nabqkben1/p1ppppppp1/1r6r1/1p6p1/3PP5/2N4N2/PPP2PPPPP/R1ABQKBE1R[QPqp] w KQ - 0 5",
        "5k4/10/10/4Q~5/10/10/10/5K4[] w - - 0 1",
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

// -- Grandhouse (10x10, crazyhouse drops) -----------------------------------
//
// Grandhouse shares Grand's pieces (the Cardinal `a` / `WideRole::Hawk` and
// Marshal `e` / `WideRole::Elephant` compounds, both geometrically symmetric), so
// only the pawn is colour-directional, exactly as Grand. The added crazyhouse hand
// (the `[..]` bracket) and the promoted mask (a `~` token) never touch the attack
// sets — a promoted Queen attacks as a Queen — so this guards that the compound
// `role_attacks` and the `attackers_to` reverse-projection agree even with a hand
// in pocket and a promoted piece (`Q~`) on the board. The corpus mixes a developed
// midgame with pieces in hand and a promoted-queen position (mce dialect, marshal
// `e`).

variant_test!(
    grandhouse,
    Grand10x10,
    GrandhouseRules,
    "grandhouse",
    [
        "r8r/2bqkeab2/pppp1ppppp/2n4n2/3Np5/3P6/7N2/PPP1PPPPPP/2BQKEAB2/R8R[QPqp] w - - 1 4",
        "4k5/8P1/10/10/4Q~5/10/10/10/10/RNBQK1EAN1[Nn] w - - 0 1",
    ]
);

// -- Ten-Cubed (10x10) ------------------------------------------------------
//
// Ten-Cubed adds two new pure leapers to the Grand army: the Wizard (`**w` /
// `WideRole::Wizard`, Camel + Ferz) and the Champion (`**x` /
// `WideRole::TencubedChampion`, Wazir + Alfil + Dabbaba). Both attack sets are
// geometrically symmetric, so `attackers_to` reverse-projects them with no
// override; this guards that the two new leapers' `role_attacks` and the
// reverse-projection agree (alongside the reused symmetric Marshal `e` / Cardinal
// `a` compounds). Corpus in the mce dialect (`**w`, `**x`).

variant_test!(
    tencubed,
    Grand10x10,
    TencubedRules,
    "tencubed",
    [
        "k9/10/10/4**x5/10/3**W6/10/10/2ae6/9K w - - 0 1",
        "2**x**wae**w**x2/1rnbqkbnr1/10/pppppppppp/10/10/PPPPPPPPPP/10/1RNBQKBNR1/2**X**WAE**W**X2 w - - 0 1",
    ]
);

// -- Opulent (10x10) --------------------------------------------------------
//
// Opulent adds three new leapers to the Grand-style army: the Wizard (`**w` /
// `WideRole::Wizard`, Camel + Ferz), the Lion (`**y` / `WideRole::OpulentLion`,
// Ferz + Dabbaba + Threeleaper) and the augmented Knight (`**z` /
// `WideRole::OpulentKnight`, Knight + Wazir). Every one is geometrically
// symmetric, so `attackers_to` reverse-projects them with no override; this guards
// that all three new leapers' `role_attacks` and the reverse-projection agree
// (alongside the reused symmetric Chancellor `e` / Archbishop `a` compounds).
// Corpus in the mce dialect (`**w`, `**y`, `**z`).

variant_test!(
    opulent,
    Grand10x10,
    OpulentRules,
    "opulent",
    [
        "k9/10/4**y5/10/2**z4**w2/10/10/10/2ea6/9K w - - 0 1",
        "r**w6**wr/e**yb**zqk**zb**ya/10/pppppppppp/10/10/PPPPPPPPPP/10/E**YB**ZQK**ZB**YA/R**W6**WR w - - 0 1",
    ]
);

// -- S-House (Seirawan gating + crazyhouse drops, 8x8) ----------------------
//
// S-House shares Seirawan's pieces (the Hawk `a` / `WideRole::Hawk` and Elephant
// `e` / `WideRole::Elephant` compounds, both geometrically symmetric), so only the
// pawn is colour-directional. The unified crazyhouse hand (the `[..]` bracket) and
// the promoted mask (a `~` token) never touch the attack sets, so this guards that
// the compound `role_attacks` and the `attackers_to` reverse-projection agree even
// with a hand in pocket and a promoted piece on the board. Corpus in mce dialect
// (Hawk `a`).

variant_test!(
    shouse,
    Chess8x8,
    ShouseRules,
    "shouse",
    [
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[AEae] w KQBCDFGkqbcdfg - 0 1",
        "r1bqk2r/ppp2ppp/2n5/3pp3/3PP3/2N5/PPP2PPP/R1BQK2R[EANean] w KQCDkqcd - 0 1",
        "4k3/8/8/3Q~4/8/8/8/4K3[] w - - 0 1",
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

// -- Dragon (chess + Bishop+Knight Dragon dropped on the back rank, 8x8) ----

variant_test!(
    dragon,
    Chess8x8,
    DragonRules,
    "dragon",
    [
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[Aa] w KQkq - 0 1",
        "rnbqkbnr/pppppppp/8/8/3A4/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1",
        "r1bqk2r/pppp1ppp/2n2n2/2a1p3/2A1P3/2N2N2/PPPP1PPP/R2QK2R[] w KQkq - 0 1",
    ]
);

// -- Fog of War (standard movement, non-royal king, 8x8) --------------------

variant_test!(
    fogofwar,
    Chess8x8,
    FogOfWarRules,
    "fogofwar",
    [
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        "rnbqkbnr/ppppp1pp/8/5p1Q/4P3/8/PPPP1PPP/RNB1KBNR b KQkq - 1 2",
        "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
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

// -- Placement / Pre-Chess (deployment phase, 8x8) --------------------------
// The startpos is a deployment-phase position (both pockets in hand, no king on
// the board yet); the corpus adds a mid-deployment position, a non-standard
// fully-deployed array with castling, and a developed middlegame. Standard chess
// movement, so this guards that adding the deployment mechanic left the attacker
// reverse-projection byte-identical.
variant_test!(
    placement,
    Chess8x8,
    PlacementRules,
    "placement",
    [
        "rnb5/pppppppp/8/8/8/8/PPPPPPPP/RNB5[NBRQKnbrqk] w - - 0 4",
        "rbnqknbr/pppppppp/8/8/8/8/PPPPPPPP/RBNQKNBR w KQkq - 0 9",
        "rbnqk1br/ppp1pppp/5n2/3pP3/8/2N5/PPPP1PPP/R1BQKBNR w KQkq d6 0 9",
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

// -- Khan's Chess (Orda-family asymmetric army + soldier promotion + flag, 8x8) -
//
// The Khan army reuses the Orda Lancer / Archer (knight *move*, rook / bishop
// *capture* — `role_attacks` is the slider attack relation, the knight jumps are
// quiet-only) and Kheshig (King + Knight), and adds the **Khan** (knight *move*,
// **king** *capture* — its king-step `role_attacks` is the symmetric attack
// relation, its knight jumps are quiet-only) and the **Khan soldier** (forward
// half-knight *move*, straight-forward Wazir *capture* — its single forward-step
// `role_attacks` is the forward-biased attack relation, reverse-projected with the
// opposite colour via `role_attack_is_directional`; its half-knight leaps are
// quiet-only). The corpus is reused from `tests/perft_khans.rs` (each
// FSF-confirmed): the all-pieces tactic, the captures position, the developed
// middlegame, the soldier-promotion race, and a flag-race position.

variant_test!(
    khans,
    Chess8x8,
    KhansRules,
    "khans",
    [
        "4k3/8/3=t4/2f1=s3/2P1P3/3w1y2/8/4K3 b - - 0 1",
        "4k3/8/8/3=t4/2PPP3/3=s4/3P4/4K3 b - - 0 1",
        "f1y=tkywf/1=s=s=s=s=s1=s/2=s5/8/2P1P3/5N2/PP1P1PPP/RNBQKB1R b KQ - 0 1",
        "4k3/8/8/8/8/8/2=s1=s3/4K3 b - - 0 1",
        "8/4K3/8/8/8/8/4k3/8 w - - 0 1",
    ]
);

// -- Ordamirror (symmetric horde: Orda leapers + the Falcon + flag win, 8x8) -
//
// Both armies are Orda-style: the Lancer / Archer **move** like a knight but
// **capture** along a slider line (their `role_attacks` rook / bishop set is the
// attack relation; the knight jumps are quiet-only). The new **Falcon** is the
// inverse — it **moves** like a queen (quiet-only) but **captures** like a knight,
// so its knight pattern is the symmetric attack relation and its queen slides
// never enter it. The corpus is reused from `tests/perft_ordamirror.rs` (each
// FSF-confirmed): the developed middlegame, the two-Falcon tactic, the
// both-Falcons middlegame, and a flag-race position.

variant_test!(
    ordamirror,
    Chess8x8,
    OrdamirrorRules,
    "ordamirror",
    [
        "fwy*fkywf/8/pppppppp/8/4P3/8/PPP1PPPP/FWY*FKYWF b - - 0 1",
        "fwy*fk1wf/8/p1pppppp/8/2y1P3/2P5/PP1P1PPP/FWY*FK1WF w - - 0 1",
        "4k3/8/3p1p2/3p*f3/3*F4/2P1P3/8/4K3 w - - 0 1",
        "fwy1k1wf/8/p1ppp1pp/5p2/2*f5/2*F5/PP1PPPPP/FWY1K1WF w - - 0 1",
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

// -- Manchu (asymmetric Xiangqi: the Banner super-piece, 9x10) --------------
//
// The Banner (Rook + Cannon + Horse) has an occupancy-asymmetric attack relation
// (its cannon part lands only on an occupied square; its horse part is hobbled by
// a leg adjacent to the Banner), so `forward_attacks_to` projects it from the
// board-aware `role_attacks_board` hook — without that, a Banner row would
// spuriously mismatch. Every other role is exactly the Xiangqi mover. The corpus
// reuses the FSF-confirmed FENs from `tests/perft_manchu.rs`: the startpos, the
// Banner centred (rook/cannon/horse in the open), the Black army to move, the
// Banner deep in enemy territory, the cannon-checkmate, and the rook-check.
variant_test!(
    manchu,
    Xiangqi9x10,
    ManchuRules,
    "manchu",
    [
        "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/9/9/*M1OUKUO2 w - - 0 1",
        "rjoukuojr/9/1c5c1/z1z1z1z1z/9/4*M4/Z1Z1Z1Z1Z/9/9/2OUKUO2 w - - 0 1",
        "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/9/9/*M1OUKUO2 b - - 0 1",
        "rjoukuojr/4*M4/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/9/9/2OUKUO2 w - - 0 1",
        "k8/9/9/9/z8/9/9/9/9/*M3K4 b - - 0 1",
        "4k4/9/9/9/9/9/9/9/9/4*M4 b - - 0 1",
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

// -- Judkins Shogi (hand / drops / Knight, 6x6) ------------------------------
//
// Judkins Shogi is Minishogi widened to 6x6 with the Shogi Knight added (the only
// new forward-biased leaper); like Minishogi its Gold/Silver/Pawn and the
// Gold-moving promoted minors are colour-directional (reverse-projected with the
// opposite colour), the Knight likewise (its 2-1 jumps point forward), and its
// Rook/Bishop and their promoted forms are symmetric sliders. This guards that the
// Knight's `role_attacks` set and the `attackers_to` reverse-projection agree — so
// a Knight check is detected on both the forward (king-danger) and reverse paths —
// on the distinct [`Judkins6x6`] geometry. The corpus reuses the FSF-confirmed
// FENs from `tests/perft_judkins.rs` (startpos, multi-hand drop swarm, the
// forward-jumping Knight, the forced Pawn, the nifu filter, and the rook midgame).

variant_test!(
    judkins,
    Judkins6x6,
    JudkinsRules,
    "judkins",
    [
        "5k/6/6/6/6/K5[RBGSNPrbgsnp] w - - 0 1",
        "5k/6/6/2N3/6/5K[] w - - 0 1",
        "5k/6/6/6/P5/5K[] w - - 0 1",
        "2k3/6/6/P5/6/2K3[P] w - - 0 1",
        "2k3/6/R4r/6/6/2K3[Pp] w - - 0 1",
    ]
);

// -- Gorogoro Shogi Plus (hand / drops / Lance + Knight, 5x6) ----------------
//
// FENs reused from `tests/perft_gorogoro.rs` (each FSF-confirmed): the startpos
// with the Lance/Knight pair in hand, a bare-king multi-hand drop swarm, the
// forward-sliding Lance, the forward-jumping Knight, and the nifu file filter.
variant_test!(
    gorogoro,
    Gorogoro5x6,
    GorogoroRules,
    "gorogoro",
    [
        "sgkgs/5/1ppp1/1PPP1/5/SGKGS[LNln] w - - 0 1",
        "2k2/5/5/5/5/2K2[LNPlnp] w - - 0 1",
        "2k2/5/5/5/2L2/2K2[] w - - 0 1",
        "2k2/5/5/2N2/5/2K2[N] w - - 0 1",
        "2k2/5/5/P4/5/2K2[P] w - - 0 1",
    ]
);

// -- Tori Shogi (bird shogi: asymmetric quails + forward birds, 7x7) --------
//
// Every Tori bird's attack set is geometrically asymmetric (forward-biased, and
// the two quails are not even left-right symmetric), so the variant routes them
// all through the leg-asymmetric forward-projection path in `attackers_to`. This
// corpus — the startpos, a quail-active midgame (both asymmetric quails plus a
// crane of each colour), a promoted-piece midgame (Goose / Eagle of each colour),
// and a drop/promotion midgame — exercises that path against the generator. The
// FENs are reused from `tests/perft_torishogi.rs` (each FSF-confirmed).

variant_test!(
    torishogi,
    Tori7x7,
    ToriRules,
    "torishogi",
    [
        "*r*z*kk*k*z*v/3*a3/*y*y*y*y*y*y*y/2*y1*Y2/*Y*Y*Y*Y*Y*Y*Y/3*A3/*V*Z*KK*K*Z*R[] w - - 0 1",
        "3k3/7/2*v*r3/7/3*V*R2/7/3K3[*Y*y*A*a] w - - 0 1",
        "3k3/2*G4/7/3*I3/7/2*i4/3K3[*Y*y] w - - 0 1",
        "2k4/1*Y5/7/3*A3/7/5*y1/4K2[*Y*A*y*a] w - - 0 1",
        "*r*z*kk*k*z*v/3*a3/*y*y*y*y*y*y*y/7/*Y*Y*Y*Y*Y*Y*Y/3*A3/*V*Z*KK*K*Z*R[*Y*y] w - - 0 1",
    ]
);

// -- Wa Shogi (11x11 animal shogi: forward-biased steppers and directional
//    sliders) ----------------------------------------------------------------
//
// Twenty-one of Wa's thirty-one piece kinds are forward-biased (a Sparrow Pawn, the
// forward-sliding Oxcart / Liberated Horse / Running Rabbit, the Cloud Eagle, and
// the rest), routed through the `role_attack_is_directional` colour-flipped
// reverse-projection — correct because every Wa piece is left-right symmetric. The
// remaining ten (Crane King, Heavenly Horse, Treacherous Fox, Plodding Ox, Bear's
// Eyes, Gliding Swallow, Swallow's Wings, Tenacious Falcon and the role-sharing
// promotions) are fully symmetric and take the default reverse projection. This
// corpus exercises both paths: the startpos, a contact midgame with the armies
// engaged on several files, a promoted-piece board (a Gliding Swallow, Tenacious
// Falcon and Heavenly Horse of each colour) and a drop/promotion midgame with hands.
// Rules-validated (no FSF/HaChu perft oracle); the FENs are hand-constructed.

variant_test!(
    washogi,
    Washogi11x11,
    WashogiRules,
    "washogi",
    [
        "**f**j**h**l**nk**o**k**g**m**d/1**v3**q3**t1/\
**b**b**b**r**b**b**b**u**b**b**b/11/11/11/11/11/\
**B**B**B**U**B**B**B**R**B**B**B/1**T3**Q3**V1/\
**D**M**G**K**OK**N**L**H**J**F[] w - - 0 1",
        "5k5/11/11/3**v3**t3/11/5K5/11/3**V3**T3/11/11/11 w - - 0 1",
        "5k5/11/11/3=x3=z3/5=h5/5K5/5=H5/3=X3=Z3/11/11/11[**b**B] w - - 0 1",
        "5k5/11/4**b1**b4/11/11/5K5/11/4**B1**B4/11/11/11[**b**B**t**T**q**Q] w - - 0 1",
    ]
);

// -- Kyoto Shogi (5x5 per-move flipping) ------------------------------------
//
// Every non-royal piece carries two forms and alternates between them each move.
// The consistency guard exercises both directional forward-biased forms (the base
// Pawn / Silver / Knight and the Gold-moving promoted Lance / Knight, flagged via
// `role_attack_is_directional`) and the color-symmetric promoted sliders (the
// `+P` Rook and `+S` Bishop), confirming `attackers_to` reverse-projects each
// flipping form exactly as the forward generator emits it. The corpus is reused
// from `tests/perft_kyotoshogi.rs`: the startpos, a Silver+Pawn dual-form-drop
// position, the all-base-roles drop pool, a board with a promoted Silver, and the
// two-promoted-slider middlegame.

variant_test!(
    kyotoshogi,
    Minishogi5x5,
    KyotoshogiRules,
    "kyotoshogi",
    [
        "2k2/5/5/5/2K2[SPsp] w - - 0 1",
        "2k2/5/5/5/2K2[PSLNpsln] w - - 0 1",
        "p+nks+l/5/2+S2/5/+LSK+NP[] w - - 0 1",
        "1k3/5/1+s3/5/1K2+P[] w - - 0 1",
    ]
);

// -- Dobutsu (3x4 animal shogi) ---------------------------------------------
//
// The forward-biased pieces (the Chick `p` and the promoted Hen `+p`, a Gold
// General) are color-directional, so the forward-projection cross-check guards
// their reverse-projection (`role_attack_is_directional`). The non-royal Lion
// (a King-role stepper) and the Giraffe (Wazir `*j`) / Elephant (Met `m`)
// steppers are color-symmetric. The corpus reuses `tests/perft_dobutsu.rs`'s
// FSF-confirmed FENs: the startpos, the drop / multi-hand positions, the forced
// Chick promotion, and the Lion try-advance.

variant_test!(
    dobutsu,
    Dobutsu3x4,
    DobutsuRules,
    "dobutsu",
    [
        "*jkm/1p1/1P1/MK*J[] w - - 0 1",
        "*jkm/3/1P1/MK*J[p] w - - 0 1",
        "1k1/3/3/1K1[M*JPm*jp] w - - 0 1",
        "1k1/1P1/3/1K1[] w - - 0 1",
        "1k1/3/1K1/3[M*JP] w - - 0 1",
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

// -- Shogun (crazyhouse hand + drops, optional capped per-piece promotion zone,
// standard chess army, 8x8) ------------------------------------------------
//
// Exercises the promoted compounds — Centaur (= Kheshig, King + Knight),
// Archbishop (= Hawk, Bishop + Knight), Chancellor (= Elephant, Rook + Knight),
// Queen (the promoted Fers), and Commoner (= king-stepper) — alongside the bare
// Met (Fers) and the standard army. Every Shogun role is color-symmetric except
// the standard Pawn (the trait default already classifies it as directional), so
// this is the guard that `attackers_to`'s reverse projection of the compounds
// matches the generator. The corpus FENs are reused from `tests/perft_shogun.rs`
// (each FSF-confirmed): the drops-and-promotions position, the promotion-cap
// position (Centaur token `W`), and the captures-feeding-the-hand midgame (the
// `q` "queens" being promoted Fers). The Centaur and Commoner are overflow roles
// (tokens `W` / `*U`, recycling `w` / `u`); the board FEN parser resolves them.

variant_test!(
    shogun,
    Chess8x8,
    ShogunRules,
    "shogun",
    [
        "r3k3/8/4N3/8/8/8/8/3RK3[NPbp] w - - 0 1",
        "6k1/8/4N3/8/8/8/8/W5K1[Nn] w - - 0 1",
        "rnbqkbnr/ppp2ppp/8/3pp3/3PP3/8/PPP2PPP/RNBQKBNR[Pp] w KQkq - 0 4",
    ]
);

// -- Empire (asymmetric: standard Black vs the White Empire "move-Queen /
//    capture-short" army + flag win + flying general, 8x8) --
//
// Each Empire piece (Eagle / Cardinal / Tower / Duke) *moves* like a Queen onto an
// empty square but its *attack* (capture / check) set is a short pattern (knight /
// bishop / rook / king) — so its threat set is **not** its move set. They ride the
// board-aware verify path (`role_attacks_board` returns the quiet-Queen ∪
// short-capture set), and `attackers_to` / king-safety project that set forward
// from each piece (flagged via `role_attack_is_leg_asymmetric`); this is the guard
// that the projection matches the generator. The White Soldier is forward-
// directional (`role_attack_is_directional`). The corpus FENs are reused from
// `tests/perft_empire.rs` (each FSF-confirmed): the startpos (both colors), the
// developed middlegame, the all-capture-patterns tactic, the flag race, and the
// flying-general faceoff. The Empire pieces are overflow roles (tokens
// `*T *E *C *D`, recycling `t e c d`); the board FEN parser resolves the `*` prefix.

variant_test!(
    empire,
    Chess8x8,
    EmpireRules,
    "empire",
    [
        "rnbqkbnr/pppppppp/8/8/8/PPPZZPPP/8/*T*E*C*DK*C*E*T w kq - 0 1",
        "rnbqkbnr/pppppppp/8/8/8/PPPZZPPP/8/*T*E*C*DK*C*E*T b kq - 0 1",
        "rnbqkbnr/pp1ppppp/8/2p5/3P*E3/2P2P2/PP2Z1PP/*T1*C*DK1*E*T w kq - 0 1",
        "4k3/8/2n1n3/3rb3/3*E*C*T2/3q4/3P4/4K3 w - - 0 1",
        "4k3/8/8/8/8/8/4K3/8 w - - 0 1",
        "8/8/3k4/8/8/8/3K4/8 w - - 0 1",
    ]
);

// -- Knightmate (royal Knight + non-royal Commoner, 8x8) --------------------
//
// The royal piece is `WideRole::King` given the **knight** attack set, so this is
// the guard that a non-king-stepping royal still satisfies the attacker/king-safety
// consistency property: the forward king-danger projection and the reverse-
// projecting `attackers_to` must agree on the royal Knight's check, and on the
// Commoner's king-steps. Both are symmetric leapers, so no directional/leg-
// asymmetric flag is set — this test confirms that choice. The corpus FENs are
// reused from `tests/perft_knightmate.rs` (each FSF-confirmed): the closed-pawn and
// castling-ready middlegames, and the royal-Knight-in-check + promotion position.
// The Commoner is the overflow role `*U` (recycling `u`); the board FEN parser
// resolves the `*` prefix.

variant_test!(
    knightmate,
    Chess8x8,
    KnightmateRules,
    "knightmate",
    [
        "r*ubqkb*ur/pp2pppp/2pp4/8/2PP4/8/PP2PPPP/R*UBQKB*UR w KQkq - 0 1",
        "r3k2r/pppq1ppp/2*up1*u2/2b1p3/2B1P3/2*UP1*U2/PPPQ1PPP/R3K2R w KQkq - 0 1",
        "4k3/1P3P2/8/8/3*u4/8/4r3/4K3 w - - 0 1",
    ]
);

// -- Hoppel-Poppel (knight captures like a bishop, bishop like a knight, 8x8) -
//
// The Knight-Bishop (`*h`, FSF `mNcB`) **moves** like a knight (quiet-only) but its
// `role_attacks` set is the bishop slide, so it captures / checks along diagonals
// and slides in the attack relation (`role_is_slider`). The Bishop-Knight (`*b`,
// FSF `mBcN`) is the inverse: it **moves** like a bishop (quiet-only) but its attack
// set is the symmetric knight pattern (a leaper). Both capture sets are
// geometrically symmetric (bishop / knight), so only the pawn is colour-directional
// and neither needs leg-asymmetry — this is the guard that those flags match the
// generator. The corpus FENs are reused from `tests/perft_hoppelpoppel.rs` (each
// FSF-confirmed): the startpos (both colours), three bishop/knight-rich middlegames,
// and the tactic+promotion position. The two pieces are overflow roles (tokens
// `*h` / `*b`, recycling `h` / `b`); the board FEN parser resolves the `*` prefix.

variant_test!(
    hoppelpoppel,
    Chess8x8,
    HoppelPoppelRules,
    "hoppelpoppel",
    [
        "r*h*bqk*b*hr/pppppppp/8/8/8/8/PPPPPPPP/R*H*BQK*B*HR w KQkq - 0 1",
        "r*h*bqk*b*hr/pppppppp/8/8/8/8/PPPPPPPP/R*H*BQK*B*HR b KQkq - 0 1",
        "r1*bqk*b*hr/pppp1ppp/2*h5/4p3/4P3/2*H5/PPPP1PPP/R1*BQK*B*HR w KQkq - 0 1",
        "r2qk2r/ppp2ppp/2*hp1*h2/2*b1p1*B1/2*B1P1*b1/2*HP1*H2/PPP2PPP/R2QK2R w KQkq - 0 1",
        "2kr3r/pp1*h1ppp/2p1p*h2/q7/3P4/2*H*BP*H2/PPQ2PPP/2KR3R w - - 0 1",
        "4k3/Pp4*h1/8/3*b4/3*H4/8/1p2*H3/4K3 w - - 0 1",
    ]
);

// -- Chak (9x9 Mayan) -------------------------------------------------------
//
// Three Chak pieces ride the forward-projection (`role_attack_is_leg_asymmetric`)
// path: the **Quetzal** (`*q`, an eight-direction cannon — its over-screen capture
// is occupancy-asymmetric) and the region-confined **Shaman** (`*w`) and **Divine
// Lord** (`*l`) (their attack relation is keyed on the origin's half, so a reverse
// projection would invent attacks across the centre line). The **Soldier** (`*p`)
// is colour-directional (forward-diagonal capture). This test is the guard that
// each piece's `role_attacks` / `role_attacks_board` set and the `attackers_to`
// reverse-projection agree, on both the king-danger and the `attackers_to` paths.
// The corpus reuses the FSF-confirmed FENs from `tests/perft_chak.rs` (the
// startpos, a centred King, a developed middlegame, a Soldier in its promotion
// half, a Quetzal-active position, a two-royal pseudo-royal position with a Divine
// Lord and Shaman on the board, and a Divine Lord beside the enemy temple), each in
// mce dialect with the six new pieces spelled `*s *q *w *l *p *o`.

variant_test!(
    chak,
    Shogi9x9,
    ChakRules,
    "chak",
    [
        "rn*s*qkw*snr/4*o4/*p1*p1*p1*p1*p/9/9/9/*P1*P1*P1*P1*P/4*O4/RN*SWK*Q*SNR w - - 0 1",
        "rn*s*qkw*snr/4*o4/*p1*p1*p1*p1*p/9/9/4K4/*P1*P1*P1*P1*P/4*O4/RN*S*Q1*Q*SNR w - - 0 1",
        "rn*s*qkw*snr/4*o4/*p3*p1*p1*p/2*p6/9/2*P6/*P3*P1*P1*P/4*O4/RN*SWK*Q*SNR w - - 0 1",
        "rn*s*qkw*snr/4*o4/*p1*p1*p1*p1*p/9/4*P4/9/*P1*P1*P3*P/4*O4/RN*SWK*Q*SNR b - - 0 1",
        "rn*s*qkw*snr/4*o4/*p1*p1*p1*p1*p/9/9/2*P3*P2/*P1*P1*P1*P1*P/4*O4/RN*SWK*Q*SNR b - - 0 1",
        "rn*s*qk1*snr/4*o4/*p1*p1*p1*p1*p/3*L5/9/9/*P1*P1*P1*P1*P/4*O4/RN*S1K*Q*SNR w - - 0 1",
        "rn*s*qk1*snr/4*o4/*p1*p1*L1*p1*p/9/9/9/*P1*P1*P1*P1*P/4*O4/RN*S1K*Q*SNR w - - 0 1",
        "rn*s*qkw*snr/4*o4/*p1*p1*p1*p1*p/4*W4/9/4*w4/*P1*P1*P1*P1*P/4*O4/RN*SWK*Q*SNR w - - 0 1",
    ]
);

// -- Sho Shogi (old 9x9 Shogi without drops; Drunk Elephant / Crown Prince) --
//
// Sho Shogi reuses the whole Shogi army on `Shogi9x9` and adds the **Drunk
// Elephant** (`**e`, a seven-direction stepper — every King step but the
// straight-backward one, so its attack set is colour-directional) and the **Crown
// Prince** (`**c`, a full King and a second royal). This test guards that those
// two roles' `role_attacks` sets and the `attackers_to` reverse-projection agree
// on both the king-danger and `attackers_to` paths. The corpus reuses the
// FSF-confirmed FENs from `tests/perft_shoshogi.rs` (the startpos, a developed
// middlegame, a two-royal position, a Drunk Elephant in the promotion zone, and a
// lone Crown Prince in check), each in mce dialect with the doubled-overflow
// tokens `**e` / `**c`.
variant_test!(
    shoshogi,
    Shogi9x9,
    ShoShogiRules,
    "shoshogi",
    [
        "lnsgkgsnl/1r2**e2b1/ppppppppp/9/9/9/PPPPPPPPP/1B2**E2R1/LNSGKGSNL w - - 0 1",
        "lnsgkgsnl/1r2**e2b1/p1pppp1pp/1p4p2/9/2P3P2/PP1PPP1PP/1B2**E2R1/LNSGKGSNL w - - 0 1",
        "4k4/9/9/9/9/9/4**C4/9/4K4 w - - 0 1",
        "4k4/9/4**E4/9/9/9/9/9/4K4 w - - 0 1",
        "3k5/9/9/9/9/9/9/r8/4**C4 w - - 0 1",
    ]
);

// -- Mansindam (9x9 Korean shogi-chess hybrid: hand + drops, mandatory promotion,
// campmate flag win) -------------------------------------------------------
//
// The only colour-directional attacker is the Shogi-style Pawn (it captures
// straight ahead, `role_attack_is_directional`); the standard chess Knight, the
// Cardinal (= Hawk, Bishop + Knight), the Marshal (= Elephant, Rook + Knight), the
// Queen, and the three new compounds — the Angel (`**a`, Bishop + Rook + Knight),
// the promoted Rhino (`**i`, Bishop + Knight + Wazir) and the promoted Ship
// (`**s`, Rook + Knight + Ferz), plus the reused Guard (= Commoner), Centaur
// (= Kheshig), Archer (`+B` = Dragon Horse) and Tiger (`+R` = Dragon) — are all
// colour-symmetric, so this guards that their `role_attacks` sets and the
// `attackers_to` reverse-projection agree on both the king-danger and `attackers_to`
// paths. The corpus reuses the FSF-confirmed FENs from `tests/perft_mansindam.rs`:
// the startpos, the drop swarm, the promoted-mover board, the lone Angel, and the
// capture-to-hand midgame, each in mce dialect (Cardinal `a`, Marshal `e`, Angel
// `**a`, Rhino `**i`, Ship `**s`).
variant_test!(
    mansindam,
    Shogi9x9,
    MansindamRules,
    "mansindam",
    [
        "rnb**akqane/9/ppppppppp/9/9/9/PPPPPPPPP/9/ENAQK**ABNR[] w - - 0 1",
        "4k4/9/9/9/9/9/9/9/4K4[NBRnbr] w - - 0 1",
        "9/k8/2+B1+R1**I2/9/2**S1W1*U2/9/9/9/4K4[Nn] w - - 0 1",
        "1k7/9/9/9/4**A4/9/9/9/4K4[] w - - 0 1",
        "rnb**akqane/9/ppp2pppp/3pP4/9/9/PPPP1PPPP/9/ENAQK**ABNR[P] b - - 0 3",
    ]
);

// -- Chennis (7x7 tennis-themed flipping variant: hand + dual-form drops, per-move
// flip, king mobility region) ----------------------------------------------
//
// The colour-directional attackers are the new Chennis Pawn (`**p`, a chess pawn
// that captures forward-diagonally, `role_attack_is_directional`) and the Soldier
// (`z`, forward / sideways). The leg-asymmetric attackers are the Cannon (`c`, its
// over-screen capture lands only on an occupied square) and the King (`k`, masked
// to its mobility region by origin), both forward-projected. The Ferz (= Met `m`),
// Commoner (`*u`), Knight (`n`), Rook (`r`) and Bishop (`b`) are colour-symmetric,
// so this guards that every role's `role_attacks` / `role_attacks_board` set and
// the `attackers_to` projection agree on both the king-danger and `attackers_to`
// paths. The corpus reuses the FSF-confirmed FENs from `tests/perft_chennis.rs`:
// the startpos, the flipping middlegame (a promoted Rook / Bishop / Cannon on the
// board), and the two drop swarms, each in mce dialect (Ferz `m`, Soldier `z`,
// Commoner `*u`, Pawn `**p`).
variant_test!(
    chennis,
    Chennis7x7,
    ChennisRules,
    "chennis",
    [
        "1mk*u3/1**p1z3/7/7/7/3Z1**P1/3*UKM1[] w - - 0 1",
        "1mk*u3/3z3/1r5/7/3B3/5**PC/3*UK2[] b - - 4 2",
        "3k3/7/7/7/7/7/3K3[**PMZ*U**pmz*u] w - - 0 1",
        "3k3/7/7/7/7/7/3K3[**P**p] w - - 0 1",
    ]
);

// -- Xiang Fu (9x9 Xiangqi-themed drop variant) -----------------------------
//
// Xiang Fu fields the hobbled Horse, the orthogonal Cannon, and the diagonal
// Crossbow (bishop-cannon) — all `role_attack_is_leg_asymmetric` (their attack set
// is a hobbled leap or a screen-dependent over-one-screen capture, so `attackers_to`
// forward-projects them, exactly as the generator and the per-move-verify king-safety
// path do) — plus the ring-confined royal Champion (also leg-asymmetric: its attack
// relation is keyed on its origin, the ring). The Mahout is a symmetric two-square
// leaper (its blocking leg is the midpoint), so it stays on the standard reverse
// projection; this guards that classification too. The corpus reuses the
// FSF-confirmed FENs from `tests/perft_xiangfu.rs`: the startpos, a Horse/cannon
// midgame, the captures-to-hand drops position (Pupils in hand), and the
// duple-check position with the Champions adjacent.
variant_test!(
    xiangfu,
    Shogi9x9,
    XiangfuRules,
    "xiangfu",
    [
        "2rb=m4/2c=cj4/2=k1=k4/9/9/9/4=K1=K2/4J=CC2/4=MBR2[] w - - 0 1",
        "2rb=m4/2c=cj4/2=k1=k4/9/9/9/2J1=K1=K2/5=CC2/4=MBR2[] b - - 2 1",
        "2rb=m4/2c=cj4/2=k1=k4/9/9/9/4=K1=K2/4J=CC2/4=MBR2[*U*u] w - - 0 1",
        "2rb=m4/2c=cj4/2=k6/4=k4/9/4=K4/6=K2/4J=CC2/4=MBR2[] w - - 3 2",
    ]
);

// -- Jieqi (hidden Xiangqi, 9x10) -------------------------------------------
//
// Jieqi reuses the Xiangqi movers wholesale and adds the face-down Dark piece.
// The Dark piece is `role_attack_is_leg_asymmetric` (it stands in for whichever
// asymmetric Xiangqi mover is native to its home square — Horse / Cannon /
// Soldier / region-confined General-Advisor-Elephant — so `attackers_to`
// forward-projects its effective attack set exactly as the generator does). The
// startpos playout reveals pieces under the identity baseline, so the corpus mixes
// the all-dark start, a fully-revealed Xiangqi middlegame, and two revealed
// tactical positions (a horse check, a flying-general pin) — every one a position
// whose attacker relation the property below pins.
variant_test!(
    jieqi,
    Xiangqi9x10,
    JieqiRules,
    "jieqi",
    [
        "=d=d=d=dk=d=d=d=d/9/1=d5=d1/=d1=d1=d1=d1=d/9/9/=D1=D1=D1=D1=D/1=D5=D1/9/=D=D=D=DK=D=D=D=D w - - 0 1",
        "r1oukuo1r/9/1cj3jc1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1CJ3JC1/9/R1OUKUO1R w - - 0 1",
        "4k4/9/9/9/9/9/9/4j4/3U5/3K5 w - - 0 1",
        "4k4/9/9/9/9/9/9/9/4R4/4K4 w - - 0 1",
    ]
);
