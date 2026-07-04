//! S-House / Seirawan-house (8x8) on the generic engine — **Seirawan gating**
//! (Hawk / Elephant) composed with **Crazyhouse drops**. Validated against
//! Fairy-Stockfish `UCI_Variant shouse` (<https://www.pychess.org/variant/shouse>).
//!
//! S-House is standard 8x8 chess with two extra mechanics, both riding a single
//! shared **hand**:
//!
//! * **Crazyhouse drops** — a captured piece flips to the captor's side, reverts
//!   to its base role, and enters the captor's hand, from which it may later be
//!   **dropped** onto an empty square (a Pawn only onto ranks 2-7; every other
//!   piece onto any empty square). A pawn that promoted and is then captured banks
//!   as a **Pawn** (the crazyhouse "promoted pieces demote" rule, tracked by the
//!   generic promoted mask and rendered in the FEN as a trailing `~`, e.g. `Q~`),
//!   so S-House sets [`WideVariant::demotes_promoted_captures`].
//! * **Seirawan gating** — the Hawk ([`WideRole::Hawk`], B+N) and Elephant
//!   ([`WideRole::Elephant`], R+N) start **in the hand**, one of each per side.
//!   When a piece standing on its **original back-rank square** makes its first
//!   move (castling counts for both king and rook), the player **may**
//!   simultaneously **gate** a held piece onto the vacated square.
//!
//! ## The unified hand (gate *or* drop)
//!
//! Unlike plain Seirawan — whose two reserves can only be *gated* — S-House keeps
//! all reserves in the **crazyhouse hand**, so the starting Hawk and Elephant are
//! **droppable as well as gateable**, and **any** held non-pawn, non-king role
//! (a captured Knight, Bishop, Rook, Queen, Hawk, or Elephant) may be gated.
//! Confirmed against FSF: at the start, both `H@`/`E@` drops and the gates are
//! legal; with a captured Knight in hand a back-rank piece's first move may gate
//! that Knight. S-House therefore sets [`WideVariant::gates_from_hand`], drawing
//! the gated piece from [`GenericPlacement`]
//! rather than the fixed Hawk/Elephant reserve; the [`GenericGating`] state still
//! supplies the gating-**eligible square set** (the virgin back-rank squares).
//!
//! A Pawn is never gated, and the King is never in hand.
//!
//! ## Promotion
//!
//! A Pawn promotes (via the standard pawn path) to any of Knight, Bishop, Rook,
//! Queen, Hawk, or Elephant — the same six targets as Seirawan.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant shouse`:
//!
//! ```text
//! FSF dialect: rnbqkbnr/pppppppppp.../RNBQKBNR[HEhe] w KQBCDFGkqbcdfg - 0 1
//! mcr dialect: rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[AEae] w KQBCDFGkqbcdfg - 0 1
//! ```
//!
//! The two differ only in the Hawk's letter (`H` in FSF, `a`/`A` in the mcr census
//! dialect — the same one Capablanca's Archbishop uses; the Elephant is `e`/`E` in
//! both). The `compare-fairy/` harness reconciles that one letter. The `[AEae]` is
//! the crazyhouse hand (the starting Hawk/Elephant reserves) and the
//! `KQBCDFGkqbcdfg` castling field carries the castling rights plus the
//! gating-eligible back-rank files, exactly as in Seirawan.
//!
//! ## Insufficient material — deliberately **default-off** (#350)
//!
//! Unlike plain [Seirawan](super::seirawan) — which opts into the material-draw
//! hook once its gating reserve empties — S-House does **not**, because it is a
//! **captures-to-hand** (crazyhouse) variant: every capture banks the taken piece
//! into the hand, from which it may be **dropped** back onto the board. Material is
//! therefore never exhausted — even a bare-king position can sprout a queen on the
//! next ply — so no insufficient-material draw can ever hold. This mirrors
//! Fairy-Stockfish's `has_insufficient_material`, whose very first guard returns
//! "sufficient" whenever `captures_to_hand()` is set; verified against
//! `UCI_Variant shouse`, where even `KvK` with an empty hand is *not* drawn. The
//! hook stays at its `false` default and the variant reports no material draw.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    Bitboard, Board, Chess8x8, Geometry, PromotionConfig, Square, WideRole, WideVariant,
};
use crate::Color;

