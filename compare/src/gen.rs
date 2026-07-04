//! Seeded per-variant position generator (the "many many many" baskets).
//!
//! For each variant we play a batch of fully *seeded* random legal games with
//! mcr — at every ply we pick a uniformly random legal move from the seeded PRNG
//! — and snapshot the FEN at a spread of plies (opening → middlegame → endgame).
//! This yields ~50–100 positions per variant with zero hand-curation, spanning
//! natural variant scenarios (atomic explosions, crazyhouse pockets, antichess
//! forced-capture chains, near-terminal races, …).
//!
//! Everything is driven by [`SplitMix64`] from a fixed seed, so the generated
//! basket is byte-for-byte stable across runs and machines — the numbers stay
//! comparable over time, which is the whole point.
//!
//! ## Keeping positions shakmaty-comparable
//!
//! The parity cross-check requires that *both* engines accept and agree on each
//! generated position. shakmaty diverges from mcr in a few documented spots, so
//! the generator stays inside the comparable envelope:
//!
//! * **Variant-terminal positions** (a king on the hill, the third check, a
//!   completed king race, an antichess side with no pieces, an atomic king blown
//!   up, …): shakmaty stops expanding such a line while mcr keeps counting. We
//!   therefore stop a game as soon as mcr reports an [`Outcome`], and we never
//!   snapshot a terminal position — only positions with the game still live.
//! * **Crazyhouse over-material pockets**: shakmaty rejects pockets that exceed
//!   the standard material count. Random play *from the start position* can never
//!   manufacture extra material, so generated crazyhouse positions are always
//!   within bounds; we additionally re-parse every snapshot through shakmaty (in
//!   the caller) and skip — with a counted note — any rare position shakmaty
//!   rejects, so a surprise is recorded, never silently dropped.
//!
//! Chess960 games start from a seeded Scharnagl start position (sampled across
//! the 960 ids) so the basket exercises many back-rank arrangements, not one.

use mcr::{AnyVariant, VariantId};

use crate::chess960::scharnagl_start_fen;
use crate::prng::SplitMix64;

/// One generated position: variant key, a label, the FEN, and a tag describing
/// where in the game it was taken (for human-readable output).
#[derive(Clone, Debug)]
pub struct GenPos {
    /// Variant key (matches [`crate::VARIANTS`]).
    pub variant: &'static str,
    /// Human label, e.g. `"g3-ply11"` (game 3, snapshot at ply 11).
    pub label: String,
    /// The position FEN.
    pub fen: String,
    /// Ply at which the snapshot was taken (0 = start position).
    pub ply: u32,
}

/// Map a variant key to the mcr [`VariantId`] used to seed games.
fn variant_id(variant: &str) -> VariantId {
    match variant {
        "standard" => VariantId::Standard,
        "chess960" => VariantId::Chess960,
        "king-of-the-hill" => VariantId::KingOfTheHill,
        "three-check" => VariantId::ThreeCheck,
        "racing-kings" => VariantId::RacingKings,
        "atomic" => VariantId::Atomic,
        "antichess" => VariantId::Antichess,
        "horde" => VariantId::Horde,
        "crazyhouse" => VariantId::Crazyhouse,
        other => panic!("unknown variant {other:?}"),
    }
}

/// A per-variant fixed seed, derived from the variant key so each variant gets
/// an independent (but reproducible) game stream.
fn seed_for(variant: &str) -> u64 {
    // FNV-1a over the key, then a constant salt; pure function of the key.
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &b in variant.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h ^ 0x5EED_B165_1234_0086u64
}

/// The starting [`AnyVariant`] for a game. Standard/variants use the canonical
/// start position; chess960 draws a seeded Scharnagl id so games sample across
/// the 960 back-rank arrangements.
fn start_position(variant: &str, id: VariantId, rng: &mut SplitMix64) -> AnyVariant {
    if variant == "chess960" {
        let sp_id = rng.below(960) as u32;
        let fen = scharnagl_start_fen(sp_id);
        AnyVariant::from_fen(id, &fen).expect("valid Scharnagl start FEN")
    } else {
        AnyVariant::startpos(id)
    }
}

/// Generate the position basket for one variant.
///
/// Plays `games` seeded random games of up to `max_plies` each, snapshotting the
/// FEN whenever the running ply hits a sampling stride that spreads snapshots
/// across the game. Stops a game early at any mcr-reported outcome (never
/// snapshotting a terminal position). De-duplicates identical FENs so the basket
/// is all distinct positions. Returns at most `cap` positions.
pub fn generate_variant(
    variant: &'static str,
    games: u32,
    max_plies: u32,
    cap: usize,
) -> Vec<GenPos> {
    let id = variant_id(variant);
    let mut rng = SplitMix64::new(seed_for(variant));
    let mut out: Vec<GenPos> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for game in 0..games {
        if out.len() >= cap {
            break;
        }
        let mut pos = start_position(variant, id, &mut rng);
        // Snapshot stride: pick a per-game phase offset so snapshots land at
        // varied plies across games (opening in one game, endgame in another),
        // giving a natural opening→endgame spread without hand-tuning.
        let stride = 3 + (rng.below(6) as u32); // 3..=8
        let offset = rng.below(stride as u64) as u32;

        for ply in 0..max_plies {
            // Stop at any terminal position: shakmaty would diverge there, and a
            // terminal position has no further perft anyway.
            if pos.outcome().is_some() {
                break;
            }
            let moves = pos.legal_moves();
            if moves.is_empty() {
                break;
            }

            // Snapshot at the seeded stride (but not the bare start position of
            // every game — ply 0 of game 0 only, to include one true startpos).
            let take = ply % stride == offset && (ply > 0 || game == 0);
            if take {
                let fen = pos.to_fen();
                if seen.insert(fen.clone()) {
                    out.push(GenPos {
                        variant,
                        label: format!("g{game}-ply{ply}"),
                        fen,
                        ply,
                    });
                    if out.len() >= cap {
                        break;
                    }
                }
            }

            let mv = &moves[rng.index(moves.len())];
            pos = pos.play(mv);
        }
    }

    out
}
