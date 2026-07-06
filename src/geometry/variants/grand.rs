//! Grand chess (10x10) on the generic engine — the first **10x10** variant on
//! the [`WideVariant`] layer (`docs/fairy-variants-architecture.md`, Phase 2,
//! Group C). It validates a **second** `u128` geometry ([`Grand10x10`], 100
//! squares) end-to-end against Fairy-Stockfish, after Capablanca proved the 10x8
//! [`Cap10x8`](crate::geometry::Cap10x8) path.
//!
//! Grand chess is played on a ten-files by ten-ranks board (files a..j, ranks
//! 1..10). Beyond the standard chess army each side adds two compound pieces:
//!
//! * **Marshal** (Rook + Knight) — mcr's [`WideRole::Elephant`], whose default
//!   movement (`rook | knight`) is already the marshal's. FEN letter `e`/`E` in
//!   the mcr dialect (Fairy-Stockfish spells the chancellor/marshal `c`/`C`, a
//!   dialect difference `compare-fairy/` reconciles, exactly as for Capablanca).
//! * **Cardinal** (Bishop + Knight) — mcr's [`WideRole::Hawk`], whose default
//!   movement (`bishop | knight`) is already the cardinal's. FEN letter `a`/`A`
//!   in both mcr and FSF.
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
//! mcr dialect: r8r/1nbqkeabn1/pppppppppp/10/10/10/10/PPPPPPPPPP/1NBQKEABN1/R8R w - - 0 1
//! ```
//!
//! The two strings differ only in the marshal/chancellor's letter (`c` in FSF,
//! `e` in mcr). The corners (a1/j1, a10/j10) hold the rooks; rank 2 (white) and
//! rank 9 (black) hold, files b..i, knight-bishop-queen-king-**marshal**-**cardinal**-bishop-knight,
//! with the king on the e-file (file 4) and the marshal beside it on the f-file
//! (file 5). Pawns sit on ranks 3 and 8.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
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

/// The confirmed Grand starting placement in the mcr dialect (marshal = `e`/`E`),
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
    /// The tightest prefix of `WideRole::ALL` that still contains every role
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

    fn starting_position() -> (Board<Grand10x10>, GenericState<Grand10x10>) {
        let board = Board::<Grand10x10>::from_fen_placement(GRAND_START_PLACEMENT)
            .expect("the Grand starting placement is valid on a 10x10 board");
        let state = GenericState {
            turn: Color::White,
            // Grand has no castling.
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

    /// Grand keeps the standard chess army plus the always-mating Cardinal
    /// ([`WideRole::Hawk`]) and Marshal ([`WideRole::Elephant`]), so the ordinary
    /// insufficient-material draw applies on the 10x10 board: king vs king, king
    /// and a lone minor (bishop or knight) vs king, and same-colour bishops only.
    /// The two compounds count as mating material (matching Fairy-Stockfish, which
    /// classes the cardinal and marshal as major pieces). Adjudication-only and
    /// behind the default-off hook, so perft stays byte-identical.
    fn is_insufficient_material(
        board: &Board<Grand10x10>,
        _state: &GenericState<Grand10x10>,
    ) -> bool {
        crate::geometry::variant::standard_insufficient_material(board)
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

#[cfg(test)]
mod insufficient_material_tests {
    use super::Grand;
    use crate::geometry::{WideEndReason, WideOutcome};

    fn end_reason(fen: &str) -> Option<WideEndReason> {
        Grand::from_fen(fen).expect("valid grand fen").end_reason()
    }

    #[test]
    fn lone_kings_draw() {
        let pos = Grand::from_fen("5k4/10/10/10/10/10/10/10/10/5K4 w - - 0 1").expect("valid fen");
        assert_eq!(pos.end_reason(), Some(WideEndReason::InsufficientMaterial));
        assert_eq!(pos.outcome(), Some(WideOutcome::Draw));
    }

    #[test]
    fn king_and_single_minor_draw() {
        assert_eq!(
            end_reason("5k4/10/10/10/10/10/10/10/10/5KN3 w - - 0 1"),
            Some(WideEndReason::InsufficientMaterial)
        );
        assert_eq!(
            end_reason("5k4/10/10/10/10/10/10/10/10/5KB3 w - - 0 1"),
            Some(WideEndReason::InsufficientMaterial)
        );
    }

    #[test]
    fn same_colour_bishops_draw() {
        // White Ba1 and black Bb10 are both on the dark complex.
        assert_eq!(
            end_reason("1b3k4/10/10/10/10/10/10/10/10/B4K4 w - - 0 1"),
            Some(WideEndReason::InsufficientMaterial)
        );
    }

    #[test]
    fn opposite_colour_bishops_are_sufficient() {
        // White Ba1 (dark) vs black Bc10 (light): a mate exists, not adjudicated.
        assert_eq!(
            end_reason("2b2k4/10/10/10/10/10/10/10/10/B4K4 w - - 0 1"),
            None
        );
    }

    #[test]
    fn compound_pieces_are_sufficient() {
        // The Marshal (R+N, `E`) and Cardinal (B+N, `A`) are major pieces: a lone
        // one beside the king is not an insufficient-material draw.
        assert_eq!(
            end_reason("5k4/10/10/10/10/10/10/10/10/5KE3 w - - 0 1"),
            None
        );
        assert_eq!(
            end_reason("5k4/10/10/10/10/10/10/10/10/5KA3 w - - 0 1"),
            None
        );
    }
}
