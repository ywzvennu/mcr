//! Grandhouse (10x10) on the generic engine — **Grand chess plus crazyhouse
//! drops**. It is [`Grand`](super::grand::Grand) (the ten-by-ten board with the
//! Marshal and Cardinal compounds, the three-rank promotion zone, and the
//! promote-only-to-a-captured-type rule) with the
//! [`Capahouse`](super::capahouse::Capahouse)/crazyhouse **hand**: every captured
//! piece flips to the captor's side, enters the hand, and may later be **dropped**
//! back onto an empty square as a move. Validated against Fairy-Stockfish
//! `UCI_Variant grandhouse` (`[grandhouse:grand]` in FSF's `variants.ini`).
//!
//! ## Pieces and movement
//!
//! Identical to Grand — every rule of movement, the absence of castling, the
//! pawn double-push rank, the three-rank promotion zone, and the
//! promote-only-to-a-captured-type rule are inherited unchanged by delegating to
//! [`GrandRules`]:
//!
//! * **Marshal** (Rook + Knight) — [`WideRole::Elephant`], FEN letter `e`/`E` in
//!   the mcr dialect (Fairy-Stockfish spells the marshal/chancellor `c`/`C`; the
//!   `compare-fairy/` harness reconciles the one-letter difference).
//! * **Cardinal** (Bishop + Knight) — [`WideRole::Hawk`], FEN letter `a`/`A` in
//!   both dialects.
//! * Pawns double-push from rank 3 (white) / rank 8 (black), take en passant, and
//!   promote in the far three ranks (8, 9, 10 for white; 3, 2, 1 for black) — to a
//!   role only while the player holds fewer than the starting count of it, exactly
//!   as in Grand.
//!
//! ## Hand and drops (crazyhouse)
//!
//! A captured piece banks to the captor's hand. A **natural** piece banks as its
//! own role; a piece that itself reached the board **by promotion** banks as a
//! **Pawn** (the crazyhouse "promoted pieces demote" rule, tracked by the generic
//! promoted mask and rendered in the FEN as a trailing `~`, e.g. `Q~`). From the
//! hand a side may **drop** a held piece onto any empty square, with one
//! restriction confirmed against FSF: a **Pawn may not be dropped on its own back
//! rank nor anywhere in the promotion zone**. On the 10x10 board this leaves a
//! white pawn droppable only on ranks 2-7 and a black pawn only on ranks 4-9
//! (FSF's default `firstRankPawnDrops = false` + `promotionZonePawnDrops = false`
//! over Grand's three-rank zone). There is no *nifu* (a dropped pawn may share a
//! file with another pawn) and drops giving check or mate are legal.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant grandhouse` (`[grandhouse:grand]`
//! `startFen`):
//!
//! ```text
//! FSF dialect: r8r/1nbqkcabn1/pppppppppp/10/10/10/10/PPPPPPPPPP/1NBQKCABN1/R8R[] w - - 0 1
//! mcr dialect: r8r/1nbqkeabn1/pppppppppp/10/10/10/10/PPPPPPPPPP/1NBQKEABN1/R8R[] w - - 0 1
//! ```
//!
//! The two differ only in the marshal's letter (`c` in FSF, `e` in mcr); the
//! trailing `[]` is the empty crazyhouse hand. There is no castling (`-`), as in
//! Grand.

use crate::geometry::position::{GenericPosition, GenericState};
use crate::geometry::variants::grand::GrandRules;
use crate::geometry::{
    Bitboard, Board, Geometry, Grand10x10, PromotionConfig, Square, WideRole, WideVariant,
};
use crate::Color;

/// The Grandhouse rule layer: a zero-sized [`WideVariant`] over [`Grand10x10`].
///
/// It is the Grand rule layer plus the crazyhouse hand. Every Grand-specific rule
/// (the starting array, no castling, the pawn double-push rank, the three-rank
/// promotion zone with its optional/forced split, and the promote-to-a-captured-
/// type limit) is **delegated** to [`GrandRules`] so Grandhouse movement stays
/// byte-identical to Grand; this layer adds only the hand hooks
/// ([`has_hand`](WideVariant::has_hand),
/// [`demotes_promoted_captures`](WideVariant::demotes_promoted_captures), and the
/// colour-aware pawn drop region).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GrandhouseRules;

impl WideVariant<Grand10x10> for GrandhouseRules {
    /// The tightest prefix of [`WideRole::ALL`] that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 12;

    /// The western **fifty-move rule**: a position whose halfmove clock has
    /// reached 100 plies (50 full moves with no capture or pawn move) is a
    /// [`WideEndReason::MoveRule`](crate::geometry::WideEndReason::MoveRule) draw,
    /// matching Fairy-Stockfish's default `nMoveRule = 50` for this standard-army
    /// large board. Adjudication-only (the clock never gates move generation), so
    /// perft stays byte-identical.
    fn move_rule_plies() -> Option<u16> {
        Some(100)
    }

