//! Makpong ("Defensive Chess") on the generic engine — a Makruk tie-break
//! variant whose only rule change is that the king may not flee a check
//! (issue #260).
//!
//! Makpong reuses the **entire** Makruk rule layer ([`MakrukRules`]) — the same
//! [`Chess8x8`] geometry, the same Rua / Ma / Khon / Met / Khun / Bia pieces,
//! the same single-step-no-en-passant pawn that promotes to a Met, the same
//! no-castling starting array, and the same starting FEN. The sole difference is
//! a single king-safety rule:
//!
//! > **While the side to move is in check, its king may not move out of check.**
//! > It may move **only to capture the lone checker**; otherwise the check must
//! > be answered by another piece (a block, or a capture of the checker by a
//! > non-king piece). Under double check no king move is legal at all.
//!
//! This is delegated to one default-off hook,
//! [`WideVariant::king_may_only_capture_checker`], which Makpong overrides to
//! `true` and every other variant leaves `false` (so they stay byte-identical).
//! Every remaining Makruk rule — movement, promotion, pawn behaviour, starting
//! position, and Makruk's counting / draw rule (which never affects perft) — is
//! inherited verbatim by forwarding to [`MakrukRules`].
//!
//! ## Confirmed rule and starting FEN
//!
//! Both are pinned against Fairy-Stockfish's `UCI_Variant makpong`. The starting
//! array is byte-identical to Makruk's:
//!
//! ```text
//! rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w - - 0 1
//! ```
//!
//! FSF's `makpongRule` (`position.cpp`) rejects a king move while in check unless
//! its destination is the (single) checker's square — exactly the rule modelled
//! here. The startpos perft is identical to Makruk's at every shallow depth (no
//! check arises in the opening tree), and the two diverge only once a king is in
//! check: Makpong then omits every king-flee move that Makruk would allow.

use crate::geometry::position::GenericState;
use crate::geometry::{
    Bitboard, Board, Chess8x8, GenericPosition, MakrukRules, PromotionConfig, Square,
    WideCountingRule, WideRole, WideVariant,
};
use crate::Color;

/// The Makpong rule layer: [`MakrukRules`] plus the single king-may-not-flee
/// rule, a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// Every hook forwards to [`MakrukRules`] except
/// [`king_may_only_capture_checker`](WideVariant::king_may_only_capture_checker),
/// which Makpong turns on. The forwarding keeps Makpong's move generation, FEN,
/// promotion, and starting array identical to Makruk's wherever the king is not
/// in check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MakpongRules;

impl WideVariant<Chess8x8> for MakpongRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 8;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        <MakrukRules as WideVariant<Chess8x8>>::starting_position()
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Chess8x8>,
        occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        <MakrukRules as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy)
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        <MakrukRules as WideVariant<Chess8x8>>::role_attack_is_directional(role)
    }

    fn promotion_config() -> PromotionConfig {
        <MakrukRules as WideVariant<Chess8x8>>::promotion_config()
    }

    fn promotion_rank(color: Color) -> u8 {
        <MakrukRules as WideVariant<Chess8x8>>::promotion_rank(color)
    }

    fn double_push_rank(color: Color) -> u8 {
        <MakrukRules as WideVariant<Chess8x8>>::double_push_rank(color)
    }

    fn has_castling() -> bool {
        <MakrukRules as WideVariant<Chess8x8>>::has_castling()
    }

    /// The one Makpong-specific rule: while in check the king may move only to
    /// capture the lone checker — it may not flee. Every other hook is Makruk's.
    fn king_may_only_capture_checker() -> bool {
        true
    }

    fn counting_rule() -> Option<WideCountingRule> {
        // Makpong keeps Makruk's counting endgame verbatim (terminal-only, so
        // perft is byte-identical); its only rule change is king-safety.
        <MakrukRules as WideVariant<Chess8x8>>::counting_rule()
    }
}

/// Makpong ("Defensive Chess") as a [`GenericPosition`] over the 8x8 geometry.
///
/// Construct the starting position with
/// [`Makpong::startpos`](crate::geometry::GenericPosition::startpos) or parse a
/// FEN with [`Makpong::from_fen`](crate::geometry::GenericPosition::from_fen).
/// The position behaves exactly like [`Makruk`](super::Makruk) except that the
/// king may not move out of check.
pub type Makpong =
    GenericPosition<Chess8x8, MakpongRules, { <MakpongRules as WideVariant<Chess8x8>>::ROLE_SPAN }>;
