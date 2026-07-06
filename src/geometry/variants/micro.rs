//! Micro Shogi (4x5 Shogi) on the generic engine — a compact reuse of the Shogi
//! (#190) **persistent hand** and **drops** on a new four-by-five (20-square)
//! [`Micro4x5`] geometry, with a promotion mechanic all its own: a piece **flips
//! form on every capture** rather than on reaching a zone. Validated against
//! Fairy-Stockfish `UCI_Variant micro`.
//!
//! Micro Shogi (5五将棋 / "micro shogi") is Shogi shrunk onto a 4-file by 5-rank
//! board with **one of every piece** per side, several of them starting
//! **pre-promoted**. Its defining rule is the **capture flip**: there is no
//! promotion zone; instead a piece toggles between its unpromoted and promoted
//! form **whenever — and only when — it captures**. A base piece promotes on its
//! capturing move, a promoted piece demotes; a quiet (non-capturing) move never
//! changes a piece's form.
//!
//! ## Pieces (confirmed against FSF; promoted forms in parentheses)
//!
//! One each per side. The promoted moves below are Micro Shogi's own (they are
//! *not* the standard Shogi promoted moves):
//!
//! * **King (K, 玉)** — a standard king; never flips.
//! * **Rook (R, 飛 → +R)** — a rook (orthogonal slider). On a capture it flips to
//!   **+R, which moves as a Gold General** (not the Shogi Dragon).
//! * **Bishop (B, 角 → +B)** — a bishop (diagonal slider). On a capture it flips to
//!   **+B, which also moves as a Gold General** (not the Shogi Horse).
//! * **Lance (L, 香 → +L)** — a Shogi Lance (slides any number of squares straight
//!   **forward**). On a capture it flips to **+L, which moves as a Silver General**
//!   (the four diagonals plus one straight forward).
//! * **Pawn (P, 歩 → +P)** — one step straight forward, capturing straight forward
//!   too (a Shogi pawn). On a capture it flips to **+P, which moves as a Shogi
//!   Knight** (the two forward 2-1 jumps).
//!
//! The board starts with each side's Rook and Lance already promoted (`+R`, `+L`).
//!
//! ## Promotion (the capture flip)
//!
//! There is **no promotion zone**: a non-capturing move never promotes, even onto
//! the far rank, and a piece may sit immobile on a square where it has no further
//! move (a Knight-moving +P on the last rank is legal — Micro Shogi has no
//! `immobilityIllegal` rule). Promotion is instead a **forced toggle on capture**:
//! the four flip pairs `Pawn ↔ +P`, `Lance ↔ +L`, `Bishop ↔ +B`, `Rook ↔ +R` each
//! swap form on any capturing move (FSF's `piecePromotionOnCapture` +
//! `mandatoryPiecePromotion` + `pieceDemotion`). The King has no alternate form and
//! never flips. The flip is applied by the generic
//! [`WideVariant::flips_on_capture`] hook after the move's legality is decided, so
//! it never affects the move's own legality — only the next position sees the
//! flipped role, and the move count is unchanged (a capture is one move, not two).
//!
//! ## Hand and drops
//!
//! A captured piece is banked **unpromoted** (a captured +R enters the hand as a
//! Rook) and flipped to the captor's side. On a turn a side may, instead of a board
//! move, **drop** a held piece onto **any empty square** — Micro Shogi imposes
//! *no* target restrictions: there is **no nifu** (two unpromoted Pawns may share a
//! file) and no dead-piece rule (a Pawn or the Knight-moving forms may be dropped
//! even on the last rank), so the default [`WideVariant::drop_targets`] (every empty
//! square) is used unchanged. It does, however, have **dual-form drops** (FSF
//! `dropPromoted`, the [`WideVariant::drops_can_promote`] hook): a held base piece
//! may be deployed either in its base form *or* in its promoted (capture-flip) form
//! — a held Lance drops as an `L` or a Silver-moving `+L`, and so on. This all
//! matches FSF node-for-node.
//!
//! As in Shogi (#190), FSF's `micro` perft does **not** enforce *uchifuzume* (the
//! no-pawn-drop-mate rule): a mating pawn drop is a legal move. mcr matches FSF and
//! leaves [`WideVariant::pawn_drop_mate_forbidden`] `false`.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant micro`, `position startpos`) renders the start as
//!
//! ```text
//! kb+r+l/p3/4/3P/+L+RBK[] w - - 0 1
//! ```
//!
//! mcr uses the same board placement and an empty `[]` holdings bracket. The
//! FSF-confirmed startpos perft sequence is `9, 80, 767, 7256, 71328`.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{attacks, Bitboard, Board, PromotionConfig, Square, WideRole, WideVariant};
use crate::Color;

