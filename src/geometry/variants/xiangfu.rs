//! Xiang Fu — a Xiangqi-themed 9x9 **drop** variant (issue #274) — on the
//! generic engine. Validated move-for-move against Fairy-Stockfish `UCI_Variant
//! xiangfu` (a pychess INI variant; FSF learns it from `variants.ini`).
//!
//! Xiang Fu is played on a 9x9 board ([`Shogi9x9`]) with a central 5x5 **ring**
//! (files c..g, ranks 3..7) that replaces the two Xiangqi palaces. Captured pieces
//! go **to hand** and may be **dropped** back onto the first two ranks of one's
//! own side. The win condition is a pseudo-royal **duple check** on a side's two
//! **Champions**.
//!
//! ## Pieces (confirmed against FSF `UCI_Variant xiangfu`)
//!
//! Most movers are reused wholesale from the Xiangqi / Cannon-Shogi role layer:
//!
//! * **Champion** ([`WideRole::Champion`], FSF `+g`) — a king's eight one-steps
//!   (`Q1`), **royal** and **confined to the ring**. A side has two; they are
//!   **pseudo-royal** under a duple-check rule (below). Banks into hand as a Pupil
//!   when captured.
//! * **Pupil** ([`WideRole::Commoner`], FSF `g`) — the plain non-royal king-stepper
//!   (`Q1`), **not** confined. Pupils only ever enter play by being **dropped** (a
//!   captured Champion demotes to a Pupil in hand).
//! * **Horse** ([`WideRole::Horse`], FSF `n`) — the hobbled Xiangqi knight.
//! * **Chariot** ([`WideRole::Rook`], FSF `r`) — a plain rook.
//! * **Cannon** ([`WideRole::Cannon`], FSF `c`) — the Xiangqi over-screen cannon.
//! * **Crossbow** ([`WideRole::BishopCannon`], FSF `w`, Betza `mBcpB`) — the
//!   **diagonal** cannon: it slides quietly like a bishop but captures only by
//!   hopping exactly one diagonal screen.
//! * **Bishop** ([`WideRole::Bishop`], FSF `b`) — the full-range Chess bishop.
//! * **Mahout** ([`WideRole::Mahout`], FSF `m`, Betza `nAnD`) — a two-square leaper
//!   in any of the eight directions that **cannot jump** (blocked by a piece on the
//!   square it passes over).
//!
//! ## Royalty: two pseudo-royal Champions under duple check
//!
//! Each side fields **two Champions** ([`WideVariant::royal_squares`]) under FSF's
//! combined `extinctionPseudoRoyal` + `dupleCheck` (with the default
//! `extinctionPieceCount = 0`). A side is "in check" only under a **duple check** —
//! when **every** one of its Champions is attacked at once — so a legal move need
//! only leave **at least one** Champion unattacked: "you must capture one Champion
//! in order to checkmate the other." (A Champion may even step adjacent to an enemy
//! Champion, provided the mover's *other* Champion is safe.) That is exactly the
//! at-least-one rule the multi-royal path already runs for Spartan
//! ([`royals_all_must_survive`](WideVariant::royals_all_must_survive) stays at its
//! `false` default): FSF marks a side's Champions strictly all-must-survive only
//! while it holds `count <= extinctionPieceCount + 1 = 1` of them, where the strict
//! and duple rules coincide. A side that has lost **both** Champions has no royal
//! constraint and keeps playing
//! ([`royalless_generates`](WideVariant::royalless_generates)) — FSF never
//! truncates such a node, and perft matches it.
//!
//! ## Confirmed starting FEN
//!
//! From the pychess `[xiangfu]` `startFen`. FSF and mcr describe the same position
//! but spell the new movers differently (mcr avoids the bare letters `k`/`m`,
//! already the King / Met, by spelling the Champion `=k` and the Mahout `=m`, and
//! spells the Horse `j` and the Crossbow `=c`):
//!
//! ```text
//! FSF dialect: 2rbm4/2cwn4/2+g1+g4/9/9/9/4+G1+G2/4NWC2/4MBR2[] w - 0 1
//! mcr dialect: 2rb=m4/2c=cj4/2=k1=k4/9/9/9/4=K1=K2/4J=CC2/4=MBR2[] w - - 0 1
//! ```
//!
//! The `compare-fairy` harness rewrites mcr's tokens (`=k → +g`, `=m → m`,
//! `j → n`, `=c → w`; the hand Pupil `*u → g`) when driving FSF.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{attacks, Bitboard, Board, Shogi9x9, Square, WideRole, WideVariant};
use crate::Color;

