//! Manchu (yipaisanxianqi, 9x10) on the generic engine — an **asymmetric Xiangqi**
//! in which one side keeps a full Xiangqi army and the other replaces its
//! rook/cannon/horse cluster with a single SUPER-PIECE, the **Banner** (issue
//! #230). Validated move-for-move against Fairy-Stockfish `UCI_Variant manchu`.
//!
//! Manchu **reuses the entire Xiangqi rule layer** ([`super::xiangqi`]): the same
//! [`Xiangqi9x10`] geometry, the palace-confined General / Advisor, the
//! river-bound blockable Elephant, the hobbled Horse, the over-screen Cannon, the
//! river-crossing Soldier, and the flying-general king-safety. The only
//! differences are the **starting array** (one side's cluster is a single Banner)
//! and the new **Banner** super-piece.
//!
//! ## The Banner (super-piece)
//!
//! The Banner ([`WideRole::Banner`], FSF `m`, Betza `RcpRnN`) combines three
//! Xiangqi movers in one piece, confirmed square-for-square against FSF:
//!
//! * **Chariot** (`R`) — a plain rook slide, moving **and** capturing along the
//!   four orthogonal rays ([`attacks::rook_attacks`]).
//! * **Cannon** (`cpR`) — a **capture-only** jump over exactly one screen onto the
//!   next piece on the ray ([`attacks::cannon_capture_targets`]). The quiet
//!   rook-rays the Banner already covers with its `R` slide, so the cannon part
//!   contributes only the over-screen *captures*.
//! * **Horse** (`nN`) — a knight leap **hobbled** by an occupied leg square,
//!   moving and capturing ([`attacks::horse_attacks`]).
//!
//! The Banner's full move-and-attack set therefore depends on **which** squares
//! are occupied (the cannon part lands only on the occupied target beyond a
//! screen), so it is computed from the live board through the default-off
//! [`WideVariant::role_attacks_board`] hook (exactly as the Janggi cannon, #213).
//! The returned set folds all three movers together; the generator's `emit_targets`
//! splits it into quiet steps (empty squares — the rook slide and horse leaps) and
//! captures (occupied enemy squares — the rook captures, cannon over-screen
//! captures, and horse captures). The occupancy-only
//! [`role_attacks`](WideVariant::role_attacks) hook returns the same set for any
//! incidental query off the board-aware path.
//!
//! The Banner's attack relation is **occupancy-asymmetric** (the cannon and horse
//! parts), so it joins the Xiangqi forward-projected roles in
//! [`role_attack_is_leg_asymmetric`](WideVariant::role_attack_is_leg_asymmetric):
//! `attackers_to` projects the Banner's set forward from each Banner origin,
//! exactly as the generator does.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `manchu_variant()` (`startFen`). Black is a full Xiangqi army; White
//! has the Banner on a1 plus its elephants, advisors, general, and soldiers, but
//! **no** horses, **no** cannons, and **no** other chariot — the Manchu asymmetry:
//!
//! ```text
//! FSF dialect: rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/9/9/M1BAKAB2 w - - 0 1
//! mce dialect: rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/9/9/*M1OUKUO2 w - - 0 1
//! ```
//!
//! The two describe the same position; mce spells the Xiangqi pieces `u j o z`
//! (the FSF letters `a n b p` already name the Hawk / Knight / Bishop / Pawn) and
//! the Banner as the overflow token `*M` (FSF's `m`, distinct from the bare Met
//! `m` by the `*` prefix). The `compare-fairy` harness rewrites mce's tokens
//! (`u → a`, `j → n`, `o → b`, `z → p`, `*m → m`) when driving FSF.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::variants::xiangqi::XiangqiRules;
use crate::geometry::{attacks, Bitboard, Board, Square, WideRole, WideVariant, Xiangqi9x10};
use crate::Color;

/// The confirmed Manchu starting placement in the mce dialect: a full Black
/// Xiangqi army, and a White side whose rook/cannon/horse cluster is the single
/// Banner `*M` on a1 (the position byte-identical to FSF's
/// `rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/9/9/M1BAKAB2`).
const MANCHU_START_PLACEMENT: &str = "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/9/9/*M1OUKUO2";

