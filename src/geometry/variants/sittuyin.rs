//! Sittuyin (Burmese chess, 8x8) on the generic engine — the first variant
//! exercising the **setup / placement phase** and a **special promotion** on the
//! [`WideVariant`] layer (`docs/fairy-variants-architecture.md` §4.4). Validated
//! against Fairy-Stockfish `UCI_Variant sittuyin`.
//!
//! Sittuyin shares Makruk's pieces but adds two mechanics no earlier variant has:
//! a hand-placement opening and a Met promotion driven by the Met's own movement.
//!
//! ## Pieces
//!
//! * **Yathay** (rook) — a standard rook. ([`WideRole::Rook`])
//! * **Myin** (knight) — a standard knight. ([`WideRole::Knight`])
//! * **Sin** (elephant = silver-general, [`WideRole::Silver`]) — one step to any
//!   of the four diagonals plus one straight step **forward**, exactly Makruk's
//!   Khon. Reuses Makruk's [`role_attacks`](WideVariant::role_attacks).
//! * **Sit-ke / Met** (general = ferz, [`WideRole::Met`]) — one step to any of the
//!   four diagonals, exactly Makruk's Met.
//! * **Min Gyi** (king, [`WideRole::King`]) — a standard king.
//! * **Nè** (pawn) — moves one square straight forward, captures one square
//!   diagonally forward, no double-push and hence no en passant — exactly
//!   Makruk's Bia.
//!
//! There is no castling. The game is won by checkmate.
//!
//! ## Setup / placement phase
//!
//! The pawns start in a fixed Sittuyin arrangement (white on ranks 3-4 in an
//! interlocked block, black mirrored on ranks 5-6); the **eight non-pawn pieces
//! per side start off the board, in hand**. Players then alternate **placing**
//! one held piece per ply onto their own territory until both pockets are empty,
//! after which normal play begins. Confirmed against FSF, the constraints are:
//!
//! * **Territory** — the three ranks nearest the player (white ranks 0-2, black
//!   ranks 5-7), minus the squares its own pawns occupy. Any empty territory
//!   square is a legal drop.
//! * **Rooks** — confined to the **back rank** (white rank 0, black rank 7).
//! * **No check filtering.** FSF applies none during placement, so a drop is
//!   legal on any permitted empty square (a side has no king on the board until
//!   it deploys one).
//!
//! The phase is **per side**: a side that has emptied its pocket plays normally
//! even while the opponent is still deploying. The pocket rides in
//! [`GenericPlacement`], and the drop
//! path is gated behind [`WideVariant::has_placement`] (default-off), so every
//! other variant is byte-identical.
//!
//! ## Special promotion
//!
//! A Nè does **not** promote by reaching the far rank. Instead, while a side has
//! **no Met on the board**, any of its pawns may transform into a Met — the only
//! promotion role — either **in place** or by a single Met (ferz) step to an
//! empty diagonal square. (A diagonal step that would land on an enemy piece is a
//! normal pawn capture and does **not** promote.) Once a Met is on the board no
//! pawn may promote until it is captured again. This matches FSF move-for-move
//! and is emitted by the generic pawn generator via
//! [`special_promotion_targets`](WideVariant::special_promotion_targets), so the
//! promotions obey the same pin / check legality as every other move.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant sittuyin`, `position startpos`) renders the start as
//!
//! ```text
//! 8/8/4pppp/pppp4/4PPPP/PPPP4/8/8[KSSFRRNNkssfrrnn] w - - 0 1
//! ```
//!
//! mcr uses the same board placement and holdings bracket but its own role
//! letters (the Met is `m`, not FSF's `f`, and the bracket is written in role-
//! index order):
//!
//! ```text
//! 8/8/4pppp/pppp4/4PPPP/PPPP4/8/8[NNRRKMSSnnrrkmss] w - - 0 1
//! ```
//!
//! The two are the same position; the `compare-fairy/` harness translates the
//! `m`↔`f` Met letter when driving FSF.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    Bitboard, Board, Chess8x8, Geometry, PromotionConfig, Square, StandardChess, WideRole,
    WideVariant,
};
use crate::Color;

/// The Sittuyin rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides the Met and Sin movement (reusing Makruk's piece patterns), the
/// Makruk-style pawn rules, the absence of castling, the setup-phase pocket and
/// drop targets, and the special Met promotion. Everything else is the trait
/// default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SittuyinRules;

/// The confirmed Sittuyin pawn placement (the non-pawn pieces start in hand, so
/// the board carries only the interlocked pawn block).
const SITTUYIN_PAWN_PLACEMENT: &str = "8/8/4pppp/pppp4/4PPPP/PPPP4/8/8";

