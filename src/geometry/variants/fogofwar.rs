//! Fog of War / Dark Chess (8x8) on the generic engine — **standard chess
//! movement with a non-royal king**. Validated against Fairy-Stockfish
//! `UCI_Variant fogofwar` (an `variants.ini` definition; see below).
//!
//! Fog of War keeps every standard chess move — the same piece moves, castling,
//! en passant, and promotions — but removes the concept of **check**. The king
//! is an ordinary, capturable piece:
//!
//! * a side **may move into "check"** and **may leave its king attacked**;
//! * **capturing the enemy king is a legal move** — it is how the game is won;
//! * the position is **terminal once a king is captured** (the side to move then
//!   has no king and therefore no legal move);
//! * castling is **never** restricted by attacked squares (there is no check to
//!   castle out of, through, or into).
//!
//! Because none of that depends on hidden information, the **move generator is
//! deterministic** and its node counts match Fairy-Stockfish's `go perft`
//! exactly — that is what [`tests/perft_fogofwar.rs`] pins.
//!
//! ## The fog (a view layer, *not* part of perft)
//!
//! The "fog" itself is the per-player **visibility**: each side sees only the
//! squares its own pieces occupy or pseudo-attack. That is purely a *view* over
//! the full position — it does not change the legal moves or the perft counts —
//! so it lives in [`FogOfWar::visible_squares`] and is checked by the unit tests
//! at the bottom of this module, **not** by the perft gate.
//!
//! ## King handling — a Duck without the Duck
//!
//! The non-royal king reuses the exact machinery Duck chess introduced: an empty
//! [`royal_squares`](WideVariant::royal_squares) set makes the generic
//! king-safety code report "never in check", and
//! [`non_royal_king`](WideVariant::non_royal_king) routes the standard generator
//! through its non-royal branch (every pseudo-legal board move is legal, the king
//! has no check mask / pin / king-danger filter). Fog of War is therefore Duck
//! chess minus the Duck: standard movement, non-royal king, no extra piece.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant fogofwar`:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! ```
//!
//! The opening array and every FEN field are plain chess — Fog of War shares the
//! standard dialect byte-for-byte (the hidden information is a *rendering*
//! concern, never encoded in the position).
//!
//! [`royal_squares`]: WideVariant::royal_squares
//! [`tests/perft_fogofwar.rs`]: https://github.com/ywzvennu/mcr/blob/main/tests/perft_fogofwar.rs

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{Bitboard, Board, Chess8x8, WideRole, WideVariant};
use crate::Color;

/// The squares `color` sees through the fog, computed from `board` alone: every
/// square its own pieces occupy, plus every square any of its pieces
/// pseudo-attacks (its raw attack pattern under the current occupancy).
///
/// Shared by [`FogOfWar::visible_squares`] and the per-player
/// [`redact_board_for`](WideVariant::redact_board_for) hook so the rendered fog
/// and the redacted view can never disagree. This is a pure read over the board
/// — it hides nothing itself and has no effect on move generation or perft.
fn fog_visible_squares<const R: usize>(
    board: &Board<Chess8x8, R>,
    color: Color,
) -> Bitboard<Chess8x8> {
    let occupied = board.occupied();
    let mut visible = board.by_color(color);
    for &role in &WideRole::ALL[..<FogOfWarRules as WideVariant<Chess8x8>>::ROLE_SPAN] {
        for from in board.pieces(color, role) {
            visible |= FogOfWarRules::role_attacks(role, color, from, occupied);
        }
    }
    visible
}

/// The Fog of War rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides only what Fog of War changes about standard chess: the king is
/// non-royal (via [`WideVariant::royal_squares`] and
/// [`WideVariant::non_royal_king`]). Every piece's movement, castling, en
/// passant, and promotion rule is the standard trait default; the no-check
/// legality lives in the generic engine, switched on by the two hooks below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FogOfWarRules;

/// The standard 8x8 starting placement (Fog of War shares the chess array).
const FOGOFWAR_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

