//! Antichess (also called Giveaway or Losing chess) as a [`Variant`]: the goal
//! is to *lose* all your pieces. Almost every standard concept is inverted or
//! removed.
//!
//! # Rules
//!
//! - **The king is an ordinary piece** ([`Variant::king_is_royal`] is `false`).
//!   It can be captured, it gives no check, and it may be left "en prise". There
//!   is no check, no checkmate, and no castling
//!   ([`Variant::castling_allowed`] is `false`).
//! - **Pawns may promote to a king** as well as to a knight, bishop, rook, or
//!   queen ([`Variant::promotion_roles`] adds [`Role::King`]).
//! - **Captures are forced.** If the side to move has *any* capture available
//!   (including en passant), then every one of its legal moves must be a
//!   capture; it must capture, though it may choose which capture to play
//!   ([`Variant::filter_forced`], H7).
//! - **The win condition is inverted.** A side that has *no legal move* — either
//!   because it has been reduced to zero pieces, or because it is stalemated —
//!   **wins**. This is the opposite of standard chess, where having no move is a
//!   loss (checkmate) or a draw (stalemate).
//!
//! # King safety is switched off entirely
//!
//! Because the king is not royal, there is no king-safety constraint at all: a
//! move that leaves your own king attacked is perfectly legal, and so is a move
//! that captures the opponent's king. The fast-legality sentinel
//! ([`Variant::USES_FAST_LEGALITY`]) is therefore set to `false` and
//! [`Variant::is_legal_after`] always returns `true`, so the legal moves are
//! exactly the pseudo-legal moves (then narrowed to forced captures by H7).
//!
//! # How the inverted win is expressed
//!
//! The shared single-position outcome machinery
//! ([`VariantPosition::outcome`](super::VariantPosition::outcome)) maps a
//! no-legal-move position to [`EndReason::Stalemate`] (a draw) when the king is
//! not royal — correct for nothing in antichess, where that situation is a *win*
//! for the side to move. Antichess therefore detects the terminal position
//! first, in [`Variant::extra_terminal`] (H1, consulted before the generic
//! no-move handling), and reports it as [`EndReason::VariantWin`], whose
//! [`EndReason::outcome`] awards the win to the side *to move* — exactly the
//! antichess rule. `extra_terminal` reproduces the antichess legal-move test
//! (pseudo-legal moves, narrowed to forced captures) directly from the core
//! position, so it never calls back into the outcome path and cannot recurse.
//!
//! # Draw simplifications
//!
//! The two basic decisive endings — zero pieces and no legal move — are
//! implemented exactly. The fine-grained antichess *material draws* (for
//! example, a lone bishop versus a lone bishop locked on opposite colours, where
//! neither side can ever force a capture) are **not** specially detected; such
//! positions are simply played out and will end by the seventy-five-move rule
//! (the shared automatic clock draw) or by a forced sequence. The standard
//! insufficient-material draw is disabled ([`Variant::insufficient_material_is_draw`]
//! returns `false`) because "insufficient to mate" is meaningless when the goal
//! is to be captured.

use super::{Variant, VariantId, VariantPosition};
use crate::board::Board;
use crate::movelist::MoveList;
use crate::position::CastlingRights;
use crate::{EndReason, Move, Position, Role};

/// The roles a pawn may promote to in antichess: the standard four plus the
/// king, in a stable order.
const ANTICHESS_PROMOTION_ROLES: [Role; 5] = [
    Role::Knight,
    Role::Bishop,
    Role::Rook,
    Role::Queen,
    Role::King,
];

/// The Antichess (Giveaway / Losing chess) rule layer: lose all your pieces to
/// win, with forced captures, a non-royal king, king-promotion, and no castling.
/// A zero-sized marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AntichessRules;

impl Variant for AntichessRules {
    type State = ();
    const ID: VariantId = VariantId::Antichess;

    /// King safety does not apply at all (the king is not royal), so the fast
    /// core generator — which filters out moves that leave the king attacked —
    /// must not be used. Generation runs the pseudo-legal pass and the
    /// always-true [`AntichessRules::is_legal_after`] filter instead.
    const USES_FAST_LEGALITY: bool = false;