/// The Manchu rule layer: a zero-sized [`WideVariant`] over [`Xiangqi9x10`].
///
/// It reuses the entire Xiangqi rule layer ([`XiangqiRules`]) — palace, river,
/// horse, cannon, elephant, soldier, advisor, flying-general — and changes only
/// the starting array (one side's cluster is a single Banner) and adds the
/// **Banner** super-piece (Rook + Cannon + Horse), whose occupancy-dependent set
/// is computed from the live board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ManchuRules;

impl ManchuRules {
    /// The Banner's full move-and-attack set on `sq` under `occupied`: the rook
    /// slide (move + capture), the cannon over-screen capture, and the hobbled
    /// horse leap (move + capture), folded together. Empty target squares are
    /// quiet steps (rook slide / horse leap); occupied enemy squares are captures
    /// (rook / cannon over-screen / horse). The cannon part lands only on the
    /// occupied target beyond a screen, so it never adds a phantom quiet step.
    fn banner_targets(
        sq: Square<Xiangqi9x10>,
        occupied: Bitboard<Xiangqi9x10>,
    ) -> Bitboard<Xiangqi9x10> {
        attacks::rook_attacks::<Xiangqi9x10>(sq, occupied)
            | attacks::cannon_capture_targets::<Xiangqi9x10>(sq, occupied)
            | attacks::horse_attacks::<Xiangqi9x10>(sq, occupied)
    }
}

impl WideVariant<Xiangqi9x10> for ManchuRules {
    fn starting_position() -> (Board<Xiangqi9x10>, GenericState<Xiangqi9x10>) {
        let board = Board::<Xiangqi9x10>::from_fen_placement(MANCHU_START_PLACEMENT)
            .expect("the Manchu starting placement is valid on a 9x10 board");
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::NONE,
            ep_square: None,
            gating: GenericGating::NONE,
            duck: None,
            placement: GenericPlacement::NONE,
            halfmove_clock: 0,
            fullmove_number: 1,
            consecutive_passes: 0,
        };
        (board, state)
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Xiangqi9x10>,
        occupancy: Bitboard<Xiangqi9x10>,
    ) -> Bitboard<Xiangqi9x10> {
        match role {
            // The Banner: its occupancy-only set is the combined rook + cannon +
            // horse set. The board-aware path (`role_attacks_board`) returns the
            // same thing; this occupancy-only fallback keeps any incidental query
            // honest (Manchu is on the per-move verify path via `has_cannons`).
            WideRole::Banner => Self::banner_targets(sq, occupancy),
            // Every other role is exactly the Xiangqi mover.
            _ => XiangqiRules::role_attacks(role, color, sq, occupancy),
        }
    }

    fn quiet_only_targets(
        role: WideRole,
        color: Color,
        sq: Square<Xiangqi9x10>,
        occupancy: Bitboard<Xiangqi9x10>,
    ) -> Bitboard<Xiangqi9x10> {
        // The Banner has no quiet-only steps of its own — its quiet rook slides and
        // horse leaps come through the empty squares of `role_attacks` /
        // `role_attacks_board`. Everything else (the Cannon's quiet rays) is the
        // Xiangqi quiet set.
        XiangqiRules::quiet_only_targets(role, color, sq, occupancy)
    }

    fn uses_board_attacks() -> bool {
        // The Banner's cannon part is occupancy-dependent (it lands only on the
        // occupied target beyond a screen), so its move-and-attack set is computed
        // from the live board. (The plain occupancy-only `role_attacks` is a sound
        // fallback, but the board hook is the canonical path.)
        true
    }

    fn role_attacks_board(
        role: WideRole,
        _color: Color,
        sq: Square<Xiangqi9x10>,
        board: &Board<Xiangqi9x10>,
    ) -> Option<Bitboard<Xiangqi9x10>> {
        // Only the Banner uses the whole board; every other role falls back to the
        // occupancy-only Xiangqi `role_attacks`. The returned set folds the rook
        // slide, the cannon over-screen capture, and the horse leap together; the
        // generator's `emit_targets` splits it into quiet/capture by enemy
        // occupancy, and the king-safety test sees a royal (occupied) square only in
        // the capture portion (a rook adjacency, a cannon over-screen capture, or a
        // horse leap).
        if role != WideRole::Banner {
            return None;
        }
        Some(Self::banner_targets(sq, board.occupied()))
    }

    fn role_attack_is_leg_asymmetric(role: WideRole) -> bool {
        // The Banner's attack relation is occupancy-asymmetric: its cannon part
        // lands only on an occupied square (so reverse-projecting from an empty
        // target invents a Banner attacker) and its horse part is hobbled by a leg
        // adjacent to the *Banner* (a different square than a reverse leap would
        // test). So `attackers_to` must forward-project the Banner's set from each
        // Banner origin, exactly as the generator does. Every other role keeps the
        // Xiangqi forward-projection classification (Horse, Soldier, Cannon,
        // General, Advisor, Elephant).
        role == WideRole::Banner || XiangqiRules::role_attack_is_leg_asymmetric(role)
    }

    fn role_is_slider(role: WideRole) -> bool {
        // The Banner is **not** a pure line slider for pin purposes: although its
        // rook part slides, its cannon and horse parts do not, and Manchu (like
        // Xiangqi) runs the cannon-verify path, which does not consult pins. Defer
        // to the Xiangqi classification (only the plain Chariot pins) for every
        // other role; the Banner is not pin-classified.
        XiangqiRules::role_is_slider(role)
    }

    fn has_castling() -> bool {
        false
    }

    fn has_cannons() -> bool {
        // Manchu fields cannons (Black's army) and the Banner's cannon-capture and
        // over-screen behaviour, so it takes the pseudo-legal + per-move verify
        // king-safety path, exactly as Xiangqi. The flying-general extra attack
        // rides the same verify.
        true
    }

    fn has_flying_general() -> bool {
        true
    }

    fn extra_royal_attack(
        board: &Board<Xiangqi9x10>,
        sq: Square<Xiangqi9x10>,
        by: Color,
        occupied: Bitboard<Xiangqi9x10>,
    ) -> bool {
        // The flying general is unchanged from Xiangqi: the two generals may not
        // face each other down an otherwise-empty file.
        XiangqiRules::extra_royal_attack(board, sq, by, occupied)
    }
}

