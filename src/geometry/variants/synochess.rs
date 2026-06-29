//! Synochess — a [Pychess] fairy variant on the 8x8 [`Chess8x8`] board pitting a
//! standard Western army (White, the "Kingdom") against a Chinese/Korean-chess
//! amalgamation (Black, the "Dynasty"). Validated node-for-node against
//! Fairy-Stockfish (`UCI_Variant synochess`, from its `variants.ini`).
//!
//! [Pychess]: https://www.pychess.org/variants/synochess
//!
//! ## Armies
//!
//! **White (Kingdom)** is ordinary chess: Pawns (double-step, en passant,
//! promotion to N/B/R/Q), Knight, Bishop, Rook, Queen, King, and castling.
//!
//! **Black (Dynasty)** fields:
//! * **Chariot** — an ordinary Rook ([`WideRole::Rook`]).
//! * **Horse** — an ordinary Knight ([`WideRole::Knight`], an *unobstructed*
//!   leaper, **not** the hobbled Xiangqi [`WideRole::Horse`]).
//! * **Elephant** — a [`WideRole::FersAlfil`]: a leaper to the four adjacent and
//!   the four two-away diagonal squares, jumping over any intervening piece.
//! * **Advisor** — a [`WideRole::Commoner`]: moves and captures exactly like a
//!   king (one step any direction) but is not royal.
//! * **King** — royal, with the campmate / flying-general rules below.
//! * **Cannon** — a Janggi-style [`WideRole::Cannon`]: it needs an intervening
//!   **screen** to move *and* to capture, and it may neither screen over nor
//!   capture another cannon (the shared [`attacks::janggi_cannon_quiet`] /
//!   [`attacks::janggi_cannon_capture`] primitive, exactly as Janggi uses it).
//! * **Soldier** — a [`WideRole::Soldier`] that steps one square **forward or
//!   sideways** (never backward, never diagonal, no promotion). Black additionally
//!   begins with **two Soldiers in hand** (the `[zz]` pocket) and may, instead of
//!   a board move, **drop** one onto any empty square of **rank 5** (its starting
//!   rank). The pocket is **fixed**: captures never replenish it
//!   ([`WideVariant::captures_to_hand`] is `false`).
//!
//! ## Special rules
//!
//! * **Campmate** ([`WideVariant::has_flag_win`]): a king that reaches the
//!   opponent's far rank (White → rank 8, Black → rank 1) **wins** — that node is
//!   terminal (a perft leaf). A king may not step onto its goal rank while the
//!   enemy king already occupies it (the flag is contested); capturing that enemy
//!   king there is the only flag-rank move then allowed.
//! * **King faceoff** ([`WideVariant::has_flying_general`]): the two kings may not
//!   see each other down an open file **or rank** (broader than Xiangqi, which is
//!   file-only).
//! * **Stalemate is a loss** for the stalemated side
//!   ([`WideVariant::stalemate_is_loss`]); this affects only the reported outcome,
//!   not perft.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `variants.ini` (`[synochess:pocketknight]`, `startFen`). mce and FSF
//! agree on the position but spell the Black pieces differently. mce avoids the
//! letters `a e s` (already the Hawk / Rook+Knight Elephant / Silver), so the
//! Elephant is `v` (Fers-Alfil) and the Soldier is `z`; the Commoner "Advisor"
//! lands **past the exhausted single-letter alphabet** (the Orda army claimed the
//! last free letters), so it takes the `*`-prefixed overflow token `*u` — the
//! [`OVERFLOW_PREFIX`](crate::geometry::OVERFLOW_PREFIX) plus the recycled Advisor letter
//! `u`, the case carrying the colour (`*U` white / `*u` black), exactly as the
//! Shogi promoted roles spell themselves under `+`. The `compare-fairy` harness
//! rewrites all of these (`*u → a`, `v → e`, `z → s`) when driving FSF:
//!
//! ```text
//! FSF dialect: rneakenr/8/1c4c1/1ss2ss1/8/8/PPPPPPPP/RNBQKBNR[ss] w KQ - 0 1
//! mce dialect: rnv*ukvnr/8/1c4c1/1zz2zz1/8/8/PPPPPPPP/RNBQKBNR[zz] w KQ - 0 1
//! ```

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Chess8x8, Geometry, RoyalSlider, Square, StandardChess, WideRole,
    WideVariant,
};
use crate::Color;

