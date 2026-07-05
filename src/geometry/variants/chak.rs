//! Chak (9x9 Mayan chess) on the generic engine — a Couch Tomato variant
//! inspired by the Mayan civilisation, validated node-for-node against
//! Fairy-Stockfish `UCI_Variant chak` (from its `variants.ini`). It reuses the
//! [`Shogi9x9`] geometry and introduces six new pieces, a
//! **king/Lord-promotes-on-the-temple-half** rule, a **region-confined** Shaman
//! and Divine Lord, an **eight-direction cannon** (the Quetzal), and a
//! **temple-square win**.
//!
//! ## Pieces (confirmed square-for-square against FSF)
//!
//! * **Rook (`R`/`r`, FSF `r`)** — a standard rook ([`WideRole::Rook`]).
//! * **Vulture (`N`/`n`, FSF `v`)** — a standard knight ([`WideRole::Knight`]).
//! * **Jaguar (`W`/`w`, FSF `j`)** — a **King + Knight** centaur (sixteen
//!   targets), exactly the Orda [`WideRole::Kheshig`].
//! * **King (`K`/`k`, FSF `k`)** — a plain royal king (one step any direction,
//!   FSF `WF`). It **promotes to the Divine Lord** on reaching its own half.
//! * **Serpent (`*S`/`*s`, FSF `s:FvW`)** — a leaper to the four diagonals (Ferz)
//!   plus one step straight forward/backward (vertical Wazir): six targets, no
//!   sideways step.
//! * **Quetzal (`*Q`/`*q`, FSF `q:pQ`)** — an **eight-direction cannon**: it moves
//!   and captures like a Queen but only by **hopping exactly one screen** (any
//!   colour) along a rank, file, or diagonal, landing on any empty square or the
//!   first enemy beyond the screen. No move on an unobstructed line; it cannot
//!   land on the screen.
//! * **Shaman (`*W`/`*w`, FSF `w:FvW`)** — moves exactly like the Serpent but is
//!   **confined to its own half** (White ranks 5-9 / Black ranks 1-5). It is the
//!   **promoted form of the Soldier**.
//! * **Divine Lord (`*L`/`*l`, FSF `d:mQ2cQ2`)** — moves and captures like a
//!   **Queen limited to two squares** (a blockable range-2 slider), **confined to
//!   its own half** like the Shaman. It is the **promoted form of the King** and
//!   is **royal**: a Lord reaching the enemy temple square **wins** (FSF
//!   `flagPiece = d`), and losing both King and Lord loses (FSF
//!   `extinctionPieceTypes = kd`, `extinctionPseudoRoyal`).
//! * **Soldier (`*P`/`*p`, FSF `p:fsmWfceF`)** — **moves** one step forward or
//!   sideways (never backward) but **captures** only one step diagonally forward.
//!   It **promotes to a Shaman** on reaching its own half.
//! * **Temple (`*O`/`*o`, FSF `o` `immobile`)** — the pyramid on each side's
//!   central rank-2/rank-8 square: it **never moves** but can be **captured**, and
//!   the square it sits on is the goal a Divine Lord wins by reaching.
//!
//! ## Promotion
//!
//! Promotion is **mandatory** and triggers the instant a King or Soldier moves to
//! a square in its own **far half** — White ranks 5-9, Black ranks 1-5 (FSF
//! `promotionRegion…`, `mandatoryPiecePromotion`). The King becomes a Divine Lord,
//! the Soldier a Shaman. No other piece promotes.
//!
//! ## Confirmed starting FEN
//!
//! ```text
//! FSF dialect: rvsqkjsvr/4o4/p1p1p1p1p/9/9/9/P1P1P1P1P/4O4/RVSJKQSVR w - - 0 1
//! mcr dialect: rn*s*qkw*snr/4*o4/*p1*p1*p1*p1*p/9/9/9/*P1*P1*P1*P1*P/4*O4/RN*SWK*Q*SNR w - - 0 1
//! ```
//!
//! The back ranks are **asymmetric**: White's rank 1 is `R V S J K Q S V R` and
//! Black's rank 9 is `r v s q k j s v r` — the Jaguar and Quetzal sit on opposite
//! sides of the King for the two armies. mcr spells the six new pieces with
//! `*`-prefixed overflow tokens and the `compare-fairy/` harness rewrites them
//! (`*s → s`, `*q → q`, `*w → w`, `*l → d`, `*p → p`, `*o → o`) plus the reused
//! Vulture (`n → v`) and Jaguar (`w → j`) when driving FSF.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Geometry, PromotionConfig, Square, WideRole, WideVariant,
};
use crate::Color;

