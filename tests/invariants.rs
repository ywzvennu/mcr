//! Cross-variant invariant suite (issue #369).
//!
//! Where the `perft_*` suites pin exact node counts against Fairy-Stockfish and
//! the make/unmake / attackers suites guard one property each, this suite asserts
//! the *structural* invariants that must hold at **every** reachable position of
//! **every** registered fairy variant — the whole [`WideVariantId::ALL`] table in
//! `src/geometry/any.rs` (54 variants, from 3x4 Dobutsu to 11x11-backed boards,
//! spanning drops, gating, placement, the Duck, the Janggi pass, and Alice's two
//! planes). It is additive: it drives only the public API and changes no
//! generation path, so existing movegen stays byte-identical.
//!
//! # How positions are reached
//!
//! A random FEN is almost never legal, and these invariants are only meaningful on
//! legal, reachable positions. So every position is reached by **seeded random
//! self-play**: from a variant's start position, play uniformly random legal moves
//! (a tiny dependency-free splitmix64 PRNG picks the index), asserting the
//! invariants at every node along the way. The seeds are a fixed list, so the
//! whole suite is fully deterministic and reproducible — it draws nothing from the
//! clock or the environment. A companion `proptest` block adds broad randomized
//! cross-variant coverage (with shrinking) over the runtime-dispatch
//! [`AnyWideVariant`] surface.
//!
//! # Invariants checked
//!
//! 1. **make/unmake round-trip (byte-for-byte).** For every legal move at every
//!    visited node, [`apply_with_undo`](mce::geometry::GenericPosition::apply_with_undo)
//!    reaches exactly the [`play`](mce::geometry::GenericPosition::play) successor,
//!    and the matching [`undo`](mce::geometry::GenericPosition::undo) restores the
//!    prior position **byte-for-byte** — the [`Board`], the whole
//!    [`GenericState`] (side, castling, en passant, gating reserves/eligibility,
//!    Duck, placement pocket, both clocks, the Janggi consecutive-pass counter,
//!    and the Alice plane mask), and the position **hash**. Equality is asserted
//!    over the public `board() == board()`, `state() == state()`, and
//!    `zobrist() == zobrist()`; the from-scratch [`zobrist`](mce::geometry::GenericPosition::zobrist)
//!    folds in the crazyhouse promoted mask, so the triple is a complete
//!    byte-for-byte check.
//!
//! 2. **FEN round-trip.** For every reachable position, `to_fen` is a fixed point:
//!    `parse(fen(p)).fen() == fen(p)`, in the mce FEN dialect (overflow letters,
//!    hand `[..]` brackets, gating, `~` promotion marks). For every variant whose
//!    FEN is **lossless** (all but Alice — see below) the re-parse also reproduces
//!    the position **hash** (`zobrist(parse(fen(p))) == zobrist(p)`), the strict
//!    `parse(fen(p)) == p` on the hash-observable state.
//!
//! 3. **hash / Zobrist consistency.** make-then-unmake restores the hash (part of
//!    invariant 1). The position hash is computed *from scratch* as a pure function
//!    of the (board, state) — never carried incrementally by the history-free
//!    [`GenericPosition`] — so it is inherently **path-independent**: any two
//!    positions equal as values hash equal regardless of the move order that
//!    reached them. This is asserted directly as hash determinism across two
//!    independent re-parses of the same FEN.
//!
//! 4. **legal-move-list integrity.** No legal-move list contains a duplicate move,
//!    and every legal move is a well-formed, uniquely addressable move — it renders
//!    to a SAN string that parses back to exactly itself (`parse_san(san(m)) == m`),
//!    so no two moves collide on a rendering. This is the publicly checkable
//!    content of "legal ⊆ pseudo-legal": the crate exposes no pseudo-legal
//!    generator, but a spurious or duplicated move in the legal list — the class of
//!    bug that framing guards against — shows up here as a duplicate value or a
//!    non-round-tripping rendering. SAN is the human move language, so it is the
//!    natural rendering to check here; it spells Kyoto Shogi's two-form drops as
//!    `L@a1` vs. `+L@a1` (face-up vs. flipped). Since #452 UCI carries the same
//!    `+` prefix and is equally injective — the dedicated UCI round-trip lives in
//!    `notation_roundtrip.rs` and the `wide_uci_round_trip` property.
//!
//! # The Alice FEN caveat (documented, not a bug)
//!
//! Alice chess plays over two mirror boards; a piece's plane rides in the
//! [`GenericState::board_b`](mce::geometry::GenericState) mask, and mce
//! deliberately reuses the **standard** FEN, which cannot express plane
//! membership — a re-parse returns every piece to plane A (documented in
//! `src/geometry/variants/alice.rs`). So Alice's FEN is lossy for any position with
//! a piece on plane B: the `to_fen` fixpoint still holds (invariant 2, first half),
//! but the strict hash re-parse does not, and is skipped for Alice alone. Its full
//! two-plane state is still guarded byte-for-byte by the make/unmake round-trip
//! (invariant 1), which carries `board_b` in `state`.

