//! Opulent chess (10x10) on the generic engine — an Omega-family 10x10 variant on
//! the [`Grand10x10`] geometry, validated node-for-node against Fairy-Stockfish
//! (`UCI_Variant opulent`, a built-in of a `largeboards=yes` build).
//!
//! Opulent is played on a ten-files by ten-ranks board (files a..j, ranks 1..10).
//! Its army is the standard Rook, Bishop, Queen, and King, an **augmented knight**,
//! and **four** extra kinds — two compounds mce already models and two genuinely-new
//! leapers:
//!
//! * **Knight** ([`WideRole::OpulentKnight`], Betza `NW` = Knight + Wazir) — Opulent's
//!   knight also steps one square orthogonally, so it reaches the eight knight squares
//!   **and** the four Wazir `(±1,0)`/`(0,±1)` one-steps (twelve targets). Distinct from
//!   the plain [`WideRole::Knight`]; FEN token `**z`/`**Z` in the mce dialect (FSF
//!   `n`/`N`). *Two per side.* (Confirmed against FSF: the rook, bishop, and queen are
//!   otherwise standard.)
//! * **Chancellor** (Rook + Knight) — mce's [`WideRole::Elephant`], FEN `e`/`E` in
//!   the mce dialect (Fairy-Stockfish spells it `c`/`C`). *One per side.*
//! * **Archbishop** (Bishop + Knight) — mce's [`WideRole::Hawk`], FEN `a`/`A` in
//!   both mce and FSF. *One per side.*
//! * **Wizard** ([`WideRole::Wizard`], Betza `CF` = Camel + Ferz) — a pure leaper to
//!   the eight Camel `(±1,±3)`/`(±3,±1)` squares and the four Ferz `(±1,±1)`
//!   one-steps. FEN token `**w`/`**W` in the mce dialect (FSF `w`/`W`). *Two per
//!   side.*
//! * **Lion** ([`WideRole::OpulentLion`], Betza `FDH` = Ferz + Dabbaba +
//!   Threeleaper) — a pure leaper to the four Ferz `(±1,±1)` diagonal one-steps, the
//!   four Dabbaba `(±2,0)`/`(0,±2)` jumps, and the four Threeleaper `(±3,0)`/`(0,±3)`
//!   jumps: one square diagonally, or two or three squares straight. FEN token
//!   `**y`/`**Y` in the mce dialect (FSF `l`/`L`). *Two per side.*
//!
//! Both leapers jump over any intervening piece; their attack sets are symmetric,
//! so [`attackers_to`](crate::geometry::GenericPosition::attackers_to) reverse-projects
//! them directly (no `attackers_to` override is needed).
//!
//! ## Rules that differ from standard chess
//!
//! Opulent shares Grand chess's pawn and promotion rules exactly (confirmed against
//! FSF):
//!
//! * **No castling** (FSF FEN rights `-`).
//! * **Pawns** double-push from their start rank (rank 3 for white, rank 8 for
//!   black), take en passant, and **promote in a three-rank zone** — the far three
//!   ranks (8, 9, 10 for white). Promotion is *optional* on the near two ranks and
//!   *forced* on the last rank.
//! * **Promote only to an already-captured piece type.** A pawn may promote to a
//!   role only while the side holds fewer than its **starting army count** on the
//!   board (FSF `promotionLimit`, read live): at most one Queen, Chancellor, or
//!   Archbishop, and at most two Rooks, Bishops, Knights, **Wizards, or Lions**.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant opulent` / `position startpos`:
//!
//! ```text
//! FSF dialect: rw6wr/clbnqknbla/pppppppppp/10/10/10/10/PPPPPPPPPP/CLBNQKNBLA/RW6WR w - - 0 1
//! mce dialect: r**w6**wr/e**yb**zqk**zb**ya/pppppppppp/10/10/10/10/PPPPPPPPPP/E**YB**ZQK**ZB**YA/R**W6**WR w - - 0 1
//! ```
//!
//! Back-two ranks (a..j): the rooks hold the corners (a/j files, rank 10 / rank 1)
//! with the wizards beside them (b/i files); rank 9 / rank 2 holds
//! chancellor-lion-bishop-knight-queen-king-knight-bishop-lion-archbishop, so the
//! king stands on the f-file, the queen on the e-file, and the two lions on the b/i
//! files. Pawns sit on ranks 3 and 8. The dialect swaps the chancellor's letter
//! (`c`→`e`) and spells the wizard `**w`, lion `**y`, and augmented knight `**z`; the
//! archbishop `a` is unchanged.

