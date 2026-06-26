//! Atomic chess as a [`Variant`]: every capture detonates, removing a 3x3 blast
//! of pieces around the square the capturing piece lands on, and the game is won
//! by exploding the enemy king.
//!
//! # The explosion
//!
//! When a piece is captured, an explosion centred on the capturing piece's
//! **destination square** removes, in addition to the captured piece:
//!
//! - the capturing piece itself, on that destination square, and
//! - every piece on the eight squares orthogonally or diagonally adjacent to the
//!   destination, **except pawns**.
//!
//! Pawns are immune to the blast: a pawn adjacent to an explosion survives. A
//! pawn is only removed if it is itself the captured piece or the capturing
//! piece. Every other piece — including **kings** — is destroyed if it lies on
//! the blast centre or one of the eight adjacent squares.
//!
//! ## En passant
//!
//! En passant is the one case where the captured piece does not sit on the
//! destination square: the captured pawn sits one rank back, on the square the
//! moving pawn passed *behind*. That pawn is still removed (the core make-move
//! removes it), but the blast is centred on the **destination square** like any
//! other capture — matching the published atomic perft. So a piece adjacent to
//! the destination is caught in the blast, while a piece adjacent only to the
//! captured pawn's square is not.
//!
//! # Win condition and legality
//!
//! You win by removing the enemy king — whether by capturing it directly or by
//! catching it in a blast. Because removing the enemy king ends the game in your
//! favour immediately, such a move is legal **even while your own king is in
//! check**. Conversely, a move is illegal if, after the explosion, your own king
//! is gone: you may never blow up your own king, which in particular means the
//! king can never capture (capturing detonates the square it just moved to,
//! taking the king with it). For non-capturing moves the ordinary king-safety
//! rule applies: you may not leave your own king attacked.

use super::{Variant, VariantId, VariantPosition};
use crate::attacks::king_attacks;
use crate::movelist::MoveList;
use crate::{EndReason, Move, MoveKind, Piece, Position, Role, Square};

/// The atomic rule layer: standard chess movement plus capture explosions and a
/// king-explosion win condition. A zero-sized marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AtomicRules;

/// Whether `mv` is a capturing move (an ordinary capture, a capturing
/// promotion, or en passant), and therefore detonates.
fn is_capture(mv: &Move) -> bool {
    matches!(
        mv.kind(),
        MoveKind::Capture | MoveKind::Promotion { capture: true, .. } | MoveKind::EnPassant
    )
}

/// Applies the atomic explosion triggered by capturing move `mv` to `core`,
/// which must already be the post-move board (the capturing piece has reached
/// its destination and the captured piece is removed by the core make-move).
///
/// The blast is centred on the move's **destination square** — the square the
/// capturing piece lands on — for *every* capture, including en passant. It
/// removes the capturing piece itself and every non-pawn piece on the eight
/// squares adjacent to the destination. Kings are destroyed by the blast; pawns
/// adjacent to it survive.
///
/// For en passant the captured pawn does not sit on the destination square (it
/// sits one rank back, on the square the moving pawn passed behind); the core
/// make-move has already removed it, so only the blast around the destination
/// remains to apply here.
fn detonate(core: &mut Position, mv: &Move) {
    let center = mv.to();

    // The capturing piece is destroyed at its destination (the blast centre).
    core.remove_piece_tracked(center);

    // Every adjacent non-pawn piece is destroyed; pawns survive the blast.
    for sq in king_attacks(center) {
        if let Some(piece) = core.board().piece_at(sq) {
            if piece.role != Role::Pawn {
                core.remove_piece_tracked(sq);
            }
        }
    }
}

