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
//! * **Dual-form / overflow drops** (issue #452): a hand piece with no bare
//!   letter of its own — a Kyoto / Micro Shogi flipped (promoted) form, or an
//!   overflow role sharing a bare piece's base letter — now carries the same
//!   `+` / `*` / `**` / `***` / `=` disambiguation prefix in its **UCI** drop
//!   token that SAN already spells, so Kyoto's face-up vs. flipped drop render
//!   `P@a1` vs. `+P@a1` (and Xiangfu's cannon `C@a8` vs. the overflow `=C@a8`).
//!   Both were once a single UCI string that `parse_uci` could not tell apart;
//!   the drop token is now injective, so every drop round-trips through UCI just
//!   as it does through SAN. This suite asserts that: drop UCI, like board-move
//!   UCI, must stay injective and lossless.
//!
//! Every check drives only the public `AnyWideVariant` API and plays no role in
//! move generation, so movegen stays byte-identical.

use std::collections::BTreeMap;

use mcr::geometry::{AnyWideVariant, WideMove, WideMoveKind, WidePgn, WideVariantId};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

/// One step of splitmix64 — a tiny dependency-free PRNG, matching the generator
/// the sibling invariant / perft suites use so lines reproduce identically.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Asserts, at one position, that every legal move round-trips through SAN
/// (`move -> san -> move`, required of **every** variant), and audits UCI:
///
/// UCI must be **injective and lossless over every legal move** — board moves and
/// drops alike. Board moves stay distinct via their squares (including Chu Shogi
/// Lion multi-step moves, whose intermediate square keeps two area paths to one
/// destination apart); drops stay distinct via the role token, which since #452
/// carries the same `+` / `*` / `**` / `***` / `=` prefix SAN uses for a promoted
/// or overflow dropped role (so a Kyoto flipped drop `+P@a1` no longer collides
/// with the face-up `P@a1`). Any shared UCI between two distinct legal moves — of
/// any kind — is a hard failure.
fn assert_node_roundtrips(pos: &AnyWideVariant) {
    let moves = pos.legal_moves();
    let mut by_uci: BTreeMap<String, Vec<WideMove>> = BTreeMap::new();
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
        by_uci.entry(pos.to_uci(mv)).or_default().push(*mv);
    }
    for (uci, group) in &by_uci {
        // Every UCI string — board move or drop — must belong to exactly one legal
        // move and resolve back to it.
        assert_eq!(
            group.len(),
            1,
            "distinct legal moves share UCI {uci:?} in {} at {} — UCI must stay injective: {:?}",
            pos.variant_id(),
            pos.to_fen(),
            group.iter().map(|m| pos.san(m)).collect::<Vec<_>>(),
        );
        assert_eq!(
            pos.parse_uci(uci),
            Some(group[0]),
            "UCI {uci:?} did not round-trip in {} at {}",
            pos.variant_id(),
            pos.to_fen(),
        );
    }
}

