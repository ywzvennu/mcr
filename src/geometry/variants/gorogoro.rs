//! Gorogoro Shogi Plus (5x6 Shogi) on the generic engine — a compact reuse of
//! the Shogi (#190) **persistent hand**, **drops**, and **promotion-zone**
//! machinery on a new five-by-six (30-square) [`Gorogoro5x6`] geometry.
//! Validated against Fairy-Stockfish `UCI_Variant gorogoroplus`.
//!
//! Gorogoro Shogi (ごろごろ将棋) is Shogi shrunk onto a 5x6 board with a King,
//! two Gold and two Silver Generals, and a row of Pawns per side; the **Plus**
//! version additionally starts each side with a **Lance and a Shogi Knight in
//! hand**, ready to drop. A captured piece flips side and enters the captor's
//! **hand**, from which it may later be **dropped** back as the captor's own
//! piece; pieces **promote** on reaching the far zone. All of that is exactly the
//! Shogi rule layer (`shogi.rs`); this module reuses the same default-off
//! [`WideVariant`] hooks (`has_hand`, the `WideMove` drop kind, the per-piece
//! promotion hooks, `role_attack_is_directional`) on a smaller board with a
//! two-rank promotion zone.
//!
//! ## Pieces (confirmed against FSF; promoted forms in parentheses)
//!
//! There is **no Rook and no Bishop**; the remaining Shogi pieces appear:
//!
//! * **King (K, 玉)** — a standard king.
//! * **Gold General (G, 金)** — one step orthogonally or one step diagonally
//!   **forward** (six directions). Never promotes.
//! * **Silver General (S, 銀 → +S, moves as Gold)** — the four diagonals or one
//!   step straight **forward** (five directions).
//! * **Knight (N, 桂 → +N, moves as Gold)** — jumps two squares forward and one
//!   to the side, **forward only** (two targets), leaping over any piece.
//! * **Lance (L, 香 → +L, moves as Gold)** — slides any number of squares
//!   straight **forward** only.
//! * **Pawn (P, 歩 → Tokin と, `+P`, moves as Gold)** — one step straight
//!   forward, and **captures straight forward** too (like Shogi, unlike chess).
//!
//! ## Promotion zone
//!
//! On 5x6 the promotion zone is the **furthest two ranks**: ranks 5-6 (0-based
//! 4-5) for White, ranks 1-2 (0-based 0-1) for Black — confirmed against FSF (a
//! Lance entering rank 5 gets both the promoting and non-promoting move, while on
//! the last rank it is forced to promote). A move that **starts or ends** in the
//! zone *may* promote (optional) — except promotion is **forced** when the piece
//! would otherwise have no further move: a Pawn or Lance reaching the last rank,
//! or a Knight reaching the last two ranks. The Gold General and King never
//! promote; a dropped or already-promoted piece does not promote again.
//!
//! ## Hand and drops
//!
//! Each side starts with a **Lance and a Knight in hand**. A captured piece is
//! banked **unpromoted** and flipped to the captor's side. On a turn a side may,
//! instead of a board move, **drop** a held piece onto any empty square, subject
//! to:
//!
//! 1. **No dead piece** — a Pawn/Lance may not be dropped on the last rank, nor a
//!    Knight on the last two ranks (it would then have no move).
//! 2. **Nifu** — no two *unpromoted* Pawns of the same side on one file (a Tokin
//!    does not count).
//! 3. A dropped piece is always unpromoted.
//!
//! As in Shogi (#190), **FSF's `gorogoroplus` perft does not enforce
//! *uchifuzume*** (the no-pawn-drop-mate rule): a mating pawn drop is listed as a
//! legal move. Since this variant is validated node-for-node against FSF, mcr
//! matches FSF and does **not** apply the uchifuzume filter
//! (`pawn_drop_mate_forbidden` stays `false`).
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant gorogoroplus`) renders the start as
//!
//! ```text
//! sgkgs/5/1ppp1/1PPP1/5/SGKGS[LNln] w 0 1
//! ```
//!
//! mcr uses the same board placement and the same `[LNln]` holdings bracket — a
//! Lance and a Knight in each side's hand. The `compare-fairy/` harness
//! reconciles the rendering when driving FSF.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Geometry, PromotionConfig, Square, WideRole, WideVariant,
};
use crate::Color;

use super::super::Gorogoro5x6;

/// The Gorogoro Shogi Plus rule layer: a zero-sized [`WideVariant`] over
/// [`Gorogoro5x6`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GorogoroRules;