/// The four ferz (diagonal one-step) offsets — the Met's movement and the
/// diagonal component of the Sin.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// The number of placement ranks nearest each side (white ranks 0-2, black ranks
/// 5-7).
const TERRITORY_DEPTH: u8 = 3;

/// White's promotion region — the "X" of the two long diagonals (as `(file,
/// rank)` pairs, 0-based): a8, b7, c6, d5, e5, f6, g7, h8. A Met-promotion is only
/// available from a pawn standing here once the side has more than one pawn.
const PROMO_REGION_WHITE: [(u8, u8); 8] = [
    (0, 7),
    (1, 6),
    (2, 5),
    (3, 4),
    (4, 4),
    (5, 5),
    (6, 6),
    (7, 7),
];

/// Black's promotion region — the mirror of [`PROMO_REGION_WHITE`]: a1, b2, c3,
/// d4, e4, f3, g2, h1.
const PROMO_REGION_BLACK: [(u8, u8); 8] = [
    (0, 0),
    (1, 1),
    (2, 2),
    (3, 3),
    (4, 3),
    (5, 2),
    (6, 1),
    (7, 0),
];

impl SittuyinRules {
    /// The promotion-region ("X" diagonals) mask of `color`.
    fn promotion_region(color: Color) -> Bitboard<Chess8x8> {
        let squares = match color {
            Color::White => &PROMO_REGION_WHITE,
            Color::Black => &PROMO_REGION_BLACK,
        };
        let mut bb = Bitboard::<Chess8x8>::EMPTY;
        for &(file, rank) in squares {
            if let Some(sq) = Square::<Chess8x8>::from_file_rank(file, rank) {
                bb.set(sq);
            }
        }
        bb
    }

    /// The territory mask of `color`: the [`TERRITORY_DEPTH`] ranks nearest the
    /// player. For white this is ranks `0..3`; for black the top three ranks.
    fn territory(color: Color) -> Bitboard<Chess8x8> {
        let mut bb = Bitboard::<Chess8x8>::EMPTY;
        for d in 0..TERRITORY_DEPTH {
            let rank = match color {
                Color::White => d,
                Color::Black => Chess8x8::HEIGHT - 1 - d,
            };
            for file in 0..Chess8x8::WIDTH {
                if let Some(sq) = Square::<Chess8x8>::from_file_rank(file, rank) {
                    bb.set(sq);
                }
            }
        }
        bb
    }

    /// The back-rank mask of `color` (white rank 0, black rank 7) — the only
    /// squares a Rook may be placed on.
    fn back_rank_mask(color: Color) -> Bitboard<Chess8x8> {
        let rank = match color {
            Color::White => 0,
            Color::Black => Chess8x8::HEIGHT - 1,
        };
        let mut bb = Bitboard::<Chess8x8>::EMPTY;
        for file in 0..Chess8x8::WIDTH {
            if let Some(sq) = Square::<Chess8x8>::from_file_rank(file, rank) {
                bb.set(sq);
            }
        }
        bb
    }
}

