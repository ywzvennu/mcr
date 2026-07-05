//! Knightmate (8x8) on the generic engine — the **Knight is royal** and the king
//! is replaced by a non-royal **Commoner** (a Mann). Validated node-for-node
//! against Fairy-Stockfish (`UCI_Variant knightmate`, an FSF built-in).
//!
//! Knightmate is otherwise ordinary chess, with one twist: the piece you must not
//! lose — the piece whose attack defines check and whose capture ends the game —
//! is a **royal Knight**, while the two knights of the opening array are replaced
//! by **Commoners** (non-royal king-steppers, a Mann each). The royal Knight
//! stands on the king's square, keeps the king's FEN letter and the king's
//! castling rights, but **moves and gives check as a Knight**.
//!
//! ## Pieces
//!
//! * **Royal Knight** ([`WideRole::King`]) — sits where the king sits (`e1` / `e8`),
//!   spelled with the king's FEN letter `k`/`K`, and is the side's **royal piece**:
//!   the engine's existing king-safety machinery (check, pins, king-danger,
//!   checkmate, the single-royal legality fast path) tracks it unchanged. Its
//!   *movement*, however, is a Knight's — so this variant overrides only
//!   [`role_attacks`](WideVariant::role_attacks) to give [`WideRole::King`] the
//!   knight leap. Because a knight's attack pattern is symmetric, the generic
//!   reverse-projecting [`attackers_to`](crate::geometry::position) and the
//!   forward king-danger map are both correct with no further hooks, and two
//!   opposing royal Knights are never mutually adjacent-attacking (knights do not
//!   attack adjacent squares) — exactly matching FSF.
//! * **Commoner** ([`WideRole::Commoner`]) — the non-royal king-stepper (one step
//!   in any of the eight directions), already used by Synochess and Shinobi. It
//!   replaces the opening array's two knights (files `b` and `g`), may be captured
//!   freely, and never defines check. mcr spells it with the `*`-prefixed overflow
//!   token `*u` (the [`OVERFLOW_PREFIX`](crate::geometry::OVERFLOW_PREFIX) plus the
//!   recycled Advisor letter `u`, the case carrying the colour); FSF spells it `m`.
//! * **Pawn, Bishop, Rook, Queen** — ordinary chess pieces. Pawns push, capture,
//!   double-step, and en-passant as usual; **promotion** targets are the Commoner,
//!   Bishop, Rook, and Queen — **never** a Knight (the bare Knight role does not
//!   exist in Knightmate) and never the royal King.
//!
//! ## Castling
//!
//! Standard king-and-rook castling, both colours (`KQkq`). The royal Knight
//! castles to `g`/`c` exactly as a king would; castling legality (path empty, not
//! in check, the king-walk squares not attacked) is independent of the royal
//! piece's movement pattern, so the generic castle generator handles it unchanged.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant knightmate`, `position startpos`) renders the start as
//!
//! ```text
//! rmbqkbmr/pppppppp/8/8/8/8/PPPPPPPP/RMBQKBMR w KQkq - 0 1
//! ```
//!
//! with FSF's Commoner letter `m`/`M` and the royal Knight on the king square
//! (`k`/`K`). mcr uses the same board but spells the Commoner with its overflow
//! token `*u` / `*U`:
//!
//! ```text
//! r*ubqkb*ur/pppppppp/8/8/8/8/PPPPPPPP/R*UBQKB*UR w KQkq - 0 1
//! ```
//!
//! The two are the same position; the `compare-fairy/` harness rewrites `*u → m`
//! when driving FSF. No new role or letter is introduced — the royal Knight reuses
//! [`WideRole::King`] and the Commoner reuses [`WideRole::Commoner`].

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Chess8x8, Geometry, PromotionConfig, Square, StandardChess, WideRole,
    WideVariant,
};
use crate::Color;

/// The confirmed Knightmate starting placement, in mcr's role letters: the
/// standard array with the two knights replaced by Commoners (`*u` / `*U`) on the
/// `b` and `g` files, and the royal Knight reusing the king's slot (`k` / `K`).
const KNIGHTMATE_START_PLACEMENT: &str = "r*ubqkb*ur/pppppppp/8/8/8/8/PPPPPPPP/R*UBQKB*UR";

