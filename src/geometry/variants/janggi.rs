//! Janggi (Korean chess, 9x10) on the generic engine — the third marquee fairy
//! variant, sharing Xiangqi's [`Xiangqi9x10`] `u128` geometry and palace but
//! differing fundamentally in almost every piece. Validated move-for-move against
//! Fairy-Stockfish `UCI_Variant janggi`.
//!
//! Janggi is played on the same 9-files (a..i) by 10-ranks (1..10) board as
//! Xiangqi, with a 3x3 **palace** (files d..f) on each side's near three ranks —
//! but, unlike Xiangqi, there is **no river**, and the palace carries **diagonal
//! lines** (the palace "X": centre joined to all four corners) along which several
//! pieces move.
//!
//! ## Pieces (confirmed against FSF)
//!
//! * **General / King** ([`WideRole::King`], FSF `k`): one orthogonal step within
//!   the palace, **plus** a one-step move along a palace diagonal line (centre ↔
//!   corner). Starts on the palace **centre** (e2 / e9), not the back rank.
//! * **Guard / Advisor** ([`WideRole::Advisor`], mce `u`, FSF `a`): identical
//!   movement to the General (palace-confined wazir + palace-diagonal step). Two
//!   per side.
//! * **Chariot / Rook** ([`WideRole::Rook`], `r`): a rook, **plus** — when it
//!   stands on a palace diagonal point — sliding along that palace diagonal line
//!   (corner-through-centre-to-corner).
//! * **Cannon (포)** ([`WideRole::Cannon`], `c`): moves **and** captures **only**
//!   by jumping exactly one **screen**; it cannot move along an empty ray at all.
//!   The screen may **not** be a cannon, and a cannon may **not** capture a cannon.
//!   It may also jump a palace diagonal when a screen sits on the palace centre.
//!   Uses the [`attacks::janggi_cannon_quiet`] / [`attacks::janggi_cannon_capture`]
//!   primitives plus the palace-diagonal jump computed here, all from the live
//!   board via the default-off [`WideVariant::role_attacks_board`] hook.
//! * **Horse (馬)** ([`WideRole::Horse`], mce `j`, FSF `n`): the Xiangqi hobbled
//!   knight — reuses [`attacks::horse_attacks`].
//! * **Elephant (象)** ([`WideRole::JanggiElephant`], mce `x`, FSF `b`): moves one
//!   orthogonal then two diagonal squares outward (a `(±2,±3)`/`(±3,±2)` leap),
//!   blockable at each intervening square, **not** river-bound. Uses
//!   [`attacks::janggi_elephant_attacks`].
//! * **Soldier (병/졸)** ([`WideRole::Soldier`], mce `z`, FSF `p`): one step
//!   forward **or sideways** (no river gate — sideways always), never backward;
//!   plus a one-step **forward** move along a palace diagonal. No promotion.
//!
//! ## Pass
//!
//! A side may **pass** the turn (FSF counts it in `go perft`, encoded as the
//! general "staying put", `from == to == the general's square`). Passing is
//! **forbidden while in check**. Modelled through the default-off
//! [`WideVariant::allows_pass`] hook.
//!
//! ## Bikjang (generals facing)
//!
//! Facing the enemy general on an open file/rank does **not** make a position an
//! ordinary check, and a side may freely **expose** its own general — Janggi is
//! *weaker* than Xiangqi's flying general here. The only restriction is that the
//! general itself may not slide along the contested open line staying faced;
//! modelled through the default-off [`WideVariant::restricts_facing_general`] hook.
//!
//! ## Confirmed starting FEN
//!
//! From FSF's janggi `startFen`; mce and FSF agree on the position but spell four
//! pieces differently (mce avoids the letters `a n b p`, already taken):
//!
//! ```text
//! FSF dialect: rnba1abnr/4k4/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/4K4/RNBA1ABNR w - - 0 1
//! mce dialect: rjxu1uxjr/4k4/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/4K4/RJXU1UXJR w - - 0 1
//! ```

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{attacks, Bitboard, Board, Square, WideRole, WideVariant, Xiangqi9x10};
use crate::Color;