/// Plays up to `plies` seeded-random legal moves from `id`'s start position,
/// asserting the per-node round-trips at every step, and returns the move list
/// played (a legal game, for the PGN round-trip).
fn walk_and_check(id: WideVariantId, seed: u64, plies: u32) -> Vec<WideMove> {
    let mut state = seed;
    let mut pos = AnyWideVariant::startpos(id);
    let mut played = Vec::new();
    for _ in 0..plies {
        assert_node_roundtrips(&pos);
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
            // The whole node round-trips (SAN + UCI): Chu's board-move UCI is
            // injective now that Lion moves spell their intermediate square.
            assert_node_roundtrips(&pos);

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
    assert!(
        saw_igui,
        "expected to observe an igui (stationary capture) Lion move"
    );
    assert!(saw_double, "expected to observe a double-capture Lion move");
    assert!(
        saw_two_step_capture,
        "expected to observe a two-step area-capture Lion move (the case UCI once could not disambiguate)"
    );
    assert!(saw_pass, "expected to observe a jitto pass Lion move");
}

/// Kyoto Shogi two-form drops round-trip through UCI (issue #452). A held piece
/// dropped face-up vs. flipped (promoted) onto the same square is two distinct
/// legal moves; their UCI drop tokens now differ by the `+` promoted prefix
/// (`P@a1` vs. `+P@a1`), so each renders distinctly and `parse_uci` resolves each
/// back to its own move — no collision, and SAN stays distinct too.
#[test]
fn kyoto_two_form_drops_are_uci_distinct_and_round_trip() {
    // A mid-game Kyoto Shogi position with pieces in hand, so a two-form drop is
    // available. Reach it by seeded self-play until a dual-form drop is legal.
    let mut state = 0xC0FF_EE12_3456_789Au64;
    let mut pos = AnyWideVariant::startpos(WideVariantId::Kyotoshogi);
    let mut saw_two_form = false;

    for _ in 0..120 {
        let moves = pos.legal_moves();

        // Every distinct drop must render to a *distinct* UCI string.
        let mut drop_ucis: BTreeMap<String, WideMove> = BTreeMap::new();
        // Whether a base and a promoted drop of the same held piece both exist —
        // the dual-form case this fix targets.
        let mut base_drops = 0usize;
        let mut promoted_drops = 0usize;

        for mv in &moves {
            let san = pos.san(mv);
            assert_eq!(
                pos.parse_san(&san),
                Some(*mv),
                "Kyoto SAN {san:?} did not round-trip to itself at {}",
                pos.to_fen(),
            );
            if mv.is_drop() {
                let uci = pos.to_uci(mv);
                // No two distinct drops share a UCI string.
                if let Some(prev) = drop_ucis.insert(uci.clone(), *mv) {
                    assert_eq!(
                        prev,
                        *mv,
                        "two distinct Kyoto drops share UCI {uci:?} at {}",
                        pos.to_fen(),
                    );
                }
                // Each drop round-trips through UCI back to itself.
                assert_eq!(
                    pos.parse_uci(&uci),
                    Some(*mv),
                    "Kyoto drop UCI {uci:?} did not round-trip at {}",
                    pos.to_fen(),
                );
                match mv.drop_role() {
                    Some(r) if r.is_promoted() => promoted_drops += 1,
                    Some(_) => base_drops += 1,
                    None => {}
                }
            }
        }
        if base_drops > 0 && promoted_drops > 0 {
            saw_two_form = true;
            break;
        }
        if moves.is_empty() {
            break;
        }
        let pick = (splitmix64(&mut state) as usize) % moves.len();
        pos = pos.play(&moves[pick]);
    }

    assert!(
        saw_two_form,
        "expected to reach a Kyoto position offering both base and promoted drops",
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

/// SAN + UCI round-trip and a whole-game PGN round-trip across every variant, via
/// the runtime-dispatch [`AnyWideVariant`] surface. Every legal move at every
/// walked node round-trips through both SAN and UCI (all variants — Kyoto /
/// Micro Shogi's dual-form drops included, since #452), and the walked game
/// survives a PGN export/import.
///
/// The generator draws `160` `(variant, seed, plies)` cases — enough to cover
/// every variant broadly while staying CI-fast — but the proptest RNG is **pinned**
/// to a fixed seed (`TestRunner::new_with_rng`), so the exact same cases run every
/// invocation. This determinism is deliberate: with an entropy-seeded RNG the walk
/// only *intermittently* reaches the Ordamirror pawn→overflow-Falcon promotion
/// that surfaced #432, which made `cargo test --all-features` flaky on main and
/// unusable as a cargo-mutants baseline (#407). A fixed seed makes any failure
/// reproducible and turns this test into a stable regression guard.
#[test]
fn any_variant_notation_round_trips() {
    let config = Config {
        cases: 160,
        // Don't let an on-disk regression file (or its absence) perturb the case
        // set: this test is fully seeded, so its coverage is self-contained.
        failure_persistence: None,
        ..Config::default()
    };
    // A fixed 32-byte ChaCha seed pins the whole case sequence.
    let rng = TestRng::from_seed(RngAlgorithm::ChaCha, &[0x42; 32]);
    let mut runner = TestRunner::new_with_rng(config, rng);
    runner
        .run(&walk_inputs(), |(id, seed, plies)| {
            let moves = walk_and_check(id, seed, plies);
            assert_pgn_roundtrip(id, &moves);
            Ok(())
        })
        .unwrap();
}
