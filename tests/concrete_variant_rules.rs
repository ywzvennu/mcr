//! Tests for the unified [`VariantRef`] catalog and the concrete-8x8
//! [`VariantId::rules`] derivation (issue #549).
//!
//! Two families of check:
//!
//! * **Coverage** — [`VariantRef::rules`] is derivable for **every**
//!   [`VariantRef::ALL`] (all ~99 concrete + wide) without panic, and every value
//!   is populated (a real board, a non-empty army, sane per-piece geometry).
//! * **Correctness of the concrete derivation** — the bespoke facts of each of the
//!   nine concrete variants (Atomic blast, Antichess mandatory captures + lose-all
//!   win + non-royal king, Crazyhouse drops, King-of-the-Hill hill squares,
//!   Three-check count = 3, Racing Kings goal + no-check, Horde asymmetry,
//!   Chess960 shuffle, Standard baseline).

use mcr::geometry::{RoyalRule, WideRole, WideVariantId};
use mcr::{VariantId, VariantRef};

// ---- Unified catalog coverage -------------------------------------------------

/// `VariantRef::ALL` spans both families: the nine concrete plus the ninety-one wide.
#[test]
fn all_spans_both_families() {
    let concrete = VariantRef::ALL
        .iter()
        .filter(|r| matches!(r, VariantRef::Concrete(_)))
        .count();
    let wide = VariantRef::ALL
        .iter()
        .filter(|r| matches!(r, VariantRef::Wide(_)))
        .count();
    assert_eq!(concrete, VariantId::ALL.len(), "every concrete variant");
    assert_eq!(wide, WideVariantId::ALL.len(), "every wide variant");
    assert_eq!(concrete, 9, "nine concrete variants");
    assert_eq!(wide, 92, "ninety-two wide variants");
    assert_eq!(
        VariantRef::ALL.len(),
        101,
        "one hundred one variants in all"
    );
}

/// Every entry in `VariantRef::ALL` derives a fully-populated `VariantRules`
/// without panicking, and dispatches to the matching per-family derivation.
#[test]
fn rules_derivable_for_every_ref() {
    for &r in &VariantRef::ALL {
        let rules = r.rules();

        // Board: real dimensions, a backing width, a geometry name, a start FEN.
        assert!(rules.board.width >= 3, "{}: board width", r.name());
        assert!(rules.board.height >= 3, "{}: board height", r.name());
        assert_eq!(
            rules.board.square_count as u32,
            rules.board.width as u32 * rules.board.height as u32,
            "{}: square count",
            r.name()
        );
        assert!(
            matches!(rules.board.backing_bits, 64 | 128 | 256),
            "{}: backing bits",
            r.name()
        );
        assert!(!rules.board.geometry.is_empty(), "{}: geometry", r.name());
        assert!(!rules.board.start_fen.is_empty(), "{}: start fen", r.name());

        // Army: at least one piece, each with a name and a FEN char, and either
        // some empty-board geometry or an explicit flag.
        assert!(!rules.army.is_empty(), "{}: army non-empty", r.name());
        for p in &rules.army {
            assert!(!p.name.is_empty(), "{}: {:?} name", r.name(), p.role);
            assert_ne!(p.fen_char, '?', "{}: {:?} fen char", r.name(), p.role);
            let has_geometry = !p.movement.steps.is_empty() || !p.capture.steps.is_empty();
            let flagged = p.hopper || p.board_dependent;
            let immobile_ok = p.role == WideRole::Temple;
            assert!(
                has_geometry || flagged || immobile_ok,
                "{}: {:?} has no derived geometry",
                r.name(),
                p.role
            );
        }
        assert!(rules.draw.repetition_fold >= 1, "{}: rep fold", r.name());
    }
}