impl WideVariant<Chess8x8> for FogOfWarRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(FOGOFWAR_START_PLACEMENT)
            .expect("the Fog of War starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::standard::<Chess8x8>(),
            ep_square: None,
            ep_captured: None,
            gating: GenericGating::NONE,
            duck: None,
            placement: GenericPlacement::NONE,
            halfmove_clock: 0,
            fullmove_number: 1,
            consecutive_passes: 0,
            board_b: Bitboard::EMPTY,
            petrified: Bitboard::EMPTY,
            checks_against: [0, 0],
            jieqi_seed: None,
        };
        (board, state)
    }

    fn non_royal_king() -> bool {
        // The king is not royal: the standard generator's non-royal branch emits
        // every pseudo-legal board move (no check mask, no pins, no king-danger
        // filter), so a king may step onto an attacked square, a piece may move
        // while "pinned", and capturing the enemy king is a legal move.
        true
    }

    fn royal_squares<const R: usize>(
        _board: &Board<Chess8x8, R>,
        _color: Color,
    ) -> Bitboard<Chess8x8> {
        // The king is **not royal**: there is no check, pin, or self-check filter.
        // An empty royal set makes the generic king-safety machinery report
        // "never in check"; a side loses by having its king captured (after which
        // it is to move with no king, hence no legal move — a terminal node).
        Bitboard::EMPTY
    }

    /// Redacts the board to what `perspective` sees through the fog: its own
    /// pieces stay, and every **enemy** piece on a square `perspective` cannot
    /// see (outside its `fog_visible_squares` set) is cleared — rendered as an empty square, so
    /// the serialized FEN never reveals a hidden enemy piece's location. Enemy
    /// pieces on visible squares (those a friendly piece attacks) remain, which
    /// is how an imminent capture is exposed.
    ///
    /// This never changes move generation, legality, or perft — it is a
    /// read-only view over the full position, the same `visible_squares` set the
    /// fog is drawn from.
    fn redact_board_for<const R: usize>(
        board: &Board<Chess8x8, R>,
        _state: &GenericState<Chess8x8, R>,
        perspective: Color,
    ) -> Option<Board<Chess8x8, R>> {
        let visible = fog_visible_squares(board, perspective);
        let mut redacted = *board;
        for square in board.by_color(perspective.opposite()) {
            if !visible.contains(square) {
                redacted.discard(square);
            }
        }
        Some(redacted)
    }
}

/// Fog of War (Dark Chess) as a [`GenericPosition`] over the 8x8 [`Chess8x8`]
/// geometry.
///
/// Construct the starting position with
/// [`FogOfWar::startpos`](GenericPosition::startpos) or parse a plain-chess FEN
/// with [`FogOfWar::from_fen`](GenericPosition::from_fen). The move generator is
/// deterministic and perft-validated against Fairy-Stockfish; the per-player
/// fog is the separate [`visible_squares`](FogOfWar::visible_squares) view.
pub type FogOfWar = GenericPosition<
    Chess8x8,
    FogOfWarRules,
    { <FogOfWarRules as WideVariant<Chess8x8>>::ROLE_SPAN },
>;

