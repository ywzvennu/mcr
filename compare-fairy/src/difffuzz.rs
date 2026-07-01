//! Differential fuzzer (issue #239): seeded random legal games cross-checked node
//! by node against Fairy-Stockfish (FSF).
//!
//! The pinned-corpus comparison in the rest of this crate confirms a fixed set of
//! hand-picked positions. That basket is broad but finite, so a movegen defect on
//! a *reachable but un-pinned* position can slip through — exactly the failure mode
//! that left the latent Xiangqi horse/soldier bugs uncaught until Minixiangqi's
//! random games (issue #199) hit them. This module closes the gap with a
//! differential fuzzer: for each variant it plays random **legal** games from the
//! start position and, at every node, asserts mce's `perft(1)`, `perft(2)`, and the
//! per-move divide equal FSF's `go perft 1` / `go perft 2` (+ divide) for the same
//! position. Any divergence is a surfaced movegen bug; it prints the FEN, both
//! dialects, and the differing move counts to reproduce.
//!
//! Determinism: the move picker is driven by an inline seeded splitmix64 PRNG —
//! **no `rand` crate, no clock**. The seed is a fixed default (overridable with
//! `--seed`), so every reported divergence is exactly reproducible.
//!
//! Scope: the **default** sweep (the fixed seed, three games, thirty plies) is a
//! zero-divergence gate over every fuzzable variant except the documented
//! [`HELD_BACK`] follow-ups — this is what the harness asserts. Driving it harder
//! (`--games` / `--plies` / other `--seed`s) turns it into an investigative tool
//! that reaches rarer states and may surface further candidates to triage; that is
//! the fuzzer working as intended, the same way Minixiangqi's random games first
//! exposed the latent Xiangqi bugs. Real movegen bugs it has already surfaced and
//! fixed: the Shinobi start array (Archer/Lancer for Shogi Knight/Commoner), the
//! Tori Pheasant drop-interposition (a jumped check wrongly blockable), and the
//! Spartan White-pawn promotion set (promoting to the Spartan army plus an illegal
//! King — issue #336).
//!
//! GPL FENCE unchanged: FSF is driven purely as a UCI subprocess (see `uci.rs`); no
//! GPL code is linked, and the INI it reads is a plain data file.
//!
//! Invocation (FSF-gated, like the rest of the crate):
//!
//! ```text
//! cargo run --release -- --difffuzz                       # all variants, default seed
//! cargo run --release -- --difffuzz --variant xiangqi     # one variant
//! cargo run --release -- --difffuzz --seed 7 --games 4 --plies 40
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use mce::geometry::{AnyWideVariant, WideVariantId};

use crate::uci::Engine;

/// A deterministic splitmix64 PRNG: seeded, reproducible, dependency-free.
///
/// splitmix64 is a tiny, well-distributed 64-bit generator; it is more than enough
/// to pick pseudo-random legal moves and needs neither the `rand` crate nor the
/// clock, keeping every fuzz run byte-for-byte reproducible from its seed.
struct Rng(u64);

impl Rng {
    /// Seed the generator. Any `u64` is a valid seed.
    fn new(seed: u64) -> Self {
        Rng(seed)
    }

    /// The next 64-bit output (the canonical splitmix64 step).
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A uniform index in `0..n` (caller guarantees `n > 0`).
    fn below(&mut self, n: usize) -> usize {
        debug_assert!(n > 0);
        (self.next_u64() % n as u64) as usize
    }
}

/// Derive a per-(variant, game) seed from the base seed, so each game is an
/// independent reproducible stream and re-seeding one variant never perturbs
/// another.
fn derive_seed(base: u64, variant_idx: usize, game: u32) -> u64 {
    let mixed = base
        .wrapping_add((variant_idx as u64).wrapping_mul(0xD1B5_4A32_D192_ED03))
        .wrapping_add((game as u64).wrapping_mul(0xCA45_57F8_5EBA_7C9B));
    Rng::new(mixed).next_u64()
}

/// The FSF-side description of a fuzzable variant: its mce identifier, the FSF
/// `UCI_Variant` name, whether it lives in `variants.ini` (vs a built-in), and the
/// FEN-dialect rewrite that turns an mce FEN into the one FSF parses.
struct Spec {
    /// The mce runtime-dispatch identifier.
    id: WideVariantId,
    /// FSF's `UCI_Variant` value.
    fsf: &'static str,
    /// Whether the variant is defined in `variants.ini` (not a FSF built-in).
    needs_ini: bool,
    /// mce FEN -> FSF FEN. The pinned-corpus modules already encapsulate every
    /// variant's letter/field rewrite; the fuzzer reuses those exact functions so
    /// the two engines always see the byte-identical position.
    dialect: fn(&str) -> String,
}

/// The dialect for variants mce and FSF spell identically: pass the FEN through.
fn identity(fen: &str) -> String {
    fen.to_string()
}