/// The spanning `VariantRef::rules` dispatch equals the per-family entry point for
/// every variant, and `name` / `from_name` round-trip.
#[test]
fn ref_dispatch_and_name_round_trip() {
    for &id in VariantId::ALL {
        let r = VariantRef::Concrete(id);
        assert_eq!(r.rules(), id.rules(), "{id}: concrete dispatch");
        assert_eq!(r.name(), id.as_str(), "{id}: name");
        assert_eq!(
            VariantRef::from_name(id.as_str()),
            Some(r),
            "{id}: from_name"
        );
    }
    for &id in WideVariantId::ALL {
        let r = VariantRef::Wide(id);
        assert_eq!(r.rules(), id.rules(), "{id}: wide dispatch");
        assert_eq!(r.name(), id.as_str(), "{id}: name");
        assert_eq!(
            VariantRef::from_name(id.as_str()),
            Some(r),
            "{id}: from_name"
        );
    }
    assert_eq!(VariantRef::from_name("not-a-variant"), None);
}

// ---- Concrete correctness -----------------------------------------------------

/// Standard chess baseline: 8x8, standard army, castling + en passant + double
/// step, promotion to Q/R/B/N, checkmate, and the fifty-move + repetition draws.
#[test]
fn standard_baseline() {
    let r = VariantId::Standard.rules();
    assert_eq!((r.board.width, r.board.height), (8, 8));
    assert_eq!(r.army.len(), 6, "all six standard roles");
    assert_eq!(r.terminal.royal, RoyalRule::Checkmate);
    assert!(r.castling.enabled, "castling");
    assert_eq!(r.castling.king_dest_kingside, 6, "king to the g-file");
    assert_eq!(r.castling.king_dest_queenside, 2, "king to the c-file");
    assert!(r.pawns.en_passant, "en passant");
    assert_eq!(
        r.pawns.double_step_ranks,
        vec![1],
        "double step from rank 2"
    );
    assert_eq!(
        r.promotion.roles,
        vec![
            WideRole::Knight,
            WideRole::Bishop,
            WideRole::Rook,
            WideRole::Queen
        ]
    );
    assert_eq!(r.draw.move_rule_plies, Some(100), "fifty-move rule");
    assert!(r.draw.tracks_repetition, "repetition tracked");
    // No concrete-only mechanic on standard chess.
    assert!(!r.mechanics.atomic_blast && !r.mechanics.mandatory_captures);
    assert!(r.terminal.check_count_to_win.is_none() && r.terminal.region_win.is_none());
}

/// The standard sliders ride their unit lines; the knight and king are leapers;
/// the pawn captures diagonally with an empty (elsewhere-generated) forward move.
#[test]
fn standard_piece_geometry() {
    let r = VariantId::Standard.rules();
    let piece = |role| r.army.iter().find(|p| p.role == role).unwrap();
    let has = |m: &mcr::geometry::Movement, f, rk, rides| {
        m.steps.contains(&mcr::geometry::Step {
            file: f,
            rank: rk,
            rides,
        })
    };
    let rook = piece(WideRole::Rook);
    assert!(rook.is_slider && has(&rook.capture, 1, 0, true));
    let bishop = piece(WideRole::Bishop);
    assert!(bishop.is_slider && has(&bishop.capture, 1, 1, true));
    let knight = piece(WideRole::Knight);
    assert!(!knight.is_slider && has(&knight.capture, 1, 2, false));
    let king = piece(WideRole::King);
    assert!(has(&king.capture, 1, 0, false) && has(&king.capture, 1, 1, false));
    let pawn = piece(WideRole::Pawn);
    assert!(pawn.move_neq_capture, "pawn move != capture");
    assert!(has(&pawn.capture, 1, 1, false) && has(&pawn.capture, -1, 1, false));
    assert!(
        pawn.movement.steps.is_empty(),
        "forward move carried by `pawns`"
    );
}

/// Chess960: the shuffled-setup flag is set and castling stays enabled.
#[test]
fn chess960_is_shuffled() {
    let r = VariantId::Chess960.rules();
    assert!(r.mechanics.shuffled_setup, "shuffled setup");
    assert!(r.castling.enabled, "castling");
    assert_eq!(r.terminal.royal, RoyalRule::Checkmate);
}

