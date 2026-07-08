//! Coverage gate (issue #497): a meta-test asserting that **every** game mcr
//! ships carries **every** required correctness check.
//!
//! mcr's 80-odd `tests/perft_*.rs` files and its draw-rule tests are
//! hand-maintained; before this gate nothing asserted that a *new* variant
//! actually came with a perft test, a differential-fuzz entry, and (if it has a
//! special terminal rule) an adjudication test. A variant could ship with the two
//! ungated correctness checks — perft and draw-rule — simply absent.
//!
//! This file closes that hole with **one in-repo manifest** — [`REQUIRED`], a row
//! per game covering exactly [`WideVariantId::ALL`] + [`VariantId::ALL`] + Ataxx —
//! and a set of tests that make a missing or stale row fail `cargo test`.
//!
//! ## What each check enforces (and its honest limits)
//!
//! 1. **Manifest completeness** ([`manifest_lists_exactly_all_games`]) —
//!    *introspectable, hard*: the manifest's game set is compared to the live
//!    registries, so a variant added to `WideVariantId::ALL` / `VariantId::ALL`
//!    without a manifest row fails the test, and a stale row for a removed variant
//!    cannot compile (a `Game::Wide` holds a real `WideVariantId`). This alone
//!    stops a new variant from being invisible.
//!
//! 2. **Perft-test presence** ([`every_variant_has_a_perft_file`]) — the manifest
//!    names each game's `tests/perft_*.rs` file and the test asserts the file
//!    exists on disk and contains real `perft(` assertions. **Honest limit:** the
//!    `min_non_ignored_depth` column is *manifest-declared*, not
//!    runtime-introspected — Rust integration tests cannot enumerate another test
//!    binary's `#[ignore]` attributes or the depths it asserts, so "this variant's
//!    perft asserts to depth N" is a documented claim in the manifest, verified by
//!    a human at review, not by this gate. The gate verifies the *file exists and
//!    is non-empty of perft calls*; the depth is a declaration.
//!
//! 3. **difffuzz membership** ([`difffuzz_membership_is_explicit_and_named`]) —
//!    *introspectable*: every wide variant is either a `Spec` in
//!    `compare-fairy/src/difffuzz.rs` **xor** a NAMED, documented exclusion in the
//!    manifest, checked by reading the difffuzz source and asserting the
//!    per-variant `Spec` is present iff the manifest says `InSpecs`. This extends
//!    the `SPECS.len() == ALL.len() - 7` count invariant (asserted in difffuzz.rs
//!    itself) into an explicit per-variant partition, and — via
//!    [`DEEP_SWEEP_HELD_BACK`] — makes the CI deep-sweep hold-back set explicit and
//!    named (the audit found `difffuzz.rs::HELD_BACK` lists only Janggi while CI
//!    holds back four).
//!
//! 4. **Property / notation drivers still iterate `ALL`**
//!    ([`property_and_notation_drivers_still_iterate_all`]) — a guard reading the
//!    driver sources so a refactor cannot silently narrow them below the full
//!    registry.
//!
//! 5. **Draw-hook → test mapping** ([`draw_hook_overrides_have_a_registered_test`])
//!    — *introspectable, hard*: for each wide variant the gate **calls the concrete
//!    rules' draw hooks** ([`WideVariantId::draw_hooks`]) and, if the variant
//!    overrides any, requires the manifest's `draw_test` column to register an
//!    adjudication test — or an explicit [`DrawTest::Todo498`] exception for a
//!    currently-missing test (issue #498 removes them). The exception count is
//!    pinned by [`todo498_debt_is_pinned`], so the debt is visible now and the gate
//!    turns hard as each test lands.
//!
//! Ataxx lives outside both `WideVariantId` and `VariantId`, so before this gate it
//! was invisible to every ALL-driven check; the [`Game`] registry enum folds it in
//! as a first-class row.

use std::collections::BTreeSet;
use std::path::Path;

use mcr::geometry::WideVariantId;
use mcr::VariantId;

/// The perft oracle a variant's `tests/perft_*.rs` file is pinned against.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PerftOracle {
    /// Cross-checked node-for-node against Fairy-Stockfish.
    Fsf,
    /// Cross-checked against the HaChu large-shogi reference engine.
    HaChu,
    /// Hand-derived low-depth counts (no external engine implements the variant).
    HandDerived,
    /// Cross-checked node-for-node against a **second, fully independent in-repo
    /// generator** — two from-scratch implementations agreeing at the declared depth,
    /// the substitute for a missing engine oracle (issue #500). Used by the
    /// oracle-less variants whose perft would otherwise be self-referential: Alice
    /// and Wa Shogi (independent brute-force in their `tests/perft_*.rs`) and Tenjiku
    /// (independent 16x16 generator cross-checking the otherwise self-referential
    /// perft(2)/(3), HaChu segfaulting on the variant).
    HandDerivedX2,
    /// Pinned against the shared EPD perft corpus. Part of the declared oracle
    /// vocabulary; no shipped variant selects it as its primary oracle today.
    #[allow(dead_code)]
    Corpus,
    /// No perft oracle (reserved; a shipped variant may never carry it — enforced
    /// by [`every_variant_has_a_perft_file`]).
    None,
}

