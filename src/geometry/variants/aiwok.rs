//! Ai-Wok on the generic engine — **Makruk** ([`makruk`](super::makruk)) with the
//! Met (ferz) replaced by a single **Ai-Wok** super-piece. Validated against
//! Fairy-Stockfish `UCI_Variant ai-wok`.
//!
//! Every other piece — the Rua (rook), Ma (knight), Khon (silver, [`WideRole::Silver`]),
//! Khun (king), and the single-step promote-on-the-sixth-rank Bia (pawn) — the
//! absence of castling, and the Makruk counting endgame are exactly Makruk's, so
//! this rule layer **delegates those hooks to [`MakrukRules`]** and overrides only
//! what the Ai-Wok changes: the starting array, the Ai-Wok's movement, the
//! promotion target, and the slider set.
//!
//! [`MakrukRules`]: super::makruk::MakrukRules
//!
//! ## The Ai-Wok piece
//!
//! The Ai-Wok (FSF `AIWOK`, Betza `RNF`) is a **Rook + Knight + Ferz** compound:
//! it slides any distance orthogonally like a rook, leaps like a knight, and steps
//! one square to any diagonal like a Met — a Chancellor (rook + knight) with the
//! Met's extra diagonal step. Confirmed square-for-square against Fairy-Stockfish.
//!
//! mce has no dedicated Ai-Wok role: the runtime board wire format packs a piece
//! as `color << 7 | role`, so a role index must fit the low 7 bits and the role
//! table is full to that bound. The Ai-Wok is therefore fielded as the existing
//! Rook + Knight + Ferz [`WideRole::Ship`] (introduced for Mansindam's promoted
//! Marshal, movement-identical), whose FEN token is the second-bank overflow
//! `**S` (white) / `**s` (black); the `compare-fairy` harness maps `**s → a` when
//! driving FSF's `ai-wok`.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant ai-wok`, `position startpos`) renders the start as
//!
//! ```text
//! rnsaksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKASNR w - - 0 1
//! ```
//!
//! where FSF's `a` / `A` is the Ai-Wok; mce spells that piece `**s` / `**S`, so
//! the two describe the byte-identical board.

use crate::geometry::attacks::{knight_attacks, leaper_attacks, rook_attacks};
use crate::geometry::position::{GenericPosition, GenericState};
use crate::geometry::variants::makruk::MakrukRules;
use crate::geometry::{
    Bitboard, Board, Chess8x8, PromotionConfig, Square, WideCountingRule, WideRole, WideVariant,
};
use crate::Color;

/// The Ai-Wok rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It delegates every unchanged hook to [`MakrukRules`] and overrides only the
/// starting array, the Ai-Wok ([`WideRole::Ship`]) movement, the promotion target,
/// and the slider set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct AiwokRules;

/// The confirmed Ai-Wok starting placement, validated against Fairy-Stockfish
/// `UCI_Variant ai-wok`. The Makruk array with each Met replaced by an Ai-Wok
/// ([`WideRole::Ship`], spelled with the second-bank overflow token `**s` / `**S`).
const AIWOK_START_PLACEMENT: &str = "rns**sksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSK**SSNR";

/// The four ferz (diagonal one-step) offsets — the Ai-Wok's diagonal component.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

impl WideVariant<Chess8x8> for AiwokRules {
    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(AIWOK_START_PLACEMENT)
            .expect("the Ai-Wok starting placement is valid on an 8x8 board");
        // The whole Makruk starting state (no castling, no en passant, move
        // counters) is reused; only the placement differs.
        let (_, state) = <MakrukRules as WideVariant<Chess8x8>>::starting_position();
        (board, state)
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Chess8x8>,
        occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        match role {
            // Ai-Wok (fielded as the Ship): Rook + Knight + Ferz.
            WideRole::Ship => {
                rook_attacks::<Chess8x8>(sq, occupancy)
                    | knight_attacks::<Chess8x8>(sq)
                    | leaper_attacks::<Chess8x8>(sq, &FERZ_OFFSETS)
            }
            // Every other piece — Khon, pawn, rook, knight, king — is exactly
            // Makruk's (the Met never appears on an Ai-Wok board, but delegating it
            // is harmless).
            _ => <MakrukRules as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // The Ai-Wok is colour-symmetric (rook + knight + ferz); only the Khon and
        // pawn are forward-biased, exactly as in Makruk.
        <MakrukRules as WideVariant<Chess8x8>>::role_attack_is_directional(role)
    }

    fn role_is_slider(role: WideRole) -> bool {
        // The Ai-Wok has a rook-ray component, so it can pin and be pinned; add it
        // to the standard slider set Makruk inherits.
        role == WideRole::Ship || <MakrukRules as WideVariant<Chess8x8>>::role_is_slider(role)
    }

    fn promotion_config() -> PromotionConfig {
        // A Bia promotes only to an Ai-Wok (the Ship), never a Met.
        PromotionConfig {
            roles: alloc::vec![WideRole::Ship],
        }
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

    fn counting_rule() -> Option<WideCountingRule> {
        // Ai-Wok keeps Makruk's counting endgame (terminal-only, so perft is
        // byte-identical).
        <MakrukRules as WideVariant<Chess8x8>>::counting_rule()
    }
}

/// Ai-Wok as a [`GenericPosition`] over the 8x8 geometry.
///
/// Construct the starting position with
/// [`Aiwok::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Aiwok::from_fen`](GenericPosition::from_fen). It is Makruk with the Met
/// replaced by a Rook + Knight + Ferz Ai-Wok; see the [module docs](self).
pub type Aiwok = GenericPosition<Chess8x8, AiwokRules>;