/// Amazon Chess dialect: mce spells the Amazon (Queen + Knight) with the
/// second-bank overflow token `**a`/`**A`; FSF spells it `a`/`A`. Strip the `**`
/// prefix from that token in the placement field; every other letter and field is
/// identical.
fn amazon_to_fsf(fen: &str) -> String {
    let (placement, rest) = match fen.split_once(' ') {
        Some((p, r)) => (p, Some(r)),
        None => (fen, None),
    };
    let mut out = String::with_capacity(placement.len());
    let mut chars = placement.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '*' && chars.peek() == Some(&'*') {
            // A second-bank overflow token `**X`: consume the second `*`, then emit
            // the base letter case-preserved (only `**a`/`**A`, the Amazon, occurs).
            chars.next();
            if let Some(base) = chars.next() {
                out.push(base);
            }
        } else {
            out.push(c);
        }
    }
    match rest {
        Some(r) => format!("{out} {r}"),
        None => out,
    }
}

/// Janus Chess dialect: mce spells the Janus (Bishop + Knight) as its Hawk `a`/`A`;
/// FSF spells it `j`/`J`. Rewrite that letter in the placement field; every other
/// letter and field is identical.
fn janus_to_fsf(fen: &str) -> String {
    let map = |c: char| match c {
        'a' => 'j',
        'A' => 'J',
        other => other,
    };
    match fen.split_once(' ') {
        Some((placement, rest)) => {
            let mapped: String = placement.chars().map(map).collect();
            format!("{mapped} {rest}")
        }
        None => fen.chars().map(map).collect(),
    }
}

