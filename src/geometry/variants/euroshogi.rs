//! EuroShogi (European Shogi, 8x8) on the generic engine — a compact reuse of the
//! Shogi (#190) **persistent hand**, **drops**, and **promotion-zone** machinery
//! on the standard [`Chess8x8`] geometry, with a reduced army, a **modified
//! Knight**, and **mandatory** in-zone promotion. Validated against
//! Fairy-Stockfish `UCI_Variant euroshogi`.
//!
//! EuroShogi is a Western adaptation of Shogi onto an 8x8 board with **one of
//! most pieces** per side, **no Silver General and no Lance**, and a Knight whose
//! move is enlarged so it is never stranded. A captured piece flips side and
//! enters the captor's **hand**, from which it may later be **dropped** back as
//! the captor's own piece; a move touching the far three ranks **promotes** — and
//! here promotion is **compulsory**, unlike standard Shogi's optional promotion.
//! All of that is the Shogi rule layer (`shogi.rs`) with a smaller board, so this
//! module reuses the same default-off [`WideVariant`] hooks (`has_hand`, the
//! `WideMove` drop kind, the per-piece promotion hooks) plus
//! [`promotion_mandatory_in_zone`](WideVariant::promotion_mandatory_in_zone).
//!
//! ## Pieces (confirmed against FSF; promoted forms in parentheses)
//!
//! No Silver and no Lance; one King and one Gold either side, and a modified
//! Knight:
//!
//! * **King (K, 玉)** — a standard king.
//! * **Rook (R, 飛 → Dragon 龍, `+R`)** — a rook; promotes to a **Dragon King**
//!   (rook slides plus one diagonal step in each direction).
//! * **Bishop (B, 角 → Horse 馬, `+B`)** — a bishop; promotes to a **Dragon
//!   Horse** (bishop slides plus one orthogonal step in each direction).
//! * **Gold General (G, 金)** — one step orthogonally or one step diagonally
//!   **forward** (six directions). Never promotes.
//! * **Knight (N, 桂 → +N, moves as Gold)** — the **modified EuroShogi Knight**:
//!   the two Shogi forward 2-1 jumps (leaping over any piece) **plus one step
//!   straight sideways** (east and west). The sideways steps mean it is never
//!   immobile, so — unlike a Shogi Knight — it may be dropped anywhere and is
//!   never *forced* to promote by immobility (though the mandatory-zone rule below
//!   still promotes it on any move touching the zone).
//! * **Pawn (P, 歩 → Tokin と, `+P`, moves as Gold)** — one step straight
//!   forward, and **captures straight forward** too (like Shogi, unlike chess).
//!
//! ## Promotion zone (mandatory)
//!
//! The promotion zone is the **furthest three ranks** from each side: ranks 6-8
//! (0-based 5-7) for White, ranks 1-3 (0-based 0-2) for Black — the same
//! three-rank depth as 9x9 Shogi. A move that **starts or ends** in the zone with
//! a promotable piece **must** promote: EuroShogi promotion is **compulsory**
//! (FSF `mandatoryPiecePromotion`), so the generator never emits a non-promoting
//! zone move (confirmed against FSF: a Rook that merely *starts* in the zone
//! promotes on every move, and a Pawn or Knight entering it has no non-promoting
//! alternative). The Gold General and King never promote; a dropped or
//! already-promoted piece does not promote again.
//!
//! ## Hand and drops
//!
//! A captured piece is banked **unpromoted** (a captured Dragon enters the hand
//! as a Rook) and flipped to the captor's side. On a turn a side may, instead of
//! a board move, **drop** a held piece onto any empty square, subject to:
//!
//! 1. **No dead Pawn** — a Pawn may not be dropped on the last rank (it would have
//!    no move). The modified Knight has a sideways move even on the last rank, so
//!    — unlike Shogi — it carries **no** drop restriction (confirmed against FSF).
//! 2. **Nifu** — no two *unpromoted* Pawns of the same side on one file (a Tokin
//!    does not count).
//! 3. A dropped piece is always unpromoted.
//!
//! As with Shogi (#190), **FSF's `euroshogi` perft does not enforce *uchifuzume***
//! (the no-pawn-drop-mate rule): a mating pawn drop is listed as a legal move.
//! Since this variant is validated node-for-node against FSF, mcr matches FSF and
//! does **not** apply the uchifuzume filter (`pawn_drop_mate_forbidden` stays
//! `false`).
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant euroshogi`, `position startpos`) renders the start as
//!
//! ```text
//! 1nbgkgn1/1r4b1/pppppppp/8/8/PPPPPPPP/1B4R1/1NGKGBN1[] w - - 0 1
//! ```
//!
//! mcr uses the same board placement and an empty `[]` holdings bracket; the piece
//! letters coincide with FSF's, so no FEN dialect rewrite is needed.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Geometry, PromotionConfig, Square, WideRole, WideVariant,
};
use crate::Color;

