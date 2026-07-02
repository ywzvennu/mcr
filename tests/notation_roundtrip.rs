//! Cross-variant notation round-trip suite (issue #410).
//!
//! The sibling `invariants.rs` (#369) already checks that every legal move
//! renders to a SAN string that parses back to itself. This suite hardens the
//! **whole** notation surface — SAN, UCI, and whole-game PGN — across **every**
//! registered variant (the full [`WideVariantId::ALL`] table, 60 variants), and
//! pins the two hard families the audit turned up:
//!
//! * **Chu Shogi Lion multi-step moves** ([`WideMoveKind::LionMove`]): the
//!   *igui* stationary capture, the double capture, the two-step area capture,
//!   and the *jitto* pass. Each renders to SAN spelling **both legs** (with the
//!   intermediate square) and to a UCI that appends that intermediate square, so
//!   both notations are injective over the legal-move list and round-trip to the
//!   exact same [`WideMove`] — including two area paths that reach the same
//!   destination through different elbow squares.
//!
//! * **Kyoto Shogi two-form drops** (documented notation limit): a piece dropped
//!   face-up vs. flipped is two genuinely distinct legal moves that share one
//!   UCI string (e.g. `P@a1`), because UCI's drop syntax carries only the base
//!   role letter. SAN disambiguates them with the `+` promoted prefix
//!   (`P@a1` vs. `+P@a1`), so the SAN round-trip is lossless where the UCI one
//!   cannot be. This suite asserts that limit explicitly rather than papering
//!   over it.
//!
//! Every check drives only the public `AnyWideVariant` API and plays no role in
//! move generation, so movegen stays byte-identical.

use std::collections::BTreeMap;

use mce::geometry::{AnyWideVariant, WideMove, WideMoveKind, WidePgn, WideVariantId};
use proptest::prelude::*;

/// One step of splitmix64 — a tiny dependency-free PRNG, matching the generator
/// the sibling invariant / perft suites use so lines reproduce identically.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Asserts, at one position, that every legal move round-trips through **both**
/// SAN (`move -> san -> move`) and UCI (`move -> uci -> move`), and that UCI is
/// injective over the legal-move list (no two distinct legal moves share a UCI
/// string — the precondition for a lossless `parse_uci`).
///
/// `uci_lossless` is `false` for variants with a documented UCI ambiguity (Kyoto
/// Shogi's two-form drops); there the UCI checks are skipped, but SAN — which
/// disambiguates — is still required to round-trip.
fn assert_node_roundtrips(pos: &AnyWideVariant, uci_lossless: bool) {
    let moves = pos.legal_moves();
    let mut uci_seen: BTreeMap<String, WideMove> = BTreeMap::new();
    for mv in &moves {
        // SAN round-trip: always required, every variant.
        let san = pos.san(mv);
        assert_eq!(
            pos.parse_san(&san).as_ref(),
            Some(mv),
            "SAN {san:?} (uci {}) did not round-trip in {} at {}",
            pos.to_uci(mv),
            pos.variant_id(),
            pos.to_fen(),
        );
        if uci_lossless {
            // UCI injectivity: two distinct legal moves must not collide.
            let uci = pos.to_uci(mv);
            if let Some(prev) = uci_seen.insert(uci.clone(), *mv) {
                assert_eq!(
                    prev,
                    *mv,
                    "two distinct legal moves share UCI {uci:?} in {} at {}",
                    pos.variant_id(),
                    pos.to_fen(),
                );
            }
            // UCI round-trip.
            assert_eq!(
                pos.parse_uci(&uci),
                Some(*mv),
                "UCI {uci:?} did not round-trip in {} at {}",
                pos.variant_id(),
                pos.to_fen(),
            );
        }
    }
}

/// Whether a variant's UCI is lossless over its legal moves. Kyoto Shogi alone
/// has a genuine UCI-drop ambiguity (documented above); every other variant —
/// including Chu Shogi, whose Lion moves now spell their intermediate square — is
/// lossless.
fn uci_lossless(id: WideVariantId) -> bool {
    id != WideVariantId::Kyotoshogi
}

/// Plays up to `plies` seeded-random legal moves from `id`'s start position,
/// asserting the per-node round-trips at every step, and returns the move list
/// played (a legal game, for the PGN round-trip).
fn walk_and_check(id: WideVariantId, seed: u64, plies: u32) -> Vec<WideMove> {
    let lossless = uci_lossless(id);
    let mut state = seed;
    let mut pos = AnyWideVariant::startpos(id);
    let mut played = Vec::new();
    for _ in 0..plies {
        assert_node_roundtrips(&pos, lossless);
        let moves = pos.legal_moves();
        if moves.is_empty() {
            break;
        }
        let pick = (splitmix64(&mut state) as usize) % moves.len();
        played.push(moves[pick]);
        pos = pos.play(&moves[pick]);
    }
    played
}