/// Every variant the fuzzer cross-checks against FSF, each reusing its pinned-corpus
/// module's dialect rewrite.
///
/// Excluded by design:
/// * **Alice** — FSF has no Alice variant (two-board teleport ruleset), so there is
///   nothing to differentially check it against.
/// * **Duck** — the neutral duck is a known harness artifact (#189): FSF's
///   `go perft` counts duck placements differently from mce, a documented
///   non-bug, so fuzzing it would only re-surface that noise.
/// * **Jieqi** — hidden-information Xiangqi; its FSF cross-check needs a
///   per-position identity reveal (see `jieqi.rs`), not a static dialect rewrite, so
///   it cannot be driven from an arbitrary fuzzed FEN.
const SPECS: &[Spec] = &[
    Spec {
        // Almost Chess shares Capablanca's `e -> c` chancellor rewrite (its only
        // non-standard piece is the Rook+Knight Chancellor).
        id: WideVariantId::Almost,
        fsf: "almost",
        needs_ini: false,
        dialect: crate::capablanca::fen_to_fsf,
    },
    Spec {
        // Amazon Chess: mce spells the Amazon `**a`; FSF spells it `a`.
        id: WideVariantId::Amazon,
        fsf: "amazon",
        needs_ini: false,
        dialect: amazon_to_fsf,
    },
    Spec {
        id: WideVariantId::Asean,
        fsf: "asean",
        needs_ini: false,
        dialect: crate::asean::fen_to_fsf,
    },
    Spec {
        id: WideVariantId::Bughouse,
        fsf: "bughouse",
        needs_ini: false,
        dialect: identity,
    },
    Spec {
        id: WideVariantId::Cambodian,
        fsf: "cambodian",
        needs_ini: false,
        dialect: identity,
    },
    Spec {
        id: WideVariantId::CannonShogi,
        fsf: "cannonshogi",
        needs_ini: true,
        dialect: crate::cannonshogi::fen_to_fsf,
    },
    Spec {
        id: WideVariantId::Capablanca,
        fsf: "capablanca",
        needs_ini: false,
        dialect: crate::capablanca::fen_to_fsf,
    },
    Spec {
        id: WideVariantId::Capahouse,
        fsf: "capahouse",
        needs_ini: false,
        dialect: crate::capahouse::fen_to_fsf,
    },
    Spec {
        // Caparandom shares Capablanca's `e -> c` chancellor rewrite (same 10x8
        // army); its file-letter (`JAja`) castling field carries no piece letters,
        // so the placement-only rewrite leaves it untouched.
        id: WideVariantId::Caparandom,
        fsf: "caparandom",
        needs_ini: false,
        dialect: crate::capablanca::fen_to_fsf,
    },
    Spec {
        id: WideVariantId::Chak,
        fsf: "chak",
        needs_ini: true,
        dialect: crate::chak::to_fsf_dialect,
    },
    Spec {
        // Chancellor chess (9x9) shares Capablanca's `e -> c` chancellor rewrite
        // (its only non-standard piece is the Rook+Knight Chancellor). A FSF
        // built-in, so no `variants.ini` is required.
        id: WideVariantId::Chancellor,
        fsf: "chancellor",
        needs_ini: false,
        dialect: crate::capablanca::fen_to_fsf,
    },
    Spec {
        id: WideVariantId::Chennis,
        fsf: "chennis",
        needs_ini: true,
        dialect: crate::chennis::to_fsf_dialect,
    },
    Spec {
        // Chigorin shares Capablanca's `e -> c` chancellor rewrite (White's only
        // non-standard piece is the Rook+Knight Chancellor).
        id: WideVariantId::Chigorin,
        fsf: "chigorin",
        needs_ini: false,
        dialect: crate::capablanca::fen_to_fsf,
    },
    Spec {
        id: WideVariantId::Dobutsu,
        fsf: "dobutsu",
        needs_ini: false,
        dialect: crate::dobutsu::fen_to_fsf,
    },
    Spec {
        id: WideVariantId::Dragon,
        fsf: "dragon",
        needs_ini: false,
        dialect: crate::dragon::fen_to_fsf,
    },
    Spec {
        // Embassy shares Capablanca's `e -> c` chancellor rewrite (same 10x8
        // Chancellor + Archbishop army, king on the e-file).
        id: WideVariantId::Embassy,
        fsf: "embassy",
        needs_ini: false,
        dialect: crate::capablanca::fen_to_fsf,
    },
    Spec {
        id: WideVariantId::Empire,
        fsf: "empire",
        needs_ini: true,
        dialect: crate::empire::to_fsf_dialect,
    },
    Spec {
        id: WideVariantId::FogOfWar,
        fsf: "fogofwar",
        needs_ini: true,
        dialect: identity,
    },
    Spec {
        id: WideVariantId::Gorogoro,
        fsf: "gorogoroplus",
        needs_ini: true,
        dialect: identity,
    },
    Spec {
        // Gothic shares Capablanca's `e -> c` chancellor rewrite (same 10x8
        // Chancellor + Archbishop army, different back-rank order).
        id: WideVariantId::Gothic,
        fsf: "gothic",
        needs_ini: false,
        dialect: crate::capablanca::fen_to_fsf,
    },
    Spec {
        id: WideVariantId::Grand,
        fsf: "grand",
        needs_ini: false,
        dialect: crate::grand::fen_to_fsf,
    },
    Spec {
        id: WideVariantId::Grandhouse,
        fsf: "grandhouse",
        needs_ini: true,
        dialect: crate::grandhouse::fen_to_fsf,
    },
    Spec {
        id: WideVariantId::HoppelPoppel,
        fsf: "hoppelpoppel",
        needs_ini: false,
        dialect: crate::hoppelpoppel::to_fsf_dialect,
    },
    Spec {
        id: WideVariantId::Janggi,
        fsf: "janggi",
        needs_ini: false,
        dialect: crate::janggi::fen_to_fsf,
    },
    Spec {
        // Janus: mce spells the Janus (Bishop + Knight) `a`; FSF spells it `j`.
        id: WideVariantId::Janus,
        fsf: "janus",
        needs_ini: false,
        dialect: janus_to_fsf,
    },
    Spec {
        id: WideVariantId::Khans,
        fsf: "khans",
        needs_ini: true,
        dialect: crate::khans::to_fsf_dialect,
    },
    Spec {
        id: WideVariantId::Knightmate,
        fsf: "knightmate",
        needs_ini: false,
        dialect: crate::knightmate::to_fsf_dialect,
    },
    Spec {
        id: WideVariantId::Kyotoshogi,
        fsf: "kyotoshogi",
        needs_ini: false,
        dialect: identity,
    },
    Spec {
        id: WideVariantId::Makpong,
        fsf: "makpong",
        needs_ini: false,
        dialect: identity,
    },
    Spec {
        id: WideVariantId::Makruk,
        fsf: "makruk",
        needs_ini: false,
        dialect: identity,
    },
    Spec {
        id: WideVariantId::Manchu,
        fsf: "manchu",
        needs_ini: false,
        dialect: crate::manchu::fen_to_fsf,
    },
    Spec {
        id: WideVariantId::Mansindam,
        fsf: "mansindam",
        needs_ini: true,
        dialect: crate::mansindam::to_fsf_dialect,
    },
    Spec {
        id: WideVariantId::Minishogi,
        fsf: "minishogi",
        needs_ini: false,
        dialect: identity,
    },
    Spec {
        id: WideVariantId::Minixiangqi,
        fsf: "minixiangqi",
        needs_ini: false,
        dialect: crate::minixiangqi::fen_to_fsf,
    },
    Spec {
        id: WideVariantId::Orda,
        fsf: "orda",
        needs_ini: true,
        dialect: crate::orda::to_fsf_dialect,
    },
    Spec {
        id: WideVariantId::Ordamirror,
        fsf: "ordamirror",
        needs_ini: true,
        dialect: crate::ordamirror::to_fsf_dialect,
    },
    Spec {
        id: WideVariantId::Placement,
        fsf: "placement",
        needs_ini: false,
        dialect: identity,
    },
    Spec {
        id: WideVariantId::Seirawan,
        fsf: "seirawan",
        needs_ini: false,
        dialect: crate::seirawan::fen_to_fsf,
    },
    Spec {
        id: WideVariantId::Shako,
        fsf: "shako",
        needs_ini: false,
        dialect: crate::shako::fen_to_fsf,
    },
    Spec {
        id: WideVariantId::Shatar,
        fsf: "shatar",
        needs_ini: false,
        dialect: crate::shatar::to_fsf_dialect,
    },
    Spec {
        id: WideVariantId::Shatranj,
        fsf: "shatranj",
        needs_ini: false,
        dialect: crate::shatranj::to_fsf_dialect,
    },
    Spec {
        id: WideVariantId::Shinobi,
        fsf: "shinobi",
        needs_ini: true,
        dialect: crate::shinobi::to_fsf_dialect,
    },
    Spec {
        id: WideVariantId::Shogi,
        fsf: "shogi",
        needs_ini: false,
        dialect: identity,
    },
    Spec {
        id: WideVariantId::Shogun,
        fsf: "shogun",
        needs_ini: true,
        dialect: crate::shogun::to_fsf_dialect,
    },
    Spec {
        id: WideVariantId::ShoShogi,
        fsf: "shoshogi",
        needs_ini: false,
        dialect: crate::shoshogi::to_fsf_dialect,
    },
    Spec {
        id: WideVariantId::Shouse,
        fsf: "shouse",
        needs_ini: false,
        dialect: crate::shouse::fen_to_fsf,
    },
    Spec {
        id: WideVariantId::Sittuyin,
        fsf: "sittuyin",
        needs_ini: false,
        dialect: crate::sittuyin::to_fsf_dialect,
    },
    Spec {
        id: WideVariantId::Spartan,
        fsf: "spartan",
        needs_ini: false,
        dialect: crate::spartan::to_fsf_dialect,
    },
    Spec {
        id: WideVariantId::Synochess,
        fsf: "synochess",
        needs_ini: true,
        dialect: crate::synochess::to_fsf_dialect,
    },
    Spec {
        id: WideVariantId::Tori,
        fsf: "torishogi",
        needs_ini: false,
        dialect: crate::tori::fen_to_fsf,
    },
    Spec {
        id: WideVariantId::Xiangfu,
        fsf: "xiangfu",
        needs_ini: true,
        dialect: crate::xiangfu::fen_to_fsf,
    },
    Spec {
        id: WideVariantId::Xiangqi,
        fsf: "xiangqi",
        needs_ini: false,
        dialect: crate::xiangqi::fen_to_fsf,
    },
];

