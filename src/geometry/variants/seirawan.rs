//! Seirawan chess (S-Chess, 8x8) on the generic engine — the first **gating**
//! variant on the [`WideVariant`] layer (`docs/fairy-variants-architecture.md`,
//! Phase 1, §4.4). It exercises the reserve / gating mechanic the generic engine
//! gained for this variant, validated against Fairy-Stockfish.
//!
//! Seirawan is standard 8x8 chess plus two extra pieces held **in reserve** off
//! the board, one of each per side:
//!
//! * **Hawk** ([`WideRole::Hawk`], Bishop + Knight) — FSF / mcr letter `H`/`h`.
//! * **Elephant** ([`WideRole::Elephant`], Rook + Knight) — FSF / mcr letter
//!   `E`/`e`.
//!
//! Their movement is already the [`WideVariant`] default (`bishop | knight` and
//! `rook | knight`), so no `role_attacks` override is needed.
//!
//! ## Gating
//!
//! When a piece standing on its **original back-rank square** makes its first
//! move, the player **may** simultaneously place ("gate") one reserve piece onto
//! the square the piece just vacated. Gating is optional and each reserve is
//! placed at most once. Castling counts as a first move for **both** the king and
//! the castling rook, so a castle may gate onto the king's *or* the rook's
//! vacated square (one, never both).
//!
//! Gating never rescues an otherwise-illegal move: the base move must be legal on
//! its own (a gated piece may not block a check or shield the king), which is
//! exactly how the generic engine emits gates — it augments already-legal base
//! moves, so the gate can only *add* an option. See
//! [`GenericPosition`]'s movegen and `apply`.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant seirawan`:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[HEhe] w KQBCDFGkqbcdfg - 0 1
//! ```
//!
//! mcr uses the **same dialect** FSF does for S-Chess (`H` Hawk, `E` Elephant),
//! so the FEN is byte-identical — no rewrite is needed in `compare-fairy/`.
//!
//! The two FEN extensions over plain chess:
//!
//! * **Holdings** `[HEhe]` after the placement: the reserves in hand (white `HE`,
//!   black `he`).
//! * **Gating rights in the castling field** `KQBCDFGkqbcdfg`: the `KQkq` letters
//!   are the usual castling rights (which *also* make the rook squares and the
//!   unmoved king square gating-eligible), and the file letters `BCDFG` /
//!   `bcdfg` mark the remaining gating-eligible back-rank squares — every
//!   original back-rank file in the start position (the a/e/h files are implied
//!   by the castling letters, so they are not re-listed, matching the FSF
//!   dialect). The generic [`from_fen`](crate::geometry::GenericPosition::from_fen)
//!   parses both extensions when [`WideVariant::supports_gating`] is `true`.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    Bitboard, Board, Chess8x8, Geometry, PromotionConfig, Square, WideRole, WideVariant,
};
use crate::Color;

/// The Seirawan rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides only what Seirawan adds to standard chess: the reserves and
/// gating-eligible squares of the opening (via [`WideVariant::supports_gating`]
/// and [`WideVariant::initial_gating`]) and the widened promotion set (a pawn may
/// also promote to a Hawk or Elephant). The Hawk (`B+N`) and Elephant (`R+N`)
/// movement is already the trait default, so there is no `role_attacks` override;
/// every other rule — pawns, knights, sliders, the king, castling — is standard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SeirawanRules;

/// The standard 8x8 starting placement (Seirawan shares the chess array; the
/// reserves live in hand, not on the board).
const SEIRAWAN_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

impl SeirawanRules {
    /// The gating-eligible back-rank square set for a fresh game: every square on
    /// white's rank 0 and black's rank 7 (all eight original back-rank pieces may
    /// gate on their first move).
    fn opening_eligible() -> Bitboard<Chess8x8> {
        let mut bb = Bitboard::<Chess8x8>::EMPTY;
        for file in 0..Chess8x8::WIDTH {
            if let Some(sq) = Square::<Chess8x8>::from_file_rank(file, 0) {
                bb.set(sq);
            }
            if let Some(sq) = Square::<Chess8x8>::from_file_rank(file, Chess8x8::HEIGHT - 1) {
                bb.set(sq);
            }
        }
        bb
    }
}

