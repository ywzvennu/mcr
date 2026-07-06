//! A structured, authoritative [`VariantRules`] model **derived from the same
//! [`WideVariant`] hooks that drive move generation**, so it can never disagree
//! with the engine.
//!
//! Where `docs/variants.md` (and `tests/variants_doc.rs`) carry a hand-authored
//! *prose* description of each variant, this module produces a machine-checkable
//! **fact model**: every field is read out of the rule layer's own hooks (the
//! promotion / castling / draw / terminal predicates) or *sampled* from its
//! movement vocabulary ([`WideVariant::role_attacks`] and
//! [`WideVariant::quiet_only_targets`]) on an empty board, so a wrong value here
//! is a wrong value in the engine. Nothing is restated by hand.
//!
//! The entry point is [`WideVariantId::rules`](super::WideVariantId::rules), which
//! covers every one of [`WideVariantId::ALL`](super::WideVariantId::ALL). The model
//! is plain owned data (no borrows tied to a live position), so a later phase can
//! render it to markdown / JSON / an API without reshaping it.
//!
//! # Scope: the concrete 8x8 `VariantId` layer
//!
//! This model is derived from the [`WideVariant`] hook surface. The frozen concrete
//! 8x8 engine ([`crate::Variant`] / [`crate::VariantId`]: Standard, Chess960,
//! Atomic, Antichess, Crazyhouse, King-of-the-Hill, Three-check, Racing Kings,
//! Horde) is driven by a **separate** trait whose rule surface differs (a fixed
//! six-role army, bespoke terminals such as the atomic blast and the racing-kings
//! goal), so it is not folded into this model. Covering it belongs to its own
//! follow-up rather than forcing the concrete hooks into the wide-layer shape.
//!
//! # Deriving per-piece move vs capture geometry
//!
//! mcr encodes movement as *functions*, not declarations, so a piece's geometry is
//! recovered by **sampling**: [`role_attacks`](WideVariant::role_attacks) and
//! [`quiet_only_targets`](WideVariant::quiet_only_targets) are evaluated from every
//! square of an **empty** board and each reached square is reduced to a primitive
//! step direction (its `(file, rank)` delta divided by the delta's gcd), with a
//! `rides` flag set when the same direction is reached at distance two or more (a
//! slider / rider). Combined with the role-classification hooks
//! ([`role_is_slider`](WideVariant::role_is_slider),
//! [`role_attacks_are_capture_only`](WideVariant::role_attacks_are_capture_only),
//! [`uses_board_attacks`](WideVariant::uses_board_attacks)) this distinguishes a
//! plain slider from a riding leaper (Nightrider), a move≠capture piece (the Orda
//! Lancer / New Zealand Rook­ni), a screen hopper (Grasshopper, cannons) and a
//! whole-board attacker (the Janggi cannon). The pawn's forward move is generated
//! outside this vocabulary, so pawn movement is carried authoritatively by
//! [`PawnRules`] and the `movement` of the pawn role is left to its quiet hook.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use super::backing::BitboardBacking;
use super::position::GenericPosition;
use super::role::WideRole;
use super::variant::{ImpasseRule, WideCountingRule, WideVariant};
use super::{Bitboard, Board, Geometry, Square};
use crate::Color;

/// The complete, engine-derived ruleset of one variant: board, army, and the
/// pawn / promotion / castling / draw / terminal / special-mechanic rules, plus a
/// pointer to the validation oracle.
///
/// Every field is derived from the variant's [`WideVariant`] hooks (see the module
/// docs); none is hand-restated prose. Build one with
/// [`WideVariantId::rules`](super::WideVariantId::rules).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantRules {
    /// Board geometry and the starting position.
    pub board: BoardRules,
    /// The roles present on the starting board, each with its derived move and
    /// capture geometry. Reserves (Seirawan gate pieces, a Shogi hand) and
    /// promoted forms are not listed here — they surface through
    /// `promotion` and `mechanics` instead.
    pub army: Vec<PieceRules>,
    /// The pawn's movement rules (double step, sideways / backward, Berolina /
    /// Legan inversion, en passant).
    pub pawns: PawnRules,
    /// The promotion rule: target roles, the promotion zone, and whether it is
    /// forced.
    pub promotion: PromotionRules,
    /// The castling rule.
    pub castling: CastlingRules,
    /// The draw / repetition / counting adjudication rules, with their values.
    pub draw: DrawRules,
    /// The win / terminal conditions.
    pub terminal: TerminalRules,
    /// Special board mechanics (petrify, cannons, hand, duck, gating, …).
    pub mechanics: SpecialMechanics,
    /// The external oracle this variant's move generation is validated against.
    pub oracle: ValidationOracle,
}