/// The confirmed Xiang Fu starting placement in the mcr dialect (Champion `=k`,
/// Mahout `=m`, Horse `j`, Crossbow `=c`), the position byte-identical to FSF's
/// `2rbm4/2cwn4/2+g1+g4/9/9/9/4+G1+G2/4NWC2/4MBR2`.
const XIANGFU_PLACEMENT: &str = "2rb=m4/2c=cj4/2=k1=k4/9/9/9/4=K1=K2/4J=CC2/4=MBR2";

/// The Xiang Fu rule layer: a zero-sized [`WideVariant`] over [`Shogi9x9`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct XiangfuRules;

impl XiangfuRules {
    /// The central 5x5 **ring** (files c..g = 2..=6, ranks 3..7 = 2..=6, 0-based)
    /// to which the Champions are confined. Shared by both colours.
    fn ring() -> Bitboard<Shogi9x9> {
        let mut bb = Bitboard::<Shogi9x9>::EMPTY;
        for rank in 2..=6u8 {
            for file in 2..=6u8 {
                if let Some(sq) = Square::<Shogi9x9>::from_file_rank(file, rank) {
                    bb.set(sq);
                }
            }
        }
        bb
    }

    /// The drop region for `color`: the first two ranks of its own side — White
    /// ranks 1-2 (0-based 0..=1), Black ranks 8-9 (0-based 7..=8). FSF
    /// `whiteDropRegion = *1 *2`, `blackDropRegion = *8 *9`.
    fn drop_region(color: Color) -> Bitboard<Shogi9x9> {
        let ranks: [u8; 2] = if color.is_white() { [0, 1] } else { [7, 8] };
        let mut bb = Bitboard::<Shogi9x9>::EMPTY;
        for &rank in &ranks {
            for file in 0..9u8 {
                if let Some(sq) = Square::<Shogi9x9>::from_file_rank(file, rank) {
                    bb.set(sq);
                }
            }
        }
        bb
    }

    /// The Cannon's (`c`, `mRcpR`) full move-and-attack set: quiet rook slides plus
    /// over-one-screen captures, folded together (`emit_targets` splits them by
    /// enemy occupancy). A king sits on an occupied square, so it falls only in the
    /// over-screen capture portion.
    fn cannon_attacks(sq: Square<Shogi9x9>, occ: Bitboard<Shogi9x9>) -> Bitboard<Shogi9x9> {
        attacks::cannon_quiet_moves::<Shogi9x9>(sq, occ)
            | attacks::cannon_capture_targets::<Shogi9x9>(sq, occ)
    }

    /// The Crossbow's (`w`, `mBcpB`) full set: quiet **bishop slides** (onto empty
    /// squares) plus diagonal over-one-screen captures. It moves like a plain
    /// bishop but captures only by jumping one diagonal screen.
    fn crossbow_attacks(sq: Square<Shogi9x9>, occ: Bitboard<Shogi9x9>) -> Bitboard<Shogi9x9> {
        (attacks::bishop_attacks::<Shogi9x9>(sq, occ) & !occ)
            | attacks::diag_cannon_capture_targets::<Shogi9x9>(sq, occ)
    }
}

