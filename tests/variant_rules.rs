//! Tests for the derived [`VariantRules`] model (issue #544).
//!
//! Two families of check:
//!
//! * **Coverage** — [`WideVariantId::rules`] is derivable for **every**
//!   [`WideVariantId::ALL`] without panic, and every value is populated (a real
//!   board, a non-empty army, a start FEN, sane per-piece geometry).
//! * **Correctness of derivation** — specific derived facts are asserted against
//!   the known ruleset across mechanic families (Berolina, Los Alamos, Torpedo,
//!   Petrified, Nightrider, Extinction, Grasshopper, New Zealand, board sizes,
//!   no-castling), so the model is a cross-check on the engine hooks: a wrong fact
//!   means either the model or a variant hook is wrong.

use mcr::geometry::{
    Movement, RoyalRule, Step, ValidationOracle, VariantRules, WideRole, WideVariantId,
};

/// Finds the rules for a variant by its canonical name.
fn rules(name: &str) -> VariantRules {
    name.parse::<WideVariantId>()
        .unwrap_or_else(|_| panic!("unknown variant {name}"))
        .rules()
}

/// Whether `movement` reaches the primitive step `(file, rank)` with the given
/// `rides` flag.
fn has_step(movement: &Movement, file: i8, rank: i8, rides: bool) -> bool {
    movement.steps.contains(&Step { file, rank, rides })
}

/// The piece rules for `role` in `variant`.
fn piece(rules: &VariantRules, role: WideRole) -> &mcr::geometry::PieceRules {
    rules
        .army
        .iter()
        .find(|p| p.role == role)
        .unwrap_or_else(|| panic!("{role:?} not in army"))
}

// ---- Coverage -----------------------------------------------------------------

/// Every wide variant derives a fully-populated `VariantRules` without panicking.
#[test]
fn rules_derivable_for_every_wide_variant() {
    for &id in WideVariantId::ALL {
        let rules = id.rules();

        // Board: real dimensions, a backing width, a geometry name, a start FEN.
        assert!(rules.board.width >= 3, "{id}: board width");
        assert!(rules.board.height >= 3, "{id}: board height");
        assert_eq!(
            rules.board.square_count as u32,
            rules.board.width as u32 * rules.board.height as u32,
            "{id}: square count is width * height"
        );
        assert!(
            matches!(rules.board.backing_bits, 64 | 128 | 256),
            "{id}: backing bits {}",
            rules.board.backing_bits
        );
        assert!(!rules.board.geometry.is_empty(), "{id}: geometry name");
        assert!(!rules.board.start_fen.is_empty(), "{id}: start FEN");

        // Army: at least one piece, each with a name and a FEN char.
        assert!(!rules.army.is_empty(), "{id}: army is non-empty");
        for p in &rules.army {
            assert!(!p.name.is_empty(), "{id}: {:?} name", p.role);
            assert_ne!(p.fen_char, '?', "{id}: {:?} fen char", p.role);
            // Every fielded piece can move some way: either an empty-board move or
            // capture geometry, or it is flagged as a hopper / board-dependent /
            // (rarely) immobile piece. This guards against a silently-empty sample.
            let has_geometry = !p.movement.steps.is_empty() || !p.capture.steps.is_empty();
            let flagged = p.hopper || p.board_dependent;
            // The only immobile shipped piece is the Chak Temple.
            let immobile_ok = p.role == WideRole::Temple;
            assert!(
                has_geometry || flagged || immobile_ok,
                "{id}: {:?} has no derived geometry and is not flagged",
                p.role
            );
        }

        // Draw / terminal: the repetition fold is sane; a counting variant tracks it.
        assert!(rules.draw.repetition_fold >= 1, "{id}: repetition fold");

        // Oracle is always populated (never left at a placeholder that disagrees
        // with the id-level mapping).
        assert_eq!(
            rules.oracle,
            id.validation_oracle(),
            "{id}: oracle matches id mapping"
        );
    }
}

/// The three history-independent draw / terminal hooks the model surfaces agree,
/// field for field, with the coverage-gate's `draw_hooks` introspection — the model
/// cannot drift from the existing draw-hook surface.
#[test]
fn draw_fields_agree_with_draw_hooks() {
    for &id in WideVariantId::ALL {
        let rules = id.rules();
        let hooks = id.draw_hooks();
        assert_eq!(
            rules.draw.counting_rule.is_some(),
            hooks.counting_rule,
            "{id}: counting_rule"
        );
        assert_eq!(
            rules.draw.impasse.is_some(),
            hooks.impasse_rule,
            "{id}: impasse_rule"
        );
        assert_eq!(rules.draw.has_bikjang, hooks.has_bikjang, "{id}: bikjang");
        assert_eq!(
            rules.draw.stalemate_is_loss, hooks.stalemate_is_loss,
            "{id}: stalemate_is_loss"
        );
        assert_eq!(
            rules.draw.has_bare_king_draw, hooks.has_bare_king_draw,
            "{id}: bare_king_draw"
        );
        assert_eq!(
            rules.draw.has_bare_king_loss, hooks.has_bare_king_loss,
            "{id}: bare_king_loss"
        );
        assert_eq!(
            rules.draw.move_rule_plies.is_some(),
            hooks.move_rule_plies,
            "{id}: move_rule_plies"
        );
        assert_eq!(
            rules.terminal.wins_on_check, hooks.wins_on_check,
            "{id}: wins_on_check"
        );
        assert_eq!(
            rules.terminal.extinction.is_some(),
            hooks.extinction_rule,
            "{id}: extinction_rule"
        );
        assert_eq!(
            rules.draw.perpetual_check_loses, hooks.perpetual_check_loses,
            "{id}: perpetual_check_loses"
        );
        assert_eq!(
            rules.draw.attack_repetition_loses, hooks.attack_repetition_loses,
            "{id}: attack_repetition_loses"
        );
    }
}