/// The confirmed Gorogoro Shogi Plus starting placement (the Lance/Knight pair
/// in hand is set separately in [`GorogoroRules::starting_position`]).
const GOROGORO_PLACEMENT: &str = "sgkgs/5/1ppp1/1PPP1/5/SGKGS";

/// The four diagonal one-step (ferz) offsets.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// The depth of the promotion zone: the furthest two ranks from each side.
const ZONE_DEPTH: u8 = 2;

impl GorogoroRules {
    /// The Gold General's attack set from `sq` for `color`: one step orthogonally
    /// (four directions) plus one step diagonally **forward** (two directions) —
    /// six squares total. Every promoted minor (+P, +L, +N, +S) moves identically.
    fn gold_attacks(color: Color, sq: Square<Gorogoro5x6>) -> Bitboard<Gorogoro5x6> {
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
        attacks::leaper_attacks::<Gorogoro5x6>(sq, &offsets)
    }

    /// The Silver General's attack set: the four diagonals plus one straight
    /// forward step (five squares).
    fn silver_attacks(color: Color, sq: Square<Gorogoro5x6>) -> Bitboard<Gorogoro5x6> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = attacks::leaper_attacks::<Gorogoro5x6>(sq, &FERZ_OFFSETS);
        if let Some(dest) = sq.offset(0, fwd) {
            bb.set(dest);
        }
        bb
    }

    /// The Shogi Knight's attack set: the two forward 2-1 jumps only.
    fn knight_attacks(color: Color, sq: Square<Gorogoro5x6>) -> Bitboard<Gorogoro5x6> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        attacks::leaper_attacks::<Gorogoro5x6>(sq, &[(1, 2 * fwd), (-1, 2 * fwd)])
    }

    /// The Shogi Pawn's attack/movement square: the single square straight
    /// forward (it both moves and captures there).
    fn pawn_attacks(color: Color, sq: Square<Gorogoro5x6>) -> Bitboard<Gorogoro5x6> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = Bitboard::<Gorogoro5x6>::EMPTY;
        if let Some(dest) = sq.offset(0, fwd) {
            bb.set(dest);
        }
        bb
    }

    /// The last rank for `color` (rank 5 white / rank 0 black) — a Pawn or Lance
    /// there has no further move (forced promotion / no drop).
    fn last_rank(color: Color) -> u8 {
        match color {
            Color::White => Gorogoro5x6::HEIGHT - 1,
            Color::Black => 0,
        }
    }

    /// `true` if `rank` is in the last two ranks for `color` — a Knight there has
    /// no further move.
    fn in_last_two(color: Color, rank: u8) -> bool {
        match color {
            Color::White => rank >= Gorogoro5x6::HEIGHT - 2,
            Color::Black => rank <= 1,
        }
    }

    /// The starting hand: one Lance and one Shogi Knight per side.
    fn initial_hand() -> GenericPlacement {
        let mut counts = [0u8; WideRole::COUNT];
        counts[WideRole::Lance.index()] = 1;
        counts[WideRole::Knight.index()] = 1;
        GenericPlacement::new(counts, counts)
    }
}

