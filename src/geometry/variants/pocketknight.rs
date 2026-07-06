//! Pocket Knight chess (8x8) on the generic engine — **standard chess with one
//! extra Knight in hand per side**, droppable at any move, and nothing else
//! changed. It is the reference [`StandardChess`] ruleset over [`Chess8x8`] with a
//! single addition: each side starts holding one Knight in a pocket, and a held
//! Knight may be **dropped** onto any empty square as an ordinary move.
//!
//! Pocket Knight is the simplest possible **drop** variant: it reuses the standard
//! army, the standard 8x8 geometry, standard castling, the standard pawn
//! double-step / en passant / promotion, and standard checkmate — it only adds the
//! starting pocket and enables drops. Crucially it is **not** crazyhouse: captures
//! do **not** bank into the hand ([`captures_to_hand`](WideVariant::captures_to_hand)
//! is `false`), so the pocket only ever holds the one starting Knight, and once a
//! side drops it the pocket stays empty for the rest of the game.
//!
//! ## Rules — standard chess plus a pocket Knight
//!
//! * **One Knight in hand per side.** The starting state carries a pocket of a
//!   single Knight for White and for Black (rendered in the FEN as the trailing
//!   `[Nn]` holdings bracket). [`has_hand`](WideVariant::has_hand) is `true` so the
//!   generic generator emits drops.
//! * **Drops.** A held Knight may be dropped onto **any empty square** (the
//!   crazyhouse-style default drop targets; a Knight has no last-rank restriction),
//!   counted as one ply. A drop that gives check or mate is legal.
//! * **Captures do not replenish the hand.**
//!   [`captures_to_hand`](WideVariant::captures_to_hand) is `false` — a capture
//!   banks nothing, so the pocket is a one-shot reserve, exactly like Synochess and
//!   Shinobi's fixed reserves (FSF `capturesToHand = false`).
//! * **Castling** is standard (both sides keep full `KQkq` rights).
//! * **Pawns** are ordinary chess pawns
//!   ([`pawn_is_stepper`](WideVariant::pawn_is_stepper) is `false`, overriding the
//!   Shogi-stepper default that a hand would otherwise imply): they double-push,
//!   capture diagonally, take en passant, and promote to Queen, Rook, Bishop, or
//!   Knight.
//! * **Win by checkmate**, standard 8x8 chess otherwise.
//!
//! No draw hook is overridden: like the reference [`StandardChess`], Pocket Knight
//! carries the trait-default terminal rules (no fifty-move / repetition /
//! insufficient-material adjudication at the bare-position level), matching
//! Fairy-Stockfish's `pocketknight` for perft.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant pocketknight`
//! (`pocketknight_variant()`, `variant.cpp:688` — the standard `chess_variant()`
//! with `pieceDrops = true`, `capturesToHand = false`, and a starting `[Nn]`
//! pocket). The array is the standard chess array; the holdings bracket is the one
//! Knight per side, and the castling field is the standard `KQkq`:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[Nn] w KQkq - 0 1
//! ```
//!
//! mcr and FSF spell the position byte-for-byte identically (standard chess
//! letters, the Knight banked as `N`/`n`, no dialect rewrite).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
#[allow(unused_imports)] // `StandardChess` is referenced by the rustdoc intra-doc links.
use crate::geometry::StandardChess;
use crate::geometry::{Bitboard, Board, Chess8x8, WideRole, WideVariant};
use crate::Color;

/// The Pocket Knight starting placement: the standard chess array (the pocket
/// Knights live in hand, not on the board).
const POCKETKNIGHT_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

/// The Pocket Knight rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It is the reference [`StandardChess`] ruleset with exactly one addition — each
/// side starts with a single Knight in hand ([`has_hand`](WideVariant::has_hand)
/// is `true`) that may be dropped onto any empty square. Captures do **not** bank
/// into the hand ([`captures_to_hand`](WideVariant::captures_to_hand) is `false`),
/// so the pocket is a one-shot reserve. Every piece, castling, the pawn
/// double-step, en passant, promotion, and checkmate are the standard-chess trait
/// defaults, so only the pocket Knight distinguishes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PocketknightRules;

impl PocketknightRules {
    /// The starting hand: one Knight for each side, held in the pocket.
    fn starting_hand() -> GenericPlacement {
        let mut white = [0u8; WideRole::COUNT];
        let mut black = [0u8; WideRole::COUNT];
        white[WideRole::Knight.index()] = 1;
        black[WideRole::Knight.index()] = 1;
        GenericPlacement::new(white, black)
    }
}

