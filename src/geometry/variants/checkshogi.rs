//! Checkshogi (Check Shogi, 9x9) on the generic engine — **standard 9x9 Shogi**
//! ([`shogi`]) with a single terminal twist: **giving check wins the game**.
//! Validated against Fairy-Stockfish `UCI_Variant checkshogi`.
//!
//! Every piece, the persistent capture-fed **hand**, **drops**, the far-three-rank
//! **promotion zone**, and all drop / promotion legality are exactly Shogi's — this
//! rule layer **delegates every move-generation hook to [`ShogiRules`]** and
//! overrides only the terminal rule. It reuses the same [`Shogi9x9`] geometry and
//! army; the FEN placement, piece letters, and hand bracket are identical to Shogi.
//!
//! [`shogi`]: super::shogi
//! [`ShogiRules`]: super::shogi::ShogiRules
//! [`Shogi9x9`]: super::super::Shogi9x9
//!
//! ## The check-win rule
//!
//! A side that **delivers check wins immediately** (FSF `checkCounting` with a
//! one-check goal — the `1+1` field of the FSF start FEN is the per-side check
//! counter). Because one check ends the game, there is no cross-move counter to
//! track: a position in which the **side to move is in check** is terminal — the
//! checker (the side that just moved) has won. This rides the default-off
//! [`wins_on_check`](WideVariant::wins_on_check) hook: the move generator
//! truncates such a position to **zero moves** (a perft leaf, exactly as FSF
//! truncates it — its `go perft` lists no reply after a checking move), and
//! [`outcome`](crate::geometry::GenericPosition::outcome) credits the win to the
//! checker. Everything else — checkmate, stalemate, sennichite — is Shogi's.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant checkshogi`, `position startpos`) renders the start as
//!
//! ```text
//! lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL[] w - - 1+1 0 1
//! ```
//!
//! mce uses the same board placement and an empty `[]` holdings bracket and omits
//! the `1+1` check-counter field (a single check is terminal, so mce keeps no
//! counter); FSF defaults the field to `1+1` when it is absent, so the two see the
//! byte-identical position.

use crate::geometry::position::{GenericPosition, GenericState};
use crate::geometry::{Bitboard, Board, PromotionConfig, Square, WideRole, WideVariant};
use crate::Color;

use super::super::Shogi9x9;
use super::shogi::ShogiRules;

/// The Checkshogi rule layer: a zero-sized [`WideVariant`] over [`Shogi9x9`] that
/// delegates every hook to [`ShogiRules`] and adds only the check-win terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CheckShogiRules;

impl WideVariant<Shogi9x9> for CheckShogiRules {
    // Every move-generation, promotion, drop, and repetition hook is Shogi's; the
    // one behavioural difference is `wins_on_check` below.

    fn starting_position() -> (Board<Shogi9x9>, GenericState<Shogi9x9>) {
        <ShogiRules as WideVariant<Shogi9x9>>::starting_position()
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Shogi9x9>,
        occupancy: Bitboard<Shogi9x9>,
    ) -> Bitboard<Shogi9x9> {
        <ShogiRules as WideVariant<Shogi9x9>>::role_attacks(role, color, sq, occupancy)
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        <ShogiRules as WideVariant<Shogi9x9>>::role_attack_is_directional(role)
    }

    fn role_is_slider(role: WideRole) -> bool {
        <ShogiRules as WideVariant<Shogi9x9>>::role_is_slider(role)
    }

    fn promotion_config() -> PromotionConfig {
        <ShogiRules as WideVariant<Shogi9x9>>::promotion_config()
    }

    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        <ShogiRules as WideVariant<Shogi9x9>>::in_promotion_zone(color, rank)
    }

    fn has_castling() -> bool {
        <ShogiRules as WideVariant<Shogi9x9>>::has_castling()
    }

    fn has_hand() -> bool {
        <ShogiRules as WideVariant<Shogi9x9>>::has_hand()
    }

    fn role_can_promote(role: WideRole) -> bool {
        <ShogiRules as WideVariant<Shogi9x9>>::role_can_promote(role)
    }

    fn role_promotion_forced(role: WideRole, color: Color, to_rank: u8) -> bool {
        <ShogiRules as WideVariant<Shogi9x9>>::role_promotion_forced(role, color, to_rank)
    }

    fn drop_targets(role: WideRole, color: Color, board: &Board<Shogi9x9>) -> Bitboard<Shogi9x9> {
        <ShogiRules as WideVariant<Shogi9x9>>::drop_targets(role, color, board)
    }

    // --- The one behavioural override: giving check wins ------------------

    fn wins_on_check() -> bool {
        true
    }

    // --- Sennichite / perpetual check (as Shogi; terminal only) -----------

    fn tracks_repetition() -> bool {
        <ShogiRules as WideVariant<Shogi9x9>>::tracks_repetition()
    }

    fn repetition_fold() -> usize {
        <ShogiRules as WideVariant<Shogi9x9>>::repetition_fold()
    }

    fn repetition_draw_reason() -> crate::geometry::WideEndReason {
        <ShogiRules as WideVariant<Shogi9x9>>::repetition_draw_reason()
    }

    fn perpetual_check_loses() -> bool {
        <ShogiRules as WideVariant<Shogi9x9>>::perpetual_check_loses()
    }
}

/// Checkshogi (Check Shogi) as a [`GenericPosition`] over the 9x9 geometry.
///
/// Construct the starting position with
/// [`CheckShogi::startpos`](GenericPosition::startpos) or parse a FEN — the
/// placement may carry the hand as a `[..]` holdings bracket — with
/// [`CheckShogi::from_fen`](GenericPosition::from_fen). It is standard Shogi
/// except that **giving check wins**; see the [module docs](self).
pub type CheckShogi = GenericPosition<Shogi9x9, CheckShogiRules>;