/// Asserts a played game round-trips through PGN: export carries the correct
/// `[Variant "..."]` tag, re-import reproduces the exact move list and final
/// position, and a second export is byte-identical (canonical form is stable).
fn assert_pgn_roundtrip(id: WideVariantId, moves: &[WideMove]) {
    let start = AnyWideVariant::startpos(id);
    let pgn = WidePgn::from_game(&start, moves, Vec::new()).expect("walked game is legal");
    let text = pgn.to_pgn();
    assert!(
        text.contains(&format!("[Variant \"{}\"]", id.as_str())),
        "variant tag present for {id}",
    );

    let reparsed = WidePgn::from_pgn(&text).expect("exported PGN re-imports");
    assert_eq!(reparsed.variant(), id, "variant round-trips for {id}");
    assert_eq!(reparsed.moves(), pgn.moves(), "moves round-trip for {id}");
    assert_eq!(
        reparsed.final_position().to_fen(),
        pgn.final_position().to_fen(),
        "final position round-trips for {id}",
    );
    assert_eq!(reparsed.to_pgn(), text, "PGN re-export is stable for {id}");
}

/// A deterministic short game (a handful of seeded-random plies) for every
/// variant, exercised through SAN, UCI, and a whole-game PGN round-trip. Fully
/// reproducible: the seeds are fixed, so any failure reprints from the FEN.
#[test]
fn every_variant_notation_round_trips() {
    for &id in WideVariantId::ALL {
        for &seed in &[0x0000_0000_0000_0001u64, 0xDEAD_BEEF_CAFE_F00D] {
            let moves = walk_and_check(id, seed, 10);
            assert_pgn_roundtrip(id, &moves);
        }
    }
}

/// The heavier reproducible sweep: more seeds, deeper walks. Run with
/// `cargo test --all-features -- --ignored`.
#[test]
#[ignore = "deep notation walk; run with --all-features -- --ignored"]
fn every_variant_notation_round_trips_deep() {
    for &id in WideVariantId::ALL {
        for &seed in &[
            0x0000_0000_0000_0001u64,
            0xDEAD_BEEF_CAFE_F00D,
            0x1234_5678_9ABC_DEF0,
            0x0F0F_0F0F_0F0F_0F0F,
            0xA5A5_5A5A_C3C3_3C3C,
        ] {
            let moves = walk_and_check(id, seed, 40);
            assert_pgn_roundtrip(id, &moves);
        }
    }
}

/// Chu Shogi Lion multi-step moves are the notation surface's hardest case: the
/// only move kind whose origin may equal its destination and the only one that
/// can remove two pieces at once. This walks Chu until it has observed each Lion
/// sub-kind — the *igui* stationary capture, the double capture, a two-step area
/// capture, and the *jitto* pass — asserting along the way that every Lion move
/// round-trips through **both** SAN and UCI to the identical [`WideMove`], and
/// that UCI stays injective over the legal-move list (two elbow paths to one
/// square do not collide).
#[test]
fn chu_lion_moves_round_trip_through_san_and_uci() {
    let mut saw_igui = false;
    let mut saw_double = false;
    let mut saw_two_step_capture = false;
    let mut saw_pass = false;
    let mut lion_moves = 0u64;

    'outer: for &seed in &[1u64, 0xDEAD_BEEF, 0x1234_5678, 0xABCD_1234, 0x99] {
        let mut state = seed;
        let mut pos = AnyWideVariant::startpos(WideVariantId::Chu);
        for _ in 0..48 {
            // The whole node round-trips (SAN + UCI, UCI injective) — Chu's UCI is
            // lossless now that Lion moves spell their intermediate square.
            assert_node_roundtrips(&pos, true);

            for mv in pos.legal_moves() {
                if let WideMoveKind::LionMove {
                    first_capture,
                    second_capture,
                } = mv.kind()
                {
                    lion_moves += 1;
                    let from_eq_to = mv.from_index() == mv.to_index();
                    if from_eq_to && first_capture {
                        saw_igui = true;
                    }
                    if from_eq_to && !first_capture && !second_capture {
                        saw_pass = true;
                    }
                    if first_capture && second_capture {
                        saw_double = true;
                    }
                    if !from_eq_to && !first_capture && second_capture {
                        saw_two_step_capture = true;
                    }
                    // Both notations resolve back to exactly this Lion move.
                    let san = pos.san(&mv);
                    assert_eq!(
                        pos.parse_san(&san),
                        Some(mv),
                        "Lion SAN {san:?} did not round-trip at {}",
                        pos.to_fen(),
                    );
                    let uci = pos.to_uci(&mv);
                    assert_eq!(
                        pos.parse_uci(&uci),
                        Some(mv),
                        "Lion UCI {uci:?} did not round-trip at {}",
                        pos.to_fen(),
                    );
                }
            }

            if saw_igui && saw_double && saw_two_step_capture && saw_pass {
                break 'outer;
            }
            let moves = pos.legal_moves();
            if moves.is_empty() {
                break;
            }
            let pick = (splitmix64(&mut state) as usize) % moves.len();
            pos = pos.play(&moves[pick]);
        }
    }

    assert!(lion_moves > 0, "the Chu walk must exercise Lion moves");
    assert!(saw_igui, "expected to observe an igui (stationary capture) Lion move");
    assert!(saw_double, "expected to observe a double-capture Lion move");
    assert!(
        saw_two_step_capture,
        "expected to observe a two-step area-capture Lion move (the case UCI once could not disambiguate)"
    );
    assert!(saw_pass, "expected to observe a jitto pass Lion move");
}