use crate::geometry::attacks::leaper_attacks;
use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    Bitboard, Board, Grand10x10, PromotionConfig, Square, StandardChess, WideRole, WideVariant,
};
use crate::Color;

/// The Opulent rule layer: a zero-sized [`WideVariant`] over [`Grand10x10`].
///
/// It overrides only what Opulent changes from the standard generic engine: the
/// 10x10 starting array, the two new leapers' movement (Wizard, Lion), the absence
/// of castling, the pawn start (double-push) rank, the three-rank promotion zone
/// with its optional/forced split, and the promote-only-to-a-captured-type rule
/// over the Opulent army. The Chancellor ([`WideRole::Elephant`]) and Archbishop
/// ([`WideRole::Hawk`]) movement is already the trait default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct OpulentRules;

/// The confirmed Opulent starting placement in the mce dialect (chancellor `e`,
/// wizard `**w`, lion `**y`), byte-equivalent to Fairy-Stockfish's
/// `RW6WR/CLBNQKNBLA/…` under the letter map.
const OPULENT_START_PLACEMENT: &str =
    "r**w6**wr/e**yb**zqk**zb**ya/pppppppppp/10/10/10/10/PPPPPPPPPP/E**YB**ZQK**ZB**YA/R**W6**WR";

/// The Wizard's leaps (Betza `CF`): the eight Camel `(±1,±3)`/`(±3,±1)` jumps and
/// the four Ferz `(±1,±1)` diagonal one-steps.
const WIZARD_OFFSETS: [(i8, i8); 12] = [
    (1, 3),
    (1, -3),
    (-1, 3),
    (-1, -3),
    (3, 1),
    (3, -1),
    (-3, 1),
    (-3, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

/// The Lion's leaps (Betza `FDH`): the four Ferz `(±1,±1)` diagonal one-steps, the
/// four Dabbaba `(±2,0)`/`(0,±2)` jumps, and the four Threeleaper `(±3,0)`/`(0,±3)`
/// jumps — one square diagonally, or two or three squares straight.
const LION_OFFSETS: [(i8, i8); 12] = [
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
    (2, 0),
    (-2, 0),
    (0, 2),
    (0, -2),
    (3, 0),
    (-3, 0),
    (0, 3),
    (0, -3),
];

/// The Opulent Knight's leaps (Betza `NW`): the eight ordinary knight
/// `(±1,±2)`/`(±2,±1)` jumps and the four Wazir `(±1,0)`/`(0,±1)` orthogonal
/// one-steps.
const OPULENT_KNIGHT_OFFSETS: [(i8, i8); 12] = [
    (1, 2),
    (1, -2),
    (-1, 2),
    (-1, -2),
    (2, 1),
    (2, -1),
    (-2, 1),
    (-2, -1),
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
];

/// The starting army count of each promotable role — the FSF `promotionLimit` for
/// Opulent. A pawn may promote to a role only while the side holds fewer than this
/// many of it on the board (a single Queen / Chancellor / Archbishop; two each of
/// Rook / Bishop / Knight / Wizard / Lion). Listed in the deterministic promotion
/// order.
const PROMOTION_LIMITS: [(WideRole, u32); 8] = [
    (WideRole::OpulentKnight, 2),
    (WideRole::Bishop, 2),
    (WideRole::Rook, 2),
    (WideRole::Wizard, 2),
    (WideRole::OpulentLion, 2),
    (WideRole::Queen, 1),
    (WideRole::Hawk, 1),     // Archbishop (B+N)
    (WideRole::Elephant, 1), // Chancellor (R+N)
];

impl WideVariant<Grand10x10> for OpulentRules {
    fn starting_position() -> (Board<Grand10x10>, GenericState<Grand10x10>) {
        let board = Board::<Grand10x10>::from_fen_placement(OPULENT_START_PLACEMENT)
            .expect("the Opulent starting placement is valid on a 10x10 board");
        let state = GenericState {
            turn: Color::White,
            // Opulent has no castling.
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
        sq: Square<Grand10x10>,
        occupancy: Bitboard<Grand10x10>,
    ) -> Bitboard<Grand10x10> {
        match role {
            // Wizard (Camel + Ferz) and Lion (Ferz + Dabbaba + Threeleaper): pure
            // leapers over any intervening piece.
            WideRole::Wizard => leaper_attacks::<Grand10x10>(sq, &WIZARD_OFFSETS),
            WideRole::OpulentLion => leaper_attacks::<Grand10x10>(sq, &LION_OFFSETS),
            // Opulent's knight is a Knight + Wazir (it also steps one square
            // orthogonally), so it is its own leaper role, not the plain Knight.
            WideRole::OpulentKnight => leaper_attacks::<Grand10x10>(sq, &OPULENT_KNIGHT_OFFSETS),
            // The Chancellor (Elephant), Archbishop (Hawk), standard army, and pawn
            // defer to the trait default (`StandardChess` overrides no movement).
            _ => {
                <StandardChess as WideVariant<Grand10x10>>::role_attacks(role, color, sq, occupancy)
            }
        }
    }

    fn promotion_config() -> PromotionConfig {
        // The full Opulent promotion army: the four standard, the two new leapers,
        // and the two compounds. `promotion_targets` filters this per move by the
        // starting-army limit; this is the unfiltered superset (and the FEN /
        // round-trip vocabulary). Order affects only enumeration order, not perft.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::OpulentKnight,
                WideRole::Bishop,
                WideRole::Rook,
                WideRole::Wizard,
                WideRole::OpulentLion,
                WideRole::Queen,
                WideRole::Hawk,     // Archbishop (B+N)
                WideRole::Elephant, // Chancellor (R+N)
            ],
        }
    }

    fn promotion_targets(color: Color, board: &Board<Grand10x10>) -> alloc::vec::Vec<WideRole> {
        // Promote-only-to-a-captured-type: a pawn may promote to a role only while
        // the side holds fewer than the starting count of it on the board (FSF
        // `promotionLimit`). Read live from the board, exactly as Grand does.
        PROMOTION_LIMITS
            .iter()
            .filter(|&&(role, limit)| board.pieces(color, role).count() < limit)
            .map(|&(role, _)| role)
            .collect()
    }

    fn promotion_rank(color: Color) -> u8 {
        // The forced (last) promotion rank: rank 10 (index 9) for white, rank 1
        // (index 0) for black.
        match color {
            Color::White => 9,
            Color::Black => 0,
        }
    }

    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        // The far three ranks: 8, 9, 10 (indices 7, 8, 9) for white; 1, 2, 3
        // (indices 0, 1, 2) for black.
        match color {
            Color::White => rank >= 7,
            Color::Black => rank <= 2,
        }
    }

    fn promotion_is_forced(color: Color, rank: u8) -> bool {
        // Forced only on the final rank; optional on the near two zone ranks.
        rank == Self::promotion_rank(color)
    }

    fn double_push_rank(color: Color) -> u8 {
        // Pawns start on (and double-push from) rank 3 (index 2) for white and
        // rank 8 (index 7) for black.
        match color {
            Color::White => 2,
            Color::Black => 7,
        }
    }

    fn has_castling() -> bool {
        false
    }

    /// The Lion ([`WideRole::OpulentLion`]) leaps two/three squares straight
    /// (Dabbaba / Threeleaper) — jumps **collinear** with a pin ray — so a pinned
    /// Lion could otherwise vault past its own king to a far-side square on the pin
    /// line. Confine every pinned piece to the king–pinner segment so that cannot
    /// happen (matching Fairy-Stockfish); ordinary sliders are unaffected, as
    /// occupancy already limits them to the segment.
    fn confine_pins_to_segment() -> bool {
        true
    }

    /// Opulent keeps the standard chess army plus the always-mating Archbishop
    /// ([`WideRole::Hawk`]), Chancellor ([`WideRole::Elephant`]), Wizard, and Lion,
    /// so the ordinary insufficient-material draw applies: king vs king, king and a
    /// lone minor vs king, and same-colour bishops only. Every fairy piece counts as
    /// mating material. Adjudication-only and behind the default-off hook, so perft
    /// stays byte-identical.
    fn is_insufficient_material(
        board: &Board<Grand10x10>,
        _state: &GenericState<Grand10x10>,
    ) -> bool {
        crate::geometry::variant::standard_insufficient_material(board)
    }
}

