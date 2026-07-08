//! Generator + drift-check for the per-variant reference pages under
//! `docs/variants/` — one richly-sectioned markdown page per variant plus an
//! index.
//!
//! Where `docs/variants.md` (and [`variants_doc`](../variants_doc.rs)) carry a
//! single hand-authored summary table, these pages are rendered **entirely from
//! the engine-derived [`VariantRules`](mcr::geometry::VariantRules) model** reached
//! through [`VariantRef`]: board, army movement geometry, pawn / promotion /
//! castling rules, draw and terminal conditions, and special mechanics. Every line
//! is a rendered fact from the model — there is no hand-authored prose — so a page
//! can never disagree with the move generator.
//!
//! Two entry points share the renderers:
//!
//! * [`render_page`] builds one variant's markdown page; [`render_index`] builds
//!   the index (`docs/variants/README.md`).
//! * The [`variant_pages_are_up_to_date`] test regenerates every page in memory and
//!   asserts each equals its committed copy (the golden-file / `insta` pattern, done
//!   with `std` only). Regenerate the committed files with
//!   `REGEN=1 cargo test --test variant_pages_doc` (or set `BLESS=1`).

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mcr::geometry::{
    CastlingRules, DrawRules, ImpasseInfo, Movement, PawnRules, PieceRules, PromotionRules,
    RoyalRule, SpecialMechanics, Step, TerminalRules, ValidationOracle, VariantRules, WideRole,
};
use mcr::VariantRef;

// --- paths ---------------------------------------------------------------------

/// The directory the per-variant pages live in.
fn pages_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/variants")
}

/// The committed page path for one variant.
fn page_path(name: &str) -> PathBuf {
    pages_dir().join(format!("{name}.md"))
}

/// The committed index path.
fn index_path() -> PathBuf {
    pages_dir().join("README.md")
}

// --- small formatting helpers --------------------------------------------------

/// A 0-based rank rendered as its human 1-based number.
fn rank_label(rank: u8) -> u16 {
    u16::from(rank) + 1
}

/// A 0-based file rendered as its board letter (`0` → `a`).
fn file_letter(file: u8) -> char {
    (b'a' + file) as char
}

/// A `(file, rank)` square in algebraic form (`(0, 0)` → `a1`).
fn square_label(file: u8, rank: u8) -> String {
    format!("{}{}", file_letter(file), rank_label(rank))
}