/// Variants whose dialect/movegen the fuzzer can drive, but whose deeper random
/// games surface a divergence that is **not yet resolved** — so they are held back
/// from the default all-variants sweep to keep it a trustworthy zero-divergence
/// gate. Each is still reachable explicitly with `--difffuzz --variant <name>` for
/// investigation.
///
/// The #239 deeper-sweep candidates were triaged in #336; three were resolved
/// there and no longer need holding back:
///
/// * **Spartan** — a real mce bug (a White Persian pawn promoted to the *Spartan*
///   army set plus an illegal King); fixed in `SpartanRules::promotion_targets`.
/// * **Seirawan / Shouse** — the only residual is an FSF artifact, not an mce bug:
///   S-Chess shares one FEN field for castling rights and gating-eligible squares,
///   so once a king has moved (losing castling) while a corner rook stays
///   gating-eligible, mce serializes that surviving gate as a bare corner-file
///   letter `A`/`H`/`a`/`h`; FSF has no encoding for "corner rook still gates but
///   the king has moved" and reads it as a *castling right*, emitting an illegal
///   castle of the already-moved king. That exact node is skipped (see
///   [`is_schess_corner_castle_artifact`]); every other S-Chess node is checked.
///
/// Resolved and released back into the default sweep (`HELD_BACK` is now EMPTY —
/// the entire variant set runs clean under the differential fuzzer):
///
/// * **Shako** (issue #335) — FSF forbids castling the king across a square a
///   **cannon** attacks over a screen, but mce's castling king-walk danger map
///   (built by *forward*-projecting each enemy piece) missed a cannon's
///   over-screen capture on the *empty* transit square the king walks onto. The
///   fix re-tests each transit square with the king placed on it (see
///   `GenericPosition::gen_castles`), so the cannon's forward projection lands on
///   it. Repro: `c1q1ck4/vrn6v/2pp2p1r1/4p1n2p/pp3p1ppb/1Q2PP4/PN1P3P2/
///   1PP1N1P1Pb/1R3KB2R/1V1CB3VC b Q - 1 17`, then `e10e5` — mce's `f2d2` is now
///   correctly illegal (the e2 transit square is hit by the e5 cannon over the e3
///   screen). Now clean under the differential fuzzer.
///
/// * **Synochess** (issue #337) — its "deeper-sweep `z`-piece divergence" was the
///   SAME cannon-aware castling-king-walk bug (Synochess reuses the Janggi cannon):
///   the divergent nodes were castles of a king walking across a square a cannon
///   attacks over a screen. The Shako fix above (in `GenericPosition::gen_castles`,
///   gated by `has_cannons`/`has_flying_general`) resolves it too — synochess is
///   clean over deep seeded sweeps (seed 7+, 8 games × 80 plies, 0 divergences).
const HELD_BACK: &[WideVariantId] = &[];

/// Tunables for a fuzz run (parsed from the CLI in `main.rs`).
pub struct Config {
    /// The base PRNG seed (default fixed; overridable with `--seed`).
    pub seed: u64,
    /// Random games per variant.
    pub games: u32,
    /// Maximum plies per game (a game also stops early at a terminal node).
    pub plies: u32,
    /// Restrict to a single mce variant name, or `None` for all.
    pub variant: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            seed: 0x239,
            games: 3,
            plies: 30,
            variant: None,
        }
    }
}

/// What a single node cross-check found wrong.
struct Divergence {
    fen: String,
    fsf_fen: String,
    mce_p1: u64,
    fsf_p1: u64,
    mce_p2: u64,
    fsf_p2: u64,
    mce_divide: Vec<(String, u64)>,
    fsf_divide: Vec<(String, u64)>,
}

/// Per-variant fuzz outcome.
struct VariantStat {
    nodes_checked: u64,
    /// Nodes the cross-check deliberately skipped as a documented FSF artifact
    /// (see [`is_schess_corner_castle_artifact`]).
    nodes_skipped: u64,
    games: u32,
    divergences: usize,
    skipped: bool,
}

