//! The [`VariantRules`] derivation for the concrete 8x8 [`VariantId`] variants —
//! the concrete-layer counterpart of the wide layer's `derive_rules`.
//!
//! The concrete engine ([`Variant`] / [`VariantId`]) is a **separate** trait
//! surface from the wide [`WideVariant`](crate::geometry::WideVariant) layer: a
//! fixed six-role army on the frozen 8x8 board, with bespoke terminals (the atomic
//! blast, the antichess losing-win, the racing-kings goal) that the wide hook set
//! cannot express. This module derives the same [`VariantRules`] *shape* for those
//! nine variants so a single unified catalog
//! ([`VariantRef`](crate::VariantRef)) can span both families.
//!
//! Every field is read from the variant's own [`Variant`] hooks where one exists
//! ([`Variant::promotion_roles`], [`Variant::castling_allowed`],
//! [`Variant::castle_geometry`], [`Variant::king_is_royal`],
//! [`Variant::starting_board`], and the start FEN of
//! [`VariantPosition::startpos`]) or sampled from the concrete attack tables (the
//! per-piece move / capture geometry), so it can never disagree with the engine.
//! The handful of facts the concrete trait bakes into the shared core rather than
//! exposing as a hook — the atomic blast, forced captures, the check count, the
//! hill squares, the racing goal, horde's asymmetry — are set from the variant's
//! stable [`Variant::ID`], each mirroring exactly one concrete rule module.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use super::{
    AntichessRules, AtomicRules, Chess960Rules, ChessRules, CrazyhouseRules, HordeRules,
    KingOfTheHillRules, RacingKingsRules, ThreeCheckRules, Variant, VariantId, VariantPosition,
};
use crate::attacks::{
    bishop_attacks, king_attacks, knight_attacks, pawn_attacks, queen_attacks, rook_attacks,
};
use crate::geometry::rules::{
    BoardRules, CastlingRules, DrawRules, FlagWin, Movement, PawnRules, PieceRules, PromotionRules,
    RegionWin, RoyalRule, SpecialMechanics, Step, TerminalRules, ValidationOracle, VariantRules,
};
use crate::geometry::WideRole;
use crate::{Bitboard, Board, CastleSide, Color, Role, Square};

impl VariantId {
    /// The structured, engine-derived [`VariantRules`](crate::geometry::VariantRules)
    /// for this concrete 8x8 variant: its board, army (with per-piece move / capture
    /// geometry), and pawn / promotion / castling / draw / terminal / special-mechanic
    /// rules, plus the validation oracle.
    ///
    /// The concrete-layer counterpart of
    /// [`WideVariantId::rules`](crate::geometry::WideVariantId::rules). Every field
    /// is derived from the variant's [`Variant`] hooks or sampled from the concrete
    /// attack tables, so it can never disagree with move generation. Available for
    /// every
    /// [`VariantId::ALL`](VariantId::ALL) without panicking. The single unified
    /// entry point spanning this family and the wide one is
    /// [`VariantRef::rules`](crate::VariantRef::rules).
    #[must_use]
    pub fn rules(self) -> VariantRules {
        match self {
            VariantId::Standard => derive_concrete::<ChessRules>(),
            VariantId::Chess960 => derive_concrete::<Chess960Rules>(),
            VariantId::Atomic => derive_concrete::<AtomicRules>(),
            VariantId::Antichess => derive_concrete::<AntichessRules>(),
            VariantId::Crazyhouse => derive_concrete::<CrazyhouseRules>(),
            VariantId::KingOfTheHill => derive_concrete::<KingOfTheHillRules>(),
            VariantId::ThreeCheck => derive_concrete::<ThreeCheckRules>(),
            VariantId::RacingKings => derive_concrete::<RacingKingsRules>(),
            VariantId::Horde => derive_concrete::<HordeRules>(),
        }
    }
}

/// Derives the [`VariantRules`] of the concrete variant `V` from its [`Variant`]
/// hooks and the concrete attack tables.
fn derive_concrete<V: Variant + Default>() -> VariantRules {
    let id = V::ID;
    let (board, _rights, _state) = V::starting_board();
    VariantRules {
        board: derive_board::<V>(),
        army: derive_army(&board),
        pawns: concrete_pawns(id),
        promotion: concrete_promotion::<V>(id),
        castling: concrete_castling::<V>(),
        draw: concrete_draw(),
        terminal: concrete_terminal::<V>(id),
        mechanics: concrete_mechanics(id),
        oracle: concrete_oracle(id),
    }
}

/// The concrete board is always the frozen standard 8x8 geometry on a `u64`
/// bitboard; only the starting array (the FEN) varies by variant.
fn derive_board<V: Variant + Default>() -> BoardRules {
    BoardRules {
        width: 8,
        height: 8,
        square_count: 64,
        backing_bits: 64,
        geometry: "Chess8x8",
        start_fen: VariantPosition::<V>::startpos().to_fen(),
    }
}