use super::super::Chess8x8;

/// The EuroShogi rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct EuroShogiRules;

/// The confirmed EuroShogi starting placement (the hand is empty at the start).
const EUROSHOGI_PLACEMENT: &str = "1nbgkgn1/1r4b1/pppppppp/8/8/PPPPPPPP/1B4R1/1NGKGBN1";

/// The four diagonal one-step (ferz) offsets.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// The four orthogonal one-step (wazir) offsets.
const WAZIR_OFFSETS: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// The depth of the promotion zone: the furthest three ranks from each side.
const ZONE_DEPTH: u8 = 3;

impl EuroShogiRules {
    /// The Gold General's attack set from `sq` for `color`: one step orthogonally
    /// (four directions) plus one step diagonally **forward** (two directions) —
    /// six squares. The promoted minors (+P, +N) move identically.
    fn gold_attacks(color: Color, sq: Square<Chess8x8>) -> Bitboard<Chess8x8> {
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
        attacks::leaper_attacks::<Chess8x8>(sq, &offsets)
    }

    /// The modified EuroShogi Knight's attack set: the two forward 2-1 jumps **and
    /// one step straight sideways** (east and west) — four targets. The sideways
    /// steps make it never immobile.
    fn knight_attacks(color: Color, sq: Square<Chess8x8>) -> Bitboard<Chess8x8> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        attacks::leaper_attacks::<Chess8x8>(
            sq,
            &[
                // The two Shogi forward jumps.
                (1, 2 * fwd),
                (-1, 2 * fwd),
                // The two sideways steps (color-independent).
                (1, 0),
                (-1, 0),
            ],
        )
    }

    /// The Shogi Pawn's attack/movement square: the single square straight forward
    /// (it both moves and captures there).
    fn pawn_attacks(color: Color, sq: Square<Chess8x8>) -> Bitboard<Chess8x8> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = Bitboard::<Chess8x8>::EMPTY;
        if let Some(dest) = sq.offset(0, fwd) {
            bb.set(dest);
        }
        bb
    }

    /// The last rank for `color` (rank 7 white / rank 0 black) — a dropped Pawn
    /// there would have no move.
    fn last_rank(color: Color) -> u8 {
        match color {
            Color::White => Chess8x8::HEIGHT - 1,
            Color::Black => 0,
        }
    }
}

