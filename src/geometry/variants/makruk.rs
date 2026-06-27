//! Makruk (Thai chess) on the generic engine — the first fairy variant on the
//! [`WideVariant`] layer (`docs/fairy-variants-architecture.md`, Phase 1).
//!
//! Makruk is an 8x8 variant ([`Chess8x8`] geometry) that differs from standard
//! chess only in a few pieces and the pawn rules; every other rule (king
//! safety, checkmate, stalemate-as-draw) is the standard default the generic
//! engine already provides. Its pieces are:
//!
//! * **Rua** (rook) — a standard rook. ([`WideRole::Rook`])
//! * **Ma** (knight) — a standard knight. ([`WideRole::Knight`])
//! * **Khon** (silver-general, [`WideRole::Silver`]) — one step to any of the
//!   four diagonals, plus one step straight **forward** (color-relative). Five
//!   destinations from an open square.
//! * **Met** (ferz, [`WideRole::Met`]) — one step to any of the four diagonals.
//! * **Khun** (king, [`WideRole::King`]) — a standard king.
//! * **Bia** (pawn) — moves one square straight forward only (**no** double
//!   push, hence **no** en passant), captures one square diagonally forward,
//!   and **promotes to a Met** on reaching the sixth rank from its own side
//!   (rank index 5 for white, rank index 2 for black).
//!
//! There is **no castling**. The game is won by checkmate; stalemate is a draw —
//! both are the generic engine's standard behaviour, so they need no override.
//!
//! ## Out of scope: the counting / draw rule
//!
//! Makruk has a "counting" endgame rule (a bare-king side counts down a move
//! budget to force a win or claim a draw). That rule affects only **game
//! termination**, never move generation, so it does not change perft and is not
//! modelled here. The perft validation against Fairy-Stockfish (which likewise
//! does not let the counting rule affect `go perft`) confirms the move
//! generation is exact regardless.
//!
//! ## Confirmed starting FEN
//!
//! The starting array is pinned against Fairy-Stockfish's
//! `UCI_Variant makruk` / `position startpos`:
//!
//! ```text
//! rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR w - - 0 1
//! ```
//!
//! Pawns sit on the third and sixth ranks; the back ranks carry
//! Rua-Ma-Khon and the Met/Khun pair. The kings face each other (white king on
//! file 3, black king on file 4), with each side's Met beside its own king —
//! the same diagonal asymmetry FSF and pychess use.

use crate::geometry::attacks::leaper_attacks;
use crate::geometry::position::{GenericCastling, GenericGating, GenericPosition, GenericState};
use crate::geometry::{
    Bitboard, Board, Chess8x8, Geometry, PromotionConfig, Square, StandardChess, WideRole,
    WideVariant,
};
use crate::Color;

/// The Makruk rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides only what Makruk changes from standard chess — the Met and Khon
/// movement, the starting array, the pawn rules (single-step, promote to Met),
/// and the absence of castling. Everything else is the trait default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MakrukRules;

/// The confirmed Makruk starting FEN placement, validated against
/// Fairy-Stockfish `UCI_Variant makruk`.
const MAKRUK_START_PLACEMENT: &str = "rnsmksnr/8/pppppppp/8/8/PPPPPPPP/8/RNSKMSNR";

/// The four ferz (diagonal one-step) offsets — the Met's movement and the
/// diagonal component of the Khon.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

impl WideVariant<Chess8x8> for MakrukRules {
    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(MAKRUK_START_PLACEMENT)
            .expect("the Makruk starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            // Makruk has no castling.
            castling: GenericCastling::NONE,
            ep_square: None,
            gating: GenericGating::NONE,
            halfmove_clock: 0,
            fullmove_number: 1,
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
            // Rua / Ma / Khun and the pawn (Bia) are standard: defer to the
            // trait default. `StandardChess` overrides no movement, so its
            // `role_attacks` *is* the trait default — the standard
            // rook / knight / king / pawn-diagonal patterns — which keeps these
            // pieces byte-identical to standard chess.
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn promotion_config() -> PromotionConfig {
        // A Bia promotes only to a Met (ferz); there is no choice of role.
        PromotionConfig {
            roles: alloc::vec![WideRole::Met],
        }
    }

    fn promotion_rank(color: Color) -> u8 {
        // White promotes on the sixth rank (index 5); black on the third rank
        // (index 2) — three ranks deep into the opponent's half, the Makruk
        // promotion zone.
        match color {
            Color::White => 5,
            Color::Black => 2,
        }
    }

    fn double_push_rank(_color: Color) -> u8 {
        // The Bia never makes a double advance. Return a rank no pawn can stand
        // on (one past the last rank), so the generic pawn generator's
        // `from.rank() == double_push_rank` guard is never satisfied — there is
        // no double push and therefore no en-passant target is ever set.
        Chess8x8::HEIGHT
    }

    fn has_castling() -> bool {
        false
    }
}

/// Makruk (Thai chess) as a [`GenericPosition`] over the 8x8 geometry.
///
/// Construct the starting position with [`Makruk::startpos`](GenericPosition::startpos)
/// or parse a FEN with
/// [`Makruk::from_fen`](GenericPosition::from_fen).
pub type Makruk = GenericPosition<Chess8x8, MakrukRules>;
