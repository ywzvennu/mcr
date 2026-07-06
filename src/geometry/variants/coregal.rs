//! Coregal chess (8x8) on the generic engine — **standard chess in which the
//! queen is royal too**. A side loses if *either* its king **or** its queen is
//! checkmated (or captured): both must be kept safe. Everything else — the
//! standard army, the 8x8 geometry, the pawn double-step / en passant /
//! promotion, and castling — is ordinary chess.
//!
//! Coregal chess reuses the [`StandardChess`] ruleset over [`Chess8x8`] and turns
//! on exactly one extra constraint: the queen is a second **(pseudo-)royal**
//! piece. It rides the same **multi-royal** king-safety machinery Spartan uses for
//! its two kings ([`WideVariant::multi_royal`]), but with the *strict*
//! pseudo-royal rule Chak uses ([`WideVariant::royals_all_must_survive`]): a legal
//! move must leave **every** royal — the king *and* the queen — unattacked, and a
//! side is in check whenever *any* royal is attacked.
//!
//! ## Rules — standard chess with a royal queen
//!
//! * **King and queen are both royal.** [`royal_squares`](WideVariant::royal_squares)
//!   reports the king **and** the queen of `color`, and
//!   [`royals_all_must_survive`](WideVariant::royals_all_must_survive) is `true`,
//!   so a move is legal only if it leaves both safe. The queen therefore behaves
//!   like a second king for *safety* — it may not move onto an attacked square,
//!   may not stay on one (it must respond to the "check" against it), and cannot be
//!   left en prise by any other move — while it still **moves** as an ordinary
//!   queen. Promoting a pawn to a queen creates another royal that must likewise
//!   be kept safe (all queens are royal,
//!   [`royal_constraint_active`](WideVariant::royal_constraint_active) stays on).
//! * **Castling is allowed**, standard `KQkq`. The king may not castle out of,
//!   through, or into check (the ordinary king-safety rule), and a castle that
//!   would leave the queen attacked is rejected like any other move.
//! * **Pawns** double-push, capture diagonally, take en passant, and promote to
//!   Queen / Rook / Bishop / Knight (the trait defaults).
//! * **Win by checkmate of *either* royal**, standard 8x8 chess otherwise.
//!
//! No draw hook is overridden: like the reference [`StandardChess`], coregal chess
//! carries the trait-default terminal rules (no fifty-move / repetition /
//! insufficient-material adjudication at the bare-position level), matching
//! Fairy-Stockfish's `coregal` for perft.
//!
//! ## Where it diverges from standard chess
//!
//! Because the queen may not move to — or be left on — an attacked square, coregal
//! forbids moves standard chess allows the moment a queen becomes exposed. This
//! shows up immediately: the startpos already diverges at depth 3 (standard 8902,
//! coregal 8882), where a queen sortie onto a square the opponent attacks (e.g.
//! `Qh5` into a knight on `f6`) is simply illegal here.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant coregal` (`coregal_variant()`,
//! `variant.cpp:1122` — the standard `chess_variant_base()` with
//! `extinctionPieceTypes = QUEEN`, `extinctionPseudoRoyal = true`, and
//! `extinctionPieceCount = 64` so *every* queen is royal). The array and castling
//! field are the standard chess ones:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//! ```
//!
//! mcr and FSF spell the position byte-for-byte identically (standard chess
//! letters, no dialect rewrite).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
#[allow(unused_imports)] // `StandardChess` is referenced by the rustdoc intra-doc links.
use crate::geometry::StandardChess;
use crate::geometry::{Bitboard, Board, Chess8x8, WideRole, WideVariant};
use crate::Color;

/// The confirmed coregal starting placement: the standard chess array.
const COREGAL_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

/// The coregal chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It is the reference [`StandardChess`] ruleset with exactly one change — the
/// queen is royal as well as the king, so a side loses if *either* is checkmated.
/// This is expressed through the **multi-royal** king-safety machinery
/// ([`multi_royal`](WideVariant::multi_royal)) with the strict pseudo-royal rule
/// ([`royals_all_must_survive`](WideVariant::royals_all_must_survive)): the royal
/// set is the king plus every queen, and a move must leave all of them unattacked.
/// Every piece, the pawn double-step, en passant, promotion, and castling are the
/// standard-chess trait defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CoregalRules;