/// Atomic: the capture blast mechanic and the king-explosion win.
#[test]
fn atomic_blast_and_explosion_win() {
    let r = VariantId::Atomic.rules();
    assert!(r.mechanics.atomic_blast, "capture detonates");
    assert!(r.terminal.explosion_win, "exploding the enemy king wins");
    assert_eq!(r.terminal.royal, RoyalRule::Checkmate, "royal king");
    assert_eq!(
        r.oracle,
        mcr::geometry::ValidationOracle::FairyStockfish("atomic")
    );
}

/// Antichess: a non-royal (capturable) king, mandatory captures, king-promotion,
/// no castling, and the lose-all win.
#[test]
fn antichess_non_royal_forced_captures_lose_all() {
    let r = VariantId::Antichess.rules();
    assert_eq!(r.terminal.royal, RoyalRule::NonRoyal, "king is non-royal");
    assert!(r.mechanics.mandatory_captures, "captures are forced");
    assert!(r.terminal.lose_all_wins, "losing all pieces / no move wins");
    assert!(!r.castling.enabled, "no castling");
    assert!(
        r.promotion.roles.contains(&WideRole::King),
        "may promote to a king"
    );
}

/// Crazyhouse: captured pieces go to hand and can be dropped (the hand mechanic).
#[test]
fn crazyhouse_has_hand_drops() {
    let r = VariantId::Crazyhouse.rules();
    assert!(r.mechanics.has_hand, "captured pieces go to hand / drops");
    assert_eq!(r.terminal.royal, RoyalRule::Checkmate);
}

/// King of the Hill: a region-goal win on the four central squares d4/e4/d5/e5.
#[test]
fn koth_center_region_win() {
    let r = VariantId::KingOfTheHill.rules();
    let region = r.terminal.region_win.expect("region win");
    let mut squares = region.squares.clone();
    squares.sort_unstable();
    // d4=(3,3), e4=(4,3), d5=(3,4), e5=(4,4).
    assert_eq!(squares, vec![(3, 3), (3, 4), (4, 3), (4, 4)]);
}

/// Three-check: the win is a check count of three.
#[test]
fn three_check_count_is_three() {
    let r = VariantId::ThreeCheck.rules();
    assert_eq!(r.terminal.check_count_to_win, Some(3), "three checks win");
    assert!(r.terminal.wins_on_check, "a repeated-check win");
}

/// Racing Kings: the goal is the eighth rank, checks are illegal, no pawns, no
/// castling.
#[test]
fn racing_kings_goal_and_no_check() {
    let r = VariantId::RacingKings.rules();
    let flag = r.terminal.flag_win.expect("flag win");
    assert_eq!(
        flag.rank_white, 7,
        "king reaches the eighth rank (0-based 7)"
    );
    assert!(
        r.mechanics.checks_forbidden,
        "no move may give/stand in check"
    );
    assert!(!r.castling.enabled, "no castling");
    assert!(!r.pawns.en_passant, "pawnless: no en passant");
    assert!(
        !r.army.iter().any(|p| p.role == WideRole::Pawn),
        "no pawns on the board"
    );
}

/// Horde: an asymmetric start (kingless White pawn horde) and the White-loses-when-
/// eliminated terminal; White's pawns double-step from the first rank too.
#[test]
fn horde_asymmetry_and_elimination() {
    let r = VariantId::Horde.rules();
    assert!(r.mechanics.asymmetric_armies, "asymmetric armies");
    assert!(
        r.terminal.all_pieces_lost_loses,
        "a side with no material loses"
    );
    assert_eq!(
        r.pawns.double_step_ranks,
        vec![0, 1],
        "White may double-push from the first rank"
    );
    assert!(r.castling.enabled, "black still castles");
}