use super::super::Micro4x5;

/// The Micro Shogi rule layer: a zero-sized [`WideVariant`] over [`Micro4x5`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MicroRules;

/// The confirmed Micro Shogi starting placement (the hand is empty at the start).
const MICRO_PLACEMENT: &str = "kb+r+l/p3/4/3P/+L+RBK";

/// The four diagonal one-step (ferz) offsets.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

impl MicroRules {
    /// The Gold General's attack set from `sq` for `color`: one step orthogonally
    /// (four directions) plus one step diagonally **forward** (two directions) —
    /// six squares. The promoted Rook (+R) and promoted Bishop (+B) move this way.
    fn gold_attacks(color: Color, sq: Square<Micro4x5>) -> Bitboard<Micro4x5> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        let offsets = [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            // The two forward diagonals.
            (1, fwd),
            (-1, fwd),
        ];
        attacks::leaper_attacks::<Micro4x5>(sq, &offsets)
    }

    /// The Silver General's attack set: the four diagonals plus one straight
    /// forward step (five squares). The promoted Lance (+L) moves this way.
    fn silver_attacks(color: Color, sq: Square<Micro4x5>) -> Bitboard<Micro4x5> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = attacks::leaper_attacks::<Micro4x5>(sq, &FERZ_OFFSETS);
        if let Some(dest) = sq.offset(0, fwd) {
            bb.set(dest);
        }
        bb
    }

    /// The Shogi Knight's attack set: the two forward 2-1 jumps only (it never
    /// moves sideways or backward), leaping over any piece. The promoted Pawn (+P)
    /// moves this way.
    fn knight_attacks(color: Color, sq: Square<Micro4x5>) -> Bitboard<Micro4x5> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        attacks::leaper_attacks::<Micro4x5>(sq, &[(1, 2 * fwd), (-1, 2 * fwd)])
    }

    /// The Pawn's attack/movement square: the single square straight forward (it
    /// both moves and captures there).
    fn pawn_attacks(color: Color, sq: Square<Micro4x5>) -> Bitboard<Micro4x5> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = Bitboard::<Micro4x5>::EMPTY;
        if let Some(dest) = sq.offset(0, fwd) {
            bb.set(dest);
        }
        bb
    }
}