/// Opulent chess as a [`GenericPosition`] over the 10x10 [`Grand10x10`] geometry.
///
/// Construct the starting position with
/// [`Opulent::startpos`](GenericPosition::startpos) or parse a FEN (mce dialect)
/// with [`Opulent::from_fen`](GenericPosition::from_fen). See the [module docs](self)
/// for the army (Wizard + Lion leapers), the no-castling / pawn rules, and the
/// three-rank promote-to-captured zone.
pub type Opulent = GenericPosition<Grand10x10, OpulentRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::perft as gperft;
    use crate::geometry::Square as Sq;
    use alloc::vec::Vec;

    fn targets_from(fen: &str, file: u8, rank: u8) -> Vec<u8> {
        let pos = Opulent::from_fen(fen).expect("valid Opulent FEN");
        let from = Sq::<Grand10x10>::from_file_rank(file, rank).expect("on board");
        let mut got: Vec<u8> = pos
            .legal_moves()
            .iter()
            .filter(|m| m.from::<Grand10x10>() == from)
            .map(|m| m.to::<Grand10x10>().index())
            .collect();
        got.sort_unstable();
        got.dedup();
        got
    }

    fn indices(coords: &[(u8, u8)]) -> Vec<u8> {
        let mut v: Vec<u8> = coords
            .iter()
            .map(|&(f, r)| {
                Sq::<Grand10x10>::from_file_rank(f, r)
                    .expect("on board")
                    .index()
            })
            .collect();
        v.sort_unstable();
        v
    }

    /// The startpos round-trips through the mce dialect FEN.
    #[test]
    fn startpos_round_trips() {
        let pos = Opulent::startpos();
        assert_eq!(
            pos.to_fen(),
            "r**w6**wr/e**yb**zqk**zb**ya/pppppppppp/10/10/10/10/PPPPPPPPPP/E**YB**ZQK**ZB**YA/R**W6**WR w - - 0 1"
        );
    }

    /// FSF-confirmed startpos perft counts (`UCI_Variant opulent`, `go perft`).
    #[test]
    fn startpos_perft_matches_fsf() {
        let pos = Opulent::startpos();
        assert_eq!(gperft::<Grand10x10, _>(&pos, 1), 50);
        assert_eq!(gperft::<Grand10x10, _>(&pos, 2), 2500);
        assert_eq!(gperft::<Grand10x10, _>(&pos, 3), 133829);
    }

    /// The Lion leaps one square diagonally (Ferz), or two/three squares straight
    /// (Dabbaba / Threeleaper) — twelve targets — jumping over any piece. e6 =
    /// file 4, rank 5 (0-based).
    #[test]
    fn lion_moves_are_ferz_dabbaba_threeleaper() {
        let got = targets_from("k9/10/10/10/4**Y5/10/10/10/10/K9 w - - 0 1", 4, 5);
        // From e6 — Ferz: d5 f5 d7 f7; Dabbaba: e4 c6 g6 e8; Threeleaper: e3 b6 h6 e9.
        let want = indices(&[
            (3, 4),
            (5, 4),
            (3, 6),
            (5, 6),
            (4, 3),
            (2, 5),
            (6, 5),
            (4, 7),
            (4, 2),
            (1, 5),
            (7, 5),
            (4, 8),
        ]);
        assert_eq!(got, want);
    }

    /// The Wizard is the Camel + Ferz leaper (same as Ten-Cubed).
    #[test]
    fn wizard_moves_are_camel_plus_ferz() {
        let got = targets_from("k9/10/10/10/4**W5/10/10/10/10/K9 w - - 0 1", 4, 5);
        let want = indices(&[
            (3, 2),
            (5, 2),
            (1, 4),
            (7, 4),
            (1, 6),
            (7, 6),
            (3, 8),
            (5, 8),
            (3, 4),
            (5, 4),
            (3, 6),
            (5, 6),
        ]);
        assert_eq!(got, want);
    }

    /// Promotion is Grand-style: a three-rank zone (optional on the near two ranks),
    /// to any captured-type army role including Wizard and Lion.
    #[test]
    fn promotion_zone_and_captured_type_rule() {
        // A pawn on e8 (rank 8, in the zone) may push to e9 or promote there.
        let pos =
            Opulent::from_fen("5k4/10/4P5/10/10/10/10/10/10/5K4 w - - 0 1").expect("valid FEN");
        let promos: Vec<WideRole> = pos
            .legal_moves()
            .iter()
            .filter_map(|m| m.promotion())
            .collect();
        assert!(promos.contains(&WideRole::Wizard));
        assert!(promos.contains(&WideRole::OpulentLion));
        assert!(promos.contains(&WideRole::Queen));
        assert!(pos.legal_moves().iter().any(|m| m.promotion().is_none()
            && m.from::<Grand10x10>() == Sq::<Grand10x10>::from_file_rank(4, 7).unwrap()));
        // With two Wizards already on the board, a pawn may no longer promote to a
        // Wizard (captured-type limit), matching FSF.
        let pos2 = Opulent::from_fen("5k4/4P5/10/10/10/10/10/10/10/**W3**WK4 w - - 0 1")
            .expect("valid FEN");
        assert!(pos2
            .legal_moves()
            .iter()
            .filter_map(|m| m.promotion())
            .all(|r| r != WideRole::Wizard));
    }
}