use std::collections::BTreeSet;

use mce::geometry::{
    Alice, Almost, Amazon, AnyWideVariant, Asean, Bughouse, Cambodian, CannonShogi, Capablanca,
    Capahouse, Chak, Chennis, Chigorin, Dobutsu, Dragon, Duck, Embassy, Empire, FogOfWar,
    GenericPosition, Geometry, Gorogoro, Gothic, Grand, Grandhouse, HoppelPoppel, Janggi, Janus,
    Jieqi, Khans, Knightmate, Kyotoshogi, Makpong, Makruk, Manchu, Mansindam, Minishogi,
    Minixiangqi, Orda, Ordamirror, Placement, Seirawan, Shako, Shatar, Shatranj, Shinobi, ShoShogi,
    Shogi, Shogun, Shouse, Sittuyin, Spartan, Synochess, Tori, WideMove, WideVariant,
    WideVariantId, Xiangfu, Xiangqi,
};
use proptest::prelude::*;

/// One step of splitmix64 — a tiny, fully deterministic, dependency-free PRNG,
/// used only to pick move indices. The same generator the sibling suites use.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Seeds for the default (fast) per-variant walks. Fixed, so the suite is fully
/// deterministic; varying the seed varies the random line, and any failure always
/// reproduces from the printed seed/FEN.
const SEEDS_FAST: &[u64] = &[
    0x0000_0000_0000_0001,
    0xDEAD_BEEF_CAFE_F00D,
    0x1234_5678_9ABC_DEF0,
];

/// Plies walked per seed in the default suite. Each node asserts every invariant,
/// so the effective position count per seed is up to `PLIES_FAST + 1`.
const PLIES_FAST: usize = 16;

/// The heavier `#[ignore]`d sweep: more seeds, deeper walks. Run with
/// `cargo test --all-features -- --ignored`.
const SEEDS_DEEP: &[u64] = &[
    0x0000_0000_0000_0001,
    0xDEAD_BEEF_CAFE_F00D,
    0x1234_5678_9ABC_DEF0,
    0x0F0F_0F0F_0F0F_0F0F,
    0xA5A5_5A5A_C3C3_3C3C,
    0xFEDC_BA98_7654_3210,
];

/// Plies walked per seed in the deep sweep.
const PLIES_DEEP: usize = 48;

/// Byte-for-byte position equality over the public surface: the board, the whole
/// state, and the from-scratch hash (which folds in the promoted mask). Two
/// positions equal here are indistinguishable in board, side, castling, en
/// passant, gating, Duck, placement pocket, clocks, pass counter, Alice plane, and
/// promotion state.
fn positions_equal<G: Geometry, V: WideVariant<G>>(
    a: &GenericPosition<G, V>,
    b: &GenericPosition<G, V>,
) -> bool {
    a.board() == b.board() && a.state() == b.state() && a.zobrist() == b.zobrist()
}

