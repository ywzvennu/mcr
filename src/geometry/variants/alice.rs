//! Alice chess on the generic engine — two mirror 8x8 boards with a
//! through-the-looking-glass transfer (issue #276).
//!
//! Alice chess (V. R. Parton, 1953) is played over **two** standard 8x8 boards,
//! A and B. Every piece occupies a square on exactly one board, and **at most one
//! piece sits on any square across both boards**. A piece **moves** by ordinary
//! chess rules on the board it currently occupies and then **transfers** to the
//! same square on the *other* board ("through the looking-glass"). The two
//! requirements for a legal move are:
//!
//! 1. the move is legal on the board it is played on (path clear on that board; a
//!    capture removes an enemy piece on that board), and
//! 2. the square it transfers to on the **other** board is vacant.
//!
//! A piece attacks, gives check, blocks, and is captured **only by same-board
//! pieces**. King-safety is therefore plane-restricted: a king is in check only
//! from enemy pieces sharing its board.
//!
//! ## Modelling
//!
//! The single [`Board<Chess8x8>`] holds **every** piece (the one-piece-per-square
//! invariant makes that unambiguous), and the per-piece board membership rides in
//! the [`GenericState::board_b`](crate::geometry::position::GenericState::board_b)
//! mask: a square in the mask holds a plane-**B** piece, a square not in it (but
//! occupied) holds a plane-**A** piece. At the start every piece is on plane A, so
//! the mask is empty and the standard starting array and FEN are reused verbatim.
//! All of this is gated behind the default-off
//! [`WideVariant::is_alice`] hook, so every other variant is byte-identical.
//!
//! ## King-safety ("moving into check"), per the Wikipedia ruleset
//!
//! Two conditions, both implemented:
//!
//! * **After the move and transfer**, the king must not be in check on the board
//!   it ends up on — this rejects a *discovered* check on the board the king
//!   stayed on (a piece leaving that board exposes its king) and a king that
//!   *transfers into* check on the board it lands on. The interpose case (a piece
//!   transferring onto the other board to block a check there) is naturally
//!   allowed, because the post-move position is what is tested.
//! * **Before the transfer**, the king must not be in check on the board the move
//!   was played on — "the king cannot transfer out of check." This adds, for an
//!   ordinary king move, the requirement that the destination square also be
//!   unattacked on the board the king is *leaving*; a castle's transit safety on
//!   that board is enforced during generation.
//!
//! ## Castling and en passant (documented interpretations)
//!
//! * **Castling** is permitted (the common Alice ruling): it is an ordinary king
//!   move on the king's board — king and rook must share that board, the traversed
//!   squares must be clear and unattacked **on that board**, and the king and
//!   rook destinations must be vacant on the **other** board; both then transfer
//!   to it.
//! * **En passant** is **excluded**. Wikipedia notes it "is normally excluded, but
//!   it can be included … opinions differ"; the engine takes the normal exclusion
//!   (no ep capture is generated and no ep target is kept). See the issue/PR notes
//!   for this flagged ambiguity.
//!
//! ## Validation
//!
//! **Rules-validated (no FSF oracle); perft pins hand-derived per the documented
//! Alice ruleset.** Fairy-Stockfish has no `alice` variant, so there is no perft
//! oracle. Correctness is instead pinned by hand-derived shallow perft (depths 1
//! and 2 from the start position are provably 20 and 400, matching standard chess
//! because plane B is still empty/sparse and no transfer conflict or check
//! arises), an independent brute-force Alice generator cross-checked against the
//! engine, invariant/property tests over seeded random playouts, and
//! hand-constructed unit tests for the cross-board mechanics. See
//! `tests/perft_alice.rs` and `tests/alice_rules.rs`.

use crate::geometry::position::GenericState;
use crate::geometry::{Board, Chess8x8, GenericPosition, StandardChess, WideVariant};