    /// H2: there is no king-safety constraint in antichess, so every
    /// pseudo-legal move is legal. (Forced-capture narrowing happens separately
    /// in [`AntichessRules::filter_forced`].)
    fn is_legal_after(_parent: &Position, _mv: &Move, _child: &Position) -> bool {
        true
    }

    /// H3: the king is an ordinary, capturable piece — there is no check or
    /// checkmate.
    fn king_is_royal() -> bool {
        false
    }

    /// Antichess places no king-count requirement: a king can be captured (a
    /// side may have zero kings) and a pawn can promote to a king (a side may
    /// have several), so FEN validation must not insist on exactly one per side.
    fn requires_two_kings() -> bool {
        false
    }

    /// "Insufficient mating material" is meaningless when the objective is to be
    /// captured, so the standard material draw is disabled. (The shared check is
    /// only consulted for royal-king variants anyway, but disabling it makes the
    /// intent explicit and keeps antichess from ever ending on it.)
    fn insufficient_material_is_draw() -> bool {
        false
    }

    /// H7: captures are forced. If any pseudo-legal move is a capture (ordinary,
    /// en passant, or a capturing promotion), keep only the captures.
    fn filter_forced(_core: &Position, _state: &Self::State, moves: &mut MoveList) {
        if moves.iter().any(|mv| mv.is_capture()) {
            moves.retain(|mv| mv.is_capture());
        }
    }

    /// H8: pawns may promote to a king in addition to knight, bishop, rook, and
    /// queen. This governs which promotion letters the variant-aware UCI parser
    /// accepts (`a7a8k`).
    fn promotion_roles() -> &'static [Role] {
        &ANTICHESS_PROMOTION_ROLES
    }

    /// The core pseudo-legal generator emits only the four standard promotion
    /// roles (it has no knowledge of [`Variant::promotion_roles`]). Antichess
    /// adds the king-promotion moves here: the standard pass is generated, then
    /// every queen-promotion (which the core always emits, exactly once per
    /// promoting from/to/capture combination) is mirrored to a king-promotion of
    /// the same shape. This is the only pseudo-legal difference from standard
    /// chess; the slow legality path (king safety is off) then accepts every one.
    fn gen_pseudo(core: &Position, out: &mut MoveList) {
        core.pseudo_into(out);
        let king_promos: Vec<Move> = out
            .iter()
            .filter_map(|mv| match mv.kind() {
                crate::MoveKind::Promotion {
                    role: Role::Queen,
                    capture,
                } => Some(Move::new(
                    mv.from(),
                    mv.to(),
                    crate::MoveKind::Promotion {
                        role: Role::King,
                        capture,
                    },
                )),
                _ => None,
            })
            .collect();
        out.extend(king_promos);
    }

    /// H9: antichess has no castling.
    fn castling_allowed() -> bool {
        false
    }

    /// H1: the inverted win. The side to move *wins* if it has no pieces left or
    /// no legal move (in antichess, being stalemated is a win). This is reported
    /// as [`EndReason::VariantWin`], whose outcome awards the win to the side to
    /// move.
    ///
    /// Consulted before the generic no-move handling, so the would-be
    /// [`EndReason::Stalemate`] draw is never reached. The legal-move test here
    /// is computed directly from the core position (pseudo-legal moves narrowed
    /// to forced captures), so it does not call back into the outcome path.
    fn extra_terminal(core: &Position, _state: &Self::State) -> Option<EndReason> {
        if core.board().by_color(core.turn()).is_empty() || !has_legal_move(core) {
            return Some(EndReason::VariantWin);
        }
        None
    }

    /// H11: the antichess start is the standard placement but with no castling
    /// rights (antichess has no castling).
    fn starting_board() -> (Board, CastlingRights, Self::State) {
        (Board::standard(), CastlingRights::NONE, ())
    }
}

/// Whether the side to move has at least one antichess-legal move in `core`.
///
/// Antichess legality is "pseudo-legal, then forced-capture narrowed" (king
/// safety never applies), so the side has a move iff it has any pseudo-legal
/// move at all: forced-capture narrowing only ever drops non-captures when a
/// capture exists, so it can never turn a non-empty pseudo-legal set into an
/// empty one. We therefore only need to know whether *any* pseudo-legal move
/// exists.
fn has_legal_move(core: &Position) -> bool {
    let mut pseudo = MoveList::new();
    core.pseudo_into(&mut pseudo);
    !pseudo.is_empty()
}