/// A comma-separated list of role names (their `WideRole` identifiers), or
/// `"none"` when the slice is empty.
fn role_list(roles: &[WideRole]) -> String {
    if roles.is_empty() {
        return "none".to_string();
    }
    roles
        .iter()
        .map(|r| format!("{r:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The compass-ish direction word for a primitive `(file, rank)` step, in White's
/// orientation (positive rank = forward, positive file = toward the h-file).
fn dir_word(file: i8, rank: i8) -> &'static str {
    match (file.cmp(&0), rank.cmp(&0)) {
        (Ordering::Equal, Ordering::Greater) => "forward",
        (Ordering::Equal, Ordering::Less) => "backward",
        (Ordering::Greater, Ordering::Equal) => "right",
        (Ordering::Less, Ordering::Equal) => "left",
        (Ordering::Greater, Ordering::Greater) => "forward-right",
        (Ordering::Less, Ordering::Greater) => "forward-left",
        (Ordering::Greater, Ordering::Less) => "back-right",
        (Ordering::Less, Ordering::Less) => "back-left",
        (Ordering::Equal, Ordering::Equal) => "in place",
    }
}

/// One primitive step rendered as `direction (Δf,Δr)`.
fn step_desc(s: &Step) -> String {
    format!("{} ({:+},{:+})", dir_word(s.file, s.rank), s.file, s.rank)
}

/// The move geometry of a [`Movement`] as bullet-body strings: a riding line and a
/// single-step line, whichever are present.
fn movement_bullets(m: &Movement) -> Vec<String> {
    let riders: Vec<String> = m.steps.iter().filter(|s| s.rides).map(step_desc).collect();
    let steppers: Vec<String> = m.steps.iter().filter(|s| !s.rides).map(step_desc).collect();
    let mut out = Vec::new();
    if !riders.is_empty() {
        out.push(format!(
            "rides (repeats until blocked): {}",
            riders.join(", ")
        ));
    }
    if !steppers.is_empty() {
        out.push(format!("single step / leap: {}", steppers.join(", ")));
    }
    out
}

// --- page sections -------------------------------------------------------------

/// The Overview section: id, board geometry, and the validation oracle.
fn render_overview(out: &mut String, r: &VariantRules, name: &str) {
    let b = &r.board;
    out.push_str("## Overview\n\n");
    out.push_str(&format!("- Id: `{name}`\n"));
    out.push_str(&format!(
        "- Board: {}x{} ({} squares, `{}` geometry, {}-bit backing)\n",
        b.width, b.height, b.square_count, b.geometry, b.backing_bits
    ));
    out.push_str(&format!(
        "- Validation oracle: {}\n\n",
        render_oracle(r.oracle)
    ));
}

/// Renders the validation oracle pointer.
fn render_oracle(oracle: ValidationOracle) -> String {
    match oracle {
        ValidationOracle::FairyStockfish(n) => {
            format!("Fairy-Stockfish (`UCI_Variant {n}`)")
        }
        ValidationOracle::HaChu => "HaChu large-shogi reference engine".to_string(),
        ValidationOracle::Independent => {
            "Independent — no external engine oracle (in-repo generator / hand-derived counts)"
                .to_string()
        }
    }
}

/// The Setup section: the starting position FEN.
fn render_setup(out: &mut String, r: &VariantRules) {
    out.push_str("## Setup\n\n");
    out.push_str("Starting position (mcr FEN dialect):\n\n");
    out.push_str(&format!("```\n{}\n```\n\n", r.board.start_fen));
}

/// The short army-overview table.
fn render_army_table(out: &mut String, army: &[PieceRules]) {
    out.push_str("| Piece | FEN | Type | Move ≠ capture |\n");
    out.push_str("|---|---|---|---|\n");
    for p in army {
        let kind = piece_kind(p);
        out.push_str(&format!(
            "| {} | `{}` | {} | {} |\n",
            p.name,
            p.board_token,
            kind,
            if p.move_neq_capture { "yes" } else { "no" },
        ));
    }
    out.push('\n');
}

/// The short classification word for a piece.
fn piece_kind(p: &PieceRules) -> &'static str {
    if p.hopper {
        "screen hopper"
    } else if p.board_dependent {
        "whole-board attacker"
    } else if p.is_slider {
        "slider"
    } else {
        "leaper / stepper"
    }
}

/// The Pieces & movement section: the summary table then per-piece geometry.
fn render_pieces(out: &mut String, army: &[PieceRules]) {
    out.push_str("## Pieces & movement\n\n");
    out.push_str(
        "Move and capture geometry are **sampled from the engine's own move hooks** on an empty \
board (White's orientation: positive rank = forward, positive file = toward the h-file). Each \
step is `direction (Δfile,Δrank)`; \"rides\" marks a repeating slider / rider.\n\n",
    );
    render_army_table(out, army);

    for p in army {
        out.push_str(&format!("### {} (`{}`)\n\n", p.name, p.board_token));
        out.push_str(&format!("- Type: {}\n", piece_kind(p)));

        let pawnish = p.name == "Pawn" || p.name == "Hoplite";
        if p.hopper {
            out.push_str(
                "- Move/capture is occupancy-dependent (needs a screen); not sampled on an empty \
board.\n",
            );
        } else if p.board_dependent {
            out.push_str(
                "- Attack set is computed from the whole board; not sampled on an empty board.\n",
            );
        } else if pawnish {
            out.push_str(
                "- Forward move is defined in the **Pawns** section; the geometry below is this \
piece's capture / threat set.\n",
            );
            render_movement_group(out, "Captures / threats", &p.capture);
        } else if p.move_neq_capture {
            out.push_str("- **Move ≠ capture** — the two geometries differ.\n");
            render_movement_group(out, "Moves (non-capturing)", &p.movement);
            render_movement_group(out, "Captures / gives check", &p.capture);
        } else if p.movement.steps.is_empty() && p.capture.steps.is_empty() {
            out.push_str("- Immobile on an empty board (no step sampled).\n");
        } else {
            render_movement_group(out, "Moves & captures", &p.movement);
        }
        out.push('\n');
    }
}

/// Appends a labelled movement group (a sub-bullet per riding / stepping line).
fn render_movement_group(out: &mut String, label: &str, m: &Movement) {
    let bullets = movement_bullets(m);
    if bullets.is_empty() {
        out.push_str(&format!("- {label}: none sampled\n"));
        return;
    }
    out.push_str(&format!("- {label}:\n"));
    for b in bullets {
        out.push_str(&format!("  - {b}\n"));
    }
}

/// The Pawns section.
fn render_pawns(out: &mut String, p: &PawnRules) {
    out.push_str("## Pawns\n\n");
    let mut any = false;
    if p.stepper {
        out.push_str("- Forward stepper (Shogi-style single forward step)\n");
        any = true;
    }
    if p.berolina {
        out.push_str("- Berolina pawn: moves diagonally forward, captures straight forward\n");
        any = true;
    }
    if p.legan {
        out.push_str("- Legan pawn: corner-diagonal directional pawn\n");
        any = true;
    }
    if p.double_step_any_rank {
        out.push_str("- May take its two-square advance from **any** rank (Torpedo)\n");
        any = true;
    } else if !p.double_step_ranks.is_empty() {
        let ranks = p
            .double_step_ranks
            .iter()
            .map(|&r| rank_label(r).to_string())
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("- Double-step allowed from rank(s): {ranks}\n"));
        any = true;
    }
    if p.en_passant {
        out.push_str("- En passant available\n");
        any = true;
    }
    if p.moves_sideways {
        out.push_str("- May also step one square sideways\n");
        any = true;
    }
    if p.moves_backward {
        out.push_str("- May also step one square backward\n");
        any = true;
    }
    if !p.move_resets_clock {
        out.push_str("- An ordinary pawn move does **not** reset the move-count clock\n");
        any = true;
    }
    if !any {
        out.push_str("- Single forward step, no double step, no en passant\n");
    }
    out.push('\n');
}