/// The roles present on the starting board, each with its derived move / capture
/// geometry, in the standard `WideRole` order (pawn … king).
fn derive_army(board: &Board) -> Vec<PieceRules> {
    Role::ALL
        .iter()
        .filter(|&&role| !board.by_role(role).is_empty())
        .map(|&role| derive_piece(role))
        .collect()
}

/// One standard role's derived gameplay geometry, sampled from the concrete attack
/// tables on an empty board.
fn derive_piece(role: Role) -> PieceRules {
    let attack = role_attack_steps(role);
    let is_slider = matches!(role, Role::Bishop | Role::Rook | Role::Queen);
    // A pawn's forward move is generated by the core outside the attack vocabulary,
    // so (like the wide model) its `movement` is left empty and carried
    // authoritatively by `pawns`; every other standard role moves where it captures.
    let (movement, capture) = if role == Role::Pawn {
        (Movement::default(), Movement { steps: attack })
    } else {
        (
            Movement {
                steps: attack.clone(),
            },
            Movement { steps: attack },
        )
    };
    let wide = role_to_wide(role);
    PieceRules {
        move_neq_capture: movement != capture,
        role: wide,
        name: format!("{wide:?}"),
        fen_char: wide.char(),
        board_token: format!("{}{}", wide.board_token_prefix(), wide.char()),
        is_slider,
        hopper: false,
        board_dependent: false,
        movement,
        capture,
    }
}

/// The primitive step directions a standard role attacks, sampled from the
/// concrete attack table over an empty board (White's orientation).
fn role_attack_steps(role: Role) -> Vec<Step> {
    match role {
        Role::Pawn => sample_steps(|sq| pawn_attacks(Color::White, sq)),
        Role::Knight => sample_steps(knight_attacks),
        Role::Bishop => sample_steps(|sq| bishop_attacks(sq, Bitboard::EMPTY)),
        Role::Rook => sample_steps(|sq| rook_attacks(sq, Bitboard::EMPTY)),
        Role::Queen => sample_steps(|sq| queen_attacks(sq, Bitboard::EMPTY)),
        Role::King => sample_steps(king_attacks),
    }
}

