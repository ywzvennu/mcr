//! Shatar (Mongolian chess) on the generic engine — an 8x8 variant whose queen
//! is replaced by the limited **Bers** and whose pawn / castling / draw rules
//! differ from standard chess (issue #229).
//!
//! Shatar is an 8x8 variant ([`Chess8x8`] geometry) confirmed square-for-square
//! against Fairy-Stockfish `UCI_Variant shatar`. Its pieces are:
//!
//! * **Rook** (rook) — a standard rook. ([`WideRole::Rook`])
//! * **Knight** (knight) — a standard knight. ([`WideRole::Knight`])
//! * **Bishop** (bishop) — a standard bishop. ([`WideRole::Bishop`])
//! * **Bers** ([`WideRole::General`], the Spartan/Shinobi Rook + Ferz) — slides
//!   any distance orthogonally (rook) **plus** a single diagonal step (ferz). A
//!   "limited queen": it cannot slide along a diagonal. This is FSF's `bers`
//!   piece (Betza `RF`), the same movement [`WideRole::General`] already models
//!   for Spartan and Shinobi, so no new role is added.
//! * **King** (king) — a standard king.
//! * **Pawn** — moves one square straight forward only (**no** double push, hence
//!   **no** en passant), captures one square diagonally forward, and **promotes
//!   to a Bers** on the last rank (the only promotion target).
//!
//! There is **no castling**.
//!
//! ## The Robado (bare-king) draw — *does* affect perft
//!
//! Shatar's distinctive terminal rule is **Robado**: the instant a side is
//! stripped of every piece but its king, the game is an immediate **draw** (FSF
//! `extinctionValue = VALUE_DRAW`, `extinctionPieceCount = 1` over all piece
//! types). Unlike Makruk's counting rule, this one **does** change perft: a
//! position with a bare king on **either** side is terminal, so FSF's `go perft`
//! generates **zero** continuations from it regardless of whose turn it is. The
//! generic engine reproduces this through the default-off
//! [`WideVariant::has_bare_king_draw`] hook, which truncates the move generator
//! to zero moves at a bare-king node and reports
//! [`WideEndReason::VariantDraw`](crate::geometry::WideEndReason). Every other
//! variant leaves the hook off and is byte-identical.
//!
//! ## Out of scope: the special check / mate rules
//!
//! Shatar has special mate rules — checkmate delivered by a knight does not win
//! (it is no mate), and a series of checks must contain a "shak" (a check by
//! rook, knight, or bers) to be a legal mate. These affect only the **value** of
//! a terminal position (whether a checkmate counts as a win), never the legal
//! move set, so they do not change perft and are not modelled here. FSF likewise
//! does not let them affect `go perft`; the perft validation confirms the move
//! generation is exact.
//!
//! ## Confirmed starting FEN
//!
//! The starting array is pinned against Fairy-Stockfish's `UCI_Variant shatar`
//! (whose dialect spells the Bers `j`; mce spells it `d`, the
//! [`WideRole::General`] letter, since `j` already names the Xiangqi Horse):
//!
//! ```text
//! FSF : rnbjkbnr/ppp1pppp/8/3p4/3P4/8/PPP1PPPP/RNBJKBNR w - - 0 1
//! mce : rnbdkbnr/ppp1pppp/8/3p4/3P4/8/PPP1PPPP/RNBDKBNR w - - 0 1
//! ```
//!
//! The Bers sits on the queen's file (file 3) beside the king (file 4). Both
//! d-pawns start **already advanced** to the fourth/fifth rank — Shatar has no
//! double step, so the array opens with the centre pawns pre-pushed (which is why
//! the start FEN carries no en-passant target).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Chess8x8, Geometry, PromotionConfig, Square, StandardChess, WideRole,
    WideVariant,
};
use crate::Color;

/// The Shatar rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides only what Shatar changes from standard chess — the Bers
/// movement (reusing [`WideRole::General`]), the starting array, the pawn rules
/// (single-step, promote to Bers), the absence of castling, and the Robado
/// bare-king draw. Everything else is the trait default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ShatarRules;

/// The confirmed Shatar starting FEN placement (mce dialect: the Bers is `d`,
/// the [`WideRole::General`] letter), validated against Fairy-Stockfish
/// `UCI_Variant shatar`. The centre pawns start pre-advanced (`3p4` / `3P4`).
const SHATAR_START_PLACEMENT: &str = "rnbdkbnr/ppp1pppp/8/3p4/3P4/8/PPP1PPPP/RNBDKBNR";

/// The four ferz (diagonal one-step) offsets — the Bers's diagonal component.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

impl WideVariant<Chess8x8> for ShatarRules {
    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(SHATAR_START_PLACEMENT)
            .expect("the Shatar starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            // Shatar has no castling.
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
            // Bers (= Spartan/Shinobi General): Rook + Ferz — orthogonal slides
            // plus a single diagonal step. The same movement those variants model.
            WideRole::General => {
                attacks::rook_attacks::<Chess8x8>(sq, occupancy)
                    | attacks::leaper_attacks::<Chess8x8>(sq, &FERZ_OFFSETS)
            }
            // Rook / Knight / Bishop / King and the pawn are standard: defer to the
            // trait default (`StandardChess` overrides no movement, so its
            // `role_attacks` *is* the trait default), keeping these pieces
            // byte-identical to standard chess.
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn role_is_slider(role: WideRole) -> bool {
        // The Bers slides orthogonally (its rook component can be pinned along a
        // line); its single diagonal step is a stepper. Every standard role keeps
        // the default classification.
        match role {
            WideRole::General => true,
            _ => <StandardChess as WideVariant<Chess8x8>>::role_is_slider(role),
        }
    }

    fn promotion_config() -> PromotionConfig {
        // A pawn promotes only to a Bers (= General); there is no choice of role.
        PromotionConfig {
            roles: alloc::vec![WideRole::General],
        }
    }

    fn double_push_rank(_color: Color) -> u8 {
        // The pawn never makes a double advance. Return a rank no pawn can stand
        // on (one past the last rank), so the generic pawn generator's
        // `from.rank() == double_push_rank` guard is never satisfied — there is no
        // double push and therefore no en-passant target is ever set.
        Chess8x8::HEIGHT
    }

    fn has_castling() -> bool {
        false
    }

    fn has_bare_king_draw() -> bool {
        // Robado: a side reduced to its lone king ends the game in an immediate
        // draw, truncating move generation to zero — a perft-affecting rule, so
        // it rides this default-off hook.
        true
    }
}

/// Shatar (Mongolian chess) as a [`GenericPosition`] over the 8x8 geometry.
///
/// Construct the starting position with [`Shatar::startpos`](GenericPosition::startpos)
/// or parse a FEN with [`Shatar::from_fen`](GenericPosition::from_fen).
pub type Shatar = GenericPosition<Chess8x8, ShatarRules>;
