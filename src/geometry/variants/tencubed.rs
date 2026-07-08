//! Ten-Cubed chess (10x10) on the generic engine — an Omega-family 10x10 variant
//! on the [`Grand10x10`] geometry, validated node-for-node against Fairy-Stockfish
//! (`UCI_Variant tencubed`, a built-in of a `largeboards=yes` build).
//!
//! Ten-Cubed is played on a ten-files by ten-ranks board (files a..j, ranks
//! 1..10). Its army is the standard chess pieces plus **four** extra kinds — two
//! compounds mcr already models and two genuinely-new leapers:
//!
//! * **Marshal** (Rook + Knight) — mcr's [`WideRole::Elephant`], FEN `e`/`E` in the
//!   mcr dialect (Fairy-Stockfish spells it `m`/`M`).
//! * **Archbishop** (Bishop + Knight) — mcr's [`WideRole::Hawk`], FEN `a`/`A` in
//!   both mcr and FSF.
//! * **Wizard** ([`WideRole::Wizard`], Betza `CF` = Camel + Ferz) — a pure leaper
//!   to the eight Camel `(±1,±3)`/`(±3,±1)` squares and the four Ferz `(±1,±1)`
//!   one-steps. FEN token `**w`/`**W` in the mcr dialect (FSF `w`/`W`).
//! * **Champion** ([`WideRole::TencubedChampion`], Betza `WAD` = Wazir + Alfil +
//!   Dabbaba) — a pure leaper to the four Wazir `(±1,0)`/`(0,±1)` one-steps, the
//!   four Dabbaba `(±2,0)`/`(0,±2)` jumps, and the four Alfil `(±2,±2)` jumps. FEN
//!   token `**x`/`**X` in the mcr dialect (FSF `c`/`C`).
//!
//! Both leapers jump over any intervening piece; their attack sets are symmetric,
//! so [`attackers_to`](crate::geometry::GenericPosition::attackers_to) reverse-projects
//! them directly (no `attackers_to` override is needed).
//!
//! ## Rules that differ from standard chess
//!
//! * **No castling** (FSF FEN rights `-`; confirmed against FSF).
//! * **Pawns** double-push from their start rank (rank 3 for white, rank 8 for
//!   black), take en passant, and **promote on the last rank only** — a single-rank
//!   promotion zone (unlike Grand's three-rank zone), confirmed against FSF: a pawn
//!   arriving on rank 8 or 9 may not promote.
//! * **Promotion is to a Queen, Marshal, or Archbishop only** — *not* to a Rook,
//!   Knight, Bishop, Wizard, or Champion — and is **unrestricted** (there is no
//!   Grand-style promote-only-to-a-captured-type limit; confirmed against FSF, which
//!   still offers all three targets with one already on the board).
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant tencubed` / `position startpos`:
//!
//! ```text
//! FSF dialect: 2cwamwc2/1rnbqkbnr1/pppppppppp/10/10/10/10/PPPPPPPPPP/1RNBQKBNR1/2CWAMWC2 w - - 0 1
//! mcr dialect: 2**x**wae**w**x2/1rnbqkbnr1/pppppppppp/10/10/10/10/PPPPPPPPPP/1RNBQKBNR1/2**X**WAE**W**X2 w - - 0 1
//! ```
//!
//! The fairy pieces sit on the **outer** back rank (rank 10 / rank 1): champions on
//! the c/h files, wizards on the d/g files, with the archbishop (e-file, above the
//! queen) and marshal (f-file, above the king) between them. The standard army
//! (rooks b/i, knights c/h, bishops d/g, queen e, king f) sits on rank 9 / rank 2;
//! the corner files a/j are empty. Pawns sit on ranks 3 and 8.

use crate::geometry::attacks::leaper_attacks;
use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    Bitboard, Board, Grand10x10, PromotionConfig, Square, StandardChess, WideRole, WideVariant,
};
use crate::Color;

/// The Ten-Cubed rule layer: a zero-sized [`WideVariant`] over [`Grand10x10`].
///
/// It overrides only what Ten-Cubed changes from the standard generic engine: the
/// 10x10 starting array, the two new leapers' movement (Wizard, Champion), the
/// absence of castling, the pawn start (double-push) rank, and the restricted
/// last-rank promotion set. The Marshal ([`WideRole::Elephant`]) and Archbishop
/// ([`WideRole::Hawk`]) movement is already the trait default; pawns (bar the
/// promotion set), knights, sliders, and the king are standard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TencubedRules;

/// The confirmed Ten-Cubed starting placement in the mcr dialect (marshal `e`,
/// wizard `**w`, champion `**x`), byte-equivalent to Fairy-Stockfish's
/// `2CWAMWC2/1RNBQKBNR1/…` under the [`fen_to_fsf`](crate) letter map.
const TENCUBED_START_PLACEMENT: &str =
    "2**x**wae**w**x2/1rnbqkbnr1/pppppppppp/10/10/10/10/PPPPPPPPPP/1RNBQKBNR1/2**X**WAE**W**X2";

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

