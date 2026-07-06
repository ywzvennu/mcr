//! Chennis (7x7 tennis-themed flipping variant) on the generic engine — a reuse
//! of the Kyoto-Shogi (#232) **per-move flip** hook and the Shogi-family
//! persistent **hand** + **dual-form drops**, on a fresh 7x7 [`Chennis7x7`]
//! geometry, with one further distinctive mechanic: a **king mobility region**
//! (each side's King is confined to a 5x4 zone). Validated against
//! Fairy-Stockfish `UCI_Variant chennis`.
//!
//! Chennis (invented by Couch Tomato, <https://www.pychess.org/variants/chennis>)
//! is a 7x7 variant where each non-royal piece carries **two forms** and
//! **alternates between them move-to-move** — exactly the Kyoto Shogi mechanic:
//! making a move toggles the moving piece's form. There is no promotion *zone* —
//! the flip is unconditional and happens on **every** board move. Captured pieces
//! enter the hand (FSF `capturesToHand`) and a held piece may be **dropped in
//! either form** (FSF `dropPromoted`). The King has no alternate form, never
//! flips, and may never leave its **mobility region**.
//!
//! ## The four flipping pairs (confirmed against FSF)
//!
//! Each pair is `base ↔ promoted`; a move flips the moving piece from one to the
//! other. Seven of the eight forms reuse existing roles; only the base **pawn** is
//! genuinely new ([`WideRole::ChennisPawn`]):
//!
//! | base (moves as)              | promoted (moves as) | base role                 | promoted role         |
//! |------------------------------|---------------------|---------------------------|-----------------------|
//! | **p** Pawn (chess pawn)      | **r** Rook          | [`WideRole::ChennisPawn`] | [`WideRole::Rook`]    |
//! | **f** Ferz (1 diagonal step) | **c** Cannon        | [`WideRole::Met`]         | [`WideRole::Cannon`]  |
//! | **s** Soldier (fwd/sideways) | **b** Bishop        | [`WideRole::Soldier`]     | [`WideRole::Bishop`]  |
//! | **m** Commoner (king step)   | **n** Knight        | [`WideRole::Commoner`]    | [`WideRole::Knight`]  |
//! | **k** King (region-confined) | — (never flips)     | [`WideRole::King`]        | —                     |
//!
//! The promoted forms are ordinary roles here (`Rook`, `Cannon`, `Bishop`,
//! `Knight`), **not** `+`-prefixed Shogi tokens, so the flip mapping is supplied
//! locally by [`flips_on_move`](ChennisRules::flips_on_move) (and the capture
//! banking by [`role_hand_base`](ChennisRules::role_hand_base)), independent of the
//! Shogi promoted-role machinery — every other variant stays byte-identical.
//!
//! ## Movement
//!
//! * **Pawn** ([`WideRole::ChennisPawn`], FSF `p:fmWfceF`): a chess pawn — a quiet
//!   forward step plus a forward-diagonal capture, with no double-step, en passant,
//!   or zone promotion. It is a move≠capture piece, so the generator's quiet/capture
//!   set is supplied by the board-aware
//!   [`role_attacks_board`](ChennisRules::role_attacks_board) fold (the empty
//!   forward step combined with the enemy-only diagonal captures), exactly as
//!   Chak's Soldier; its pure capture set rides
//!   [`role_attacks`](ChennisRules::role_attacks) for the attacker scan.
//! * **Rook** / **Bishop** ([`WideRole::Rook`] / [`WideRole::Bishop`]): the plain
//!   orthogonal / diagonal sliders.
//! * **Ferz** ([`WideRole::Met`]): one diagonal step (move and capture).
//! * **Cannon** ([`WideRole::Cannon`]): the Xiangqi cannon — slides like a rook
//!   over empty squares (its `quiet_only_targets`) and captures by jumping exactly
//!   one screen (its capture-only `role_attacks`). Chennis fields cannons, so it
//!   takes the pseudo-legal + per-move verify king-safety path.
//! * **Soldier** ([`WideRole::Soldier`]): one step forward or sideways (Chennis has
//!   no river, so the Soldier always moves forward/sideways), move and capture.
//! * **Commoner** ([`WideRole::Commoner`]) / **Knight** ([`WideRole::Knight`]): the
//!   non-royal king-stepper and the standard chess knight.
//! * **King** ([`WideRole::King`]): a full king (eight steps) **confined to its
//!   mobility region** — files b..f and its own half plus the two central ranks
//!   (White ranks 1-4, Black ranks 4-7). The confinement masks the King's
//!   `role_attacks` by its origin, so it rides the forward-projection attacker path
//!   (`role_attack_is_leg_asymmetric`).
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant chennis`, `position startpos`) renders the start as
//!
//! ```text
//! 1fkm3/1p1s3/7/7/7/3S1P1/3MKF1[] w - 0 1
//! ```
//!
//! mcr renders the same placement in its own dialect (the Ferz is `m`, the Soldier
//! `z`, the Commoner `*u`, the Pawn `**p`) with an empty `[]` holdings bracket; the
//! `compare-fairy/` harness rewrites each token to FSF's spelling when driving FSF.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, PromotionConfig, RoyalSlider, Square, WideRole, WideVariant,
};
use crate::Color;