/// Whether the capturing move `mv` is legal in atomic chess from `parent`.
///
/// The move is applied (the core make-move plus the explosion) and judged on the
/// fully-exploded position with the same rule as [`AtomicRules::is_legal_after`]:
/// the mover's king must survive, and either the enemy king is gone (a win, legal
/// even out of check) or the mover's king is not attacked by a non-king piece.
///
/// `mv` must be a capturing move; non-captures never detonate and are handled by
/// the fast non-capture generator instead.
fn capture_is_legal(parent: &Position, mv: &Move) -> bool {
    let mut after = parent.play(mv);
    detonate(&mut after, mv);

    let mover = parent.turn();
    let opponent = mover.opposite();

    let Some(my_king) = after.board().king_of(mover) else {
        // The blast (or capture) removed the mover's own king: never legal.
        return false;
    };

    let Some(enemy_king) = after.board().king_of(opponent) else {
        // The enemy king is gone: the mover wins, legal even while in check.
        return true;
    };

    // Both kings stand: ordinary king safety, but the enemy king never gives
    // check, so exclude it from the attacker set.
    let mut attackers = after.attackers_to(my_king, opponent, after.board().occupied());
    attackers.clear(enemy_king);
    attackers.is_empty()
}

impl Variant for AtomicRules {
    type State = ();
    const ID: VariantId = VariantId::Atomic;

    /// Atomic king safety is not standard, so the fast pin/check-mask generator
    /// cannot be used; legality is decided by [`AtomicRules::is_legal_after`]
    /// after a full make-move-and-explode.
    const USES_FAST_LEGALITY: bool = false;

    /// H4: detonate on any capture (including en passant).
    ///
    /// The core make-move (run before this hook) has already removed the captured
    /// piece — for en passant from its true square one rank back. The explosion
    /// itself is centred on the capturing piece's destination square, so the
    /// `captured` square is not needed here.
    fn capture_side_effects(
        core: &mut Position,
        _state: &mut Self::State,
        mv: &Move,
        _captured: (Piece, Square),
    ) {
        detonate(core, mv);
    }

    /// H2: legality after the move and its explosion have been applied.
    ///
    /// Legal iff the moving side's king still exists **and** either the enemy
    /// king is gone (an explosion that removes the enemy king wins outright, so
    /// it is legal even out of check) or the moving side's king is not attacked.
    /// A move that destroys the moving side's own king — including any capture by
    /// the king itself — is rejected. Non-capturing moves reduce to the ordinary
    /// rule: the mover's king must not be left attacked.
    fn is_legal_after(parent: &Position, mv: &Move, child: &Position) -> bool {
        // `child` is the *core* child (the move applied without the explosion).
        // Legality is judged on the fully-exploded position, so apply the blast
        // to a local copy first whenever the move was a capture.
        let exploded;
        let after = if is_capture(mv) {
            let mut c = child.clone();
            detonate(&mut c, mv);
            exploded = c;
            &exploded
        } else {
            child
        };

        // The side that just moved is `parent.turn()`; in `after` it is the
        // opponent's turn.
        let mover = parent.turn();
        let opponent = mover.opposite();

        let Some(my_king) = after.board().king_of(mover) else {
            // The move blew up (or captured) the mover's own king: never legal.
            return false;
        };

        let Some(enemy_king) = after.board().king_of(opponent) else {
            // The enemy king is gone: the mover wins, so the move is legal
            // regardless of whether the mover is in check.
            return true;
        };

        // Both kings stand: ordinary king safety applies to the mover, with one
        // atomic twist — the enemy king never gives check (a king capturing
        // explodes itself, so it cannot threaten capture). Two kings may stand
        // adjacent. Exclude the enemy king from the attacker set so a move that
        // merely walks next to it is not wrongly rejected.
        let mut attackers = after.attackers_to(my_king, opponent, after.board().occupied());
        attackers.clear(enemy_king);
        attackers.is_empty()
    }

    /// Atomic legal-move generation split by capture status.
    ///
    /// A non-capturing atomic move triggers no explosion, so it has ordinary
    /// chess legality (with the atomic twist that the enemy king gives no check
    /// and the kings may stand adjacent). Those moves therefore come straight
    /// from the fast pin/check-mask generator via
    /// [`Position::atomic_noncapture_legal_into`], skipping the per-candidate
    /// make-move filter entirely. Only *captures* — which detonate — need the
    /// explosion-aware legality test, so they alone go through the
    /// pseudo-legal + make-move-and-explode filter.
    ///
    /// This reproduces the slow path's legal-move set exactly: the union of the
    /// fast generator's non-capturing moves (which equal the non-capturing moves
    /// that [`AtomicRules::is_legal_after`] accepts) and the explosion-filtered
    /// captures.
    fn legal_into(core: &Position, out: &mut MoveList) {
        // Non-captures (and castles): fast pin/check-mask path, no make-move.
        core.atomic_noncapture_legal_into(out);

        // Captures: gather the pseudo-legal capturing moves and keep those that
        // survive the explosion-aware legality test.
        let mut caps = MoveList::new();
        core.pseudo_into(&mut caps);
        caps.for_each(|mv| {
            if is_capture(&mv) && capture_is_legal(core, &mv) {
                out.push(mv);
            }
        });
    }