/// The documented Kyoto Shogi UCI-drop limit. A piece dropped face-up vs. flipped
/// is two distinct legal moves that share one UCI drop string, so UCI cannot tell
/// them apart — but SAN spells the flipped form with a `+` prefix, so both SAN
/// strings are distinct and each round-trips to its own move. This asserts the
/// ambiguity exists in UCI *and* that SAN resolves it (never `Ambiguous`).
#[test]
fn kyoto_two_form_drops_are_uci_ambiguous_but_san_distinct() {
    // A mid-game Kyoto Shogi position with pieces in hand, so a two-form drop is
    // available. Reach it by seeded self-play until some drop is legal.
    let mut state = 0xC0FF_EE12_3456_789Au64;
    let mut pos = AnyWideVariant::startpos(WideVariantId::Kyotoshogi);
    let mut found_collision = false;

    for _ in 0..80 {
        let moves = pos.legal_moves();
        // Group drops by their UCI string; a collision is two distinct drops that
        // share one UCI. Assert each drop's SAN round-trips regardless.
        let mut by_uci: BTreeMap<String, Vec<WideMove>> = BTreeMap::new();
        for mv in &moves {
            let san = pos.san(mv);
            assert_ne!(
                pos.parse_san(&san),
                None,
                "every Kyoto move must round-trip through SAN: {san:?} at {}",
                pos.to_fen(),
            );
            assert_eq!(
                pos.parse_san(&san),
                Some(*mv),
                "Kyoto SAN {san:?} did not round-trip to itself at {}",
                pos.to_fen(),
            );
            if mv.is_drop() {
                by_uci.entry(pos.to_uci(mv)).or_default().push(*mv);
            }
        }
        for (uci, group) in &by_uci {
            if group.len() > 1 {
                // The documented limit: >1 distinct legal drop share this UCI, and
                // parse_uci can only return one of them.
                found_collision = true;
                let resolved = pos.parse_uci(uci).expect("a shared UCI still resolves to one move");
                assert!(
                    group.contains(&resolved),
                    "parse_uci({uci:?}) must return one of the colliding drops",
                );
                // But every colliding drop has a *distinct* SAN that round-trips.
                let sans: std::collections::BTreeSet<String> =
                    group.iter().map(|m| pos.san(m)).collect();
                assert_eq!(
                    sans.len(),
                    group.len(),
                    "SAN must disambiguate the two-form Kyoto drops sharing UCI {uci:?}: {sans:?}",
                );
            }
        }
        if found_collision {
            break;
        }
        if moves.is_empty() {
            break;
        }
        let pick = (splitmix64(&mut state) as usize) % moves.len();
        pos = pos.play(&moves[pick]);
    }

    assert!(
        found_collision,
        "expected to reach a Kyoto position with two-form drop UCI collision",
    );
}

/// A `(WideVariantId, seed, plies)` strategy: the inputs to a seeded random walk
/// across every registered variant. Short plies keep shrinking fast.
fn walk_inputs() -> impl Strategy<Value = (WideVariantId, u64, u32)> {
    (
        proptest::sample::select(WideVariantId::ALL),
        any::<u64>(),
        0u32..24,
    )
}

proptest! {
    // Each case walks many nodes across a randomly chosen variant, so a modest
    // case count still covers all 60 variants broadly while staying CI-fast.
    #![proptest_config(ProptestConfig::with_cases(160))]

    /// SAN + UCI round-trip and a whole-game PGN round-trip across every variant,
    /// via the runtime-dispatch [`AnyWideVariant`] surface. Every legal move at
    /// every walked node round-trips through SAN (all variants) and UCI (all but
    /// Kyoto's documented drop ambiguity), and the walked game survives a PGN
    /// export/import.
    #[test]
    fn any_variant_notation_round_trips((id, seed, plies) in walk_inputs()) {
        let moves = walk_and_check(id, seed, plies);
        assert_pgn_roundtrip(id, &moves);
    }
}