/// The Black Soldier's starting (and only legal drop) rank, zero-based: rank 5.
const SOLDIER_DROP_RANK: u8 = 4;

/// The Black Elephant (Fers-Alfil) leaps: one and two squares diagonally, jumping
/// over any intervening piece.
const FERS_ALFIL_OFFSETS: [(i8, i8); 8] = [
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
    (2, 2),
    (2, -2),
    (-2, 2),
    (-2, -2),
];

/// The starting placement, mce dialect (see the [module docs](self)).
const SYNOCHESS_START_PLACEMENT: &str = "rnv*ukvnr/8/1c4c1/1zz2zz1/8/8/PPPPPPPP/RNBQKBNR";

/// The Synochess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides only what Synochess changes from the generic engine: the
/// asymmetric Black army (Commoner advisor, FersAlfil elephant, Janggi cannon,
/// forward/sideways Soldier), the fixed two-Soldier reinforcement pocket and its
/// rank-5 drops, the campmate flag win, the broadened (file + rank) flying
/// general, and stalemate-as-loss. White stays standard chess (pawns, castling,
/// promotion, en passant).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SynochessRules;

impl SynochessRules {
    /// The Black Soldier's reachable squares from `sq`: one step forward (toward
    /// rank 1) and one step to either side. White never fields a Soldier, so this
    /// is only ever asked for Black, but it honours `color` for completeness.
    fn soldier_targets(color: Color, sq: Square<Chess8x8>) -> Bitboard<Chess8x8> {
        let forward: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = Bitboard::<Chess8x8>::EMPTY;
        if let Some(dest) = sq.offset(0, forward) {
            bb.set(dest);
        }
        for df in [-1i8, 1] {
            if let Some(dest) = sq.offset(df, 0) {
                bb.set(dest);
            }
        }
        bb
    }
}

impl WideVariant<Chess8x8> for SynochessRules {
    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(SYNOCHESS_START_PLACEMENT)
            .expect("the Synochess starting placement is valid on an 8x8 board");
        // White keeps both castling rights (rooks on files 0 and WIDTH-1); Black
        // has no castling. The fixed two-Soldier pocket rides in `placement`.
        let mut castling = GenericCastling::NONE;
        castling.set(Color::White, 0, Some(Chess8x8::WIDTH - 1));
        castling.set(Color::White, 1, Some(0));
        // Black's fixed two-Soldier reinforcement pocket; White's is empty.
        let mut black_pocket = [0u8; WideRole::COUNT];
        black_pocket[WideRole::Soldier.index()] = 2;
        let placement = GenericPlacement::new([0u8; WideRole::COUNT], black_pocket);
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
            // Black's Advisor is a Commoner: the king's one-step pattern, unconfined.
            WideRole::Commoner => attacks::king_attacks::<Chess8x8>(sq),
            // Black's Elephant: the Fers-Alfil leaper (jumps; no eye-block).
            WideRole::FersAlfil => attacks::leaper_attacks::<Chess8x8>(sq, &FERS_ALFIL_OFFSETS),
            // Black's Soldier: forward / sideways one step.
            WideRole::Soldier => Self::soldier_targets(color, sq),
            // Black's Cannon: the Janggi cannon. The occupancy-only over-screen
            // capture set is a sound fallback for any incidental query; the real
            // screen/target filtering (the screen may not be a cannon, and no
            // cannon is capturable) comes through `role_attacks_board`, which the
            // cannon-verify path uses.
            WideRole::Cannon => {
                attacks::janggi_cannon_capture::<Chess8x8>(sq, occupancy, Bitboard::EMPTY)
            }
            // Everything else (White's whole army, Black's Rook/Knight/King) is
            // standard chess.
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn uses_board_attacks() -> bool {
        true
    }