use super::super::Shogi9x9;

/// The Chak rule layer: a zero-sized [`WideVariant`] over [`Shogi9x9`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ChakRules;

/// The confirmed Chak starting placement (mcr dialect; see the [module docs](self)).
const CHAK_PLACEMENT: &str =
    "rn*s*qkw*snr/4*o4/*p1*p1*p1*p1*p/9/9/9/*P1*P1*P1*P1*P/4*O4/RN*SWK*Q*SNR";

/// The four diagonal one-step (Ferz) offsets.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// The eight queen directions (file, rank deltas), used by the Quetzal cannon and
/// the range-2 Divine Lord mask.
const QUEEN_DIRS: [(i8, i8); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

impl ChakRules {
    /// The Serpent / Shaman base attack set from `sq`: the four diagonals (Ferz)
    /// plus one step straight forward or backward (vertical Wazir) — six targets.
    /// The Shaman additionally intersects this with its own half (see
    /// [`own_half`](Self::own_half)).
    fn serpent_attacks(sq: Square<Shogi9x9>) -> Bitboard<Shogi9x9> {
        let mut bb = attacks::leaper_attacks::<Shogi9x9>(sq, &FERZ_OFFSETS);
        for dr in [-1i8, 1] {
            if let Some(dest) = sq.offset(0, dr) {
                bb.set(dest);
            }
        }
        bb
    }

    /// The Divine Lord's base attack set from `sq` under `occupancy`: a Queen
    /// slide limited to two squares in each direction (blockable — it stops at the
    /// first piece, capturing it). Confined to its own half by the caller.
    fn divine_lord_attacks(
        sq: Square<Shogi9x9>,
        occupancy: Bitboard<Shogi9x9>,
    ) -> Bitboard<Shogi9x9> {
        let mut bb = Bitboard::<Shogi9x9>::EMPTY;
        for (df, dr) in QUEEN_DIRS {
            let mut step = 1i8;
            while step <= 2 {
                match sq.offset(df * step, dr * step) {
                    Some(dest) => {
                        bb.set(dest);
                        if occupancy.contains(dest) {
                            break; // blocked: stop after capturing the blocker
                        }
                    }
                    None => break,
                }
                step += 1;
            }
        }
        bb
    }

    /// The Quetzal's full move+capture set from `sq` under `occupancy`: along each
    /// of the eight queen directions it must **hop exactly one screen** (any
    /// colour), after which every square up to (and including) the first piece
    /// beyond the screen is reachable — empty squares are quiet moves, the first
    /// piece beyond is a capture. It has no move on a screenless line and never
    /// lands on the screen. `emit_targets` splits the returned set into quiet /
    /// capture by enemy occupancy.
    fn quetzal_attacks(sq: Square<Shogi9x9>, occupancy: Bitboard<Shogi9x9>) -> Bitboard<Shogi9x9> {
        let mut bb = Bitboard::<Shogi9x9>::EMPTY;
        for (df, dr) in QUEEN_DIRS {
            // Walk to the first piece (the screen); squares before it are not
            // reachable (a cannon needs a screen to move).
            let mut step = 1i8;
            let screen = loop {
                match sq.offset(df * step, dr * step) {
                    Some(dest) if occupancy.contains(dest) => break Some(dest),
                    Some(_) => step += 1,
                    None => break None,
                }
            };
            if screen.is_none() {
                continue; // no screen on this ray: no move
            }
            // Continue beyond the screen: every empty square is reachable, and the
            // first piece beyond is a capture target (then stop).
            step += 1;
            while let Some(dest) = sq.offset(df * step, dr * step) {
                bb.set(dest);
                if occupancy.contains(dest) {
                    break; // the first piece beyond the screen: a capture, then stop
                }
                step += 1;
            }
        }
        bb
    }

    /// The Chak Soldier's **capture** set from `sq` for `color`: one step
    /// diagonally forward (a forward Ferz) — the only squares it threatens.
    fn soldier_captures(color: Color, sq: Square<Shogi9x9>) -> Bitboard<Shogi9x9> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        attacks::leaper_attacks::<Shogi9x9>(sq, &[(1, fwd), (-1, fwd)])
    }

    /// The Chak Soldier's **quiet move** set from `sq` for `color`: one step
    /// forward or to either side (a forward/sideways Wazir, never backward).
    fn soldier_quiets(color: Color, sq: Square<Shogi9x9>) -> Bitboard<Shogi9x9> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        attacks::leaper_attacks::<Shogi9x9>(sq, &[(0, fwd), (1, 0), (-1, 0)])
    }

    /// The mask of `color`'s own half — the squares the Shaman and Divine Lord may
    /// never leave: White ranks 5-9 (0-based 4-8), Black ranks 1-5 (0-based 0-4).
    fn own_half(color: Color) -> Bitboard<Shogi9x9> {
        let mut bb = Bitboard::<Shogi9x9>::EMPTY;
        let ranks: core::ops::RangeInclusive<u8> = if color.is_white() {
            4..=(Shogi9x9::HEIGHT - 1)
        } else {
            0..=4
        };
        for rank in ranks {
            for file in 0..Shogi9x9::WIDTH {
                if let Some(sq) = Square::<Shogi9x9>::from_file_rank(file, rank) {
                    bb.set(sq);
                }
            }
        }
        bb
    }
}