use super::super::Chennis7x7;

/// The Chennis rule layer: a zero-sized [`WideVariant`] over [`Chennis7x7`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ChennisRules;

/// The confirmed Chennis starting placement (the hand is empty at the start), in
/// mcr dialect: the Ferz is `m` ([`WideRole::Met`]), the Soldier `z`
/// ([`WideRole::Soldier`]), the Commoner `*u` ([`WideRole::Commoner`]) and the
/// Pawn `**p` ([`WideRole::ChennisPawn`]). This is FSF's `1fkm3/1p1s3/7/7/7/3S1P1/3MKF1`
/// rewritten token-for-token.
const CHENNIS_PLACEMENT: &str = "1mk*u3/1**p1z3/7/7/7/3Z1**P1/3*UKM1";

/// The four diagonal one-step (ferz) offsets.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

impl ChennisRules {
    /// The King's **mobility region** for `color`: files b..f (1..=5) on the side's
    /// own half plus the two central ranks — White ranks 1-4 (0-based 0..=3), Black
    /// ranks 4-7 (0-based 3..=6). The King may never leave this zone.
    fn king_region(color: Color) -> Bitboard<Chennis7x7> {
        let mut bb = Bitboard::<Chennis7x7>::EMPTY;
        let ranks: core::ops::RangeInclusive<u8> = if color.is_white() { 0..=3 } else { 3..=6 };
        for rank in ranks {
            for file in 1..=5u8 {
                if let Some(sq) = Square::<Chennis7x7>::from_file_rank(file, rank) {
                    bb.set(sq);
                }
            }
        }
        bb
    }

    /// The Soldier's attack/move set for `color`: one step forward or to either side
    /// (a forward/sideways Wazir, never backward). Chennis has no river, so the
    /// Soldier always moves forward/sideways; it captures wherever it moves.
    fn soldier_targets(color: Color, sq: Square<Chennis7x7>) -> Bitboard<Chennis7x7> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        attacks::leaper_attacks::<Chennis7x7>(sq, &[(0, fwd), (1, 0), (-1, 0)])
    }

    /// The Chennis Pawn's **capture** set for `color`: one step diagonally forward
    /// (a forward Ferz). It captures only here, never as a quiet move.
    fn pawn_captures(color: Color, sq: Square<Chennis7x7>) -> Bitboard<Chennis7x7> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        attacks::leaper_attacks::<Chennis7x7>(sq, &[(1, fwd), (-1, fwd)])
    }

    /// The Chennis Pawn's **quiet move** square for `color`: the single square
    /// straight forward (it moves there only onto an empty square, never captures).
    fn pawn_quiets(color: Color, sq: Square<Chennis7x7>) -> Bitboard<Chennis7x7> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = Bitboard::<Chennis7x7>::EMPTY;
        if let Some(dest) = sq.offset(0, fwd) {
            bb.set(dest);
        }
        bb
    }
}

