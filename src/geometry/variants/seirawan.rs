//! Seirawan chess (S-Chess, 8x8) on the generic engine — the first **gating**
//! variant on the [`WideVariant`] layer (`docs/fairy-variants-architecture.md`,
//! Phase 1, §4.4). It exercises the reserve / gating mechanic the generic engine
//! gained for this variant, validated against Fairy-Stockfish.
//!
//! Seirawan is standard 8x8 chess plus two extra pieces held **in reserve** off
//! the board, one of each per side:
//!
//! * **Hawk** ([`WideRole::Hawk`], Bishop + Knight) — FSF / mce letter `H`/`h`.
//! * **Elephant** ([`WideRole::Elephant`], Rook + Knight) — FSF / mce letter
//!   `E`/`e`.
//!
//! Their movement is already the [`WideVariant`] default (`bishop | knight` and
//! `rook | knight`), so no `role_attacks` override is needed.
//!
//! ## Gating
//!
//! When a piece standing on its **original back-rank square** makes its first
//! move, the player **may** simultaneously place ("gate") one reserve piece onto
//! the square the piece just vacated. Gating is optional and each reserve is
//! placed at most once. Castling counts as a first move for **both** the king and
//! the castling rook, so a castle may gate onto the king's *or* the rook's
//! vacated square (one, never both).
//!
//! Gating never rescues an otherwise-illegal move: the base move must be legal on
//! its own (a gated piece may not block a check or shield the king), which is
//! exactly how the generic engine emits gates — it augments already-legal base
//! moves, so the gate can only *add* an option. See
//! [`GenericPosition`](crate::geometry::GenericPosition)'s movegen and `apply`.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant seirawan`:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[HEhe] w KQBCDFGkqbcdfg - 0 1
//! ```
//!
//! mce uses the **same dialect** FSF does for S-Chess (`H` Hawk, `E` Elephant),
//! so the FEN is byte-identical — no rewrite is needed in `compare-fairy/`.
//!
//! The two FEN extensions over plain chess:
//!
//! * **Holdings** `[HEhe]` after the placement: the reserves in hand (white `HE`,
//!   black `he`).
//! * **Gating rights in the castling field** `KQBCDFGkqbcdfg`: the `KQkq` letters
//!   are the usual castling rights (which *also* make the rook squares and the
//!   unmoved king square gating-eligible), and the file letters `BCDFG` /
//!   `bcdfg` mark the remaining gating-eligible back-rank squares — every
//!   original back-rank file in the start position (the a/e/h files are implied
//!   by the castling letters, so they are not re-listed, matching the FSF
//!   dialect). The generic [`from_fen`](crate::geometry::GenericPosition::from_fen)
//!   parses both extensions when [`WideVariant::supports_gating`] is `true`.

use crate::geometry::position::{GenericCastling, GenericGating, GenericPosition, GenericState};
use crate::geometry::{
    Bitboard, Board, Chess8x8, Geometry, PromotionConfig, Square, WideRole, WideVariant,
};
use crate::Color;

/// The Seirawan rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides only what Seirawan adds to standard chess: the reserves and
/// gating-eligible squares of the opening (via [`WideVariant::supports_gating`]
/// and [`WideVariant::initial_gating`]) and the widened promotion set (a pawn may
/// also promote to a Hawk or Elephant). The Hawk (`B+N`) and Elephant (`R+N`)
/// movement is already the trait default, so there is no `role_attacks` override;
/// every other rule — pawns, knights, sliders, the king, castling — is standard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SeirawanRules;

/// The standard 8x8 starting placement (Seirawan shares the chess array; the
/// reserves live in hand, not on the board).
const SEIRAWAN_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

impl SeirawanRules {
    /// The gating-eligible back-rank square set for a fresh game: every square on
    /// white's rank 0 and black's rank 7 (all eight original back-rank pieces may
    /// gate on their first move).
    fn opening_eligible() -> Bitboard<Chess8x8> {
        let mut bb = Bitboard::<Chess8x8>::EMPTY;
        for file in 0..Chess8x8::WIDTH {
            if let Some(sq) = Square::<Chess8x8>::from_file_rank(file, 0) {
                bb.set(sq);
            }
            if let Some(sq) = Square::<Chess8x8>::from_file_rank(file, Chess8x8::HEIGHT - 1) {
                bb.set(sq);
            }
        }
        bb
    }
}

impl WideVariant<Chess8x8> for SeirawanRules {
    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(SEIRAWAN_START_PLACEMENT)
            .expect("the Seirawan starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::standard::<Chess8x8>(),
            ep_square: None,
            gating: Self::initial_gating(),
            duck: None,
            halfmove_clock: 0,
            fullmove_number: 1,
        };
        (board, state)
    }

    fn promotion_config() -> PromotionConfig {
        // A Seirawan pawn promotes to any non-pawn, non-king role of the army: the
        // four standard plus the two reserve compounds. FSF lists the reserve
        // pieces among the legal promotion targets, so the perft branching matches
        // only with the Hawk and Elephant included. The order affects only move
        // enumeration order, not the leaf count.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Knight,
                WideRole::Bishop,
                WideRole::Rook,
                WideRole::Queen,
                WideRole::Hawk,     // B+N reserve compound
                WideRole::Elephant, // R+N reserve compound
            ],
        }
    }

    fn supports_gating() -> bool {
        true
    }

    fn initial_gating() -> GenericGating<Chess8x8> {
        // Both reserves in hand for both colors; every original back-rank square
        // gating-eligible.
        GenericGating::new(Self::opening_eligible(), [true, true], [true, true])
    }
}

/// Seirawan chess (S-Chess) as a [`GenericPosition`] over the 8x8 [`Chess8x8`]
/// geometry.
///
/// Construct the starting position with
/// [`Seirawan::startpos`](GenericPosition::startpos) or parse a FEN — including
/// the `[HEhe]` holdings and the `KQBCDFGkqbcdfg` gating-rights extensions — with
/// [`Seirawan::from_fen`](GenericPosition::from_fen). The Hawk and Elephant reuse
/// the generic compound movement defaults; only the reserves, gating, and the
/// widened promotion set distinguish it from standard chess.
pub type Seirawan = GenericPosition<Chess8x8, SeirawanRules>;