/// The Promotion section.
fn render_promotion(out: &mut String, p: &PromotionRules) {
    out.push_str("## Promotion\n\n");
    if p.roles.is_empty() && p.zone_ranks.is_empty() && !p.piece_promotion_no_hand {
        out.push_str("- No promotion.\n\n");
        return;
    }
    out.push_str(&format!("- Promotes to: {}\n", role_list(&p.roles)));
    if !p.zone_ranks.is_empty() {
        let ranks = p
            .zone_ranks
            .iter()
            .map(|&r| rank_label(r).to_string())
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("- Promotion zone rank(s): {ranks}\n"));
    }
    if p.forced_on_last_rank {
        out.push_str("- Forced on the furthest rank\n");
    }
    if p.mandatory_in_zone {
        out.push_str("- Mandatory anywhere in the zone (Shogi far-zone rule)\n");
    }
    if p.lion_style {
        out.push_str("- Chu-Shogi lion-style promotion\n");
    }
    if p.piece_promotion_no_hand {
        out.push_str("- Non-pawn pieces promote by ending in the zone (no hand)\n");
    }
    out.push('\n');
}

/// The Castling section.
fn render_castling(out: &mut String, c: &CastlingRules) {
    out.push_str("## Castling\n\n");
    if !c.enabled {
        out.push_str("- Not available.\n\n");
        return;
    }
    out.push_str(&format!(
        "- Castling rank (White): {}\n",
        rank_label(c.castle_rank_white)
    ));
    out.push_str(&format!(
        "- Kingside: king lands on the {}-file, castling with the {:?}\n",
        file_letter(c.king_dest_kingside),
        c.rook_role_kingside,
    ));
    out.push_str(&format!(
        "- Queenside: king lands on the {}-file, castling with the {:?}\n",
        file_letter(c.king_dest_queenside),
        c.rook_role_queenside,
    ));
    out.push('\n');
}

/// The Draws & terminal section.
fn render_draw_terminal(out: &mut String, d: &DrawRules, t: &TerminalRules) {
    out.push_str("## Draws & terminal conditions\n\n");

    out.push_str("**Royalty & win condition**\n\n");
    out.push_str(&format!("- {}\n", royal_phrase(t.royal)));
    for line in terminal_lines(t) {
        out.push_str(&format!("- {line}\n"));
    }
    out.push('\n');

    out.push_str("**Draw / adjudication rules**\n\n");
    let draws = draw_lines(d);
    if draws.is_empty() {
        out.push_str("- No special draw rules beyond the standard checkmate / stalemate.\n");
    } else {
        for line in draws {
            out.push_str(&format!("- {line}\n"));
        }
    }
    out.push('\n');
}