/// The Janggi rule layer: a zero-sized [`WideVariant`] over [`Xiangqi9x10`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct JanggiRules;

/// The confirmed Janggi starting placement in the mce dialect (guard `u`, horse
/// `j`, elephant `x`, soldier `z`), the position byte-identical to FSF's
/// `rnba1abnr/4k4/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/4K4/RNBA1ABNR`.
const JANGGI_START_PLACEMENT: &str =
    "rjxu1uxjr/4k4/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/4K4/RJXU1UXJR";

/// The four wazir (one orthogonal step) offsets — the General / Guard's orthogonal
/// movement.
const WAZIR_OFFSETS: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

impl JanggiRules {
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

    /// Both palaces' union.
    fn both_palaces() -> Bitboard<Xiangqi9x10> {
        Self::palace(Color::White) | Self::palace(Color::Black)
    }

    /// Returns `true` if `sq` is a **palace diagonal point** — a square that lies on
    /// a palace's marked diagonal "X" lines: the palace centre or any of its four
    /// corners. Only on these squares may a piece use the palace diagonals.
    fn is_palace_diag_point(sq: Square<Xiangqi9x10>) -> bool {
        let f = sq.file();
        let r = sq.rank();
        // Centre: e2 (4,1) or e9 (4,8). Corners: d/f files (3 or 5) on rank 0/2 or
        // 7/9. All have file 3 or 5 (corner) or 4 (centre), and even (file+rank)
        // parity within the palace places them on the X. Simplest: enumerate.
        let centre = f == 4 && (r == 1 || r == 8);
        let corner = (f == 3 || f == 5) && (r == 0 || r == 2 || r == 7 || r == 9);
        centre || corner
    }

    /// The palace-diagonal neighbours (one diagonal step along an X line) of a
    /// diagonal point `sq`. From a corner this is the palace centre; from the
    /// centre it is all four corners. Empty for any non-diagonal-point square.
    ///
    /// The General and Guard step to these; the Soldier steps to the *forward*
    /// subset; the Chariot slides through them; the Cannon jumps over the centre to
    /// the opposite corner.
    fn palace_diag_neighbours(sq: Square<Xiangqi9x10>) -> Bitboard<Xiangqi9x10> {
        let mut bb = Bitboard::<Xiangqi9x10>::EMPTY;
        if !Self::is_palace_diag_point(sq) {
            return bb;
        }
        let palaces = Self::both_palaces();
        for &(df, dr) in &[(1i8, 1i8), (1, -1), (-1, 1), (-1, -1)] {
            if let Some(dest) = sq.offset(df, dr) {
                // A diagonal neighbour must itself be a palace diagonal point in the
                // same palace (the centre↔corner adjacency).
                if palaces.contains(dest) && Self::is_palace_diag_point(dest) {
                    bb.set(dest);
                }
            }
        }
        bb
    }

    /// The General / Guard movement on `sq` for `color`: a wazir step **or** a
    /// palace-diagonal step, all confined to that color's palace.
    fn general_targets(color: Color, sq: Square<Xiangqi9x10>) -> Bitboard<Xiangqi9x10> {
        let ortho = attacks::leaper_attacks::<Xiangqi9x10>(sq, &WAZIR_OFFSETS);
        (ortho | Self::palace_diag_neighbours(sq)) & Self::palace(color)
    }

