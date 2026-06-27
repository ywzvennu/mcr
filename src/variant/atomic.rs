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

/// Whether the side to move in `core` is in check under the atomic king-safety
/// rule.
///
/// This mirrors the move-generation immunity of [`king_is_safe`]: a king
/// standing **adjacent to the enemy king** can never be captured (any enemy
/// attacker would detonate a blast that also catches its own king), so it is
/// never in check there — regardless of how many pieces ordinarily "attack" the
/// square. Otherwise the standard core check test applies.
///
/// With both kings present this is precisely `!king_is_safe(core, my_king,
/// enemy, enemy_king)` evaluated on the current position; it is factored out so
/// the result reads directly as "in check". A side with no king is treated as
/// not in check (the game is already decided by [`AtomicRules::extra_terminal`]).
fn atomic_is_check(core: &Position) -> bool {
    let mover = core.turn();
    let Some(my_king) = core.board().king_of(mover) else {
        // No king of the side to move: the game is over by king-explosion, not
        // by check. Report "not in check" so terminal labeling defers to
        // `extra_terminal` (KingExploded) rather than the checkmate path.
        return false;
    };
    let opponent = mover.opposite();
    match core.board().king_of(opponent) {
        // Adjacent to the enemy king: immune to every attacker, never in check.
        Some(enemy_king) => !king_is_safe(core, my_king, opponent, enemy_king),
        // The enemy king is gone: there is no attacker to give check (and the
        // mover has already won); not in check.
        None => false,
    }
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

    /// H3b: atomic check detection with king-adjacency immunity.
    ///
    /// Delegates to [`atomic_is_check`], which excludes the enemy king as an
    /// attacker and treats a king adjacent to the enemy king as never in check
    /// (it cannot be captured without the capturer exploding the adjacent enemy
    /// king). This is the same immunity the move generator applies via #121, so
    /// the checkmate-vs-stalemate split in [`VariantPosition::end_reason`] agrees
    /// with the legal-move set: a position with adjacent kings and no legal move
    /// is a stalemate draw, not a false checkmate.
    fn is_check(core: &Position, _state: &Self::State) -> bool {
        atomic_is_check(core)
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

    /// Regression for #130: when an explosion removes a castling rook, the
    /// incrementally-maintained Zobrist key carried through `play` must match a
    /// from-scratch hash of the reached position, byte for byte. The explosion
    /// revokes a castling right, and the from-scratch path folds that revoked
    /// right out of the castling-key contribution, so the incremental path must
    /// fold the same delta when the rook is removed by the blast.
    fn assert_incremental_hash_matches_from_scratch(parent_fen: &str, uci: &str) {
        let parent: Atomic = parent_fen.parse().expect("legal parent fen");
        let mv = parent.parse_uci(uci).expect("legal uci move");

        // Compare the *stored* incremental key (the `hash` field carried through
        // the move) against a from-scratch recomputation of the reached board.
        // The public `zobrist()` recomputes from scratch and so would mask the
        // bug entirely; the divergence lives only in the stored field, so the
        // test must read it directly via `incremental_zobrist`.
        let reached = parent.play(&mv);
        let reached_fen = reached.to_fen();
        assert_eq!(
            reached.core().incremental_zobrist(),
            reached.core().compute_zobrist(),
            "play stored hash diverges from from-scratch for {parent_fen} {uci} \
             (reached {reached_fen})",
        );

        // `play_unchecked` shares the same incremental path; confirm it too.
        let mut unchecked = parent.clone();
        unchecked.play_unchecked(&mv);
        assert_eq!(
            unchecked.core().incremental_zobrist(),
            unchecked.core().compute_zobrist(),
            "play_unchecked stored hash diverges from from-scratch for \
             {parent_fen} {uci} (reached {reached_fen})",
        );
    }

    #[test]
    fn explosion_removing_castling_rook_keeps_incremental_hash() {
        // The reported repro: Qxh7 blasts the h8 rook, revoking Black's king-side
        // right; the incremental key must still match a fresh hash of the result.
        assert_incremental_hash_matches_from_scratch(
            "rnb1k1nr/pp2bp1p/2pp4/2P2Qp1/1P3PPP/N6N/P3P3/R1B1KB2 w Qkq - 0 1",
            "f5h7",
        );

        // Enemy queen-side rook removed by a capture centred on a8: the white
        // queen captures the knight on b8, and the adjacent a8 rook is blasted,
        // revoking Black's queen-side right.
        assert_incremental_hash_matches_from_scratch("rn2k3/8/1Q6/8/8/8/8/4K3 w q - 0 1", "b6b8");

        // Own (White) queen-side rook caught in the blast: Black queen captures
        // the knight on b1, and the adjacent a1 rook is removed, revoking White's
        // Q right.
        assert_incremental_hash_matches_from_scratch("4k3/8/8/8/8/8/1q6/RN2K3 b Q - 0 1", "b2b1");

        // Enemy king-side rook removed by a capture centred on h8: the white
        // queen captures the knight on g8, and the adjacent h8 rook is blasted,
        // revoking Black's king-side right. (A single explosion cannot revoke a
        // White and a Black castling right at once: the two sides' rooks sit on
        // different back ranks and can never share one 3x3 blast, so own- and
        // enemy-side revocations are exercised by the separate cases above.)
        assert_incremental_hash_matches_from_scratch("4k1nr/6Q1/8/8/8/8/8/4K3 w k - 0 1", "g7g8");

        // The exploding (capturing) piece lands itself adjacent to a rook on
        // its home square: a black knight captures the queen on g1, and the
        // blast removes the adjacent h1 rook, revoking White's king-side right.
        assert_incremental_hash_matches_from_scratch("8/8/4k3/8/8/5n2/8/2K3QR b K - 0 1", "f3g1");
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

    /// Regression for #131: an atomic position with the side-to-move's king
    /// adjacent to the enemy king, attacked by an enemy rook, and with **zero**
    /// legal moves must be labelled a **stalemate draw**, not a false checkmate.
    ///
    /// The enemy rook "attacks" the black king, so the shared core check test
    /// (`Position::is_check`) reports check -- which, with no legal move, the old
    /// labeling turned into `Checkmate`. But in atomic the black king is immune
    /// while adjacent to the white king (any capturer would detonate the white
    /// king too -- the #121 movegen immunity), so it is *not* in check, and zero
    /// legal moves is a stalemate. The mapping is: adjacent-king "check" with no
    /// legal move -> `EndReason::Stalemate` -> `Outcome::Draw`.
    ///
    /// The adjacent-kings position is unparseable as a FEN, so it is built by
    /// play. Final position (black to move): black Ka8/Pa7, white Kb7/Rb8/Ra1/Pa6
    /// -- `kR6/pK6/P7/8/8/8/8/R7 b - - 5 3`.
    #[test]
    fn adjacent_kings_zero_moves_is_stalemate_not_checkmate_issue_131() {
        let start: Atomic = "k7/p1K5/P7/8/8/8/8/R6R w - - 0 1".parse().unwrap();
        let pos = play_line(start, &["c7b7", "a8b8", "h1h8", "b8a8", "h8b8"]);

        // Both kings stand adjacent, black to move, with a rook bearing on a8.
        let core = pos.core();
        assert_eq!(pos.turn(), Color::Black);
        assert_eq!(core.board().king_of(Color::Black), Some(sq("a8")));
        assert_eq!(core.board().king_of(Color::White), Some(sq("b7")));
        assert!(
            king_attacks(sq("a8")).contains(sq("b7")),
            "the two kings are adjacent"
        );

        // The shared core test sees a rook attacking a8 and reports check...
        assert!(
            core.is_check(),
            "core check test (no atomic immunity) reports check"
        );
        // ...but atomic's variant-aware test applies the adjacency immunity: the
        // black king cannot be captured here, so it is NOT in check.
        assert!(
            !pos.is_check(),
            "atomic king adjacent to the enemy king is immune -> not in check"
        );

        // With zero legal moves and not in check, the position is a stalemate
        // draw, not a checkmate.
        assert!(pos.legal_moves().is_empty(), "black has no legal move");
        assert_eq!(
            pos.end_reason(),
            Some(EndReason::Stalemate),
            "no-move adjacent-kings position is stalemate, not a false checkmate"
        );
        assert_eq!(pos.outcome(), Some(Outcome::Draw));
    }

    /// A genuine atomic checkmate -- the mated king is *not* adjacent to the
    /// enemy king, so no immunity applies -- must still label `Checkmate`. This
    /// is the control that keeps the #131 fix from over-relaxing the labeling.
    #[test]
    fn ordinary_atomic_checkmate_still_labels_checkmate_issue_131() {
        // Back-rank mate: black Kg8 boxed by its own g7/h7 pawns. White Re8
        // checks along the 8th rank; white Rf7 covers the f7/f8 flight squares.
        // The white king is on a1, nowhere near g8, so the black king has no
        // adjacency immunity -- this is a real checkmate.
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

    /// The atomic king-adjacency immunity must also make `is_check()` report
    /// `false` for an adjacent-kings position that still has legal moves, even
    /// though the core test reports check. (Pairs with the movegen-side
    /// `king_already_adjacent_to_enemy_king_keeps_full_move_set` test, whose doc
    /// noted `is_check()` was previously left to this separate change.)
    #[test]
    fn is_check_false_when_king_adjacent_to_enemy_king_issue_131() {
        // White king on a2 adjacent to black king a3, "attacked" by a rook on the
        // 2nd rank -- built by play, since adjacent kings are unparseable.
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
        // The game is not over: the king still has its normal moves.
        assert!(pos.outcome().is_none());
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