/// The Knightmate rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It changes exactly three things from standard chess: the royal piece
/// ([`WideRole::King`]) **moves as a Knight**, the opening array's knights become
/// non-royal **Commoners**, and pawn promotion offers the Commoner in place of the
/// Knight. Everything else — pawns, en passant, castling, pins, check and
/// checkmate — is the generic engine's standard single-royal behaviour, so the
/// variant adds no new core mechanism and rides the existing fast path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct KnightmateRules;

impl WideVariant<Chess8x8> for KnightmateRules {
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

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(KNIGHTMATE_START_PLACEMENT)
            .expect("the Knightmate starting placement is valid on an 8x8 board");
        // Both sides castle, with rooks on files 0 and WIDTH-1 (standard layout).
        let mut castling = GenericCastling::NONE;
        for color in [Color::White, Color::Black] {
            castling.set(color, 0, Some(Chess8x8::WIDTH - 1));
            castling.set(color, 1, Some(0));
        }
        let placement = GenericPlacement::new([0u8; WideRole::COUNT], [0u8; WideRole::COUNT]);
        let state = GenericState {
            turn: Color::White,
            castling,
            ep_square: None,
            gating: GenericGating::NONE,
            duck: None,
            placement,
            halfmove_clock: 0,
            fullmove_number: 1,
            consecutive_passes: 0,
            board_b: crate::geometry::Bitboard::EMPTY,
        };
        (board, state)
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Chess8x8>,
        occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        match role {
            // The royal Knight: the king's slot, but the knight's leap. This is the
            // whole variant — the generic king-safety machinery treats
            // `WideRole::King` as royal, so detecting check, king-danger, pins, and
            // mate on the royal Knight all follow automatically from giving it the
            // knight attack set. (Both the forward king-danger projection and the
            // reverse-projecting `attackers_to` are correct because the knight
            // pattern is symmetric.)
            WideRole::King => attacks::knight_attacks::<Chess8x8>(sq),
            // The Commoner replaces the opening knights: a king's eight one-steps,
            // but non-royal and freely capturable.
            WideRole::Commoner => attacks::king_attacks::<Chess8x8>(sq),
            // Pawns, Bishops, Rooks, Queens are standard chess.
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn role_is_slider(role: WideRole) -> bool {
        // The royal Knight and the Commoner are pure leapers/steppers (never pinned
        // along a ray as the moving piece); the Bishop / Rook / Queen keep their
        // standard slider classification.
        match role {
            WideRole::King | WideRole::Commoner => false,
            _ => <StandardChess as WideVariant<Chess8x8>>::role_is_slider(role),
        }
    }

    fn promotion_config() -> PromotionConfig {
        // A promoting pawn becomes a Commoner, Bishop, Rook, or Queen — never the
        // (now-nonexistent) bare Knight and never the royal King. This matches
        // FSF's `b7b8m / b7b8q / b7b8r / b7b8b`.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Commoner,
                WideRole::Bishop,
                WideRole::Rook,
                WideRole::Queen,
            ],
        }
    }

    fn has_castling() -> bool {
        // Standard king-and-rook castling for both colours. The royal Knight
        // castles to g/c exactly as a king does; the generic castle generator is
        // independent of the royal piece's own movement.
        true
    }

    /// Knightmate's army classifies **exactly** as the standard
    /// `standard_insufficient_material` helper: the royal piece is
    /// [`WideRole::King`] (Fairy-Stockfish's `KING` type, *restricted* in the
    /// material count regardless of its knight movement), the Bishop is the only
    /// colour-bound minor, and the Commoner — like FSF's `COMMONER` — is mating
    /// material, so a position holding any Commoner, Rook, Queen, or Pawn is
    /// sufficient. There is **no** bare Knight role (the opening knights are
    /// Commoners), so the helper's knight arm never fires. The variant therefore
    /// adjudicates the ordinary draws — king vs king, king and a lone bishop vs
    /// king, and same-colour bishops only.
    ///
    /// This is **byte-confirmed against** Fairy-Stockfish `UCI_Variant knightmate`'s
    /// `has_insufficient_material`: `KvK` and `K+B vs K` are drawn, while
    /// `K+Commoner vs K`, `K+R vs K`, and opposite-colour bishops are not — exactly
    /// what the helper returns. (Hoppel-Poppel, by contrast, stays default-off: its
    /// Knight-Bishop / Bishop-Knight are *unbound* minors FSF draws against a bare
    /// king but the standard-army helper does not classify — see its module docs.)
    /// Adjudication-only and behind the default-off hook, so perft is byte-identical.
    fn is_insufficient_material(board: &Board<Chess8x8>, _state: &GenericState<Chess8x8>) -> bool {
        crate::geometry::variant::standard_insufficient_material(board)
    }
}