/// Resolve the FSF `variants.ini`: `$MCE_FSF_VARIANTS_INI`, then a sibling
/// `variants.ini` beside the FSF binary (the upstream `…/src/stockfish` +
/// `…/src/variants.ini` layout).
fn resolve_variants_ini(fsf_bin: &str) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MCE_FSF_VARIANTS_INI") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
    }
    let sibling = PathBuf::from(fsf_bin).parent()?.join("variants.ini");
    sibling.is_file().then_some(sibling)
}

/// Whether an S-Chess (Seirawan / S-House) node is an FSF castle-rights artifact
/// that cannot be faithfully cross-checked, so the fuzzer skips it.
///
/// S-Chess folds castling rights and gating-eligible back-rank squares into one FEN
/// field. Gating is per-piece — a back-rank piece may introduce a Hawk/Elephant on
/// its **first** move (pychess/chessvariants) — so once a king has moved (losing
/// castling) a corner rook that has *not* moved is still gating-eligible. mce
/// serializes that surviving corner gate as a bare corner-file letter (`A`/`H` for
/// White, `a`/`h` for Black) in the field. FSF has no encoding for "corner rook
/// still gates but the king has moved": it reads the corner letter as a *castling
/// right* and emits a castle of the already-moved king (verified — FSF plays e.g.
/// `f8h8`, landing the king on g8: an illegal castle). The two FENs are therefore
/// not mutually representable at this node, so it is skipped — the same targeted
/// approach the neutral Duck (#189) uses. Every other S-Chess node is still checked.
///
/// The signal is exact: a corner-file letter appears in the field **only** in this
/// state. While castling is available the corners ride on `K`/`Q`/`k`/`q`, and the
/// non-corner gating files are `b`..`g` only, so a standalone `A`/`H`/`a`/`h`
/// implies the king has moved while its corner rook stays gating-eligible. Limited
/// to Seirawan / S-House, whose FEN field carries this dual meaning.
fn is_schess_corner_castle_artifact(spec: &Spec, fen: &str) -> bool {
    if spec.id != WideVariantId::Seirawan && spec.id != WideVariantId::Shouse {
        return false;
    }
    // The castling/gating field is the 3rd space-separated field.
    fen.split(' ')
        .nth(2)
        .is_some_and(|castling| castling.contains(['A', 'H', 'a', 'h']))
}

/// Cross-check one node: mce `perft(1)`/`perft(2)` + divide vs FSF's.
///
/// Returns `Ok(())` when the two engines agree, `Ok-with-Err` carrying a
/// [`Divergence`] when they disagree, or an outer `Err` for an FSF protocol/parse
/// failure (e.g. FSF rejects the dialect FEN).
fn check_node(
    engine: &mut Engine,
    spec: &Spec,
    pos: &AnyWideVariant,
) -> Result<Result<(), Box<Divergence>>, String> {
    let fen = pos.to_fen();
    let fsf_fen = (spec.dialect)(&fen);

    // Re-parse the mce side from the FEN string FSF is fed, so both engines
    // evaluate the *same stateless position*. FSF's `position fen` carries no move
    // history, so comparing it against an in-game mce position would wrongly flag
    // history-only state that the FEN does not encode (e.g. Janggi's
    // consecutive-pass adjudication) as a movegen divergence. Re-parsing makes the
    // comparison FEN-to-FEN; a failure here would itself be an mce round-trip bug.
    let node = AnyWideVariant::from_fen(spec.id, &fen)
        .map_err(|e| format!("mce failed to re-parse its own FEN {fen:?}: {e:?}"))?;

    // ---- mce side: perft(1) is the legal-move count; perft(2)'s divide is each
    // legal move's child perft(1). -------------------------------------------
    let moves = node.legal_moves();
    let mce_p1 = moves.len() as u64;
    let mut mce_divide: Vec<(String, u64)> = Vec::with_capacity(moves.len());
    let mut mce_p2 = 0u64;
    for mv in &moves {
        let child_nodes = node.play(mv).perft(1);
        mce_p2 += child_nodes;
        mce_divide.push((node.to_uci(mv), child_nodes));
    }

    // ---- FSF side -----------------------------------------------------------
    engine.set_variant(spec.fsf, false)?;
    engine.set_position(&fsf_fen)?;
    let fsf_p1 = engine.go_perft(1, false)?.nodes;
    let fsf_res = engine.go_perft(2, true)?;
    let fsf_p2 = fsf_res.nodes;
    let fsf_divide = fsf_res.divide;

    // The divide move *labels* use each engine's own UCI dialect (drops, gates,
    // and shogi-style promotions render differently), so comparing them by string
    // would be a false-positive factory. The *multiset of child counts* is
    // label-independent: if both engines enumerate the same legal moves, each
    // move's subtree size is well defined, so the sorted count vectors must match.
    // A mismatch there (even with equal totals) is a genuine divergence in how the
    // node's successors are generated.
    let mut mce_counts: Vec<u64> = mce_divide.iter().map(|&(_, n)| n).collect();
    let mut fsf_counts: Vec<u64> = fsf_divide.iter().map(|&(_, n)| n).collect();
    mce_counts.sort_unstable();
    fsf_counts.sort_unstable();

    if mce_p1 != fsf_p1 || mce_p2 != fsf_p2 || mce_counts != fsf_counts {
        return Ok(Err(Box::new(Divergence {
            fen,
            fsf_fen,
            mce_p1,
            fsf_p1,
            mce_p2,
            fsf_p2,
            mce_divide,
            fsf_divide,
        })));
    }
    Ok(Ok(()))
}