/// Antichess as a [`VariantPosition`].
///
/// Movegen runs the slow pseudo-legal + (always-true) make-move filter, then
/// narrows to forced captures; there is no king safety, no castling, and pawns
/// may promote to a king. The inverted win — a side with no move or no pieces
/// wins — is reported through
/// [`VariantPosition::outcome`](super::VariantPosition::outcome).
pub type Antichess = VariantPosition<AntichessRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variant::perft_variant;
    use crate::{Color, EndReason, Outcome, Square, VariantId};

    const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1";

    fn play_line(mut pos: Antichess, ucis: &[&str]) -> Antichess {
        for uci in ucis {
            let mv = pos.parse_uci(uci).expect("legal uci move");
            pos = pos.play(&mv);
        }
        pos
    }

    #[test]
    fn startpos_is_standard_placement_without_castling() {
        let pos = Antichess::startpos();
        assert_eq!(pos.variant_id(), VariantId::Antichess);
        assert_eq!(pos.turn(), Color::White);
        assert_eq!(pos.to_fen(), START_FEN);
        assert_eq!(pos.core().castling_rights(), CastlingRights::NONE);
        assert!(!AntichessRules::castling_allowed());
        // Published shakmaty `antichess.perft` start perft(1).
        assert_eq!(pos.legal_moves().len(), 20);
        assert!(pos.outcome().is_none());
        // Parsing the canonical FEN yields the same position.
        assert_eq!(Antichess::from_fen(START_FEN).unwrap(), pos);
    }

    #[test]
    fn never_in_check_king_not_royal() {
        // A position where white's king is plainly attacked by a black rook; in
        // antichess this is not "check" and imposes no constraint.
        let pos: Antichess = "8/8/8/8/8/8/8/r3K3 w - - 0 1".parse().unwrap();
        assert!(!pos.is_check());
        assert!(!AntichessRules::king_is_royal());
        // The king may walk into or stay under attack: Ke1-e2/d2/f2 etc. are all
        // legal because there is no capture forcing it, and king safety is off.
        let ucis: Vec<String> = pos.legal_moves().iter().map(|m| m.to_uci()).collect();
        assert!(ucis.contains(&"e1d2".to_string()), "{ucis:?}");
    }

    #[test]
    fn captures_are_forced() {
        // White pawn on e4, black pawn on d5: exd5 is the only capture, so it is
        // forced and is the sole legal move (no other piece, nothing else moves).
        let pos: Antichess = "8/8/8/3p4/4P3/8/8/8 w - - 0 1".parse().unwrap();
        let moves = pos.legal_moves();
        assert!(
            moves.iter().all(|mv| mv.is_capture()),
            "all moves must be captures when a capture exists: {:?}",
            moves.iter().map(|m| m.to_uci()).collect::<Vec<_>>()
        );
        let ucis: Vec<String> = moves.iter().map(|m| m.to_uci()).collect();
        assert_eq!(ucis, vec!["e4d5".to_string()]);
    }

    #[test]
    fn en_passant_is_a_forced_capture() {
        // White pawn e5, black just played d7-d5 (ep target d6). exd6 e.p. is the
        // only capture and is forced. The lone white king on a1 must not move.
        let pos: Antichess = "8/8/8/3pP3/8/8/8/K7 w - d6 0 1".parse().unwrap();
        let ucis: Vec<String> = pos.legal_moves().iter().map(|m| m.to_uci()).collect();
        assert_eq!(ucis, vec!["e5d6".to_string()], "ep capture must be forced");
    }

    #[test]
    fn multiple_captures_all_retained_no_quiets() {
        // White king on e4 with two adjacent black pawns it could capture (d5,
        // f5) and empty quiet squares too. Only the captures survive the filter.
        let pos: Antichess = "8/8/8/3p1p2/4K3/8/8/8 w - - 0 1".parse().unwrap();
        let moves = pos.legal_moves();
        assert!(moves.iter().all(|mv| mv.is_capture()));
        let mut ucis: Vec<String> = moves.iter().map(|m| m.to_uci()).collect();
        ucis.sort();
        assert_eq!(ucis, vec!["e4d5".to_string(), "e4f5".to_string()]);
    }

    #[test]
    fn king_is_capturable() {
        // White rook on a8, black king on h8 alone: Rxh8 captures the king and is
        // forced (it is the only capture). The king is just an ordinary piece.
        let pos: Antichess = "R6k/8/8/8/8/8/8/8 w - - 0 1".parse().unwrap();
        let ucis: Vec<String> = pos.legal_moves().iter().map(|m| m.to_uci()).collect();
        assert!(ucis.contains(&"a8h8".to_string()), "{ucis:?}");
        assert!(
            ucis.iter().all(|m| m == "a8h8"),
            "the king capture is forced: {ucis:?}"
        );
        let after = play_line(pos, &["a8h8"]);
        // Black now has no pieces at all -> black (the side to move) WINS.
        assert!(after.core().board().by_color(Color::Black).is_empty());
        assert_eq!(
            after.outcome(),
            Some(Outcome::Decisive {
                winner: Color::Black
            })
        );
        assert_eq!(after.end_reason(), Some(EndReason::VariantWin));
    }

    #[test]
    fn pawn_promotes_to_king() {
        // White pawn on a7 promotes; the king is an allowed promotion role.
        let pos: Antichess = "8/P7/8/8/8/8/8/7k w - - 0 1".parse().unwrap();
        let ucis: Vec<String> = pos.legal_moves().iter().map(|m| m.to_uci()).collect();
        assert!(ucis.contains(&"a7a8k".to_string()), "{ucis:?}");
        // All five promotion roles (incl. king) are offered.
        for promo in ["a7a8n", "a7a8b", "a7a8r", "a7a8q", "a7a8k"] {
            assert!(
                ucis.contains(&promo.to_string()),
                "missing {promo}: {ucis:?}"
            );
        }
        // The king-promotion move actually applies, placing a white king on a8.
        let after = play_line(pos, &["a7a8k"]);
        assert_eq!(
            after.core().board().piece_at(Square::A8).map(|p| p.role),
            Some(Role::King)
        );
    }

    #[test]
    fn no_legal_move_is_a_win_for_side_to_move() {
        // White's only piece is a pawn on a2, head-to-head blocked by a black
        // pawn on a3: it can neither push (a3 occupied) nor capture (b3 is empty,
        // and a pawn captures only diagonally). White to move therefore has no
        // legal move at all, which in antichess is a WIN for White.
        let pos: Antichess = "8/8/8/8/8/p7/P7/8 w - - 0 1".parse().unwrap();
        assert!(pos.legal_moves().is_empty(), "white should be immobilized");
        assert_eq!(
            pos.outcome(),
            Some(Outcome::Decisive {
                winner: Color::White
            })
        );
        assert_eq!(pos.end_reason(), Some(EndReason::VariantWin));
    }

    #[test]
    fn zero_pieces_is_a_win_for_that_side() {
        // Black has no pieces on the board; black to move. A side with no pieces
        // wins in antichess.
        let pos: Antichess = "8/8/8/8/8/8/8/K7 b - - 0 1".parse().unwrap();
        assert!(pos.core().board().by_color(Color::Black).is_empty());
        assert_eq!(
            pos.outcome(),
            Some(Outcome::Decisive {
                winner: Color::Black
            })
        );
        assert_eq!(pos.end_reason(), Some(EndReason::VariantWin));
    }

    #[test]
    fn start_perft_shallow() {
        // Published shakmaty `antichess.perft` start counts (cheap depths).
        let pos = Antichess::startpos();
        assert_eq!(perft_variant(&pos, 1), 20);
        assert_eq!(perft_variant(&pos, 2), 400);
        assert_eq!(perft_variant(&pos, 3), 8067);
    }

    #[test]
    fn fen_round_trip() {
        for fen in [
            START_FEN,
            "8/1p6/8/8/8/8/P7/8 w - - 0 1",
            "R6k/8/8/8/8/8/8/8 w - - 0 1",
        ] {
            let pos: Antichess = fen.parse().unwrap();
            assert_eq!(pos.to_fen(), fen, "round trip for {fen}");
        }
    }

    #[test]
    fn uci_round_trip_king_promotion() {
        let pos: Antichess = "8/P7/8/8/8/8/8/7k w - - 0 1".parse().unwrap();
        let mv = pos.parse_uci("a7a8k").expect("king promotion is legal");
        assert_eq!(pos.to_uci(&mv), "a7a8k");
    }
}
