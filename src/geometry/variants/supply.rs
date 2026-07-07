//! Supply chess — Xiangqi (9x10) **with drops** (issue #585) — on the generic
//! engine. Fairy-Stockfish's built-in `supply_variant()`: the standard Xiangqi army
//! and rules (palace-confined General / Advisor, river-bound Elephant, hobbled
//! Horse, over-screen Cannon, river-crossing Soldier, flying general) plus a
//! **hand** and **piece drops** restricted to the dropping side's **own half**.
//!
//! ## What Supply adds to Xiangqi
//!
//! Supply is FSF's `supply_variant()`, built on `xiangqi_variant_base()` (so every
//! board move is Xiangqi's — [`SupplyRules`] delegates all movement, king-safety and
//! terminal rules to [`XiangqiRules`]). It layers on
//! FSF `pieceDrops = true`, `dropChecks = false`, `twoBoards = true` and
//! `capturesToHand = false`: a **bughouse-style** two-board game where a side's hand
//! is fed by its *partner's* captures, never by its own. On a single board the hand
//! therefore starts **empty** (start FEN `…[]`) and is never replenished by a
//! capture, so ordinary play generates no drop and Supply's perft is **identical to
//! Xiangqi's** — FSF likewise excludes its two-board "virtual" drops from perft
//! (`movegen.cpp`: "Do not generate virtual drops for perft"). Held pieces (reached
//! by a crafted position / a partner feed) drop under the region rule below.
//!
//! ## Drop region (FSF `dropRegion` + the per-piece `mobilityRegion`)
//!
//! FSF sets `dropRegion[c] = mobilityRegion[c][ELEPHANT]` — in `xiangqi_variant_base`
//! that is the whole **own half** (White ranks 1–5, Black ranks 6–10) — then a drop
//! of a piece is further intersected with *that piece's* `mobilityRegion`. A held
//! piece may thus be dropped onto any empty square of the dropping side's own half
//! **where that piece could legally stand**:
//!
//! * **Chariot / Horse / Cannon** — anywhere in the own half (no `mobilityRegion`).
//! * **Advisor** (FSF `FERS`) — only the five palace diagonal points
//!   (White `d1 f1 e2 d3 f3`).
//! * **Elephant** — only the seven Elephant points (White `c1 g1 a3 e3 i3 c5 g5`).
//! * **Soldier** — only the pre-river Soldier residences (White files a/c/e/g/i on
//!   ranks 4–5), the sole own-half squares a Soldier can occupy.
//!
//! The General is royal and never enters a hand, so it is never dropped. Because
//! each region is exactly the set of own-half squares the piece could reach by
//! normal Xiangqi movement, the FSF `mobilityRegion` reassignments change no board
//! move versus base Xiangqi — they bind only the drop targets.
//!
//! ## Confirmed starting FEN
//!
//! FSF `rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR[] w - - 0 1`;
//! mcr spells the Advisor / Horse / Elephant / Soldier `u j o z` (as Xiangqi does):
//!
//! ```text
//! mcr dialect: rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/9/RJOUKUOJR[] w - - 0 1
//! ```

use crate::geometry::position::{GenericPosition, GenericState};
use crate::geometry::variants::xiangqi::XiangqiRules;
use crate::geometry::{Bitboard, Board, RoyalSlider, Square, WideRole, WideVariant, Xiangqi9x10};
use crate::Color;

/// The Supply rule layer: a zero-sized [`WideVariant`] over [`Xiangqi9x10`]. Every
/// movement, king-safety and terminal rule is Xiangqi's (delegated to
/// [`XiangqiRules`]); Supply overrides only the hand / drop mechanics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SupplyRules;

/// Builds a bitboard from a list of `(file, rank)` cells (0-based), for White; for
/// Black each cell is reflected across the river (`rank -> 9 - rank`).
fn cells(color: Color, points: &[(u8, u8)]) -> Bitboard<Xiangqi9x10> {
    let mut bb = Bitboard::<Xiangqi9x10>::EMPTY;
    for &(file, rank) in points {
        let rank = if color.is_white() { rank } else { 9 - rank };
        if let Some(sq) = Square::<Xiangqi9x10>::from_file_rank(file, rank) {
            bb.set(sq);
        }
    }
    bb
}