/// Knightmate as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position (the standard array with the knights replaced
/// by Commoners and a royal Knight on each king square) with
/// [`Knightmate::startpos`](GenericPosition::startpos) or parse a FEN (mcr dialect,
/// Commoner `*u`) with [`Knightmate::from_fen`](GenericPosition::from_fen). See the
/// [module docs](self) for the royal Knight, the Commoners, and the promotion set.
pub type Knightmate = GenericPosition<Chess8x8, KnightmateRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::perft as gperft;
    use crate::geometry::position::WideOutcome;

    /// The canonical start FEN round-trips in mcr's dialect.
    #[test]
    fn startpos_round_trips() {
        let pos = Knightmate::startpos();
        assert_eq!(
            pos.to_fen(),
            "r*ubqkb*ur/pppppppp/8/8/8/8/PPPPPPPP/R*UBQKB*UR w KQkq - 0 1"
        );
    }

    /// The royal Knight on its home square has exactly the two opening knight
    /// leaps available at the start (the d3/f3 jumps), confirming `WideRole::King`
    /// moves as a Knight.
    #[test]
    fn royal_knight_moves_as_a_knight() {
        let pos = Knightmate::startpos();
        let king = pos.board().king_of(Color::White).expect("royal knight");
        let knight_moves: usize = pos
            .legal_moves()
            .iter()
            .filter(|m| m.from::<Chess8x8>() == king)
            .count();
        // From e1, the two forward knight jumps d3 and f3 (c2/g2 are blocked by own
        // pawns); the king's own square has 18 total opening moves (16 pawn, 2
        // knight), per FSF's perft(1) = 18.
        assert_eq!(knight_moves, 2);
        assert_eq!(pos.legal_moves().len(), 18);
    }

    /// The Commoner replaces the opening knights and steps like a king: a lone
    /// Commoner on e4 has eight one-step moves.
    #[test]
    fn commoner_steps_like_a_king() {
        let pos = Knightmate::from_fen("4k3/8/8/8/4*U3/8/8/4K3 w - - 0 1").expect("valid FEN");
        let from = Square::<Chess8x8>::from_file_rank(4, 3).expect("e4");
        let steps = pos
            .legal_moves()
            .iter()
            .filter(|m| m.from::<Chess8x8>() == from)
            .count();
        assert_eq!(steps, 8);
    }

    /// Check on the royal Knight: a rook on e2 checks the royal Knight on e1, which
    /// must leap off the e-file and rank 2. Its jumps c2/g2 land on the rook's rank
    /// (still attacked), so only d3/f3 escape — FSF perft(1) = 2 here.
    #[test]
    fn royal_knight_in_check_must_escape() {
        let pos = Knightmate::from_fen("4k3/8/8/8/8/8/4r3/4K3 w - - 0 1").expect("valid FEN");
        assert!(
            pos.is_check(),
            "the rook on e2 checks the royal Knight on e1"
        );
        // The knight jumps d3 and f3 leave both the e-file and rank 2; c2/g2 stay on
        // the rook's rank and are still attacked.
        assert_eq!(pos.legal_moves().len(), 2);
    }

    /// Checkmate on the royal Knight ends the game with the checking side winning.
    #[test]
    fn royal_knight_can_be_mated() {
        // White royal Knight cornered on a1, checked down the a-file by Ra8; its only
        // jumps are b3 and c2, covered by Rb8 (b-file) and Rc8 (c-file). The royal
        // cannot block (it leaps), so it is mated — FSF perft(1) = 0 here.
        let pos = Knightmate::from_fen("rrr1k3/8/8/8/8/8/8/K7 w - - 0 1").expect("valid FEN");
        assert!(pos.is_check());
        assert!(pos.legal_moves().is_empty(), "the royal Knight is mated");
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::Black
            })
        );
    }

    /// Pawn promotion offers Commoner / Bishop / Rook / Queen — four targets, no
    /// Knight (FSF: `b7b8m b7b8q b7b8r b7b8b`).
    #[test]
    fn promotion_targets_exclude_knight() {
        let pos = Knightmate::from_fen("4k3/1P6/8/8/8/8/8/4K3 w - - 0 1").expect("valid FEN");
        let promos: usize = pos
            .legal_moves()
            .iter()
            .filter(|m| m.from::<Chess8x8>().file() == 1 && m.from::<Chess8x8>().rank() == 6)
            .count();
        assert_eq!(promos, 4);
    }

    /// The royal Knight castles to g/c like a king (FSF: `e1g1`, `e1c1`).
    #[test]
    fn royal_knight_castles() {
        let pos = Knightmate::from_fen("4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1").expect("valid FEN");
        let castles: usize = pos
            .legal_moves()
            .iter()
            .filter(|m| {
                matches!(
                    m.kind(),
                    crate::geometry::WideMoveKind::CastleKingside
                        | crate::geometry::WideMoveKind::CastleQueenside
                )
            })
            .count();
        assert_eq!(castles, 2);
    }

    /// Shallow perft from the start position matches the FSF-confirmed counts.
    #[test]
    fn startpos_perft_matches_fsf() {
        let pos = Knightmate::startpos();
        assert_eq!(gperft::<Chess8x8, _>(&pos, 1), 18);
        assert_eq!(gperft::<Chess8x8, _>(&pos, 2), 324);
        assert_eq!(gperft::<Chess8x8, _>(&pos, 3), 6765);
        assert_eq!(gperft::<Chess8x8, _>(&pos, 4), 139774);
    }
}