/// Samples a target-set function from every square of the empty 8x8 board and
/// returns the primitive step directions it reaches, each with a `rides` flag set
/// when the direction is reached at distance two or more — the concrete-board twin
/// of the wide layer's sampler.
fn sample_steps<F: Fn(Square) -> Bitboard>(targets: F) -> Vec<Step> {
    let mut dirs: BTreeMap<(i8, i8), bool> = BTreeMap::new();
    for from in Bitboard::FULL {
        for to in targets(from) {
            let df = to.file().index() as i8 - from.file().index() as i8;
            let dr = to.rank().index() as i8 - from.rank().index() as i8;
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

/// Maps a concrete [`Role`] to its [`WideRole`] counterpart; the standard six
/// share the leading indices `0..6`, so the two enums agree on them.
fn role_to_wide(role: Role) -> WideRole {
    match role {
        Role::Pawn => WideRole::Pawn,
        Role::Knight => WideRole::Knight,
        Role::Bishop => WideRole::Bishop,
        Role::Rook => WideRole::Rook,
        Role::Queen => WideRole::Queen,
        Role::King => WideRole::King,
    }
}

/// The pawn rules of a concrete variant: standard chess pawns, save for Horde's
/// first-rank double push and pawnless Racing Kings.
fn concrete_pawns(id: VariantId) -> PawnRules {
    let base = PawnRules {
        double_step_ranks: vec![1],
        double_step_any_rank: false,
        en_passant: true,
        moves_sideways: false,
        moves_backward: false,
        berolina: false,
        legan: false,
        stepper: false,
        move_resets_clock: true,
    };
    match id {
        // White's horde may also double-push from the first rank (0-based rank 0).
        VariantId::Horde => PawnRules {
            double_step_ranks: vec![0, 1],
            ..base
        },
        // Racing Kings is pawnless: no double step, no en passant.
        VariantId::RacingKings => PawnRules {
            double_step_ranks: Vec::new(),
            en_passant: false,
            ..base
        },
        _ => base,
    }
}

/// The promotion rule: the target roles come from the variant's own
/// [`Variant::promotion_roles`] hook (antichess adds the king); the zone is the
/// far rank for every pawn variant, empty for pawnless Racing Kings.
fn concrete_promotion<V: Variant>(id: VariantId) -> PromotionRules {
    let roles = V::promotion_roles()
        .iter()
        .map(|&r| role_to_wide(r))
        .collect();
    let (zone_ranks, forced_on_last_rank) = if id == VariantId::RacingKings {
        (Vec::new(), false)
    } else {
        (vec![7], true)
    };
    PromotionRules {
        roles,
        zone_ranks,
        forced_on_last_rank,
        mandatory_in_zone: false,
        lion_style: false,
        piece_promotion_no_hand: false,
    }
}

/// The castling rule, read from [`Variant::castling_allowed`] and
/// [`Variant::castle_geometry`]. The concrete rook is always the standard Rook and
/// White castles on the first rank (0-based rank 0).
fn concrete_castling<V: Variant>() -> CastlingRules {
    let king_dest = |side| V::castle_geometry(Color::White, side).map(|g| g.king_dest_file.index());
    CastlingRules {
        enabled: V::castling_allowed(),
        rook_role_kingside: WideRole::Rook,
        rook_role_queenside: WideRole::Rook,
        castle_rank_white: 0,
        king_dest_kingside: king_dest(CastleSide::King).unwrap_or(6),
        king_dest_queenside: king_dest(CastleSide::Queen).unwrap_or(2),
    }
}

/// The shared concrete draw rules: the fifty-move claim (100 plies), automatic
/// fivefold repetition, and no fairy counting / impasse / perpetual rules.
fn concrete_draw() -> DrawRules {
    DrawRules {
        move_rule_plies: Some(100),
        tracks_repetition: true,
        repetition_fold: 5,
        counting_rule: None,
        impasse: None,
        has_bikjang: false,
        stalemate_is_loss: false,
        stalemate_is_win: false,
        stalemate_piece_count: false,
        has_bare_king_draw: false,
        has_bare_king_loss: false,
        perpetual_check_loses: false,
        perpetual_chase_loses: false,
        attack_repetition_loses: false,
    }
}

/// The terminal / win conditions: the ordinary royal / checkmate rule
/// ([`Variant::king_is_royal`]) plus each variant's bespoke win, keyed off its
/// [`Variant::ID`].
fn concrete_terminal<V: Variant>(id: VariantId) -> TerminalRules {
    let royal = if V::king_is_royal() {
        RoyalRule::Checkmate
    } else {
        RoyalRule::NonRoyal
    };
    let mut terminal = TerminalRules {
        royal,
        extinction: None,
        flag_win: None,
        stalemate_is_loss: false,
        checkmate_is_win: false,
        wins_on_check: false,
        temple_win: false,
        bare_king_draw: false,
        bare_king_loss: false,
        explosion_win: false,
        lose_all_wins: false,
        all_pieces_lost_loses: false,
        check_count_to_win: None,
        region_win: None,
    };
    match id {
        VariantId::Atomic => terminal.explosion_win = true,
        VariantId::Antichess => terminal.lose_all_wins = true,
        VariantId::Horde => terminal.all_pieces_lost_loses = true,
        VariantId::ThreeCheck => {
            // A repeated-check win: three delivered checks win the game.
            terminal.wins_on_check = true;
            terminal.check_count_to_win = Some(3);
        }
        VariantId::KingOfTheHill => {
            // The four central squares d4/e4/d5/e5 as 0-based (file, rank) pairs.
            terminal.region_win = Some(RegionWin {
                squares: vec![(3, 3), (4, 3), (3, 4), (4, 4)],
            });
        }
        VariantId::RacingKings => {
            // The race goal: a king reaching the eighth rank (0-based rank 7).
            terminal.flag_win = Some(FlagWin {
                rank_white: 7,
                requires_safe: false,
            });
        }
        _ => {}
    }
    terminal
}

/// The special mechanics: every fairy mechanic is off; the concrete-only mechanics
/// are set by the variant's [`Variant::ID`] (Crazyhouse reuses the wide `has_hand`
/// flag for its drops).
fn concrete_mechanics(id: VariantId) -> SpecialMechanics {
    let mut mechanics = SpecialMechanics {
        needs_full_verify: false,
        has_petrify: false,
        petrifying_roles: Vec::new(),
        royal_cannot_capture: false,
        has_cannons: false,
        uses_board_attacks: false,
        has_flying_general: false,
        has_hand: false,
        has_placement: false,
        supports_gating: false,
        has_duck: false,
        is_alice: false,
        has_first_move_leaps: false,
        has_lion_moves: false,
        has_area_burn: false,
        has_jump_captures: false,
        allows_pass: false,
        confine_pins_to_segment: false,
        atomic_blast: false,
        mandatory_captures: false,
        checks_forbidden: false,
        asymmetric_armies: false,
        shuffled_setup: false,
    };
    match id {
        VariantId::Atomic => mechanics.atomic_blast = true,
        VariantId::Antichess => mechanics.mandatory_captures = true,
        // Captured pieces go to hand and can be dropped — the wide hand mechanic.
        VariantId::Crazyhouse => mechanics.has_hand = true,
        VariantId::RacingKings => mechanics.checks_forbidden = true,
        VariantId::Horde => mechanics.asymmetric_armies = true,
        VariantId::Chess960 => mechanics.shuffled_setup = true,
        _ => {}
    }
    mechanics
}

/// The external validation oracle: every concrete variant is cross-checked against
/// Fairy-Stockfish, under its FSF `UCI_Variant` spelling.
fn concrete_oracle(id: VariantId) -> ValidationOracle {
    ValidationOracle::FairyStockfish(match id {
        VariantId::Standard => "chess",
        VariantId::Chess960 => "chess960",
        VariantId::Atomic => "atomic",
        VariantId::Antichess => "giveaway",
        VariantId::Crazyhouse => "crazyhouse",
        VariantId::KingOfTheHill => "kingofthehill",
        VariantId::ThreeCheck => "3check",
        VariantId::RacingKings => "racingkings",
        VariantId::Horde => "horde",
    })
}
