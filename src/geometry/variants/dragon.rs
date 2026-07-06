//! Dragon chess (8x8) on the generic engine — standard chess plus a single
//! **Dragon** piece held in each side's pocket, droppable only onto the player's
//! own back rank. Validated against Fairy-Stockfish `UCI_Variant dragon`.
//!
//! Dragon chess is ordinary 8x8 chess with one addition: each side starts with a
//! **Dragon** in hand — a Bishop + Knight compound (an Archbishop / Cardinal /
//! Janus, the same piece as Seirawan's Hawk and Capablanca's archbishop). The
//! Dragon reuses the existing [`WideRole::Hawk`] role (`B+N`), whose movement is
//! already the [`WideVariant`] default (`bishop | knight`), so no `role_attacks`
//! override is needed.
//!
//! ## The Dragon pocket
//!
//! Unlike crazyhouse, the pocket is **fixed**: a capture never banks a piece, so
//! the hand only ever holds the one starting Dragon (0 or 1 per side). The Dragon
//! may be **dropped onto an empty square of the player's own back rank** — rank 1
//! for White, rank 8 for Black (FSF `dropRegion = Rank1BB` / `Rank8BB`) — as a
//! turn in its own right. There is no other droppable piece (no pawn-drop rules
//! apply), and a drop may give check or mate.
//!
//! ## Promotion
//!
//! A Pawn promotes (via the standard pawn path) to a Knight, Bishop, Rook, Queen,
//! or **Dragon** (the Hawk compound) — FSF lists the Archbishop among the legal
//! promotion targets, so the perft branching matches only with the Hawk included.
//!
//! Every other rule — pawns (double step, diagonal capture, en passant), knights,
//! sliders, the king, and castling — is standard chess.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant dragon`:
//!
//! ```text
//! FSF dialect: rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[Dd] w KQkq - 0 1
//! mcr dialect: rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[Aa] w KQkq - 0 1
//! ```
//!
//! The two differ only in the Dragon's letter (`D`/`d` in FSF, `a`/`A` in the mcr
//! census dialect — the same one Capablanca's Archbishop and Seirawan's Hawk use).
//! The `[Aa]` is the fixed pocket (the starting Dragon reserves), and the `compare-fairy/`
//! harness rewrites that one letter when driving FSF.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    Bitboard, Board, Chess8x8, Geometry, PromotionConfig, Square, WideRole, WideVariant,
};
use crate::Color;

/// The Dragon rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides only what Dragon adds to standard chess: the fixed one-Dragon
/// pocket ([`has_hand`](WideVariant::has_hand) with
/// [`captures_to_hand`](WideVariant::captures_to_hand) off), its back-rank drops
/// ([`drop_targets`](WideVariant::drop_targets)), and the widened promotion set (a
/// pawn may also promote to a Dragon). The Dragon (`B+N`) reuses the
/// [`WideRole::Hawk`] compound, whose movement is the trait default, so there is no
/// `role_attacks` override; every other rule — pawns, knights, sliders, the king,
/// castling, en passant — is standard chess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DragonRules;

/// The standard 8x8 starting placement (the Dragon lives in hand, not on the
/// board).
const DRAGON_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

impl DragonRules {
    /// `color`'s own back rank: White's rank 0 (rank 1), Black's rank 7 (rank 8) —
    /// the only rank the Dragon may be dropped onto (FSF `dropRegion`).
    fn back_rank(color: Color) -> Bitboard<Chess8x8> {
        let rank = if color.is_white() {
            0
        } else {
            Chess8x8::HEIGHT - 1
        };
        let mut bb = Bitboard::<Chess8x8>::EMPTY;
        for file in 0..Chess8x8::WIDTH {
            if let Some(sq) = Square::<Chess8x8>::from_file_rank(file, rank) {
                bb.set(sq);
            }
        }
        bb
    }

    /// The starting hand: one Dragon (the [`WideRole::Hawk`] `B+N` compound) for
    /// each side, held in the fixed pocket.
    fn starting_hand() -> GenericPlacement {
        let mut white = [0u8; WideRole::COUNT];
        let mut black = [0u8; WideRole::COUNT];
        white[WideRole::Hawk.index()] = 1;
        black[WideRole::Hawk.index()] = 1;
        GenericPlacement::new(white, black)
    }
}

impl WideVariant<Chess8x8> for DragonRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 11;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(DRAGON_START_PLACEMENT)
            .expect("the Dragon starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::standard::<Chess8x8>(),
            ep_square: None,
            ep_captured: None,
            gating: GenericGating::NONE,
            duck: None,
            placement: Self::starting_hand(),
            halfmove_clock: 0,
            fullmove_number: 1,
            consecutive_passes: 0,
            board_b: Bitboard::EMPTY,
            petrified: Bitboard::EMPTY,
        };
        (board, state)
    }

    fn promotion_config() -> PromotionConfig {
        // A Dragon pawn promotes to any of Knight, Bishop, Rook, Queen, or the
        // Dragon (the Hawk `B+N` compound). FSF lists the Archbishop among the
        // legal promotion targets, so the perft branching matches only with the
        // Hawk included. The order affects only move enumeration, not the leaf
        // count.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Knight,
                WideRole::Bishop,
                WideRole::Rook,
                WideRole::Queen,
                WideRole::Hawk, // the Dragon: B+N compound
            ],
        }
    }

    // --- the fixed one-Dragon pocket -------------------------------------

    fn has_hand() -> bool {
        true
    }

    fn captures_to_hand() -> bool {
        // The pocket is fixed (FSF `capturesToHand = false`): a capture never banks
        // a piece, so the hand only ever holds the one starting Dragon.
        false
    }

    fn pawn_is_stepper() -> bool {
        // Dragon pawns are ordinary chess pawns (double push, diagonal capture, en
        // passant), not Shogi forward-steppers — `pawn_is_stepper` defaults to
        // `has_hand()`, so it must be turned off here.
        false
    }

    fn drop_targets(role: WideRole, color: Color, board: &Board<Chess8x8>) -> Bitboard<Chess8x8> {
        // Only the Dragon (the Hawk compound) drops, only onto an empty square of
        // the dropping side's own back rank.
        if role != WideRole::Hawk {
            return Bitboard::EMPTY;
        }
        Self::back_rank(color) & !board.occupied()
    }
}

/// Dragon chess (8x8) as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position (the standard chess array with a Dragon in each
/// side's hand) with [`Dragon::startpos`](GenericPosition::startpos) or parse a FEN
/// — including the `[Aa]` Dragon pocket — with
/// [`Dragon::from_fen`](GenericPosition::from_fen). The Dragon reuses the generic
/// Hawk (`B+N`) movement default; only the fixed pocket, the back-rank drops, and
/// the widened promotion set distinguish it from standard chess.
pub type Dragon = GenericPosition<Chess8x8, DragonRules>;
