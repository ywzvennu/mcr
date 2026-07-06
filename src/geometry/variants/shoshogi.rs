//! Sho Shogi (将棋, "small/middle Shogi" — old 9x9 Shogi **without drops**) on
//! the generic engine, validated node-for-node against Fairy-Stockfish
//! `UCI_Variant shoshogi`.
//!
//! Sho Shogi is the historical ancestor of modern Shogi: the **same 9x9 board and
//! the same army** (King, Rook, Bishop, Gold/Silver Generals, Knight, Lance, Pawn,
//! with the identical `+`-promotions to Dragon King / Dragon Horse / Gold-moving
//! minors), but **captured pieces are removed, not pocketed** — there is **no hand
//! and no drops**. Its one extra piece is the **Drunk Elephant**, which promotes
//! to a **Crown Prince**, a *second royal piece*.
//!
//! This module is a thin layer over [`ShogiRules`]: the
//! whole shared army (movement, the `+`-promotions, the promotion zone, the forced
//! promotions, the directional/slider classifications) is **delegated** to it, and
//! only the two new pieces and the multi-royal rule are added here. The hand and
//! drops are simply left off ([`has_hand`](WideVariant::has_hand) stays `false`);
//! the per-piece promotion instead rides the no-hand
//! [`has_piece_promotion`](WideVariant::has_piece_promotion) path (as Chak's does).
//!
//! ## The two new pieces (confirmed square-for-square against FSF)
//!
//! * **Drunk Elephant** ([`WideRole::DrunkElephant`], FSF `e`, Betza `FsfW`) —
//!   steps one square to any of **seven** directions: the four diagonals (Ferz)
//!   plus one step **forward** or **sideways** — every King step except the
//!   straight-**backward** one. It **promotes to a Crown Prince**.
//! * **Crown Prince** ([`WideRole::CrownPrince`], FSF `+E`) — the promoted Drunk
//!   Elephant: a full one-step King in every direction, and a **second royal**
//!   piece.
//!
//! ## Two royals, count-thresholded (FSF `extinctionPseudoRoyal`)
//!
//! FSF builds Sho Shogi as `shogi` with the King replaced by a (non-royal)
//! Commoner and `extinctionPieceTypes = {Commoner}`, `extinctionPieceCount = 0`,
//! `extinctionPseudoRoyal = true`: the King and a promoted Crown Prince are the
//! **same** Commoner piece type, and a Commoner is **royal only while a side holds
//! at most one of them** (`count ≤ extinctionPieceCount + 1 = 1`). So:
//!
//! * While a side has **both** a King and a Crown Prince, **neither is royal** —
//!   it is never in check and may leave either (or both) en prise; it is lost only
//!   when **both** are gone.
//! * Reduced to a **single** royal, that piece behaves exactly like an ordinary
//!   royal King (checks, mate).
//!
//! mcr expresses this with the multi-royal machinery (reused from Spartan / Chak):
//! [`royal_squares`](WideVariant::royal_squares) reports the King and the Crown
//! Prince, and [`royal_constraint_active`](WideVariant::royal_constraint_active)
//! turns the king-safety constraint **off** while the side holds more than one of
//! them. Promoting a Drunk Elephant into a second royal is therefore always legal
//! (it drops the side's pseudo-royalty), matching FSF move-for-move.
//!
//! ## FEN dialect and confirmed start
//!
//! FSF (`UCI_Variant shoshogi`, `position startpos`) renders the start as
//!
//! ```text
//! lnsgkgsnl/1r2e2b1/ppppppppp/9/9/9/PPPPPPPPP/1B2E2R1/LNSGKGSNL w 0 1
//! ```
//!
//! with the Drunk Elephant as `e`/`E`. The single-`*` overflow alphabet being
//! exhausted, mcr spells the Drunk Elephant and Crown Prince with the **doubled**
//! overflow prefix `**` (`**E`/`**e` Drunk Elephant, `**C`/`**c` Crown Prince), so
//! mcr's start FEN is
//!
//! ```text
//! lnsgkgsnl/1r2**e2b1/ppppppppp/9/9/9/PPPPPPPPP/1B2**E2R1/LNSGKGSNL w - - 0 1
//! ```
//!
//! The `compare-fairy/` harness rewrites `**e → e` and `**c → +E` (plus the shared
//! Shogi letters, which match verbatim) when driving FSF. Both are the same
//! position; the comparison asserts only node counts.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{attacks, Bitboard, Board, PromotionConfig, Square, WideRole, WideVariant};
use crate::Color;