impl WideVariant<Chess8x8> for SeirawanRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 12;

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

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(SEIRAWAN_START_PLACEMENT)
            .expect("the Seirawan starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::standard::<Chess8x8>(),
            ep_square: None,
            ep_captured: None,
            gating: Self::initial_gating(),
            duck: None,
            placement: GenericPlacement::NONE,
            halfmove_clock: 0,
            fullmove_number: 1,
            consecutive_passes: 0,
            board_b: crate::geometry::Bitboard::EMPTY,
            petrified: crate::geometry::Bitboard::EMPTY,
            checks_against: [0, 0],
            jieqi_seed: None,
        };
        (board, state)
    }

    fn promotion_config() -> PromotionConfig {
        // A Seirawan pawn promotes to any non-pawn, non-king role of the army: the
        // four standard plus the two reserve compounds. FSF lists the reserve
        // pieces among the legal promotion targets, so the perft branching matches
        // only with the Hawk and Elephant included. The order affects only move
        // enumeration order, not the leaf count.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Knight,
                WideRole::Bishop,
                WideRole::Rook,
                WideRole::Queen,
                WideRole::Hawk,     // B+N reserve compound
                WideRole::Elephant, // R+N reserve compound
            ],
        }
    }

    fn supports_gating() -> bool {
        true
    }

    fn initial_gating() -> GenericGating<Chess8x8> {
        // Both reserves in hand for both colors; every original back-rank square
        // gating-eligible.
        GenericGating::new(Self::opening_eligible(), [true, true], [true, true])
    }

    /// Once **both** sides have spent their gating reserve, Seirawan is plain
    /// standard-army 8x8 chess, so the ordinary insufficient-material draw applies:
    /// king vs king, king and a lone minor (bishop or knight) vs king, and
    /// same-colour bishops only. The Hawk (B+N) and Elephant (R+N) count as mating
    /// material, exactly as the `standard_insufficient_material` helper classifies
    /// the census compounds.
    ///
    /// **The reserve gates the material.** While **either** side still holds a Hawk
    /// or Elephant in reserve, that piece can still enter the board on a back-rank
    /// piece's first move, so the material is never settled and the position is
    /// **never** insufficient — this guard reproduces Fairy-Stockfish's
    /// `has_insufficient_material`, whose very first test returns "sufficient" while
    /// `count_in_hand(c, ALL_PIECES)` is non-zero (verified against FSF
    /// `UCI_Variant seirawan`: `KvK[He]` and `K+B vs K` with a lone enemy reserve
    /// are both *not* drawn, while the same positions with empty reserves are). Only
    /// when neither reserve remains does the standard material test decide.
    ///
    /// Adjudication-only and behind the default-off hook, so perft stays
    /// byte-identical.
    fn is_insufficient_material<const R: usize>(
        board: &Board<Chess8x8, R>,
        state: &GenericState<Chess8x8, R>,
    ) -> bool {
        if state.gating.any_reserve(Color::White) || state.gating.any_reserve(Color::Black) {
            return false;
        }
        crate::geometry::variant::standard_insufficient_material(board)
    }
}

/// Seirawan chess (S-Chess) as a [`GenericPosition`] over the 8x8 [`Chess8x8`]
/// geometry.
///
/// Construct the starting position with
/// [`Seirawan::startpos`](GenericPosition::startpos) or parse a FEN — including
/// the `[HEhe]` holdings and the `KQBCDFGkqbcdfg` gating-rights extensions — with
/// [`Seirawan::from_fen`](GenericPosition::from_fen). The Hawk and Elephant reuse
/// the generic compound movement defaults; only the reserves, gating, and the
/// widened promotion set distinguish it from standard chess.
pub type Seirawan = GenericPosition<
    Chess8x8,
    SeirawanRules,
    { <SeirawanRules as WideVariant<Chess8x8>>::ROLE_SPAN },
