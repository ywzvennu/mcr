//! Wolf chess (8x10) on the generic engine — an **eight-file by ten-rank** variant
//! with the Knight removed and a compound / rider army added. A Fairy-Stockfish
//! built-in (`UCI_Variant wolf`, derived from the standard-chess base). **The
//! available Fairy-Stockfish binary is a non-large-board build and does not
//! implement the 10-rank `wolf`** (it silently falls back to standard chess), so
//! this variant carries **no live FSF perft oracle**. Like the other oracle-less
//! variants (Okisaki Shogi, Gustav 3, Wa Shogi, Alice, Tenjiku; see
//! `docs/oracle-less-validation.md`) it is instead *rules-validated*:
//! `tests/perft_wolf.rs` hand-derives the start-position move count and cross-checks
//! the engine's perft against a fully **independent, from-scratch 8x10 move
//! generator**.
//!
//! ## Pieces (confirmed against FSF `wolf_variant()`)
//!
//! The board is 8 files (a–h) by 10 ranks. The Knight is removed; the army is:
//!
//! * **King (K)** — a standard king. No castling in this variant.
//! * **Queen (Q)**, **Rook (R)**, **Bishop (B)** — orthodox.
//! * **Wolf (FSF `w`, a Chancellor = Rook + Knight)** — reuses
//!   [`WideRole::Elephant`] (mcr's Rook+Knight compound), FEN letter `e`.
//! * **Fox (FSF `f`, an Archbishop = Bishop + Knight)** — reuses [`WideRole::Hawk`]
//!   (mcr's Bishop+Knight compound), FEN letter `a`.
//! * **Nightrider (FSF `n`, Betza `NN`)** — a riding knight ([`WideRole::Nightrider`],
//!   token `****n`): leaps like a knight and continues in the same knight-direction
//!   over empty squares until blocked.
//! * **Sergeant (FSF `s`, Betza `fKifmnD`)** — a new role ([`WideRole::Sergeant`],
//!   token `****y`): moves/captures one step straight or diagonally **forward** (a
//!   forward King: N, NE, NW), plus an **initial** two-square straight advance
//!   (move-only, the skipped square must be empty), available only from the
//!   double-step region — exactly a pawn's double step.
//! * **Wolf Elephant (FSF `e`, Betza `NNQ`)** — a new role
//!   ([`WideRole::WolfElephant`], token `****z`): a **Nightrider + Queen** compound.
//!   It is absent from the start array and reachable only by pawn promotion.
//!
//! ## Pawns, double step, en passant
//!
//! Standard chess pawns (single forward step, diagonal captures, en passant). The
//! double step follows FSF's `doubleStepRegion`: a pawn's whole home rank (rank 2
//! White / rank 9 Black) **plus** the four inner files **b, c, f, g** one rank
//! further forward (rank 3 White / rank 8 Black), where the start array's advanced
//! pawns stand. The **Sergeant** shares that same region for its initial two-square
//! advance. See [`in_double_step_region`](WolfRules::in_double_step_region).
//!
//! ## Promotion
//!
//! A pawn reaching the last rank promotes to a **Queen**, **Wolf** (Chancellor),
//! **Fox** (Archbishop), **Rook**, **Bishop**, or **Wolf Elephant** (`NNQ`) — six
//! targets. (FSF's five `promotionPieceTypes` `q`/`w`/`f`/`r`/`b` plus the
//! `promotedPieceType[PAWN]` = Wolf Elephant, which FSF reaches by a piece-promotion
//! move; the resulting piece and node count are identical.)
//!
//! **Known simplification:** FSF also lists the Sergeant in `promotionPawnTypes`, so
//! a Sergeant reaching the last rank promotes there. That is unreachable in the
//! validated low-depth perft (a Sergeant is a forward-only stepper eight+ ranks from
//! its target at the root) and is not modeled here; the Sergeant's movement, the
//! pawn promotion set, and king safety are complete.
//!
//! ## King safety — the full-verify path
//!
//! Both the Nightrider and the Wolf Elephant ride **knight-rays**, which are not the
//! king's rank / file / diagonals, so the line-based pin / check-interposition
//! machinery cannot express their king-safety. This variant therefore opts into
//! [`WideVariant::needs_full_verify`], routing every move through the per-move
//! make/unmake `king_safe_after` re-test, whose reverse projection of the riders'
//! symmetric, occupancy-aware attack sets sees their checks and pins exactly (as in
//! Nightrider chess).
//!
//! ## Confirmed starting FEN
//!
//! FSF `wolf_variant()` gives the start as
//!
//! ```text
//! FSF dialect: qwfrbbnk/pssppssp/1pp2pp1/8/8/8/8/1PP2PP1/PSSPPSSP/KNBBRFWQ w - - 0 1
//! ```
//!
//! In mcr's role letters the Wolf is `e` (Rook+Knight Elephant), the Fox is `a`
//! (Bishop+Knight Hawk), the Nightrider is `****n`, and the Sergeant is `****y`, so
//! the same placement is spelled with those tokens (see [`WOLF_PLACEMENT`]).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Geometry, PromotionConfig, Square, WideRole, WideVariant,
};
use crate::Color;