impl WideVariant<Shogi9x9> for ChakRules {
    fn starting_position() -> (Board<Shogi9x9>, GenericState<Shogi9x9>) {
        let board = Board::<Shogi9x9>::from_fen_placement(CHAK_PLACEMENT)
            .expect("the Chak starting placement is valid on a 9x9 board");
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
        sq: Square<Shogi9x9>,
        occupancy: Bitboard<Shogi9x9>,
    ) -> Bitboard<Shogi9x9> {
        match role {
            WideRole::Rook => attacks::rook_attacks::<Shogi9x9>(sq, occupancy),
            WideRole::Knight => attacks::knight_attacks::<Shogi9x9>(sq),
            WideRole::King => attacks::king_attacks::<Shogi9x9>(sq),
            // Jaguar = King + Knight centaur.
            WideRole::Kheshig => {
                attacks::king_attacks::<Shogi9x9>(sq) | attacks::knight_attacks::<Shogi9x9>(sq)
            }
            WideRole::Serpent => Self::serpent_attacks(sq),
            // The Shaman moves as a Serpent but never leaves its own half.
            WideRole::Shaman => Self::serpent_attacks(sq) & Self::own_half(color),
            // The Divine Lord is a range-2 Queen confined to its own half.
            WideRole::DivineLord => {
                Self::divine_lord_attacks(sq, occupancy) & Self::own_half(color)
            }
            // The Quetzal's screen-dependent set is the board-aware path; this
            // occupancy-only fallback is the same hop computation (it sees no
            // friendly/enemy split, but the verify path always uses
            // `role_attacks_board`).
            WideRole::Quetzal => Self::quetzal_attacks(sq, occupancy),
            // The Soldier's *attack* set is its forward-diagonal capture only; its
            // quiet forward/sideways steps ride `quiet_targets_board`.
            WideRole::ChakSoldier => Self::soldier_captures(color, sq),
            // The Temple never moves and never threatens.
            WideRole::Temple => Bitboard::EMPTY,
            _ => Bitboard::EMPTY,
        }
    }