use super::super::Shogi9x9;
use super::shogi::ShogiRules;

/// The Sho Shogi rule layer: a zero-sized [`WideVariant`] over [`Shogi9x9`].
///
/// It reuses [`ShogiRules`] for the whole shared Shogi army (movement, the
/// `+`-promotions, the promotion zone and forced promotions) and adds only the
/// Drunk Elephant / Crown Prince movement, the Drunk Elephant → Crown Prince
/// promotion, and the count-thresholded two-royal rule. The hand / drops are left
/// off; promotion rides the no-hand per-piece promotion path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ShoShogiRules;

/// The confirmed Sho Shogi starting placement (mcr dialect; the Drunk Elephants
/// are the doubled-overflow `**E` / `**e` tokens).
const SHOSHOGI_PLACEMENT: &str =
    "lnsgkgsnl/1r2**e2b1/ppppppppp/9/9/9/PPPPPPPPP/1B2**E2R1/LNSGKGSNL";

impl ShoShogiRules {
    /// The Drunk Elephant's attack set from `sq` for `color`: the four diagonals
    /// (Ferz) plus one step **forward** and one step to each **side** — every King
    /// step except the straight-backward one (seven squares).
    fn drunk_elephant_attacks(color: Color, sq: Square<Shogi9x9>) -> Bitboard<Shogi9x9> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        let offsets = [
            // The four diagonals (Ferz).
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
            // One step forward and one to each side.
            (0, fwd),
            (1, 0),
            (-1, 0),
        ];
        attacks::leaper_attacks::<Shogi9x9>(sq, &offsets)
    }
}

