//! Judkins Shogi (6x6 Shogi) on the generic engine — a compact reuse of the
//! Shogi (#190) **persistent hand**, **drops**, and **promotion-zone** machinery
//! on a new six-by-six (36-square) [`Judkins6x6`] geometry. Validated against
//! Fairy-Stockfish `UCI_Variant judkins`.
//!
//! Judkins Shogi (ジャドケンス将棋) is Shogi shrunk onto a 6x6 board with **one of
//! every piece** per side, keeping the Shogi **Knight** but dropping the Lance.
//! A captured piece flips side and enters the captor's **hand**, from which it may
//! later be **dropped** back as the captor's own piece; pieces **promote** on
//! reaching the far two ranks. All of that is exactly the Shogi rule layer
//! (`shogi.rs`); this module reuses the same default-off [`WideVariant`] hooks
//! (`has_hand`, the `WideMove` drop kind, the per-piece promotion hooks,
//! `role_attack_is_directional`) with a smaller board and a two-rank promotion
//! zone.
//!
//! ## Pieces (confirmed against FSF; promoted forms in parentheses)
//!
//! One each per side, keeping the Knight but with no Lance:
//!
//! * **King (K, 玉)** — a standard king.
//! * **Rook (R, 飛 → Dragon 龍, `+R`)** — a rook; promotes to a **Dragon King**
//!   (rook slides plus one diagonal step in each direction).
//! * **Bishop (B, 角 → Horse 馬, `+B`)** — a bishop; promotes to a **Dragon
//!   Horse** (bishop slides plus one orthogonal step in each direction).
//! * **Gold General (G, 金)** — one step orthogonally or one step diagonally
//!   **forward** (six directions). Never promotes.
//! * **Silver General (S, 銀 → +S, moves as Gold)** — the four diagonals or one
//!   step straight **forward** (five directions).
//! * **Knight (N, 桂 → +N, moves as Gold)** — jumps two squares forward and one to
//!   the side, **forward only** (two targets), leaping over any piece.
//! * **Pawn (P, 歩 → Tokin と, `+P`, moves as Gold)** — one step straight
//!   forward, and **captures straight forward** too (like Shogi, unlike chess).
//!
//! ## Promotion zone
//!
//! On 6x6 the promotion zone is the **furthest two ranks** (ranks 5-6 / 0-based
//! 4-5 for White, ranks 1-2 / 0-based 0-1 for Black) — confirmed against FSF. A
//! move that starts or ends in the zone *may* promote (optional), except promotion
//! is **forced** when the piece would otherwise have no further move: a Pawn
//! reaching the last rank, or a Knight reaching the last two ranks. The Gold
//! General and King never promote; a dropped or already-promoted piece does not
//! promote again.
//!
//! ## Hand and drops
//!
//! A captured piece is banked **unpromoted** (a captured Dragon enters the hand
//! as a Rook) and flipped to the captor's side. On a turn a side may, instead of
//! a board move, **drop** a held piece onto any empty square, subject to:
//!
//! 1. **No dead piece** — a Pawn may not be dropped on the last rank, nor a Knight
//!    on the last two ranks (it would have no move).
//! 2. **Nifu** — no two *unpromoted* Pawns of the same side on one file (a Tokin
//!    does not count).
//! 3. A dropped piece is always unpromoted.
//!
//! As in Shogi (#190), **FSF's `judkins` perft does not enforce *uchifuzume***
//! (the no-pawn-drop-mate rule): a mating pawn drop is listed as a legal move.
//! Since this variant is validated node-for-node against FSF, mcr matches FSF and
//! does **not** apply the uchifuzume filter (`pawn_drop_mate_forbidden` stays
//! `false`).
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant judkins`, `position startpos`) renders the start as
//!
//! ```text
//! rbnsgk/5p/6/6/P5/KGSNBR[-] w - - 0 1
//! ```
//!
//! mcr uses the same board placement and an empty `[]` holdings bracket; the
//! `compare-fairy/` harness reconciles the empty-hand rendering when driving FSF.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Geometry, PromotionConfig, Square, WideRole, WideVariant,
};
use crate::Color;

use super::super::Judkins6x6;

/// The Judkins Shogi rule layer: a zero-sized [`WideVariant`] over [`Judkins6x6`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct JudkinsRules;

/// The confirmed Judkins Shogi starting placement (the hand is empty at the start).
const JUDKINS_PLACEMENT: &str = "rbnsgk/5p/6/6/P5/KGSNBR";

/// The four diagonal one-step (ferz) offsets.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// The four orthogonal one-step (wazir) offsets.
const WAZIR_OFFSETS: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// The depth of the promotion zone: the furthest two ranks from each side.
const ZONE_DEPTH: u8 = 2;

