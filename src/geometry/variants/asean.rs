//! ASEAN chess — a modern Makruk variant on the generic engine.
//!
//! ASEAN chess is the FIDE-affiliated modernisation of Makruk adopted for the
//! ASEAN region. It keeps Makruk's pieces but swaps two of Makruk's hallmark
//! rules for FIDE-style ones, so on the generic engine it is the
//! [`MakrukRules`](super::makruk::MakrukRules) layer with exactly two changes —
//! the starting array and the promotion rule. Its pieces are:
//!
//! * **Rook** — a standard rook. ([`WideRole::Rook`])
//! * **Knight** — a standard knight. ([`WideRole::Knight`])
//! * **Khon / "bishop"** (silver-general, [`WideRole::Silver`]) — one step to
//!   any of the four diagonals, plus one step straight **forward**
//!   (color-relative). Five destinations from an open square. Exactly Makruk's
//!   Khon; ASEAN labels it with the international `b`.
//! * **Met / "queen"** (ferz, [`WideRole::Met`]) — one step to any of the four
//!   diagonals. Exactly Makruk's Met; ASEAN labels it with the international `q`.
//! * **King** — a standard king. ([`WideRole::King`])
//! * **Pawn** — moves one square straight forward only (**no** double push,
//!   hence **no** en passant) and captures one square diagonally forward,
//!   exactly as Makruk's Bia.
//!
//! There is **no castling** (as in Makruk). The game is won by checkmate;
//! stalemate is a draw — the generic engine's standard behaviour.
//!
//! ## The two differences from Makruk
//!
//! 1. **Starting array.** ASEAN uses the symmetric FIDE layout — both kings on
//!    the e-file and both Mets on the d-file — rather than Makruk's mirrored
//!    Met/King pair. Internally mcr names the Khon `s` and the Met `m`, so the
//!    placement is `rnsmksnr/.../RNSMKSNR`; rewriting `s→b`, `m→q` yields the
//!    Fairy-Stockfish dialect `rnbqkbnr/.../RNBQKBNR`.
//! 2. **Promotion.** A pawn promotes on the **last rank** (FIDE-style, rank
//!    index 7 for white / 0 for black) and may promote to **Met, Rook, Silver,
//!    or Knight** — a choice of four, the FSF `q`/`r`/`b`/`n` targets. Makruk,
//!    by contrast, promotes only to a Met and three ranks earlier (rank index
//!    5 / 2).
//!
//! Because the start array differs only in the Met/King swap and promotion
//! diverges only once a pawn reaches the far rank, ASEAN and Makruk share their
//! shallow perft (depths 1–5 from the start position are identical); they first
//! diverge at depth 6, where the extra promotion targets multiply.
//!
//! ## The counting / draw rule (terminal only)
//!
//! ASEAN has its own modernised-Makruk counting endgame, modelled at the *game*
//! level through the default-off [`WideVariant::counting_rule`] hook (adjudicated
//! by [`GenericGame`](crate::geometry::game::GenericGame)). Unlike Makruk it is
//! **pieces-honour only** — the count begins once the losing side is a lone king
//! with no pawns, limited to 16 / 44 / 64 full moves by the superior side's
//! strongest piece. It affects only game termination, never move generation, so
//! perft is unchanged.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant asean` / `position startpos`
//! (`rnbqkbnr/8/pppppppp/8/8/PPPPPPPP/8/RNBQKBNR w - - 0 1` in the FSF dialect):
//!
//! ```text
//! rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSMKSNR w - - 0 1
//! ```

use crate::geometry::attacks::leaper_attacks;
use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    Bitboard, Board, Chess8x8, Geometry, PromotionConfig, Square, StandardChess, WideRole,
    WideVariant,
};
use crate::Color;

/// The ASEAN-chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides only what ASEAN changes from standard chess — the Met and Khon
/// movement (shared with Makruk), the symmetric starting array, the pawn rules
/// (single-step, no double push), the FIDE-style four-target last-rank
/// promotion, and the absence of castling. Everything else is the trait default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct AseanRules;