    fn uses_board_attacks() -> bool {
        // The Quetzal (an eight-direction cannon whose captures land only beyond a
        // screen) and the Soldier (a move≠capture piece) compute their sets from
        // the whole board. Every other role returns `None` below and falls back to
        // the occupancy-only `role_attacks`.
        true
    }

    fn role_attacks_board(
        role: WideRole,
        color: Color,
        sq: Square<Shogi9x9>,
        board: &Board<Shogi9x9>,
    ) -> Option<Bitboard<Shogi9x9>> {
        match role {
            // The Quetzal's hop set already folds quiet jumps and over-screen
            // captures together; `emit_targets` splits them by enemy occupancy.
            WideRole::Quetzal => Some(Self::quetzal_attacks(sq, board.occupied())),
            // The Soldier is a move≠capture piece: it moves forward/sideways onto an
            // *empty* square and captures only forward-diagonally onto an *enemy*.
            // The set folds the two: the quiet steps are masked to empty squares so
            // a king (always on an occupied square) falls only in the capture
            // portion — exactly the Empire trick that keeps the forward king-safety
            // projection a true threat set. `emit_targets` then re-splits it into
            // quiet (empty) and capture (enemy).
            WideRole::ChakSoldier => {
                let occupied = board.occupied();
                let enemies = board.by_color(color.opposite());
                let quiet = Self::soldier_quiets(color, sq) & !occupied;
                let captures = Self::soldier_captures(color, sq) & enemies;
                Some(quiet | captures)
            }
            _ => None,
        }
    }

    fn role_attack_is_leg_asymmetric(role: WideRole) -> bool {
        // * The Quetzal's attack set is its over-screen capture, which is
        //   occupancy-asymmetric (a cannon must jump a screen) — detect it forward
        //   from each Quetzal, exactly as the generator does.
        // * The Shaman and Divine Lord are confined to their own half: their attack
        //   relation is keyed on the *origin* (a piece in its half can threaten a
        //   square the same piece could not threaten from across the line), so a
        //   reverse projection from the target would invent attacks. Detect them
        //   forward from each origin.
        // * The Soldier's move set folds quiet forward/sideways steps (onto empty
        //   squares) with forward-diagonal captures (onto enemies); only the
        //   board-aware set isolates the capture portion (an empty forward square
        //   must never count as a threat), so it rides the forward-projection path.
        matches!(
            role,
            WideRole::Quetzal | WideRole::Shaman | WideRole::DivineLord | WideRole::ChakSoldier
        )
    }

    fn role_is_slider(role: WideRole) -> bool {
        // The Rook slides (can pin / be pinned along a ray). The Divine Lord slides
        // but is range-2 and confined and rides the board-aware verify path; the
        // Quetzal is a cannon (not a slider). Every other Chak piece is a stepper.
        matches!(role, WideRole::Rook)
    }

    // --- promotion: King -> Divine Lord, Soldier -> Shaman (no hand) ------

    fn has_piece_promotion() -> bool {
        true
    }

    fn role_can_promote(role: WideRole) -> bool {
        matches!(role, WideRole::King | WideRole::ChakSoldier)
    }

    fn role_promoted_to(role: WideRole) -> WideRole {
        match role {
            WideRole::King => WideRole::DivineLord,
            WideRole::ChakSoldier => WideRole::Shaman,
            other => other,
        }
    }