    /// The Soldier movement / attack set on `sq` for `color`: one step forward or
    /// sideways (never backward), **plus** a one-step *forward* palace-diagonal move
    /// when on a palace diagonal point. The set is the same for moving and
    /// capturing (a soldier captures wherever it can move).
    fn soldier_targets(color: Color, sq: Square<Xiangqi9x10>) -> Bitboard<Xiangqi9x10> {
        let forward: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = Bitboard::<Xiangqi9x10>::EMPTY;
        // Forward and the two sideways steps (no river gate in Janggi).
        for &(df, dr) in &[(0i8, forward), (-1, 0), (1, 0)] {
            if let Some(dest) = sq.offset(df, dr) {
                bb.set(dest);
            }
        }
        // Forward palace diagonals: only the diagonal neighbours that move toward
        // the enemy (a forward rank step).
        for dest in Self::palace_diag_neighbours(sq) {
            let dr = dest.rank() as i8 - sq.rank() as i8;
            if dr == forward {
                bb.set(dest);
            }
        }
        bb
    }

    /// The Chariot's palace-diagonal slides from `sq` given `occupied`: when `sq`
    /// is a palace diagonal point, the squares reachable along the palace X lines
    /// (stopping at and including the first blocker), added to its rook rays by the
    /// caller. A corner slides centre then opposite corner; the centre slides to
    /// each corner.
    fn chariot_palace_diag(
        sq: Square<Xiangqi9x10>,
        occupied: Bitboard<Xiangqi9x10>,
    ) -> Bitboard<Xiangqi9x10> {
        let mut bb = Bitboard::<Xiangqi9x10>::EMPTY;
        if !Self::is_palace_diag_point(sq) {
            return bb;
        }
        let palaces = Self::both_palaces();
        for &(df, dr) in &[(1i8, 1i8), (1, -1), (-1, 1), (-1, -1)] {
            let mut cur = sq.offset(df, dr);
            while let Some(next) = cur {
                // The diagonal only follows the palace X line: every step must stay
                // a palace diagonal point in a palace.
                if !(palaces.contains(next) && Self::is_palace_diag_point(next)) {
                    break;
                }
                bb.set(next);
                if occupied.contains(next) {
                    break; // first blocker on the diagonal is the last reachable.
                }
                cur = next.offset(df, dr);
            }
        }
        bb
    }

    /// The Cannon's palace-diagonal **jump** from `sq` given the board: when `sq` is
    /// a palace **corner** and the palace **centre** holds a non-cannon screen, the
    /// cannon may land on the opposite corner (jump over the centre), capturing a
    /// non-cannon there or moving onto it if empty. Returns the (at most one)
    /// opposite-corner target square.
    fn cannon_palace_diag(
        sq: Square<Xiangqi9x10>,
        board: &Board<Xiangqi9x10>,
    ) -> Bitboard<Xiangqi9x10> {
        let mut bb = Bitboard::<Xiangqi9x10>::EMPTY;
        if !Self::is_palace_diag_point(sq) {
            return bb;
        }
        let occupied = board.occupied();
        let cannons = board.pieces(Color::White, WideRole::Cannon)
            | board.pieces(Color::Black, WideRole::Cannon);
        let palaces = Self::both_palaces();
        for &(df, dr) in &[(1i8, 1i8), (1, -1), (-1, 1), (-1, -1)] {
            // Step 1: the palace centre (the screen).
            let Some(centre) = sq.offset(df, dr) else {
                continue;
            };
            if !(palaces.contains(centre) && Self::is_palace_diag_point(centre)) {
                continue;
            }
            // The screen must be present and must not be a cannon.
            if !occupied.contains(centre) || cannons.contains(centre) {
                continue;
            }
            // Step 2: the opposite corner (the landing square).
            let Some(target) = centre.offset(df, dr) else {
                continue;
            };
            if !(palaces.contains(target) && Self::is_palace_diag_point(target)) {
                continue;
            }
            // A cannon may not capture a cannon; an empty corner is a quiet jump.
            if occupied.contains(target) && cannons.contains(target) {
                continue;
            }
            bb.set(target);
        }
        bb
    }
}