// ---- Correctness of derivation ------------------------------------------------

/// Berolina: the pawn *captures straight* and *moves diagonally* — the inversion of
/// the ordinary pawn. The derived capture geometry is the straight-forward square;
/// the diagonal move is carried by the pawn's Berolina flag (it is generated outside
/// the attack vocabulary).
#[test]
fn berolina_pawn_captures_straight_moves_diagonally() {
    let r = rules("berolina");
    assert!(r.pawns.berolina, "berolina flag");
    let pawn = piece(&r, WideRole::Pawn);
    assert!(
        has_step(&pawn.capture, 0, 1, false),
        "berolina pawn captures straight forward"
    );
    // It does not capture on the ordinary diagonal.
    assert!(
        !has_step(&pawn.capture, 1, 1, false) && !has_step(&pawn.capture, -1, 1, false),
        "berolina pawn does not capture diagonally"
    );
}

/// Los Alamos: no Bishop in the army, and pawns promote only to Queen / Rook /
/// Knight (never a Bishop) on a 6x6 board.
#[test]
fn los_alamos_has_no_bishop() {
    let r = rules("losalamos");
    assert_eq!((r.board.width, r.board.height), (6, 6), "6x6 board");
    assert!(
        !r.army.iter().any(|p| p.role == WideRole::Bishop),
        "no bishop on the board"
    );
    assert!(
        !r.promotion.roles.contains(&WideRole::Bishop),
        "promotion never yields a bishop"
    );
    assert_eq!(
        r.promotion.roles,
        alloc_vec(&[WideRole::Queen, WideRole::Rook, WideRole::Knight]),
        "promotes to Q/R/N"
    );
}

/// Torpedo: a pawn may double-step from **any** rank.
#[test]
fn torpedo_double_steps_from_any_rank() {
    let r = rules("torpedo");
    assert!(r.pawns.double_step_any_rank, "double step from any rank");
    assert_eq!(r.pawns.double_step_ranks.len(), r.board.height as usize);
}

/// A plain single-step / standard pawn does *not* double-step from every rank.
#[test]
fn gardner_has_no_double_step() {
    let r = rules("gardner");
    assert!(!r.pawns.double_step_any_rank, "no any-rank double step");
    assert_eq!((r.board.width, r.board.height), (5, 5), "5x5 board");
}

/// Petrified: the petrify-on-capture mechanic (Queen/Rook/Bishop/Knight petrify),
/// a pseudo-royal Commoner that may not capture.
#[test]
fn petrified_has_petrify_and_pseudo_royal() {
    let r = rules("petrified");
    assert!(r.mechanics.has_petrify, "has petrify");
    assert_eq!(
        r.mechanics.petrifying_roles,
        alloc_vec(&[
            WideRole::Knight,
            WideRole::Bishop,
            WideRole::Rook,
            WideRole::Queen
        ]),
        "the sliders/knight petrify (not the pawn or king)"
    );
    assert!(
        r.mechanics.royal_cannot_capture,
        "the pseudo-royal Commoner cannot capture"
    );
    assert_eq!(
        r.terminal.royal,
        RoyalRule::PseudoRoyalAllSurvive,
        "pseudo-royal king"
    );
}

/// Nightrider: a riding leaper (its knight step repeats), routed through the
/// full per-move king-safety verify path.
#[test]
fn nightrider_is_a_riding_leaper_needing_full_verify() {
    let r = rules("nightrider");
    assert!(r.mechanics.needs_full_verify, "needs full verify");
    let nr = piece(&r, WideRole::Nightrider);
    // It rides the knight direction (1,2) — a repeating leap, distinct from a plain
    // knight (which would be `rides == false`).
    assert!(
        has_step(&nr.capture, 1, 2, true),
        "nightrider rides the knight step"
    );
    assert!(
        !has_step(&nr.capture, 1, 2, false),
        "the knight step is a rider, not a single leap"
    );
}

/// Extinction: the king is non-royal and the extinction terminal watches every army
/// role at threshold 0.
#[test]
fn extinction_watches_the_whole_army() {
    let r = rules("extinction");
    assert_eq!(r.terminal.royal, RoyalRule::NonRoyal, "non-royal king");
    let ext = r.terminal.extinction.expect("extinction rule");
    assert_eq!(ext.threshold, 0, "threshold 0");
    for role in [
        WideRole::Pawn,
        WideRole::Knight,
        WideRole::Bishop,
        WideRole::Rook,
        WideRole::Queen,
        WideRole::King,
    ] {
        assert!(ext.watched.contains(&role), "watches {role:?}");
    }
}

