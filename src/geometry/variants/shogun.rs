//! Shogun (8x8) on the generic engine — a **shogi-chess hybrid**: standard chess
//! movement plus a **crazyhouse hand with drops** (a captured piece banks to the
//! captor's hand and may be dropped back) and a **shogi-style promotion zone**
//! where a piece reaching the far ranks may upgrade into a stronger compound.
//! Validated against Fairy-Stockfish `UCI_Variant shogun`.
//!
//! Shogun is, structurally, crazyhouse on the standard 8x8 array with one twist:
//! instead of pawns promoting to `N/B/R/Q`, **every base piece promotes — by a
//! shogi `+` move — into a single stronger form**, and the strong forms are
//! **capped** at one of each on the board.
//!
//! ## Pieces (confirmed against FSF; promoted forms in parentheses)
//!
//! The full chess army moves exactly as in chess:
//!
//! * **King (K)**, **Rook (R)**, **Bishop (B)**, **Knight (N)** — standard.
//! * **Pawn (P)** — a standard chess pawn (double push, diagonal capture, en
//!   passant); promotes into a **Commoner**.
//! * **Fers / "Queen" (the start `+f`)** — the d-file piece of the start array is
//!   a **promoted Fers**, which moves as a full Queen. In mcr it is the
//!   [`WideRole::Queen`] (a promoted [`WideRole::Met`]); on capture it banks to the
//!   hand as a Met (fers), and a bare Met dropped from hand promotes back into a
//!   Queen.
//!
//! Each base piece's promoted form (all reuse existing roles — Shogun adds **no
//! new role**):
//!
//! | base | promotes to | mcr role | moves like |
//! |------|-------------|----------|------------|
//! | Pawn   | Commoner   | [`WideRole::Commoner`] (`*u`) | a King (non-royal, eight one-steps) |
//! | Knight | Centaur    | [`WideRole::Kheshig`] (`w`)    | King + Knight |
//! | Bishop | Archbishop | [`WideRole::Hawk`] (`a`)       | Bishop + Knight |
//! | Rook   | Chancellor | [`WideRole::Elephant`] (`e`)   | Rook + Knight |
//! | Met    | Queen      | [`WideRole::Queen`] (`q`)      | Rook + Bishop |
//!
//! ## Promotion zone (optional, capped)
//!
//! The promotion zone is the **far three ranks** from each side: ranks 6-8
//! (0-based 5-7) for White, ranks 1-3 (0-based 0-2) for Black. A Pawn, Knight,
//! Bishop, Rook, or Fers whose move **starts or ends** in the zone *may* promote
//! (optional) — both the promoting and the non-promoting move are legal — with two
//! exceptions:
//!
//! * **Forced** only for a Pawn reaching the **last rank** (rank 8 / rank 1): an
//!   unpromoted pawn there would be immobile (`immobilityIllegal`), so promotion
//!   to a Commoner is the only move.
//! * **Capped** by FSF's `promotionLimit = g:1 a:1 m:1 q:1`: a Knight / Bishop /
//!   Rook / Fers may **not** promote while the side already holds one Centaur /
//!   Archbishop / Chancellor / Queen on the board; only the non-promoting move is
//!   then offered. The Commoner (promoted Pawn) is **uncapped**.
//!
//! The King, the Queen (already promoted), and any already-promoted piece never
//! promote. The Pawn rides the standard pawn generator (its lone promotion target
//! is the Commoner); the other four ride the generic per-piece promotion path.
//!
//! ## Hand and drops (crazyhouse)
//!
//! A captured piece flips to the captor's side and enters the **hand**, reverted
//! to its **base** role (a captured Archbishop banks a Bishop, a captured Queen a
//! Met). From the hand a side may **drop** a held piece onto an empty square in
//! its **drop region**: ranks 1-5 (0-based 0-4) for White, ranks 4-8 (0-based 3-7)
//! for Black. The region is the same for every piece type — there is **no nifu**
//! (a second pawn may share a file) and a Pawn may drop on the **first rank**.
//! Because White's drop region (ranks 1-5) never reaches its promotion zone (ranks
//! 6-8), a dropped piece always lands unpromoted; dropping a Pawn where it would
//! be immobile is impossible (the last rank is outside the drop region). Drops
//! giving check or mate are legal (no *uchifuzume*).
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant shogun`, `position startpos`) renders the start as
//!
//! ```text
//! rnb+fkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNB+FKBNR[] w KQkq - 0 1
//! ```
//!
//! The `+f` / `+F` is a **promoted Fers** that moves as a Queen. mcr represents it
//! with the [`WideRole::Queen`] token, so its canonical start FEN is exactly the
//! standard chess array with an empty holdings bracket:
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[] w KQkq - 0 1
//! ```
//!
//! The two are the same position; the `compare-fairy/` harness translates the
//! promoted-piece tokens (mcr's `q`/`w`/`a`/`e`/`*u` ↔ FSF's `+f`/`+n`/`+b`/`+r`/`+p`,
//! and the bare Met `m` ↔ FSF's fers `f`) when driving FSF.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Chess8x8, Geometry, PromotionConfig, Square, StandardChess, WideRole,
    WideVariant,
};
use crate::Color;