impl WideVariant<Chess8x8> for CoregalRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field (Pawn..King, the standard army; promotions are Queen /
    /// Rook / Bishop / Knight, all within the prefix). See
    /// [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(COREGAL_START_PLACEMENT)
            .expect("the standard starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            // Standard castling rights: coregal keeps ordinary castling.
            castling: GenericCastling::standard::<Chess8x8>(),
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

    // --- royal queen (King AND Queen both royal) --------------------------

    /// Route coregal through the generic multi-royal king-safety path so a *set* of
    /// royal squares (king + every queen) is verified per move, rather than the
    /// single-king fast path. Combined with
    /// [`royals_all_must_survive`](WideVariant::royals_all_must_survive) this makes
    /// the queen behave like a second king for safety. (FSF `coregal`:
    /// `extinctionPseudoRoyal` on the QUEEN, with the KING already royal.)
    fn multi_royal() -> bool {
        true
    }

    /// The royal squares of `color` are its king **and** every queen — both are
    /// subject to check and checkmate. A promoted pawn on a queen adds another
    /// royal that must equally be kept safe (FSF `extinctionPieceCount = 64`, so no
    /// matter how many queens a side has, all of them are royal).
    fn royal_squares(board: &Board<Chess8x8>, color: Color) -> Bitboard<Chess8x8> {
        board.kings_of(color) | board.pieces(color, WideRole::Queen)
    }

    /// Strict pseudo-royalty (FSF `extinctionPseudoRoyal`): a legal move must leave
    /// **every** royal — the king *and* the queen — unattacked, and a side is in
    /// check whenever *any* royal is attacked. This is the rule that distinguishes
    /// coregal from Spartan's "at least one royal survives" duple check: here the
    /// queen may never be sacrificed or left en prise.
    fn royals_all_must_survive() -> bool {
        true
    }
}

/// Coregal chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Coregal::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Coregal::from_fen`](GenericPosition::from_fen). Every rule is the standard
/// [`StandardChess`] default except that the queen is royal alongside the king:
/// a side loses if *either* is checkmated. See the [module docs](self) for the
/// royal-queen rule and how it rides the multi-royal machinery.
pub type Coregal = GenericPosition<Chess8x8, CoregalRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::WideMoveKind;

    /// The canonical start FEN round-trips and keeps standard castling rights.
    #[test]
    fn startpos_fen_round_trips_with_castling() {
        let pos = Coregal::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
        assert_eq!(pos.turn(), Color::White);
        assert_eq!(pos.legal_move_count(), 20);
        assert!(pos.castling().has_any(Color::White));
        assert!(pos.castling().has_any(Color::Black));
    }

    /// The royal queen cannot move onto a square the opponent attacks: with a black
    /// knight on f6, `Qh5` (and `Qg4`) are illegal — the queen would be en prise,
    /// exactly a king refusing to step into check. This is the coregal divergence
    /// from standard chess.
    #[test]
    fn royal_queen_cannot_move_into_attack() {
        // 1.e4 opens the d1-h5 diagonal; a black knight on f6 attacks g4 and h5.
        let pos = Coregal::from_fen("rnbqkb1r/pppppppp/5n2/8/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 1")
            .expect("valid coregal FEN");
        let queen_dests: Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter(|m| pos.board().role_at(m.from::<Chess8x8>()) == Some(WideRole::Queen))
            .map(|m| m.to::<Chess8x8>())
            .collect();
        let g4 = crate::geometry::Square::<Chess8x8>::from_file_rank(6, 3).unwrap();
        let h5 = crate::geometry::Square::<Chess8x8>::from_file_rank(7, 4).unwrap();
        assert!(
            !queen_dests.contains(&g4),
            "queen may not move to g4 (attacked by Nf6)"
        );
        assert!(
            !queen_dests.contains(&h5),
            "queen may not move to h5 (attacked by Nf6)"
        );
        // The nearer diagonal squares e2 and f3 are unattacked and remain legal.
        let f3 = crate::geometry::Square::<Chess8x8>::from_file_rank(5, 2).unwrap();
        assert!(queen_dests.contains(&f3), "Qf3 is safe and legal");
    }

    /// An attacked queen is "in check": with the queen on an open file facing an
    /// enemy rook, the side to move must resolve the threat to the queen — a king
    /// move that ignores it is illegal, since both royals must survive.
    #[test]
    fn attacked_queen_must_be_saved() {
        // Black rook d8 attacks the white queen d1 down the open d-file.
        let pos = Coregal::from_fen("3rk3/8/8/8/8/8/8/3QK3 w - - 0 1").expect("valid FEN");
        // Every legal move must leave the queen unattacked: no move keeps the queen
        // on the d-file (still attacked) unless it removes the attacker.
        for mv in pos.legal_moves() {
            let next = pos.play(&mv);
            // After our move it is Black's turn; the white royals must all be safe.
            assert!(
                next.royal_squares(Color::White)
                    .into_iter()
                    .all(|sq| { next.attackers_of(sq, Color::Black).is_empty() }),
                "no legal coregal move may leave a white royal attacked",
            );
        }
    }

    /// A double pawn push still sets an en-passant target — every non-royal rule is
    /// standard chess.
    #[test]
    fn pawn_double_push_sets_en_passant() {
        let pos = Coregal::startpos();
        let dbl = pos
            .legal_moves()
            .into_iter()
            .find(|m| matches!(m.kind(), WideMoveKind::DoublePawnPush))
            .expect("a double pawn push exists at the start");
        let next = pos.play(&dbl);
        assert!(
            next.ep_square().is_some(),
            "a double push creates an en-passant target",
        );
    }
}
