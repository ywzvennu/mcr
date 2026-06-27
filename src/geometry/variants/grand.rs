//! Grand chess (10x10) on the generic engine — the first **10x10** variant on
//! the [`WideVariant`] layer (`docs/fairy-variants-architecture.md`, Phase 2,
//! Group C). It validates a **second** `u128` geometry ([`Grand10x10`], 100
//! squares) end-to-end against Fairy-Stockfish, after Capablanca proved the 10x8
//! [`Cap10x8`] path.
//!
//! Grand chess is played on a ten-files by ten-ranks board (files a..j, ranks
//! 1..10). Beyond the standard chess army each side adds two compound pieces:
//!
//! * **Marshal** (Rook + Knight) — mce's [`WideRole::Elephant`], whose default
//!   movement (`rook | knight`) is already the marshal's. FEN letter `e`/`E` in
//!   the mce dialect (Fairy-Stockfish spells the chancellor/marshal `c`/`C`, a
//!   dialect difference `compare-fairy/` reconciles, exactly as for Capablanca).
//! * **Cardinal** (Bishop + Knight) — mce's [`WideRole::Hawk`], whose default
//!   movement (`bishop | knight`) is already the cardinal's. FEN letter `a`/`A`
//!   in both mce and FSF.
//!
//! ## Rules that differ from standard chess
//!
//! * **No castling** (FSF `castling = false`).
//! * **Pawns** double-push from their start rank (rank 3 for white, rank 8 for
//!   black — the rank the pawns begin on), take en passant, and **promote in a
//!   three-rank zone**: the far three ranks (8, 9, 10 for white; 3, 2, 1 for
//!   black). Promotion is *optional* on the near two ranks of the zone (a pawn
//!   may instead push or capture and stay a pawn) and *forced* on the last rank
//!   (a pawn there would otherwise be immobile). This matches FSF's
//!   `mandatoryPawnPromotion = false` with `immobilityIllegal = true`.
//! * **Promote only to an already-captured piece type.** A pawn may promote to a
//!   role only while the player has fewer than the **starting army count** of
//!   that role on the board: at most one Queen, Marshal, or Cardinal, and at most
//!   two Rooks, Bishops, or Knights. Equivalently — since the army starts at
//!   exactly those counts — a pawn may promote to a type only after one of that
//!   type has been captured. This mirrors FSF's `promotionLimit` (Archbishop = 1,
//!   Chancellor = 1, Queen = 1, Rook = 2, Bishop = 2, Knight = 2), read live from
//!   the board, so no extra position state is needed.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant grand` / `position startpos`
//! (its `grand_variant()` `startFen`):
//!
//! ```text
//! FSF dialect: r8r/1nbqkcabn1/pppppppppp/10/10/10/10/PPPPPPPPPP/1NBQKCABN1/R8R w - - 0 1
//! mce dialect: r8r/1nbqkeabn1/pppppppppp/10/10/10/10/PPPPPPPPPP/1NBQKEABN1/R8R w - - 0 1
//! ```
//!
//! The two strings differ only in the marshal/chancellor's letter (`c` in FSF,
//! `e` in mce). The corners (a1/j1, a10/j10) hold the rooks; rank 2 (white) and
//! rank 9 (black) hold, files b..i, knight-bishop-queen-king-**marshal**-**cardinal**-bishop-knight,
//! with the king on the e-file (file 4) and the marshal beside it on the f-file
//! (file 5). Pawns sit on ranks 3 and 8.

use crate::geometry::position::{GenericCastling, GenericGating, GenericPosition, GenericState};
use crate::geometry::{Board, Grand10x10, PromotionConfig, WideRole, WideVariant};
use crate::Color;

/// The Grand chess rule layer: a zero-sized [`WideVariant`] over [`Grand10x10`].
///
/// It overrides only what Grand changes from the standard generic engine: the
/// 10x10 starting array, the absence of castling, the pawn start (double-push)
/// rank, the three-rank promotion zone with its optional/forced split, and the
/// promote-only-to-a-captured-type rule. The Marshal ([`WideRole::Elephant`]) and
/// Cardinal ([`WideRole::Hawk`]) movement is already the trait default, so no
/// `role_attacks` override is needed; pawns (bar promotion), knights, sliders, and
/// the king are standard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GrandRules;