/// Board geometry and the starting array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardRules {
    /// The number of files (board width), `G::WIDTH`.
    pub width: u8,
    /// The number of ranks (board height), `G::HEIGHT`.
    pub height: u8,
    /// The number of squares, `G::SQUARES`.
    pub square_count: u16,
    /// The bit width of the bitboard backing integer (`64`, `128`, or `256`).
    pub backing_bits: u32,
    /// The geometry marker type's short name (e.g. `"Chess8x8"`), the trailing
    /// path segment of its `type_name`.
    pub geometry: &'static str,
    /// The starting position in mcr's FEN dialect.
    pub start_fen: String,
}

/// One role's derived gameplay mechanics: how it moves and how it captures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PieceRules {
    /// The role.
    pub role: WideRole,
    /// The role's identifier name (its `WideRole` variant name, e.g. `"Nightrider"`).
    pub name: String,
    /// The role's lowercase FEN letter (overflow roles report the recycled base
    /// letter; the board FEN adds the `*` / `**` / `=` prefix).
    pub fen_char: char,
    /// Whether the role slides (its attack set is blocked along rays),
    /// [`WideVariant::role_is_slider`].
    pub is_slider: bool,
    /// The role's move/attack set is an occupancy-dependent **screen hop**
    /// (Grasshopper, a cannon's over-screen capture): its geometry is not visible on
    /// an empty board, so `movement` / `capture` steps are empty.
    pub hopper: bool,
    /// The role's move/attack set is computed from the **whole board**
    /// ([`WideVariant::uses_board_attacks`], the Janggi / Manchu cannon), not from a
    /// `(square, occupancy)` pair, so it is not sampled here.
    pub board_dependent: bool,
    /// Whether the derived move geometry differs from the derived capture geometry
    /// (a move≠capture piece such as the Orda Lancer or the New Zealand Rook­ni).
    pub move_neq_capture: bool,
    /// The non-capturing move geometry (see the module docs). For a pawn this is
    /// intentionally partial — the authoritative pawn move is in `pawns`.
    pub movement: Movement,
    /// The capture / threat geometry — the squares the role attacks
    /// ([`WideVariant::role_attacks`]), which is also its check / king-danger set.
    pub capture: Movement,
}

/// A derived movement geometry: a set of primitive step directions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Movement {
    /// The primitive directions reached on an empty board (White's orientation),
    /// each with a `rides` flag (a repeating slider / rider vs a single-step
    /// leaper). Empty when the owning piece is a `hopper` / `board_dependent`, is
    /// immobile, or has no move of this class.
    pub steps: Vec<Step>,
}

/// One primitive move direction: a `(file, rank)` delta with a `rides` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Step {
    /// The file delta (positive = toward the h-file), the primitive (gcd-reduced)
    /// component.
    pub file: i8,
    /// The rank delta (positive = forward, toward the far rank for White), the
    /// primitive component.
    pub rank: i8,
    /// Whether the piece repeats this step (a slider or riding leaper) rather than
    /// taking it exactly once (a leaper / stepper).
    pub rides: bool,
}