/// The Champion's leaps (Betza `WAD`): the four Wazir `(±1,0)`/`(0,±1)` one-steps,
/// the four Dabbaba `(±2,0)`/`(0,±2)` jumps, and the four Alfil `(±2,±2)` jumps.
const CHAMPION_OFFSETS: [(i8, i8); 12] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (2, 0),
    (-2, 0),
    (0, 2),
    (0, -2),
    (2, 2),
    (2, -2),
    (-2, 2),
    (-2, -2),
];

impl WideVariant<Grand10x10> for TencubedRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 108;

    /// The western **fifty-move rule**: a position whose halfmove clock has
    /// reached 100 plies (50 full moves with no capture or pawn move) is a
    /// [`WideEndReason::MoveRule`](crate::geometry::WideEndReason::MoveRule) draw,
    /// matching Fairy-Stockfish's default `nMoveRule = 50` for this standard-army
    /// large board. Adjudication-only (the clock never gates move generation), so
    /// perft stays byte-identical.
    fn move_rule_plies() -> Option<u16> {
        Some(100)
    }

    /// Records a position history so the standard **threefold** repetition draw
    /// ([`WideEndReason::Repetition`](crate::geometry::WideEndReason::Repetition),
    /// fold 3) fires at the [`GenericGame`](crate::geometry::game::GenericGame)
    /// level. History-dependent and never consulted by a bare
    /// [`GenericPosition`], so perft is unchanged.
    fn tracks_repetition() -> bool {
        true
    }

    fn starting_position() -> (Board<Grand10x10>, GenericState<Grand10x10>) {
        let board = Board::<Grand10x10>::from_fen_placement(TENCUBED_START_PLACEMENT)
            .expect("the Ten-Cubed starting placement is valid on a 10x10 board");
        let state = GenericState {
            turn: Color::White,
            // Ten-Cubed has no castling.
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
        color: Color,
        sq: Square<Grand10x10>,
        occupancy: Bitboard<Grand10x10>,
    ) -> Bitboard<Grand10x10> {
        match role {
            // Wizard (Camel + Ferz) and Champion (Wazir + Alfil + Dabbaba): pure
            // leapers over any intervening piece.
            WideRole::Wizard => leaper_attacks::<Grand10x10>(sq, &WIZARD_OFFSETS),
            WideRole::TencubedChampion => leaper_attacks::<Grand10x10>(sq, &CHAMPION_OFFSETS),
            // The Marshal (Elephant), Archbishop (Hawk), standard army, and pawn
            // defer to the trait default (`StandardChess` overrides no movement).
            _ => {
                <StandardChess as WideVariant<Grand10x10>>::role_attacks(role, color, sq, occupancy)
            }
        }
    }

    fn promotion_config() -> PromotionConfig {
        // A Ten-Cubed pawn promotes only to a Queen, Marshal, or Archbishop — never
        // to a Rook, Knight, Bishop, Wizard, or Champion — and with no captured-type
        // limit (confirmed against FSF). The order only affects move enumeration
        // order, not the perft leaf count.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Queen,
                WideRole::Elephant, // Marshal (R+N)
                WideRole::Hawk,     // Archbishop (B+N)
            ],
        }
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

    /// The Champion ([`WideRole::TencubedChampion`]) leaps two squares straight
    /// (Dabbaba) and two diagonally (Alfil) — jumps **collinear** with a pin ray —
    /// so a pinned Champion could otherwise vault past its own king to a far-side
    /// square on the pin line. Confine every pinned piece to the king–pinner
    /// segment so that cannot happen (matching Fairy-Stockfish); ordinary sliders
    /// are unaffected, as occupancy already limits them to the segment.
    fn confine_pins_to_segment() -> bool {
        true
    }

    /// Ten-Cubed keeps the standard chess army plus the always-mating Archbishop
    /// ([`WideRole::Hawk`]), Marshal ([`WideRole::Elephant`]), Wizard, and Champion,
    /// so the ordinary insufficient-material draw applies on the 10x10 board: king
    /// vs king, king and a lone minor (bishop or knight) vs king, and same-colour
    /// bishops only. Every fairy piece counts as mating material (any non-minor,
    /// non-king occupant makes the position sufficient). Adjudication-only and behind
    /// the default-off hook, so perft stays byte-identical.
    fn is_insufficient_material<const R: usize>(
        board: &Board<Grand10x10, R>,
        _state: &GenericState<Grand10x10, R>,
    ) -> bool {
        crate::geometry::variant::standard_insufficient_material(board)
    }
}