/// Kinglet: the extinction terminal watches only the Pawn.
#[test]
fn kinglet_watches_only_the_pawn() {
    let r = rules("kinglet");
    let ext = r.terminal.extinction.expect("extinction rule");
    assert_eq!(
        ext.watched,
        alloc_vec(&[WideRole::Pawn]),
        "watches only pawn"
    );
    assert_eq!(ext.threshold, 0);
}

/// Grasshopper: the Grasshopper is a screen hopper — no empty-board geometry, the
/// hopper flag set.
#[test]
fn grasshopper_is_a_hopper() {
    let r = rules("grasshopper");
    let g = piece(&r, WideRole::Grasshopper);
    assert!(g.hopper, "grasshopper is a hopper");
    assert!(
        g.movement.steps.is_empty() && g.capture.steps.is_empty(),
        "no empty-board geometry"
    );
}

/// New Zealand: the Rook­ni *moves* like a rook (a slider) but *captures* like a
/// knight (a leaper) — a move≠capture piece; the Lancer is its mirror.
#[test]
fn newzealand_rookni_move_differs_from_capture() {
    let r = rules("newzealand");
    let rookni = piece(&r, WideRole::Rookni);
    assert!(rookni.move_neq_capture, "rookni move != capture");
    assert!(
        has_step(&rookni.movement, 1, 0, true),
        "rookni moves along a rook line (rides)"
    );
    assert!(
        has_step(&rookni.capture, 1, 2, false),
        "rookni captures like a knight (single leap)"
    );

    let lancer = piece(&r, WideRole::Lancer);
    assert!(lancer.move_neq_capture, "lancer move != capture");
    assert!(
        has_step(&lancer.movement, 1, 2, false),
        "lancer moves like a knight"
    );
    assert!(
        has_step(&lancer.capture, 1, 0, true),
        "lancer captures along a rook line"
    );
}

/// No-castle chess disables castling; a plain variant (standard-army no-castle)
/// keeps the standard promotion set and en passant.
#[test]
fn nocastle_has_no_castling() {
    let r = rules("nocastle");
    assert!(!r.castling.enabled, "no castling");
    assert!(r.pawns.en_passant, "en passant retained");
    assert_eq!(
        r.terminal.royal,
        RoyalRule::Checkmate,
        "ordinary royal king"
    );
}

/// Standard sliders are classified as sliders and ride their unit directions; the
/// Rook rides the four orthogonals, the Bishop the four diagonals.
#[test]
fn standard_sliders_ride_their_lines() {
    let r = rules("nocastle");
    let rook = piece(&r, WideRole::Rook);
    assert!(rook.is_slider, "rook is a slider");
    for (f, rk) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        assert!(
            has_step(&rook.capture, f, rk, true),
            "rook rides ({f},{rk})"
        );
    }
    let bishop = piece(&r, WideRole::Bishop);
    assert!(bishop.is_slider, "bishop is a slider");
    for (f, rk) in [(1, 1), (-1, 1), (1, -1), (-1, -1)] {
        assert!(
            has_step(&bishop.capture, f, rk, true),
            "bishop rides ({f},{rk})"
        );
    }
}

/// The validation-provenance pointer is derived: an FSF variant names its UCI
/// variant, the HaChu-only large shogi reports HaChu, and the oracle-less variants
/// report Independent.
#[test]
fn validation_oracle_is_derived() {
    assert_eq!(
        rules("berolina").oracle,
        ValidationOracle::FairyStockfish("berolina")
    );
    // A name that differs from mcr's canonical spelling.
    assert_eq!(
        rules("tori").oracle,
        ValidationOracle::FairyStockfish("torishogi")
    );
    assert_eq!(rules("chu").oracle, ValidationOracle::HaChu);
    assert_eq!(rules("alice").oracle, ValidationOracle::Independent);
}

/// Xiangqi: cannons and the flying-general rule are surfaced; the whole-board
/// Janggi cannon is flagged board-dependent.
#[test]
fn cannon_variants_surface_their_mechanics() {
    let xq = rules("xiangqi");
    assert!(xq.mechanics.has_cannons, "xiangqi has cannons");
    assert!(xq.mechanics.has_flying_general, "xiangqi flying general");
    assert!(xq.army.iter().any(|p| p.role == WideRole::Cannon), "cannon");

    // The Janggi cannon computes its move/attack set from the whole board.
    let jg = rules("janggi");
    assert!(jg.mechanics.uses_board_attacks, "janggi board attacks");
    let cannon = piece(&jg, WideRole::Cannon);
    assert!(cannon.board_dependent, "janggi cannon is board-dependent");
}

/// A small owned-vec helper (the crate is `no_std` + `alloc`, but the test binary is
/// `std`, so this just wraps `Vec::from`).
fn alloc_vec(roles: &[WideRole]) -> Vec<WideRole> {
    roles.to_vec()
}