impl JudkinsRules {
    /// The Gold General's attack set from `sq` for `color`: one step orthogonally
    /// (four directions) plus one step diagonally **forward** (two directions) —
    /// six squares. The promoted minors (+P, +N, +S) move identically.
    fn gold_attacks(color: Color, sq: Square<Judkins6x6>) -> Bitboard<Judkins6x6> {
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
        attacks::leaper_attacks::<Judkins6x6>(sq, &offsets)
    }

    /// The Silver General's attack set: the four diagonals plus one straight
    /// forward step (five squares).
    fn silver_attacks(color: Color, sq: Square<Judkins6x6>) -> Bitboard<Judkins6x6> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = attacks::leaper_attacks::<Judkins6x6>(sq, &FERZ_OFFSETS);
        if let Some(dest) = sq.offset(0, fwd) {
            bb.set(dest);
        }
        bb
    }

    /// The Shogi Knight's attack set: the two forward 2-1 jumps only (it never
    /// moves sideways or backward), leaping over any piece.
    fn knight_attacks(color: Color, sq: Square<Judkins6x6>) -> Bitboard<Judkins6x6> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        attacks::leaper_attacks::<Judkins6x6>(sq, &[(1, 2 * fwd), (-1, 2 * fwd)])
    }

    /// The Pawn's attack/movement square: the single square straight forward (it
    /// both moves and captures there).
    fn pawn_attacks(color: Color, sq: Square<Judkins6x6>) -> Bitboard<Judkins6x6> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = Bitboard::<Judkins6x6>::EMPTY;
        if let Some(dest) = sq.offset(0, fwd) {
            bb.set(dest);
        }
        bb
    }

    /// The last rank for `color` (rank 5 white / rank 0 black) — a Pawn there has
    /// no further move (forced promotion / no drop).
    fn last_rank(color: Color) -> u8 {
        match color {
            Color::White => Judkins6x6::HEIGHT - 1,
            Color::Black => 0,
        }
    }

    /// `true` if `rank` is in the last two ranks for `color` — a Knight there has
    /// no further move (forced promotion / no drop).
    fn in_last_two(color: Color, rank: u8) -> bool {
        match color {
            Color::White => rank >= Judkins6x6::HEIGHT - 2,
            Color::Black => rank <= 1,
        }
    }
}