/// The pawn's movement rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PawnRules {
    /// The White ranks (0-based) a pawn may make its initial two-square advance
    /// from, [`WideVariant::pawn_may_double_push_from`].
    pub double_step_ranks: Vec<u8>,
    /// Whether a pawn may double-step from **every** rank (Torpedo chess).
    pub double_step_any_rank: bool,
    /// Whether the variant offers en passant, [`WideVariant::has_en_passant`].
    pub en_passant: bool,
    /// Whether a pawn may also step one square sideways
    /// ([`WideVariant::pawn_moves_sideways`]).
    pub moves_sideways: bool,
    /// Whether a pawn may also step one square backward
    /// ([`WideVariant::pawn_moves_backward`]).
    pub moves_backward: bool,
    /// Whether the pawn is a Berolina pawn — moves diagonally, captures straight
    /// ([`WideVariant::pawn_is_berolina`]).
    pub berolina: bool,
    /// Whether the pawn is a Legan pawn — a corner-diagonal directional pawn
    /// ([`WideVariant::pawn_is_legan`]).
    pub legan: bool,
    /// Whether the pawn is a forward stepper (the Shogi pawn),
    /// [`WideVariant::pawn_is_stepper`].
    pub stepper: bool,
    /// Whether an ordinary pawn move resets the move-count clock
    /// ([`WideVariant::pawn_move_resets_move_clock`]).
    pub move_resets_clock: bool,
}

/// The promotion rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionRules {
    /// The roles a promoting pawn may become, [`WideVariant::promotion_config`].
    pub roles: Vec<WideRole>,
    /// The White ranks (0-based) that form the promotion zone,
    /// [`WideVariant::in_promotion_zone`]. Usually a single rank; a region for Grand
    /// and the mandatory-far-zone shogi variants.
    pub zone_ranks: Vec<u8>,
    /// Whether promotion is forced on the furthest rank
    /// ([`WideVariant::promotion_is_forced`]).
    pub forced_on_last_rank: bool,
    /// Whether promotion is mandatory anywhere in the zone
    /// ([`WideVariant::promotion_mandatory_in_zone`], the shogi far-zone rule).
    pub mandatory_in_zone: bool,
    /// Whether the variant uses the Chu-Shogi lion-style promotion rule
    /// ([`WideVariant::lion_style_promotion`]).
    pub lion_style: bool,
    /// Whether non-pawn pieces promote by ending in the zone without a hand
    /// ([`WideVariant::has_piece_promotion`], Chak's King → Divine Lord).
    pub piece_promotion_no_hand: bool,
}

/// The castling rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastlingRules {
    /// Whether the variant offers castling, [`WideVariant::has_castling`].
    pub enabled: bool,
    /// The role that castles as the kingside "rook"
    /// ([`WideVariant::castle_rook_role`] for side `0`).
    pub rook_role_kingside: WideRole,
    /// The role that castles as the queenside "rook"
    /// ([`WideVariant::castle_rook_role`] for side `1`).
    pub rook_role_queenside: WideRole,
    /// The 0-based rank White castles along, [`WideVariant::castle_rank`].
    pub castle_rank_white: u8,
    /// The file the king lands on castling kingside
    /// ([`WideVariant::castle_dest_files`] side `0`).
    pub king_dest_kingside: u8,
    /// The file the king lands on castling queenside
    /// ([`WideVariant::castle_dest_files`] side `1`).
    pub king_dest_queenside: u8,
}

