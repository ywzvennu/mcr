//! Horde as a [`Variant`]: white is a kingless horde of pawns, black a standard
//! army.
//!
//! White starts with thirty-six pawns and **no king**; black has the full
//! standard back rank and pawns plus the two black castling rights. The asymmetry
//! drives every rule difference:
//!
//! - **Kingless white.** White has no royal piece, so white is never in check and
//!   white's move legality never consults king safety. Black, by contrast, has a
//!   normal royal king with ordinary check, checkmate, and pin rules. Because the
//!   two sides need different legality treatment, horde leaves the fast-legality
//!   sentinel ([`Variant::USES_FAST_LEGALITY`] is `false`) and supplies
//!   [`Variant::is_legal_after`]: when white is to move there is no king to keep
//!   safe (vacuously legal), and when black is to move the standard king-safety
//!   filter applies.
//! - **First-rank double pushes.** White's pawns may begin on the first rank, and
//!   such a pawn may advance two squares just as a second-rank pawn would. Per the
//!   horde convention, however, a *first-rank* double push does **not** create an
//!   en-passant target (only the ordinary second-rank double push does), so it is
//!   generated as a plain quiet two-square advance and cannot be answered en
//!   passant. The core pawn generator admits this through the dedicated horde
//!   pseudo path ([`Position::pseudo_into_horde`]), reached here by overriding
//!   [`Variant::gen_pseudo`]; standard pawns are generated identically.
//! - **No white castling.** White has no king, so only black may castle. The
//!   starting rights are `kq`; [`Variant::castling_allowed`] stays `true` (black
//!   still castles) and the absence of white rights is carried in the position's
//!   castling rights, not the rule layer.
//!
//! # Win, loss, and draw
//!
//! Black wins by capturing all of white's material (no white piece remains), which
//! [`Variant::extra_terminal`] reports the moment white's piece count reaches
//! zero. White wins by checkmating black in the ordinary way — black is the only
//! side with a king, so only black can be mated, and the inherited
//! checkmate/stalemate detection handles it. **Stalemate is a draw:** when the
//! side to move has no legal move and is not in check the game is drawn, following
//! the prevailing FIDE-style horde convention; this is the inherited
//! [`VariantPosition::end_reason`] behaviour and is deliberately *not* overridden.

use super::{Variant, VariantId, VariantPosition};
use crate::position::parse_castling_field;
use crate::{Color, EndReason, Move, Position};

/// The horde rule layer: a kingless white pawn horde against a standard black
/// army. A zero-sized marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HordeRules;

impl Variant for HordeRules {
    type State = ();
    const ID: VariantId = VariantId::Horde;

    // White has no king, so white-to-move legality differs from standard chess
    // (no king to keep safe). Run the pseudo-legal + make-move filter path and
    // supply `is_legal_after`.
    const USES_FAST_LEGALITY: bool = false;

    /// H1: black wins the instant white has no material left.
    ///
    /// White's pieces can only ever be removed by black capturing them, so an
    /// empty horde is always reached on a black move and the resulting position is
    /// white to move. [`EndReason::HordeDefeated`] maps through
    /// `HordeDefeated.outcome(White) = Decisive { winner: Black }`, awarding the
    /// win to black, the side that eliminated the horde — exactly the desired
    /// decisive result.
    fn extra_terminal(core: &Position, _state: &Self::State) -> Option<EndReason> {
        if core.board().by_color(Color::White).is_empty() {
            Some(EndReason::HordeDefeated)
        } else {
            None
        }
    }

    /// H2: king-safety legality.
    ///
    /// When white is to move there is no white king, so no move can leave it in
    /// check — every pseudo-legal white move is legal. When black is to move the
    /// ordinary king-safety filter applies to black's royal king. The core
    /// `Position::move_keeps_king_safe` already returns `true` when the moving
    /// side has no king, so it expresses both cases directly.
    fn is_legal_after(parent: &Position, mv: &Move, child: &Position) -> bool {
        let _ = child;
        parent.move_keeps_king_safe(mv)
    }

    /// H_pseudo: generate white's first-rank double pushes via the horde pawn
    /// path. Standard piece movement (including black's) is unchanged.
    fn gen_pseudo(core: &Position, out: &mut Vec<Move>) {
        core.pseudo_into_horde(out);
    }

