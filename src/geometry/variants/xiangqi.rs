//! Xiangqi (Chinese chess, 9x10) on the generic engine — the first marquee fairy
//! variant and the first real consumer of the reusable **cannon** primitive
//! (issue #184), the blockable-leg **horse** / **elephant** primitives, and the
//! palace / river / flying-general machinery (`docs/fairy-variants-architecture.md`,
//! Phase 3). Validated move-for-move against Fairy-Stockfish `UCI_Variant
//! xiangqi`.
//!
//! Xiangqi is played on a 9-files (a..i) by 10-ranks (1..10) board — mcr's
//! [`Xiangqi9x10`] `u128` geometry — with pieces on the cells. The two armies face
//! across a central **river** (between ranks 5 and 6); each general lives in a 3x3
//! **palace** (files d..f, the near three ranks).
//!
//! ## Pieces (confirmed against FSF)
//!
//! * **General / King** ([`WideRole::King`], FSF `k`): one orthogonal step,
//!   **confined to the palace**. Plus the **flying-general** rule (below).
//! * **Advisor / Guard** ([`WideRole::Advisor`], mcr `u`, FSF `a`): one diagonal
//!   step, **confined to the palace**.
//! * **Elephant / Minister** ([`WideRole::XiangqiElephant`], mcr `o`, FSF `b`):
//!   a two-square diagonal jump, **blocked by a piece on the intervening "eye"**,
//!   and **may not cross the river** (stays on its own half). Uses
//!   [`attacks::elephant_attacks_blockable`].
//! * **Horse** ([`WideRole::Horse`], mcr `j`, FSF `n`): a knight leap **hobbled**
//!   if the orthogonally-adjacent leg square is occupied. Uses
//!   [`attacks::horse_attacks`].
//! * **Chariot / Rook** ([`WideRole::Rook`], `r`): a plain rook.
//! * **Cannon** ([`WideRole::Cannon`], `c`): moves like a rook over empty squares,
//!   captures only by jumping exactly one screen — the **same** primitive as the
//!   Shako cannon ([`attacks::cannon_quiet_moves`] / [`attacks::cannon_capture_targets`]),
//!   confirmed identical against FSF.
//! * **Soldier / Pawn** ([`WideRole::Soldier`], mcr `z`, FSF `p`): one step
//!   forward; **after crossing the river** also one step sideways. Never backward,
//!   no double-step, no promotion.
//!
//! ## Flying general
//!
//! The two generals may not face each other on a file with no piece between — a
//! move leaving them facing is illegal, and a general gives check down such an
//! open file. Modelled through the default-off
//! [`WideVariant::extra_royal_attack`] hook: the engine ORs it into every
//! king-safety test, so it behaves exactly like an attack down the open file.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `xiangqi_variant()` (`startFen`); mcr and FSF agree on the position
//! but spell four pieces differently (mcr avoids the letters `a n b p`, already
//! taken by the Hawk / Knight / Bishop / Pawn), so the `compare-fairy` harness
//! rewrites them when driving FSF:
//!
//! ```text
//! FSF dialect: rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1
//! mcr dialect: rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/9/RJOUKUOJR w - - 0 1
//! ```

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, RoyalSlider, Square, WideRole, WideVariant, Xiangqi9x10,
};
use crate::Color;

/// The Xiangqi rule layer: a zero-sized [`WideVariant`] over [`Xiangqi9x10`].
///
/// It overrides only what Xiangqi changes from the generic engine: the 9x10
/// starting array, the palace-confined General and Advisor, the blockable Horse
/// and river-bound Elephant, the river-crossing Soldier, the cannon (reused from
/// the shared primitive), the flying-general king-safety, and the no-castling /
/// no-promotion / no-en-passant rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct XiangqiRules;

/// The confirmed Xiangqi starting placement in the mcr dialect (advisor `u`,
/// horse `j`, elephant `o`, soldier `z`), the position byte-identical to FSF's
/// `rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR`.
const XIANGQI_START_PLACEMENT: &str = "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/9/RJOUKUOJR";

/// The four ferz (one diagonal step) offsets — the Advisor's movement.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// The four wazir (one orthogonal step) offsets — the General's movement.
const WAZIR_OFFSETS: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// The four two-step-diagonal offsets — the Elephant's leap shape (unhobbled,
/// unconfined). Used only to build the **superset** of squares from which an
/// Elephant could reach a target, for the king-safety reach pre-filter.
const ELEPHANT_OFFSETS: [(i8, i8); 4] = [(2, 2), (2, -2), (-2, 2), (-2, -2)];