/// The royalty phrase for the [`RoyalRule`].
fn royal_phrase(royal: RoyalRule) -> &'static str {
    match royal {
        RoyalRule::Checkmate => "Single royal king — a side loses by checkmate.",
        RoyalRule::NonRoyal => {
            "King is non-royal (no check) — a side loses by king capture / extinction."
        }
        RoyalRule::MultiRoyalAnySurvives => {
            "Multiple royals — in check only when every royal is attacked; a side may sacrifice \
one royal and play on."
        }
        RoyalRule::PseudoRoyalAllSurvive => "Pseudo-royal — every move must leave all royals safe.",
    }
}

/// The active terminal-win lines (excluding the royalty phrase and bare-king rules,
/// which are rendered with royalty / the draw rules).
fn terminal_lines(t: &TerminalRules) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(e) = &t.extinction {
        let dir = if e.extinct_wins { "win" } else { "lose" };
        if e.count_total {
            lines.push(format!(
                "Extinction: {dir} when the total piece count falls to {} or fewer",
                e.threshold,
            ));
        } else {
            lines.push(format!(
                "Extinction: {dir} when any watched role [{}] falls to {} or fewer",
                role_list(&e.watched),
                e.threshold,
            ));
        }
    }
    if t.checkmate_is_win {
        lines.push("Being checkmated wins for the mated side".to_string());
    }
    if let Some(f) = &t.flag_win {
        let safe = if f.requires_safe {
            " (the king must be safe there)"
        } else {
            ""
        };
        lines.push(format!(
            "Flag / campmate: a king reaching rank {} wins{safe}",
            rank_label(f.rank_white),
        ));
    }
    if let Some(reg) = &t.region_win {
        let squares = reg
            .squares
            .iter()
            .map(|&(f, r)| square_label(f, r))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "Region goal: a king reaching any of {squares} wins"
        ));
    }
    if let Some(n) = t.check_count_to_win {
        lines.push(format!("Check-count: deliver {n} checks to win"));
    }
    if t.wins_on_check {
        lines.push("Giving check wins the game outright".to_string());
    }
    if t.temple_win {
        lines.push("Move a Divine Lord onto the enemy temple to win".to_string());
    }
    if t.explosion_win {
        lines.push("Atomic: a capture whose blast destroys the enemy king wins".to_string());
    }
    if t.lose_all_wins {
        lines.push(
            "Losing chess: a side reduced to no pieces (or with no legal move) wins".to_string(),
        );
    }
    if t.all_pieces_lost_loses {
        lines.push("Elimination: a side reduced to no material loses (Horde)".to_string());
    }
    lines
}

/// The active draw / adjudication lines.
fn draw_lines(d: &DrawRules) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(plies) = d.move_rule_plies {
        lines.push(format!("Move-count draw after {plies} plies"));
    }
    if d.tracks_repetition {
        lines.push(format!(
            "Repetition tracked; adjudicates on {}-fold repetition",
            d.repetition_fold,
        ));
    }
    if let Some(rule) = d.counting_rule {
        lines.push(format!("Counting endgame: {rule:?}"));
    }
    if let Some(i) = &d.impasse {
        lines.push(render_impasse(i));
    }
    if d.has_bikjang {
        lines.push("Bikjang: the two generals facing on an open line draws".to_string());
    }
    if d.stalemate_is_loss {
        lines.push("Stalemate is a loss for the stalemated side".to_string());
    }
    if d.stalemate_is_win {
        lines.push("Stalemate is a win for the stalemated side".to_string());
    }
    if d.stalemate_piece_count {
        lines.push(
            "Stalemate is decided by piece count: the side with fewer pieces wins".to_string(),
        );
    }
    if d.has_bare_king_draw {
        lines.push("Reducing a side to a lone king is an immediate draw".to_string());
    }
    if d.has_bare_king_loss {
        lines.push("Baring a side's king is a loss".to_string());
    }
    if d.perpetual_check_loses {
        lines.push("Perpetual check loses for the checker".to_string());
    }
    if d.perpetual_chase_loses {
        lines.push("Perpetual chase loses for the chaser".to_string());
    }
    if d.attack_repetition_loses {
        lines.push("One-sided attack repetition loses".to_string());
    }
    lines
}