impl WideVariant<Chennis7x7> for ChennisRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 73;

    fn starting_position() -> (Board<Chennis7x7>, GenericState<Chennis7x7>) {
        let board = Board::<Chennis7x7>::from_fen_placement(CHENNIS_PLACEMENT)
            .expect("the Chennis starting placement is valid on a 7x7 board");
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::NONE,
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

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Chennis7x7>,
        occupancy: Bitboard<Chennis7x7>,
    ) -> Bitboard<Chennis7x7> {
        match role {
            // Base forms.
            // The Pawn's *attack* set is its forward-diagonal capture only; its
            // quiet forward step rides `quiet_only_targets`.
            WideRole::ChennisPawn => Self::pawn_captures(color, sq),
            // Ferz: one diagonal step (move and capture).
            WideRole::Met => attacks::leaper_attacks::<Chennis7x7>(sq, &FERZ_OFFSETS),
            // Soldier: forward plus sideways (no river); captures where it moves.
            WideRole::Soldier => Self::soldier_targets(color, sq),
            // Commoner: a non-royal king-stepper.
            WideRole::Commoner => attacks::king_attacks::<Chennis7x7>(sq),
            // King: a full king confined to its mobility region (masked by origin).
            WideRole::King => attacks::king_attacks::<Chennis7x7>(sq) & Self::king_region(color),
            // Promoted forms.
            WideRole::Rook => attacks::rook_attacks::<Chennis7x7>(sq, occupancy),
            WideRole::Bishop => attacks::bishop_attacks::<Chennis7x7>(sq, occupancy),
            WideRole::Knight => attacks::knight_attacks::<Chennis7x7>(sq),
            // Cannon: its *attack* set is the over-one-screen capture set; the quiet
            // rook-rays come through `quiet_only_targets`.
            WideRole::Cannon => attacks::cannon_capture_targets::<Chennis7x7>(sq, occupancy),
            _ => Bitboard::EMPTY,
        }
    }

    fn quiet_only_targets(
        role: WideRole,
        _color: Color,
        sq: Square<Chennis7x7>,
        occupancy: Bitboard<Chennis7x7>,
    ) -> Bitboard<Chennis7x7> {
        match role {
            // The Cannon's non-capturing moves: the empty rook-ray squares. (The
            // Pawn's quiet step is folded into `role_attacks_board` and suppressed
            // here by `quiet_targets_board`.)
            WideRole::Cannon => attacks::cannon_quiet_moves::<Chennis7x7>(sq, occupancy),
            _ => Bitboard::EMPTY,
        }
    }

    fn uses_board_attacks() -> bool {
        // The Pawn is a move≠capture piece: on the cannon pseudo-legal + verify
        // path the generator splits a role's set into quiet/capture by occupancy, so
        // the Pawn's empty forward-diagonal squares must be excluded *before* that
        // split (its `role_attacks` capture set would otherwise be emitted as quiet
        // moves onto empty squares). The board-aware fold isolates the capture
        // portion. Every other role returns `None` below and falls back to the
        // occupancy-only `role_attacks` / `quiet_only_targets`.
        true
    }

    fn role_attacks_board(
        role: WideRole,
        color: Color,
        sq: Square<Chennis7x7>,
        board: &Board<Chennis7x7>,
    ) -> Option<Bitboard<Chennis7x7>> {
        match role {
            // The Pawn moves one step forward onto an *empty* square and captures
            // only forward-diagonally onto an *enemy*. The set folds the two: the
            // quiet step is masked to empty squares so a king (always on an occupied
            // square) falls only in the capture portion — the same trick Chak's
            // Soldier uses. `emit_targets` then re-splits it into quiet (empty) and
            // capture (enemy).
            WideRole::ChennisPawn => {
                let occupied = board.occupied();
                let enemies = board.by_color(color.opposite());
                let quiet = Self::pawn_quiets(color, sq) & !occupied;
                let captures = Self::pawn_captures(color, sq) & enemies;
                Some(quiet | captures)
            }
            _ => None,
        }
    }

    fn quiet_targets_board(
        role: WideRole,
        _color: Color,
        _sq: Square<Chennis7x7>,
        _board: &Board<Chennis7x7>,
    ) -> Option<Bitboard<Chennis7x7>> {
        match role {
            // The Pawn's quiet step is already folded into `role_attacks_board`, so it
            // emits no separate quiet-only set (returning an explicit empty set
            // suppresses the `quiet_only_targets` fallback that would double-emit it).
            WideRole::ChennisPawn => Some(Bitboard::EMPTY),
            // The Cannon folds nothing here: its quiet rook-rays ride the
            // occupancy-only `quiet_only_targets` (the `None` fallback).
            _ => None,
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // The Soldier (forward / sideways) is forward-biased and captures wherever it
        // moves, so its occupancy-independent attack set points the *opposite* way
        // for each color: `attackers_to` reverse-projects the *opposing*-color
        // pattern from the target. (The sideways component is color-symmetric, so
        // flipping the color leaves it unchanged.) The Pawn is *not* directional —
        // it is a move≠capture piece whose board-aware set folds quiet forward steps
        // with diagonal captures, so it rides the forward-projection path below.
        matches!(role, WideRole::Soldier)
    }

    fn role_attack_is_leg_asymmetric(role: WideRole) -> bool {
        // * The Pawn's `role_attacks_board` set folds its quiet forward step (onto an
        //   empty square) with its forward-diagonal captures (onto enemies); only the
        //   board-aware set isolates the capture portion (an empty diagonal must
        //   never count as a threat from an empty target, nor an empty forward square
        //   from an occupied one), so it rides the forward-projection path (the same
        //   handling as Chak's Soldier).
        // * The Cannon's over-screen capture set lands only on an occupied square, so
        //   reverse-projecting from an empty target reports a phantom cannon attacker
        //   there. Forward-projecting from each cannon keeps `attackers_to` the true
        //   relation on every square (the same handling as Xiangqi / Minixiangqi).
        // * The King is **region-confined**: its attack set is masked to the mobility
        //   region by its *origin*, so reverse-projecting from a target outside the
        //   region would invent an attack from an in-region King that cannot reach
        //   it. Forward-projection from each King matches the real geometry. This
        //   never affects king-safety (a royal square is always inside its own
        //   region), so perft is unchanged.
        matches!(
            role,
            WideRole::ChennisPawn | WideRole::Cannon | WideRole::King
        )
    }

    fn role_is_slider(role: WideRole) -> bool {
        // Only the Rook and Bishop are line sliders that can pin. The Cannon is not
        // (it needs a screen to capture); every other role is a stepper/leaper.
        matches!(role, WideRole::Rook | WideRole::Bishop)
    }

    fn royal_slider_kind(role: WideRole) -> Option<RoyalSlider> {
        // The Rook is the plain orthogonal slider and the Bishop the plain diagonal
        // slider (their `role_attacks` are exactly `rook_attacks` / `bishop_attacks`),
        // so the cannon king-safety verify reuses the king's precomputed line masks.
        match role {
            WideRole::Rook => Some(RoyalSlider::Rook),
            WideRole::Bishop => Some(RoyalSlider::Bishop),
            _ => None,
        }
    }

    fn royal_reach_superset(
        role: WideRole,
        king: Square<Chennis7x7>,
    ) -> Option<Bitboard<Chennis7x7>> {
        // Supersets (occupancy-independent) of the squares from which each
        // forward-projected role could attack the king. The directional Pawn /
        // Soldier ride the reverse-projection (no superset); the Rook / Bishop ride
        // `royal_slider_kind`; the symmetric Ferz / Commoner / Knight are
        // reverse-projectable. Only the leg-asymmetric Cannon and King need supersets.
        match role {
            // Pawn: it captures the king only from a square one diagonal step away,
            // so its sources lie on the king's four diagonal neighbours.
            WideRole::ChennisPawn => {
                Some(attacks::leaper_attacks::<Chennis7x7>(king, &FERZ_OFFSETS))
            }
            // Cannon: an over-screen orthogonal ray, so its sources lie on the king's
            // rank/file.
            WideRole::Cannon => Some(attacks::rook_attacks::<Chennis7x7>(king, Bitboard::EMPTY)),
            // King: attacks only from a square adjacent to the king.
            WideRole::King => Some(attacks::king_attacks::<Chennis7x7>(king)),
            _ => None,
        }
    }

    fn has_cannons() -> bool {
        // Chennis fields cannons, so it takes the pseudo-legal + per-move verify
        // king-safety path (the cannon's check and king-danger are screen-dependent).
        // The region-confined King rides the same verify naturally.
        true
    }

    fn has_castling() -> bool {
        false
    }

    fn promotion_config() -> PromotionConfig {
        // Chennis has no promotion *zone* (the flip is per-move, via `flips_on_move`),
        // so this static set is unused; the trait requires it.
        PromotionConfig {
            roles: alloc::vec![WideRole::Rook],
        }
    }

    fn in_promotion_zone(_color: Color, _rank: u8) -> bool {
        // No promotion zone: the per-move flip replaces zone promotion entirely.
        false
    }

    fn role_can_promote(_role: WideRole) -> bool {
        // No *zone* promotion (the flip is the per-move `flips_on_move` mechanic).
        false
    }

    fn stalemate_is_loss() -> bool {
        // FSF `stalemateValue = loss`: a stalemated side loses. This does not affect
        // perft (terminal evaluation, not move counts), but matches the rules.
        true
    }

    // --- hand / drops + per-move flip -------------------------------------

    fn has_hand() -> bool {
        true
    }

    fn drops_can_promote() -> bool {
        // FSF `dropPromoted`: a held piece may be deployed in its base or its flipped
        // (promoted) form.
        true
    }

    fn role_hand_base(role: WideRole) -> WideRole {
        // A captured promoted piece banks as its base partner (the hand stores the
        // base form, exactly as Kyoto Shogi banks a captured `+S` as a Silver), and a
        // dropped flipped form takes the same base from the pocket. The four pairs:
        // Rook→Pawn, Cannon→Ferz, Bishop→Soldier, Knight→Commoner; every base (and
        // every non-Chennis role) maps to itself.
        match role {
            WideRole::Rook => WideRole::ChennisPawn,
            WideRole::Cannon => WideRole::Met,
            WideRole::Bishop => WideRole::Soldier,
            WideRole::Knight => WideRole::Commoner,
            other => other,
        }
    }

    fn flips_on_move(role: WideRole) -> Option<WideRole> {
        // Every move flips the moving piece to its alternate form: a base piece to
        // its promoted form, a promoted piece back to its base. The King has no
        // alternate form and never flips.
        match role {
            WideRole::ChennisPawn => Some(WideRole::Rook),
            WideRole::Rook => Some(WideRole::ChennisPawn),
            WideRole::Met => Some(WideRole::Cannon),
            WideRole::Cannon => Some(WideRole::Met),
            WideRole::Soldier => Some(WideRole::Bishop),
            WideRole::Bishop => Some(WideRole::Soldier),
            WideRole::Commoner => Some(WideRole::Knight),
            WideRole::Knight => Some(WideRole::Commoner),
            _ => None,
        }
    }
}

