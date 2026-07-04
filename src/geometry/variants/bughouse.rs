//! Bughouse — the **single-board** rules on the generic engine. Bughouse is a
//! 2-board, 4-player **team** game: a piece captured on one board is handed to
//! the capturer's **partner** on the **other** board, who may later **drop** it.
//! Each board, taken on its own, plays exactly like **Crazyhouse with the hand
//! fed from outside** — standard 8x8 chess plus drops from a pocket — except that
//! a capture **does not** bank the taken piece into the captor's own hand (it
//! crosses to the partner board instead). Validated against Fairy-Stockfish
//! `UCI_Variant bughouse` (<https://www.pychess.org/variant/bughouse>).
//!
//! ## What this library models, and what it does not
//!
//! This type is the **per-board** rule layer only: legal move generation
//! (standard chess + crazyhouse-style drops) and a hand that is **injected
//! externally**. The **2-board linkage** — pairing two boards, routing a captured
//! piece to the partner's hand on the other board, the team clocks, sit/stall and
//! mate-relay rules — is **out of scope for this library**: it is a server (mcs)
//! concern. A server runs **two** [`Bughouse`] positions and, whenever a capture
//! occurs on one, calls
//! [`GenericPosition::inject_into_hand`](crate::geometry::GenericPosition::inject_into_hand)
//! on the partner board to deliver the captured piece (reverted to its base
//! role, flipped to the partner's color). That single hook is the entire
//! cross-board coupling this library exposes; everything else here is local,
//! deterministic, and single-board perft-able.
//!
//! ## Single-board rules
//!
//! * **Movement** — every piece is an ordinary chess piece: pawns double-push,
//!   capture diagonally, take en passant, and promote on the last rank to Knight,
//!   Bishop, Rook, or Queen; the king castles normally.
//! * **Drops** — a side may **drop** a held piece onto any empty square, with the
//!   one crazyhouse restriction confirmed against FSF: a **Pawn may not be dropped
//!   on the first or last rank** (only ranks 2-7). There is no *nifu* (a dropped
//!   pawn may share a file with another pawn) and a drop giving check or mate is
//!   legal.
//! * **Captures do NOT replenish the hand** — unlike Crazyhouse, a piece captured
//!   on this board leaves it entirely (it goes to the partner board), so it is
//!   **not** added to the captor's hand. This is the single rule that separates
//!   single-board Bughouse from Crazyhouse, and it is exactly FSF's `twoBoards`
//!   attribute (pocket pieces come "from an external source"). The hand is grown
//!   only by [`inject_into_hand`](crate::geometry::GenericPosition::inject_into_hand).
//!
//! Because a capture never banks, a promoted piece's demotion-on-capture rule
//! never fires single-board, so [`WideVariant::demotes_promoted_captures`] stays
//! at its `false` default. (When a server transfers a captured *promoted* piece to
//! the partner, it injects a **Pawn** — the demotion is applied at the transfer
//! site, not here.)
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant bughouse` start position. The
//! hand starts **empty** (no reserves), and Bughouse uses only standard chess
//! pieces, so the FEN — and every node count — is identical to standard chess at
//! the start:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1
//! ```
//!
//! The trailing `[]` is the empty crazyhouse hand (FSF accepts it present or
//! omitted). The piece letters `K Q R B N P` are shared between mcr and FSF, so no
//! dialect translation is needed.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{Bitboard, Board, Chess8x8, Geometry, Square, WideRole, WideVariant};
use crate::Color;

/// The Bughouse single-board rule layer: a zero-sized [`WideVariant`] over
/// [`Chess8x8`].
///
/// It is standard 8x8 chess plus the crazyhouse hand/drop machinery, with one
/// twist: [`captures_to_hand`](WideVariant::captures_to_hand) is `false`, so a
/// capture never banks into the captor's hand (the taken piece crosses to the
/// partner board). The hand is grown only by the external transfer API,
/// [`GenericPosition::inject_into_hand`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BughouseRules;

/// The standard 8x8 starting placement; the hand starts empty.
const BUGHOUSE_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

impl BughouseRules {
    /// The squares a Pawn may **not** be dropped onto: the first and last rank
    /// (rank 1 and rank 8), where a dropped pawn would sit on a promotion rank.
    fn pawn_forbidden_ranks() -> Bitboard<Chess8x8> {
        let mut bb = Bitboard::<Chess8x8>::EMPTY;
        for file in 0..Chess8x8::WIDTH {
            for rank in [0, Chess8x8::HEIGHT - 1] {
                if let Some(sq) = Square::<Chess8x8>::from_file_rank(file, rank) {
                    bb.set(sq);
                }
            }
        }
        bb
    }
}

impl WideVariant<Chess8x8> for BughouseRules {
    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(BUGHOUSE_START_PLACEMENT)
            .expect("the Bughouse starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::standard::<Chess8x8>(),
            ep_square: None,
            gating: GenericGating::NONE,
            duck: None,
            // The hand starts empty; it is fed externally (the partner board) and
            // rides in the FEN's `[..]` bracket.
            placement: GenericPlacement::NONE,
            halfmove_clock: 0,
            fullmove_number: 1,
            consecutive_passes: 0,
            board_b: Bitboard::EMPTY,
        };
        (board, state)
    }

    // Promotion uses the trait default (Knight, Bishop, Rook, Queen) — ordinary
    // chess promotion targets.

    // --- crazyhouse hand + drops, fed externally --------------------------

    fn has_hand() -> bool {
        true
    }

    fn captures_to_hand() -> bool {
        // The single-board Bughouse twist (FSF `twoBoards`): a captured piece is
        // *not* banked into the captor's hand — it crosses to the partner board.
        // The hand grows only via `GenericPosition::inject_into_hand`. This is the
        // one rule that distinguishes single-board Bughouse from Crazyhouse.
        false
    }

    fn pawn_is_stepper() -> bool {
        // Bughouse pawns are ordinary chess pawns (double push, diagonal capture,
        // en passant, last-rank promotion), not Shogi forward steppers.
        false
    }

    fn drop_targets(role: WideRole, _color: Color, board: &Board<Chess8x8>) -> Bitboard<Chess8x8> {
        // Every empty square (crazyhouse) — except a Pawn may not be dropped on the
        // first or last rank (FSF confirms pawn drops only on ranks 2-7). There is
        // no nifu, and a drop giving check or mate is legal.
        let empty = !board.occupied();
        if role == WideRole::Pawn {
            empty & !Self::pawn_forbidden_ranks()
        } else {
            empty
        }
    }
}

/// Bughouse (single-board) as a [`GenericPosition`] over the 8x8 [`Chess8x8`]
/// geometry.
///
/// This is one board of the 2-board game: standard chess plus drops from a hand
/// that is **fed externally**. Construct the starting position (the standard array
/// with an empty hand) with [`Bughouse::startpos`](GenericPosition::startpos) or
/// parse a FEN — the placement may carry the hand as a `[..]` bracket — with
/// [`Bughouse::from_fen`](GenericPosition::from_fen). To deliver a piece captured
/// on the partner board, call
/// [`inject_into_hand`](GenericPosition::inject_into_hand); to reclaim one, call
/// [`remove_from_hand`](GenericPosition::remove_from_hand). The 2-board
/// orchestration that wires those calls together is a **server** concern, not part
/// of this library — see the [module docs](self).
pub type Bughouse = GenericPosition<Chess8x8, BughouseRules>;