    /// Records a position history so the standard **threefold** repetition draw
    /// ([`WideEndReason::Repetition`](crate::geometry::WideEndReason::Repetition),
    /// fold 3) fires at the [`GenericGame`](crate::geometry::game::GenericGame)
    /// level. History-dependent and never consulted by a bare
    /// [`GenericPosition`], so perft is unchanged.
    fn tracks_repetition() -> bool {
        true
    }

    // --- Grand rules, delegated unchanged ---------------------------------

    fn starting_position() -> (Board<Grand10x10>, GenericState<Grand10x10>) {
        // The Grand starting array (no castling, empty placement); `has_hand`
        // renders the empty hand as the FEN's trailing `[]`.
        GrandRules::starting_position()
    }

    fn promotion_config() -> PromotionConfig {
        GrandRules::promotion_config()
    }

    fn promotion_targets(color: Color, board: &Board<Grand10x10>) -> alloc::vec::Vec<WideRole> {
        // Grand's promote-only-to-a-captured-type limit, read live from the board.
        GrandRules::promotion_targets(color, board)
    }

    fn promotion_rank(color: Color) -> u8 {
        GrandRules::promotion_rank(color)
    }

    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        GrandRules::in_promotion_zone(color, rank)
    }

    fn promotion_is_forced(color: Color, rank: u8) -> bool {
        GrandRules::promotion_is_forced(color, rank)
    }

    fn double_push_rank(color: Color) -> u8 {
        GrandRules::double_push_rank(color)
    }

    fn has_castling() -> bool {
        false
    }

    // --- crazyhouse hand + drops ------------------------------------------

    fn has_hand() -> bool {
        true
    }

    fn pawn_is_stepper() -> bool {
        // Grandhouse pawns are ordinary Grand/chess pawns (double push, diagonal
        // capture, en passant, zone promotion), not Shogi forward steppers.
        false
    }

    fn demotes_promoted_captures() -> bool {
        // The crazyhouse rule: a captured piece that reached the board by
        // promotion banks as a Pawn (`Q~` -> `p` in hand), tracked by the generic
        // promoted mask. `captures_to_hand` keeps its default `true`, and
        // `role_hand_base` its default identity (a natural Marshal/Cardinal banks
        // as itself — `promoted_base` leaves the compounds untouched).
        true
    }

    fn drop_targets(
        role: WideRole,
        color: Color,
        board: &Board<Grand10x10>,
    ) -> Bitboard<Grand10x10> {
        // Every empty square (crazyhouse) — except that a Pawn may not be dropped
        // on its own back rank nor in the promotion zone (FSF confirms white pawn
        // drops on ranks 2-7 only, black on ranks 4-9 only). There is no nifu, so
        // no file filter.
        let empty = !board.occupied();
        if role == WideRole::Pawn {
            empty & !Self::pawn_forbidden_ranks(color)
        } else {
            empty
        }
    }
}

impl GrandhouseRules {
    /// The squares a Pawn of `color` may **not** be dropped onto: that colour's
    /// back rank (rank 1 for white, rank 10 for black) plus its promotion zone
    /// (the far three ranks). On the 10x10 board this forbids ranks 1 and 8-10 for
    /// white (leaving 2-7) and ranks 1-3 and 10 for black (leaving 4-9).
    fn pawn_forbidden_ranks(color: Color) -> Bitboard<Grand10x10> {
        let back_rank = match color {
            Color::White => 0,
            Color::Black => Grand10x10::HEIGHT - 1,
        };
        let mut bb = Bitboard::<Grand10x10>::EMPTY;
        for rank in 0..Grand10x10::HEIGHT {
            if rank == back_rank || GrandRules::in_promotion_zone(color, rank) {
                for file in 0..Grand10x10::WIDTH {
                    if let Some(sq) = Square::<Grand10x10>::from_file_rank(file, rank) {
                        bb.set(sq);
                    }
                }
            }
        }
        bb
    }
}

/// Grandhouse (10x10 Grand + crazyhouse drops) as a [`GenericPosition`] over the
/// 10x10 [`Grand10x10`] geometry.
///
/// Construct the starting position (the Grand array with an empty crazyhouse hand)
/// with [`Grandhouse::startpos`](GenericPosition::startpos) or parse a FEN — the
/// placement may carry the hand as a `[..]` bracket and promoted pieces as a `~`
/// suffix — with [`Grandhouse::from_fen`](GenericPosition::from_fen).
pub type Grandhouse = GenericPosition<Grand10x10, GrandhouseRules>;