/// The confirmed ASEAN starting placement (mcr dialect; `s`=Khon, `m`=Met),
/// validated against Fairy-Stockfish `UCI_Variant asean`. Symmetric FIDE
/// layout: both Mets on the d-file, both kings on the e-file.
const ASEAN_START_PLACEMENT: &str = "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSMKSNR";

/// The four ferz (diagonal one-step) offsets — the Met's movement and the
/// diagonal component of the Khon.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

impl WideVariant<Chess8x8> for AseanRules {
    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(ASEAN_START_PLACEMENT)
            .expect("the ASEAN starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            // ASEAN, like Makruk, has no castling.
            castling: GenericCastling::NONE,
            ep_square: None,
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
        sq: Square<Chess8x8>,
        occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        match role {
            // Met = ferz: the four diagonal one-steps.
            WideRole::Met => leaper_attacks::<Chess8x8>(sq, &FERZ_OFFSETS),
            // Khon = silver general: the four diagonals plus one straight step
            // toward the far rank (color-relative forward).
            WideRole::Silver => {
                let forward: i8 = if color.is_white() { 1 } else { -1 };
                let mut bb = leaper_attacks::<Chess8x8>(sq, &FERZ_OFFSETS);
                if let Some(dest) = sq.offset(0, forward) {
                    bb.set(dest);
                }
                bb
            }
            // Rook / Knight / King and the pawn are standard: defer to the trait
            // default, exactly as Makruk does, keeping them byte-identical to
            // standard chess.
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // The Khon (Silver General) adds a single straight step toward the far
        // rank to its four diagonals, so its attack set is color-directional and
        // `attackers_to` must reverse-project with the opposite colour — exactly
        // as in Makruk (the #201 class). The Met (ferz) is symmetric.
        matches!(role, WideRole::Pawn | WideRole::Silver)
    }

    fn promotion_config() -> PromotionConfig {
        // ASEAN's FIDE-style promotion: a pawn reaching the last rank promotes to
        // a Met (the FSF `q`), Rook, Silver/Khon (the FSF `b`), or Knight — a
        // choice of four, unlike Makruk's Met-only promotion.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Knight,
                WideRole::Rook,
                WideRole::Met,
                WideRole::Silver,
            ],
        }
    }

    // ASEAN promotes on the last rank, so `promotion_rank` keeps the default
    // (`HEIGHT - 1` for white, `0` for black) — Makruk overrides it to rank
    // index 5 / 2, but ASEAN does not.

    fn double_push_rank(_color: Color) -> u8 {
        // The pawn never makes a double advance (as in Makruk). Return a rank no
        // pawn can stand on (one past the last rank) so the generic pawn
        // generator's `from.rank() == double_push_rank` guard never fires — no
        // double push and therefore no en-passant target is ever set.
        Chess8x8::HEIGHT
    }

    fn has_castling() -> bool {
        false
    }

    fn counting_rule() -> Option<crate::geometry::WideCountingRule> {
        // ASEAN's modernised-Makruk counting: a pieces-honour-only countdown that
        // begins once the losing side is a lone king with no pawns left, limited to
        // 16 / 44 / 64 full moves by the superior side's strongest piece (rook /
        // khon / knight). Reproduced exactly from Fairy-Stockfish (see
        // [`GenericGame`](crate::geometry::game::GenericGame)); terminal-only, so
        // perft is byte-identical.
        Some(crate::geometry::WideCountingRule::Asean)
    }
}

/// ASEAN chess as a [`GenericPosition`] over the 8x8 geometry.
///
/// Construct the starting position with [`Asean::startpos`](GenericPosition::startpos)
/// or parse a FEN with [`Asean::from_fen`](GenericPosition::from_fen).
pub type Asean = GenericPosition<Chess8x8, AseanRules>;