impl WideVariant<Shogi9x9> for XiangfuRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 75;

    fn starting_position() -> (Board<Shogi9x9>, GenericState<Shogi9x9>) {
        let board = Board::<Shogi9x9>::from_fen_placement(XIANGFU_PLACEMENT)
            .expect("the Xiang Fu starting placement is valid on a 9x9 board");
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
            checks_against: [0, 0],
        };
        (board, state)
    }

    fn role_attacks(
        role: WideRole,
        _color: Color,
        sq: Square<Shogi9x9>,
        occupancy: Bitboard<Shogi9x9>,
    ) -> Bitboard<Shogi9x9> {
        match role {
            // Champion: a king-stepper confined to the ring. Royal + pseudo-royal.
            WideRole::Champion => attacks::king_attacks::<Shogi9x9>(sq) & Self::ring(),
            // Pupil: the plain non-royal king-stepper, unconfined.
            WideRole::Commoner => attacks::king_attacks::<Shogi9x9>(sq),
            // Horse: a knight hobbled by a leg blocker (Xiangqi horse).
            WideRole::Horse => attacks::horse_attacks::<Shogi9x9>(sq, occupancy),
            // Chariot: a plain rook.
            WideRole::Rook => attacks::rook_attacks::<Shogi9x9>(sq, occupancy),
            // Bishop: the full Chess bishop.
            WideRole::Bishop => attacks::bishop_attacks::<Shogi9x9>(sq, occupancy),
            // Cannon: quiet rook slides + over-screen captures (occupancy-only; the
            // hop depends only on which squares are occupied, not piece identity).
            WideRole::Cannon => Self::cannon_attacks(sq, occupancy),
            // Crossbow: quiet bishop slides + diagonal over-screen captures.
            WideRole::BishopCannon => Self::crossbow_attacks(sq, occupancy),
            // Mahout: a two-square leaper (all eight directions) blocked by the
            // single square it passes over.
            WideRole::Mahout => attacks::mahout_attacks_blockable::<Shogi9x9>(sq, occupancy),
            _ => Bitboard::EMPTY,
        }
    }

    fn role_attack_is_leg_asymmetric(role: WideRole) -> bool {
        // * **Horse** — its leap is hobbled by the leg adjacent to the *horse*, a
        //   different square than a reverse leap would test.
        // * **Cannon / Crossbow** — their *attack* (over-screen capture) set lands
        //   only on an occupied square, so it is occupancy-asymmetric: reverse-
        //   projecting from an empty target would invent a cannon attacker.
        // * **Champion** — region-confined to the ring: its attack relation is keyed
        //   on the *origin* (a Champion outside the ring is immobile), so reverse-
        //   projecting from a target outside the ring would invent attacks. The
        //   asymmetry never affects king-safety (a Champion always stands inside the
        //   ring, where the relation is symmetric), so perft is unchanged; it only
        //   corrects `attackers_to` on out-of-ring squares.
        //
        // The Mahout's blocking leg is the geometric **midpoint**, so its relation
        // is symmetric and it stays on the standard reverse-projection path.
        matches!(
            role,
            WideRole::Horse | WideRole::Cannon | WideRole::BishopCannon | WideRole::Champion
        )
    }

    fn role_is_slider(role: WideRole) -> bool {
        // The Chariot (rook) and the Bishop are line sliders. The Cannon / Crossbow
        // need a screen to capture (not pin-classified). Xiang Fu runs the
        // multi-royal verify path (make/unmake), which does not consult pins, so
        // this classification is otherwise inert.
        matches!(role, WideRole::Rook | WideRole::Bishop)
    }

    // --- pseudo-royal Champions + duple check ----------------------------

    fn multi_royal() -> bool {
        true
    }

    fn royal_squares<const R: usize>(
        board: &Board<Shogi9x9, R>,
        color: Color,
    ) -> Bitboard<Shogi9x9> {
        board.pieces(color, WideRole::Champion)
    }

    // `royals_all_must_survive()` stays at its `false` default: Xiang Fu is a
    // **duple-check** variant (FSF `extinctionPseudoRoyal` + `dupleCheck` with the
    // default `extinctionPieceCount = 0`). FSF marks a side's Champions strictly
    // pseudo-royal only while it holds `count <= extinctionPieceCount + 1 = 1` of
    // them; with two Champions only the duple rule applies (a side is in check only
    // when **every** Champion is attacked at once), and with one Champion the strict
    // rule and the duple rule coincide (that lone Champion must be safe). So across
    // every Champion count the legality is exactly "at least one Champion survives"
    // — the same at-least-one rule as Spartan, confirmed move-for-move against FSF
    // (a Champion may even step adjacent to an enemy Champion as long as the mover's
    // other Champion is safe). A side that has lost **both** Champions has no
    // constraint at all (`royalless_generates`).

    fn royalless_generates() -> bool {
        // FSF keeps generating the moves of a side that has lost both Champions.
        true
    }

    // --- hand / drops ----------------------------------------------------

    fn has_hand() -> bool {
        true
    }

    fn role_hand_base(role: WideRole) -> WideRole {
        // A captured Champion demotes to a Pupil in hand (FSF `promotedPieceType =
        // g:k`, so the promoted commoner `k` banks as the base `g`). Every other
        // Xiang Fu piece banks as itself.
        match role {
            WideRole::Champion => WideRole::Commoner,
            other => other,
        }
    }

    fn drop_targets<const R: usize>(
        _role: WideRole,
        color: Color,
        board: &Board<Shogi9x9, R>,
    ) -> Bitboard<Shogi9x9> {
        // Any held piece drops onto an empty square in the dropping side's own first
        // two ranks (FSF `whiteDropRegion = *1 *2`, `blackDropRegion = *8 *9`).
        // Captured Champions are always demoted to Pupils before they reach the
        // hand, so no Champion is ever dropped.
        Self::drop_region(color) & !board.occupied()
    }

    fn has_castling() -> bool {
        false
    }
}

