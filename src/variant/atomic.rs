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

use super::heapless_removals::Removals;
use super::{Variant, VariantId, VariantPosition};
use crate::attacks::king_attacks;
use crate::movelist::MoveList;
use crate::{Color, EndReason, Move, MoveKind, Piece, Position, Role, Square};
#[cfg(test)]
use alloc::{string::String, vec::Vec};

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
fn detonate(core: &mut Position, mv: &Move, removed: &mut Removals) {
    let center = mv.to();

    // The capturing piece is destroyed at its destination (the blast centre).
    if let Some(piece) = core.remove_piece_tracked(center) {
        removed.push(piece, center);
    }

    // Every adjacent non-pawn piece is destroyed; pawns survive the blast.
    for sq in king_attacks(center) {
        if let Some(piece) = core.board().piece_at(sq) {
            if piece.role != Role::Pawn {
                core.remove_piece_tracked(sq);
                removed.push(piece, sq);
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
    detonate(&mut after, mv, &mut Removals::new());

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

    // Both kings stand: ordinary king safety, but the mover's king is immune to
    // capture whenever it stands adjacent to the enemy king.
    king_is_safe(&after, my_king, opponent, enemy_king)
}

/// Whether the mover's king on `my_king` is safe in atomic chess, given the
/// enemy king stands on `enemy_king`.
///
/// In atomic a king can never be captured while it stands **adjacent to the
/// enemy king**: any enemy piece capturing it there detonates a blast that also
/// catches the enemy's own king, and a move that destroys the mover's own king
/// is illegal — so the capture can never be executed, and the square is immune
/// to every enemy attacker (sliders, knights, pawns, and the enemy king alike).
///
/// Otherwise ordinary king safety applies, with the single twist that the enemy
/// king itself never gives an executable check (it would explode itself), so it
/// is excluded from the attacker set.
fn king_is_safe(after: &Position, my_king: Square, opponent: Color, enemy_king: Square) -> bool {
    // Adjacent to the enemy king: immune to every attacker.
    if king_attacks(my_king).contains(enemy_king) {
        return true;
    }
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
        removed: &mut Removals,
    ) {
        // Record the blasted pieces so make/unmake can restore them; the forward
        // `play` path passes a throwaway buffer and ignores them.
        detonate(core, mv, removed);
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
            detonate(&mut c, mv, &mut Removals::new());
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

        // Both kings stand: ordinary king safety applies to the mover, with the
        // atomic twist that the mover's king is immune to capture whenever it
        // stands adjacent to the enemy king.
        king_is_safe(after, my_king, opponent, enemy_king)
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

    /// H3: whether the side to move is in check, under atomic king safety.
    ///
    /// The side-to-move's king is in check iff it is *not* safe by the atomic
    /// rule [`king_is_safe`]: the enemy king is excluded as an attacker (it would
    /// explode itself), and a king adjacent to the enemy king is immune. The
    /// standard [`Position::is_check`] counts both the neighbouring enemy king
    /// and attacks that exist only because the kings stand adjacent, so it would
    /// over-report check in exactly the positions atomic reaches with the kings
    /// side by side.
    fn is_check(core: &Position) -> bool {
        let us = core.turn();
        let Some(my_king) = core.board().king_of(us) else {
            // No king of the side to move (it was exploded): not "in check" —
            // the game is already decided by the missing king.
            return false;
        };
        match core.board().king_of(us.opposite()) {
            Some(enemy_king) => !king_is_safe(core, my_king, us.opposite(), enemy_king),
            // Enemy king gone: no attacker can give atomic check (the game is
            // already won), so the side to move is not in check.
            None => false,
        }
    }

    /// Atomic FEN validation of the side *not* to move's king safety.
    ///
    /// Reuses the atomic king-safety rule [`king_is_safe`]: the enemy king (here
    /// the side *to* move) gives no executable check, and a king adjacent to it
    /// is immune. Without this override the standard `is_attacked` would reject
    /// legal atomic positions in which the two kings stand adjacent — positions
    /// atomic reaches in normal play and serializes via `to_fen`, then could not
    /// re-parse. With the side not to move's king missing the caller skips this
    /// hook entirely, so `their_king` is always present here.
    fn opposite_king_in_check_for_fen(core: &Position, their_king: Square, them: Color) -> bool {
        // The side to move is `them.opposite()`; from its perspective `them`'s
        // king is the "enemy king" excluded from the attacker set, and `them`'s
        // king is "my king" whose safety is being judged.
        match core.board().king_of(them.opposite()) {
            // Both kings present: atomic king safety, enemy king excluded.
            Some(my_king) => !king_is_safe(core, their_king, them.opposite(), my_king),
            // The side to move has no king (e.g. it was just exploded); fall back
            // to the plain attack test against the remaining king.
            None => core.is_attacked(their_king, them.opposite()),
        }
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

    /// Regression for #130: when an explosion removes a castling rook, the move
    /// must actually revoke the matching castling right, so the reached position's
    /// Zobrist key (recomputed from scratch) reflects the lost right. We assert it
    /// by comparing the key of the position reached via `play` against the key of
    /// the same position parsed back from its own FEN: if the explosion failed to
    /// revoke the right, the played position and its FEN re-parse would disagree on
    /// the castling-key contribution and the two keys would differ.
    fn assert_explosion_revokes_castling(parent_fen: &str, uci: &str) {
        let parent: Atomic = parent_fen.parse().expect("legal parent fen");
        let mv = parent.parse_uci(uci).expect("legal uci move");

        let reached = parent.play(&mv);
        let reached_fen = reached.to_fen();
        let reparsed: Atomic = reached_fen.parse().expect("reached fen re-parses");
        assert_eq!(
            reached.zobrist(),
            reparsed.zobrist(),
            "play key diverges from a FEN re-parse for {parent_fen} {uci} \
             (reached {reached_fen}) — explosion did not revoke the castling right",
        );

        // `play_unchecked` shares the same edit path; confirm it agrees with `play`.
        let mut unchecked = parent.clone();
        unchecked.play_unchecked(&mv);
        assert_eq!(
            unchecked.zobrist(),
            reached.zobrist(),
            "play_unchecked key diverges from play for {parent_fen} {uci} \
             (reached {reached_fen})",
        );
    }

    #[test]
    fn explosion_removing_castling_rook_revokes_right() {
        // The reported repro: Qxh7 blasts the h8 rook, revoking Black's king-side
        // right; the incremental key must still match a fresh hash of the result.
        assert_explosion_revokes_castling(
            "rnb1k1nr/pp2bp1p/2pp4/2P2Qp1/1P3PPP/N6N/P3P3/R1B1KB2 w Qkq - 0 1",
            "f5h7",
        );

        // Enemy queen-side rook removed by a capture centred on a8: the white
        // queen captures the knight on b8, and the adjacent a8 rook is blasted,
        // revoking Black's queen-side right.
        assert_explosion_revokes_castling("rn2k3/8/1Q6/8/8/8/8/4K3 w q - 0 1", "b6b8");

        // Own (White) queen-side rook caught in the blast: Black queen captures
        // the knight on b1, and the adjacent a1 rook is removed, revoking White's
        // Q right.
        assert_explosion_revokes_castling("4k3/8/8/8/8/8/1q6/RN2K3 b Q - 0 1", "b2b1");

        // Enemy king-side rook removed by a capture centred on h8: the white
        // queen captures the knight on g8, and the adjacent h8 rook is blasted,
        // revoking Black's king-side right. (A single explosion cannot revoke a
        // White and a Black castling right at once: the two sides' rooks sit on
        // different back ranks and can never share one 3x3 blast, so own- and
        // enemy-side revocations are exercised by the separate cases above.)
        assert_explosion_revokes_castling("4k1nr/6Q1/8/8/8/8/8/4K3 w k - 0 1", "g7g8");

        // The exploding (capturing) piece lands itself adjacent to a rook on
        // its home square: a black knight captures the queen on g1, and the
        // blast removes the adjacent h1 rook, revoking White's king-side right.
        assert_explosion_revokes_castling("8/8/4k3/8/8/5n2/8/2K3QR b K - 0 1", "f3g1");
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

    /// The legal UCI strings of `pos`, sorted, for set comparisons.
    fn legal_ucis(pos: &Atomic) -> Vec<String> {
        let mut ucis: Vec<String> = pos.legal_moves().iter().map(|m| pos.to_uci(m)).collect();
        ucis.sort();
        ucis
    }

    #[test]
    fn king_may_move_adjacent_to_enemy_king_issue_121() {
        // Issue #121 repro found by the differential fuzzer (#109). The white king
        // on a2 may step to b3, adjacent to the black king on c4: the bishop on d1
        // "attacks" b3, but it could never capture the king there without the blast
        // also catching the adjacent black king, so b3 is immune. mce previously
        // omitted a2b3.
        let pos: Atomic = "8/8/8/3p4/2k2pp1/8/K7/3b4 w - - 0 1".parse().unwrap();
        assert!(
            pos.legal_moves()
                .iter()
                .any(|m| m.from() == sq("a2") && m.to() == sq("b3")),
            "a2b3 (king step adjacent to the enemy king) must be legal"
        );
    }

    #[test]
    fn king_step_adjacent_to_enemy_king_immune_to_every_attacker() {
        // The destination b3 is adjacent to the black king on c4 and is therefore
        // immune to every kind of attacker. A king step onto it stays legal whether
        // the attacker is a slider, a knight, or a pawn.
        for attacker_fen in [
            "8/8/8/8/2k5/8/K7/3b4 w - - 0 1", // bishop d1 hits b3
            "8/8/8/8/2k5/8/K7/1r6 w - - 0 1", // rook b1 hits b3
            "8/8/8/8/2k5/8/K7/3n4 w - - 0 1", // knight d1 hits b3
            "8/8/8/8/p1k5/8/K7/8 w - - 0 1",  // pawn a4 hits b3
        ] {
            let pos: Atomic = attacker_fen.parse().unwrap();
            assert!(
                pos.legal_moves()
                    .iter()
                    .any(|m| m.from() == sq("a2") && m.to() == sq("b3")),
                "a2b3 must be legal (immune via enemy-king adjacency) in {attacker_fen}"
            );
        }
    }

    #[test]
    fn king_step_to_attacked_square_not_adjacent_to_enemy_king_is_illegal() {
        // Same bishop on d1 hitting b3, but the enemy king is far away, so b3 is
        // NOT immune: the king step a2b3 must be rejected. This is the control case
        // that keeps the fix from over-allowing.
        let pos: Atomic = "7k/8/8/8/8/8/K7/3b4 w - - 0 1".parse().unwrap();
        assert!(
            !pos.legal_moves()
                .iter()
                .any(|m| m.from() == sq("a2") && m.to() == sq("b3")),
            "a2b3 must be illegal when b3 is attacked and not adjacent to the enemy king"
        );
    }

    #[test]
    fn king_already_adjacent_to_enemy_king_keeps_full_move_set() {
        // Reach a position where the white king on a2 already stands adjacent to
        // the black king on a3 and is "attacked" by a rook on the 2nd rank. (Two
        // kings can never be placed adjacent in a parseable FEN, so the position is
        // built by play.) In atomic this is not check — the rook could not capture
        // without exploding the adjacent black king — so the king keeps its normal
        // moves and no piece is treated as pinned.
        // The legal-move set proves the generator does not treat the king as in
        // check: a rook "attacks" a2, yet the king keeps its full quiet-move set
        // (it is not forced into check-evasion only). (Whether `is_check()` itself
        // should report `false` here is a separate concern — it routes through the
        // shared core check test — and is intentionally left to its own change.)
        let start: Atomic = "8/8/8/8/8/k7/7r/K7 w - - 0 1".parse().unwrap();
        let pos = play_line(start, &["a1a2", "h2g2"]);
        assert_eq!(legal_ucis(&pos), ["a2a1", "a2b1", "a2b2", "a2b3"]);
    }

    #[test]
    fn no_pin_while_king_adjacent_to_enemy_king() {
        // Built by play (adjacent kings are unparseable): the white king on a2
        // stands adjacent to the black king on a3, and a black rook on the 2nd rank
        // would "pin" the white rook on c2 under ordinary rules. But the white king
        // is immune, so the rook is not pinned and may leave the rank.
        let start: Atomic = "8/8/8/8/8/k7/2R4r/K7 w - - 0 1".parse().unwrap();
        let pos = play_line(start, &["a1a2", "h2g2"]);
        assert!(
            pos.legal_moves()
                .iter()
                .any(|m| m.from() == sq("c2") && m.to() == sq("c3")),
            "the rook is not pinned while our king is immune (adjacent to enemy king)"
        );
    }

    #[test]
    fn fast_and_slow_legal_paths_agree_over_random_atomic_games() {
        // The fast atomic generator (`AtomicRules::legal_into` via
        // `atomic_noncapture_legal_into`) must produce exactly the same legal set as
        // the make-move reference filter (`slow_legal_into` driven by
        // `is_legal_after`) — including for the king-adjacency immunity this fix
        // adds. Walk a deterministic random tree of atomic positions and compare the
        // two sets at every node.
        let mut rng: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };

        for _ in 0..400 {
            let mut pos = Atomic::startpos();
            for _ in 0..40 {
                let core = pos.core();

                let mut fast = MoveList::new();
                AtomicRules::legal_into(core, &mut fast);
                let mut fast_ucis: Vec<String> = fast.iter().map(|m| pos.to_uci(m)).collect();
                fast_ucis.sort();

                let mut slow = MoveList::new();
                AtomicRules::slow_legal_into(core, &mut slow);
                let mut slow_ucis: Vec<String> = slow.iter().map(|m| pos.to_uci(m)).collect();
                slow_ucis.sort();

                assert_eq!(
                    fast_ucis,
                    slow_ucis,
                    "fast/slow legal-move divergence at {}",
                    pos.to_fen()
                );

                let moves = pos.legal_moves();
                if moves.is_empty() {
                    break;
                }
                let mv = moves[(next() as usize) % moves.len()];
                pos = pos.play(&mv);
            }
        }
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

    /// Regression for #134: atomic legitimately reaches positions with the two
    /// kings adjacent (a king may step beside the enemy king — capturing it would
    /// explode the capturer's own king). `from_fen` must accept such a position
    /// and round-trip it; the standard validator wrongly rejected it because a
    /// king "attacks" its neighbouring square, so atomic generated legal FENs it
    /// could not re-parse.
    #[test]
    fn adjacent_kings_fen_round_trips_issue_134() {
        // Real FENs from the difftest --all atomic divergences (#134): the two
        // kings stand on adjacent squares with the side not to move's king beside
        // the side-to-move's king.
        for fen in [
            "4k3/3K4/1p3r2/b1ppp3/P1PP3p/N6P/5QPR/R4B2 b - - 1 42",
            "8/8/4P3/6pP/2k5/p2K1p2/P3RP2/8 w - - 4 49",
            "1nb2r2/3p4/6Pp/P1N5/5p1P/2pK4/2Pk4/R7 w - - 3 46",
            // Minimal hand-built adjacent-kings position.
            "8/8/8/8/8/8/3k4/3K4 w - - 0 1",
        ] {
            let pos: Atomic = fen.parse().unwrap_or_else(|e| {
                panic!("atomic from_fen rejected legal adjacent-kings FEN {fen}: {e}")
            });
            let out = pos.to_fen();
            assert_eq!(out, fen, "atomic FEN did not round-trip");
            // The re-parsed position must equal the original.
            let reparsed: Atomic = out.parse().expect("re-parse of to_fen output");
            assert_eq!(reparsed, pos, "atomic FEN round-trip not equal");
        }
    }

    /// Regression for #134: with the two kings adjacent, the side to move is NOT
    /// in check under atomic rules (its king is immune beside the enemy king),
    /// even when an enemy piece otherwise bears on the king square. The standard
    /// `is_check` over-reported check here, diverging from shakmaty.
    #[test]
    fn adjacent_kings_side_to_move_not_in_check_issue_134() {
        // White king d3 adjacent to black king d2 (this is one of the #134 FENs).
        let pos: Atomic = "1nb2r2/3p4/6Pp/P1N5/5p1P/2pK4/2Pk4/R7 w - - 3 46"
            .parse()
            .expect("legal adjacent-kings atomic FEN");
        assert!(
            !pos.is_check(),
            "atomic: side to move adjacent to enemy king must not be in check"
        );
    }

    /// Standard chess must still reject two kings on adjacent squares: such a
    /// position is unreachable (the side not to move would be giving/standing in
    /// an illegal king-on-king position), so `Chess::from_fen` errors. This pins
    /// down that the #134 relaxation is atomic-only.
    #[test]
    fn standard_rejects_adjacent_kings() {
        use crate::variant::Chess;
        let result: Result<Chess, _> = "8/8/8/8/8/8/3k4/3K4 w - - 0 1".parse();
        assert!(
            result.is_err(),
            "standard chess must reject adjacent kings, but from_fen accepted it"
        );
    }

    /// Adjacent kings with zero legal moves is a stalemate draw, not a false
    /// checkmate: the core check test reports check (a rook bears on the king),
    /// but atomic's adjacency immunity means the king is not actually in check.
    #[test]
    fn adjacent_kings_zero_moves_is_stalemate_not_checkmate_issue_131() {
        let start: Atomic = "k7/p1K5/P7/8/8/8/8/R6R w - - 0 1".parse().unwrap();
        let pos = play_line(start, &["c7b7", "a8b8", "h1h8", "b8a8", "h8b8"]);

        let core = pos.core();
        assert_eq!(pos.turn(), Color::Black);
        assert_eq!(core.board().king_of(Color::Black), Some(sq("a8")));
        assert_eq!(core.board().king_of(Color::White), Some(sq("b7")));
        assert!(
            king_attacks(sq("a8")).contains(sq("b7")),
            "the two kings are adjacent"
        );
        assert!(
            core.is_check(),
            "core check test (no atomic immunity) reports check"
        );
        assert!(
            !pos.is_check(),
            "atomic king adjacent to the enemy king is immune -> not in check"
        );
        assert!(pos.legal_moves().is_empty(), "black has no legal move");
        assert_eq!(
            pos.end_reason(),
            Some(EndReason::Stalemate),
            "no-move adjacent-kings position is stalemate, not a false checkmate"
        );
        assert_eq!(pos.outcome(), Some(Outcome::Draw));
    }

    /// A genuine atomic checkmate (mated king not adjacent to the enemy king, so
    /// no immunity) must still label `Checkmate` — the control against
    /// over-relaxing the labeling.
    #[test]
    fn ordinary_atomic_checkmate_still_labels_checkmate_issue_131() {
        let pos: Atomic = "4R1k1/5Rpp/8/8/8/8/8/K7 b - - 0 1".parse().unwrap();
        assert!(pos.is_check(), "black king is genuinely in check");
        assert!(pos.legal_moves().is_empty(), "black has no legal move");
        assert_eq!(pos.end_reason(), Some(EndReason::Checkmate));
        assert_eq!(
            pos.outcome(),
            Some(Outcome::Decisive {
                winner: Color::White
            })
        );
    }

    /// The adjacency immunity must also make `is_check()` report `false` for an
    /// adjacent-kings position that still has legal moves, even though the core
    /// test reports check.
    #[test]
    fn is_check_false_when_king_adjacent_to_enemy_king_issue_131() {
        let start: Atomic = "8/8/8/8/8/k7/7r/K7 w - - 0 1".parse().unwrap();
        let pos = play_line(start, &["a1a2", "h2g2"]);
        assert!(
            pos.core().is_check(),
            "core test reports check from the g2 rook"
        );
        assert!(
            !pos.is_check(),
            "atomic: king adjacent to the enemy king is immune, so not in check"
        );
        assert!(pos.outcome().is_none());
    }
}