impl SupplyRules {
    /// The own-half drop region for `color`: every square on its side of the river
    /// (White ranks 1–5, Black ranks 6–10). FSF `dropRegion[c]`, the base
    /// `mobilityRegion[c][ELEPHANT]`. The drop region for the Chariot, Horse and
    /// Cannon (which carry no per-piece `mobilityRegion`).
    fn own_half(color: Color) -> Bitboard<Xiangqi9x10> {
        let ranks: [u8; 5] = if color.is_white() {
            [0, 1, 2, 3, 4]
        } else {
            [5, 6, 7, 8, 9]
        };
        let mut bb = Bitboard::<Xiangqi9x10>::EMPTY;
        for &rank in &ranks {
            for file in 0..9u8 {
                if let Some(sq) = Square::<Xiangqi9x10>::from_file_rank(file, rank) {
                    bb.set(sq);
                }
            }
        }
        bb
    }

    /// The Advisor drop points (FSF `mobilityRegion[FERS]`): the five palace
    /// diagonal points, White `d1 f1 e2 d3 f3`.
    fn advisor_region(color: Color) -> Bitboard<Xiangqi9x10> {
        cells(color, &[(3, 0), (5, 0), (4, 1), (3, 2), (5, 2)])
    }

    /// The Elephant drop points (FSF `mobilityRegion[ELEPHANT]`): the seven Elephant
    /// points, White `c1 g1 a3 e3 i3 c5 g5`.
    fn elephant_region(color: Color) -> Bitboard<Xiangqi9x10> {
        cells(
            color,
            &[(2, 0), (6, 0), (0, 2), (4, 2), (8, 2), (2, 4), (6, 4)],
        )
    }

    /// The Soldier drop points: the own-half part of FSF `mobilityRegion[SOLDIER]`
    /// — the pre-river Soldier residences, White files a/c/e/g/i on ranks 4–5. (An
    /// own-half drop can never reach the enemy half, so only these squares survive
    /// the `dropRegion` intersection.)
    fn soldier_region(color: Color) -> Bitboard<Xiangqi9x10> {
        cells(
            color,
            &[
                (0, 3),
                (0, 4),
                (2, 3),
                (2, 4),
                (4, 3),
                (4, 4),
                (6, 3),
                (6, 4),
                (8, 3),
                (8, 4),
            ],
        )
    }
}

impl WideVariant<Xiangqi9x10> for SupplyRules {
    /// Same army as Xiangqi (drops re-deploy the existing roles; no new role is
    /// fielded), so the role span is Xiangqi's. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 23;

    fn starting_position() -> (Board<Xiangqi9x10>, GenericState<Xiangqi9x10>) {
        // The board and state are Xiangqi's; the empty hand (`placement = NONE`) is
        // rendered as the `[]` bracket by `has_hand()`.
        XiangqiRules::starting_position()
    }