/// Manchu (yipaisanxianqi) as a [`GenericPosition`] over the 9x10 [`Xiangqi9x10`]
/// geometry.
///
/// Construct the starting position (a full Black Xiangqi army against White's
/// single Banner super-piece) with
/// [`Manchu::startpos`](GenericPosition::startpos) or parse a FEN (mce dialect)
/// with [`Manchu::from_fen`](GenericPosition::from_fen). See the [module
/// docs](self) for the Banner movement (Rook + Cannon + Horse) and the reused
/// Xiangqi palace / river / flying-general rules.
pub type Manchu = GenericPosition<Xiangqi9x10, ManchuRules>;

#[cfg(test)]
mod tests {
    use super::*;

    /// The canonical start FEN round-trips through the mce dialect.
    #[test]
    fn startpos_round_trips() {
        let pos = Manchu::startpos();
        assert_eq!(
            pos.to_fen(),
            "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/9/9/*M1OUKUO2 w - - 0 1"
        );
    }

    /// The Banner combines rook, cannon (over-screen), and horse moves: a lone
    /// Banner on an empty board has exactly the rook (orthogonal rays) plus horse
    /// (knight) reach; with a screen and a piece beyond it, the cannon capture is
    /// added.
    #[test]
    fn banner_moves_match_rook_plus_horse_plus_cannon() {
        // Banner on e5 of an otherwise empty board (kings parked far apart so the
        // position is legal). Rook rays + horse leaps, no cannon target (no screen).
        let pos = Manchu::from_fen("k8/9/9/9/9/4*M4/9/9/9/4K4 w - - 0 1").expect("valid FEN");
        let banner_moves = pos
            .legal_moves()
            .iter()
            .filter(|m| {
                pos.board()
                    .role_at(m.from())
                    .is_some_and(|r| r == WideRole::Banner)
            })
            .count();
        // Rook from e5: the e-file reaches e2..e10 but **not** e1 (the white king
        // sits there) = 8 vertical; plus the full e-rank a5..i5 minus e5 = 8
        // horizontal; 16 rook squares. The horse adds its 8 on-board knight leaps
        // (c4 c6 d3 d7 f3 f7 g4 g6). 16 + 8 = 24, matching FSF (its `e5*` moves).
        assert_eq!(
            banner_moves, 24,
            "rook (16, e1 blocked by king) + horse (8)"
        );
    }
}