use super::super::Wolf8x10;

/// The confirmed Wolf starting placement in mcr's role letters: the Wolf (Chancellor)
/// is `e`, the Fox (Archbishop) is `a`, the Nightrider is `****n`, and the Sergeant
/// is `****y`. Same position as FSF's `qwfrbbnk/pssppssp/1pp2pp1/8/8/8/8/1PP2PP1/PSSPPSSP/KNBBRFWQ`.
pub const WOLF_PLACEMENT: &str = "qearbb****nk/p****y****ypp****y****yp/1pp2pp1/8/8/8/8/1PP2PP1/P****Y****YPP****Y****YP/K****NBBRAEQ";

/// The Wolf-chess rule layer: a zero-sized [`WideVariant`] over [`Wolf8x10`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct WolfRules;

impl WolfRules {
    /// The four inner files **b, c, f, g** (0-based `1, 2, 5, 6`) whose advanced
    /// pawns / sergeants form the second (further-forward) rank of the start array
    /// and share the double-step region.
    const INNER_FILES: [u8; 4] = [1, 2, 5, 6];

    /// Returns `true` if a piece of `color` on `sq` stands in the variant's
    /// double-step region (Fairy-Stockfish's `doubleStepRegion`): the pawns' whole
    /// home rank, **plus** the four inner files one rank further forward. Both the
    /// Pawn's and the Sergeant's two-square advance are gated by this.
    #[must_use]
    pub fn in_double_step_region(color: Color, sq: Square<Wolf8x10>) -> bool {
        let (home, advanced) = match color {
            // White: rank 2 (index 1) home; the b/c/f/g pawns start on rank 3 (index 2).
            Color::White => (1u8, 2u8),
            // Black: rank 9 (index 8) home; the b/c/f/g pawns start on rank 8 (index 7).
            Color::Black => (Wolf8x10::HEIGHT - 2, Wolf8x10::HEIGHT - 3),
        };
        let rank = sq.rank();
        rank == home || (rank == advanced && Self::INNER_FILES.contains(&sq.file()))
    }

    /// The Sergeant's attack / step set from `sq` for `color`: the three **forward**
    /// King steps (straight ahead and both forward diagonals — N, NE, NW for White).
    /// It moves and captures on all three. The initial two-square advance is a quiet
    /// move only and comes from [`quiet_only_targets`](WolfRules::quiet_only_targets),
    /// not this set.
    fn sergeant_attacks(color: Color, sq: Square<Wolf8x10>) -> Bitboard<Wolf8x10> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        let offsets = [(0, fwd), (1, fwd), (-1, fwd)];
        attacks::leaper_attacks::<Wolf8x10>(sq, &offsets)
    }
}