/// Chennis (7x7 tennis-themed flipping variant) as a [`GenericPosition`] over the
/// 7x7 [`Chennis7x7`] geometry.
///
/// Construct the starting position with
/// [`Chennis::startpos`](GenericPosition::startpos) or parse a FEN (the placement
/// may carry the hand as a `[..]` holdings bracket) with
/// [`Chennis::from_fen`](GenericPosition::from_fen). See the [module docs](self)
/// for the per-move flip, the hand, the dual-form drops, and the king mobility
/// region.
pub type Chennis = GenericPosition<Chennis7x7, ChennisRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::position::WideOutcome;

    /// Stalemate is scored as a **loss** for the side to move (FSF
    /// `stalemateValue = loss`, issue #498). The Black king is confined to files
    /// b..f on ranks 4-7, so on b7 its only in-region steps are b6, c6 and c7: a
    /// White Rook on c1 seals the c-file (c6/c7) and a White Knight on d5 covers
    /// b6 (and c7), while b7 itself is unattacked. Black is stalemated, so Black
    /// loses and White wins.
    #[test]
    fn stalemate_is_a_loss() {
        let pos =
            Chennis::from_fen("1k5/7/3N3/7/7/7/2R1K2[] b - - 0 1").expect("valid chennis FEN");
        assert!(pos.legal_moves().is_empty(), "Black has no legal move");
        assert!(!pos.is_check(), "Black is not in check — a true stalemate");
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::White
            })
        );
    }
}