/// The draw / repetition / counting adjudication rules, with their values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawRules {
    /// The move-count ("fifty-move") draw ply threshold, if any
    /// ([`WideVariant::move_rule_plies`]).
    pub move_rule_plies: Option<u16>,
    /// Whether a position history is kept for repetition rules
    /// ([`WideVariant::tracks_repetition`]).
    pub tracks_repetition: bool,
    /// The repetition fold count that draws / adjudicates
    /// ([`WideVariant::repetition_fold`]); meaningful only when
    /// `tracks_repetition`.
    pub repetition_fold: usize,
    /// The counting endgame rule, if any ([`WideVariant::counting_rule`]).
    pub counting_rule: Option<WideCountingRule>,
    /// The impasse / jishogi declaration rule, if any
    /// ([`WideVariant::impasse_rule`]).
    pub impasse: Option<ImpasseInfo>,
    /// Whether the two generals facing on an open line draws (Janggi bikjang),
    /// [`WideVariant::has_bikjang`].
    pub has_bikjang: bool,
    /// Whether stalemate is a loss for the stalemated side
    /// ([`WideVariant::stalemate_is_loss`]).
    pub stalemate_is_loss: bool,
    /// Whether reducing a side to a lone king is an immediate draw (Shatar Robado),
    /// [`WideVariant::has_bare_king_draw`].
    pub has_bare_king_draw: bool,
    /// Whether baring a side's king is a loss (Shatranj),
    /// [`WideVariant::has_bare_king_loss`].
    pub has_bare_king_loss: bool,
    /// Whether perpetual check loses for the checker
    /// ([`WideVariant::perpetual_check_loses`]).
    pub perpetual_check_loses: bool,
    /// Whether perpetual chase loses for the chaser (Xiangqi),
    /// [`WideVariant::perpetual_chase_loses`].
    pub perpetual_chase_loses: bool,
    /// Whether one-sided attack repetition loses (Chu / Dai large-shogi chase),
    /// [`WideVariant::attack_repetition_loses`].
    pub attack_repetition_loses: bool,
}

/// The impasse / jishogi (entering-king) declaration parameters, an owned copy of
/// [`ImpasseRule`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpasseInfo {
    /// The minimum own pieces required inside the promotion zone.
    pub min_pieces_in_zone: u32,
    /// The first player's (White / Sente) point threshold.
    pub sente_threshold: u32,
    /// The second player's (Black / Gote) point threshold.
    pub gote_threshold: u32,
    /// The point value of a "big" piece (Rook / Bishop and their promotions).
    pub big_piece_points: u32,
    /// The point value of every other counted piece.
    pub small_piece_points: u32,
    /// The roles scored at the big-piece value.
    pub big_roles: Vec<WideRole>,
}

/// The win / terminal conditions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalRules {
    /// How the king's royalty is treated (checkmate / non-royal / pseudo-royal).
    pub royal: RoyalRule,
    /// The extinction terminal, if any ([`WideVariant::extinction_rule`]).
    pub extinction: Option<ExtinctionInfo>,
    /// The flag / campmate win (a king reaching the far rank), if any
    /// ([`WideVariant::has_flag_win`]).
    pub flag_win: Option<FlagWin>,
    /// Whether stalemate is a loss (mirrors the draw-rules field).
    pub stalemate_is_loss: bool,
    /// Whether giving check wins the game outright (Checkshogi),
    /// [`WideVariant::wins_on_check`].
    pub wins_on_check: bool,
    /// Whether moving a Divine Lord onto the enemy temple wins (Chak),
    /// [`WideVariant::has_temple_win`].
    pub temple_win: bool,
    /// Whether reducing a side to a bare king draws (mirrors the draw-rules field).
    pub bare_king_draw: bool,
    /// Whether baring a side's king loses (mirrors the draw-rules field).
    pub bare_king_loss: bool,
}

/// How a variant treats royalty and the decisive check terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoyalRule {
    /// A single royal king; a side loses by checkmate.
    Checkmate,
    /// The king is a non-royal piece — there is no check; a side loses by
    /// extinction / king capture ([`WideVariant::non_royal_king`]).
    NonRoyal,
    /// Several royal pieces; a side is in check only when **every** royal is
    /// attacked and may sacrifice one and play on (Spartan duple-check;
    /// [`WideVariant::multi_royal`] with the survivor rule).
    MultiRoyalAnySurvives,
    /// Pseudo-royal: a move must leave **every** royal safe
    /// ([`WideVariant::royals_all_must_survive`], Chak's King + Divine Lord).
    PseudoRoyalAllSurvive,
}

/// The extinction terminal parameters, an owned copy of
/// [`ExtinctionRule`](super::variant::ExtinctionRule).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtinctionInfo {
    /// The roles whose disappearance ends the game.
    pub watched: Vec<WideRole>,
    /// The count at or below which a watched role is extinct for its side.
    pub threshold: usize,
}

/// The flag / campmate win parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlagWin {
    /// The 0-based rank White wins by reaching, [`WideVariant::flag_rank`].
    pub rank_white: u8,
    /// Whether the king on the goal rank must also be safe (Dobutsu's "try"),
    /// [`WideVariant::flag_win_requires_safe`].
    pub requires_safe: bool,
}