/// Print a divergence in full reproduction detail.
fn report_divergence(id: WideVariantId, fsf: &str, d: &Divergence) {
    eprintln!(
        "*** DIFFFUZZ DIVERGENCE {} (UCI_Variant {fsf}) ***",
        id.as_str()
    );
    eprintln!("    mce FEN : {}", d.fen);
    eprintln!("    FSF FEN : {}", d.fsf_fen);
    eprintln!("    perft(1): mce={} fsf={}", d.mce_p1, d.fsf_p1);
    eprintln!("    perft(2): mce={} fsf={}", d.mce_p2, d.fsf_p2);

    // Localise: group both divides by child count and show where the multisets
    // diverge, plus the raw labelled lists for hand inspection.
    let mut mce_by_count: BTreeMap<u64, usize> = BTreeMap::new();
    for &(_, n) in &d.mce_divide {
        *mce_by_count.entry(n).or_default() += 1;
    }
    let mut fsf_by_count: BTreeMap<u64, usize> = BTreeMap::new();
    for &(_, n) in &d.fsf_divide {
        *fsf_by_count.entry(n).or_default() += 1;
    }
    eprintln!("    divide child-count histogram (count -> #moves):");
    let mut keys: Vec<u64> = mce_by_count
        .keys()
        .chain(fsf_by_count.keys())
        .copied()
        .collect();
    keys.sort_unstable();
    keys.dedup();
    for k in keys {
        let m = mce_by_count.get(&k).copied().unwrap_or(0);
        let f = fsf_by_count.get(&k).copied().unwrap_or(0);
        if m != f {
            eprintln!("      {k:>6}: mce {m}  fsf {f}   <- differs");
        }
    }
    eprintln!(
        "    mce divide ({} moves): {}",
        d.mce_divide.len(),
        fmt_divide(&d.mce_divide)
    );
    eprintln!(
        "    fsf divide ({} moves): {}",
        d.fsf_divide.len(),
        fmt_divide(&d.fsf_divide)
    );
}

/// Compact `mv:count mv:count …` rendering, capped so a wide node stays readable.
fn fmt_divide(divide: &[(String, u64)]) -> String {
    let mut sorted = divide.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let shown: Vec<String> = sorted
        .iter()
        .take(48)
        .map(|(m, n)| format!("{m}:{n}"))
        .collect();
    let mut s = shown.join(" ");
    if sorted.len() > 48 {
        s.push_str(" …");
    }
    s
}

/// Play one seeded random legal game, cross-checking every node. Returns
/// `(nodes_checked, nodes_skipped, divergences, fsf_error)`; an FSF protocol error
/// ends the game.
fn fuzz_game(
    engine: &mut Engine,
    spec: &Spec,
    mut rng: Rng,
    plies: u32,
) -> (u64, u64, usize, Option<String>) {
    let mut pos = AnyWideVariant::startpos(spec.id);
    let mut nodes = 0u64;
    let mut skipped = 0u64;
    let mut divergences = 0usize;

    for _ in 0..plies {
        if pos.outcome().is_some() {
            break;
        }
        let moves = pos.legal_moves();
        if moves.is_empty() {
            break;
        }

        // A documented FSF artifact (not an mce bug) is not cross-checkable; skip
        // the node but play on so the random walk still explores past it.
        if is_schess_corner_castle_artifact(spec, &pos.to_fen()) {
            skipped += 1;
        } else {
            match check_node(engine, spec, &pos) {
                Ok(Ok(())) => {}
                Ok(Err(d)) => {
                    divergences += 1;
                    report_divergence(spec.id, spec.fsf, &d);
                }
                Err(e) => return (nodes, skipped, divergences, Some(e)),
            }
            nodes += 1;
        }

        let idx = rng.below(moves.len());
        pos = pos.play(&moves[idx]);
    }
    (nodes, skipped, divergences, None)
}

/// Fuzz one variant for `cfg.games` seeded games of up to `cfg.plies` plies.
fn fuzz_variant(engine: &mut Engine, spec: &Spec, variant_idx: usize, cfg: &Config) -> VariantStat {
    let mut stat = VariantStat {
        nodes_checked: 0,
        nodes_skipped: 0,
        games: 0,
        divergences: 0,
        skipped: false,
    };

    if !engine.has_variant(spec.fsf) {
        println!(
            "  SKIP {:<14} (FSF binary lacks `{}`; build largeboards=yes / load variants.ini)",
            spec.id.as_str(),
            spec.fsf,
        );
        stat.skipped = true;
        return stat;
    }

    for game in 0..cfg.games {
        let rng = Rng::new(derive_seed(cfg.seed, variant_idx, game));
        let (nodes, skipped, divergences, err) = fuzz_game(engine, spec, rng, cfg.plies);
        stat.nodes_checked += nodes;
        stat.nodes_skipped += skipped;
        stat.divergences += divergences;
        stat.games += 1;
        if let Some(e) = err {
            eprintln!(
                "  note {}: FSF protocol error after {nodes} nodes in game {game}: {e}",
                spec.id.as_str(),
            );
        }
    }

    let skip_note = if stat.nodes_skipped > 0 {
        format!("  ({} FSF-artifact node(s) skipped)", stat.nodes_skipped)
    } else {
        String::new()
    };
    println!(
        "  {:<14} games {:>2}  nodes {:>6}  {}{}",
        spec.id.as_str(),
        stat.games,
        stat.nodes_checked,
        if stat.divergences == 0 {
            "ok".to_string()
        } else {
            format!("{} DIVERGENCE(S)", stat.divergences)
        },
        skip_note,
    );
    stat
}