    /// H1: a missing king is decisive for the side whose king survives.
    ///
    /// Atomic positions reachable by play never have *both* kings missing, so
    /// exactly one side is the winner. [`EndReason::KingExploded`] is the variant
    /// reason whose outcome,
    /// `KingExploded.outcome(turn) = Decisive { winner: turn.opposite() }`, awards
    /// the win to the side *not* to move — which is the surviving side, since the
    /// side to move is the one whose king was just exploded.
    fn extra_terminal(core: &Position, _state: &Self::State) -> Option<EndReason> {
        // In any position reachable by legal play the only missing king is the
        // side-to-move's: the previous move exploded it, ending the game in the
        // mover's favour. The mover is the side *not* to move, which is exactly
        // the winner `KingExploded` awards.
        if core.board().king_of(core.turn()).is_none() {
            return Some(EndReason::KingExploded);
        }
        None
    }
}

/// Atomic chess as a [`VariantPosition`].
///
/// Captures explode (see the module docs), and the game is won by exploding the
/// enemy king. Movement, castling, promotion, and en-passant generation are
/// inherited from standard chess; only king safety, capture side effects, and
/// the terminal condition differ.
pub type Atomic = VariantPosition<AtomicRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variant::VariantId;
    use crate::{Color, Outcome, Role};

    fn play_line(mut pos: Atomic, ucis: &[&str]) -> Atomic {
        for uci in ucis {
            let mv = pos.parse_uci(uci).expect("legal uci move");
            pos = pos.play(&mv);
        }
        pos
    }

    #[test]
    fn startpos_matches_standard_movegen() {
        let pos = Atomic::startpos();
        assert_eq!(pos.variant_id(), VariantId::Atomic);
        assert_eq!(pos.legal_moves().len(), 20);
        assert!(pos.outcome().is_none());
    }

    fn sq(s: &str) -> Square {
        s.parse().unwrap()
    }

    #[test]
    fn capture_explodes_adjacent_non_pawns() {
        // The white queen on d1 captures the black rook on d8. The blast centre
        // d8 removes the capturing queen, the captured rook, and every adjacent
        // non-pawn (the king on c8) while sparing the adjacent pawn on e7. The
        // knight on b8 is not adjacent to d8 and survives.
        let pos: Atomic = "1nkr4/4p3/8/8/8/8/8/3QK3 w - - 0 1".parse().unwrap();
        let after = play_line(pos, &["d1d8"]);
        let b = after.core().board();
        assert!(b.piece_at(sq("d8")).is_none(), "captured rook gone");
        assert!(b.piece_at(sq("d1")).is_none(), "capturing queen gone");
        assert!(
            b.piece_at(sq("c8")).is_none(),
            "adjacent black king destroyed"
        );
        assert_eq!(
            b.piece_at(sq("e7")).map(|p| p.role),
            Some(Role::Pawn),
            "pawn adjacent to the blast survives"
        );
        assert_eq!(
            b.piece_at(sq("b8")).map(|p| p.role),
            Some(Role::Knight),
            "non-adjacent knight survives"
        );
    }

    #[test]
    fn pawn_survives_blast_but_dies_if_captured() {
        // exd5: both the captured pawn (blast centre d5) and the capturing pawn
        // vanish. A pawn merely adjacent to the centre would survive.
        let pos: Atomic = "4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1".parse().unwrap();
        let after = play_line(pos, &["e4d5"]);
        let b = after.core().board();
        assert!(b.piece_at(sq("d5")).is_none(), "captured pawn gone");
        assert!(b.piece_at(sq("e4")).is_none(), "capturing pawn gone");
    }

    #[test]
    fn king_caught_in_blast_is_destroyed_and_wins() {
        // The queen on e2 captures the rook on e7; the black king on e8 is
        // adjacent to the e7 blast centre and is destroyed, winning for White.
        let pos: Atomic = "4k3/4r3/8/8/8/8/4Q3/4K3 w - - 0 1".parse().unwrap();
        let after = play_line(pos, &["e2e7"]);
        assert!(after.core().board().king_of(Color::Black).is_none());
        assert_eq!(
            after.outcome(),
            Some(Outcome::Decisive {
                winner: Color::White
            })
        );
        assert_eq!(after.end_reason(), Some(EndReason::KingExploded));
    }

    #[test]
    fn cannot_capture_to_explode_own_king() {
        // The white king on e2 is adjacent to d2; Qxd2 would explode d2 and catch
        // the white king, so the capture is rejected.
        let pos: Atomic = "4k3/8/8/8/8/8/3rK3/3Q4 w - - 0 1".parse().unwrap();
        assert!(
            pos.parse_uci("d1d2").is_err(),
            "capture that explodes own king must be rejected"
        );
        assert!(!pos
            .legal_moves()
            .iter()
            .any(|m| m.from() == sq("d1") && m.to() == sq("d2")));
    }

    #[test]
    fn king_cannot_capture() {
        // The white king on e1 may not capture the rook on e2: capturing
        // detonates e2 and removes the king itself.
        let pos: Atomic = "4k3/8/8/8/8/8/4r3/4K3 w - - 0 1".parse().unwrap();
        assert!(
            !pos.legal_moves()
                .iter()
                .any(|m| m.from() == sq("e1") && m.to() == sq("e2")),
            "king may not capture (would explode itself)"
        );
    }

    #[test]
    fn exploding_enemy_king_legal_while_in_check() {
        // The white king on e1 is in check from the rook on e2, yet Qg1xg7
        // explodes the black king on g8 (adjacent to g7) and wins outright --
        // legal despite the check.
        let pos: Atomic = "6k1/6q1/8/8/8/8/4r3/4K1Q1 w - - 0 1".parse().unwrap();
        assert!(pos.is_check(), "white king is in check from the e2 rook");
        let mv = pos
            .parse_uci("g1g7")
            .expect("explode-king capture is legal even in check");
        let after = pos.play(&mv);
        assert!(after.core().board().king_of(Color::Black).is_none());
        assert_eq!(
            after.outcome(),
            Some(Outcome::Decisive {
                winner: Color::White
            })
        );
        assert_eq!(after.end_reason(), Some(EndReason::KingExploded));
    }

    #[test]
    fn en_passant_blast_centre_is_destination_square() {
        // exd6 en passant. The captured pawn on d5 is removed, the capturing pawn
        // that lands on d6 is removed, and the blast is centred on the *landing*
        // square d6: a knight on c7 (adjacent to d6) is destroyed, while a knight
        // on c4 (adjacent only to the captured pawn on d5) survives.
        let pos: Atomic = "4k3/2n5/8/3pP3/2n5/8/8/4K3 w - d6 0 1".parse().unwrap();
        let after = play_line(pos, &["e5d6"]);
        let b = after.core().board();
        assert!(b.piece_at(sq("d5")).is_none(), "captured pawn gone");
        assert!(b.piece_at(sq("d6")).is_none(), "capturing pawn gone");
        assert!(
            b.piece_at(sq("c7")).is_none(),
            "non-pawn adjacent to the d6 destination is destroyed"
        );
        assert_eq!(
            b.piece_at(sq("c4")).map(|p| p.role),
            Some(Role::Knight),
            "piece adjacent only to the captured pawn (d5) survives"
        );
    }

    #[test]
    fn fen_round_trips() {
        for fen in [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "4k3/8/8/8/8/8/8/4K3 w - - 0 1",
        ] {
            let pos: Atomic = fen.parse().unwrap();
            assert_eq!(pos.to_fen(), fen);
        }
    }
}
