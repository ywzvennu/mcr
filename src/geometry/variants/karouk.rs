//! Ka Ouk / Kar Ouk on the generic engine — **Cambodian chess**
//! ([`cambodian`](super::cambodian)) with a single terminal twist: **giving check
//! wins the game**. Validated against Fairy-Stockfish `UCI_Variant karouk`.
//!
//! Every piece, the one-time king / Met first-move leaps, the `DEde` leap-rights
//! field, the single-step promote-to-Met pawns, and the Cambodian counting
//! endgame are exactly Cambodian's — this rule layer **delegates every
//! move-generation hook to [`CambodianRules`]** and overrides only the terminal
//! rule. It reuses the same [`Chess8x8`] geometry and Makruk army; the FEN
//! placement, piece letters, and leap-rights field are identical to Cambodian.
//!
//! [`CambodianRules`]: super::cambodian::CambodianRules
//! [`Chess8x8`]: super::super::Chess8x8
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
//! checker. Everything else — checkmate, stalemate, the Cambodian leaps and
//! counting — is Cambodian's. (This is why Ka Ouk's perft first diverges from
//! Cambodian's only once a check can arise: identical through depth 5 from the
//! start, then fewer nodes at depth 6 where a checked position is pruned.)
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant karouk`, `position startpos`) renders the start as
//!
//! ```text
//! rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w DEde - 1+1 0 1
//! ```
//!
//! mcr uses the same board placement and `DEde` leap-rights field and omits the
//! `1+1` check-counter field (a single check is terminal, so mcr keeps no
//! counter); FSF defaults the field to `1+1` when it is absent, so the two see the
//! byte-identical position.

use crate::geometry::position::{GenericCastling, GenericPosition, GenericState};
use crate::geometry::variants::cambodian::CambodianRules;
use crate::geometry::{
    Bitboard, Board, Chess8x8, PromotionConfig, Square, WideCountingRule, WideRole, WideVariant,
};
use crate::Color;

/// The Ka Ouk rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`] that
/// delegates every move-generation hook to [`CambodianRules`] and adds only the
/// check-win terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct KaroukRules;

impl WideVariant<Chess8x8> for KaroukRules {
    /// The tightest prefix of [`WideRole::ALL`] that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 8;

    // Every move-generation, promotion, pawn, leap, and counting hook is
    // Cambodian's; the one behavioural difference is `wins_on_check` below.

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        <CambodianRules as WideVariant<Chess8x8>>::starting_position()
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Chess8x8>,
        occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        <CambodianRules as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy)
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        <CambodianRules as WideVariant<Chess8x8>>::role_attack_is_directional(role)
    }

    fn promotion_config() -> PromotionConfig {
        <CambodianRules as WideVariant<Chess8x8>>::promotion_config()
    }

    fn promotion_rank(color: Color) -> u8 {
        <CambodianRules as WideVariant<Chess8x8>>::promotion_rank(color)
    }

    fn double_push_rank(color: Color) -> u8 {
        <CambodianRules as WideVariant<Chess8x8>>::double_push_rank(color)
    }

    fn has_castling() -> bool {
        <CambodianRules as WideVariant<Chess8x8>>::has_castling()
    }

    fn has_first_move_leaps() -> bool {
        <CambodianRules as WideVariant<Chess8x8>>::has_first_move_leaps()
    }

    fn king_leap_offsets(color: Color) -> &'static [(i8, i8)] {
        <CambodianRules as WideVariant<Chess8x8>>::king_leap_offsets(color)
    }

    fn met_leap_offsets(color: Color) -> &'static [(i8, i8)] {
        <CambodianRules as WideVariant<Chess8x8>>::met_leap_offsets(color)
    }

    fn parse_first_move_rights(field: &str) -> Option<GenericCastling> {
        <CambodianRules as WideVariant<Chess8x8>>::parse_first_move_rights(field)
    }

    fn write_first_move_rights(rights: GenericCastling, out: &mut alloc::string::String) {
        <CambodianRules as WideVariant<Chess8x8>>::write_first_move_rights(rights, out);
    }

    fn counting_rule() -> Option<WideCountingRule> {
        <CambodianRules as WideVariant<Chess8x8>>::counting_rule()
    }

    // --- The one behavioural override: giving check wins ------------------

    fn wins_on_check() -> bool {
        true
    }
}

/// Ka Ouk (Kar Ouk) as a [`GenericPosition`] over the 8x8 geometry.
///
/// Construct the starting position with
/// [`Karouk::startpos`](GenericPosition::startpos) or parse a FEN — carrying the
/// `DEde` leap-rights field — with [`Karouk::from_fen`](GenericPosition::from_fen).
/// It is Cambodian chess except that **giving check wins**; see the
/// [module docs](self).
pub type Karouk = GenericPosition<Chess8x8, KaroukRules>;