impl XiangqiRules {
    /// The palace mask for `color`: the 3x3 block on files d..f (3..=5), on the
    /// three ranks nearest that color (ranks 1..3 for White, 8..10 for Black).
    fn palace(color: Color) -> Bitboard<Xiangqi9x10> {
        let ranks: [u8; 3] = match color {
            Color::White => [0, 1, 2],
            Color::Black => [7, 8, 9],
        };
        let mut bb = Bitboard::<Xiangqi9x10>::EMPTY;
        for &rank in &ranks {
            for file in 3..=5u8 {
                if let Some(sq) = Square::<Xiangqi9x10>::from_file_rank(file, rank) {
                    bb.set(sq);
                }
            }
        }
        bb
    }

    /// The own-half mask for `color`: the five ranks on its side of the river
    /// (ranks 1..5 for White, 6..10 for Black). The Elephant may not leave it.
    fn own_half(color: Color) -> Bitboard<Xiangqi9x10> {
        let (lo, hi) = match color {
            Color::White => (0u8, 4u8),
            Color::Black => (5u8, 9u8),
        };
        let mut bb = Bitboard::<Xiangqi9x10>::EMPTY;
        for rank in lo..=hi {
            for file in 0..9u8 {
                if let Some(sq) = Square::<Xiangqi9x10>::from_file_rank(file, rank) {
                    bb.set(sq);
                }
            }
        }
        bb
    }

    /// Returns `true` if a soldier of `color` standing on `rank` has crossed the
    /// river (and so may also step sideways). White crosses at rank 6 (index 5),
    /// Black at rank 5 (index 4).
    fn soldier_crossed(color: Color, rank: u8) -> bool {
        match color {
            Color::White => rank >= 5,
            Color::Black => rank <= 4,
        }
    }

    /// The Soldier's move/attack squares for `color` on `sq`: one step forward
    /// always, plus one step left and right once it has crossed the river. Never
    /// backward. A soldier never promotes. The set is the same for moving and for
    /// capturing (a soldier captures wherever it can move).
    fn soldier_targets(color: Color, sq: Square<Xiangqi9x10>) -> Bitboard<Xiangqi9x10> {
        let forward: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = Bitboard::<Xiangqi9x10>::EMPTY;
        if let Some(dest) = sq.offset(0, forward) {
            bb.set(dest);
        }
        if Self::soldier_crossed(color, sq.rank()) {
            for df in [-1i8, 1] {
                if let Some(dest) = sq.offset(df, 0) {
                    bb.set(dest);
                }
            }
        }
        bb
    }
}