#[cfg(test)]
mod insufficient_material_tests {
    use super::Knightmate;
    use crate::geometry::{WideEndReason, WideOutcome};

    fn end_reason(fen: &str) -> Option<WideEndReason> {
        Knightmate::from_fen(fen)
            .expect("valid knightmate fen")
            .end_reason()
    }

    // Every assertion below is byte-confirmed against Fairy-Stockfish
    // `UCI_Variant knightmate`'s `has_insufficient_material`.

    #[test]
    fn lone_royal_knights_draw() {
        // Two bare royal Knights (the king slot) cannot mate — FSF: insufficient.
        let pos = Knightmate::from_fen("5k2/8/8/8/8/8/8/5K2 w - - 0 1").expect("valid fen");
        assert_eq!(pos.end_reason(), Some(WideEndReason::InsufficientMaterial));
        assert_eq!(pos.outcome(), Some(WideOutcome::Draw));
    }

    #[test]
    fn king_and_single_bishop_draw() {
        // Royal Knight + lone Bishop vs royal Knight is a dead draw (FSF: draw).
        assert_eq!(
            end_reason("5k2/8/8/8/8/8/8/5KB1 w - - 0 1"),
            Some(WideEndReason::InsufficientMaterial)
        );
    }

    #[test]
    fn same_colour_bishops_draw() {
        // White Ba1 (dark) and black Bh8 (dark) share one complex (FSF: draw).
        assert_eq!(
            end_reason("4k2b/8/8/8/8/8/8/B4K2 w - - 0 1"),
            Some(WideEndReason::InsufficientMaterial)
        );
    }

    #[test]
    fn opposite_colour_bishops_are_sufficient() {
        // White Ba1 (dark) vs black Bg8 (light): opposite complexes can mate.
        assert_eq!(end_reason("4k1b1/8/8/8/8/8/8/B4K2 w - - 0 1"), None);
    }

    #[test]
    fn commoner_is_sufficient() {
        // The Commoner (`*U`/`*u`) is mating material (FSF classes it major), so a
        // royal Knight + Commoner vs royal Knight is NOT an insufficient draw.
        assert_eq!(end_reason("5k2/8/8/8/8/8/8/5K*U1 w - - 0 1"), None);
    }

    #[test]
    fn rook_is_sufficient() {
        // A lone Rook can force mate (FSF: sufficient).
        assert_eq!(end_reason("5k2/8/8/8/8/8/8/5KR1 w - - 0 1"), None);
    }
}
