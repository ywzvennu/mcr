//! Minixiangqi (7x7) on the generic engine — a compact reduction of Xiangqi
//! (#187) onto a new 49-square [`Minixiangqi7x7`] `u128` geometry. It reuses the
//! Xiangqi **cannon**, **horse**, **palace**, and **flying-general** machinery
//! (issue #196) but drops the river, advisors, and elephants. Validated
//! move-for-move against Fairy-Stockfish `UCI_Variant minixiangqi`.
//!
//! Minixiangqi is played on a 7-files (a..g) by 7-ranks (1..7) board — mcr's
//! [`Minixiangqi7x7`] geometry — with pieces on the cells. Each general lives in
//! a 3x3 **palace** (files c..e, the near three ranks). There is **no river**, so
//! soldiers may step sideways from the start.
//!
//! ## Pieces (confirmed against FSF `UCI_Variant minixiangqi`)
//!
//! * **General / King** ([`WideRole::King`], FSF `k`): one orthogonal step,
//!   **confined to the palace**. Plus the **flying-general** rule (below).
//! * **Horse** ([`WideRole::Horse`], mcr `j`, FSF `n`): a knight leap **hobbled**
//!   if the orthogonally-adjacent leg square is occupied. Reuses
//!   [`attacks::horse_attacks`] — the *same* primitive as Xiangqi.
//! * **Chariot / Rook** ([`WideRole::Rook`], `r`): a plain rook.
//! * **Cannon** ([`WideRole::Cannon`], `c`): moves like a rook over empty
//!   squares, captures only by jumping exactly one screen — the **same** cannon
//!   primitive as Xiangqi and Shako ([`attacks::cannon_quiet_moves`] /
//!   [`attacks::cannon_capture_targets`]), confirmed identical against FSF.
//! * **Soldier / Pawn** ([`WideRole::Soldier`], mcr `z`, FSF `p`): one step
//!   forward **plus** one step sideways. Minixiangqi has **no river**, so the
//!   sideways step is available everywhere (unlike Xiangqi, where it unlocks only
//!   after crossing). Never backward, no double-step, no promotion.
//!
//! ## Flying general
//!
//! As in Xiangqi, the two generals may not face each other on a file with no
//! piece between — modelled through the default-off
//! [`WideVariant::extra_royal_attack`] hook, reused unchanged.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's `minixiangqi` variant (`startFen`); mcr and FSF agree on the
//! position but spell the Horse and Soldier differently (mcr avoids `n`/`p`,
//! already taken by the Knight / Pawn), so the `compare-fairy` harness rewrites
//! them when driving FSF:
//!
//! ```text
//! FSF dialect: rcnkncr/p1ppp1p/7/7/7/P1PPP1P/RCNKNCR w - - 0 1
//! mcr dialect: rcjkjcr/z1zzz1z/7/7/7/Z1ZZZ1Z/RCJKJCR w - - 0 1
//! ```

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Minixiangqi7x7, RoyalSlider, Square, WideRole, WideVariant,
};
use crate::Color;

/// The Minixiangqi rule layer: a zero-sized [`WideVariant`] over
/// [`Minixiangqi7x7`].
///
/// It overrides only what Minixiangqi changes from the generic engine: the 7x7
/// starting array, the palace-confined General, the blockable Horse, the
/// always-sideways Soldier, the cannon (reused from the shared primitive), the
/// flying-general king-safety, and the no-castling / no-promotion /
/// no-en-passant rules. There are no advisors or elephants, and no river.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MinixiangqiRules;

/// The confirmed Minixiangqi starting placement in the mcr dialect (horse `j`,
/// soldier `z`), the position byte-identical to FSF's
/// `rcnkncr/p1ppp1p/7/7/7/P1PPP1P/RCNKNCR`.
const MINIXIANGQI_START_PLACEMENT: &str = "rcjkjcr/z1zzz1z/7/7/7/Z1ZZZ1Z/RCJKJCR";

/// The four wazir (one orthogonal step) offsets — the General's movement.
const WAZIR_OFFSETS: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

