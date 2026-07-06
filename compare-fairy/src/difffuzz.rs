//! Differential fuzzer (issue #239): seeded random legal games cross-checked node
//! by node against Fairy-Stockfish (FSF).
//!
//! The pinned-corpus comparison in the rest of this crate confirms a fixed set of
//! hand-picked positions. That basket is broad but finite, so a movegen defect on
//! a *reachable but un-pinned* position can slip through — exactly the failure mode
//! that left the latent Xiangqi horse/soldier bugs uncaught until Minixiangqi's
//! random games (issue #199) hit them. This module closes the gap with a
//! differential fuzzer: for each variant it plays random **legal** games from the
//! start position and, at every node, asserts mcr's `perft(1)`, `perft(2)`, and the
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

use mcr::geometry::{AnyWideVariant, WideMove, WideMoveKind, WideVariantId};

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

/// The FSF-side description of a fuzzable variant: its mcr identifier, the FSF
/// `UCI_Variant` name, whether it lives in `variants.ini` (vs a built-in), and the
/// FEN-dialect rewrite that turns an mcr FEN into the one FSF parses.
struct Spec {
    /// The mcr runtime-dispatch identifier.
    id: WideVariantId,
    /// FSF's `UCI_Variant` value.
    fsf: &'static str,
    /// Whether the variant is defined in `variants.ini` (not a FSF built-in).
    needs_ini: bool,
    /// mcr FEN -> FSF FEN. The pinned-corpus modules already encapsulate every
    /// variant's letter/field rewrite; the fuzzer reuses those exact functions so
    /// the two engines always see the byte-identical position.
    dialect: fn(&str) -> String,
}

/// The dialect for variants mcr and FSF spell identically: pass the FEN through.
fn identity(fen: &str) -> String {
    fen.to_string()
}

/// Amazon Chess dialect: mcr spells the Amazon (Queen + Knight) with the
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