    fn promotion_mandatory_in_zone() -> bool {
        true
    }

    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        // The promotion region is each side's own far half: White ranks 5-9
        // (0-based 4-8), Black ranks 1-5 (0-based 0-4).
        if color.is_white() {
            rank >= 4
        } else {
            rank <= 4
        }
    }

    // --- royalty (King OR Divine Lord) + temple win ----------------------

    fn multi_royal() -> bool {
        // Chak's royal piece can be a King *or* a promoted Divine Lord (FSF
        // `extinctionPieceTypes = kd`, `extinctionPseudoRoyal`). Routing through
        // the multi-royal verify path lets `royal_squares` (below) carry both, and
        // re-queries the royal set after each move so a King's promotion to the
        // (also-royal) Divine Lord is naturally king-safety-verified. A side always
        // has exactly one royal, so "at least one survives" is ordinary check.
        true
    }

    fn royal_squares(board: &Board<Shogi9x9>, color: Color) -> Bitboard<Shogi9x9> {
        board.kings_of(color) | board.pieces(color, WideRole::DivineLord)
    }

    fn royals_all_must_survive() -> bool {
        // Strict pseudo-royal (FSF `extinctionPseudoRoyal`): a move may not leave
        // *any* royal (King or Divine Lord) en prise. A reachable Chak position has
        // exactly one royal, but this matches FSF on artificial two-royal positions
        // too.
        true
    }

    fn has_temple_win() -> bool {
        true
    }

    fn temple_goal(color: Color) -> Bitboard<Shogi9x9> {
        // A Divine Lord wins by reaching the **enemy** temple square: e8 (file 4,
        // rank 7) for White, e2 (file 4, rank 1) for Black.
        let rank = if color.is_white() { 7 } else { 1 };
        let mut bb = Bitboard::<Shogi9x9>::EMPTY;
        if let Some(sq) = Square::<Shogi9x9>::from_file_rank(4, rank) {
            bb.set(sq);
        }
        bb
    }

    fn promotion_config() -> PromotionConfig {
        // Chak has no pawn-path promotion (its Soldier is a non-pawn piece routed
        // through the per-piece promotion path); this static set is unused, but the
        // trait requires it.
        PromotionConfig {
            roles: alloc::vec![WideRole::DivineLord],
        }
    }

    fn has_castling() -> bool {
        false
    }

    fn stalemate_is_loss() -> bool {
        // FSF `stalemateValue = loss`. This affects only the reported outcome, not
        // perft (a stalemated node already generates zero moves).
        true
    }
}

/// Chak (9x9 Mayan chess) as a [`GenericPosition`] over the 9x9 [`Shogi9x9`]
/// geometry.
///
/// Construct the starting position with
/// [`Chak::startpos`](GenericPosition::startpos) or parse a FEN (mcr dialect) with
/// [`Chak::from_fen`](GenericPosition::from_fen). See the [module docs](self) for
/// the piece movements, the King/Lord promotion, the region confinement, and the
/// temple-square win.
pub type Chak = GenericPosition<Shogi9x9, ChakRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::position::WideOutcome;

    /// The canonical start FEN round-trips through mcr's FEN I/O.
    #[test]
    fn startpos_round_trips() {
        let pos = Chak::startpos();
        assert_eq!(
            pos.to_fen(),
            "rn*s*qkw*snr/4*o4/*p1*p1*p1*p1*p/9/9/9/*P1*P1*P1*P1*P/4*O4/RN*SWK*Q*SNR w - - 0 1"
        );
    }

    /// Stalemate is scored as a **loss** for the side to move (FSF
    /// `stalemateValue = loss`, issue #498). The lone Black king on a9 (its far
    /// corner — rank 9 is outside the Black promotion half 1-5, so it stays a
    /// plain king) has no legal move and is not in check: a White Rook on i8 sweeps
    /// the whole (unobstructed) rank 8 to seal the a8/b8 escapes and a White
    /// Vulture (knight) on c7 covers b9, while a9 itself is unattacked. Black is
    /// stalemated, so Black loses and White wins.
    #[test]
    fn stalemate_is_a_loss() {
        let pos = Chak::from_fen("k8/8R/2N6/9/9/9/9/9/4K4 b - - 0 1").expect("valid chak FEN");
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