/// A wide variant's place in the differential fuzzer (`compare-fairy`'s `SPECS`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Difffuzz {
    /// Present in `SPECS` — cross-checked against FSF in the default sweep.
    InSpecs,
    /// A named, documented by-design exclusion from `SPECS` (the reason string is
    /// non-empty and mirrors the `SPECS` module doc-comment).
    Excluded(&'static str),
    /// Not part of the wide-layer difffuzz domain at all — the concrete 8x8 engine
    /// and Ataxx are validated by their own harnesses, not the `WideVariantId`
    /// `SPECS` sweep.
    NotWideLayer,
}

/// The adjudication (draw-rule) test registered for a variant.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DrawTest {
    /// The variant overrides no draw / adjudication hook, so none is required.
    /// Must agree with [`WideVariantId::draw_hooks`] introspection.
    None,
    /// A registered adjudication test that covers this variant's overridden
    /// hook(s). The string is the test's function name (variant-specific, or the
    /// variant-agnostic `move_rule_draw_when_enabled` mechanism test for the
    /// move-count rule).
    Named(&'static str),
    /// The variant overrides a draw hook but no adjudication test exists yet —
    /// tracked debt removed by issue #498. The string names the untested hook(s).
    ///
    /// Issue #498 cleared the last of these, so the manifest constructs none today
    /// (the pin [`EXPECTED_TODO498`] is `0`). The variant and its gate arm are kept
    /// so a *future* variant that ships a draw hook ahead of its adjudication test
    /// can be pinned as visible debt again rather than silently regressing the
    /// `Named`/`None` binding.
    #[allow(dead_code)]
    Todo498(&'static str),
}

/// The three registries mcr ships, unified so the gate can iterate one list.
/// Folding Ataxx in here is what stops it being invisible to the ALL-driven
/// checks.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Game {
    /// A geometry-layer fairy variant ([`WideVariantId`]).
    Wide(WideVariantId),
    /// A concrete 8x8-engine variant ([`VariantId`]).
    Concrete(VariantId),
    /// Ataxx — outside both registries (its own `Position` in `mcr::ataxx`).
    Ataxx,
}

impl Game {
    /// A stable human label for failure messages.
    fn label(self) -> String {
        match self {
            Game::Wide(w) => format!("wide:{}", w.as_str()),
            Game::Concrete(c) => format!("concrete:{}", c.as_str()),
            Game::Ataxx => "ataxx".to_string(),
        }
    }
}

/// One manifest row: a game and the correctness checks it is required to carry.
struct Required {
    /// Which game this row is for.
    game: Game,
    /// The `tests/perft_*.rs` file (basename) that pins this game's perft.
    perft_file: &'static str,
    /// The oracle that perft file is validated against.
    oracle: PerftOracle,
    /// The minimum non-`#[ignore]` perft depth the file asserts. **Declared, not
    /// introspected** (see the module docs): a human-maintained claim, not verified
    /// by this gate.
    min_non_ignored_depth: u32,
    /// The game's difffuzz status.
    difffuzz: Difffuzz,
    /// The registered adjudication test (or the introspected absence of a hook).
    draw_test: DrawTest,
}