/// Renders the impasse / jishogi declaration parameters.
fn render_impasse(i: &ImpasseInfo) -> String {
    format!(
        "Impasse / jishogi declaration: Sente ≥ {}, Gote ≥ {} points; ≥ {} own pieces in the \
zone; big pieces ({}) score {}, others {}",
        i.sente_threshold,
        i.gote_threshold,
        i.min_pieces_in_zone,
        role_list(&i.big_roles),
        i.big_piece_points,
        i.small_piece_points,
    )
}

/// The Special mechanics section.
fn render_mechanics(out: &mut String, m: &SpecialMechanics) {
    out.push_str("## Special mechanics\n\n");
    let lines = mechanics_lines(m);
    if lines.is_empty() {
        out.push_str("- None.\n\n");
        return;
    }
    for line in lines {
        out.push_str(&format!("- {line}\n"));
    }
    out.push('\n');
}

/// The active special-mechanic lines.
fn mechanics_lines(m: &SpecialMechanics) -> Vec<String> {
    let mut lines = Vec::new();
    if m.needs_full_verify {
        lines.push(
            "Full make/unmake king-safety re-test each move (riding-leaper check geometry)"
                .to_string(),
        );
    }
    if m.has_petrify {
        lines.push(format!(
            "Petrify-on-capture — roles that turn to stone: {}",
            role_list(&m.petrifying_roles),
        ));
    }
    if m.royal_cannot_capture {
        lines.push("The (pseudo-royal) king may not capture".to_string());
    }
    if m.has_cannons {
        lines.push("Fields cannons (screen-hopping capture)".to_string());
    }
    if m.uses_board_attacks {
        lines.push("Some role's attack set is computed from the whole board".to_string());
    }
    if m.has_flying_general {
        lines.push("Flying-general rule (facing generals)".to_string());
    }
    if m.has_hand {
        lines.push("Persistent hand with drops".to_string());
    }
    if m.has_placement {
        lines.push("Setup / placement phase".to_string());
    }
    if m.supports_gating {
        lines.push("Seirawan gating of reserve pieces".to_string());
    }
    if m.has_duck {
        lines.push("Neutral Duck blocker (belongs to neither side)".to_string());
    }
    if m.is_alice {
        lines.push("Alice chess — pieces transfer between two mirror boards".to_string());
    }
    if m.has_first_move_leaps {
        lines.push("One-time first-move leap (Cambodian King / Met)".to_string());
    }
    if m.has_lion_moves {
        lines.push("Full Chu-Shogi Lion moves (igui, double capture, area move, pass)".to_string());
    }
    if m.has_area_burn {
        lines.push("Tenjiku Fire Demon area burn".to_string());
    }
    if m.has_jump_captures {
        lines.push("Tenjiku range-jumping generals".to_string());
    }
    if m.allows_pass {
        lines.push("A side may pass the turn".to_string());
    }
    if m.confine_pins_to_segment {
        lines.push("Pinned leapers are confined to the king–pinner segment".to_string());
    }
    if m.atomic_blast {
        lines.push("Atomic: a capture detonates a 3x3 blast".to_string());
    }
    if m.mandatory_captures {
        lines.push("Captures are mandatory when available".to_string());
    }
    if m.checks_forbidden {
        lines.push("Checks are wholly illegal (Racing Kings)".to_string());
    }
    if m.asymmetric_armies {
        lines.push("The two sides start with different armies".to_string());
    }
    if m.shuffled_setup {
        lines.push("Shuffled back-rank setup (the start FEN is one representative)".to_string());
    }
    lines
}

// --- top-level renderers -------------------------------------------------------