impl WideVariant<Judkins6x6> for JudkinsRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 29;

    fn starting_position() -> (Board<Judkins6x6>, GenericState<Judkins6x6>) {
        let board = Board::<Judkins6x6>::from_fen_placement(JUDKINS_PLACEMENT)
            .expect("the Judkins Shogi starting placement is valid on a 6x6 board");
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
        sq: Square<Judkins6x6>,
        occupancy: Bitboard<Judkins6x6>,
    ) -> Bitboard<Judkins6x6> {
        match role {
            WideRole::Pawn => Self::pawn_attacks(color, sq),
            WideRole::Knight => Self::knight_attacks(color, sq),
            WideRole::Silver => Self::silver_attacks(color, sq),
            // Gold and every promoted minor move as a Gold General.
            WideRole::Gold
            | WideRole::Tokin
            | WideRole::PromotedKnight
            | WideRole::PromotedSilver => Self::gold_attacks(color, sq),
            WideRole::Bishop => attacks::bishop_attacks::<Judkins6x6>(sq, occupancy),
            WideRole::Rook => attacks::rook_attacks::<Judkins6x6>(sq, occupancy),
            WideRole::King => attacks::king_attacks::<Judkins6x6>(sq),
            // Dragon (+R): rook plus one diagonal step in each direction.
            WideRole::Dragon => {
                attacks::rook_attacks::<Judkins6x6>(sq, occupancy)
                    | attacks::leaper_attacks::<Judkins6x6>(sq, &FERZ_OFFSETS)
            }
            // Dragon Horse (+B): bishop plus one orthogonal step in each direction.
            WideRole::DragonHorse => {
                attacks::bishop_attacks::<Judkins6x6>(sq, occupancy)
                    | attacks::leaper_attacks::<Judkins6x6>(sq, &WAZIR_OFFSETS)
            }
            _ => Bitboard::EMPTY,
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // The forward-biased pieces: the Pawn (forward capture), the Gold and
        // Silver Generals, the Knight, and the Gold-moving promoted minors. Their
        // attack sets point forward, so the attacker scan must project the opposite
        // color from the target. The Rook, Bishop, King, and their (color-symmetric)
        // promoted forms are not directional.
        matches!(
            role,
            WideRole::Pawn
                | WideRole::Silver
                | WideRole::Gold
                | WideRole::Knight
                | WideRole::Tokin
                | WideRole::PromotedKnight
                | WideRole::PromotedSilver
        )
    }

    fn role_is_slider(role: WideRole) -> bool {
        // The Rook, Bishop, and their promoted forms slide and so can pin / be
        // pinned along a ray. Every stepper (Gold family, Silver, Knight, Pawn,
        // King) does not.
        matches!(
            role,
            WideRole::Rook | WideRole::Bishop | WideRole::Dragon | WideRole::DragonHorse
        )
    }

    fn promotion_config() -> PromotionConfig {
        // Judkins's promotions are per-piece (each base role has exactly one
        // promoted form, handled by the generic per-piece promotion path); this
        // static set is unused, but the trait requires it.
        PromotionConfig {
            roles: alloc::vec![WideRole::Gold],
        }
    }

    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        match color {
            Color::White => rank >= Judkins6x6::HEIGHT - ZONE_DEPTH,
            Color::Black => rank < ZONE_DEPTH,
        }
    }

    fn has_castling() -> bool {
        false
    }

    // --- hand / drops + per-piece promotion -------------------------------

    fn has_hand() -> bool {
        true
    }

    fn role_can_promote(role: WideRole) -> bool {
        // The promotable base pieces; Gold and King never promote, and a piece
        // already promoted does not promote again. No Lance here.
        matches!(
            role,
            WideRole::Pawn
                | WideRole::Knight
                | WideRole::Silver
                | WideRole::Rook
                | WideRole::Bishop
        )
    }

    fn role_promotion_forced(role: WideRole, color: Color, to_rank: u8) -> bool {
        match role {
            // A Pawn on the last rank has no further move.
            WideRole::Pawn => to_rank == Self::last_rank(color),
            // A Knight on the last two ranks has no further move.
            WideRole::Knight => Self::in_last_two(color, to_rank),
            _ => false,
        }
    }

    fn drop_targets<const R: usize>(
        role: WideRole,
        color: Color,
        board: &Board<Judkins6x6, R>,
    ) -> Bitboard<Judkins6x6> {
        let mut mask = !board.occupied();
        // Dead-piece rule: a dropped Pawn may not land on the last rank, nor a
        // Knight on the last two ranks (it would then have no move).
        match role {
            WideRole::Pawn => {
                mask &= !Self::rank_mask(Self::last_rank(color));
            }
            WideRole::Knight => {
                mask &= !Self::last_two_mask(color);
            }
            _ => {}
        }
        // Nifu: a Pawn may not be dropped onto a file that already holds an
        // unpromoted friendly Pawn (a Tokin does not count).
        if role == WideRole::Pawn {
            let own_pawns = board.pieces(color, WideRole::Pawn);
            for pawn in own_pawns {
                mask &= !Self::file_mask(pawn.file());
            }
        }
        mask
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

    fn stalemate_is_loss() -> bool {
        // Stalemate is a loss for the stalemated side (FSF `stalemateValue =
        // -VALUE_MATE`); adjudication only, so perft is byte-identical.
        true
    }
}

impl JudkinsRules {
    /// The mask of every square on `rank`.
    fn rank_mask(rank: u8) -> Bitboard<Judkins6x6> {
        let mut bb = Bitboard::<Judkins6x6>::EMPTY;
        for file in 0..Judkins6x6::WIDTH {
            if let Some(sq) = Square::<Judkins6x6>::from_file_rank(file, rank) {
                bb.set(sq);
            }
        }
        bb
    }

    /// The mask of every square on `file`.
    fn file_mask(file: u8) -> Bitboard<Judkins6x6> {
        let mut bb = Bitboard::<Judkins6x6>::EMPTY;
        for rank in 0..Judkins6x6::HEIGHT {
            if let Some(sq) = Square::<Judkins6x6>::from_file_rank(file, rank) {
                bb.set(sq);
            }
        }
        bb
    }

    /// The mask of the last two ranks for `color` (where a Knight has no move).
    fn last_two_mask(color: Color) -> Bitboard<Judkins6x6> {
        let (a, b) = match color {
            Color::White => (Judkins6x6::HEIGHT - 1, Judkins6x6::HEIGHT - 2),
            Color::Black => (0, 1),
        };
        Self::rank_mask(a) | Self::rank_mask(b)
    }
}

/// Judkins Shogi (6x6 Shogi) as a [`GenericPosition`] over the 6x6 geometry.
///
/// Construct the starting position with
/// [`Judkins::startpos`](GenericPosition::startpos) or parse a FEN — the placement
/// may carry the hand as a `[..]` holdings bracket — with
/// [`Judkins::from_fen`](GenericPosition::from_fen). See the [module docs](self)
/// for the hand, drops, and two-rank promotion zone.
pub type Judkins = GenericPosition<
    Judkins6x6,
    JudkinsRules,
    { <JudkinsRules as WideVariant<Judkins6x6>>::ROLE_SPAN },
>;