impl WideVariant<Xiangqi9x10> for JanggiRules {
    fn starting_position() -> (Board<Xiangqi9x10>, GenericState<Xiangqi9x10>) {
        let board = Board::<Xiangqi9x10>::from_fen_placement(JANGGI_START_PLACEMENT)
            .expect("the Janggi starting placement is valid on a 9x10 board");
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
            // General and Guard: a palace-confined wazir plus a palace-diagonal
            // step.
            WideRole::King | WideRole::Advisor => Self::general_targets(color, sq),
            // Chariot: rook rays plus palace-diagonal slides.
            WideRole::Rook => {
                attacks::rook_attacks::<Xiangqi9x10>(sq, occupancy)
                    | Self::chariot_palace_diag(sq, occupancy)
            }
            // Horse: the Xiangqi hobbled knight.
            WideRole::Horse => attacks::horse_attacks::<Xiangqi9x10>(sq, occupancy),
            // Elephant: the long blockable leaper, not river-bound.
            WideRole::JanggiElephant => {
                attacks::janggi_elephant_attacks::<Xiangqi9x10>(sq, occupancy)
            }
            // Soldier: forward / sideways / forward-palace-diagonal.
            WideRole::Soldier => Self::soldier_targets(color, sq),
            // Cannon: the occupancy-only primitive is a sound fallback for the
            // orthogonal rays (its real screen/target cannon filtering and the
            // palace-diagonal jump come through `role_attacks_board`, which the
            // cannon-verify path uses). Using the screen-mandatory capture set here
            // keeps any incidental occupancy-only query (none on the Janggi path)
            // honest.
            WideRole::Cannon => {
                attacks::janggi_cannon_capture::<Xiangqi9x10>(sq, occupancy, Bitboard::EMPTY)
            }
            _ => Bitboard::EMPTY,
        }
    }

    fn uses_board_attacks() -> bool {
        true
    }

    fn role_attacks_board(
        role: WideRole,
        _color: Color,
        sq: Square<Xiangqi9x10>,
        board: &Board<Xiangqi9x10>,
    ) -> Option<Bitboard<Xiangqi9x10>> {
        // Only the Cannon needs the whole board (screen/target may not be a cannon,
        // plus the palace-diagonal jump over a centre screen). Every other role
        // returns `None` and falls back to the occupancy-only `role_attacks`.
        if role != WideRole::Cannon {
            return None;
        }
        let occupied = board.occupied();
        let cannons = board.pieces(Color::White, WideRole::Cannon)
            | board.pieces(Color::Black, WideRole::Cannon);
        // The combined move-and-attack set: over-screen captures, quiet jumps past a
        // screen, and the palace-diagonal jump. The generator's `emit_targets`
        // splits it into quiet/capture by enemy occupancy; the king-safety test sees
        // the king (an occupied royal) only in the capture portion.
        let ortho_caps = attacks::janggi_cannon_capture::<Xiangqi9x10>(sq, occupied, cannons);
        let ortho_quiet = attacks::janggi_cannon_quiet::<Xiangqi9x10>(sq, occupied, cannons);
        let diag = Self::cannon_palace_diag(sq, board);
        Some(ortho_caps | ortho_quiet | diag)
    }

    fn role_attack_is_leg_asymmetric(role: WideRole) -> bool {
        // Forward-projected (from each origin) in `attackers_to` / `king_safe_after`
        // because their attack sets are not reverse-projectable:
        // * Horse — its hobbling leg is asymmetric (as in Xiangqi).
        // * Soldier — forward-biased (and the palace-diagonal-forward squares are
        //   color-directional).
        // * Cannon — its attack depends on a screen and on which pieces are
        //   cannons (the board hook), so it must be projected forward from each
        //   cannon, exactly as the generator computes it.
        // * King / Advisor (General / Guard) — `general_targets` masks the reachable
        //   set by the *destination* being inside the palace, but does **not**
        //   require the origin to be: from a square just outside the palace the
        //   pattern still reaches palace squares. The attack relation is therefore
        //   asymmetric (a palace-edge piece attacks inward but not outward), so a
        //   reverse projection from the target would invent attacks the piece cannot
        //   make. Project forward from each general/guard origin, exactly as the
        //   generator does.
        // * JanggiElephant — its long `(±2,±3)/(±3,±2)` leap is blocked at the two
        //   intervening squares **adjacent to the origin** and pointing toward the
        //   destination (the same hobbling-leg class as the Horse). Those blockers
        //   differ from the ones a reverse leap from the target would test, so the
        //   relation is asymmetric under occupancy; project forward from each
        //   elephant origin.
        matches!(
            role,
            WideRole::Horse
                | WideRole::Soldier
                | WideRole::Cannon
                | WideRole::King
                | WideRole::Advisor
                | WideRole::JanggiElephant
        )
    }

    fn role_is_slider(role: WideRole) -> bool {
        // Only the Chariot is a line slider. The cannon-verify king-safety path does
        // not consult pins, but the classification is kept honest.
        matches!(role, WideRole::Rook)
    }

    fn has_castling() -> bool {
        false
    }

    fn has_cannons() -> bool {
        // Janggi fields cannons, so it takes the pseudo-legal + per-move verify
        // king-safety path (the cannon's check and king-danger are screen-
        // dependent). The pass and the bikjang filter ride the same verify.
        true
    }

    fn allows_pass() -> bool {
        true
    }

    fn restricts_facing_general() -> bool {
        true
    }
}