impl WideVariant<Wolf8x10> for WolfRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field — its highest-indexed role is the promotion-only Wolf
    /// Elephant ([`WideRole::WolfElephant`]), so the span reaches it. See
    /// [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = WideRole::WolfElephant.index() + 1;

    fn starting_position() -> (Board<Wolf8x10>, GenericState<Wolf8x10>) {
        let board = Board::<Wolf8x10>::from_fen_placement(WOLF_PLACEMENT)
            .expect("the Wolf starting placement is valid on an 8x10 board");
        let state = GenericState {
            turn: Color::White,
            // No castling in Wolf chess.
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
            jieqi_seed: None,
        };
        (board, state)
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Wolf8x10>,
        occupancy: Bitboard<Wolf8x10>,
    ) -> Bitboard<Wolf8x10> {
        match role {
            // The Sergeant's forward-King move/capture set; its double step is a
            // quiet-only extra (see `quiet_only_targets`).
            WideRole::Sergeant => Self::sergeant_attacks(color, sq),
            // The Nightrider (Betza `NN`): rides each knight direction until blocked.
            WideRole::Nightrider => attacks::nightrider_attacks::<Wolf8x10>(sq, occupancy),
            // The Wolf Elephant (Betza `NNQ`): a Nightrider ride unioned with a Queen
            // slide. Both components are symmetric and occupancy-aware.
            WideRole::WolfElephant => {
                attacks::nightrider_attacks::<Wolf8x10>(sq, occupancy)
                    | attacks::queen_attacks::<Wolf8x10>(sq, occupancy)
            }
            // Everything else is standard: the default handles Pawn, Bishop, Rook,
            // Queen, King, and the two census compounds — the Fox (Hawk, Bishop+Knight)
            // and the Wolf (Elephant, Rook+Knight).
            _ => <crate::geometry::StandardChess as WideVariant<Wolf8x10>>::role_attacks(
                role, color, sq, occupancy,
            ),
        }
    }

    fn role_is_slider(role: WideRole) -> bool {
        // The line-sliders that can pin / be pinned along a rank / file / diagonal:
        // Rook, Bishop, Queen, the Fox (Hawk, bishop slides), the Wolf (Elephant,
        // rook slides), and the Wolf Elephant (its Queen component). The pure
        // knight-ray Nightrider has no line-slide and, like the Wolf Elephant's own
        // knight-rays, is handled by the full-verify path, not the line machinery.
        matches!(
            role,
            WideRole::Rook
                | WideRole::Bishop
                | WideRole::Queen
                | WideRole::Hawk
                | WideRole::Elephant
                | WideRole::WolfElephant
        )
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // The forward-biased steppers whose attack set must be reverse-projected with
        // the opposite colour: the chess Pawn (diagonal capture) and the Sergeant
        // (forward King). Every other Wolf piece is colour-symmetric.
        matches!(role, WideRole::Pawn | WideRole::Sergeant)
    }

    fn pawn_may_double_push_from_sq(sq: Square<Wolf8x10>, color: Color) -> bool {
        Self::in_double_step_region(color, sq)
    }

    fn quiet_only_targets(
        role: WideRole,
        color: Color,
        sq: Square<Wolf8x10>,
        occupancy: Bitboard<Wolf8x10>,
    ) -> Bitboard<Wolf8x10> {
        // The Sergeant's **initial two-square straight advance** (Betza `ifmnD`): a
        // move-only, non-jumping (lame) double step available only from the
        // double-step region. The skipped square must be empty; the landing square's
        // emptiness is enforced by the generic generator (`& !occupied`).
        if role != WideRole::Sergeant || !Self::in_double_step_region(color, sq) {
            return Bitboard::EMPTY;
        }
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        let Some(mid) = sq.offset(0, fwd) else {
            return Bitboard::EMPTY;
        };
        if occupancy.contains(mid) {
            return Bitboard::EMPTY;
        }
        match sq.offset(0, 2 * fwd) {
            Some(two) => Bitboard::EMPTY.with(two),
            None => Bitboard::EMPTY,
        }
    }

    fn promotion_config() -> PromotionConfig {
        // FSF `promotionPieceTypes` q/w/f/r/b (Queen, Wolf=Elephant, Fox=Hawk, Rook,
        // Bishop) plus the `promotedPieceType[PAWN]` Wolf Elephant (`NNQ`) — six
        // targets. There is no ordinary Knight in this army.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Queen,
                WideRole::Elephant,
                WideRole::Hawk,
                WideRole::Rook,
                WideRole::Bishop,
                WideRole::WolfElephant,
            ],
        }
    }

    fn has_castling() -> bool {
        false
    }

    fn needs_full_verify() -> bool {
        // The Nightrider and the Wolf Elephant ride knight-rays, which are not board
        // lines; every move is re-tested by the authoritative `king_safe_after`.
        true
    }

    /// The western **fifty-move rule** (FSF's standard-chess-base `nMoveRule = 50`):
    /// a halfmove clock of 100 plies with no capture or pawn move is a draw.
    /// Adjudication-only, so perft stays byte-identical.
    fn move_rule_plies() -> Option<u16> {
        Some(100)
    }
}

/// Wolf chess (8x10 compound + rider army) as a [`GenericPosition`] over the
/// [`Wolf8x10`] geometry.
///
/// Construct the starting position with [`Wolf::startpos`](GenericPosition::startpos)
/// or parse a FEN (mcr dialect) with [`Wolf::from_fen`](GenericPosition::from_fen).
/// See the [module docs](self) for the army, the file-restricted double-step region,
/// the six-target pawn promotion, and the full-verify king safety.
pub type Wolf =
    GenericPosition<Wolf8x10, WolfRules, { <WolfRules as WideVariant<Wolf8x10>>::ROLE_SPAN }>;