/// The single source of truth: exactly `WideVariantId::ALL` + `VariantId::ALL` +
/// Ataxx, each declaring its perft oracle + depth, difffuzz status, and draw-test.
/// Rows are in registry-declaration order (wide, then concrete, then Ataxx).
const REQUIRED: &[Required] = &[
    // ---- Geometry-layer fairy variants (`WideVariantId::ALL`) ----------------
    row(Game::Wide(WideVariantId::Aiwok), "perft_aiwok.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("aiwok_pieces_honour_count_matches_makruk")),
    row(Game::Wide(WideVariantId::Alice), "perft_alice.rs", PerftOracle::HandDerivedX2, 4, Difffuzz::Excluded("FSF has no alice variant (two-board teleport ruleset)"), DrawTest::None),
    row(Game::Wide(WideVariantId::Almost), "perft_almost.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Amazon), "perft_amazon.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Asean), "perft_asean.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("asean_pieces_honour_count_matches_fsf")),
    row(Game::Wide(WideVariantId::Berolina), "perft_berolina.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Bughouse), "perft_bughouse.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::None),
    row(Game::Wide(WideVariantId::Cambodian), "perft_cambodian.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("cambodian_pieces_honour_count_matches_fsf")),
    row(Game::Wide(WideVariantId::CannonShogi), "perft_cannonshogi.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::Named("cannonshogi_perpetual_check_loses_for_the_checker")),
    row(Game::Wide(WideVariantId::Capablanca), "perft_capablanca.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::Named("capablanca_fifty_move_rule_draws_at_the_game_level")),
    row(Game::Wide(WideVariantId::Capahouse), "perft_capahouse.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::None),
    row(Game::Wide(WideVariantId::Caparandom), "perft_caparandom.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Centaur), "perft_centaur.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Chak), "perft_chak.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::Named("stalemate_is_a_loss")),
    row(Game::Wide(WideVariantId::Chancellor), "perft_chancellor.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Chaturanga), "perft_chaturanga.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("stalemate_is_a_loss")),
    row(Game::Wide(WideVariantId::CheckShogi), "perft_checkshogi.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::Named("check_win_is_terminal")),
    row(Game::Wide(WideVariantId::Chennis), "perft_chennis.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("stalemate_is_a_loss")),
    row(Game::Wide(WideVariantId::Chigorin), "perft_chigorin.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Chu), "perft_chu.rs", PerftOracle::HaChu, 2, Difffuzz::Excluded("HaChu-only large shogi; FSF has no chu variant"), DrawTest::Named("chu_attack_repetition_loses_for_the_attacker")),
    row(Game::Wide(WideVariantId::Codrus), "perft_codrus.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("losing_the_king_wins")),
    row(Game::Wide(WideVariantId::Coregal), "perft_coregal.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::None),
    row(Game::Wide(WideVariantId::Courier), "perft_courier.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::Named("stalemate_is_a_loss")),
    row(Game::Wide(WideVariantId::Dai), "perft_dai.rs", PerftOracle::HaChu, 3, Difffuzz::Excluded("HaChu-only large shogi; FSF has no dai variant"), DrawTest::Named("dai_attack_repetition_loses_for_the_attacker")),
    row(Game::Wide(WideVariantId::Dobutsu), "perft_dobutsu.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::None),
    row(Game::Wide(WideVariantId::Dragon), "perft_dragon.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::None),
    row(Game::Wide(WideVariantId::Duck), "perft_duck.rs", PerftOracle::Fsf, 4, Difffuzz::Excluded("FSF counts duck placements differently (#189 harness artifact)"), DrawTest::None),
    row(Game::Wide(WideVariantId::Embassy), "perft_embassy.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Empire), "perft_empire.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("stalemate_is_a_loss")),
    row(Game::Wide(WideVariantId::EuroShogi), "perft_euroshogi.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::Named("euroshogi_sennichite_is_a_draw")),
    row(Game::Wide(WideVariantId::Extinction), "perft_extinction.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("extinction_last_of_a_type_loses")),
    row(Game::Wide(WideVariantId::FogOfWar), "perft_fogofwar.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::None),
    row(Game::Wide(WideVariantId::Gardner), "perft_gardner.rs", PerftOracle::Fsf, 5, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Georgian), "perft_georgian.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Giveaway), "perft_giveaway.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("zero_pieces_is_a_win_for_that_side")),
    row(Game::Wide(WideVariantId::Gorogoro), "perft_gorogoro.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("gorogoro_sennichite_is_a_draw")),
    row(Game::Wide(WideVariantId::Gothic), "perft_gothic.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Grand), "perft_grand.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Grandhouse), "perft_grandhouse.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Grasshopper), "perft_grasshopper.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Gustav3), "perft_gustav3.rs", PerftOracle::HandDerivedX2, 3, Difffuzz::Excluded("the available FSF binary is a non-large-board build lacking gustav3 (10x8 with wall squares); an independent from-scratch 10x8 generator is the second source"), DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::HoppelPoppel), "perft_hoppelpoppel.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::None),
    row(Game::Wide(WideVariantId::Janggi), "perft_janggi.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::Named("janggi_bikjang_facing_generals_draw")),
    row(Game::Wide(WideVariantId::Janus), "perft_janus.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Jieqi), "perft_jieqi.rs", PerftOracle::HandDerived, 3, Difffuzz::Excluded("hidden-info Xiangqi; needs a per-position identity reveal, not a static dialect rewrite"), DrawTest::None),
    row(Game::Wide(WideVariantId::Judkins), "perft_judkins.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("judkins_sennichite_is_a_draw")),
    row(Game::Wide(WideVariantId::Karouk), "perft_karouk.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("giving_check_wins")),
    row(Game::Wide(WideVariantId::Khans), "perft_khans.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("stalemate_is_a_loss")),
    row(Game::Wide(WideVariantId::Kinglet), "perft_kinglet.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("kinglet_last_pawn_loss")),
    row(Game::Wide(WideVariantId::Knightmate), "perft_knightmate.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Kyotoshogi), "perft_kyotoshogi.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("kyotoshogi_sennichite_is_a_draw")),
    row(Game::Wide(WideVariantId::Legan), "perft_legan.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Losalamos), "perft_losalamos.rs", PerftOracle::Fsf, 5, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Losers), "perft_losers.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("bare_king_wins_for_the_bared_side")),
    row(Game::Wide(WideVariantId::Makpong), "perft_makpong.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("makpong_pieces_honour_count_matches_makruk")),
    row(Game::Wide(WideVariantId::Makruk), "perft_makruk.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("makruk_pieces_honour_count_matches_fsf")),
    row(Game::Wide(WideVariantId::Manchu), "perft_manchu.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::Named("stalemate_is_a_loss")),
    row(Game::Wide(WideVariantId::Mansindam), "perft_mansindam.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::None),
    row(Game::Wide(WideVariantId::Micro), "perft_micro.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("micro_sennichite_is_a_draw")),
    row(Game::Wide(WideVariantId::Minishogi), "perft_minishogi.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("minishogi_sennichite_is_a_draw")),
    row(Game::Wide(WideVariantId::Minixiangqi), "perft_minixiangqi.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("minixiangqi_threefold_repetition_is_a_draw")),
    row(Game::Wide(WideVariantId::Misere), "perft_misere.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("checkmated_side_wins")),
    row(Game::Wide(WideVariantId::Modern), "perft_modern.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Newzealand), "perft_newzealand.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Nightrider), "perft_nightrider.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Nocastle), "perft_nocastle.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::None),
    row(Game::Wide(WideVariantId::OkisakiShogi), "perft_okisakishogi.rs", PerftOracle::HandDerivedX2, 3, Difffuzz::Excluded("the available FSF binary is a non-large-board build lacking okisakishogi (10x10); an independent from-scratch 10x10 generator is the second source"), DrawTest::Named("okisakishogi_sennichite_is_a_draw")),
    row(Game::Wide(WideVariantId::Omicron), "perft_omicron.rs", PerftOracle::HandDerivedX2, 3, Difffuzz::Excluded("the available FSF binary is a non-large-board build lacking omicron (12x10 with wall squares); an independent from-scratch 12x10 generator is the second source"), DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Opulent), "perft_opulent.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Orda), "perft_orda.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::None),
    row(Game::Wide(WideVariantId::Ordamirror), "perft_ordamirror.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::None),
    row(Game::Wide(WideVariantId::Paradigm), "perft_paradigm.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Pawnback), "perft_pawnback.rs", PerftOracle::Fsf, 5, Difffuzz::InSpecs, DrawTest::Named("pawnback_pawn_shuffle_reaches_move_rule_draw")),
    row(Game::Wide(WideVariantId::Pawnsideways), "perft_pawnsideways.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Perfect), "perft_perfect.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Petrified), "perft_petrified.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Placement), "perft_placement.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::None),
    row(Game::Wide(WideVariantId::Pocketknight), "perft_pocketknight.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::None),
    row(Game::Wide(WideVariantId::Seirawan), "perft_seirawan.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Shako), "perft_shako.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Shatar), "perft_shatar.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("bare_king_is_a_robado_draw")),
    row(Game::Wide(WideVariantId::Shatranj), "perft_shatranj.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("bared_king_loses")),
    row(Game::Wide(WideVariantId::Shinobi), "perft_shinobi.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::None),
    row(Game::Wide(WideVariantId::Shogi), "perft_shogi.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::Named("shogi_sennichite_is_a_draw")),
    row(Game::Wide(WideVariantId::Shogun), "perft_shogun.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("shogun_sennichite_is_a_draw")),
    row(Game::Wide(WideVariantId::ShoShogi), "perft_shoshogi.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::Named("shoshogi_sennichite_is_a_draw")),
    row(Game::Wide(WideVariantId::Shouse), "perft_shouse.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::None),
    row(Game::Wide(WideVariantId::Sittuyin), "perft_sittuyin.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("sittuyin_pieces_honour_count_matches_asean_base")),
    row(Game::Wide(WideVariantId::Sortofalmost), "perft_sortofalmost.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Spartan), "perft_spartan.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::None),
    row(Game::Wide(WideVariantId::Suicide), "perft_suicide.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("stalemate_with_fewer_pieces_wins")),
    row(Game::Wide(WideVariantId::Supply), "perft_supply.rs", PerftOracle::HandDerivedX2, 3, Difffuzz::Excluded("the available FSF binary is a non-large-board build lacking supply (9x10 Xiangqi-with-drops) and even xiangqi; an independent from-scratch 9x10 generator is the second source (and empty-hand play equals FSF-pinned Xiangqi)"), DrawTest::Named("supply_stalemate_is_a_loss")),
    row(Game::Wide(WideVariantId::Synochess), "perft_synochess.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("stalemate_is_a_loss")),
    row(Game::Wide(WideVariantId::Tencubed), "perft_tencubed.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Tenjiku), "perft_tenjiku.rs", PerftOracle::HandDerivedX2, 3, Difffuzz::Excluded("HaChu-only large shogi; HaChu crashes on 16x16 and FSF has no tenjiku"), DrawTest::Named("tenjiku_one_sided_attack_repetition_draws")),
    row(Game::Wide(WideVariantId::Threekings), "perft_threekings.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("threekings_losing_a_king_loses")),
    row(Game::Wide(WideVariantId::Tori), "perft_torishogi.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::Named("tori_sennichite_is_a_draw")),
    row(Game::Wide(WideVariantId::Torpedo), "perft_torpedo.rs", PerftOracle::Fsf, 4, Difffuzz::InSpecs, DrawTest::None),
    row(Game::Wide(WideVariantId::Washogi), "perft_washogi.rs", PerftOracle::HandDerivedX2, 3, Difffuzz::Excluded("no FSF wa-shogi; HaChu's wa-shogi is a different ruleset (51 vs 57 start moves) — independent brute force is the second source"), DrawTest::Named("washogi_sennichite_is_a_draw")),
    row(Game::Wide(WideVariantId::Wolf), "perft_wolf.rs", PerftOracle::HandDerivedX2, 3, Difffuzz::Excluded("the available FSF binary is a non-large-board build lacking the 10-rank wolf (8x10 compound + rider army); an independent from-scratch 8x10 generator is the second source"), DrawTest::Named("move_rule_draw_when_enabled")),
    row(Game::Wide(WideVariantId::Xiangfu), "perft_xiangfu.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::None),
    row(Game::Wide(WideVariantId::Xiangqi), "perft_xiangqi.rs", PerftOracle::Fsf, 3, Difffuzz::InSpecs, DrawTest::Named("xiangqi_perpetual_chase_loses_for_the_chaser")),
    row(Game::Wide(WideVariantId::Yari), "perft_yarishogi.rs", PerftOracle::HandDerivedX2, 3, Difffuzz::Excluded("no FSF yarishogi in the built binary (9-rank board needs large boards, off); independent brute force is the second source"), DrawTest::Named("yari_sennichite_is_a_draw")),
    // ---- Concrete 8x8 engine variants (`VariantId::ALL`) ---------------------
    // The concrete engine has its own terminal-rule suite (not the `WideVariant`
    // draw hooks), so these rows are not draw-hook-introspected: `draw_test` is
    // `None` and `difffuzz` is `NotWideLayer`. They are still folded in so the
    // perft-presence and manifest-completeness checks cover them.
    row(Game::Concrete(VariantId::Standard), "perft.rs", PerftOracle::HandDerived, 5, Difffuzz::NotWideLayer, DrawTest::None),
    row(Game::Concrete(VariantId::Chess960), "perft_chess960.rs", PerftOracle::HandDerived, 4, Difffuzz::NotWideLayer, DrawTest::None),
    row(Game::Concrete(VariantId::Atomic), "perft_atomic.rs", PerftOracle::Fsf, 4, Difffuzz::NotWideLayer, DrawTest::None),
    row(Game::Concrete(VariantId::Antichess), "perft_antichess.rs", PerftOracle::Fsf, 4, Difffuzz::NotWideLayer, DrawTest::None),
    row(Game::Concrete(VariantId::Crazyhouse), "perft_crazyhouse.rs", PerftOracle::Fsf, 4, Difffuzz::NotWideLayer, DrawTest::None),
    row(Game::Concrete(VariantId::KingOfTheHill), "perft_koth.rs", PerftOracle::Fsf, 4, Difffuzz::NotWideLayer, DrawTest::None),
    row(Game::Concrete(VariantId::ThreeCheck), "perft_three_check.rs", PerftOracle::Fsf, 4, Difffuzz::NotWideLayer, DrawTest::None),
    row(Game::Concrete(VariantId::RacingKings), "perft_racing.rs", PerftOracle::Fsf, 4, Difffuzz::NotWideLayer, DrawTest::None),
    row(Game::Concrete(VariantId::Horde), "perft_horde.rs", PerftOracle::Fsf, 4, Difffuzz::NotWideLayer, DrawTest::None),
    // ---- Ataxx (outside both registries) -------------------------------------
    row(Game::Ataxx, "perft_ataxx.rs", PerftOracle::Fsf, 4, Difffuzz::NotWideLayer, DrawTest::None),
];

/// The number of [`DrawTest::Todo498`] exceptions the manifest currently carries —
/// the enforced-once-filled draw-rule debt that issue #498 removes. Pinned so the
/// debt cannot silently grow (a new variant with an untested draw hook must either
/// add a test or bump this count deliberately).
const EXPECTED_TODO498: usize = 0;

/// The by-design difffuzz exclusion count (Alice, Duck, Jieqi + the HaChu-only
/// large shogi Chu / Dai / Tenjiku + the oracle-less Wa Shogi, Okisaki Shogi, Yari
/// Shogi, Gustav 3, Omicron, Supply, and Wolf) — the `13` in the `SPECS.len() == ALL.len() - 13`
/// invariant that `compare-fairy/src/difffuzz.rs` asserts on its own side.
const DIFFFUZZ_EXCLUSIONS: usize = 13;

/// The variants CI holds back from the **deep rotating** difffuzz sweep (12 games ×
/// 90 plies), each hitting a documented FSF *oracle* limitation whose false
/// divergences scale with depth — see `.github/workflows/ci.yml` and
/// `difffuzz.rs::HELD_BACK`. They are still in `SPECS` (fuzzed in the default sweep
/// and at their proven-clean depth), so they carry `Difffuzz::InSpecs` above; this
/// named list makes the deep-sweep hold-back explicit in-repo, reconciling the
/// audit finding that `HELD_BACK` names only Janggi while CI holds back four.
const DEEP_SWEEP_HELD_BACK: &[WideVariantId] = &[
    WideVariantId::Janggi,
    WideVariantId::Sittuyin,
    WideVariantId::Synochess,
    WideVariantId::Empire,
];

/// A `const`-friendly row constructor.
const fn row(
    game: Game,
    perft_file: &'static str,
    oracle: PerftOracle,
    min_non_ignored_depth: u32,
    difffuzz: Difffuzz,
    draw_test: DrawTest,
) -> Required {
    Required {
        game,
        perft_file,
        oracle,
        min_non_ignored_depth,
        difffuzz,
        draw_test,
    }
}

/// The sources the gate introspects, embedded at compile time (so a moved or
/// deleted file fails the build, not just the test).
const DIFFFUZZ_SRC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/compare-fairy/src/difffuzz.rs"
));
const PROPERTIES_SRC: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/properties.rs"));
const NOTATION_SRC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/notation_roundtrip.rs"
));
const CI_SRC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/.github/workflows/ci.yml"
));

/// Finds the manifest row for a wide variant (every one exists once the
/// completeness test passes).
fn wide_row(id: WideVariantId) -> &'static Required {
    REQUIRED
        .iter()
        .find(|r| r.game == Game::Wide(id))
        .unwrap_or_else(|| panic!("no manifest row for wide variant {}", id.as_str()))
}

/// **Check 1 — manifest completeness.** The manifest lists exactly
/// `WideVariantId::ALL` + `VariantId::ALL` + one Ataxx row, with no gaps, extras,
/// or duplicates. A new variant added to a registry without a manifest row fails
/// here.
#[test]
fn manifest_lists_exactly_all_games() {
    let declared_wide: BTreeSet<&str> = REQUIRED
        .iter()
        .filter_map(|r| match r.game {
            Game::Wide(w) => Some(w.as_str()),
            _ => None,
        })
        .collect();
    let all_wide: BTreeSet<&str> = WideVariantId::ALL.iter().map(|w| w.as_str()).collect();
    assert_eq!(
        declared_wide, all_wide,
        "manifest wide rows must equal WideVariantId::ALL exactly (a new/removed fairy variant needs a manifest row)"
    );
    // No duplicate wide rows (the set above would hide a dup; count catches it).
    let wide_rows = REQUIRED
        .iter()
        .filter(|r| matches!(r.game, Game::Wide(_)))
        .count();
    assert_eq!(
        wide_rows,
        WideVariantId::ALL.len(),
        "duplicate or missing wide manifest row"
    );

    let declared_concrete: BTreeSet<&str> = REQUIRED
        .iter()
        .filter_map(|r| match r.game {
            Game::Concrete(c) => Some(c.as_str()),
            _ => None,
        })
        .collect();
    let all_concrete: BTreeSet<&str> = VariantId::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(
        declared_concrete, all_concrete,
        "manifest concrete rows must equal VariantId::ALL exactly"
    );
    let concrete_rows = REQUIRED
        .iter()
        .filter(|r| matches!(r.game, Game::Concrete(_)))
        .count();
    assert_eq!(
        concrete_rows,
        VariantId::ALL.len(),
        "duplicate or missing concrete manifest row"
    );

    let ataxx_rows = REQUIRED
        .iter()
        .filter(|r| matches!(r.game, Game::Ataxx))
        .count();
    assert_eq!(ataxx_rows, 1, "Ataxx must be folded in as exactly one row");

    assert_eq!(
        REQUIRED.len(),
        WideVariantId::ALL.len() + VariantId::ALL.len() + 1,
        "manifest length must be WideVariantId::ALL + VariantId::ALL + Ataxx"
    );
}

/// **Check 2 — perft-test presence.** Every game's declared `tests/perft_*.rs`
/// file exists and contains real `perft(` assertions. (The asserted depth is
/// manifest-declared, not introspected — see the module docs.)
#[test]
fn every_variant_has_a_perft_file() {
    let tests_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    for r in REQUIRED {
        let path = tests_dir.join(r.perft_file);
        assert!(
            path.is_file(),
            "{}: declared perft file tests/{} does not exist",
            r.game.label(),
            r.perft_file
        );
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("reading tests/{}: {e}", r.perft_file));
        // A real perft test drives perft (directly, via the `gperft` alias, or a
        // local `check`/`perft` helper) and asserts the node counts.
        assert!(
            src.contains("perft") && src.contains("assert"),
            "{}: tests/{} does not look like a perft test (no perft driver + assertion)",
            r.game.label(),
            r.perft_file
        );
        // The declared floor must be a real depth (documents the claim; not
        // runtime-verified against the file's `#[ignore]` set).
        assert!(
            r.min_non_ignored_depth >= 1,
            "{}: declared min_non_ignored_depth must be >= 1",
            r.game.label()
        );
        // No oracle-less rows ship today.
        assert_ne!(
            r.oracle,
            PerftOracle::None,
            "{}: a shipped variant must declare a real perft oracle",
            r.game.label()
        );
    }
}

/// **Check 3 — difffuzz membership is explicit and named.** Every wide variant is
/// a `Spec` in `compare-fairy/src/difffuzz.rs` **xor** a named documented
/// exclusion; concrete / Ataxx rows are `NotWideLayer`. This extends the
/// `SPECS.len() == ALL.len() - 7` count invariant into a per-variant partition.
#[test]
fn difffuzz_membership_is_explicit_and_named() {
    let mut in_specs = 0usize;
    let mut excluded = 0usize;
    for &id in WideVariantId::ALL {
        let r = wide_row(id);
        // A `Spec { id: WideVariantId::X, ... }` is present in the source iff the
        // variant is fuzzed. The trailing comma disambiguates prefixes
        // (`Xiangqi` vs `Xiangfu`, `Shatar` vs `Shatranj`).
        let token = format!("id: WideVariantId::{id:?},");
        let present = DIFFFUZZ_SRC.contains(&token);
        match r.difffuzz {
            Difffuzz::InSpecs => {
                assert!(
                    present,
                    "{} is declared InSpecs but has no Spec in compare-fairy/src/difffuzz.rs",
                    id.as_str()
                );
                in_specs += 1;
            }
            Difffuzz::Excluded(reason) => {
                assert!(
                    !present,
                    "{} is declared Excluded but a Spec exists in difffuzz.rs",
                    id.as_str()
                );
                assert!(
                    !reason.is_empty(),
                    "{} exclusion must carry a documented reason",
                    id.as_str()
                );
                excluded += 1;
            }
            Difffuzz::NotWideLayer => panic!(
                "{} is a wide variant; its difffuzz status must be InSpecs or Excluded, not NotWideLayer",
                id.as_str()
            ),
        }
    }
    // Concrete / Ataxx rows are outside the wide difffuzz domain.
    for r in REQUIRED {
        if !matches!(r.game, Game::Wide(_)) {
            assert_eq!(
                r.difffuzz,
                Difffuzz::NotWideLayer,
                "{}: non-wide games are not part of the WideVariantId SPECS sweep",
                r.game.label()
            );
        }
    }
    assert_eq!(
        excluded, DIFFFUZZ_EXCLUSIONS,
        "the by-design difffuzz exclusion set must be exactly {DIFFFUZZ_EXCLUSIONS} named variants"
    );
    assert_eq!(
        in_specs,
        WideVariantId::ALL.len() - DIFFFUZZ_EXCLUSIONS,
        "SPECS membership must equal ALL - {DIFFFUZZ_EXCLUSIONS} (mirrors difffuzz.rs's own SPECS.len() invariant)"
    );
}

/// **Check 3b — the CI deep-sweep hold-back set is explicit and named.** Each
/// held-back variant is still a real fuzzable `Spec` (so it is not silently
/// dropped from coverage), and CI's held-back loop still names exactly these four
/// — reconciling the audit's `HELD_BACK`-lists-one-but-CI-holds-four finding.
#[test]
fn deep_sweep_held_back_is_explicit_and_matches_ci() {
    let mut seen = BTreeSet::new();
    for &id in DEEP_SWEEP_HELD_BACK {
        assert!(
            seen.insert(id.as_str()),
            "duplicate deep-sweep hold-back {}",
            id.as_str()
        );
        assert_eq!(
            wide_row(id).difffuzz,
            Difffuzz::InSpecs,
            "{} is held back from the deep sweep but must still be a Spec (covered in the default sweep)",
            id.as_str()
        );
    }
    assert!(
        CI_SRC.contains("for variant in janggi sittuyin synochess empire"),
        "ci.yml's held-back loop changed; update DEEP_SWEEP_HELD_BACK to match the four CI holds back"
    );
    for &id in DEEP_SWEEP_HELD_BACK {
        assert!(
            CI_SRC.contains(id.as_str()),
            "ci.yml no longer names deep-sweep hold-back {}",
            id.as_str()
        );
    }
}

/// **Check 4 — the property / notation drivers still iterate `ALL`.** A refactor
/// that narrowed either driver below the full registry (dropping a variant's
/// property / notation coverage) fails here.
#[test]
fn property_and_notation_drivers_still_iterate_all() {
    assert!(
        NOTATION_SRC.contains("WideVariantId::ALL"),
        "tests/notation_roundtrip.rs no longer iterates WideVariantId::ALL"
    );
    assert!(
        PROPERTIES_SRC.contains("WideVariantId::ALL"),
        "tests/properties.rs no longer iterates WideVariantId::ALL"
    );
    // The concrete driver enumerates a fixed `ALL_IDS` array; guard both its
    // length and that it still names every concrete variant, so it cannot be
    // narrowed silently.
    assert!(
        PROPERTIES_SRC.contains(&format!("[VariantId; {}]", VariantId::ALL.len())),
        "tests/properties.rs ALL_IDS length drifted from VariantId::ALL ({} ids)",
        VariantId::ALL.len()
    );
    for &id in VariantId::ALL {
        assert!(
            PROPERTIES_SRC.contains(&format!("VariantId::{id:?}")),
            "tests/properties.rs ALL_IDS no longer covers concrete variant {}",
            id.as_str()
        );
    }
}

/// **Check 5 — draw-hook overrides carry a registered adjudication test.** For
/// each wide variant the gate *introspects* the concrete rules' draw hooks and
/// binds the result to the manifest, both ways: a variant that overrides a hook
/// must register a `Named` test or an explicit `Todo498` exception, and a variant
/// that overrides nothing must declare `None` (so a stale row cannot claim a test
/// for a rule that no longer exists).
#[test]
fn draw_hook_overrides_have_a_registered_test() {
    for &id in WideVariantId::ALL {
        let r = wide_row(id);
        let hooks = id.draw_hooks();
        match r.draw_test {
            DrawTest::None => assert!(
                !hooks.any(),
                "{} overrides draw hooks {:?} but the manifest declares DrawTest::None — \
register an adjudication test or a TODO(#498) exception",
                id.as_str(),
                hooks.names()
            ),
            DrawTest::Named(name) => {
                assert!(
                    hooks.any(),
                    "{} registers draw test {name:?} but overrides no draw hook — stale manifest row",
                    id.as_str()
                );
                assert!(!name.is_empty(), "{} draw test name is empty", id.as_str());
            }
            DrawTest::Todo498(hook) => {
                assert!(
                    hooks.any(),
                    "{} carries a TODO(#498) but overrides no draw hook — stale exception",
                    id.as_str()
                );
                assert!(
                    !hook.is_empty(),
                    "{} TODO(#498) must name the untested hook(s)",
                    id.as_str()
                );
            }
        }
    }
}

/// **Check 5b — the #498 draw-test debt is pinned.** The number of `Todo498`
/// exceptions is fixed, so the debt is visible and cannot silently grow; each test
/// #498 lands lets this drop by one.
#[test]
fn todo498_debt_is_pinned() {
    let todo = REQUIRED
        .iter()
        .filter(|r| matches!(r.draw_test, DrawTest::Todo498(_)))
        .count();
    assert_eq!(
        todo, EXPECTED_TODO498,
        "the #498 draw-test debt changed; if you added/removed a draw test, update EXPECTED_TODO498"
    );
}

/// Ataxx — outside both `WideVariantId` and `VariantId` — is a real, playable game
/// with its own `Position`, and is folded into the manifest so the ALL-driven
/// checks cover it. This asserts the fold-in is not a paper entry: the game
/// constructs and its perft file is the one the manifest names.
#[test]
fn ataxx_is_registered_and_real() {
    let ataxx = REQUIRED
        .iter()
        .find(|r| matches!(r.game, Game::Ataxx))
        .expect("Ataxx manifest row");
    assert_eq!(ataxx.perft_file, "perft_ataxx.rs");
    // The game is real: its start position enumerates legal moves.
    let pos = mcr::ataxx::Position::startpos();
    assert!(
        !pos.legal_moves().is_empty(),
        "Ataxx start position must have legal moves"
    );
    assert_eq!(pos.perft(1), 16, "Ataxx perft(1) is the FSF-pinned 16");
}