/// Special board mechanics that fall outside the movement / terminal categories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecialMechanics {
    /// Every move is verified by make/unmake king-safety re-test (a riding-leaper
    /// check geometry), [`WideVariant::needs_full_verify`].
    pub needs_full_verify: bool,
    /// The petrify-on-capture mechanic ([`WideVariant::has_petrify`]).
    pub has_petrify: bool,
    /// The roles that turn to stone when they capture
    /// ([`WideVariant::role_petrifies`]).
    pub petrifying_roles: Vec<WideRole>,
    /// The pseudo-royal piece may not capture (petrified chess),
    /// [`WideVariant::royal_cannot_capture`].
    pub royal_cannot_capture: bool,
    /// The variant fields cannons ([`WideVariant::has_cannons`]).
    pub has_cannons: bool,
    /// Some role's attack set is computed from the whole board
    /// ([`WideVariant::uses_board_attacks`]).
    pub uses_board_attacks: bool,
    /// The Xiangqi flying-general rule ([`WideVariant::has_flying_general`]).
    pub has_flying_general: bool,
    /// The variant has a persistent hand and drops ([`WideVariant::has_hand`]).
    pub has_hand: bool,
    /// The variant has a setup / placement phase ([`WideVariant::has_placement`]).
    pub has_placement: bool,
    /// The variant supports Seirawan gating ([`WideVariant::supports_gating`]).
    pub supports_gating: bool,
    /// The variant has the neutral Duck ([`WideVariant::has_duck`]).
    pub has_duck: bool,
    /// The variant is Alice chess ([`WideVariant::is_alice`]).
    pub is_alice: bool,
    /// The variant grants a one-time first-move leap (Cambodian),
    /// [`WideVariant::has_first_move_leaps`].
    pub has_first_move_leaps: bool,
    /// The variant fields full Chu-Shogi Lion moves
    /// ([`WideVariant::has_lion_moves`]).
    pub has_lion_moves: bool,
    /// The variant fields a Tenjiku Fire Demon area burn
    /// ([`WideVariant::has_area_burn`]).
    pub has_area_burn: bool,
    /// The variant fields Tenjiku range-jumping generals
    /// ([`WideVariant::has_jump_captures`]).
    pub has_jump_captures: bool,
    /// The variant lets a side pass the turn (Janggi),
    /// [`WideVariant::allows_pass`].
    pub allows_pass: bool,
    /// Pinned leapers are confined to the king–pinner segment
    /// ([`WideVariant::confine_pins_to_segment`]).
    pub confine_pins_to_segment: bool,
}

/// The external oracle a variant's move generation is validated against — a
/// structured pointer, not prose. Mirrors the `tests/coverage_gate.rs` manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationOracle {
    /// Cross-checked node-for-node against Fairy-Stockfish `UCI_Variant <name>`.
    /// The name is FSF's spelling, which may differ from mcr's canonical name.
    FairyStockfish(&'static str),
    /// Cross-checked against the HaChu large-shogi reference engine.
    HaChu,
    /// No external engine oracle: pinned by an independent in-repo generator or
    /// hand-derived counts (Alice, Jieqi, Tenjiku, Wa Shogi).
    Independent,
}

// --- derivation ----------------------------------------------------------------

/// Derives the [`VariantRules`] of the wide variant `V` over geometry `G` from its
/// hooks. The `oracle` is left as [`ValidationOracle::Independent`] and filled in by
/// [`WideVariantId::rules`](super::WideVariantId::rules), which knows the variant's
/// identity.
pub(crate) fn derive_rules<G: Geometry, V: WideVariant<G>>() -> VariantRules {
    let pos = GenericPosition::<G, V>::startpos();
    let board = pos.board();

    VariantRules {
        board: derive_board::<G, V>(&pos),
        army: derive_army::<G, V>(board),
        pawns: derive_pawns::<G, V>(),
        promotion: derive_promotion::<G, V>(),
        castling: derive_castling::<G, V>(),
        draw: derive_draw::<G, V>(),
        terminal: derive_terminal::<G, V>(),
        mechanics: derive_mechanics::<G, V>(),
        oracle: ValidationOracle::Independent,
    }
}