/// The Shogun rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ShogunRules;

/// The confirmed Shogun starting placement: the standard chess array (the d-file
/// "queen" is a promoted Fers, represented by the [`WideRole::Queen`] token). The
/// hand is empty at the start and rides in the FEN's `[..]` holdings bracket.
const SHOGUN_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

/// The four diagonal one-step (ferz) offsets — the Met's whole move and the
/// Queen's / Archbishop's diagonal component.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// The depth of the promotion zone: the far three ranks from each side.
const ZONE_DEPTH: u8 = 3;

/// The depth of the drop region: each side's near five ranks.
const DROP_DEPTH: u8 = 5;

/// The promotion cap per FSF `promotionLimit = g:1 a:1 m:1 q:1`: at most one each
/// of the Centaur, Archbishop, Chancellor, and Queen on the board. The Commoner
/// (promoted Pawn) is uncapped and absent here.
const PROMOTION_LIMIT: u8 = 1;

impl WideVariant<Chess8x8> for ShogunRules {
    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(SHOGUN_PLACEMENT)
            .expect("the Shogun starting placement is valid on an 8x8 board");
        // Standard chess castling rights (both sides, both wings); the hand starts
        // empty.
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::standard::<Chess8x8>(),
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
        sq: Square<Chess8x8>,
        occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        match role {
            // Met (fers): one diagonal step. The bare fers only arises in hand /
            // FEN; on the board it appears promoted (as the Queen below).
            WideRole::Met => attacks::leaper_attacks::<Chess8x8>(sq, &FERZ_OFFSETS),
            // Commoner (promoted Pawn): a King's eight one-steps, non-royal.
            WideRole::Commoner => attacks::king_attacks::<Chess8x8>(sq),
            // Centaur (promoted Knight) = Kheshig: King + Knight.
            WideRole::Kheshig => {
                attacks::king_attacks::<Chess8x8>(sq) | attacks::knight_attacks::<Chess8x8>(sq)
            }
            // The Archbishop (promoted Bishop) = Hawk (B+N), the Chancellor
            // (promoted Rook) = Elephant (R+N), the Queen (promoted Fers), the
            // standard chess pieces, and the kings all use the standard movement.
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn role_is_slider(role: WideRole) -> bool {
        match role {
            // The Met and Commoner and Centaur are pure steppers/leapers.
            WideRole::Met | WideRole::Commoner | WideRole::Kheshig => false,
            // The Queen, Archbishop (Hawk), Chancellor (Elephant) and every
            // standard role keep the default classification (their sliding
            // component can be pinned).
            _ => <StandardChess as WideVariant<Chess8x8>>::role_is_slider(role),
        }
    }

    // --- promotion zone (optional, per-piece, capped) ---------------------

    fn promotion_config() -> PromotionConfig {
        // The Pawn's sole promotion target on the standard pawn path: the Commoner.
        PromotionConfig {
            roles: alloc::vec![WideRole::Commoner],
        }
    }

    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        // The far three ranks: 6-8 (indices 5-7) for White, 1-3 (indices 0-2) for
        // Black.
        match color {
            Color::White => rank >= Chess8x8::HEIGHT - ZONE_DEPTH,
            Color::Black => rank < ZONE_DEPTH,
        }
    }

    fn promotion_is_forced(color: Color, rank: u8) -> bool {
        // A Pawn (the only piece on the standard pawn path) is forced to promote
        // only on the last rank, where an unpromoted pawn would be immobile; the
        // near zone ranks (6, 7 for White) offer the plain push too.
        rank == <Self as WideVariant<Chess8x8>>::promotion_rank(color)
    }

    fn role_can_promote(role: WideRole) -> bool {
        // The non-pawn promotable base pieces (the Pawn promotes via the standard
        // pawn path, not the generic per-piece path). The King, the Queen (already
        // a promoted Fers), and every promoted form never promote.
        matches!(
            role,
            WideRole::Knight | WideRole::Bishop | WideRole::Rook | WideRole::Met
        )
    }