impl WideVariant<Chess8x8> for SittuyinRules {
    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(SITTUYIN_PAWN_PLACEMENT)
            .expect("the Sittuyin pawn placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            // Sittuyin has no castling.
            castling: GenericCastling::NONE,
            ep_square: None,
            gating: GenericGating::NONE,
            duck: None,
            placement: Self::initial_placement(),
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
        sq: Square<Chess8x8>,
        occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        match role {
            // Met = ferz: the four diagonal one-steps. Sin = silver general: the
            // four diagonals plus one straight forward. Both are exactly Makruk's
            // pieces, so reuse the same patterns.
            WideRole::Met => {
                crate::geometry::attacks::leaper_attacks::<Chess8x8>(sq, &FERZ_OFFSETS)
            }
            WideRole::Silver => {
                let forward: i8 = if color.is_white() { 1 } else { -1 };
                let mut bb =
                    crate::geometry::attacks::leaper_attacks::<Chess8x8>(sq, &FERZ_OFFSETS);
                if let Some(dest) = sq.offset(0, forward) {
                    bb.set(dest);
                }
                bb
            }
            // Yathay / Myin / Min Gyi and the pawn are standard.
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // The Sin (Silver General) adds a single straight step **toward the far
        // rank** to its four diagonals (exactly Makruk's Khon), so its attack set
        // is color-directional — forward-biased, like a pawn's diagonal capture.
        // `attackers_to` must reverse-project it with the *opposite* color; the
        // default (Pawn / Hoplite only) would test the wrong forward direction and
        // miss a Sin check (the #201 class). The Met (ferz) is symmetric.
        matches!(role, WideRole::Pawn | WideRole::Silver)
    }

    fn promotion_config() -> PromotionConfig {
        // A Nè promotes only to a Met; there is no choice of role.
        PromotionConfig {
            roles: alloc::vec![WideRole::Met],
        }
    }

    fn in_promotion_zone(_color: Color, _rank: u8) -> bool {
        // Sittuyin has **no rank-based promotion**: a Nè reaching the far rank by
        // a straight push simply stays a pawn (it may later promote in place).
        // Every promotion is instead the Met-step / in-place transformation
        // emitted by `special_promotion_targets`. Reporting "never in the
        // promotion zone" makes the generic pawn generator treat a last-rank push
        // as a plain quiet move, matching FSF.
        false
    }

    fn double_push_rank(_color: Color) -> u8 {
        // The Nè never makes a double advance: place the trigger rank past the
        // board so the generic pawn generator's guard is never satisfied and no
        // en-passant target is ever set.
        Chess8x8::HEIGHT
    }

    fn has_castling() -> bool {
        false
    }

    // --- placement phase --------------------------------------------------

    fn has_placement() -> bool {
        true
    }

    fn initial_placement() -> GenericPlacement {
        // Each side deploys, by hand: 2 Knights, 2 Rooks, 1 King, 1 Met, 2 Sin.
        let mut counts = [0u8; WideRole::COUNT];
        counts[WideRole::Knight.index()] = 2;
        counts[WideRole::Rook.index()] = 2;
        counts[WideRole::King.index()] = 1;
        counts[WideRole::Met.index()] = 1;
        counts[WideRole::Silver.index()] = 2;
        GenericPlacement::new(counts, counts)
    }

    fn placement_targets(
        role: WideRole,
        color: Color,
        board: &Board<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        // Empty squares in the player's own territory; Rooks confined to the back
        // rank. (The own pawns sitting in the territory are excluded by the
        // "empty" filter, so no separate pawn mask is needed.)
        let mut mask = Self::territory(color) & !board.occupied();
        if role == WideRole::Rook {
            mask &= Self::back_rank_mask(color);
        }
        mask
    }

    fn special_promotion_targets(
        board: &Board<Chess8x8>,
        from: Square<Chess8x8>,
        color: Color,
    ) -> Option<Bitboard<Chess8x8>> {
        // A pawn may specially promote only while its side has **no Met** on the
        // board (the promotion limit is one Met, so a side with its Met already on
        // the board promotes nothing until it is captured).
        if !board.pieces(color, WideRole::Met).is_empty() {
            return None;
        }
        // Pawn eligibility: a side with **more than one pawn** may promote only a
        // pawn standing on its promotion region (the "X" of the long diagonals);
        // a side with exactly one pawn may promote that pawn from anywhere. (FSF:
        // `promotionPawns = count<PAWN> > 1 ? pawns & promotionZone : pawns`.)
        let pawns = board.pieces(color, WideRole::Pawn);
        if pawns.count() > 1 && !Self::promotion_region(color).contains(from) {
            return None;
        }
        let occupied = board.occupied();
        let ferz = crate::geometry::attacks::leaper_attacks::<Chess8x8>(from, &FERZ_OFFSETS);

        // A Met-promotion landing square is forbidden if it is **ferz-adjacent to
        // any enemy piece** — equivalently, if the promoted Met would stand on a
        // square an enemy attacks diagonally (a Met cannot be promoted into such
        // an en-prise square). Build the union of every enemy's ferz-step squares.
        let mut enemy_ferz_zone = Bitboard::<Chess8x8>::EMPTY;
        for sq in board.by_color(color.opposite()) {
            enemy_ferz_zone |=
                crate::geometry::attacks::leaper_attacks::<Chess8x8>(sq, &FERZ_OFFSETS);
        }

        // The diagonal Met steps: a one-step ferz move to an **empty** square not
        // in the forbidden zone (a step onto an enemy is a normal pawn capture,
        // not a promotion; a step onto a friend is blocked).
        let mut targets = ferz & !occupied & !enemy_ferz_zone;
        // The in-place promotion (the pawn stays on `from`) is available only when
        // `from` itself is not ferz-adjacent to an enemy.
        if !enemy_ferz_zone.contains(from) {
            targets = targets.with(from);
        }
        Some(targets)
    }
}

/// Sittuyin (Burmese chess) as a [`GenericPosition`] over the 8x8 geometry.
///
/// Construct the starting position (the fixed pawns plus both pockets in hand)
/// with [`Sittuyin::startpos`](GenericPosition::startpos) or parse a FEN — the
/// placement may carry the setup-phase pocket as a `[..]` holdings bracket — with
/// [`Sittuyin::from_fen`](GenericPosition::from_fen). See the [module docs](self)
/// for the placement phase and special promotion.
pub type Sittuyin = GenericPosition<Chess8x8, SittuyinRules>;
