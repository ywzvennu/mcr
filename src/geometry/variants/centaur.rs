//! Centaur Chess (10x8) on the generic engine — the Capablanca board and
//! castling, but with the Archbishop/Chancellor compounds replaced by two
//! **Centaurs** (`docs/fairy-variants-architecture.md`, Phase 2). Validated
//! against Fairy-Stockfish's INI `centaur` variant.
//!
//! Centaur Chess is played on the ten-files by eight-ranks [`Cap10x8`] board.
//! The one non-standard piece is the
//!
//! * **Centaur** ([`WideRole::Kheshig`]) — a **King + Knight** leaper (sixteen
//!   squares; it moves and captures alike), exactly Fairy-Stockfish's built-in
//!   `centaur` piece type (Betza `KN`). FSF spells it `c`/`C`; mcr reuses the
//!   Orda [`WideRole::Kheshig`] letter `w`/`W` (the `compare-fairy/` harness
//!   reconciles the dialect).
//!
//! Every other rule is standard chess: pawns push one (or two from their second
//! rank), capture diagonally, take en passant, and promote on the last rank —
//! here to Queen, Rook, Bishop, Knight, or **Centaur**. The king and rooks
//! castle on the Capablanca files ([`WideVariant::castle_dest_files`], reused
//! verbatim from Capablanca).
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's INI `centaur` variant (a `capablanca`
//! descendant with the compounds removed and `centaur = c`):
//!
//! ```text
//! FSF dialect: rcnbqkbncr/pppppppppp/10/10/10/10/PPPPPPPPPP/RCNBQKBNCR w KQkq - 0 1
//! mcr dialect: rwnbqkbnwr/pppppppppp/10/10/10/10/PPPPPPPPPP/RWNBQKBNWR w KQkq - 0 1
//! ```
//!
//! The two strings differ only in the centaur's letter (`c` in FSF, `w` in mcr).
//! Back rank, a-file to j-file: Rook, Centaur, Knight, Bishop, Queen, King,
//! Bishop, Knight, Centaur, Rook. The king stands on the f-file (file 5); the
//! castling rooks are the a-file (file 0) and j-file (file 9) rooks.
//!
//! ## Castling geometry
//!
//! Identical to Capablanca: on castling the king lands two files from its start
//! and the rook ends beside it.
//!
//! * **Kingside**: king f1 -> **i1** (file 8), rook j1 -> **h1** (file 7).
//! * **Queenside**: king f1 -> **c1** (file 2), rook a1 -> **d1** (file 3).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Cap10x8, PromotionConfig, Square, StandardChess, WideRole,
    WideVariant,
};
use crate::Color;

/// The Centaur Chess rule layer: a zero-sized [`WideVariant`] over [`Cap10x8`].
///
/// It overrides only what Centaur Chess changes from the standard generic
/// engine: the 10x8 starting array, the Centaur ([`WideRole::Kheshig`])
/// movement, the promotion set (adding the Centaur), and the castle destination
/// files (the Capablanca files). Pawns, knights, sliders, and the king are
/// standard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CentaurRules;

/// The confirmed Centaur Chess starting FEN placement in the mcr dialect
/// (centaur = `w`/`W`, the Orda Kheshig letter), byte-for-byte equivalent to
/// Fairy-Stockfish's `rcnbqkbncr/.../RCNBQKBNCR` modulo the centaur's letter.
const CENTAUR_START_PLACEMENT: &str = "rwnbqkbnwr/pppppppppp/10/10/10/10/PPPPPPPPPP/RWNBQKBNWR";

/// The kingside castle side index, matching the position layer's `KINGSIDE`.
const KINGSIDE: usize = 0;