/// The Alice chess rule layer: standard chess movement over two mirror boards
/// with a per-move transfer, a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// Every movement/promotion/castling hook is standard chess's; the sole override
/// is [`is_alice`](WideVariant::is_alice), which routes the engine through the
/// dedicated two-plane generation, king-safety, and transfer paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct AliceRules;

impl WideVariant<Chess8x8> for AliceRules {
    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        // The standard chess start: all pieces on board A (the empty `board_b`
        // mask), board B empty. Reuse the reference array verbatim.
        <StandardChess as WideVariant<Chess8x8>>::starting_position()
    }

    /// The one Alice-specific switch: enable the two-board transfer engine.
    fn is_alice() -> bool {
        true
    }

    /// Alice keeps the **standard chess army**, so a position with no mating
    /// material is the ordinary insufficient-material draw (king vs king, king and
    /// a lone minor vs king, or same-colour bishops only). The looking-glass
    /// transfer never *adds* mating power, so the standard rule is a sound (and
    /// conservative) draw test here; Fairy-Stockfish has no Alice variant, so it is
    /// matched to standard chess rather than an FSF oracle. Adjudication-only and
    /// behind the default-off hook, so perft is byte-identical.
    fn is_insufficient_material(board: &Board<Chess8x8>, _state: &GenericState<Chess8x8>) -> bool {
        crate::geometry::variant::standard_insufficient_material(board)
    }
}

/// Alice chess as a [`GenericPosition`] over the 8x8 geometry.
///
/// Construct the starting position with
/// [`Alice::startpos`](crate::geometry::GenericPosition::startpos) or parse the
/// standard FEN with [`Alice::from_fen`](crate::geometry::GenericPosition::from_fen)
/// (which yields all pieces on board A). A position's per-piece board membership
/// is then maintained internally as each move transfers its mover to the other
/// board.
pub type Alice = GenericPosition<Chess8x8, AliceRules>;

#[cfg(test)]
mod insufficient_material_tests {
    use super::Alice;
    use crate::geometry::{WideEndReason, WideOutcome};

    fn end_reason(fen: &str) -> Option<WideEndReason> {
        Alice::from_fen(fen).expect("valid alice fen").end_reason()
    }

    #[test]
    fn lone_kings_draw() {
        let pos = Alice::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").expect("valid fen");
        assert_eq!(pos.end_reason(), Some(WideEndReason::InsufficientMaterial));
        assert_eq!(pos.outcome(), Some(WideOutcome::Draw));
    }

    #[test]
    fn king_and_single_minor_draw() {
        // K + N vs K and K + B vs K are both dead draws.
        assert_eq!(
            end_reason("4k3/8/8/8/8/8/8/4KN2 w - - 0 1"),
            Some(WideEndReason::InsufficientMaterial)
        );
        assert_eq!(
            end_reason("4k3/8/8/8/8/8/8/4KB2 w - - 0 1"),
            Some(WideEndReason::InsufficientMaterial)
        );
    }

    #[test]
    fn same_colour_bishops_draw() {
        // White Ba1 (dark) and black Bh8 (dark): same complex, no mate possible.
        assert_eq!(
            end_reason("4k2b/8/8/8/8/8/8/B3K3 w - - 0 1"),
            Some(WideEndReason::InsufficientMaterial)
        );
    }

    #[test]
    fn opposite_colour_bishops_are_sufficient() {
        // White Ba1 (dark) vs black Bg8 (light): opposite complexes can mate, so
        // the game is not adjudicated drawn.
        assert_eq!(end_reason("4k1b1/8/8/8/8/8/8/B3K3 w - - 0 1"), None);
    }

    #[test]
    fn rook_and_two_knights_are_sufficient() {
        // A lone rook can force mate; K+N+N is not an automatic draw.
        assert_eq!(end_reason("4k3/8/8/8/8/8/8/R3K3 w - - 0 1"), None);
        assert_eq!(end_reason("4k3/8/8/8/8/8/8/2N1KN2 w - - 0 1"), None);
    }
}