    /// White has no king and therefore cannot castle; black castles normally.
    ///
    /// The rule layer keeps castling enabled (black uses it); whether *white* may
    /// castle is governed by white's castling rights, which the horde start has
    /// none of (`kq`).
    fn castling_allowed() -> bool {
        true
    }

    /// FEN validation must not demand a white king — white is a kingless horde.
    /// Black's king remains royal, so this relaxes only the king *count*.
    fn requires_two_kings() -> bool {
        false
    }

    /// H11: the horde starting board — white's thirty-six pawns and no king,
    /// black's standard army with the two black (`kq`) castling rights.
    ///
    /// Built directly from the published placement rather than the full FEN parser
    /// because the kingless white side fails the standard two-king validation; the
    /// placement and `kq` rights are the single source of truth here.
    fn starting_board() -> (crate::Board, crate::CastlingRights, Self::State) {
        let board = crate::Board::from_fen_placement(HORDE_START_PLACEMENT)
            .expect("the horde starting placement is valid");
        let castling = parse_castling_field(HORDE_START_CASTLING, &board)
            .expect("the horde starting castling field is valid");
        (board, castling, ())
    }
}

/// The placement field of the canonical horde starting position (the source of
/// truth for [`HordeRules::starting_board`]): white's thirty-six pawns and no
/// king, black's standard army.
const HORDE_START_PLACEMENT: &str =
    "rnbqkbnr/pppppppp/8/1PP2PP1/PPPPPPPP/PPPPPPPP/PPPPPPPP/PPPPPPPP";

/// The castling-rights field of the horde start: black keeps both rights, white
/// (kingless) has none.
const HORDE_START_CASTLING: &str = "kq";