impl MinixiangqiRules {
    /// The palace mask for `color`: the 3x3 block on files c..e (2..=4), on the
    /// three ranks nearest that color (ranks 1..3 for White, 5..7 for Black).
    fn palace(color: Color) -> Bitboard<Minixiangqi7x7> {
        let ranks: [u8; 3] = match color {
            Color::White => [0, 1, 2],
            Color::Black => [4, 5, 6],
        };
        let mut bb = Bitboard::<Minixiangqi7x7>::EMPTY;
        for &rank in &ranks {
            for file in 2..=4u8 {
                if let Some(sq) = Square::<Minixiangqi7x7>::from_file_rank(file, rank) {
                    bb.set(sq);
                }
            }
        }
        bb
    }

    /// The Soldier's move/attack squares for `color` on `sq`: one step forward
    /// and one step sideways (left and right). Minixiangqi has no river, so the
    /// sideways step is always available. Never backward; a soldier never
    /// promotes. The set is the same for moving and for capturing (a soldier
    /// captures wherever it can move).
    fn soldier_targets(color: Color, sq: Square<Minixiangqi7x7>) -> Bitboard<Minixiangqi7x7> {
        let forward: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = Bitboard::<Minixiangqi7x7>::EMPTY;
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

impl WideVariant<Minixiangqi7x7> for MinixiangqiRules {
    /// The tightest prefix of [`WideRole::ALL`] that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 23;

    fn starting_position() -> (Board<Minixiangqi7x7>, GenericState<Minixiangqi7x7>) {
        let board = Board::<Minixiangqi7x7>::from_fen_placement(MINIXIANGQI_START_PLACEMENT)
            .expect("the Minixiangqi starting placement is valid on a 7x7 board");
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
        sq: Square<Minixiangqi7x7>,
        occupancy: Bitboard<Minixiangqi7x7>,
    ) -> Bitboard<Minixiangqi7x7> {
        match role {
            // General: a wazir confined to the palace. (The flying-general
            // file-attack is handled separately in `extra_royal_attack`.)
            WideRole::King => {
                attacks::leaper_attacks::<Minixiangqi7x7>(sq, &WAZIR_OFFSETS) & Self::palace(color)
            }
            // Horse: a knight hobbled by a leg blocker.
            WideRole::Horse => attacks::horse_attacks::<Minixiangqi7x7>(sq, occupancy),
            // Chariot: a plain rook.
            WideRole::Rook => attacks::rook_attacks::<Minixiangqi7x7>(sq, occupancy),
            // Cannon: its *attack* set is the over-one-screen capture set; the
            // quiet rook-rays come through `quiet_only_targets`.
            WideRole::Cannon => attacks::cannon_capture_targets::<Minixiangqi7x7>(sq, occupancy),
            // Soldier: forward plus sideways (no river). It captures wherever it
            // moves, so the attack set equals the move set.
            WideRole::Soldier => Self::soldier_targets(color, sq),
            // No other role is fielded in Minixiangqi.
            _ => Bitboard::EMPTY,
        }
    }

    fn quiet_only_targets(
        role: WideRole,
        _color: Color,
        sq: Square<Minixiangqi7x7>,
        occupancy: Bitboard<Minixiangqi7x7>,
    ) -> Bitboard<Minixiangqi7x7> {
        match role {
            // The Cannon's non-capturing moves: the empty rook-ray squares. (Its
            // captures are the over-screen set in `role_attacks`.)
            WideRole::Cannon => attacks::cannon_quiet_moves::<Minixiangqi7x7>(sq, occupancy),
            _ => Bitboard::EMPTY,
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // The Soldier's attack set is forward-biased (it captures one step toward
        // the enemy, and — Minixiangqi having no river — also sideways). Its
        // forward step points the *opposite* way for each color, so `attackers_to`
        // must reverse-project the *opposing*-color soldier pattern from the
        // target to find the soldiers attacking it (the sideways component is
        // symmetric, so flipping the color leaves it unchanged). Without this the
        // king-safety scan would miss a soldier guarding the square directly ahead
        // of it and let the enemy king step into capture.
        matches!(role, WideRole::Soldier)
    }

    fn role_attack_is_leg_asymmetric(role: WideRole) -> bool {
        // The Horse's leap is hobbled by the leg adjacent to the *horse* toward
        // the leap — a per-leap geometric asymmetry that reverse-projection from
        // the target cannot resolve, so attacker detection must project forward
        // from each horse (the same handling as Xiangqi; Minixiangqi has no
        // elephant). Reusing #199's hook keeps `attackers_to` symmetric for the
        // hobbled horse.
        //
        // The Cannon is also leg-asymmetric: its over-screen capture set lands
        // only on an occupied square, so reverse-projecting from an empty target
        // reports a phantom cannon attacker there. Forward-projecting from each
        // cannon keeps `attackers_to` the true forward relation on every square
        // (the same handling as Xiangqi; issue #202).
        //
        // The General (King) is **palace-confined**: its attack set is masked to
        // the palace by its *origin*, so reverse-projecting from a target outside
        // the palace invents an attack from an in-palace general that cannot
        // actually reach it. Forward-projection from each general matches the real
        // geometry on every square. This never affects king-safety (a royal square
        // is always inside its own palace), so perft is unchanged. (Issue #202.)
        matches!(role, WideRole::Horse | WideRole::Cannon | WideRole::King)
    }

    fn role_is_slider(role: WideRole) -> bool {
        // Only the Chariot (rook) is a line slider that can pin. The Cannon is not
        // (it needs a screen to capture); the Horse and Soldier are
        // steppers/leapers. Minixiangqi runs the cannon verify path, which does
        // not consult pins, but the classification is kept honest.
        matches!(role, WideRole::Rook)
    }

    fn royal_slider_kind(role: WideRole) -> Option<RoyalSlider> {
        // The Chariot is the plain standard rook (`role_attacks` is exactly
        // `rook_attacks`), so the cannon king-safety verify reuses the king's
        // precomputed line masks. No diagonal sliders; every other role is a leaper
        // or the asymmetric Cannon, which keep the forward path.
        matches!(role, WideRole::Rook).then_some(RoyalSlider::Rook)
    }

    fn royal_reach_superset(
        role: WideRole,
        king: Square<Minixiangqi7x7>,
    ) -> Option<Bitboard<Minixiangqi7x7>> {
        // Supersets (occupancy-independent, ignoring legs / palace confinement /
        // cannon screens) of the squares from which each forward-projected role could
        // attack the king. Minixiangqi has no river and no elephant; the Soldier is
        // handled by the directional reverse-projection (no superset needed).
        match role {
            // Horse: the knight-shape neighbourhood of the king.
            WideRole::Horse => Some(attacks::knight_attacks::<Minixiangqi7x7>(king)),
            // General (King): attacks only from a square adjacent to the king.
            WideRole::King => Some(attacks::king_attacks::<Minixiangqi7x7>(king)),
            // Cannon: an over-screen orthogonal ray, so its sources lie on the king's
            // rank/file.
            WideRole::Cannon => Some(attacks::rook_attacks::<Minixiangqi7x7>(
                king,
                Bitboard::EMPTY,
            )),
            _ => None,
        }
    }

    fn has_castling() -> bool {
        false
    }

    fn has_cannons() -> bool {
        // Minixiangqi fields cannons, so it takes the pseudo-legal + per-move
        // verify king-safety path (the cannon's check and king-danger are
        // screen-dependent). The flying-general extra attack rides the same
        // verify.
        true
    }

    fn has_flying_general() -> bool {
        true
    }

    fn extra_royal_attack(
        board: &Board<Minixiangqi7x7>,
        sq: Square<Minixiangqi7x7>,
        by: Color,
        occupied: Bitboard<Minixiangqi7x7>,
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
        (attacks::between::<Minixiangqi7x7>(general, sq) & occupied).is_empty()
    }

    // --- Repetition / perpetual check (terminal only; perft unaffected) ----

    fn tracks_repetition() -> bool {
        true
    }

    fn perpetual_check_loses() -> bool {
        true
    }

    // Minixiangqi deliberately does **not** set `perpetual_chase_loses` (issue #475):
    // Fairy-Stockfish's `minixiangqi_variant()` enables `perpetualCheckIllegal` but
    // leaves `chasingRule` at its `none` default (only full `xiangqi_variant()` sets
    // `AXF_CHASING`), so the perpetual-chase rule applies to Xiangqi alone. mcr
    // matches that: the default `perpetual_chase_loses() == false` is inherited here.
}

/// Minixiangqi as a [`GenericPosition`] over the 7x7 [`Minixiangqi7x7`] geometry.
///
/// Construct the starting position with
/// [`Minixiangqi::startpos`](GenericPosition::startpos) or parse a FEN (mcr
/// dialect) with [`Minixiangqi::from_fen`](GenericPosition::from_fen). See the
/// [module docs](self) for the piece movements, the palace confinement, and the
/// flying-general rule.
pub type Minixiangqi = GenericPosition<Minixiangqi7x7, MinixiangqiRules>;