/// Invariant 4: the legal-move list has no duplicate move, and every legal move
/// round-trips through SAN to exactly itself (so every move is well-formed and
/// uniquely addressable).
fn assert_move_list_integrity<G: Geometry, V: WideVariant<G>>(
    pos: &GenericPosition<G, V>,
    variant: &str,
    fen: &str,
) {
    let mut seen_moves: BTreeSet<WideMove> = BTreeSet::new();
    for mv in pos.legal_moves() {
        assert!(
            seen_moves.insert(mv),
            "{variant}: duplicate legal move {} at {fen}",
            mv.to_uci::<G>(),
        );
        let san = pos.san(&mv);
        assert_eq!(
            pos.parse_san(&san).ok(),
            Some(mv),
            "{variant}: SAN {san} did not round-trip at {fen}",
        );
    }
}

/// Invariants 2 and 3: `to_fen` is a fixed point (always), and — for a lossless
/// FEN dialect — the re-parse reproduces the hash, and two independent re-parses
/// of the same FEN hash equally (hash is a pure, path-independent function of the
/// FEN-observable position).
fn assert_fen_round_trip<G: Geometry, V: WideVariant<G>>(
    pos: &GenericPosition<G, V>,
    variant: &str,
    fen_lossless: bool,
) {
    let fen = pos.to_fen();
    let reparsed = GenericPosition::<G, V>::from_fen(&fen)
        .unwrap_or_else(|e| panic!("{variant}: to_fen output failed to re-parse: {fen}: {e:?}"));
    assert_eq!(
        reparsed.to_fen(),
        fen,
        "{variant}: FEN is not a fixed point",
    );
    if fen_lossless {
        assert_eq!(
            reparsed.zobrist(),
            pos.zobrist(),
            "{variant}: re-parse of {fen} changed the position hash",
        );
        let reparsed2 = GenericPosition::<G, V>::from_fen(&fen)
            .expect("second re-parse of a serialized position");
        assert_eq!(
            reparsed.zobrist(),
            reparsed2.zobrist(),
            "{variant}: hash not a deterministic function of {fen}",
        );
    }
}

/// Invariant 1: at `pos`, every legal move round-trips through make/unmake
/// byte-for-byte and the made position equals the `play` successor (hash
/// included).
fn assert_make_unmake_node<G: Geometry, V: WideVariant<G>>(
    pos: &GenericPosition<G, V>,
    variant: &str,
) {
    let fen = pos.to_fen();
    for mv in pos.legal_moves() {
        let expected = pos.play(&mv);
        let mut work = pos.clone();
        let undo = work.apply_with_undo(&mv);
        assert!(
            positions_equal(&work, &expected),
            "{variant}: apply_with_undo({}) diverged from play() at {fen}",
            mv.to_uci::<G>(),
        );
        work.undo(undo);
        assert!(
            positions_equal(&work, pos),
            "{variant}: unmake({}) did not restore {fen} byte-for-byte",
            mv.to_uci::<G>(),
        );
    }
}

/// Drives seeded random self-play from `start`, asserting every invariant at each
/// visited node. Generic over the variant so one body serves all 54.
fn drive<G: Geometry, V: WideVariant<G>>(
    start: GenericPosition<G, V>,
    variant: &str,
    fen_lossless: bool,
    seeds: &[u64],
    plies: usize,
) {
    for &seed in seeds {
        let mut state = seed;
        let mut pos = start.clone();
        for _ply in 0..plies {
            let fen = pos.to_fen();
            assert_move_list_integrity(&pos, variant, &fen);
            assert_fen_round_trip(&pos, variant, fen_lossless);
            assert_make_unmake_node(&pos, variant);

            let moves = pos.legal_moves();
            if moves.is_empty() {
                break;
            }
            let pick = (splitmix64(&mut state) as usize) % moves.len();
            pos = pos.play(&moves[pick]);
        }
    }
}

