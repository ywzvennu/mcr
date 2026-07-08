//! Raazuvaa chess (8x8) on the generic engine — **standard chess with both
//! castling and the pawn double-step disabled**, and nothing else changed. "The
//! chess of the Maldives." It is the reference [`StandardChess`] ruleset over
//! [`Chess8x8`] with two rules turned off: neither side may ever castle, and a
//! pawn advances only a single square, so no en-passant target is ever created.
//! Validated against Fairy-Stockfish `UCI_Variant raazuvaa`.
//!
//! Raazuvaa is standard chess minus castling and minus the two-square opening pawn
//! push: it reuses the standard army, the standard 8x8 geometry, standard diagonal
//! pawn captures, last-rank promotion, and standard checkmate. From the opening
//! array the games diverge immediately — every pawn has a single push instead of
//! two, so the opening move count is 12 (eight single pawn pushes plus the four
//! knight hops) rather than standard chess's 20.
//!
//! ## Rules — standard chess without castling or the double step
//!
//! * **No castling.** [`has_castling`](WideVariant::has_castling) is `false`, and
//!   the starting state carries no castling rights, so the generator never emits a
//!   castle. Every other king and rook move is standard.
//! * **No pawn double step.**
//!   [`pawn_may_double_push_from`](WideVariant::pawn_may_double_push_from) is
//!   `false` for every rank, so a pawn only ever advances one square.
//! * **No en passant.** [`has_en_passant`](WideVariant::has_en_passant) is
//!   `false`; with no double step there is no skipped square, so no en-passant
//!   target is ever recorded — en passant can never arise in play.
//! * **Pawns** capture diagonally and promote on the far rank (the trait defaults)
//!   to Queen, Rook, Bishop, or Knight.
//! * **Win by checkmate**, standard 8x8 chess otherwise.
//!
//! No draw hook is overridden: like the reference [`StandardChess`], raazuvaa
//! carries the trait-default terminal rules (no fifty-move / repetition /
//! insufficient-material adjudication at the bare-position level), matching
//! Fairy-Stockfish's `raazuvaa` for perft.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant raazuvaa`
//! (`raazuvaa_variant()`, `variant.cpp:147` — the standard `chess_variant_base()`
//! with `castling = false` and `doubleStep = false`). The array is the standard
//! chess array; the castling field is `-` (no rights):
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1
//! ```
//!
//! mcr and FSF spell the position byte-for-byte identically (standard chess
//! letters, no dialect rewrite).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
#[allow(unused_imports)] // `StandardChess` is referenced by the rustdoc intra-doc links.
use crate::geometry::StandardChess;
use crate::geometry::{Board, Chess8x8, WideVariant};
use crate::Color;

/// The confirmed raazuvaa starting placement: the standard chess array.
const RAAZUVAA_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

/// The raazuvaa chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It is the reference [`StandardChess`] ruleset with exactly two changes —
/// castling is disabled ([`has_castling`](WideVariant::has_castling) is `false`
/// and the start state grants no castling rights) and the pawn double step is
/// disabled ([`pawn_may_double_push_from`](WideVariant::pawn_may_double_push_from)
/// is `false`), which in turn removes en passant. Every piece, the single pawn
/// push, diagonal capture, promotion, and checkmate are the standard-chess trait
/// defaults, so only the two disabled features distinguish it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct RaazuvaaRules;

impl WideVariant<Chess8x8> for RaazuvaaRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (Pawn..King, the standard army; promotions are Queen
    /// / Rook / Bishop / Knight, all within the prefix). See
    /// [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(RAAZUVAA_START_PLACEMENT)
            .expect("the standard starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            // No castling rights: castling is disabled entirely.
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
            petrified: crate::geometry::Bitboard::EMPTY,
        };
        (board, state)
    }

    /// Raazuvaa has **no castling**. The generic move generator consults this to
    /// suppress castle generation, and the FEN layer to reject castling rights, so
    /// neither side can ever castle.
    fn has_castling() -> bool {
        false
    }

    /// Raazuvaa has **no pawn double step**: a pawn only ever advances a single
    /// square, so the double push is unavailable from every rank.
    fn pawn_may_double_push_from(_rank: u8, _color: Color) -> bool {
        false
    }

    /// Raazuvaa has **no en passant** — with no double step there is no skipped
    /// square, so no en-passant target is ever recorded or captured.
    fn has_en_passant() -> bool {
        false
    }
}

/// Raazuvaa chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Raazuvaa::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Raazuvaa::from_fen`](GenericPosition::from_fen). Every rule is the standard
/// [`StandardChess`] default except castling and the pawn double step (and hence
/// en passant), which are disabled.
pub type Raazuvaa = GenericPosition<
    Chess8x8,
    RaazuvaaRules,
    { <RaazuvaaRules as WideVariant<Chess8x8>>::ROLE_SPAN },
>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::WideMoveKind;

    /// The canonical start FEN round-trips, opens with the 12 FSF-confirmed moves
    /// (eight single pawn pushes plus four knight hops), and carries no castling
    /// rights.
    #[test]
    fn startpos_fen_round_trips_without_castling() {
        let pos = Raazuvaa::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1"
        );
        assert_eq!(pos.turn(), Color::White);
        // Eight single pawn pushes + four knight hops = 12 (standard chess has 20:
        // the eight extra double-pushes are gone).
        assert_eq!(pos.legal_move_count(), 12);
        assert!(!pos.castling().has_any(Color::White));
        assert!(!pos.castling().has_any(Color::Black));
    }

    /// No castle, no double step, and no en passant is ever generated — not from
    /// the opening, and not from a position where standard chess would offer each.
    #[test]
    fn no_castle_double_step_or_en_passant() {
        // Opening: no double push and no castle among the twelve legal moves.
        let pos = Raazuvaa::startpos();
        for m in pos.legal_moves() {
            assert!(
                !matches!(
                    m.kind(),
                    WideMoveKind::CastleKingside
                        | WideMoveKind::CastleQueenside
                        | WideMoveKind::DoublePawnPush
                        | WideMoveKind::EnPassant
                ),
                "raazuvaa never castles, double-steps, or takes en passant",
            );
        }
        // A pawn advancing a single square sets no en-passant target.
        let push = pos
            .legal_moves()
            .into_iter()
            .find(|m| m.from::<Chess8x8>().file() == 4 && m.promotion().is_none())
            .expect("the e-pawn can advance");
        let next = pos.play(&push);
        assert!(
            next.ep_square().is_none(),
            "a single pawn push sets no en-passant target",
        );
    }

    /// A castling-rich position (both kings home, both rooks on the corner files,
    /// empty back rank) emits **no** castle move — one of the two rules that
    /// separate raazuvaa from standard chess.
    #[test]
    fn no_castle_move_when_standard_chess_would() {
        let pos = Raazuvaa::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w - - 0 1").expect("valid FEN");
        let castles = pos
            .legal_moves()
            .into_iter()
            .filter(|m| {
                matches!(
                    m.kind(),
                    WideMoveKind::CastleKingside | WideMoveKind::CastleQueenside
                )
            })
            .count();
        assert_eq!(castles, 0, "raazuvaa never castles");
    }
}