/// Ten-Cubed chess as a [`GenericPosition`] over the 10x10 [`Grand10x10`] geometry.
///
/// Construct the starting position with
/// [`Tencubed::startpos`](GenericPosition::startpos) or parse a FEN (mcr dialect)
/// with [`Tencubed::from_fen`](GenericPosition::from_fen). See the [module
/// docs](self) for the army (Wizard + Champion leapers), the no-castling / pawn
/// rules, and the restricted last-rank promotion set.
pub type Tencubed = GenericPosition<
    Grand10x10,
    TencubedRules,
    { <TencubedRules as WideVariant<Grand10x10>>::ROLE_SPAN },
>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::perft as gperft;
    use crate::geometry::Square as Sq;
    use alloc::vec::Vec;

    /// The set of destination-square indices reachable from `sq` by a piece of the
    /// side to move, on a board where that piece is otherwise alone (kings tucked
    /// away), i.e. its full leaper target set.
    fn targets_from(fen: &str, file: u8, rank: u8) -> Vec<u8> {
        let pos = Tencubed::from_fen(fen).expect("valid Ten-Cubed FEN");
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

    /// Builds a sorted index list from `(file, rank)` pairs (0-based).
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

    /// The startpos round-trips through the mcr dialect FEN.
    #[test]
    fn startpos_round_trips() {
        let pos = Tencubed::startpos();
        assert_eq!(
            pos.to_fen(),
            "2**x**wae**w**x2/1rnbqkbnr1/pppppppppp/10/10/10/10/PPPPPPPPPP/1RNBQKBNR1/2**X**WAE**W**X2 w - - 0 1"
        );
    }

    /// FSF-confirmed startpos perft counts (`UCI_Variant tencubed`, `go perft`).
    #[test]
    fn startpos_perft_matches_fsf() {
        let pos = Tencubed::startpos();
        assert_eq!(gperft::<Grand10x10, _, _>(&pos, 1), 40);
        assert_eq!(gperft::<Grand10x10, _, _>(&pos, 2), 1600);
        assert_eq!(gperft::<Grand10x10, _, _>(&pos, 3), 68230);
    }

    /// The Wizard leaps to the eight Camel and four Ferz squares (twelve targets),
    /// jumping over any intervening piece. e6 = file 4, rank 5 (0-based).
    #[test]
    fn wizard_moves_are_camel_plus_ferz() {
        let got = targets_from("k9/10/10/10/4**W5/10/10/10/10/K9 w - - 0 1", 4, 5);
        // Camel from e6: d3 f3 b5 h5 b7 h7 d9 f9; Ferz: d5 f5 d7 f7.
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

    /// The Champion leaps to the four Wazir, four Dabbaba, and four Alfil squares.
    #[test]
    fn champion_moves_are_wazir_dabbaba_alfil() {
        let got = targets_from("k9/10/10/10/4**X5/10/10/10/10/K9 w - - 0 1", 4, 5);
        // From e6 — Wazir: e5 d6 f6 e7; Dabbaba: e4 c6 g6 e8; Alfil: c4 g4 c8 g8.
        let want = indices(&[
            (4, 4),
            (3, 5),
            (5, 5),
            (4, 6),
            (4, 3),
            (2, 5),
            (6, 5),
            (4, 7),
            (2, 3),
            (6, 3),
            (2, 7),
            (6, 7),
        ]);
        assert_eq!(got, want);
    }

    /// A leaper jumps over an intervening piece: a Champion on e6 still reaches e8
    /// (a Dabbaba jump) with a friendly pawn on e7 in the way.
    #[test]
    fn champion_jumps_over_a_blocker() {
        let got = targets_from("k9/10/10/4P5/4**X5/10/10/10/10/K9 w - - 0 1", 4, 5);
        // e8 (file 4, rank 7) is still a target despite the pawn on e7.
        let e8 = Sq::<Grand10x10>::from_file_rank(4, 7).unwrap().index();
        assert!(got.contains(&e8), "Champion leaps over the e7 pawn to e8");
    }

    /// Promotion is to a Queen, Marshal, or Archbishop only, on the last rank only.
    #[test]
    fn promotion_is_last_rank_queen_marshal_archbishop() {
        // A white pawn on e9 promotes on e10; a pawn on e8 may not promote.
        let pos =
            Tencubed::from_fen("5k4/4P5/10/10/10/10/10/10/10/5K4 w - - 0 1").expect("valid FEN");
        let mut promos: Vec<WideRole> = pos
            .legal_moves()
            .iter()
            .filter_map(|m| m.promotion())
            .collect();
        promos.sort_unstable();
        promos.dedup();
        let mut want = alloc::vec![WideRole::Queen, WideRole::Hawk, WideRole::Elephant];
        want.sort_unstable();
        assert_eq!(promos, want);
        // A pawn two ranks away (e8) never promotes when pushing to e9.
        let pos2 =
            Tencubed::from_fen("5k4/10/4P5/10/10/10/10/10/10/5K4 w - - 0 1").expect("valid FEN");
        assert!(pos2.legal_moves().iter().all(|m| m.promotion().is_none()));
    }
}