impl WideVariant<Xiangqi9x10> for XiangqiRules {
    fn starting_position() -> (Board<Xiangqi9x10>, GenericState<Xiangqi9x10>) {
        let board = Board::<Xiangqi9x10>::from_fen_placement(XIANGQI_START_PLACEMENT)
            .expect("the Xiangqi starting placement is valid on a 9x10 board");
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
            board_b: crate::geometry::Bitboard::EMPTY,
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
            // General: a wazir confined to the palace. (The flying-general
            // file-attack is handled separately in `extra_royal_attack`.)
            WideRole::King => {
                attacks::leaper_attacks::<Xiangqi9x10>(sq, &WAZIR_OFFSETS) & Self::palace(color)
            }
            // Advisor: a ferz confined to the palace.
            WideRole::Advisor => {
                attacks::leaper_attacks::<Xiangqi9x10>(sq, &FERZ_OFFSETS) & Self::palace(color)
            }
            // Elephant: a blockable two-diagonal leaper bound to its own half.
            WideRole::XiangqiElephant => {
                attacks::elephant_attacks_blockable::<Xiangqi9x10>(sq, occupancy)
                    & Self::own_half(color)
            }
            // Horse: a knight hobbled by a leg blocker.
            WideRole::Horse => attacks::horse_attacks::<Xiangqi9x10>(sq, occupancy),
            // Chariot: a plain rook.
            WideRole::Rook => attacks::rook_attacks::<Xiangqi9x10>(sq, occupancy),
            // Cannon: its *attack* set is the over-one-screen capture set; the
            // quiet rook-rays come through `quiet_only_targets`.
            WideRole::Cannon => attacks::cannon_capture_targets::<Xiangqi9x10>(sq, occupancy),
            // Soldier: forward (and, past the river, sideways). It captures
            // wherever it moves, so the attack set equals the move set.
            WideRole::Soldier => Self::soldier_targets(color, sq),
            // No other role is fielded in Xiangqi.
            _ => Bitboard::EMPTY,
        }
    }

    fn quiet_only_targets(
        role: WideRole,
        _color: Color,
        sq: Square<Xiangqi9x10>,
        occupancy: Bitboard<Xiangqi9x10>,
    ) -> Bitboard<Xiangqi9x10> {
        match role {
            // The Cannon's non-capturing moves: the empty rook-ray squares. (Its
            // captures are the over-screen set in `role_attacks`.)
            WideRole::Cannon => attacks::cannon_quiet_moves::<Xiangqi9x10>(sq, occupancy),
            _ => Bitboard::EMPTY,
        }
    }

    fn role_attack_is_leg_asymmetric(role: WideRole) -> bool {
        // Two roles cannot be detected by reverse-projecting their pattern from the
        // target square, so attacker detection must instead project each piece's
        // attack set *forward* from its own origin (exactly as the move generator
        // does):
        //
        // * **Horse** — its leap is hobbled by the leg adjacent to the *horse*
        //   toward the leap, a different square than the leg adjacent to the target
        //   toward the horse. Reverse-projection tests the wrong leg. (#198.)
        // * **Soldier** — its attack set is forward-biased *and* its sideways step
        //   unlocks only past the **river**, a rank threshold that is *color-
        //   dependent* (White crosses at rank 6, Black at rank 5). A simple color-
        //   flipped reverse-projection (the `role_attack_is_directional` path that
        //   suffices for the *riverless* Minixiangqi soldier, #200) flips that
        //   threshold along with the color, so it would test the wrong crossing
        //   state and miss a crossed soldier guarding a square *beside* it. Forward
        //   projection from each soldier — keyed on the soldier's *own* rank and
        //   color — matches the soldier's real attack geometry (forward always,
        //   sideways after the river) exactly. Without it `attackers_to` lets the
        //   enemy king step in front of (or, post-river, beside) a soldier.
        //
        // * **Cannon** — its *attack* (over-screen capture) set lands only on an
        //   **occupied** square (the captured piece), so it is occupancy-
        //   asymmetric: reverse-projecting the cannon pattern from a target `t`
        //   treats `t` as a cannon origin and reports a cannon attacker even when
        //   `t` is *empty* (where a cannon, capturing nothing, does not attack).
        //   That phantom is harmless on an occupied royal square but is a genuine
        //   asymmetry, so attacker detection forward-projects from each cannon —
        //   exactly as the move generator does — keeping `attackers_to` the true
        //   forward relation on every square. (Issue #202.)
        //
        // * **General, Advisor, Elephant** — each is **region-confined**: its
        //   attack set is intersected with the palace (General, Advisor) or the
        //   own river-half (Elephant), a mask keyed on the piece's **origin**
        //   square. That makes the relation asymmetric across the region boundary:
        //   reverse-projecting from a target *outside* the region still yields
        //   in-region source squares, but a confined piece on such a source cannot
        //   actually reach the out-of-region target (its own confinement mask
        //   forbids it). Reverse-projection therefore invents attacks on
        //   out-of-region squares; forward-projecting from each piece — keyed on
        //   its own origin and confinement — matches the real geometry on every
        //   square. (The Elephant's *eye* is symmetric, but its half-board
        //   confinement is not.) These never affect king-safety (a royal square is
        //   always inside its own palace, where the relation is symmetric), so
        //   perft is unchanged; the fix only corrects `attackers_to` on the
        //   out-of-region squares the property test probes. (Issue #202.)
        matches!(
            role,
            WideRole::Horse
                | WideRole::Soldier
                | WideRole::Cannon
                | WideRole::King
                | WideRole::Advisor
                | WideRole::XiangqiElephant
        )
    }

    fn role_is_slider(role: WideRole) -> bool {
        // Only the Chariot (rook) is a line slider that can pin. The Cannon is not
        // (it needs a screen to capture); the Horse, Elephant, Advisor, General,
        // and Soldier are steppers/leapers. Xiangqi runs the cannon verify path,
        // which does not consult pins, but the classification is kept honest.
        matches!(role, WideRole::Rook)
    }

    fn royal_slider_kind(role: WideRole) -> Option<RoyalSlider> {
        // The Chariot is the plain standard rook (`role_attacks` is exactly
        // `rook_attacks`), so the cannon king-safety verify reverse-projects it from
        // the king with the precomputed line masks instead of rebuilding them every
        // sibling move. No diagonal sliders exist in Xiangqi; every other role is a
        // leaper or the asymmetric Cannon, which keep the forward path.
        matches!(role, WideRole::Rook).then_some(RoyalSlider::Rook)
    }

    fn royal_reach_superset(
        role: WideRole,
        king: Square<Xiangqi9x10>,
    ) -> Option<Bitboard<Xiangqi9x10>> {
        // A superset (occupancy-independent, ignoring legs / confinement / screens —
        // all re-checked by the exact forward projection) of the squares from which
        // each forward-projected role could attack the king. Every leap shape is
        // symmetric, so its shape from the king is a superset of its attack-source
        // squares; confinement and hobbling only remove attacks. The Cannon attacks
        // only along orthogonals, so its sources lie on the king's rank/file.
        match role {
            // Horse: knight-shape neighbourhood of the king.
            WideRole::Horse => Some(attacks::knight_attacks::<Xiangqi9x10>(king)),
            // Elephant: the two-step-diagonal leap shape (unhobbled, unconfined).
            WideRole::XiangqiElephant => Some(attacks::leaper_attacks::<Xiangqi9x10>(
                king,
                &ELEPHANT_OFFSETS,
            )),
            // General (King), Advisor, Soldier: each attacks only from a square
            // adjacent to the king (orthogonally for general/soldier, diagonally for
            // advisor), so the king's one-step neighbourhood is a superset of all
            // three. (The flying-general file is handled separately.)
            WideRole::King | WideRole::Advisor | WideRole::Soldier => {
                Some(attacks::king_attacks::<Xiangqi9x10>(king))
            }
            // Cannon: its over-screen capture travels a straight rank/file ray, so a
            // cannon attacking the king lies on the king's orthogonal lines.
            WideRole::Cannon => Some(attacks::rook_attacks::<Xiangqi9x10>(king, Bitboard::EMPTY)),
            _ => None,
        }
    }

    fn has_castling() -> bool {
        false
    }

    fn has_cannons() -> bool {
        // Xiangqi fields cannons, so it takes the pseudo-legal + per-move verify
        // king-safety path (the cannon's check and king-danger are screen-
        // dependent). The flying-general extra attack rides the same verify.
        true
    }

    fn has_flying_general() -> bool {
        true
    }

    fn king_diag_attack_radius() -> Option<u8> {
        // Xiangqi has no diagonal slider: the only diagonal threats to the general
        // are the Elephant's blocked two-step jump (its eye one diagonal step away)
        // and the Horse's hobbled leg (the king's diagonal neighbour) — every
        // diagonal square that can bear on the general lies within two diagonal
        // steps. The flying-general attack travels the (full-length, uncapped) file.
        // So capping the fast-accept king diagonals at two squares is exact — see
        // [`WideVariant::king_diag_attack_radius`].
        Some(2)
    }

    fn extra_royal_attack(
        board: &Board<Xiangqi9x10>,
        sq: Square<Xiangqi9x10>,
        by: Color,
        occupied: Bitboard<Xiangqi9x10>,
    ) -> bool {
        // The flying general: `by`'s general attacks the royal square `sq` (the
        // other general) iff they share a file with no piece strictly between
        // them. `sq` is the enemy general's square; find `by`'s general and test
        // the file.
        let Some(general) = board.king_of(by) else {
            return false;
        };
        if general.file() != sq.file() || general == sq {
            return false;
        }
        // No piece may lie strictly between the two generals on their shared file.
        (attacks::between::<Xiangqi9x10>(general, sq) & occupied).is_empty()
    }

    // --- Repetition / perpetual check + chase (terminal only; perft unaffected) -
    //
    // Xiangqi forbids both perpetual **check** and perpetual **chase**: a repetition
    // forced by one side checking — or chasing the same unprotected / value-superior
    // enemy piece — on every move is a loss for that side. Both are adjudicated by
    // [`GenericGame`](crate::geometry::game::GenericGame); move generation is
    // untouched, so perft stays byte-identical. The chase model reproduces
    // Fairy-Stockfish's AXF direct-attack chase (the dominant case); see
    // [`GenericGame`](crate::geometry::game::GenericGame) for the precise subset and
    // its residual simplifications versus FSF.

    fn tracks_repetition() -> bool {
        true
    }

    fn perpetual_check_loses() -> bool {
        true
    }

    fn perpetual_chase_loses() -> bool {
        true
    }
}

/// Xiangqi (Chinese chess) as a [`GenericPosition`] over the 9x10 [`Xiangqi9x10`]
/// geometry.
///
/// Construct the starting position with
/// [`Xiangqi::startpos`](GenericPosition::startpos) or parse a FEN (mcr dialect)
/// with [`Xiangqi::from_fen`](GenericPosition::from_fen). See the [module
/// docs](self) for the piece movements, the palace / river confinement, and the
/// flying-general rule.
pub type Xiangqi = GenericPosition<Xiangqi9x10, XiangqiRules>;