    fn role_attacks_board(
        role: WideRole,
        _color: Color,
        sq: Square<Chess8x8>,
        board: &Board<Chess8x8>,
    ) -> Option<Bitboard<Chess8x8>> {
        // Only the Cannon needs the whole board (its screen and target may not be a
        // cannon). Every other role returns `None` and falls back to the
        // occupancy-only `role_attacks`.
        if role != WideRole::Cannon {
            return None;
        }
        let occupied = board.occupied();
        let cannons = board.pieces(Color::White, WideRole::Cannon)
            | board.pieces(Color::Black, WideRole::Cannon);
        // The combined move-and-attack set: over-screen captures plus quiet jumps
        // past a screen. The generator's `emit_targets` splits it by enemy
        // occupancy; the king-safety test sees the king (an occupied royal) only in
        // the capture portion.
        let caps = attacks::janggi_cannon_capture::<Chess8x8>(sq, occupied, cannons);
        let quiet = attacks::janggi_cannon_quiet::<Chess8x8>(sq, occupied, cannons);
        Some(caps | quiet)
    }

    fn role_attack_is_leg_asymmetric(role: WideRole) -> bool {
        // These roles cannot be detected by reverse-projecting their pattern from
        // the target square, so `attackers_to` / `king_safe_after` must project
        // each piece's attack set *forward* from its own origin (as the move
        // generator does):
        //
        // * **Soldier** — forward-biased: a simple color-flipped reverse projection
        //   would test the wrong forward direction and miss a soldier guarding the
        //   square in front of it.
        // * **Cannon** — its over-screen capture set lands only on an *occupied*
        //   square and depends on which pieces are cannons (the board hook), so it
        //   must be projected forward from each cannon.
        //
        // The Commoner (king pattern) and FersAlfil (symmetric leaper) are both
        // reverse-projectable, so they are excluded — keeping `attackers_to`
        // minimal and exact.
        matches!(role, WideRole::Soldier | WideRole::Cannon)
    }

    fn role_is_slider(role: WideRole) -> bool {
        // Standard line sliders plus White's army; the Cannon needs a screen (not a
        // pinning slider), and the Commoner / FersAlfil / Soldier are steppers.
        <StandardChess as WideVariant<Chess8x8>>::role_is_slider(role)
    }

    fn royal_slider_kind(role: WideRole) -> Option<RoyalSlider> {
        // The Rook, Bishop, and Queen are the plain standard sliders (the
        // `role_attacks` standard-chess fallback), so the cannon king-safety verify
        // reverse-projects them from the king with its precomputed line masks. The
        // Janggi Cannon (asymmetric), the Soldier, the Commoner, and the FersAlfil
        // are not standard sliders and keep their existing paths.
        match role {
            WideRole::Rook => Some(RoyalSlider::Rook),
            WideRole::Bishop => Some(RoyalSlider::Bishop),
            WideRole::Queen => Some(RoyalSlider::Queen),
            _ => None,
        }
    }

    fn royal_reach_superset(role: WideRole, king: Square<Chess8x8>) -> Option<Bitboard<Chess8x8>> {
        // Supersets of the squares from which the two forward-projected roles could
        // attack the king. The Soldier moves forward / sideways one step, so it
        // attacks from a square adjacent to the king — the king's one-step
        // neighbourhood covers it. The Janggi-style Cannon attacks along an
        // over-screen orthogonal ray (Synochess has no palace), so its sources lie on
        // the king's rank/file.
        match role {
            WideRole::Soldier => Some(attacks::king_attacks::<Chess8x8>(king)),
            WideRole::Cannon => Some(attacks::rook_attacks::<Chess8x8>(king, Bitboard::EMPTY)),
            _ => None,
        }
    }

    fn has_cannons() -> bool {
        // Black fields Janggi cannons, so the engine takes the pseudo-legal +
        // per-move verify king-safety path (a cannon's check and king-danger are
        // screen-dependent). The flying-general and flag rules ride the same verify.
        true
    }

    fn has_flying_general() -> bool {
        true
    }

    fn extra_royal_attack(
        board: &Board<Chess8x8>,
        sq: Square<Chess8x8>,
        by: Color,
        occupied: Bitboard<Chess8x8>,
    ) -> bool {
        // The king faceoff: `by`'s king attacks the enemy royal square `sq` iff they
        // share a file **or a rank** with no piece strictly between them (broader
        // than Xiangqi's file-only flying general).
        let Some(king) = board.king_of(by) else {
            return false;
        };
        if king == sq {
            return false;
        }
        if king.file() != sq.file() && king.rank() != sq.rank() {
            return false;
        }
        (attacks::between::<Chess8x8>(king, sq) & occupied).is_empty()
    }

