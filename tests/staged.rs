//! Staged (lazy) move-generation tests (issue #104).
//!
//! [`Position::staged_moves`] / [`VariantPosition::staged_moves`] yield the legal
//! moves in move-ordering stages — an optional priority (TT/hash) move, then
//! captures ordered by victim value, then quiets — pulling each stage lazily. The
//! load-bearing contract is that the moves yielded across all stages are *exactly*
//! `legal_moves()` as a set, for standard chess and every variant, including
//! in-check positions where only evasions are legal. These tests assert that set
//! equality and the stage ordering over a broad population of positions reached
//! by seeded random self-play (the same walk the property suite uses), plus a few
//! hand-picked tactical and in-check positions.

use std::collections::BTreeSet;

use mce::{
    Antichess, AnyVariant, Atomic, Chess, Chess960, Crazyhouse, Horde, KingOfTheHill, Move,
    Position, RacingKings, ThreeCheck, Variant, VariantId, VariantPosition,
};

/// One step of splitmix64 — a tiny deterministic PRNG to pick random legal moves.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

const ALL_IDS: [VariantId; 9] = [
    VariantId::Standard,
    VariantId::Chess960,
    VariantId::KingOfTheHill,
    VariantId::ThreeCheck,
    VariantId::RacingKings,
    VariantId::Horde,
    VariantId::Atomic,
    VariantId::Antichess,
    VariantId::Crazyhouse,
];

/// Plays up to `plies` random legal moves from the start of `id`, returning the
/// reached live position (stopping before any move that ends the game).
fn random_anyvariant(id: VariantId, seed: u64, plies: u32) -> AnyVariant {
    let mut state = seed;
    let mut pos = AnyVariant::startpos(id);
    for _ in 0..plies {
        if pos.outcome().is_some() {
            break;
        }
        let moves = pos.legal_moves();
        if moves.is_empty() {
            break;
        }
        let pick = (splitmix64(&mut state) as usize) % moves.len();
        let next = pos.play(&moves[pick]);
        if next.outcome().is_some() {
            break;
        }
        pos = next;
    }
    pos
}

/// Asserts the captures-before-quiets ordering of a staged sequence: once a quiet
/// move appears, no later move may be a capture. A leading TT move is allowed to
/// be a capture or a quiet anywhere it lands first, so we only check from the
/// point the non-TT body begins; checking the whole sequence is still correct
/// when the TT move is a capture (it sits in the capture prefix) — the only
/// position a TT quiet could violate is the very first slot, which we skip.
fn assert_captures_before_quiets(staged: &[Move], skip_first: bool) {
    let start = usize::from(skip_first);
    let mut seen_quiet = false;
    for mv in &staged[start.min(staged.len())..] {
        if mv.is_capture() {
            assert!(
                !seen_quiet,
                "a capture {mv} followed a quiet move in staged order"
            );
        } else {
            seen_quiet = true;
        }
    }
}

/// The full set-equality + ordering contract for one [`AnyVariant`] position.
fn check_anyvariant(pos: &AnyVariant) {
    let legal: BTreeSet<Move> = pos.legal_moves().into_iter().collect();

    // No TT move: the staged set equals the legal set exactly.
    let staged = pos.staged_moves(None);
    let staged_set: BTreeSet<Move> = staged.iter().copied().collect();
    assert_eq!(
        staged_set,
        legal,
        "staged set must equal legal_moves for {} ({})",
        pos.variant_id(),
        pos.to_fen()
    );
    assert_eq!(
        staged.len(),
        legal.len(),
        "staged must have no duplicates for {} ({})",
        pos.variant_id(),
        pos.to_fen()
    );
    assert_captures_before_quiets(&staged, false);

    // With a legal TT move supplied: it must be yielded first, exactly once, and
    // the overall set is unchanged.
    if let Some(&tt) = legal.iter().next() {
        let with_tt = pos.staged_moves(Some(tt));
        assert_eq!(with_tt.first(), Some(&tt), "TT move must be yielded first");
        let with_tt_set: BTreeSet<Move> = with_tt.iter().copied().collect();
        assert_eq!(
            with_tt_set, legal,
            "staged-with-TT set must still equal legal_moves"
        );
        assert_eq!(
            with_tt.len(),
            legal.len(),
            "TT move must not be duplicated in staged output"
        );
        assert_captures_before_quiets(&with_tt, true);
    }

    // An illegal/stale TT move is ignored: the set and length are untouched.
    let bogus = Move::new(mce::Square::A1, mce::Square::A2, mce::MoveKind::Quiet);
    if !legal.contains(&bogus) {
        let with_bogus = pos.staged_moves(Some(bogus));
        let with_bogus_set: BTreeSet<Move> = with_bogus.iter().copied().collect();
        assert_eq!(
            with_bogus_set, legal,
            "an illegal TT move must be ignored, leaving the set unchanged"
        );
        assert_eq!(with_bogus.len(), legal.len());
    }
}

/// The same contract on a concrete generic [`VariantPosition`], exercising the
/// typed (lazy iterator) `staged_moves` rather than the `AnyVariant` Vec surface.
fn check_typed<V: Variant>(pos: &VariantPosition<V>) {
    let legal: BTreeSet<Move> = pos.legal_moves().into_iter().collect();

    let staged: Vec<Move> = pos.staged_moves(None).collect();
    let staged_set: BTreeSet<Move> = staged.iter().copied().collect();
    assert_eq!(staged_set, legal, "typed staged set must equal legal_moves");
    assert_eq!(staged.len(), legal.len(), "typed staged has no duplicates");
    assert_captures_before_quiets(&staged, false);

    if let Some(&tt) = legal.iter().next() {
        let with_tt: Vec<Move> = pos.staged_moves(Some(tt)).collect();
        assert_eq!(with_tt.first(), Some(&tt), "typed TT move yielded first");
        let s: BTreeSet<Move> = with_tt.iter().copied().collect();
        assert_eq!(s, legal);
        assert_eq!(with_tt.len(), legal.len());
        assert_captures_before_quiets(&with_tt, true);
    }
}