impl WideVariant<Gorogoro5x6> for GorogoroRules {
    fn starting_position() -> (Board<Gorogoro5x6>, GenericState<Gorogoro5x6>) {
        let board = Board::<Gorogoro5x6>::from_fen_placement(GOROGORO_PLACEMENT)
            .expect("the Gorogoro starting placement is valid on a 5x6 board");
        let state = GenericState {
            turn: Color::White,
            castling: GenericCastling::NONE,
            ep_square: None,
            gating: GenericGating::NONE,
            duck: None,
            placement: Self::initial_hand(),
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
        sq: Square<Gorogoro5x6>,
        occupancy: Bitboard<Gorogoro5x6>,
    ) -> Bitboard<Gorogoro5x6> {
        match role {
            WideRole::Pawn => Self::pawn_attacks(color, sq),
            WideRole::Knight => Self::knight_attacks(color, sq),
            WideRole::Silver => Self::silver_attacks(color, sq),
            // Gold and every promoted minor move as a Gold General.
            WideRole::Gold
            | WideRole::Tokin
            | WideRole::PromotedLance
            | WideRole::PromotedKnight
            | WideRole::PromotedSilver => Self::gold_attacks(color, sq),
            WideRole::Lance => attacks::lance_attacks::<Gorogoro5x6>(color, sq, occupancy),
            WideRole::King => attacks::king_attacks::<Gorogoro5x6>(sq),
            _ => Bitboard::EMPTY,
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // Every forward-biased piece: the Pawn (forward capture), the Gold and
        // Silver Generals, the Knight, the Lance, and the Gold-moving promoted
        // minors. Their attack sets point forward, so the attacker scan must
        // project the opposite color from the target. The King is not directional.
        matches!(
            role,
            WideRole::Pawn
                | WideRole::Silver
                | WideRole::Gold
                | WideRole::Knight
                | WideRole::Lance
                | WideRole::Tokin
                | WideRole::PromotedLance
                | WideRole::PromotedKnight
                | WideRole::PromotedSilver
        )
    }

    fn role_is_slider(role: WideRole) -> bool {
        // Only the Lance slides (on its forward file) and so can pin / be pinned
        // along a ray. Every stepper (Gold family, Silver, Knight, Pawn, King)
        // does not. There is no Rook or Bishop here.
        matches!(role, WideRole::Lance)
    }

    fn promotion_config() -> PromotionConfig {
        // Gorogoro's promotions are per-piece (each base role has exactly one
        // promoted form, handled by the generic per-piece promotion path); this
        // static set is unused, but the trait requires it.
        PromotionConfig {
            roles: alloc::vec![WideRole::Gold],
        }
    }

    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        match color {
            Color::White => rank >= Gorogoro5x6::HEIGHT - ZONE_DEPTH,
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
        // already promoted does not promote again. No Rook or Bishop here.
        matches!(
            role,
            WideRole::Pawn | WideRole::Lance | WideRole::Knight | WideRole::Silver
        )
    }

    fn role_promotion_forced(role: WideRole, color: Color, to_rank: u8) -> bool {
        match role {
            // A Pawn or Lance on the last rank has no further move.
            WideRole::Pawn | WideRole::Lance => to_rank == Self::last_rank(color),
            // A Knight on the last two ranks has no further move.
            WideRole::Knight => Self::in_last_two(color, to_rank),
            _ => false,
        }
    }

    fn drop_targets(
        role: WideRole,
        color: Color,
        board: &Board<Gorogoro5x6>,
    ) -> Bitboard<Gorogoro5x6> {
        let mut mask = !board.occupied();
        // Dead-piece rule: a dropped Pawn/Lance may not land on the last rank, nor
        // a Knight on the last two ranks (it would then have no move).
        match role {
            WideRole::Pawn | WideRole::Lance => {
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
}

impl GorogoroRules {
    /// The mask of every square on `rank`.
    fn rank_mask(rank: u8) -> Bitboard<Gorogoro5x6> {
        let mut bb = Bitboard::<Gorogoro5x6>::EMPTY;
        for file in 0..Gorogoro5x6::WIDTH {
            if let Some(sq) = Square::<Gorogoro5x6>::from_file_rank(file, rank) {
                bb.set(sq);
            }
        }
        bb
    }

    /// The mask of every square on `file`.
    fn file_mask(file: u8) -> Bitboard<Gorogoro5x6> {
        let mut bb = Bitboard::<Gorogoro5x6>::EMPTY;
        for rank in 0..Gorogoro5x6::HEIGHT {
            if let Some(sq) = Square::<Gorogoro5x6>::from_file_rank(file, rank) {
                bb.set(sq);
            }
        }
        bb
    }

    /// The mask of the last two ranks for `color` (where a Knight has no move).
    fn last_two_mask(color: Color) -> Bitboard<Gorogoro5x6> {
        let (a, b) = match color {
            Color::White => (Gorogoro5x6::HEIGHT - 1, Gorogoro5x6::HEIGHT - 2),
            Color::Black => (0, 1),
        };
        Self::rank_mask(a) | Self::rank_mask(b)
    }
}

/// Gorogoro Shogi Plus (5x6 Shogi) as a [`GenericPosition`] over the 5x6
/// geometry.
///
/// Construct the starting position with
/// [`Gorogoro::startpos`](GenericPosition::startpos) or parse a FEN — the
/// placement may carry the hand as a `[..]` holdings bracket — with
/// [`Gorogoro::from_fen`](GenericPosition::from_fen). See the [module docs](self)
/// for the hand, drops, and two-rank promotion zone.
pub type Gorogoro = GenericPosition<Gorogoro5x6, GorogoroRules>;