    // --- movement / king-safety / terminal rules: all Xiangqi's ----------------

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Xiangqi9x10>,
        occupancy: Bitboard<Xiangqi9x10>,
    ) -> Bitboard<Xiangqi9x10> {
        XiangqiRules::role_attacks(role, color, sq, occupancy)
    }

    fn quiet_only_targets(
        role: WideRole,
        color: Color,
        sq: Square<Xiangqi9x10>,
        occupancy: Bitboard<Xiangqi9x10>,
    ) -> Bitboard<Xiangqi9x10> {
        XiangqiRules::quiet_only_targets(role, color, sq, occupancy)
    }

    fn role_attack_is_leg_asymmetric(role: WideRole) -> bool {
        XiangqiRules::role_attack_is_leg_asymmetric(role)
    }

    fn role_is_slider(role: WideRole) -> bool {
        XiangqiRules::role_is_slider(role)
    }

    fn royal_slider_kind(role: WideRole) -> Option<RoyalSlider> {
        XiangqiRules::royal_slider_kind(role)
    }

    fn royal_reach_superset(
        role: WideRole,
        king: Square<Xiangqi9x10>,
    ) -> Option<Bitboard<Xiangqi9x10>> {
        XiangqiRules::royal_reach_superset(role, king)
    }

    fn has_castling() -> bool {
        false
    }

    fn has_cannons() -> bool {
        // Supply fields Xiangqi cannons, so it takes the pseudo-legal + per-move
        // verify king-safety path; the same path generates and verifies hand drops.
        true
    }

    fn has_flying_general() -> bool {
        true
    }

    fn king_diag_attack_radius() -> Option<u8> {
        XiangqiRules::king_diag_attack_radius()
    }

    fn extra_royal_attack<const R: usize>(
        board: &Board<Xiangqi9x10, R>,
        sq: Square<Xiangqi9x10>,
        by: Color,
        occupied: Bitboard<Xiangqi9x10>,
    ) -> bool {
        XiangqiRules::extra_royal_attack(board, sq, by, occupied)
    }

    // Repetition / perpetual-check + stalemate: Xiangqi's (FSF `supply_variant` is
    // built on `xiangqi_variant_base`, which sets `stalemateValue = -VALUE_MATE`,
    // `perpetualCheckIllegal = true` and `flyingGeneral = true`). Supply does **not**
    // inherit the `chasingRule` (that is set only by `xiangqi_variant()`), so
    // `perpetual_chase_loses` stays at its `false` default. Adjudication only — perft
    // is unaffected.

    fn tracks_repetition() -> bool {
        true
    }

    fn perpetual_check_loses() -> bool {
        true
    }

    fn stalemate_is_loss() -> bool {
        true
    }

    // --- hand / drops ----------------------------------------------------------

    fn has_hand() -> bool {
        true
    }

    fn pawn_is_stepper() -> bool {
        // Supply fields no Pawn role (its foot soldiers are the Xiangqi Soldier,
        // handled in `role_attacks`), so the forward-stepper Pawn routing is inert;
        // pinned `false` rather than the `has_hand()` default for clarity.
        false
    }

    fn drop_check_forbidden() -> bool {
        // FSF `supply_variant` sets `dropChecks = false`: a drop may not give check.
        true
    }

    fn captures_to_hand() -> bool {
        // FSF `supply_variant` leaves `capturesToHand = false`: it is a two-board
        // (`twoBoards`) game whose hand is fed by the *partner* board, never by a
        // capture on this board. On a single board the hand is thus only ever what
        // the FEN seeds (empty at the start), so ordinary play never drops and Supply
        // perft matches Xiangqi.
        false
    }

    fn drop_targets<const R: usize>(
        role: WideRole,
        color: Color,
        board: &Board<Xiangqi9x10, R>,
    ) -> Bitboard<Xiangqi9x10> {
        // A held piece drops onto an empty own-half square where it could legally
        // stand: the Chariot / Horse / Cannon anywhere in the own half, the Advisor /
        // Elephant / Soldier only on their own-half `mobilityRegion` points. The
        // General is royal and never banked, so it is never dropped.
        let region = match role {
            WideRole::Rook | WideRole::Horse | WideRole::Cannon => Self::own_half(color),
            WideRole::Advisor => Self::advisor_region(color),
            WideRole::XiangqiElephant => Self::elephant_region(color),
            WideRole::Soldier => Self::soldier_region(color),
            _ => Bitboard::EMPTY,
        };
        region & !board.occupied()
    }
}

/// Supply (Xiangqi with drops) as a [`GenericPosition`] over the 9x10
/// [`Xiangqi9x10`] geometry.
///
/// Construct the starting position with
/// [`Supply::startpos`](GenericPosition::startpos) or parse a FEN (mcr dialect, with
/// a `[…]` holdings bracket) with [`Supply::from_fen`](GenericPosition::from_fen).
/// See the [module docs](self) for the Xiangqi movement it inherits and the own-half
/// drop rule it adds.
pub type Supply = GenericPosition<
    Xiangqi9x10,
    SupplyRules,
    { <SupplyRules as WideVariant<Xiangqi9x10>>::ROLE_SPAN },