/// Ai-Wok dialect: mcr fields the Ai-Wok (Rook + Knight + Ferz) as the existing
/// [`WideRole::Ship`], spelled with the second-bank overflow token `**s`/`**S`;
/// FSF's `ai-wok` spells it `a`/`A`. Strip the `**` prefix from that token and
/// remap the recycled base letter `s`->`a` (case-preserving) in the placement
/// field. The bare Silver/Khon `s`/`S` (which has no `**` prefix) and every other
/// letter and field are left untouched.
fn aiwok_to_fsf(fen: &str) -> String {
    let (placement, rest) = match fen.split_once(' ') {
        Some((p, r)) => (p, Some(r)),
        None => (fen, None),
    };
    let mut out = String::with_capacity(placement.len());
    let mut chars = placement.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '*' && chars.peek() == Some(&'*') {
            // A second-bank overflow token `**s`/`**S`, the Ai-Wok (Ship): consume
            // the second `*`, then emit FSF's `a`/`A`, case-preserved.
            chars.next();
            match chars.next() {
                Some('s') => out.push('a'),
                Some('S') => out.push('A'),
                Some(base) => out.push(base),
                None => {}
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

/// Janus Chess dialect: mcr spells the Janus (Bishop + Knight) as its Hawk `a`/`A`;
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

/// Centaur Chess dialect: mcr spells the Centaur (King + Knight) with the Orda
/// Kheshig letter `w`/`W`; FSF's INI `centaur` variant spells it `c`/`C`. Rewrite
/// that letter in the placement field; the standard KQkq castling field carries no
/// piece letters and the side-to-move `w` sits outside the placement field, so both
/// are left untouched.
fn centaur_to_fsf(fen: &str) -> String {
    let map = |c: char| match c {
        'w' => 'c',
        'W' => 'C',
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
///   `go perft` counts duck placements differently from mcr, a documented
///   non-bug, so fuzzing it would only re-surface that noise.
/// * **Jieqi** — hidden-information Xiangqi; its FSF cross-check needs a
///   per-position identity reveal (see `jieqi.rs`), not a static dialect rewrite, so
///   it cannot be driven from an arbitrary fuzzed FEN.
/// * **Chu** — 12x12 Chu Shogi is validated against the HaChu reference engine (the
///   `--hachu` mode), not Fairy-Stockfish, so it carries no FSF difffuzz spec.
/// * **Dai** — 15x15 Dai Shogi is likewise HaChu-only (the `--hachu` mode, issue
///   #401); Fairy-Stockfish does not implement it, so it carries no FSF spec.
/// * **Tenjiku** — 16x16 Tenjiku Shogi is likewise HaChu-only (the `--hachu` mode,
///   issue #402); Fairy-Stockfish does not implement it, so it carries no FSF spec.
/// * **Washogi** — 11x11 Wa Shogi is absent from Fairy-Stockfish's shogi family and
///   HaChu's perft is unreliable, so it has no trustworthy oracle and is rules-only
///   (like Alice); it carries no FSF spec.
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
        // Amazon Chess: mcr spells the Amazon `**a`; FSF spells it `a`.
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
        // Centaur Chess (10x8): the Capablanca board with the compounds replaced by
        // two Centaurs (King + Knight). FSF has no built-in `centaur`; it is an INI
        // variant (a `capablanca` descendant with `centaur = c`), so a variants.ini
        // defining it must be loaded. mcr spells the Centaur `w`, FSF `c`.
        id: WideVariantId::Centaur,
        fsf: "centaur",
        needs_ini: true,
        dialect: centaur_to_fsf,
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
        // Coregal chess: standard chess with a royal queen. mcr and FSF spell it
        // with the identical standard-chess letters, so the dialect is identity.
        id: WideVariantId::Coregal,
        fsf: "coregal",
        needs_ini: false,
        dialect: identity,
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
        // Janus: mcr spells the Janus (Bishop + Knight) `a`; FSF spells it `j`.
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
        // Modern chess (9x9): standard chess widened to a 9x9 board with an added
        // Archbishop. mcr spells the archbishop `a`/`A` (its Hawk compound), FSF
        // spells it `m`/`M`; the FSF `modern` built-in needs no variants.ini.
        id: WideVariantId::Modern,
        fsf: "modern",
        needs_ini: false,
        dialect: crate::modern::fen_to_fsf,
    },
    Spec {
        // No-castle chess: standard chess with castling disabled. mcr and FSF spell
        // it with the identical standard-chess letters, so the dialect is identity.
        id: WideVariantId::Nocastle,
        fsf: "nocastle",
        needs_ini: false,
        dialect: identity,
    },
    Spec {
        // Opulent (10x10, Grand geometry): mcr spells the Wizard `**w`, Lion `**y`,
        // augmented Knight `**z`, and Chancellor `e`; FSF spells them `w`/`l`/`n`/`c`.
        id: WideVariantId::Opulent,
        fsf: "opulent",
        needs_ini: false,
        dialect: crate::opulent::fen_to_fsf,
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
        // Pocket Knight: standard chess with one Knight in hand per side. mcr and
        // FSF spell it with the identical standard-chess letters (the Knight banked
        // as `N`/`n` in the `[Nn]` holdings bracket), so the dialect is identity.
        id: WideVariantId::Pocketknight,
        fsf: "pocketknight",
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
        // Ten-Cubed (10x10, Grand geometry): mcr spells the Wizard `**w`, Champion
        // `**x`, and Marshal `e`; FSF spells them `w`/`c`/`m`.
        id: WideVariantId::Tencubed,
        fsf: "tencubed",
        needs_ini: false,
        dialect: crate::tencubed::fen_to_fsf,
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
    // Courier is appended **last**, out of alphabetical order, on purpose: the
    // per-variant fuzz seed is keyed on each spec's *positional index* in this
    // list (`derive_seed(base, idx, game)`), so inserting a new variant mid-list
    // would shift every later variant's index and silently re-roll its random
    // games. Appending here keeps every pre-existing variant's index — and thus
    // its exact game stream and its clean sweep — byte-identical.
    Spec {
        // Courier chess (12x8) — its short-range Alfil / Man / Wazir / Ferz take
        // mcr's `*x`/`*u`/`*j`/`m` tokens, rewritten to FSF's `e`/`m`/`w`/`f` by a
        // single-pass scan. A FSF built-in (needs a `largeboards=yes` build).
        id: WideVariantId::Courier,
        fsf: "courier",
        needs_ini: false,
        dialect: crate::courier::to_fsf_dialect,
    },
    // EuroShogi and Checkshogi are appended (out of alphabetical order) for the
    // same seed-stability reason as Courier above: the per-variant fuzz seed is
    // keyed on each spec's positional index, so a mid-list insert would re-roll
    // every later variant's games. Both are FSF built-ins spelled identically to
    // mcr (EuroShogi's `k g n b r p` + `+`-promoted forms; Checkshogi is Shogi's
    // letters), so each takes the `identity` dialect. FSF fills Checkshogi's absent
    // `1+1` check-counter field with its default, so mcr's counter-free FEN parses.
    Spec {
        id: WideVariantId::EuroShogi,
        fsf: "euroshogi",
        needs_ini: false,
        dialect: identity,
    },
    Spec {
        id: WideVariantId::CheckShogi,
        fsf: "checkshogi",
        needs_ini: false,
        dialect: identity,
    },
    // Karouk and Ai-Wok are appended (out of alphabetical order) for the same
    // seed-stability reason as Courier / EuroShogi / Checkshogi above: the
    // per-variant fuzz seed is keyed on each spec's positional index, so a
    // mid-list insert would re-roll every later variant's games. Both are FSF
    // built-ins. Karouk is Cambodian spelled identically (its absent `1+1`
    // check-counter field is filled with FSF's default, so mcr's counter-free FEN
    // parses); Ai-Wok spells its super-piece `**s` where FSF spells it `a`.
    Spec {
        id: WideVariantId::Karouk,
        fsf: "karouk",
        needs_ini: false,
        dialect: identity,
    },
    Spec {
        id: WideVariantId::Aiwok,
        fsf: "ai-wok",
        needs_ini: false,
        dialect: aiwok_to_fsf,
    },
    // Judkins Shogi is appended (out of alphabetical order) for the same
    // seed-stability reason as Courier / EuroShogi / Checkshogi / Karouk above: the
    // per-variant fuzz seed is keyed on each spec's positional index, so a mid-list
    // insert would re-roll every later variant's games. A FSF built-in (needs a
    // `largeboards=yes` build) spelled identically to mcr (`n s g k r b p` plus the
    // `+`-promoted forms), so it takes the `identity` dialect.
    Spec {
        id: WideVariantId::Judkins,
        fsf: "judkins",
        needs_ini: false,
        dialect: identity,
    },
    // Micro Shogi is appended (out of alphabetical order) for the same
    // seed-stability reason as Judkins above: a mid-list insert would re-roll every
    // later variant's fuzz seed. A FSF built-in (needs a `largeboards=yes` build)
    // spelled identically to mcr (`k b r l p` plus the `+`-promoted forms), so it
    // takes the `identity` dialect.
    Spec {
        id: WideVariantId::Micro,
        fsf: "micro",
        needs_ini: false,
        dialect: identity,
    },
    // Extinction chess is appended (out of alphabetical order) for the same
    // seed-stability reason as Micro above: a mid-list insert would re-roll every
    // later variant's fuzz seed. A FSF built-in spelled identically to mcr
    // (standard-chess letters — the king is a non-royal Commoner spelled `k`, a
    // *rule* difference, not a letter one), so it takes the `identity` dialect.
    Spec {
        id: WideVariantId::Extinction,
        fsf: "extinction",
        needs_ini: false,
        dialect: identity,
    },
    // Three kings chess is appended (out of alphabetical order) for the same
    // seed-stability reason: a mid-list insert would re-roll every later variant's
    // fuzz seed. A FSF built-in spelled identically to mcr (standard-chess letters —
    // each king is a non-royal Commoner spelled `k`, a *rule* difference, not a
    // letter one), so it takes the `identity` dialect.
    Spec {
        id: WideVariantId::Threekings,
        fsf: "threekings",
        needs_ini: false,
        dialect: identity,
    },
    // Kinglet chess is appended (out of alphabetical order) for the same
    // seed-stability reason as Extinction above: a mid-list insert would re-roll
    // every later variant's fuzz seed. A FSF built-in spelled identically to mcr
    // (standard-chess letters — the king is a non-royal Commoner spelled `k`, a
    // *rule* difference, not a letter one), so it takes the `identity` dialect.
    Spec {
        id: WideVariantId::Kinglet,
        fsf: "kinglet",
        needs_ini: false,
        dialect: identity,
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
/// * **Spartan** — a real mcr bug (a White Persian pawn promoted to the *Spartan*
///   army set plus an illegal King); fixed in `SpartanRules::promotion_targets`.
/// * **Seirawan / Shouse** — the only residual is an FSF artifact, not an mcr bug:
///   S-Chess shares one FEN field for castling rights and gating-eligible squares,
///   so once a king has moved (losing castling) while a corner rook stays
///   gating-eligible, mcr serializes that surviving gate as a bare corner-file
///   letter `A`/`H`/`a`/`h`; FSF has no encoding for "corner rook still gates but
///   the king has moved" and reads it as a *castling right*, emitting an illegal
///   castle of the already-moved king. That exact node is skipped (see
///   [`is_schess_corner_castle_artifact`]); every other S-Chess node is checked.
/// * **Sho Shogi** (issue #454) — the only residual is an FSF artifact, not an mcr
///   bug: mcr lets a side **promote a Drunk Elephant into a Crown Prince** (a
///   second royal) on a move that FSF's generator confines away — a King already
///   in check escaping by gaining a second royal, or a Drunk Elephant pinned in
///   front of its King stepping off the pin *because the promotion makes the
///   exposure legal*. Both engines agree the resulting two-royal position is not in
///   check (a two-royal side is never in check and may leave either king en prise),
///   so mcr's move set is legitimately larger; FSF simply never generates it.
///   Rather than skip such nodes wholesale (the divergence usually surfaces one ply
///   down, inside `perft(2)`), [`check_node`] discounts exactly those moves via
///   [`fsf_visible_count`] so the cross-check stays faithful while still
///   catching any *other* Sho Shogi movegen difference.
///
/// Currently held back (an FSF oracle limitation, not an mcr bug):
///
/// * **Janggi** (issue #442) — FSF over-generates the **pass** (the general staying
///   put, `from == to`) under an in-check position, an illegal move (a pass cannot
///   resolve check) that mcr correctly rejects (`position.rs` gates the pass behind
///   `!in_check`). Unlike the reconstructable artifacts — clean per-move rules
///   discounted by [`fsf_visible_count`] — FSF's Janggi in-check pass is **not
///   uniform**: it appears under some piece checks (e.g. a Horse check, and every
///   checkmate, where FSF then reports the pass as the sole "legal" move) but not
///   others (e.g. a Chariot check, where FSF omits it and agrees with mcr), so no
///   fixed mcr-side discount reconstructs FSF's count. It also cannot be keyed on
///   [`is_in_check`](AnyWideVariant::is_in_check): both generals share the central
///   file and face each other constantly, and mcr (correctly) treats a bikjang
///   facing as a check, so `is_in_check` fires on ~60% of nodes while FSF's
///   `Checkers` (piece-only) does not. The false divergences scale with fuzz depth
///   (in-check nodes become common past ~80 plies), so Janggi is held back from the
///   deep default sweep and validated instead by the fixed-corpus differential
///   (`compare-fairy`, whose curated positions avoid the artifact) and the exact
///   node counts in `tests/perft_janggi.rs`. Still reachable with `--variant janggi`.
///
/// Resolved and released back into the default sweep:
///
/// * **Sittuyin** (issue #460) — FSF *suppresses* a legal Pawn → Met (Ferz)
///   promotion whenever it gives check (`Position::legal` in position.cpp marks any
///   `sittuyin_promotion()` promotion with `gives_check` illegal — discovered *or*
///   direct), a move mcr correctly generates; so mcr's move set is legitimately
///   larger. Confirmed mcr-correct in the #422 bug-hunt. Reconstructed mcr-side by
///   [`fsf_drops_sittuyin_promotion`] (a promotion that leaves the enemy king in
///   check), so the discount is inert on every other node; clean over the deep
///   sweep. Its exact node counts are also pinned by `tests/perft_sittuyin.rs`.
///
/// * **Synochess** (issue #460) — FSF *drops* a legal castle whose rook gives
///   check: with `flyingGeneral = true`, FSF's castling legality (position.cpp)
///   rejects a castle when any king-transit square — including the rook's landing
///   square — faces the enemy king down an open file/rank. E.g. White's queenside
///   `e1c1` lands the a1 rook on d1, checking the black king down an open d-file;
///   FSF omits it, the sole root divergence, while mcr correctly generates it.
///   Confirmed mcr-correct. Reconstructed mcr-side by [`fsf_drops_castle`] (the
///   castled rook's landing square is among the enemy king's checkers); inert on
///   every non-checking castle. Clean over the deep sweep; `tests/perft_synochess.rs`
///   pins its node counts.
///
/// * **Empire** (issue #460) — the same `flyingGeneral` rook-check castle drop as
///   Synochess, seen on the **kingside**: Black's `e8g8` lands the h8 rook on f8,
///   checking the white king down an open f-file, which FSF omits while mcr
///   generates it. Reconstructed by the same [`fsf_drops_castle`] discount (which
///   covers either wing). Note this is distinct from the *queenside* artifact still
///   handled by the [`is_empire_no_queenside_castle_artifact`] node skip: FSF drops
///   Black's queenside castle unconditionally (its rook-piece auto-detection, not a
///   check), so that node is skipped rather than count-reconstructed. Clean over the
///   deep sweep; `tests/perft_empire.rs` pins its node counts.
///
/// * **Tori** (Tori Shogi, issue #416) — a latent **mcr** bug surfaced by the
///   deeper sweep (`--difffuzz --variant tori --seed 1 --games 8 --plies 60`,
///   issue #394): a Pheasant pinned to its own king along a file was wrongly
///   allowed to make its two-square forward *jump*, vacating the shielding square
///   and leaving the king in check (FSF then has a king-capture in the child). mcr
///   generated the illegal `f5f3`; the correct `perft(1)` is 34, not 35. A sibling
///   of the already-fixed "Tori Pheasant drop-interposition" — a *jumping* leaper
///   can leap past the pinning slider (or its own king) off the king-to-pinner
///   segment. Fixed in mcr by confining a pinned Tori piece to that segment
///   (`ToriRules::confine_pins_to_segment`, the same hook Courier uses), which is
///   byte-identical for Tori's sliders and every other variant. Repro FEN (Black
///   to move):
///   `*G*z1*y1*y*v/3*a1k1/*k*K*y2*z*Y/1*y*y*y*k*R*Y/1*Y1*Y*Y2/*V2*YK*Y*R/1*Z1*A*K*Z1[*Y*y] b - - 2 28`.
///   Clean over seeds 1-3 (8 games × 80 plies, 0 divergences).
///
/// * **Shako** (issue #335) — FSF forbids castling the king across a square a
///   **cannon** attacks over a screen, but mcr's castling king-walk danger map
///   (built by *forward*-projecting each enemy piece) missed a cannon's
///   over-screen capture on the *empty* transit square the king walks onto. The
///   fix re-tests each transit square with the king placed on it (see
///   `GenericPosition::gen_castles`), so the cannon's forward projection lands on
///   it. Repro: `c1q1ck4/vrn6v/2pp2p1r1/4p1n2p/pp3p1ppb/1Q2PP4/PN1P3P2/
///   1PP1N1P1Pb/1R3KB2R/1V1CB3VC b Q - 1 17`, then `e10e5` — mcr's `f2d2` is now
///   correctly illegal (the e2 transit square is hit by the e5 cannon over the e3
///   screen). Now clean under the differential fuzzer.
///
/// * **Synochess** (issue #337) — its "deeper-sweep `z`-piece divergence" was the
///   SAME cannon-aware castling-king-walk bug (Synochess reuses the Janggi cannon):
///   the divergent nodes were castles of a king walking across a square a cannon
///   attacks over a screen. The Shako fix above (in `GenericPosition::gen_castles`,
///   gated by `has_cannons`/`has_flying_general`) resolves it too — synochess is
///   clean over deep seeded sweeps (seed 7+, 8 games × 80 plies, 0 divergences).
const HELD_BACK: &[WideVariantId] = &[WideVariantId::Janggi];

/// Tunables for a fuzz run (parsed from the CLI in `main.rs`).
pub struct Config {
    /// The base PRNG seed (default fixed; overridable with `--seed`).
    pub seed: u64,
    /// Random games per variant.
    pub games: u32,
    /// Maximum plies per game (a game also stops early at a terminal node).
    pub plies: u32,
    /// Restrict to a single mcr variant name, or `None` for all.
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
    mcr_p1: u64,
    fsf_p1: u64,
    mcr_p2: u64,
    fsf_p2: u64,
    mcr_divide: Vec<(String, u64)>,
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

/// Resolve the FSF `variants.ini`: `$MCR_FSF_VARIANTS_INI`, then a sibling
/// `variants.ini` beside the FSF binary (the upstream `…/src/stockfish` +
/// `…/src/variants.ini` layout).
fn resolve_variants_ini(fsf_bin: &str) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MCR_FSF_VARIANTS_INI") {
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
/// castling) a corner rook that has *not* moved is still gating-eligible. mcr
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

/// Whether an Empire node is the FSF *no-black-queenside-castle* artifact, which
/// cannot be faithfully cross-checked, so the fuzzer skips it (issue #394).
///
/// In Empire, White fields the Empire army (`TECDKCET`, no Rook pieces) and Black
/// plays the standard chess army, which castles normally — both sides. mcr
/// generates Black's queenside castle (`e8c8`) whenever it is legal. FSF does
/// **not**: it derives castling files from the start position, and because the
/// opposing (White) army carries no `castlingRookPieces` on the queenside, FSF's
/// auto-detection drops Black's queenside castle entirely — Black kingside
/// (`e8g8`) is emitted, queenside never is. This is a long-standing FSF property,
/// not upstream drift: a from-source 2023-01 FSF build refuses `e8c8` in Empire
/// exactly like the pinned 2026 build, and it is unreachable from the pinned
/// perft corpus (whose Empire positions never offer Black the queenside castle).
/// mcr is correct per Empire's rules; FSF simply cannot represent the move, so the
/// two engines are not mutually checkable at such a node — the same targeted skip
/// the S-Chess corner-castle artifact uses. Every other Empire node is checked.
///
/// The signal is a persistent *state*, not a single move: whenever Black's
/// queenside castle is geometrically set up (king on e8, a-rook on a8, the `q`
/// right present, and b8/c8/d8 empty) the extra mcr move both surfaces directly
/// (as Black's `perft(1)`) and leaks into the parent White node's `perft(2)`, so
/// firing on the state skips both. The configuration is specific enough that it
/// masks no other Empire movegen.
fn is_empire_no_queenside_castle_artifact(spec: &Spec, fen: &str) -> bool {
    if spec.id != WideVariantId::Empire {
        return false;
    }
    let mut fields = fen.split(' ');
    let board = fields.next().unwrap_or("");
    // fields now yields side-to-move, then the castling/gating field.
    let castling = fields.nth(1).unwrap_or("");
    if !castling.contains('q') {
        return false;
    }
    // Expand rank 8 (the first `/`-group) into one slot per file; a `*`-prefixed
    // overflow piece occupies a single square (its base letter follows).
    let rank8 = board.split('/').next().unwrap_or("");
    let mut files: Vec<Option<char>> = Vec::with_capacity(8);
    let mut chars = rank8.chars();
    while let Some(c) = chars.next() {
        if let Some(n) = c.to_digit(10) {
            for _ in 0..n {
                files.push(None);
            }
        } else if c == '*' {
            files.push(chars.next());
        } else {
            files.push(Some(c));
        }
    }
    // Black queenside castle available: a8 rook, e8 king, b8/c8/d8 empty.
    files.len() >= 5
        && files[0] == Some('r')
        && files[1].is_none()
        && files[2].is_none()
        && files[3].is_none()
        && files[4] == Some('k')
}

/// Whether `uci` — a legal move of `pos` — is a Sho Shogi Drunk-Elephant →
/// Crown-Prince promotion that FSF's move generator omits (issue #454).
///
/// Sho Shogi's Crown Prince is a *second royal*; while a side holds two royals
/// neither is royal, so the side is never in check and may leave *either* king en
/// prise (mcr and FSF **agree** on that rule — a two-royal side with both kings
/// attacked still gets every move). The one place they part is *generation*: a
/// Drunk Elephant that is pinned to, or shielding, its lone King is confined by
/// FSF (as any single-royal pinned/blocking piece would be), so FSF never emits
/// the move that steps it off that line — even though, *because that move also
/// promotes it to a second royal*, the resulting position is one both engines
/// agree is legal (not in check). mcr generates it; FSF does not.
///
/// Such a move is spelled with the `**c` doubled-overflow promotion token (both
/// colours) and is legal *solely* because of the promotion — detectable as its
/// non-promoting twin (same from/to, without the `**c`) being **illegal**, i.e.
/// absent from the legal set (moving the Drunk Elephant there without promoting
/// would leave the single King attacked). A promotion whose non-promoting twin is
/// *legal* is an ordinary move FSF emits too, so it is **not** omitted. This holds
/// whether or not the side is currently in check: the same second-royal escape
/// covers a King already in check and a Drunk Elephant pinned in front of a King
/// that is not yet attacked. `legal_ucis` is the node's full legal-move UCI list.
fn shoshogi_move_is_omitted_escape(uci: &str, legal_ucis: &[String]) -> bool {
    let Some(base) = uci.strip_suffix("**c") else {
        return false;
    };
    !legal_ucis.iter().any(|u| u == base)
}

/// Whether `mv` — a legal Sittuyin move of `pos` — is a Pawn → Met promotion FSF
/// wrongly suppresses because it gives check (issue #460).
///
/// Sittuyin's only promotion is the Pawn → Met (Ferz) in-place/diagonal promotion.
/// FSF marks any such promotion that gives check **illegal** (`Position::legal`,
/// position.cpp: a `sittuyin_promotion()` `PROMOTION` for which `gives_check` holds
/// returns `false`), so it never emits the discovered- or direct-check promotion
/// mcr correctly generates — the divergent nodes show mcr with one more move than
/// FSF. Confirmed mcr-correct in the #422 bug-hunt. Reconstructed mcr-side by
/// playing the promotion and asking whether the enemy king is left in check; inert
/// on every promotion that gives no check.
fn fsf_drops_sittuyin_promotion(pos: &AnyWideVariant, mv: &WideMove) -> bool {
    if mv.promotion().is_none() {
        return false;
    }
    let child = pos.play(mv);
    // After the promotion it is the enemy's turn; a checked enemy means the
    // promotion gave check, so FSF's generator dropped it.
    let enemy = child.turn();
    child.is_in_check(enemy)
}

/// Whether `mv` — a legal castling move of `pos` — is one FSF's `flyingGeneral`
/// castling legality wrongly drops (issue #460): the castled **rook** lands on a
/// square from which it checks the enemy king down an open file/rank.
///
/// Synochess and Empire both set `flyingGeneral = true`. FSF's `Position::legal`
/// CASTLING branch (position.cpp) walks the king's transit squares and, for each,
/// rejects the castle if a rook placed there would face the enemy king — and the
/// rook's landing square is always one of those transit squares (the square the
/// king steps over). So whenever the castled rook gives check, FSF drops the whole
/// castle; mcr correctly generates it (the sole root divergence, e.g. White's
/// synochess `e1c1` giving check down an open d-file, or Black's empire `e8g8`
/// giving check down an open f-file). Reconstructed mcr-side by playing the castle
/// and asking whether the rook's landing square is among the enemy king's checkers
/// — a genuine *rook* check, not an unrelated discovered check (which FSF keeps,
/// as does mcr, so it must not be discounted). Inert on every castle that gives no
/// such rook check.
fn fsf_drops_castle(pos: &AnyWideVariant, mv: &WideMove) -> bool {
    // The rook lands on the square the king steps over: one file toward the king's
    // origin from its destination (the f-file for O-O, the d-file for O-O-O).
    let king_to = mv.to_index();
    let rook_sq = match mv.kind() {
        WideMoveKind::CastleKingside => king_to - 1,
        WideMoveKind::CastleQueenside => king_to + 1,
        _ => return false,
    };
    let child = pos.play(mv);
    let enemy = child.turn();
    child.checkers_of(enemy).contains(&rook_sq)
}

/// Whether the legal move `mv` of `pos` (UCI `uci`, the node's full legal UCI list
/// `legal_ucis`) is one FSF's generator omits for variant `id` — the union of the
/// per-move FSF artifacts the fuzzer reconstructs (issues #454, #460). Returns
/// `false` (inert) for every variant/move without a known artifact, so the
/// cross-check stays byte-identical wherever the engines already agree.
fn fsf_omits_move(
    id: WideVariantId,
    pos: &AnyWideVariant,
    mv: &WideMove,
    uci: &str,
    legal_ucis: &[String],
) -> bool {
    match id {
        WideVariantId::ShoShogi => shoshogi_move_is_omitted_escape(uci, legal_ucis),
        WideVariantId::Sittuyin => fsf_drops_sittuyin_promotion(pos, mv),
        WideVariantId::Synochess | WideVariantId::Empire => fsf_drops_castle(pos, mv),
        _ => false,
    }
}

/// Whether variant `id` carries a per-move FSF artifact the fuzzer reconstructs
/// (see [`fsf_omits_move`]). Gates the (slightly costlier) FSF-visible counting so
/// every other variant keeps the plain `perft(1)` child count.
fn variant_has_fsf_artifact(id: WideVariantId) -> bool {
    matches!(
        id,
        WideVariantId::ShoShogi
            | WideVariantId::Sittuyin
            | WideVariantId::Synochess
            | WideVariantId::Empire
    )
}

/// The count of `pos`'s legal moves **as FSF's generator sees them**: the full
/// legal count minus the moves FSF omits for variant `id` (see [`fsf_omits_move`]).
/// On any node without such a move this equals the plain legal-move count, so it is
/// inert everywhere except the exact FSF-limited nodes. Used to reconstruct a
/// child's `perft(1)` the way FSF counts it, so the artifact does not leak into the
/// parent node's `perft(2)`.
fn fsf_visible_count(id: WideVariantId, pos: &AnyWideVariant) -> u64 {
    let moves = pos.legal_moves();
    let ucis: Vec<String> = moves.iter().map(|mv| pos.to_uci(mv)).collect();
    moves
        .iter()
        .zip(ucis.iter())
        .filter(|(mv, uci)| !fsf_omits_move(id, pos, mv, uci, &ucis))
        .count() as u64
}

/// The count of `pos`'s legal Sho Shogi moves as FSF's generator sees them — the
/// crown-prince special case of [`fsf_visible_count`] (issue #454). Kept as a named
/// entry point for the unit test that pins the discount.
#[cfg(test)]
fn shoshogi_fsf_visible_count(pos: &AnyWideVariant) -> u64 {
    fsf_visible_count(WideVariantId::ShoShogi, pos)
}

/// Cross-check one node: mcr `perft(1)`/`perft(2)` + divide vs FSF's.
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

    // Re-parse the mcr side from the FEN string FSF is fed, so both engines
    // evaluate the *same stateless position*. FSF's `position fen` carries no move
    // history, so comparing it against an in-game mcr position would wrongly flag
    // history-only state that the FEN does not encode (e.g. Janggi's
    // consecutive-pass adjudication) as a movegen divergence. Re-parsing makes the
    // comparison FEN-to-FEN; a failure here would itself be an mcr round-trip bug.
    let node = AnyWideVariant::from_fen(spec.id, &fen)
        .map_err(|e| format!("mcr failed to re-parse its own FEN {fen:?}: {e:?}"))?;

    // ---- mcr side: perft(1) is the legal-move count; perft(2)'s divide is each
    // legal move's child perft(1). --------------------------------------------
    //
    // A few variants carry a documented FSF *movegen* limitation (never an mcr bug):
    // FSF's generator omits a legal move mcr correctly emits, so mcr's move set is
    // legitimately larger. The reconstructed count [`fsf_visible_count`] counts each
    // node the way FSF's generator sees it — discounting exactly those FSF-omitted
    // moves (see [`fsf_omits_move`]) — so the cross-check stays faithful while still
    // catching any *other* movegen difference:
    //
    //   * **Sho Shogi** (#454) — a Drunk-Elephant → Crown-Prince second-royal escape
    //     FSF's pinned/in-check generator never emits.
    //   * **Sittuyin** (#460) — a Pawn → Met promotion FSF drops because it gives
    //     (discovered or direct) check.
    //   * **Synochess / Empire** (#460) — a castle whose rook gives check, which
    //     FSF's `flyingGeneral` castling legality wrongly rejects.
    //
    // The omitted move usually also surfaces one ply down, in a child's move count,
    // so the child perft(1) is likewise counted through [`fsf_visible_count`] and the
    // diverging root moves are dropped here too.
    let discounted = variant_has_fsf_artifact(spec.id);
    let moves = node.legal_moves();
    let root_ucis: Vec<String> = moves.iter().map(|mv| node.to_uci(mv)).collect();
    let mut mcr_p1 = 0u64;
    let mut mcr_divide: Vec<(String, u64)> = Vec::with_capacity(moves.len());
    let mut mcr_p2 = 0u64;
    for (mv, uci) in moves.iter().zip(root_ucis.iter()) {
        if discounted && fsf_omits_move(spec.id, &node, mv, uci, &root_ucis) {
            continue;
        }
        mcr_p1 += 1;
        let child = node.play(mv);
        let child_nodes = if discounted {
            fsf_visible_count(spec.id, &child)
        } else {
            child.perft(1)
        };
        mcr_p2 += child_nodes;
        mcr_divide.push((uci.clone(), child_nodes));
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
    let mut mcr_counts: Vec<u64> = mcr_divide.iter().map(|&(_, n)| n).collect();
    let mut fsf_counts: Vec<u64> = fsf_divide.iter().map(|&(_, n)| n).collect();
    mcr_counts.sort_unstable();
    fsf_counts.sort_unstable();

    if mcr_p1 != fsf_p1 || mcr_p2 != fsf_p2 || mcr_counts != fsf_counts {
        return Ok(Err(Box::new(Divergence {
            fen,
            fsf_fen,
            mcr_p1,
            fsf_p1,
            mcr_p2,
            fsf_p2,
            mcr_divide,
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
    eprintln!("    mcr FEN : {}", d.fen);
    eprintln!("    FSF FEN : {}", d.fsf_fen);
    eprintln!("    perft(1): mcr={} fsf={}", d.mcr_p1, d.fsf_p1);
    eprintln!("    perft(2): mcr={} fsf={}", d.mcr_p2, d.fsf_p2);

    // Localise: group both divides by child count and show where the multisets
    // diverge, plus the raw labelled lists for hand inspection.
    let mut mcr_by_count: BTreeMap<u64, usize> = BTreeMap::new();
    for &(_, n) in &d.mcr_divide {
        *mcr_by_count.entry(n).or_default() += 1;
    }
    let mut fsf_by_count: BTreeMap<u64, usize> = BTreeMap::new();
    for &(_, n) in &d.fsf_divide {
        *fsf_by_count.entry(n).or_default() += 1;
    }
    eprintln!("    divide child-count histogram (count -> #moves):");
    let mut keys: Vec<u64> = mcr_by_count
        .keys()
        .chain(fsf_by_count.keys())
        .copied()
        .collect();
    keys.sort_unstable();
    keys.dedup();
    for k in keys {
        let m = mcr_by_count.get(&k).copied().unwrap_or(0);
        let f = fsf_by_count.get(&k).copied().unwrap_or(0);
        if m != f {
            eprintln!("      {k:>6}: mcr {m}  fsf {f}   <- differs");
        }
    }
    eprintln!(
        "    mcr divide ({} moves): {}",
        d.mcr_divide.len(),
        fmt_divide(&d.mcr_divide)
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

        // A documented FSF artifact (not an mcr bug) is not cross-checkable; skip
        // the node but play on so the random walk still explores past it.
        let fen = pos.to_fen();
        if is_schess_corner_castle_artifact(spec, &fen)
            || is_empire_no_queenside_castle_artifact(spec, &fen)
        {
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
        println!("  no variants.ini found (set $MCR_FSF_VARIANTS_INI); INI variants skipped");
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
                "  HELD {:<14} (held back from the default sweep, see HELD_BACK; --variant to run)",
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

    /// The mcr side of the cross-check is internally consistent for every fuzzable
    /// variant: walking a short seeded random game, `perft(2)` always equals the
    /// sum of each legal move's child `perft(1)` (the divide), and `play` never
    /// panics. This validates the fuzzer's mcr-side arithmetic without FSF, so it
    /// runs in the default `cargo test`.
    #[test]
    fn mcr_side_self_consistent_on_random_games() {
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

    /// Every fuzzable spec names a real, distinct variant, and the five documented
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
            WideVariantId::Chu,
            WideVariantId::Dai,
            WideVariantId::Tenjiku,
            WideVariantId::Washogi,
        ] {
            assert!(
                !ids.contains(&excluded),
                "{} must be excluded",
                excluded.as_str()
            );
        }
        // Every shipped variant minus the 7 by-design exclusions (Alice / Duck /
        // Jieqi, the HaChu-only large-shogi Chu / Dai / Tenjiku, and the oracle-less
        // Wa Shogi, which Fairy-Stockfish does not implement); the deeper-sweep
        // follow-ups stay in SPECS but are skipped via `HELD_BACK` on the default run.
        assert_eq!(SPECS.len(), WideVariantId::ALL.len() - 7);
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

    /// The Empire no-black-queenside-castle skip fires exactly on the state where
    /// Black's queenside castle is set up (king e8, a-rook a8, `q` right, b8/c8/d8
    /// empty) — at both the Black node and its White parent — and nowhere else.
    #[test]
    fn empire_no_queenside_castle_artifact_is_precise() {
        let empire = SPECS
            .iter()
            .find(|s| s.id == WideVariantId::Empire)
            .expect("empire spec");
        let other = SPECS
            .iter()
            .find(|s| s.id == WideVariantId::Capablanca)
            .expect("capablanca spec");

        // The reported divergence node (White to move) and its Black child both
        // carry the state, so both are skipped.
        assert!(is_empire_no_queenside_castle_artifact(
            empire,
            "r3k1*C1/p5p1/bnn*T2*D1/1pb2pq1/1P6/P1P1*E1pP/6*C*T/2*EK4 w q - 1 26",
        ));
        assert!(is_empire_no_queenside_castle_artifact(
            empire,
            "r3k1*C1/p5p1/bnn3*D1/1pb2pq1/1P3*T2/P1P1*E1pP/6*C*T/2*EK4 b q - 2 26",
        ));

        // No `q` right → not the artifact (Black cannot queenside castle).
        assert!(!is_empire_no_queenside_castle_artifact(
            empire,
            "r3k1*C1/p5p1/bnn*T2*D1/1pb2pq1/1P6/P1P1*E1pP/6*C*T/2*EK4 w k - 1 26",
        ));
        // a-rook has left a8 → geometry broken.
        assert!(!is_empire_no_queenside_castle_artifact(
            empire,
            "4k1*C1/p5p1/bnn3*D1/1pb2pq1/1P3*T2/P1P1*E1pP/6*C*T/r1*EK4 b q - 2 26",
        ));
        // A piece sits on d8 → the queenside path is not clear.
        assert!(!is_empire_no_queenside_castle_artifact(
            empire,
            "r2bk1*C1/p5p1/bnn3*D1/1pb2pq1/1P3*T2/P1P1*E1pP/6*C*T/2*EK4 b q - 2 26",
        ));
        // The Empire start array (b8/c8/d8 occupied) is not the artifact.
        assert!(!is_empire_no_queenside_castle_artifact(
            empire,
            "rnbqkbnr/pppppppp/8/8/8/PPPssPPP/8/*T*E*C*DK*C*E*T w kq - 0 1",
        ));
        // Restricted to Empire: another variant with a queenside-castle FEN is not
        // affected.
        assert!(!is_empire_no_queenside_castle_artifact(
            other,
            "r3k3/pppppppp/8/8/8/8/PPPPPPPP/R3K3 b q - 0 1",
        ));
    }

    /// [`shoshogi_fsf_visible_count`] counts a node's moves as FSF's in-check
    /// generator would: every legal move, minus the Drunk-Elephant → Crown-Prince
    /// promotions whose legality rests solely on gaining a second royal (their
    /// non-promoting twin is illegal, so FSF omits them). It equals the full legal
    /// count on any non-diverging node.
    #[test]
    fn shoshogi_fsf_visible_count_discounts_only_crown_prince_escapes() {
        // Issue #454's node: White's lone King (h3) is in check from the Pawn on
        // h4, and the Drunk Elephant on d6 promotes to a Crown Prince (`d6d7**c`).
        // mcr has 4 legal evasions; FSF sees 3.
        let node = AnyWideVariant::from_fen(
            WideVariantId::ShoShogi,
            "7k1/l3**er2l/1p+N1Ng2n/ps1**Esbp2/9/P2PPPPpp/LG4GK1/6S1R/2S5L w - - 0 50",
        )
        .expect("valid Sho Shogi FEN");
        assert_eq!(node.legal_moves().len(), 4);
        assert_eq!(shoshogi_fsf_visible_count(&node), 3);

        // A Drunk Elephant in the promotion zone but NOT in check: its promotions
        // are ordinary moves FSF also generates, so nothing is discounted.
        let de = AnyWideVariant::from_fen(
            WideVariantId::ShoShogi,
            "4k4/9/4**E4/9/9/9/9/9/4K4 w - - 0 1",
        )
        .expect("valid Sho Shogi FEN");
        assert!(!de.is_check());
        assert_eq!(
            shoshogi_fsf_visible_count(&de),
            de.legal_moves().len() as u64
        );

        // An ordinary single-royal check with no Drunk Elephant to promote: no
        // discount, full count.
        let plain =
            AnyWideVariant::from_fen(WideVariantId::ShoShogi, "4k4/9/9/9/9/9/9/9/r3K4 w - - 0 1")
                .expect("valid Sho Shogi FEN");
        assert!(plain.is_check());
        assert_eq!(
            shoshogi_fsf_visible_count(&plain),
            plain.legal_moves().len() as u64
        );

        // A Drunk Elephant (d7) pinned in front of its King (d1) by a Rook (d8),
        // the side NOT in check: mcr lets it step off the pin *only by promoting*
        // (six `**c` moves whose non-promoting twins are illegal); FSF omits all
        // six. The `d7d8`/`d7d8**c` pair that captures the pinner keeps its legal
        // twin, so it is not discounted.
        let pin = AnyWideVariant::from_fen(
            WideVariantId::ShoShogi,
            "l1+Ng1g3/3r1k3/1+S1**E**ep1pl/1pp6/2P2P1np/7G1/PP2+n1N2/L3G3L/3K2S2 w - - 1 46",
        )
        .expect("valid Sho Shogi FEN");
        assert!(!pin.is_check());
        assert_eq!(pin.legal_moves().len(), 43);
        assert_eq!(shoshogi_fsf_visible_count(&pin), 37);
    }

    /// [`fsf_visible_count`] discounts exactly the Sittuyin Pawn → Met promotions
    /// FSF drops for giving check (issue #460), and nothing else.
    #[test]
    fn sittuyin_visible_count_discounts_only_checking_promotions() {
        // A #460 node (child of White `b6c8`): Black's `e4d3m` promotes to a Met and
        // uncovers a discovered check on the White King down the e-file; FSF omits
        // it, so mcr's 33 legal moves count as 32 to FSF.
        let node = AnyWideVariant::from_fen(
            WideVariantId::Sittuyin,
            "r1N1r3/6n1/k4npp/1p1s1P1P/pP1spPP1/P1P5/7N/SRMSK2R[] b - - 0 34",
        )
        .expect("valid Sittuyin FEN");
        assert_eq!(node.legal_moves().len(), 33);
        assert_eq!(fsf_visible_count(WideVariantId::Sittuyin, &node), 32);

        // The placement-phase start position has no promotions at all: nothing is
        // discounted, so the count is the plain legal-move count.
        let start = AnyWideVariant::startpos(WideVariantId::Sittuyin);
        assert_eq!(
            fsf_visible_count(WideVariantId::Sittuyin, &start),
            start.legal_moves().len() as u64
        );
    }

    /// [`fsf_visible_count`] discounts exactly the Synochess / Empire castles whose
    /// rook gives check (the `flyingGeneral` drop, issue #460), and nothing else.
    #[test]
    fn castle_visible_count_discounts_only_checking_castles() {
        // Synochess #460 node: White's queenside `e1c1` lands the a1 rook on d1,
        // checking the Black King (d7) down the open d-file; FSF omits it, so mcr's
        // 40 legal moves count as 39.
        let syno = AnyWideVariant::from_fen(
            WideVariantId::Synochess,
            "1nv*u1vn1/3k4/r5c1/zzz1PzB1/P1P5/NQ6/1P2PPPr/R3KBNR[] w KQ - 1 8",
        )
        .expect("valid Synochess FEN");
        assert_eq!(syno.legal_moves().len(), 40);
        assert_eq!(fsf_visible_count(WideVariantId::Synochess, &syno), 39);

        // Empire #460 node: Black's kingside `e8g8` lands the h8 rook on f8, checking
        // the White King (f2) down the open f-file; FSF omits it, so mcr's 39 legal
        // moves count as 38.
        let empire = AnyWideVariant::from_fen(
            WideVariantId::Empire,
            "1n2k2r/1q1pp1bp/r1b5/p3p2P/P1nZ4/1P1p*D1P1/*E3*CK2/4*E2*T b k - 0 33",
        )
        .expect("valid Empire FEN");
        assert_eq!(empire.legal_moves().len(), 39);
        assert_eq!(fsf_visible_count(WideVariantId::Empire, &empire), 38);

        // Neither start position offers a checking castle, so nothing is discounted.
        for id in [WideVariantId::Synochess, WideVariantId::Empire] {
            let start = AnyWideVariant::startpos(id);
            assert_eq!(
                fsf_visible_count(id, &start),
                start.legal_moves().len() as u64
            );
        }
    }
}