impl WideVariant<Micro4x5> for MicroRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 29;

    fn starting_position() -> (Board<Micro4x5>, GenericState<Micro4x5>) {
        let board = Board::<Micro4x5>::from_fen_placement(MICRO_PLACEMENT)
            .expect("the Micro Shogi starting placement is valid on a 4x5 board");
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
        };
        (board, state)
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Micro4x5>,
        occupancy: Bitboard<Micro4x5>,
    ) -> Bitboard<Micro4x5> {
        match role {
            WideRole::Pawn => Self::pawn_attacks(color, sq),
            // The promoted Pawn (+P) moves as a forward Shogi Knight.
            WideRole::Tokin => Self::knight_attacks(color, sq),
            // The promoted Lance (+L) moves as a Silver General.
            WideRole::PromotedLance => Self::silver_attacks(color, sq),
            // Both promoted sliders (+R, +B) move as a Gold General.
            WideRole::Dragon | WideRole::DragonHorse => Self::gold_attacks(color, sq),
            WideRole::Lance => attacks::lance_attacks::<Micro4x5>(color, sq, occupancy),
            WideRole::Bishop => attacks::bishop_attacks::<Micro4x5>(sq, occupancy),
            WideRole::Rook => attacks::rook_attacks::<Micro4x5>(sq, occupancy),
            WideRole::King => attacks::king_attacks::<Micro4x5>(sq),
            _ => Bitboard::EMPTY,
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // The forward-biased pieces: the Pawn (forward capture), the Lance, and the
        // three promoted forms — +P (forward Knight), +L (Silver), and +R / +B
        // (Gold). Their attack sets point forward, so the attacker scan must project
        // the opposite color from the target. The Rook, Bishop, and King are not
        // directional.
        matches!(
            role,
            WideRole::Pawn
                | WideRole::Lance
                | WideRole::Tokin
                | WideRole::PromotedLance
                | WideRole::Dragon
                | WideRole::DragonHorse
        )
    }

    fn role_is_slider(role: WideRole) -> bool {
        // The Rook and Bishop slide and so can pin / be pinned along a ray, as does
        // the Lance on its forward file. Every stepper — including Micro Shogi's
        // Gold-moving +R / +B (which, unlike the Shogi Dragon / Horse, do **not**
        // slide), the Silver-moving +L, the Knight-moving +P, the Pawn, and the King
        // — does not.
        matches!(role, WideRole::Rook | WideRole::Bishop | WideRole::Lance)
    }

    fn promotion_config() -> PromotionConfig {
        // Micro Shogi has no promotion *zone*: its form change is the capture flip
        // ([`flips_on_capture`]), not the generic per-piece zone-promotion path. This
        // static set is unused, but the trait requires it.
        PromotionConfig {
            roles: alloc::vec![WideRole::Gold],
        }
    }

    fn in_promotion_zone(_color: Color, _rank: u8) -> bool {
        // No promotion zone — the form change is the capture flip.
        false
    }

    fn has_castling() -> bool {
        false
    }

    // --- hand / drops + capture flip --------------------------------------

    fn has_hand() -> bool {
        true
    }

    fn drops_can_promote() -> bool {
        // Micro Shogi has dual-form drops (FSF `dropPromoted`): a held base piece
        // may be deployed in either its base or its promoted (capture-flip) form,
        // onto any empty square (no dead-piece rule — a `+P` may sit on the last
        // rank). The generic drop generator finds the alternate form via
        // [`flips_on_capture`].
        true
    }

    fn flips_on_capture(role: WideRole) -> Option<WideRole> {
        // The four Micro Shogi flip pairs. A base piece promotes on a capture, a
        // promoted piece demotes; the King has no alternate form. A quiet move never
        // flips (the generic move path only consults this on a capture).
        match role {
            WideRole::Pawn => Some(WideRole::Tokin),
            WideRole::Tokin => Some(WideRole::Pawn),
            WideRole::Lance => Some(WideRole::PromotedLance),
            WideRole::PromotedLance => Some(WideRole::Lance),
            WideRole::Rook => Some(WideRole::Dragon),
            WideRole::Dragon => Some(WideRole::Rook),
            WideRole::Bishop => Some(WideRole::DragonHorse),
            WideRole::DragonHorse => Some(WideRole::Bishop),
            _ => None,
        }
    }

    // --- Sennichite / perpetual check (terminal only; perft unaffected) ----

    fn tracks_repetition() -> bool {
        true
    }

    fn repetition_fold() -> usize {
        4
    }

    fn repetition_draw_reason() -> crate::geometry::WideEndReason {
        crate::geometry::WideEndReason::Sennichite
    }

    fn perpetual_check_loses() -> bool {
        true
    }
}

/// Micro Shogi (4x5 Shogi) as a [`GenericPosition`] over the 4x5 geometry.
///
/// Construct the starting position with
/// [`Micro::startpos`](GenericPosition::startpos) or parse a FEN — the placement
/// may carry the hand as a `[..]` holdings bracket — with
/// [`Micro::from_fen`](GenericPosition::from_fen). See the [module docs](self) for
/// the capture-flip promotion, the hand, and drops.
pub type Micro = GenericPosition<Micro4x5, MicroRules>;