>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::position::WideOutcome;
    use crate::geometry::{perft as gperft, Xiangqi};

    const STARTPOS: &str =
        "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/9/RJOUKUOJR[] w - - 0 1";

    /// The canonical start FEN round-trips through the mcr dialect, seeding an empty
    /// hand (`[]`).
    #[test]
    fn startpos_round_trips() {
        let pos = Supply::startpos();
        assert_eq!(pos.to_fen(), STARTPOS);
        assert_eq!(pos.hand_count(Color::White, WideRole::Rook), 0);
    }

    /// With an empty hand and `capturesToHand = false`, Supply generates no drop in
    /// ordinary play, so its move tree is Xiangqi's node-for-node — the internal
    /// cross-check that pins the "Supply perft == Xiangqi perft" equivalence.
    #[test]
    fn startpos_perft_matches_xiangqi() {
        let supply = Supply::startpos();
        let xiangqi = Xiangqi::startpos();
        for depth in 1..=3 {
            assert_eq!(
                gperft::<Xiangqi9x10, _, _>(&supply, depth),
                gperft::<Xiangqi9x10, _, _>(&xiangqi, depth),
                "Supply perft({depth}) must equal Xiangqi perft({depth})",
            );
        }
    }

    /// Collects the destination squares (the part after `@`) of every drop move.
    fn drop_dests(fen: &str) -> Vec<String> {
        let pos = Supply::from_fen(fen).expect("valid supply FEN");
        pos.legal_moves()
            .iter()
            .filter(|m| m.is_drop())
            .map(|m| {
                m.to_uci::<Xiangqi9x10>()
                    .split('@')
                    .nth(1)
                    .expect("drop UCI is ROLE@square")
                    .to_string()
            })
            .collect()
    }

    /// A held Chariot drops onto any empty own-half square, but never past the river
    /// (the own-half `dropRegion`).
    #[test]
    fn chariot_drops_only_in_own_half() {
        // White to move, a Chariot in hand, kings parked out of the way.
        let dests = drop_dests("5k3/9/9/9/9/9/9/9/9/3K5[R] w - - 0 1");
        assert!(!dests.is_empty(), "a held chariot must have drop targets");
        // Every target is on ranks 1..5 (own half); a3/e5 in, e6/i10 out.
        assert!(dests.iter().any(|d| d == "a3"), "own-half a3 is a target");
        assert!(dests.iter().any(|d| d == "e5"), "own-half e5 is a target");
        assert!(
            !dests.iter().any(|d| d == "e6"),
            "a chariot may not be dropped past the river"
        );
        assert!(
            !dests.iter().any(|d| d == "e10"),
            "a chariot may not be dropped onto the far back rank"
        );
    }

    /// A held Advisor may be dropped only onto a palace diagonal point (e2), never
    /// onto an arbitrary own-half square (e4).
    #[test]
    fn advisor_drops_only_on_palace_points() {
        let dests = drop_dests("5k3/9/9/9/9/9/9/9/9/3K5[U] w - - 0 1");
        assert!(
            dests.iter().any(|d| d == "e2"),
            "an advisor may be dropped onto the palace point e2"
        );
        assert!(
            !dests.iter().any(|d| d == "e4"),
            "an advisor may not be dropped off the palace onto e4"
        );
    }

    /// A held Soldier may be dropped onto a pre-river residence (a4) but not onto a
    /// palace square (e1) or a non-Soldier file.
    #[test]
    fn soldier_drops_only_on_residences() {
        let dests = drop_dests("5k3/9/9/9/9/9/9/9/9/3K5[Z] w - - 0 1");
        assert!(
            dests.iter().any(|d| d == "a4"),
            "a soldier may be dropped onto the residence a4"
        );
        assert!(
            !dests.iter().any(|d| d == "b4"),
            "a soldier may not be dropped onto the non-file square b4"
        );
        assert!(
            !dests.iter().any(|d| d == "e1"),
            "a soldier may not be dropped onto the back rank e1"
        );
    }

    /// Xiangqi movement is intact: stalemate is a **loss** for the side to move (as
    /// in Xiangqi / FSF `stalemateValue = -VALUE_MATE`).
    #[test]
    fn supply_stalemate_is_a_loss() {
        let pos =
            Supply::from_fen("4k4/R8/9/9/9/9/9/5R3/9/3R1K3[] b - - 0 1").expect("valid supply FEN");
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