fn derive_board<G: Geometry, V: WideVariant<G>>(pos: &GenericPosition<G, V>) -> BoardRules {
    let geometry = core::any::type_name::<G>()
        .rsplit("::")
        .next()
        .unwrap_or("");
    BoardRules {
        width: G::WIDTH,
        height: G::HEIGHT,
        square_count: G::SQUARES,
        backing_bits: <G::Bits as BitboardBacking>::BITS,
        geometry,
        start_fen: pos.to_fen(),
    }
}

fn derive_army<G: Geometry, V: WideVariant<G>>(board: &Board<G>) -> Vec<PieceRules> {
    let span = V::ROLE_SPAN.min(WideRole::COUNT);
    WideRole::ALL[..span]
        .iter()
        .filter(|&&role| !board.by_role(role).is_empty())
        .map(|&role| derive_piece::<G, V>(role, board))
        .collect()
}

/// The roles whose non-capturing move is generated outside the
/// [`role_attacks`](WideVariant::role_attacks) / [`quiet_only_targets`](WideVariant::quiet_only_targets)
/// vocabulary (their `role_attacks` set is their *capture* only), so their sampled
/// `movement` must not fold in the attack squares.
fn move_is_generated_elsewhere(role: WideRole) -> bool {
    matches!(role, WideRole::Pawn | WideRole::Hoplite)
}

fn derive_piece<G: Geometry, V: WideVariant<G>>(role: WideRole, board: &Board<G>) -> PieceRules {
    let capture_only = V::role_attacks_are_capture_only(role);

    // Sample the attack (capture / threat) set and the quiet-only set on an empty
    // board, from every square, reducing each reached square to a primitive step.
    let attack =
        sample_steps::<G, _>(|sq| V::role_attacks(role, Color::White, sq, Bitboard::EMPTY));
    let quiet =
        sample_steps::<G, _>(|sq| V::quiet_only_targets(role, Color::White, sq, Bitboard::EMPTY));

    let (movement, capture) = if capture_only {
        // The role captures along its `role_attacks` set and moves via its
        // quiet-only set (Orda Lancer / Archer, New Zealand Rookni, Empire pieces).
        (Movement { steps: quiet }, Movement { steps: attack })
    } else if move_is_generated_elsewhere(role) {
        // A pawn-family role: `role_attacks` is its capture; the quiet forward move
        // is generated separately (see `pawns`).
        (Movement { steps: quiet }, Movement { steps: attack })
    } else {
        // An ordinary piece: its `role_attacks` squares are both moves and
        // captures; `quiet_only_targets` adds any move-only squares.
        let mut moves = sample_steps::<G, _>(|sq| {
            V::role_attacks(role, Color::White, sq, Bitboard::EMPTY)
                | V::quiet_only_targets(role, Color::White, sq, Bitboard::EMPTY)
        });
        moves.sort_unstable();
        (Movement { steps: moves }, Movement { steps: attack })
    };

    // Occupancy dependence: a whole-board attacker (Janggi cannon) or a screen
    // hopper (Grasshopper, cannon capture) has no empty-board geometry.
    let board_dependent = V::uses_board_attacks()
        && center_square::<G>()
            .and_then(|c| V::role_attacks_board(role, Color::White, c, board))
            .is_some();
    let hopper = !board_dependent
        && movement.steps.is_empty()
        && capture.steps.is_empty()
        && has_screen_move::<G, V>(role);

    PieceRules {
        move_neq_capture: movement != capture,
        role,
        name: alloc::format!("{role:?}"),
        fen_char: role.char(),
        is_slider: V::role_is_slider(role),
        hopper,
        board_dependent,
        movement,
        capture,
    }
}