impl WideVariant<Cap10x8> for CentaurRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 32;

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

    fn starting_position() -> (Board<Cap10x8>, GenericState<Cap10x8>) {
        let board = Board::<Cap10x8>::from_fen_placement(CENTAUR_START_PLACEMENT)
            .expect("the Centaur Chess starting placement is valid on a 10x8 board");
        let state = GenericState {
            turn: Color::White,
            // Both colors castle with both rooks (a/j files); the generic
            // castling layer reads the rook files from the back rank, so the
            // standard `KQkq` field maps to the a-file (queenside) and j-file
            // (kingside) rooks on this 10-wide board.
            castling: GenericCastling::standard::<Cap10x8>(),
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
            checks_against: [0, 0],
            jieqi_seed: None,
        };
        (board, state)
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Cap10x8>,
        occupancy: Bitboard<Cap10x8>,
    ) -> Bitboard<Cap10x8> {
        match role {
            // Centaur: King + Knight leaper — moves and captures alike.
            WideRole::Kheshig => {
                attacks::king_attacks::<Cap10x8>(sq) | attacks::knight_attacks::<Cap10x8>(sq)
            }
            // Every other piece is standard chess.
            _ => <StandardChess as WideVariant<Cap10x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn role_is_slider(role: WideRole) -> bool {
        match role {
            // The Centaur is a pure leaper, never pinned along a line.
            WideRole::Kheshig => false,
            _ => <StandardChess as WideVariant<Cap10x8>>::role_is_slider(role),
        }
    }

    fn promotion_config() -> PromotionConfig {
        // A Centaur pawn promotes to any of the five non-pawn, non-king roles of
        // the army: the four standard plus the Centaur. The order matches
        // Fairy-Stockfish's promotion set (knight, bishop, rook, queen, centaur);
        // the order only affects move enumeration order, not the perft leaf count.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Knight,
                WideRole::Bishop,
                WideRole::Rook,
                WideRole::Queen,
                WideRole::Kheshig, // Centaur (K+N)
            ],
        }
    }

    fn castle_dest_files(side: usize) -> (u8, u8) {
        // Capablanca castling, matching Fairy-Stockfish's
        // castlingKingsideFile = FILE_I (8) / castlingQueensideFile = FILE_C (2),
        // with the rook ending beside the king toward the centre.
        if side == KINGSIDE {
            // King f1 -> i1 (file 8); rook j1 -> h1 (file 7).
            (8, 7)
        } else {
            // King f1 -> c1 (file 2); rook a1 -> d1 (file 3).
            (2, 3)
        }
    }

    /// Centaur Chess keeps the standard chess army plus the always-mating Centaur
    /// ([`WideRole::Kheshig`], a King + Knight), so the ordinary
    /// insufficient-material draw applies on the wider board: king vs king, king
    /// and a lone minor (bishop or knight) vs king, and same-colour bishops only.
    /// The Centaur counts as mating material (matching Fairy-Stockfish, which
    /// classes it as a major piece). Adjudication-only and behind the default-off
    /// hook, so perft stays byte-identical.
    fn is_insufficient_material<const R: usize>(
        board: &Board<Cap10x8, R>,
        _state: &GenericState<Cap10x8, R>,
    ) -> bool {
        crate::geometry::variant::standard_insufficient_material(board)
    }
}

/// Centaur Chess as a [`GenericPosition`] over the 10x8 [`Cap10x8`] geometry.
///
/// Construct the starting position with
/// [`Centaur::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Centaur::from_fen`](GenericPosition::from_fen). The Centaur reuses the Orda
/// [`WideRole::Kheshig`] King + Knight leaper, so only the array, the Centaur
/// movement, the promotion set, and the castle files distinguish it.
pub type Centaur =
    GenericPosition<Cap10x8, CentaurRules, { <CentaurRules as WideVariant<Cap10x8>>::ROLE_SPAN }>;

#[cfg(test)]
mod insufficient_material_tests {
    use super::Centaur;
    use crate::geometry::{WideEndReason, WideOutcome};

    fn end_reason(fen: &str) -> Option<WideEndReason> {
        Centaur::from_fen(fen)
            .expect("valid centaur fen")
            .end_reason()
    }

    #[test]
    fn lone_kings_draw() {
        let pos = Centaur::from_fen("5k4/10/10/10/10/10/10/5K4 w - - 0 1").expect("valid fen");
        assert_eq!(pos.end_reason(), Some(WideEndReason::InsufficientMaterial));
        assert_eq!(pos.outcome(), Some(WideOutcome::Draw));
    }

    #[test]
    fn king_and_single_minor_draw() {
        assert_eq!(
            end_reason("5k4/10/10/10/10/10/10/5KN3 w - - 0 1"),
            Some(WideEndReason::InsufficientMaterial)
        );
        assert_eq!(
            end_reason("5k4/10/10/10/10/10/10/5KB3 w - - 0 1"),
            Some(WideEndReason::InsufficientMaterial)
        );
    }

    #[test]
    fn lone_centaur_is_sufficient() {
        // The Centaur (K+N, `w`) is a major piece: a lone one beside the king is
        // not an insufficient-material draw.
        assert_eq!(end_reason("5k4/10/10/10/10/10/10/5KW3 w - - 0 1"), None);
    }
}