/// The S-House rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ShouseRules;

/// The standard 8x8 starting placement (the reserves live in hand, not on the
/// board).
const SHOUSE_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

impl ShouseRules {
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

    /// The starting hand: one Hawk and one Elephant for each side, held in the
    /// crazyhouse pocket (gateable *and* droppable).
    fn starting_hand() -> GenericPlacement {
        let mut white = [0u8; WideRole::COUNT];
        let mut black = [0u8; WideRole::COUNT];
        white[WideRole::Hawk.index()] = 1;
        white[WideRole::Elephant.index()] = 1;
        black[WideRole::Hawk.index()] = 1;
        black[WideRole::Elephant.index()] = 1;
        GenericPlacement::new(white, black)
    }

    /// The squares a Pawn may **not** be dropped onto: the first and last rank
    /// (rank 1 and rank 8), where a dropped pawn would be on a promotion rank.
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

impl WideVariant<Chess8x8> for ShouseRules {
    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(SHOUSE_START_PLACEMENT)
            .expect("the S-House starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::standard::<Chess8x8>(),
            ep_square: None,
            gating: Self::initial_gating(),
            duck: None,
            placement: Self::starting_hand(),
            halfmove_clock: 0,
            fullmove_number: 1,
            consecutive_passes: 0,
            board_b: Bitboard::EMPTY,
        };
        (board, state)
    }

    fn promotion_config() -> PromotionConfig {
        // The same six targets as Seirawan: the four standard non-pawn roles plus
        // the two reserve compounds. FSF lists the Hawk and Elephant among the
        // legal promotions, so the perft branching matches only with both included.
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

    // --- Seirawan gating, sourced from the crazyhouse hand ----------------

    fn supports_gating() -> bool {
        true
    }

    fn gates_from_hand() -> bool {
        true
    }

    fn initial_gating() -> GenericGating<Chess8x8> {
        // Only the eligible-square set matters here; the gateable pieces come from
        // the hand, so the fixed Hawk/Elephant reserve is left empty.
        GenericGating::new(Self::opening_eligible(), [false, false], [false, false])
    }

    // --- Crazyhouse hand + drops ------------------------------------------

    fn has_hand() -> bool {
        true
    }

    // `captures_to_hand` keeps its default `true` (crazyhouse banks captures);
    // `role_hand_base` keeps its default (each captured piece banks as itself —
    // the promoted-pawn-reverts-to-Pawn case is handled by the promoted mask).

    fn pawn_is_stepper() -> bool {
        // S-House pawns are ordinary chess pawns (double push, diagonal capture,
        // en passant), not Shogi forward-steppers.
        false
    }

    fn demotes_promoted_captures() -> bool {
        // The crazyhouse rule: a captured piece that reached the board by
        // promotion banks as a Pawn (`Q~` -> `p` in hand), tracked by the generic
        // promoted mask.
        true
    }

    fn drop_targets(role: WideRole, _color: Color, board: &Board<Chess8x8>) -> Bitboard<Chess8x8> {
        // Every empty square (crazyhouse) — except a Pawn may not be dropped on the
        // first or last rank (FSF confirms pawn drops only on ranks 2-7). There is
        // no nifu, and drops giving check or mate are legal.
        let empty = !board.occupied();
        if role == WideRole::Pawn {
            empty & !Self::pawn_forbidden_ranks()
        } else {
            empty
        }
    }
}

/// S-House / Seirawan-house (8x8) as a [`GenericPosition`] over the 8x8
/// [`Chess8x8`] geometry.
///
/// Construct the starting position (the standard chess array with a Hawk and
/// Elephant in each side's hand) with
/// [`Shouse::startpos`](GenericPosition::startpos) or parse a FEN — including the
/// `[AEae]` crazyhouse hand, the `KQBCDFGkqbcdfg` gating rights, and crazyhouse
/// `~` promotion marks — with [`Shouse::from_fen`](GenericPosition::from_fen). See
/// the [module docs](self) for the unified gate/drop hand and the promotion rule.
pub type Shouse = GenericPosition<Chess8x8, ShouseRules>;