impl WideVariant<Chess8x8> for PocketknightRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (Pawn..King, the standard army; the pocket and every
    /// promotion — Queen / Rook / Bishop / Knight — are all within the prefix). See
    /// [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(POCKETKNIGHT_START_PLACEMENT)
            .expect("the standard starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::standard::<Chess8x8>(),
            ep_square: None,
            ep_captured: None,
            gating: GenericGating::NONE,
            duck: None,
            // One Knight per side in the pocket.
            placement: Self::starting_hand(),
            halfmove_clock: 0,
            fullmove_number: 1,
            consecutive_passes: 0,
            board_b: Bitboard::EMPTY,
            petrified: Bitboard::EMPTY,
        };
        (board, state)
    }

    // --- The pocket Knight: a hand with drops, but not captures-to-hand ----

    /// Pocket Knight has a hand (the starting Knight pocket), so the generic
    /// generator emits drops and renders the `[Nn]` holdings bracket.
    fn has_hand() -> bool {
        true
    }

    /// Captures do **not** replenish the hand — the pocket is a one-shot reserve
    /// (FSF `capturesToHand = false`), like Synochess and Shinobi. A capture banks
    /// nothing, so once a side drops its Knight the pocket stays empty.
    fn captures_to_hand() -> bool {
        false
    }

    /// Pocket Knight pawns are ordinary chess pawns (double push, diagonal capture,
    /// en passant), not Shogi forward-steppers — overriding the stepper default a
    /// hand would otherwise imply.
    fn pawn_is_stepper() -> bool {
        false
    }

    // `drop_targets` keeps its default — every empty square (crazyhouse). The only
    // role ever in hand is a Knight, which has no last-rank restriction, so a held
    // Knight may be dropped onto any empty square, matching FSF `pocketknight`.
}

/// Pocket Knight chess (8x8) as a [`GenericPosition`] over the 8x8 [`Chess8x8`]
/// geometry.
///
/// Construct the starting position (the standard chess array with one Knight in
/// each side's pocket) with [`Pocketknight::startpos`](GenericPosition::startpos)
/// or parse a FEN — including the `[Nn]` holdings bracket — with
/// [`Pocketknight::from_fen`](GenericPosition::from_fen). Every rule is the
/// standard [`StandardChess`] default except the added pocket Knight and drops.
pub type Pocketknight = GenericPosition<Chess8x8, PocketknightRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::WideMoveKind;

    /// The canonical start FEN round-trips, carries the `[Nn]` pocket, keeps full
    /// castling rights, and has the 20 board moves plus the Knight drops available.
    #[test]
    fn startpos_fen_round_trips_with_pocket_knight() {
        let pos = Pocketknight::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[Nn] w KQkq - 0 1"
        );
        assert_eq!(pos.turn(), Color::White);
        assert!(pos.castling().has_any(Color::White));
        assert!(pos.castling().has_any(Color::Black));
    }

    /// From the opening array the pocket Knight can drop onto any of the empty
    /// squares, so the move count is the 20 standard opening moves plus one Knight
    /// drop per empty square.
    #[test]
    fn opening_has_standard_moves_plus_knight_drops() {
        let pos = Pocketknight::startpos();
        let drops = pos
            .legal_moves()
            .into_iter()
            .filter(|m| matches!(m.kind(), WideMoveKind::Drop { .. }))
            .count();
        // 64 squares minus the 32 occupied by the starting armies = 32 empty
        // squares, each a legal Knight drop.
        assert_eq!(drops, 32, "a Knight may drop onto every empty square");
        // 20 standard opening board moves + 32 drops.
        assert_eq!(pos.legal_move_count(), 20 + 32);
    }

    /// A capture does **not** bank the taken piece into the hand: Pocket Knight is
    /// not crazyhouse. After a capture the captor's pocket still holds exactly the
    /// one starting Knight (never two).
    #[test]
    fn capture_does_not_replenish_the_hand() {
        // White pawn on e4 can capture the black pawn on d5.
        let pos = Pocketknight::from_fen(
            "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR[Nn] w KQkq d6 0 3",
        )
        .expect("valid FEN");
        let capture = pos
            .legal_moves()
            .into_iter()
            .find(|m| matches!(m.kind(), WideMoveKind::Capture))
            .expect("exd5 is available");
        let next = pos.play(&capture);
        // The FEN still shows the one Knight per side — the captured pawn was not
        // banked.
        assert!(
            next.to_fen().contains("[Nn]"),
            "capture must not add to the pocket: {}",
            next.to_fen()
        );
    }

    /// Once a side drops its pocket Knight, the pocket is empty and stays empty (no
    /// captures-to-hand refill).
    #[test]
    fn pocket_empties_after_the_knight_is_dropped() {
        let pos = Pocketknight::startpos();
        let drop = pos
            .legal_moves()
            .into_iter()
            .find(|m| matches!(m.kind(), WideMoveKind::Drop { .. }))
            .expect("a Knight drop exists at the start");
        let next = pos.play(&drop);
        // White's pocket is now empty; only Black's Knight (`n`) remains.
        assert!(
            next.to_fen().contains("[n]"),
            "white's pocket empties after the drop: {}",
            next.to_fen()
        );
    }

    /// A double pawn push still sets an en-passant target — every non-pocket rule
    /// is standard chess.
    #[test]
    fn pawn_double_push_sets_en_passant() {
        let pos = Pocketknight::startpos();
        let dbl = pos
            .legal_moves()
            .into_iter()
            .find(|m| matches!(m.kind(), WideMoveKind::DoublePawnPush))
            .expect("a double pawn push exists at the start");
        let next = pos.play(&dbl);
        assert!(
            next.ep_square().is_some(),
            "a double push creates an en-passant target",
        );
    }
}