    fn role_promoted_to(role: WideRole) -> WideRole {
        match role {
            WideRole::Knight => WideRole::Kheshig, // Centaur
            WideRole::Bishop => WideRole::Hawk,    // Archbishop
            WideRole::Rook => WideRole::Elephant,  // Chancellor
            WideRole::Met => WideRole::Queen,
            other => other,
        }
    }

    fn role_promotion_blocked_by_limit(
        role: WideRole,
        color: Color,
        board: &Board<Chess8x8>,
    ) -> bool {
        // FSF `promotionLimit = g:1 a:1 m:1 q:1`: a piece may not promote while its
        // promoted form is already at the cap on the board. The Commoner (promoted
        // Pawn) is uncapped, but the Pawn never reaches this hook (it rides the
        // standard pawn path), so only the four capped base roles matter here.
        let promoted = Self::role_promoted_to(role);
        board.pieces(color, promoted).count() >= u32::from(PROMOTION_LIMIT)
    }

    // --- crazyhouse hand + drops ------------------------------------------

    fn has_hand() -> bool {
        true
    }

    fn pawn_is_stepper() -> bool {
        // Shogun pawns are ordinary chess pawns (double push, diagonal capture, en
        // passant), not Shogi forward-steppers; they promote into a Commoner.
        false
    }

    // `captures_to_hand` keeps its default `true` — Shogun banks captures
    // (crazyhouse). `role_hand_base` reverts each promoted form to its base.

    fn role_hand_base(role: WideRole) -> WideRole {
        // A captured promoted piece sheds its promotion before entering the hand
        // (FSF's `+X` token loses its `+`): Commoner → Pawn, Centaur → Knight,
        // Archbishop → Bishop, Chancellor → Rook, Queen → Met (fers). Every base
        // piece banks as itself.
        match role {
            WideRole::Commoner => WideRole::Pawn,
            WideRole::Kheshig => WideRole::Knight,
            WideRole::Hawk => WideRole::Bishop,
            WideRole::Elephant => WideRole::Rook,
            WideRole::Queen => WideRole::Met,
            other => other,
        }
    }

    fn drop_targets(role: WideRole, color: Color, board: &Board<Chess8x8>) -> Bitboard<Chess8x8> {
        // The dropping side's drop region (ranks 1-5 for White, 4-8 for Black) —
        // the same for every piece type. There is no nifu and no last-rank pawn
        // ban: the region simply never reaches the rank where a pawn would be
        // immobile, so no piece-specific filter is needed.
        let _ = role;
        !board.occupied() & Self::drop_region(color)
    }

    // --- Sennichite / perpetual check (default-off draw rules) -------------
    //
    // These affect only terminal adjudication in [`GenericGame`], never move
    // generation, so perft is byte-identical.

    fn tracks_repetition() -> bool {
        true
    }

    fn repetition_fold() -> usize {
        // Sennichite: the same position (including both hands) occurring a fourth
        // time is a draw.
        4
    }

    fn repetition_draw_reason() -> crate::geometry::WideEndReason {
        crate::geometry::WideEndReason::Sennichite
    }

    fn perpetual_check_loses() -> bool {
        // A sennichite brought about by perpetual check is a loss for the checking
        // side.
        true
    }
}

impl ShogunRules {
    /// The drop region for `color`: the near five ranks (1-5 for White, 4-8 for
    /// Black).
    fn drop_region(color: Color) -> Bitboard<Chess8x8> {
        let mut bb = Bitboard::<Chess8x8>::EMPTY;
        let ranks = match color {
            Color::White => 0..DROP_DEPTH,
            Color::Black => (Chess8x8::HEIGHT - DROP_DEPTH)..Chess8x8::HEIGHT,
        };
        for rank in ranks {
            for file in 0..Chess8x8::WIDTH {
                if let Some(sq) = Square::<Chess8x8>::from_file_rank(file, rank) {
                    bb.set(sq);
                }
            }
        }
        bb
    }
}

/// Shogun (8x8 shogi-chess hybrid) as a [`GenericPosition`] over the 8x8
/// [`Chess8x8`] geometry.
///
/// Construct the starting position (the standard chess array with an empty
/// crazyhouse hand) with [`Shogun::startpos`](GenericPosition::startpos) or parse
/// a FEN — the placement may carry the hand as a `[..]` holdings bracket — with
/// [`Shogun::from_fen`](GenericPosition::from_fen). See the [module docs](self)
/// for the promotion zone, the per-piece promotions and their cap, and the
/// crazyhouse hand and drops.
pub type Shogun = GenericPosition<Chess8x8, ShogunRules>;