/// The confirmed Grand starting placement in the mce dialect (marshal = `e`/`E`),
/// byte-for-byte equivalent to Fairy-Stockfish's
/// `r8r/1nbqkcabn1/.../1NBQKCABN1/R8R` modulo the marshal's letter.
const GRAND_START_PLACEMENT: &str =
    "r8r/1nbqkeabn1/pppppppppp/10/10/10/10/PPPPPPPPPP/1NBQKEABN1/R8R";

/// The starting army count of each promotable role — the FSF `promotionLimit`
/// for Grand. A pawn may promote to a role only while the player has fewer than
/// this many of it on the board (so a single Queen / Marshal / Cardinal, two each
/// of Rook / Bishop / Knight). Listed in the deterministic promotion order.
const PROMOTION_LIMITS: [(WideRole, u32); 6] = [
    (WideRole::Knight, 2),
    (WideRole::Bishop, 2),
    (WideRole::Rook, 2),
    (WideRole::Queen, 1),
    (WideRole::Hawk, 1),     // Cardinal (B+N)
    (WideRole::Elephant, 1), // Marshal (R+N)
];

impl WideVariant<Grand10x10> for GrandRules {
    fn starting_position() -> (Board<Grand10x10>, GenericState<Grand10x10>) {
        let board = Board::<Grand10x10>::from_fen_placement(GRAND_START_PLACEMENT)
            .expect("the Grand starting placement is valid on a 10x10 board");
        let state = GenericState {
            turn: Color::White,
            // Grand has no castling.
            castling: GenericCastling::NONE,
            ep_square: None,
            gating: GenericGating::NONE,
            halfmove_clock: 0,
            fullmove_number: 1,
        };
        (board, state)
    }

    fn promotion_config() -> PromotionConfig {
        // The full Grand promotion army, in FSF's order (knight, bishop, rook,
        // queen, cardinal, marshal). `promotion_targets` filters this per move by
        // the starting-army limit; this is the unfiltered superset (and the FEN /
        // round-trip vocabulary).
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Knight,
                WideRole::Bishop,
                WideRole::Rook,
                WideRole::Queen,
                WideRole::Hawk,     // Cardinal (B+N)
                WideRole::Elephant, // Marshal (R+N)
            ],
        }
    }

    fn promotion_targets(color: Color, board: &Board<Grand10x10>) -> alloc::vec::Vec<WideRole> {
        // A pawn may promote to a role only while the player holds fewer than the
        // starting count of it on the board — FSF's `promotionLimit`. Counting the
        // live board makes "promote only to a captured type" exact for the
        // single-copy pieces and correct for the two-copy ones, with no extra
        // state to thread through make/unmake.
        PROMOTION_LIMITS
            .iter()
            .filter(|&&(role, limit)| board.pieces(color, role).count() < limit)
            .map(|&(role, _)| role)
            .collect()
    }

    fn promotion_rank(color: Color) -> u8 {
        // The forced (last) promotion rank: rank 10 (index 9) for white, rank 1
        // (index 0) for black. The optional near ranks are handled by
        // `in_promotion_zone` / `promotion_is_forced`.
        match color {
            Color::White => 9,
            Color::Black => 0,
        }
    }

    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        // The far three ranks: 8, 9, 10 (indices 7, 8, 9) for white; 1, 2, 3
        // (indices 0, 1, 2) for black.
        match color {
            Color::White => rank >= 7,
            Color::Black => rank <= 2,
        }
    }

    fn promotion_is_forced(color: Color, rank: u8) -> bool {
        // Forced only on the final rank; optional on the near two zone ranks.
        rank == Self::promotion_rank(color)
    }

    fn double_push_rank(color: Color) -> u8 {
        // Pawns start on (and double-push from) rank 3 (index 2) for white and
        // rank 8 (index 7) for black.
        match color {
            Color::White => 2,
            Color::Black => 7,
        }
    }

    fn has_castling() -> bool {
        false
    }
}

/// Grand chess as a [`GenericPosition`] over the 10x10 [`Grand10x10`] geometry.
///
/// Construct the starting position with
/// [`Grand::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Grand::from_fen`](GenericPosition::from_fen). The Marshal and Cardinal reuse
/// the [`StandardChess`](crate::geometry::StandardChess) compound defaults, so
/// only the array, no-castling, pawn rules, promotion zone, and
/// promote-to-captured rule distinguish it.
pub type Grand = GenericPosition<Grand10x10, GrandRules>;