/// Samples a target-set function from every square of an empty board and returns
/// the primitive step directions it reaches, sorted, each with a `rides` flag set
/// when the direction is reached at distance two or more.
fn sample_steps<G: Geometry, F: Fn(Square<G>) -> Bitboard<G>>(targets: F) -> Vec<Step> {
    let mut dirs: BTreeMap<(i8, i8), bool> = BTreeMap::new();
    for from in Bitboard::<G>::FULL {
        for to in targets(from) {
            let df = to.file() as i8 - from.file() as i8;
            let dr = to.rank() as i8 - from.rank() as i8;
            if df == 0 && dr == 0 {
                continue;
            }
            let g = gcd(df.unsigned_abs(), dr.unsigned_abs()) as i8;
            let prim = (df / g, dr / g);
            let rides = g >= 2;
            let entry = dirs.entry(prim).or_insert(false);
            *entry = *entry || rides;
        }
    }
    dirs.into_iter()
        .map(|((file, rank), rides)| Step { file, rank, rides })
        .collect()
}

/// The greatest common divisor of two non-negative values (`gcd(a, 0) == a`).
fn gcd(mut a: u8, mut b: u8) -> u8 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a.max(1)
}

/// The central square of the board, used as the sample point for occupancy probes.
fn center_square<G: Geometry>() -> Option<Square<G>> {
    Square::from_file_rank(G::WIDTH / 2, G::HEIGHT / 2)
}

/// Returns `true` if the role gains any move or capture once a ring of screens is
/// placed on the eight squares adjacent to the board centre — the general test that
/// distinguishes a screen hopper (Grasshopper, cannon) from an immobile piece
/// (Chak Temple) when the empty-board sample is empty.
fn has_screen_move<G: Geometry, V: WideVariant<G>>(role: WideRole) -> bool {
    let Some(center) = center_square::<G>() else {
        return false;
    };
    let mut occ = Bitboard::<G>::EMPTY;
    for (df, dr) in [
        (1, 0),
        (-1, 0),
        (0, 1),
        (0, -1),
        (1, 1),
        (1, -1),
        (-1, 1),
        (-1, -1),
    ] {
        let f = center.file() as i8 + df;
        let r = center.rank() as i8 + dr;
        if let (Ok(f), Ok(r)) = (u8::try_from(f), u8::try_from(r)) {
            if let Some(sq) = Square::<G>::from_file_rank(f, r) {
                occ |= Bitboard::from_square(sq);
            }
        }
    }
    !V::role_attacks(role, Color::White, center, occ).is_empty()
        || !V::quiet_only_targets(role, Color::White, center, occ).is_empty()
}

fn derive_pawns<G: Geometry, V: WideVariant<G>>() -> PawnRules {
    let mut double_step_ranks = Vec::new();
    for rank in 0..G::HEIGHT {
        if V::pawn_may_double_push_from(rank, Color::White) {
            double_step_ranks.push(rank);
        }
    }
    let double_step_any_rank = double_step_ranks.len() == G::HEIGHT as usize && G::HEIGHT > 0;
    PawnRules {
        double_step_ranks,
        double_step_any_rank,
        en_passant: V::has_en_passant(),
        moves_sideways: V::pawn_moves_sideways(),
        moves_backward: V::pawn_moves_backward(),
        berolina: V::pawn_is_berolina(),
        legan: V::pawn_is_legan(),
        stepper: V::pawn_is_stepper(),
        move_resets_clock: V::pawn_move_resets_move_clock(),
    }
}

fn derive_promotion<G: Geometry, V: WideVariant<G>>() -> PromotionRules {
    let mut zone_ranks = Vec::new();
    for rank in 0..G::HEIGHT {
        if V::in_promotion_zone(Color::White, rank) {
            zone_ranks.push(rank);
        }
    }
    PromotionRules {
        roles: V::promotion_config().roles,
        zone_ranks,
        forced_on_last_rank: V::promotion_is_forced(Color::White, V::promotion_rank(Color::White)),
        mandatory_in_zone: V::promotion_mandatory_in_zone(),
        lion_style: V::lion_style_promotion(),
        piece_promotion_no_hand: V::has_piece_promotion(),
    }
}

