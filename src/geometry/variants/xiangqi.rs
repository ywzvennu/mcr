//! Xiangqi (Chinese chess, 9x10) on the generic engine — the first marquee fairy
//! variant and the first real consumer of the reusable **cannon** primitive
//! (issue #184), the blockable-leg **horse** / **elephant** primitives, and the
//! palace / river / flying-general machinery (`docs/fairy-variants-architecture.md`,
//! Phase 3). Validated move-for-move against Fairy-Stockfish `UCI_Variant
//! xiangqi`.
//!
//! Xiangqi is played on a 9-files (a..i) by 10-ranks (1..10) board — mce's
//! [`Xiangqi9x10`] `u128` geometry — with pieces on the cells. The two armies face
//! across a central **river** (between ranks 5 and 6); each general lives in a 3x3
//! **palace** (files d..f, the near three ranks).
//!
//! ## Pieces (confirmed against FSF)
//!
//! * **General / King** ([`WideRole::King`], FSF `k`): one orthogonal step,
//!   **confined to the palace**. Plus the **flying-general** rule (below).
//! * **Advisor / Guard** ([`WideRole::Advisor`], mce `u`, FSF `a`): one diagonal
//!   step, **confined to the palace**.
//! * **Elephant / Minister** ([`WideRole::XiangqiElephant`], mce `o`, FSF `b`):
//!   a two-square diagonal jump, **blocked by a piece on the intervening "eye"**,
//!   and **may not cross the river** (stays on its own half). Uses
//!   [`attacks::elephant_attacks_blockable`].
//! * **Horse** ([`WideRole::Horse`], mce `j`, FSF `n`): a knight leap **hobbled**
//!   if the orthogonally-adjacent leg square is occupied. Uses
//!   [`attacks::horse_attacks`].
//! * **Chariot / Rook** ([`WideRole::Rook`], `r`): a plain rook.
//! * **Cannon** ([`WideRole::Cannon`], `c`): moves like a rook over empty squares,
//!   captures only by jumping exactly one screen — the **same** primitive as the
//!   Shako cannon ([`attacks::cannon_quiet_moves`] / [`attacks::cannon_capture_targets`]),
//!   confirmed identical against FSF.
//! * **Soldier / Pawn** ([`WideRole::Soldier`], mce `z`, FSF `p`): one step
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
//! From FSF's `xiangqi_variant()` (`startFen`); mce and FSF agree on the position
//! but spell four pieces differently (mce avoids the letters `a n b p`, already
//! taken by the Hawk / Knight / Bishop / Pawn), so the `compare-fairy` harness
//! rewrites them when driving FSF:
//!
//! ```text
//! FSF dialect: rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1
//! mce dialect: rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/9/RJOUKUOJR w - - 0 1
//! ```

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{attacks, Bitboard, Board, Square, WideRole, WideVariant, Xiangqi9x10};
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

/// The confirmed Xiangqi starting placement in the mce dialect (advisor `u`,
/// horse `j`, elephant `o`, soldier `z`), the position byte-identical to FSF's
/// `rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR`.
const XIANGQI_START_PLACEMENT: &str = "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/9/RJOUKUOJR";

/// The four ferz (one diagonal step) offsets — the Advisor's movement.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// The four wazir (one orthogonal step) offsets — the General's movement.
const WAZIR_OFFSETS: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

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
        // The Horse's leap is hobbled by the leg adjacent to the *horse* toward the
        // leap — a per-leap geometric asymmetry that reverse-projection from the
        // target cannot resolve, so attacker detection must project forward from
        // each horse. The Elephant's eye is symmetric (the intervening diagonal is
        // the same square from either end), so it needs no special handling.
        matches!(role, WideRole::Horse)
    }

    fn role_is_slider(role: WideRole) -> bool {
        // Only the Chariot (rook) is a line slider that can pin. The Cannon is not
        // (it needs a screen to capture); the Horse, Elephant, Advisor, General,
        // and Soldier are steppers/leapers. Xiangqi runs the cannon verify path,
        // which does not consult pins, but the classification is kept honest.
        matches!(role, WideRole::Rook)
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
}

/// Xiangqi (Chinese chess) as a [`GenericPosition`] over the 9x10 [`Xiangqi9x10`]
/// geometry.
///
/// Construct the starting position with
/// [`Xiangqi::startpos`](GenericPosition::startpos) or parse a FEN (mce dialect)
/// with [`Xiangqi::from_fen`](GenericPosition::from_fen). See the [module
/// docs](self) for the piece movements, the palace / river confinement, and the
/// flying-general rule.
pub type Xiangqi = GenericPosition<Xiangqi9x10, XiangqiRules>;