/// Horde as a [`VariantPosition`].
///
/// White is a kingless pawn horde; black is a standard army with a royal king.
/// White wins by checkmating black, black wins by capturing all white material,
/// and stalemate is a draw.
pub type Horde = VariantPosition<HordeRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variant::perft_variant;
    use crate::{Color, MoveKind, Outcome, Role, Square};

    /// The canonical horde starting FEN.
    const HORDE_START_FEN: &str =
        "rnbqkbnr/pppppppp/8/1PP2PP1/PPPPPPPP/PPPPPPPP/PPPPPPPP/PPPPPPPP w kq - 0 1";

    #[test]
    fn startpos_is_the_published_horde_start() {
        let pos = Horde::startpos();
        assert_eq!(pos.variant_id(), VariantId::Horde);
        assert_eq!(pos.to_fen(), HORDE_START_FEN);
        assert_eq!(pos.turn(), Color::White);
        // White has no king; black has exactly one.
        let board = pos.core().board();
        assert_eq!(board.pieces(Color::White, Role::King).count(), 0);
        assert_eq!(board.pieces(Color::Black, Role::King).count(), 1);
        // White's thirty-six pawns.
        assert_eq!(board.pieces(Color::White, Role::Pawn).count(), 36);
        // The published depth-1 perft from the horde start is 8.
        assert_eq!(pos.legal_moves().len(), 8);
    }

    #[test]
    fn startpos_fen_round_trips() {
        let pos: Horde = HORDE_START_FEN.parse().unwrap();
        assert_eq!(pos.to_fen(), HORDE_START_FEN);
        assert_eq!(Horde::default(), Horde::startpos());
    }

    #[test]
    fn white_is_never_in_check() {
        // White has no king, so `is_check` is always false even when black pieces
        // bear on white's camp.
        let pos = Horde::startpos();
        assert!(!pos.is_check());
    }

    #[test]
    fn first_rank_pawn_double_pushes_without_ep_target() {
        // A lone white pawn on the first rank may advance two squares, but per the
        // horde convention this does NOT set an en-passant target — it is a quiet
        // two-square move.
        let pos: Horde = "4k3/8/8/8/8/8/8/4P3 w - - 0 1".parse().unwrap();
        let mv = pos.parse_uci("e1e3").unwrap();
        assert_eq!(mv.kind(), MoveKind::Quiet);
        let after = pos.play(&mv);
        assert_eq!(after.core().ep_square(), None);
        assert_eq!(after.to_fen(), "4k3/8/8/8/8/4P3/8/8 b - - 0 1");
    }

    #[test]
    fn first_rank_double_push_is_not_ep_capturable() {
        // Black pawn on d3, white pawn on e1. White advances e1-e3; because a
        // first-rank double push leaves no en-passant target, black has no en
        // passant reply on e2 (only its quiet d3-d2 advance remains for that pawn).
        let pos: Horde = "4k3/8/8/8/8/3p4/8/4P3 w - - 0 1".parse().unwrap();
        let push = pos.parse_uci("e1e3").unwrap();
        let after = pos.play(&push);
        assert_eq!(after.core().ep_square(), None);
        assert!(after.parse_uci("d3e2").is_err());
        let pawn_moves: Vec<String> = after
            .legal_moves()
            .iter()
            .filter(|m| m.from() == Square::D3)
            .map(|m| after.to_uci(m))
            .collect();
        assert_eq!(pawn_moves, vec!["d3d2".to_owned()]);
    }

    #[test]
    fn second_rank_double_push_still_sets_ep_target() {
        // A white pawn double-pushing from the *second* rank behaves exactly like
        // standard chess and DOES create an en-passant target, so the horde change
        // is confined to first-rank pawns.
        let pos: Horde = "4k3/8/8/8/8/8/4P3/8 w - - 0 1".parse().unwrap();
        let mv = pos.parse_uci("e2e4").unwrap();
        assert_eq!(mv.kind(), MoveKind::DoublePawnPush);
        let after = pos.play(&mv);
        assert_eq!(after.core().ep_square(), Some(Square::E3));
    }

    #[test]
    fn white_cannot_castle_black_can() {
        // Black has castling rights and an empty king-side; white has no king and
        // no rights, so white offers no castle.
        let pos: Horde = "r3k2r/8/8/8/8/8/PPPPPPPP/PPPPPPPP b kq - 0 1"
            .parse()
            .unwrap();
        let castles: Vec<MoveKind> = pos
            .legal_moves()
            .iter()
            .map(|m| m.kind())
            .filter(|k| matches!(k, MoveKind::CastleKingside | MoveKind::CastleQueenside))
            .collect();
        assert!(castles.contains(&MoveKind::CastleKingside));
        assert!(castles.contains(&MoveKind::CastleQueenside));
    }

    #[test]
    fn eliminating_all_white_material_is_a_black_win() {
        // White has a single pawn; black is to move and captures it, leaving the
        // horde empty -> black wins.
        let pos: Horde = "4k3/8/8/8/8/8/3r4/3P4 b - - 0 1".parse().unwrap();
        let cap = pos.parse_uci("d2d1").unwrap();
        let after = pos.play(&cap);
        assert!(after.core().board().by_color(Color::White).is_empty());
        assert_eq!(
            after.outcome(),
            Some(Outcome::Decisive {
                winner: Color::Black
            })
        );
        assert_eq!(after.end_reason(), Some(EndReason::HordeDefeated));
    }

    #[test]
    fn black_can_be_checkmated_by_the_horde() {
        // White (the horde) checkmates the black king the ordinary way — only
        // black has a king, so only black can be mated. Black king cornered on a8,
        // white queen b7 giving check, with the a6 pawn guarding the queen so the
        // king cannot capture it and the queen covers every flight square.
        let mate: Horde = "k7/1Q6/P7/8/8/8/8/8 b - - 0 1".parse().unwrap();
        assert!(mate.is_check());
        assert_eq!(mate.legal_moves().len(), 0);
        assert_eq!(
            mate.outcome(),
            Some(Outcome::Decisive {
                winner: Color::White
            })
        );
        assert_eq!(mate.end_reason(), Some(EndReason::Checkmate));
    }

    #[test]
    fn stalemate_of_black_is_a_draw() {
        // Black king a8 with no legal move and not in check: a draw, per the horde
        // stalemate convention (inherited, not overridden).
        let pos: Horde = "k7/2Q5/1P6/8/8/8/8/8 b - - 0 1".parse().unwrap();
        assert!(!pos.is_check());
        assert_eq!(pos.legal_moves().len(), 0);
        assert_eq!(pos.outcome(), Some(Outcome::Draw));
        assert_eq!(pos.end_reason(), Some(EndReason::Stalemate));
    }

    #[test]
    fn published_perft_horde_start_shallow() {
        // From shakmaty `tests/horde.perft`, id `horde-start`.
        let pos = Horde::startpos();
        assert_eq!(perft_variant(&pos, 1), 8);
        assert_eq!(perft_variant(&pos, 2), 128);
        assert_eq!(perft_variant(&pos, 3), 1274);
    }
}