/// Janggi (Korean chess) as a [`GenericPosition`] over the 9x10 [`Xiangqi9x10`]
/// geometry.
///
/// Construct the starting position with
/// [`Janggi::startpos`](GenericPosition::startpos) or parse a FEN (mce dialect)
/// with [`Janggi::from_fen`](GenericPosition::from_fen). See the [module
/// docs](self) for the piece movements, the palace diagonals, the screen-cannon,
/// the pass move, and the bikjang facing rule.
pub type Janggi = GenericPosition<Xiangqi9x10, JanggiRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::perft as gperft;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    fn startpos() -> Janggi {
        Janggi::startpos()
    }

    fn uci_set(pos: &Janggi) -> Vec<String> {
        pos.legal_moves()
            .into_iter()
            .map(|m| m.to_uci::<Xiangqi9x10>())
            .collect()
    }

    #[test]
    fn startpos_matches_fsf_shallow() {
        let p = startpos();
        assert_eq!(gperft::<Xiangqi9x10, _>(&p, 1), 32);
        assert_eq!(gperft::<Xiangqi9x10, _>(&p, 2), 1024);
        assert_eq!(gperft::<Xiangqi9x10, _>(&p, 3), 33000);
    }

    #[test]
    fn startpos_offers_the_pass() {
        // The general sits on the palace centre e2; the pass renders `e2e2`.
        let moves = uci_set(&startpos());
        assert!(moves.contains(&"e2e2".to_string()), "{moves:?}");
        // Exactly one pass.
        assert_eq!(moves.iter().filter(|m| *m == "e2e2").count(), 1);
    }

    #[test]
    fn general_uses_palace_diagonal() {
        // White general alone in the palace centre e2; black king far in its
        // palace. The general reaches the four diagonal corners and the orthogonal
        // steps, all inside the palace.
        let pos = Janggi::from_fen("4k4/9/9/9/9/9/9/9/4K4/9 w - - 0 1").unwrap();
        let moves = uci_set(&pos);
        for sq in ["e2d1", "e2f1", "e2d3", "e2f3"] {
            assert!(
                moves.contains(&sq.to_string()),
                "missing diagonal {sq}: {moves:?}"
            );
        }
    }

    #[test]
    fn chariot_slides_full_palace_diagonal() {
        // A chariot on the palace corner d1 with an otherwise-empty palace slides
        // the whole corner-centre-corner diagonal: d1->e2 and d1->f3.
        let pos = Janggi::from_fen("3k5/9/9/9/9/9/9/9/9/3R1K3 w - - 0 1").unwrap();
        let moves = uci_set(&pos);
        assert!(moves.contains(&"d1e2".to_string()), "{moves:?}");
        assert!(moves.contains(&"d1f3".to_string()), "{moves:?}");
    }

    #[test]
    fn cannon_cannot_move_without_a_screen() {
        // A lone cannon (no screen on any ray) makes no cannon move; only the king
        // and the pass exist.
        // Cannon on a5 (file 0, rank 4) with nothing on its file or rank — no
        // screen on any ray. The kings sit in their palaces off all the cannon's
        // lines, so the cannon has no move at all.
        let pos = Janggi::from_fen("4k4/9/9/9/9/C8/9/9/4K4/9 w - - 0 1").unwrap();
        let moves = uci_set(&pos);
        assert!(
            !moves.iter().any(|m| m.starts_with("a5")),
            "cannon moved with no screen: {moves:?}"
        );
    }

    #[test]
    fn cannon_jumps_palace_diagonal_over_a_centre_screen() {
        // Cannon on the palace corner d1, a (non-cannon) horse screen on the palace
        // centre e2, a capturable chariot on the opposite corner f3.
        let pos = Janggi::from_fen("4k4/9/9/9/9/9/9/9/4J4/3C1r3 w - - 0 1").unwrap();
        let moves = uci_set(&pos);
        assert!(moves.contains(&"d1f3".to_string()), "{moves:?}");
    }

    #[test]
    fn soldier_moves_sideways_and_forward_not_backward() {
        // An open white soldier on e5 (file 4, rank 4): forward e6, sideways d5/f5,
        // never backward e4.
        let pos = Janggi::from_fen("4k4/9/9/9/9/4Z4/9/9/4K4/9 w - - 0 1").unwrap();
        let moves = uci_set(&pos);
        for sq in ["e5e6", "e5d5", "e5f5"] {
            assert!(moves.contains(&sq.to_string()), "missing {sq}: {moves:?}");
        }
        assert!(
            !moves.contains(&"e5e4".to_string()),
            "soldier moved backward"
        );
    }

    #[test]
    fn no_pass_while_in_check_but_pass_when_safe() {
        // In check: no pass offered.
        let checked = Janggi::from_fen("9/1k7/9/9/9/9/9/4z4/4K4/9 w - - 0 1").unwrap();
        assert!(checked.is_check());
        assert!(!uci_set(&checked).iter().any(|m| m == "e2e2"));
        // Safe: pass offered.
        let safe = Janggi::from_fen("9/1k7/9/9/9/9/4z4/9/4K4/9 w - - 0 1").unwrap();
        assert!(!safe.is_check());
        assert!(uci_set(&safe).iter().any(|m| m == "e2e2"));
    }

    #[test]
    fn two_consecutive_passes_end_the_game() {
        // From a quiet position, white passes then black passes; the next side has
        // no legal move at all (Fairy-Stockfish returns zero).
        let pos = Janggi::from_fen("9/1k7/9/9/9/9/9/9/4K4/9 w - - 0 1").unwrap();
        let after_white = pos.play(
            &pos.legal_moves()
                .into_iter()
                .find(|m| m.to_uci::<Xiangqi9x10>() == "e2e2")
                .expect("white pass"),
        );
        let after_black = after_white.play(
            &after_white
                .legal_moves()
                .into_iter()
                .find(|m| m.from_index() == m.to_index())
                .expect("black pass"),
        );
        assert!(
            after_black.legal_moves().is_empty(),
            "two passes should end the game"
        );
    }

    #[test]
    fn startpos_fen_round_trips() {
        let p = startpos();
        assert_eq!(
            p.to_fen(),
            "rjxu1uxjr/4k4/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/4K4/RJXU1UXJR w - - 0 1"
        );
    }
}