/// Renders one variant's complete markdown page.
fn render_page(vref: VariantRef) -> String {
    let name = vref.name();
    let r = vref.rules();
    let mut out = String::new();
    out.push_str("<!-- GENERATED FILE — do not edit by hand. -->\n");
    out.push_str(
        "<!-- Regenerate with: REGEN=1 cargo test --test variant_pages_doc (see tests/variant_pages_doc.rs). -->\n\n",
    );
    out.push_str(&format!("# {name}\n\n"));
    out.push_str(
        "Engine-derived ruleset — every statement below is rendered from mcr's own \
`VariantRules` model, so it can never drift from the move generator. See the \
[index](README.md) for all variants.\n\n",
    );
    render_overview(&mut out, &r, name);
    render_setup(&mut out, &r);
    render_pieces(&mut out, &r.army);
    render_pawns(&mut out, &r.pawns);
    render_promotion(&mut out, &r.promotion);
    render_castling(&mut out, &r.castling);
    render_draw_terminal(&mut out, &r.draw, &r.terminal);
    render_mechanics(&mut out, &r.mechanics);
    out
}

/// Renders the index page, grouping every variant by board size.
fn render_index() -> String {
    let mut by_size: BTreeMap<(u8, u8), Vec<&'static str>> = BTreeMap::new();
    for &vref in &VariantRef::ALL {
        let b = vref.rules().board;
        by_size
            .entry((b.width, b.height))
            .or_default()
            .push(vref.name());
    }

    let mut out = String::new();
    out.push_str("<!-- GENERATED FILE — do not edit by hand. -->\n");
    out.push_str(
        "<!-- Regenerate with: REGEN=1 cargo test --test variant_pages_doc (see tests/variant_pages_doc.rs). -->\n\n",
    );
    out.push_str("# mcr per-variant reference\n\n");
    out.push_str(&format!(
        "One engine-derived page per variant — **{}** in all, grouped by board size (`files`x`ranks`). \
Every page is rendered straight from mcr's `VariantRules` model (board, army movement, pawn / \
promotion / castling rules, draws, terminal conditions, special mechanics), so it can never drift \
from the move generator. For the one-line summary table see [../variants.md](../variants.md).\n\n",
        VariantRef::ALL.len(),
    ));

    for ((w, h), names) in &by_size {
        out.push_str(&format!("## {w}x{h}\n\n"));
        for name in names {
            out.push_str(&format!("- [{name}]({name}.md)\n"));
        }
        out.push('\n');
    }
    out
}

// --- tests ---------------------------------------------------------------------

/// Whether to (re)write the committed files instead of only checking them.
fn regen() -> bool {
    std::env::var_os("REGEN").is_some() || std::env::var_os("BLESS").is_some()
}

#[test]
fn variant_pages_are_up_to_date() {
    let regen = regen();
    if regen {
        std::fs::create_dir_all(pages_dir()).expect("create docs/variants");
        std::fs::write(index_path(), render_index()).expect("write index");
        for &vref in &VariantRef::ALL {
            std::fs::write(page_path(vref.name()), render_page(vref)).expect("write page");
        }
    }

    let index = render_index();
    // Normalize CRLF -> LF (Windows git checkout) so the diff is line-ending-agnostic.
    let committed_index = std::fs::read_to_string(index_path())
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert_eq!(
        committed_index, index,
        "docs/variants/README.md is out of date; regenerate with `REGEN=1 cargo test --test variant_pages_doc`",
    );

    for &vref in &VariantRef::ALL {
        let name = vref.name();
        let page = render_page(vref);
        let committed = std::fs::read_to_string(page_path(name))
            .unwrap_or_default()
            .replace("\r\n", "\n");
        assert_eq!(
            committed, page,
            "docs/variants/{name}.md is out of date; regenerate with `REGEN=1 cargo test --test variant_pages_doc`",
        );
    }
}

/// A structural sanity check: one page per variant plus the index, and every page
/// carries its own id and the required sections.
#[test]
fn every_variant_has_a_page() {
    for &vref in &VariantRef::ALL {
        let page = render_page(vref);
        assert!(
            page.contains(&format!("# {}", vref.name())),
            "page for {} missing its title",
            vref.name(),
        );
        for section in [
            "## Overview",
            "## Setup",
            "## Pieces & movement",
            "## Pawns",
            "## Promotion",
            "## Castling",
            "## Draws & terminal conditions",
            "## Special mechanics",
        ] {
            assert!(
                page.contains(section),
                "page for {} missing section {section}",
                vref.name(),
            );
        }
    }
    assert_eq!(VariantRef::ALL.len(), 114, "expected 114 variants");
}