impl WideVariant<Shogi9x9> for ShoShogiRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 60;

    fn starting_position() -> (Board<Shogi9x9>, GenericState<Shogi9x9>) {
        let board = Board::<Shogi9x9>::from_fen_placement(SHOSHOGI_PLACEMENT)
            .expect("the Sho Shogi starting placement is valid on a 9x9 board");
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::NONE,
            ep_square: None,
            ep_captured: None,
            gating: GenericGating::NONE,
            duck: None,
            placement: GenericPlacement::NONE,
            halfmove_clock: 0,
            fullmove_number: 1,
            consecutive_passes: 0,
            board_b: crate::geometry::Bitboard::EMPTY,
        };
        (board, state)
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Shogi9x9>,
        occupancy: Bitboard<Shogi9x9>,
    ) -> Bitboard<Shogi9x9> {
        match role {
            // The Drunk Elephant: King minus the straight-backward step.
            WideRole::DrunkElephant => Self::drunk_elephant_attacks(color, sq),
            // The Crown Prince: a full one-step King in every direction.
            WideRole::CrownPrince => attacks::king_attacks::<Shogi9x9>(sq),
            // Every shared Shogi piece (and its `+`-promoted form) moves exactly as
            // in Shogi.
            _ => <ShogiRules as WideVariant<Shogi9x9>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // The Drunk Elephant's attack set is forward-biased (it omits the
        // straight-backward step), so a Drunk-Elephant attacker is found by
        // projecting the opposite colour back from the target — exactly as for the
        // Shogi forward steppers. The Crown Prince is a symmetric King and is not
        // directional.
        role == WideRole::DrunkElephant
            || <ShogiRules as WideVariant<Shogi9x9>>::role_attack_is_directional(role)
    }

    fn role_is_slider(role: WideRole) -> bool {
        // The Drunk Elephant and Crown Prince are pure steppers; every shared piece
        // keeps its Shogi classification (Rook / Bishop / Dragon / Dragon Horse /
        // Lance slide).
        <ShogiRules as WideVariant<Shogi9x9>>::role_is_slider(role)
    }

    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        // The same furthest-three-ranks zone as Shogi.
        <ShogiRules as WideVariant<Shogi9x9>>::in_promotion_zone(color, rank)
    }

    fn has_castling() -> bool {
        false
    }

    // --- per-piece promotion (no hand) ------------------------------------

    fn has_piece_promotion() -> bool {
        true
    }

    fn pawn_is_stepper() -> bool {
        // The Pawn is the Shogi forward stepper (one square forward, capturing
        // straight ahead, promoting in the zone), not a chess pawn — even though
        // there is no hand. The multi-royal generator routes it through the
        // per-piece promotion path on this flag.
        true
    }

    fn role_can_promote(role: WideRole) -> bool {
        // The Drunk Elephant promotes to a Crown Prince; every other promotable
        // piece is exactly Shogi's set (Pawn, Lance, Knight, Silver, Rook, Bishop).
        role == WideRole::DrunkElephant
            || <ShogiRules as WideVariant<Shogi9x9>>::role_can_promote(role)
    }

    fn role_promoted_to(role: WideRole) -> WideRole {
        match role {
            WideRole::DrunkElephant => WideRole::CrownPrince,
            // The shared Shogi per-piece promotions (Pawn→Tokin, Rook→Dragon, …).
            other => other.promoted_form(),
        }
    }

    fn role_promotion_forced(role: WideRole, color: Color, to_rank: u8) -> bool {
        // The Shogi forced promotions (a Pawn/Lance on the last rank, a Knight on
        // the last two ranks). The Drunk Elephant always has a move and is never
        // forced (it is not in Shogi's set, so the delegate returns `false`).
        <ShogiRules as WideVariant<Shogi9x9>>::role_promotion_forced(role, color, to_rank)
    }

    fn promotion_config() -> PromotionConfig {
        // Sho Shogi has no pawn-path promotion (every promotion rides the per-piece
        // path); this static set is unused, but the trait requires it.
        PromotionConfig {
            roles: alloc::vec![WideRole::CrownPrince],
        }
    }

    // --- two royals: King + Crown Prince, count-thresholded ---------------

    fn multi_royal() -> bool {
        true
    }

    fn royal_squares(board: &Board<Shogi9x9>, color: Color) -> Bitboard<Shogi9x9> {
        board.kings_of(color) | board.pieces(color, WideRole::CrownPrince)
    }

    fn royals_all_must_survive() -> bool {
        // When the king-safety constraint is active there is exactly one royal, so
        // "all survive" and "at least one survives" coincide; `true` states the
        // pseudo-royal intent (a single royal must be kept safe).
        true
    }

    fn royal_constraint_active(board: &Board<Shogi9x9>, color: Color) -> bool {
        // FSF `extinctionPieceCount = 0`: a royal (King or Crown Prince) is royal
        // only while the side holds **at most one** of them. With two, neither is
        // royal and the constraint is off.
        let royals = board.kings_of(color) | board.pieces(color, WideRole::CrownPrince);
        royals.count() <= 1
    }

    // --- Sennichite / perpetual check (default-off draw rules) -------------
    //
    // These affect only terminal adjudication in [`GenericGame`], never move
    // generation, so perft is byte-identical.

    fn tracks_repetition() -> bool {
        true
    }

    fn repetition_fold() -> usize {
        // Sennichite: the same position (including both hands) occurring a fourth
        // time is a draw.
        4
    }

    fn repetition_draw_reason() -> crate::geometry::WideEndReason {
        crate::geometry::WideEndReason::Sennichite
    }

    fn perpetual_check_loses() -> bool {
        // A sennichite brought about by perpetual check is a loss for the checking
        // side.
        true
    }
}

/// Sho Shogi (old 9x9 Shogi without drops) as a [`GenericPosition`] over the 9x9
/// [`Shogi9x9`] geometry.
///
/// Construct the starting position with
/// [`ShoShogi::startpos`](GenericPosition::startpos) or parse a FEN (mcr dialect)
/// with [`ShoShogi::from_fen`](GenericPosition::from_fen). See the [module
/// docs](self) for the piece movements, the Drunk Elephant → Crown Prince
/// promotion, and the count-thresholded two-royal rule.
pub type ShoShogi = GenericPosition<Shogi9x9, ShoShogiRules>;

#[cfg(test)]
mod tests {
    use super::*;

    /// The canonical start FEN round-trips through mcr's FEN I/O.
    #[test]
    fn startpos_round_trips() {
        let pos = ShoShogi::startpos();
        assert_eq!(
            pos.to_fen(),
            "lnsgkgsnl/1r2**e2b1/ppppppppp/9/9/9/PPPPPPPPP/1B2**E2R1/LNSGKGSNL w - - 0 1"
        );
    }
}