/// Emits, for one variant, a fast `#[test]` and an `#[ignore]`d deep one. The
/// concrete position alias fixes the geometry and rule type; `drive` infers them
/// from the `startpos()` value. `$lossless` is `false` only where the FEN dialect
/// cannot represent the full state (Alice's two planes).
macro_rules! variant_suite {
    ($name:ident, $alias:ty, $label:literal, $lossless:expr) => {
        mod $name {
            use super::*;

            #[test]
            fn invariants() {
                drive(
                    <$alias>::startpos(),
                    $label,
                    $lossless,
                    SEEDS_FAST,
                    PLIES_FAST,
                );
            }

            #[test]
            #[ignore = "deep invariant walk; run with --all-features -- --ignored"]
            fn invariants_deep() {
                drive(
                    <$alias>::startpos(),
                    $label,
                    $lossless,
                    SEEDS_DEEP,
                    PLIES_DEEP,
                );
            }
        }
    };
}

variant_suite!(alice, Alice, "alice", false);
variant_suite!(almost, Almost, "almost", true);
variant_suite!(amazon, Amazon, "amazon", true);
variant_suite!(asean, Asean, "asean", true);
variant_suite!(bughouse, Bughouse, "bughouse", true);
variant_suite!(cambodian, Cambodian, "cambodian", true);
variant_suite!(cannonshogi, CannonShogi, "cannonshogi", true);
variant_suite!(capablanca, Capablanca, "capablanca", true);
variant_suite!(capahouse, Capahouse, "capahouse", true);
variant_suite!(chak, Chak, "chak", true);
variant_suite!(chennis, Chennis, "chennis", true);
variant_suite!(chigorin, Chigorin, "chigorin", true);
variant_suite!(dobutsu, Dobutsu, "dobutsu", true);
variant_suite!(dragon, Dragon, "dragon", true);
variant_suite!(duck, Duck, "duck", true);
variant_suite!(embassy, Embassy, "embassy", true);
variant_suite!(empire, Empire, "empire", true);
variant_suite!(fogofwar, FogOfWar, "fogofwar", true);
variant_suite!(gorogoro, Gorogoro, "gorogoro", true);
variant_suite!(gothic, Gothic, "gothic", true);
variant_suite!(grand, Grand, "grand", true);
variant_suite!(grandhouse, Grandhouse, "grandhouse", true);
variant_suite!(hoppelpoppel, HoppelPoppel, "hoppelpoppel", true);
variant_suite!(janggi, Janggi, "janggi", true);
variant_suite!(janus, Janus, "janus", true);
variant_suite!(jieqi, Jieqi, "jieqi", true);
variant_suite!(khans, Khans, "khans", true);
variant_suite!(knightmate, Knightmate, "knightmate", true);
variant_suite!(kyotoshogi, Kyotoshogi, "kyotoshogi", true);
variant_suite!(makpong, Makpong, "makpong", true);
variant_suite!(makruk, Makruk, "makruk", true);
variant_suite!(manchu, Manchu, "manchu", true);
variant_suite!(mansindam, Mansindam, "mansindam", true);
variant_suite!(minishogi, Minishogi, "minishogi", true);
variant_suite!(minixiangqi, Minixiangqi, "minixiangqi", true);
variant_suite!(orda, Orda, "orda", true);
variant_suite!(ordamirror, Ordamirror, "ordamirror", true);
variant_suite!(placement, Placement, "placement", true);
variant_suite!(seirawan, Seirawan, "seirawan", true);
variant_suite!(shako, Shako, "shako", true);
variant_suite!(shatar, Shatar, "shatar", true);
variant_suite!(shatranj, Shatranj, "shatranj", true);
variant_suite!(shinobi, Shinobi, "shinobi", true);
variant_suite!(shoshogi, ShoShogi, "shoshogi", true);
variant_suite!(shogi, Shogi, "shogi", true);
variant_suite!(shogun, Shogun, "shogun", true);
variant_suite!(shouse, Shouse, "shouse", true);
variant_suite!(sittuyin, Sittuyin, "sittuyin", true);
variant_suite!(spartan, Spartan, "spartan", true);
variant_suite!(synochess, Synochess, "synochess", true);
variant_suite!(tori, Tori, "tori", true);
variant_suite!(xiangfu, Xiangfu, "xiangfu", true);
variant_suite!(xiangqi, Xiangqi, "xiangqi", true);