fn derive_castling<G: Geometry, V: WideVariant<G>>() -> CastlingRules {
    CastlingRules {
        enabled: V::has_castling(),
        rook_role_kingside: V::castle_rook_role(0),
        rook_role_queenside: V::castle_rook_role(1),
        castle_rank_white: V::castle_rank(Color::White),
        king_dest_kingside: V::castle_dest_files(0).0,
        king_dest_queenside: V::castle_dest_files(1).0,
    }
}

fn derive_draw<G: Geometry, V: WideVariant<G>>() -> DrawRules {
    DrawRules {
        move_rule_plies: V::move_rule_plies(),
        tracks_repetition: V::tracks_repetition(),
        repetition_fold: V::repetition_fold(),
        counting_rule: V::counting_rule(),
        impasse: V::impasse_rule().map(impasse_info),
        has_bikjang: V::has_bikjang(),
        stalemate_is_loss: V::stalemate_is_loss(),
        has_bare_king_draw: V::has_bare_king_draw(),
        has_bare_king_loss: V::has_bare_king_loss(),
        perpetual_check_loses: V::perpetual_check_loses(),
        perpetual_chase_loses: V::perpetual_chase_loses(),
        attack_repetition_loses: V::attack_repetition_loses(),
    }
}

fn impasse_info(rule: ImpasseRule) -> ImpasseInfo {
    ImpasseInfo {
        min_pieces_in_zone: rule.min_pieces_in_zone,
        sente_threshold: rule.sente_threshold,
        gote_threshold: rule.gote_threshold,
        big_piece_points: rule.big_piece_points,
        small_piece_points: rule.small_piece_points,
        big_roles: rule.big_roles.to_vec(),
    }
}

fn derive_terminal<G: Geometry, V: WideVariant<G>>() -> TerminalRules {
    let royal = if V::non_royal_king() {
        RoyalRule::NonRoyal
    } else if V::multi_royal() {
        if V::royals_all_must_survive() {
            RoyalRule::PseudoRoyalAllSurvive
        } else {
            RoyalRule::MultiRoyalAnySurvives
        }
    } else {
        RoyalRule::Checkmate
    };
    let extinction = V::extinction_rule().map(|rule| ExtinctionInfo {
        watched: rule.watched.to_vec(),
        threshold: rule.threshold,
    });
    let flag_win = V::has_flag_win().then(|| FlagWin {
        rank_white: V::flag_rank(Color::White),
        requires_safe: V::flag_win_requires_safe(),
    });
    TerminalRules {
        royal,
        extinction,
        flag_win,
        stalemate_is_loss: V::stalemate_is_loss(),
        wins_on_check: V::wins_on_check(),
        temple_win: V::has_temple_win(),
        bare_king_draw: V::has_bare_king_draw(),
        bare_king_loss: V::has_bare_king_loss(),
    }
}

fn derive_mechanics<G: Geometry, V: WideVariant<G>>() -> SpecialMechanics {
    let span = V::ROLE_SPAN.min(WideRole::COUNT);
    let petrifying_roles = if V::has_petrify() {
        WideRole::ALL[..span]
            .iter()
            .copied()
            .filter(|&role| V::role_petrifies(role))
            .collect()
    } else {
        Vec::new()
    };
    SpecialMechanics {
        needs_full_verify: V::needs_full_verify(),
        has_petrify: V::has_petrify(),
        petrifying_roles,
        royal_cannot_capture: V::royal_cannot_capture(),
        has_cannons: V::has_cannons(),
        uses_board_attacks: V::uses_board_attacks(),
        has_flying_general: V::has_flying_general(),
        has_hand: V::has_hand(),
        has_placement: V::has_placement(),
        supports_gating: V::supports_gating(),
        has_duck: V::has_duck(),
        is_alice: V::is_alice(),
        has_first_move_leaps: V::has_first_move_leaps(),
        has_lion_moves: V::has_lion_moves(),
        has_area_burn: V::has_area_burn(),
        has_jump_captures: V::has_jump_captures(),
        allows_pass: V::allows_pass(),
        confine_pins_to_segment: V::confine_pins_to_segment(),
    }
}