impl FogOfWar {
    /// Returns the squares `color` can **see** through the fog: every square its
    /// own pieces occupy, plus every square any of its pieces pseudo-attacks.
    ///
    /// This is a **view helper only** — the deterministic rendering of the "fog"
    /// — and has **no effect on move generation, legality, or perft**. The set
    /// is computed against the *full* position (it does not itself hide
    /// anything); a UI renders the complement as fog.
    ///
    /// "Pseudo-attack" means the raw attack pattern of each piece under the
    /// current occupancy, ignoring check and move legality (pawns use their
    /// diagonal capture pattern). A square an enemy piece occupies is visible
    /// when one of `color`'s pieces attacks it — that is how a capture is
    /// revealed. Note this is *attack*-based: a square a pawn could only *push*
    /// to (never capture) is not, by itself, made visible here.
    ///
    /// The result always contains `color`'s own pieces (each occupied square is
    /// in the set by definition), so a side never loses sight of its own army.
    #[must_use]
    pub fn visible_squares(&self, color: Color) -> Bitboard<Chess8x8> {
        fog_visible_squares(self.board(), color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A position with one **hidden** and one **visible** enemy piece for each
    // side: White has Ke1 and Pe4; Black has Ke8 and Pd5. The white pawn on e4
    // attacks d5, so Black's pawn there is *visible* to White (and, symmetrically,
    // Black's d5 pawn attacks e4, so White's pawn is visible to Black). Each
    // king is far from any enemy attack and is therefore *hidden* from the
    // opponent.
    const MIXED_VISIBILITY_FEN: &str = "4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1";

    #[test]
    fn view_for_white_hides_black_king_but_shows_visible_pawn() {
        let pos = FogOfWar::from_fen(MIXED_VISIBILITY_FEN).expect("valid fog position");
        let view = pos.view_for(Color::White);
        let placement = view
            .fen
            .split(' ')
            .next()
            .expect("fen has a placement field");

        // The visible Black pawn (attacked by White's e4 pawn) IS shown — a
        // lowercase `p` still appears in White's view.
        assert!(
            placement.contains('p'),
            "the visible Black pawn must appear in White's view: {}",
            view.fen
        );
        // The hidden Black king is NOT shown: no lowercase `k` leaks through.
        assert!(
            !placement.contains('k'),
            "the hidden Black king leaked into White's view: {}",
            view.fen
        );
        // White's own pieces (K, P) are always visible.
        assert!(placement.contains('K') && placement.contains('P'));
    }

    #[test]
    fn view_for_black_hides_white_king_but_shows_visible_pawn() {
        let pos = FogOfWar::from_fen(MIXED_VISIBILITY_FEN).expect("valid fog position");
        let view = pos.view_for(Color::Black);
        let placement = view
            .fen
            .split(' ')
            .next()
            .expect("fen has a placement field");

        // The visible White pawn (attacked by Black's d5 pawn) IS shown.
        assert!(
            placement.contains('P'),
            "the visible White pawn must appear in Black's view: {}",
            view.fen
        );
        // The hidden White king is NOT shown.
        assert!(
            !placement.contains('K'),
            "the hidden White king leaked into Black's view: {}",
            view.fen
        );
        assert!(placement.contains('k') && placement.contains('p'));
    }

    #[test]
    fn view_for_mover_shows_own_moves_non_mover_shows_none() {
        // White to move: White sees its own legal moves; Black (the non-mover)
        // sees no move list — a fog player never sees the opponent's moves.
        let white_to_move = FogOfWar::from_fen(MIXED_VISIBILITY_FEN).expect("valid");
        assert!(!white_to_move.view_for(Color::White).legal_ucis.is_empty());
        assert!(
            white_to_move.view_for(Color::Black).legal_ucis.is_empty(),
            "the non-mover must not see the opponent's move list"
        );

        // Black to move: symmetric.
        let black_to_move = FogOfWar::from_fen("4k3/8/8/3p4/4P3/8/8/4K3 b - - 0 1").expect("valid");
        assert!(!black_to_move.view_for(Color::Black).legal_ucis.is_empty());
        assert!(black_to_move.view_for(Color::White).legal_ucis.is_empty());
    }

    #[test]
    fn redacted_view_matches_the_visibility_set_square_for_square() {
        // The redacted view agrees with `visible_squares` exactly: an enemy piece
        // is shown iff its square is visible, and every own piece survives.
        let pos = FogOfWar::from_fen(MIXED_VISIBILITY_FEN).expect("valid");
        for perspective in [Color::White, Color::Black] {
            let visible = pos.visible_squares(perspective);
            let view = pos.view_for(perspective);
            let placement = view.fen.split(' ').next().expect("placement");
            let shown = Board::<Chess8x8>::from_fen_placement(placement)
                .expect("redacted placement parses");

            // Enemy pieces are present in the view iff their square is visible.
            for sq in pos.board().by_color(perspective.opposite()) {
                assert_eq!(
                    shown.is_occupied(sq),
                    visible.contains(sq),
                    "enemy square {sq:?} shown/visible mismatch for {perspective:?}"
                );
            }
            // Own pieces are always shown — a side never loses sight of its army.
            for sq in pos.board().by_color(perspective) {
                assert!(shown.is_occupied(sq), "own piece {sq:?} vanished");
            }
        }
    }

    #[test]
    fn spectator_view_hides_both_sides_kings() {
        // A spectator of an in-progress fog game sees only the mutually visible
        // pieces and no move list: both kings (hidden from the opponent) are gone.
        let pos = FogOfWar::from_fen(MIXED_VISIBILITY_FEN).expect("valid");
        let view = pos.spectator_view();
        let placement = view.fen.split(' ').next().expect("placement");
        assert_eq!(view.perspective, None);
        assert!(view.legal_ucis.is_empty(), "a spectator sees no move list");
        assert!(
            !placement.contains('k') && !placement.contains('K'),
            "neither king should be visible to a spectator: {}",
            view.fen
        );
    }
}