/// Plays up to `plies` uniformly random legal moves from the start position of
/// `id`, seeded by `seed`, returning the reached (possibly terminal) position.
/// Drives the runtime-dispatch [`AnyWideVariant`] surface, so one walk spans every
/// variant by id.
fn random_any(id: WideVariantId, seed: u64, plies: u32) -> AnyWideVariant {
    let mut state = seed;
    let mut pos = AnyWideVariant::startpos(id);
    for _ in 0..plies {
        let moves = pos.legal_moves();
        if moves.is_empty() {
            break;
        }
        let pick = (splitmix64(&mut state) as usize) % moves.len();
        pos = pos.play(&moves[pick]);
    }
    pos
}

/// A `(WideVariantId, seed, plies)` strategy: the inputs to a seeded random walk
/// across every registered variant. `plies` is short so shrinking converges on a
/// small reproducing line.
fn walk_inputs() -> impl Strategy<Value = (WideVariantId, u64, u32)> {
    (
        proptest::sample::select(WideVariantId::ALL),
        any::<u64>(),
        0u32..40,
    )
}

proptest! {
    // Modest case count: each case walks many nodes, so this stays fast in CI
    // while exercising the invariants broadly across all 54 variants.
    #![proptest_config(ProptestConfig::with_cases(192))]

    /// FEN fixed point and path-independent hash across every variant, via the
    /// runtime [`AnyWideVariant`] surface. `to_fen` re-parses to the same string,
    /// and the hash is a deterministic function of that FEN (two independent
    /// re-parses agree) — holding for Alice too, since both re-parses land every
    /// piece on plane A identically.
    #[test]
    fn any_fen_and_hash((id, seed, plies) in walk_inputs()) {
        let pos = random_any(id, seed, plies);
        let fen = pos.to_fen();
        let a = AnyWideVariant::from_fen(id, &fen)
            .expect("a serialized legal position must re-parse");
        prop_assert_eq!(a.to_fen(), fen.clone(), "fen fixed point for {}", id);
        let b = AnyWideVariant::from_fen(id, &fen)
            .expect("a serialized legal position must re-parse");
        prop_assert_eq!(a.position_key(), b.position_key(), "hash determinism for {}", fen);
    }

    /// Legal-move-list integrity across every variant: no duplicate move, and
    /// every legal move round-trips through SAN to exactly itself (so no two moves
    /// collide on a rendering). SAN is the human move language checked here; the
    /// parallel UCI injectivity (Kyoto's two-form drops included, since #452) is
    /// covered by `notation_roundtrip.rs` and the `wide_uci_round_trip` property.
    #[test]
    fn any_move_list_integrity((id, seed, plies) in walk_inputs()) {
        let pos = random_any(id, seed, plies);
        let moves = pos.legal_moves();
        let mut seen: BTreeSet<WideMove> = BTreeSet::new();
        for mv in &moves {
            prop_assert!(seen.insert(*mv), "duplicate legal move in {} at {}", id, pos.to_fen());
            let san = pos.san(mv);
            let parsed = pos.parse_san(&san);
            prop_assert_eq!(parsed.as_ref(), Some(mv), "san round trip {} in {}", san, pos.to_fen());
        }
    }
}