#[test]
fn staged_set_equals_legal_all_variants_random_walk() {
    // A spread of seeds and depths over every variant: each reached position has
    // its staged output checked for set-equality and ordering against legal_moves.
    for &id in &ALL_IDS {
        for seed in 0..40u64 {
            for plies in [0u32, 1, 3, 7, 15, 25] {
                let pos = random_anyvariant(id, seed.wrapping_mul(0x1234_5678), plies);
                check_anyvariant(&pos);
            }
        }
    }
}

#[test]
fn staged_matches_legal_for_standard_position() {
    // The standard-chess `Position` path (masked fast generator), independent of
    // the variant layer.
    let pos = Position::startpos();
    let legal: BTreeSet<Move> = pos.legal_moves().into_iter().collect();
    let staged: BTreeSet<Move> = pos.staged_moves(None).collect();
    assert_eq!(staged, legal);

    // A sharp middlegame with captures available (Kiwipete) exercises ordering.
    let kiwipete =
        Position::from_fen("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1")
            .unwrap();
    let legal: BTreeSet<Move> = kiwipete.legal_moves().into_iter().collect();
    let staged: Vec<Move> = kiwipete.staged_moves(None).collect();
    assert_eq!(staged.iter().copied().collect::<BTreeSet<_>>(), legal);
    assert_captures_before_quiets(&staged, false);
    // There are real captures here, so the first staged move must be a capture.
    assert!(
        staged[0].is_capture(),
        "with captures available, the first staged move is a capture"
    );

    // TT move first.
    let tt = *legal.iter().find(|m| !m.is_capture()).unwrap();
    let with_tt: Vec<Move> = kiwipete.staged_moves(Some(tt)).collect();
    assert_eq!(with_tt[0], tt);
    assert_eq!(with_tt.len(), legal.len());
}

#[test]
fn staged_in_check_yields_only_evasions() {
    // White king on e1 is checked by the rook on e8; only legal evasions exist.
    let pos = Position::from_fen("4r2k/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
    assert!(pos.is_check());
    let legal: BTreeSet<Move> = pos.legal_moves().into_iter().collect();
    let staged: BTreeSet<Move> = pos.staged_moves(None).collect();
    assert_eq!(
        staged, legal,
        "in check, staged yields exactly the evasions"
    );

    // A checkmate has no legal moves and so no staged moves.
    let mate = Position::from_fen("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3")
        .unwrap();
    assert!(mate.is_checkmate());
    assert_eq!(mate.staged_moves(None).count(), 0);
}

#[test]
fn staged_captures_ordered_by_victim_value() {
    // A white queen on d4 can capture the black queen on a4 (value 900) along the
    // rank or the black knight on d7 (value 320) up the file: the staged captures
    // stage must offer the queen capture before the knight capture.
    let pos = Position::from_fen("k7/3n4/8/8/q2Q4/8/8/4K3 w - - 0 1").unwrap();
    let staged: Vec<Move> = pos.staged_moves(None).collect();
    let caps: Vec<Move> = staged.iter().copied().filter(|m| m.is_capture()).collect();
    assert!(caps.len() >= 2, "expected at least two captures");
    // Victim values are non-increasing across the capture prefix.
    let mut prev = i32::MAX;
    for c in &caps {
        let v = victim_value(&pos, c);
        assert!(
            v <= prev,
            "captures must be ordered by non-increasing victim value"
        );
        prev = v;
    }
    // The very first capture takes the queen (the most valuable victim).
    assert_eq!(victim_value(&pos, &caps[0]), 900);
}

/// Re-derives the public victim value for a capture (mirrors the crate-internal
/// MVV key) for the ordering assertion above.
fn victim_value(pos: &Position, mv: &Move) -> i32 {
    use mce::{MoveKind, Role};
    let value = |role: Role| match role {
        Role::Pawn => 100,
        Role::Knight => 320,
        Role::Bishop => 330,
        Role::Rook => 500,
        Role::Queen => 900,
        Role::King => 10_000,
    };
    if mv.kind() == MoveKind::EnPassant {
        return value(Role::Pawn);
    }
    pos.board().role_at(mv.to()).map_or(0, value)
}

#[test]
fn staged_typed_variants_smoke() {
    // Exercise the concrete generic `VariantPosition::staged_moves` (the typed,
    // lazy-iterator API) for a representative position of each variant.
    check_typed(&Chess::startpos());
    check_typed(&Chess960::startpos());
    check_typed(&KingOfTheHill::startpos());
    check_typed(&ThreeCheck::startpos());
    check_typed(&RacingKings::startpos());
    check_typed(&Horde::startpos());
    check_typed(&Atomic::startpos());
    check_typed(&Antichess::startpos());
    check_typed(&Crazyhouse::startpos());

    // And a few moves deep into each, where captures and variant rules kick in.
    for &id in &ALL_IDS {
        let any = random_anyvariant(id, 0xDEAD_BEEF, 12);
        check_anyvariant(&any);
    }
}