    fn has_flag_win() -> bool {
        true
    }

    fn stalemate_is_loss() -> bool {
        true
    }

    fn has_castling() -> bool {
        true
    }

    // --- the fixed Soldier reinforcement pocket ---

    fn has_hand() -> bool {
        true
    }

    fn captures_to_hand() -> bool {
        // The pocket is fixed: captures never bank into a hand.
        false
    }

    fn drop_targets(role: WideRole, color: Color, board: &Board<Chess8x8>) -> Bitboard<Chess8x8> {
        // Only Black drops, only the Soldier, only onto an empty square of rank 5.
        if color.is_white() || role != WideRole::Soldier {
            return Bitboard::EMPTY;
        }
        let mut rank5 = Bitboard::<Chess8x8>::EMPTY;
        for file in 0..Chess8x8::WIDTH {
            if let Some(sq) = Square::<Chess8x8>::from_file_rank(file, SOLDIER_DROP_RANK) {
                rank5.set(sq);
            }
        }
        rank5 & !board.occupied()
    }
}

/// Synochess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position with
/// [`Synochess::startpos`](GenericPosition::startpos) or parse a FEN (mce dialect)
/// with [`Synochess::from_fen`](GenericPosition::from_fen). See the [module
/// docs](self) for the armies, the Soldier pocket, and the campmate / king-faceoff
/// rules.
pub type Synochess = GenericPosition<Chess8x8, SynochessRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::position::WideOutcome;

    /// The canonical start FEN round-trips, seeding Black's two-Soldier pocket.
    #[test]
    fn startpos_seeds_pocket_and_round_trips() {
        let pos = Synochess::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnv*ukvnr/8/1c4c1/1zz2zz1/8/8/PPPPPPPP/RNBQKBNR[zz] w KQ - 0 1"
        );
        assert_eq!(pos.hand_count(Color::Black, WideRole::Soldier), 2);
        assert_eq!(pos.hand_count(Color::White, WideRole::Soldier), 0);
    }

    /// Campmate: a Black king on rank 1 has won (the position is terminal and
    /// White, to move, has no reply), and the win is credited to Black.
    #[test]
    fn campmate_black_king_on_rank_one_wins() {
        let pos = Synochess::from_fen("8/8/8/8/8/8/4K3/3k4 w - - 0 1").expect("valid FEN");
        assert!(pos.legal_moves().is_empty(), "won position has no moves");
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::Black
            })
        );
    }

    /// Campmate for White: a White king on rank 8 has won.
    #[test]
    fn campmate_white_king_on_rank_eight_wins() {
        let pos = Synochess::from_fen("4K3/8/8/8/8/8/4k3/8 b - - 0 1").expect("valid FEN");
        assert!(pos.legal_moves().is_empty());
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::White
            })
        );
    }

    /// A king may not step onto its goal rank while the enemy king holds it, but a
    /// king already on the goal rank (reached when it was free) has won.
    #[test]
    fn contested_flag_blocks_entry() {
        // White king on rank 1 (h1) contests Black's flag: Black king d2 cannot
        // enter rank 1 (c1/d1/e1), only the five non-rank-1 steps remain.
        let pos = Synochess::from_fen("8/8/8/8/8/8/3k4/7K b - - 0 1").expect("valid FEN");
        for mv in pos.legal_moves() {
            assert_ne!(
                mv.to::<Chess8x8>().rank(),
                0,
                "no entry onto contested rank 1"
            );
        }
        assert_eq!(pos.legal_moves().len(), 5);
    }

    /// Stalemate is scored as a loss for the side to move.
    #[test]
    fn stalemate_is_a_loss() {
        // Black king a8 stalemated by White queen on b6 and king c7 — no legal
        // move, not in check. (Standard chess would call this a draw.)
        let pos = Synochess::from_fen("k7/2K5/1Q6/8/8/8/8/8 b - - 0 1").expect("valid FEN");
        assert!(pos.legal_moves().is_empty());
        assert!(!pos.is_check());
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::White
            })
        );
    }
}