impl WideVariant<Chess8x8> for EuroShogiRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 29;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(EUROSHOGI_PLACEMENT)
            .expect("the EuroShogi starting placement is valid on an 8x8 board");
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
        sq: Square<Chess8x8>,
        occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        match role {
            WideRole::Pawn => Self::pawn_attacks(color, sq),
            WideRole::Knight => Self::knight_attacks(color, sq),
            // Gold and every promoted minor move as a Gold General.
            WideRole::Gold | WideRole::Tokin | WideRole::PromotedKnight => {
                Self::gold_attacks(color, sq)
            }
            WideRole::Bishop => attacks::bishop_attacks::<Chess8x8>(sq, occupancy),
            WideRole::Rook => attacks::rook_attacks::<Chess8x8>(sq, occupancy),
            WideRole::King => attacks::king_attacks::<Chess8x8>(sq),
            // Dragon (+R): rook plus one diagonal step in each direction.
            WideRole::Dragon => {
                attacks::rook_attacks::<Chess8x8>(sq, occupancy)
                    | attacks::leaper_attacks::<Chess8x8>(sq, &FERZ_OFFSETS)
            }
            // Dragon Horse (+B): bishop plus one orthogonal step in each direction.
            WideRole::DragonHorse => {
                attacks::bishop_attacks::<Chess8x8>(sq, occupancy)
                    | attacks::leaper_attacks::<Chess8x8>(sq, &WAZIR_OFFSETS)
            }
            _ => Bitboard::EMPTY,
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // The forward-biased pieces: the Pawn (forward capture), the Gold General,
        // the modified Knight (its forward jumps flip under a color change; the
        // sideways steps are symmetric), and the Gold-moving promoted minors. Their
        // attack sets are asymmetric under a color flip, so the attacker scan must
        // project the opposite color from the target. The Rook, Bishop, King, and
        // their (color-symmetric) promoted forms are not directional.
        matches!(
            role,
            WideRole::Pawn
                | WideRole::Gold
                | WideRole::Knight
                | WideRole::Tokin
                | WideRole::PromotedKnight
        )
    }

    fn role_is_slider(role: WideRole) -> bool {
        // The Rook, Bishop, and their promoted forms slide and so can pin / be
        // pinned along a ray. Every stepper (Gold, Knight, Pawn, King) does not.
        matches!(
            role,
            WideRole::Rook | WideRole::Bishop | WideRole::Dragon | WideRole::DragonHorse
        )
    }

    fn promotion_config() -> PromotionConfig {
        // EuroShogi's promotions are per-piece (each base role has exactly one
        // promoted form, handled by the generic per-piece promotion path); this
        // static set is unused, but the trait requires it.
        PromotionConfig {
            roles: alloc::vec![WideRole::Gold],
        }
    }

    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        match color {
            Color::White => rank >= Chess8x8::HEIGHT - ZONE_DEPTH,
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
        // already promoted does not promote again. No Silver or Lance here.
        matches!(
            role,
            WideRole::Pawn | WideRole::Knight | WideRole::Rook | WideRole::Bishop
        )
    }

    fn promotion_mandatory_in_zone() -> bool {
        // EuroShogi promotion is compulsory: any move that starts or ends in the
        // promotion zone with a promotable piece is always the promoting form (FSF
        // `mandatoryPiecePromotion`).
        true
    }

    fn drop_targets(role: WideRole, color: Color, board: &Board<Chess8x8>) -> Bitboard<Chess8x8> {
        let mut mask = !board.occupied();
        // Dead-piece rule: a dropped Pawn may not land on the last rank (it would
        // then have no move). The modified Knight always has a sideways move, so it
        // carries no drop restriction.
        if role == WideRole::Pawn {
            mask &= !Self::rank_mask(Self::last_rank(color));
            // Nifu: a Pawn may not be dropped onto a file that already holds an
            // unpromoted friendly Pawn (a Tokin does not count).
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
}

impl EuroShogiRules {
    /// The mask of every square on `rank`.
    fn rank_mask(rank: u8) -> Bitboard<Chess8x8> {
        let mut bb = Bitboard::<Chess8x8>::EMPTY;
        for file in 0..Chess8x8::WIDTH {
            if let Some(sq) = Square::<Chess8x8>::from_file_rank(file, rank) {
                bb.set(sq);
            }
        }
        bb
    }

    /// The mask of every square on `file`.
    fn file_mask(file: u8) -> Bitboard<Chess8x8> {
        let mut bb = Bitboard::<Chess8x8>::EMPTY;
        for rank in 0..Chess8x8::HEIGHT {
            if let Some(sq) = Square::<Chess8x8>::from_file_rank(file, rank) {
                bb.set(sq);
            }
        }
        bb
    }
}

/// EuroShogi (European Shogi, 8x8) as a [`GenericPosition`] over the 8x8 geometry.
///
/// Construct the starting position with
/// [`EuroShogi::startpos`](GenericPosition::startpos) or parse a FEN — the
/// placement may carry the hand as a `[..]` holdings bracket — with
/// [`EuroShogi::from_fen`](GenericPosition::from_fen). See the [module docs](self)
/// for the modified Knight, the hand, drops, and the mandatory promotion zone.
pub type EuroShogi = GenericPosition<Chess8x8, EuroShogiRules>;