>;

#[cfg(test)]
mod insufficient_material_tests {
    use super::Seirawan;
    use crate::geometry::{WideEndReason, WideOutcome};

    fn end_reason(fen: &str) -> Option<WideEndReason> {
        Seirawan::from_fen(fen)
            .expect("valid seirawan fen")
            .end_reason()
    }

    // --- reserve empty: the standard material draws apply ------------------

    #[test]
    fn lone_kings_empty_reserve_draw() {
        let pos = Seirawan::from_fen("5k2/8/8/8/8/8/8/5K2[] w - - 0 1").expect("valid fen");
        assert_eq!(pos.end_reason(), Some(WideEndReason::InsufficientMaterial));
        assert_eq!(pos.outcome(), Some(WideOutcome::Draw));
    }

    #[test]
    fn king_and_single_minor_empty_reserve_draw() {
        // K + N vs K and K + B vs K with no reserve left are dead draws.
        assert_eq!(
            end_reason("5k2/8/8/8/8/8/8/5KN1[] w - - 0 1"),
            Some(WideEndReason::InsufficientMaterial)
        );
        assert_eq!(
            end_reason("5k2/8/8/8/8/8/8/5KB1[] w - - 0 1"),
            Some(WideEndReason::InsufficientMaterial)
        );
    }

    #[test]
    fn same_colour_bishops_empty_reserve_draw() {
        // White Ba1 (dark) and black Bh8 (dark): one complex, no mate possible.
        assert_eq!(
            end_reason("4k2b/8/8/8/8/8/8/B4K2[] w - - 0 1"),
            Some(WideEndReason::InsufficientMaterial)
        );
    }

    // --- reserve NON-empty: never insufficient (a piece can still gate) ----

    #[test]
    fn lone_kings_with_reserve_not_insufficient() {
        // A reserve Hawk (white) and Elephant (black) can still enter on a
        // back-rank first move, so the bare-king position is not yet settled. FSF
        // `UCI_Variant seirawan` agrees: `KvK[He]` is not insufficient.
        assert_eq!(end_reason("5k2/8/8/8/8/8/8/5K2[He] w - - 0 1"), None);
    }

    #[test]
    fn single_minor_with_enemy_reserve_not_insufficient() {
        // White is a lone king-and-bishop but Black still holds an Elephant in
        // reserve, so Black is not insufficient and the game is not drawn — matching
        // FSF, whose per-colour test reports Black sufficient while its hand is
        // non-empty.
        assert_eq!(end_reason("5k2/8/8/8/8/8/8/5KB1[e] w - - 0 1"), None);
    }

    // --- material controls (empty reserve, still sufficient) ---------------

    #[test]
    fn opposite_colour_bishops_are_sufficient() {
        // White Ba1 (dark) vs black Bg8 (light): opposite complexes can mate.
        assert_eq!(end_reason("4k1b1/8/8/8/8/8/8/B4K2[] w - - 0 1"), None);
    }

    #[test]
    fn rook_and_compounds_are_sufficient() {
        // A lone rook mates; the Hawk (B+N) and Elephant (R+N) compounds are mating
        // material even with the reserve spent.
        assert_eq!(end_reason("5k2/8/8/8/8/8/8/5KR1[] w - - 0 1"), None);
        // A board Hawk is the global `a` letter (the `H`/`E` dialect is the reserve
        // bracket only); earlier this used `H`, which is Hoplite — a role Seirawan
        // never fields, now past the exact role span (#580).
        assert_eq!(end_reason("5k2/8/8/8/8/8/8/5Ka1[] w - - 0 1"), None);
        assert_eq!(end_reason("5k2/8/8/8/8/8/8/5KE1[] w - - 0 1"), None);
    }
}