/// Run the differential fuzzer. Returns the total number of divergences found
/// (0 = clean). `main` maps a non-zero return to a non-zero exit status.
pub fn run(engine: &mut Engine, fsf_bin: &str, cfg: &Config) -> usize {
    println!();
    println!(
        "Differential fuzzer (issue #239): seeded random legal games, perft(1..2)+divide vs FSF"
    );
    println!(
        "  seed={:#x} games/variant={} max-plies={}",
        cfg.seed, cfg.games, cfg.plies
    );

    // Load the INI so the non-built-in variants (Orda, Shinobi, Chak, …) join the
    // engine's `UCI_Variant` list. Best-effort: built-in variants still run if the
    // INI is absent.
    let mut ini_loaded = false;
    if let Some(ini) = resolve_variants_ini(fsf_bin) {
        match engine.load_variant_path(&ini.to_string_lossy()) {
            Ok(()) => {
                println!("  loaded variants.ini: {}", ini.display());
                ini_loaded = true;
            }
            Err(e) => {
                eprintln!("  warning: could not load variants.ini ({e}); INI variants skipped")
            }
        }
    } else {
        println!("  no variants.ini found (set $MCE_FSF_VARIANTS_INI); INI variants skipped");
    }

    // Resolve the optional single-variant filter up front so a bad name fails fast.
    let only: Option<WideVariantId> = match &cfg.variant {
        Some(name) => match name.parse::<WideVariantId>() {
            Ok(id) => {
                if !SPECS.iter().any(|s| s.id == id) {
                    eprintln!(
                        "ERROR: variant {:?} is not differentially fuzzable (Alice/Duck/Jieqi are \
excluded by design).",
                        id.as_str(),
                    );
                    return 1;
                }
                Some(id)
            }
            Err(e) => {
                eprintln!("ERROR: {e}");
                return 1;
            }
        },
        None => None,
    };

    let mut total_divergences = 0usize;
    let mut total_nodes = 0u64;
    let mut variants_run = 0usize;
    let mut variants_skipped = 0usize;

    for (idx, spec) in SPECS.iter().enumerate() {
        if let Some(id) = only {
            if spec.id != id {
                continue;
            }
        } else if HELD_BACK.contains(&spec.id) {
            // Held back from the default all-variants sweep (see `HELD_BACK`); still
            // reachable with `--variant` for investigation.
            println!(
                "  HELD {:<14} (deeper-sweep divergence under follow-up; --variant to run)",
                spec.id.as_str()
            );
            variants_skipped += 1;
            continue;
        }
        if spec.needs_ini && !ini_loaded && !engine.has_variant(spec.fsf) {
            println!(
                "  SKIP {:<14} (INI variant; no variants.ini loaded)",
                spec.id.as_str()
            );
            variants_skipped += 1;
            continue;
        }
        let stat = fuzz_variant(engine, spec, idx, cfg);
        if stat.skipped {
            variants_skipped += 1;
        } else {
            variants_run += 1;
            total_nodes += stat.nodes_checked;
            total_divergences += stat.divergences;
        }
    }

    println!();
    if total_divergences == 0 {
        println!(
            "OK: differential fuzzer found 0 divergences across {variants_run} variant(s) \
({total_nodes} nodes cross-checked vs FSF; {variants_skipped} skipped).",
        );
    } else {
        eprintln!(
            "ERROR: differential fuzzer found {total_divergences} divergence(s) across \
{variants_run} variant(s) ({total_nodes} nodes checked). See the FENs above to reproduce.",
        );
    }
    total_divergences
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The PRNG is deterministic: same seed -> same stream; different seeds ->
    /// different streams. This is the property the whole fuzzer's reproducibility
    /// rests on, and it runs without FSF.
    #[test]
    fn rng_is_deterministic_and_seed_sensitive() {
        let a: Vec<u64> = (0..8)
            .map(|_| ())
            .scan(Rng::new(0x239), |r, _| Some(r.next_u64()))
            .collect();
        let b: Vec<u64> = (0..8)
            .map(|_| ())
            .scan(Rng::new(0x239), |r, _| Some(r.next_u64()))
            .collect();
        assert_eq!(a, b, "same seed must reproduce the same stream");

        let c: Vec<u64> = (0..8)
            .map(|_| ())
            .scan(Rng::new(0x240), |r, _| Some(r.next_u64()))
            .collect();
        assert_ne!(a, c, "different seeds must diverge");

        // `below` stays in range.
        let mut r = Rng::new(1);
        for _ in 0..1000 {
            assert!(r.below(7) < 7);
        }
    }

    /// Per-(variant, game) seeds are distinct across both axes, so games never
    /// alias each other.
    #[test]
    fn derived_seeds_are_distinct() {
        let mut seen = std::collections::HashSet::new();
        for v in 0..SPECS.len() {
            for g in 0..16 {
                assert!(
                    seen.insert(derive_seed(0x239, v, g)),
                    "seed collision at ({v},{g})"
                );
            }
        }
    }

    /// The mce side of the cross-check is internally consistent for every fuzzable
    /// variant: walking a short seeded random game, `perft(2)` always equals the
    /// sum of each legal move's child `perft(1)` (the divide), and `play` never
    /// panics. This validates the fuzzer's mce-side arithmetic without FSF, so it
    /// runs in the default `cargo test`.
    #[test]
    fn mce_side_self_consistent_on_random_games() {
        for (idx, spec) in SPECS.iter().enumerate() {
            let mut rng = Rng::new(derive_seed(0x239, idx, 0));
            let mut pos = AnyWideVariant::startpos(spec.id);
            for _ in 0..12 {
                if pos.outcome().is_some() {
                    break;
                }
                let moves = pos.legal_moves();
                if moves.is_empty() {
                    break;
                }
                let p1 = moves.len() as u64;
                assert_eq!(
                    p1,
                    pos.perft(1),
                    "{} perft(1) == move count",
                    spec.id.as_str()
                );
                let mut p2 = 0u64;
                for mv in &moves {
                    p2 += pos.play(mv).perft(1);
                }
                assert_eq!(
                    p2,
                    pos.perft(2),
                    "{} perft(2) == sum of child perft(1)",
                    spec.id.as_str()
                );
                let idx = rng.below(moves.len());
                pos = pos.play(&moves[idx]);
            }
        }
    }

    /// Every fuzzable spec names a real, distinct variant, and the three documented
    /// exclusions are absent.
    #[test]
    fn specs_are_well_formed() {
        let mut ids = std::collections::HashSet::new();
        for spec in SPECS {
            assert!(
                ids.insert(spec.id),
                "duplicate spec for {}",
                spec.id.as_str()
            );
            assert!(!spec.fsf.is_empty());
        }
        for excluded in [
            WideVariantId::Alice,
            WideVariantId::Duck,
            WideVariantId::Jieqi,
        ] {
            assert!(
                !ids.contains(&excluded),
                "{} must be excluded",
                excluded.as_str()
            );
        }
        // Every shipped variant minus the 3 by-design exclusions (Alice / Duck /
        // Jieqi); the deeper-sweep follow-ups stay in SPECS but are skipped via
        // `HELD_BACK` on the default run.
        assert_eq!(SPECS.len(), WideVariantId::ALL.len() - 3);
    }

    /// Every `HELD_BACK` id is a real, distinct fuzzable spec (so a rename can never
    /// silently drop a hold-back), and the default sweep therefore runs the rest.
    #[test]
    fn held_back_entries_are_specs_and_distinct() {
        let spec_ids: std::collections::HashSet<_> = SPECS.iter().map(|s| s.id).collect();
        let mut seen = std::collections::HashSet::new();
        for &id in HELD_BACK {
            assert!(seen.insert(id), "duplicate HELD_BACK entry {}", id.as_str());
            assert!(
                spec_ids.contains(&id),
                "HELD_BACK names {} which is not a spec",
                id.as_str()
            );
        }
        // The default sweep still covers the clear majority of variants.
        assert!(SPECS.len() - HELD_BACK.len() >= 39);
    }

    /// The S-Chess corner-castle artifact detector fires exactly on a standalone
    /// corner-file letter in the castling field of a Seirawan / S-House FEN, and
    /// never for a castling-available field or a non-S-Chess variant.
    #[test]
    fn schess_corner_castle_artifact_is_precise() {
        let spec = SPECS
            .iter()
            .find(|s| s.id == WideVariantId::Seirawan)
            .expect("seirawan spec");
        let shouse = SPECS
            .iter()
            .find(|s| s.id == WideVariantId::Shouse)
            .expect("shouse spec");
        let other = SPECS
            .iter()
            .find(|s| s.id == WideVariantId::Capablanca)
            .expect("capablanca spec");

        // King has moved (no K/Q/k/q) but a corner rook stays gating-eligible: the
        // field carries a bare `h` (Black) / `A` (White) — the artifact.
        assert!(is_schess_corner_castle_artifact(
            spec,
            "1rbq1k1r/1pbp2a1/4pppn/2e2P2/1A1NP2p/6PN/P1PP3P/R1BQKERB[] b QCDcdh - 4 20",
        ));
        assert!(is_schess_corner_castle_artifact(
            spec,
            "r4bnr/3kqp1p/3pNn2/8/8/8/1P1BBPP1/R2K1ENR[] w AGHafgh - 2 21",
        ));
        // The same dual-meaning field applies to S-House.
        assert!(is_schess_corner_castle_artifact(
            shouse,
            "1rbq1k1r/1pbp2a1/8/8/8/8/P1PP3P/R1BQKERB[] b QCDcdh - 4 20",
        ));
        // A castling-available start field uses K/Q for the corners and only b..g
        // file letters — no artifact.
        assert!(!is_schess_corner_castle_artifact(
            spec,
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[HEhe] w KQBCDFGkqbcdfg - 0 1",
        ));
        // Restricted to S-Chess: another variant whose castling field legitimately
        // carries file letters must not be touched.
        assert!(!is_schess_corner_castle_artifact(
            other,
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w ah - 0 1",
        ));
    }
}