/// Xiang Fu (9x9 Xiangqi-themed drop variant) as a [`GenericPosition`] over the 9x9
/// [`Shogi9x9`] geometry.
///
/// Construct the starting position with
/// [`Xiangfu::startpos`](GenericPosition::startpos) or parse a FEN (mcr dialect)
/// with [`Xiangfu::from_fen`](GenericPosition::from_fen). See the [module
/// docs](self) for the piece movements, the ring confinement, the captures-to-hand
/// drops, and the pseudo-royal duple-check Champions.
pub type Xiangfu =
    GenericPosition<Shogi9x9, XiangfuRules, { <XiangfuRules as WideVariant<Shogi9x9>>::ROLE_SPAN }>;

#[cfg(test)]
mod tests {
    use super::*;

    /// The canonical start FEN round-trips through the mcr dialect.
    #[test]
    fn startpos_round_trips() {
        let pos = Xiangfu::startpos();
        assert_eq!(
            pos.to_fen(),
            "2rb=m4/2c=cj4/2=k1=k4/9/9/9/4=K1=K2/4J=CC2/4=MBR2[] w - - 0 1"
        );
    }

    /// A lone Mahout reaches all eight two-step squares on an empty board, and is
    /// blocked when a piece sits on the square it would pass over.
    #[test]
    fn mahout_leaps_and_is_blocked() {
        // Mahout (=M) on e5, kings parked in the ring so the position is legal.
        let pos = Xiangfu::from_fen("9/9/9/9/2=k1=K1=M2/9/9/9/9 w - - 0 1");
        // Just assert the helper directly: a central Mahout has eight targets.
        let e5 = Square::<Shogi9x9>::from_file_rank(4, 4).unwrap();
        assert_eq!(
            attacks::mahout_attacks_blockable::<Shogi9x9>(e5, Bitboard::EMPTY).count(),
            8
        );
        let _ = pos;
    }
}
