//! Shogi (Japanese chess, 9x9) on the generic engine — the first variant
//! exercising a **persistent hand** and **drops** on the [`WideVariant`] layer
//! (`docs/fairy-variants-architecture.md` §4.4). Validated against
//! Fairy-Stockfish `UCI_Variant shogi`.
//!
//! Shogi is the most structurally different fairy variant so far: a captured
//! piece flips side and enters the captor's **hand**, from which it may later be
//! **dropped** back onto an empty square as the captor's own piece, and pieces
//! **promote** when they move through a far-board zone. The Sittuyin placement
//! pocket (#179) proved a from-hand drop path exists; Shogi generalises it to a
//! persistent, capture-fed, multi-piece hand with drop legality, on a new
//! 9-by-9 (81-square) `u128` [`Shogi9x9`] geometry.
//!
//! ## Pieces (confirmed against FSF; promoted forms in parentheses)
//!
//! * **King (K, 玉)** — a standard king.
//! * **Rook (R, 飛 → Dragon 龍, `+R`)** — a rook; promotes to a **Dragon King**
//!   (rook slides plus one diagonal step in each direction).
//! * **Bishop (B, 角 → Horse 馬, `+B`)** — a bishop; promotes to a **Dragon
//!   Horse** (bishop slides plus one orthogonal step in each direction).
//! * **Gold General (G, 金)** — moves one step orthogonally or one step
//!   diagonally **forward** (six directions). Never promotes.
//! * **Silver General (S, 銀 → +S, moves as Gold)** — one step to any of the four
//!   diagonals or one step straight **forward** (five directions).
//! * **Knight (N, 桂 → +N, moves as Gold)** — jumps two squares forward and one
//!   to the side, **forward only** (two targets), leaping over any piece.
//! * **Lance (L, 香 → +L, moves as Gold)** — slides any number of squares
//!   straight **forward** only.
//! * **Pawn (P, 歩 → Tokin と, `+P`, moves as Gold)** — one step straight
//!   forward, and **captures straight forward** too (unlike a chess pawn).
//!
//! ## Promotion zone
//!
//! The promotion zone is the **furthest three ranks** from each side: ranks 7-9
//! (0-based 6-8) for White, ranks 1-3 (0-based 0-2) for Black. A move that
//! **starts or ends** in the zone *may* promote (optional) — except promotion is
//! **forced** when the piece would otherwise have no further move: a Pawn or
//! Lance reaching the last rank, or a Knight reaching the last two ranks. The
//! Gold General and King never promote; a dropped or already-promoted piece does
//! not promote (again).
//!
//! ## Hand and drops
//!
//! A captured piece is banked **unpromoted** (a captured Dragon enters the hand
//! as a Rook) and flipped to the captor's side. On a turn a side may, instead of
//! a board move, **drop** a held piece onto any empty square, subject to:
//!
//! 1. **No dead piece** — a Pawn/Lance may not be dropped on the last rank, nor a
//!    Knight on the last two ranks (it would have no move).
//! 2. **Nifu** — no two *unpromoted* Pawns of the same side on one file (a Tokin
//!    does not count).
//! 3. **Uchifuzume** — a Pawn drop may not deliver immediate checkmate.
//! 4. A dropped piece is always unpromoted.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant shogi`, `position startpos`) renders the start as
//!
//! ```text
//! lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL[-] w - - 0 1
//! ```
//!
//! mce uses the same board placement and an empty `[]` holdings bracket (its hand
//! is empty at the start); the `compare-fairy/` harness reconciles the empty-hand
//! rendering when driving FSF.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Geometry, PromotionConfig, Square, WideRole, WideVariant,
};
use crate::Color;

use super::super::Shogi9x9;

/// The Shogi rule layer: a zero-sized [`WideVariant`] over [`Shogi9x9`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ShogiRules;

/// The confirmed Shogi starting placement (the hand is empty at the start).
const SHOGI_PLACEMENT: &str = "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL";

/// The four diagonal one-step (ferz) offsets.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// The four orthogonal one-step (wazir) offsets.
const WAZIR_OFFSETS: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// The depth of the promotion zone: the furthest three ranks from each side.
const ZONE_DEPTH: u8 = 3;

impl ShogiRules {
    /// The Gold General's attack set from `sq` for `color`: one step orthogonally
    /// (four directions) plus one step diagonally **forward** (two directions) —
    /// six squares total. All promoted minors (+P, +L, +N, +S) move identically.
    fn gold_attacks(color: Color, sq: Square<Shogi9x9>) -> Bitboard<Shogi9x9> {
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
        attacks::leaper_attacks::<Shogi9x9>(sq, &offsets)
    }

    /// The Silver General's attack set: the four diagonals plus one straight
    /// forward step (five squares).
    fn silver_attacks(color: Color, sq: Square<Shogi9x9>) -> Bitboard<Shogi9x9> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = attacks::leaper_attacks::<Shogi9x9>(sq, &FERZ_OFFSETS);
        if let Some(dest) = sq.offset(0, fwd) {
            bb.set(dest);
        }
        bb
    }

    /// The Shogi Knight's attack set: the two forward 2-1 jumps only.
    fn knight_attacks(color: Color, sq: Square<Shogi9x9>) -> Bitboard<Shogi9x9> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        attacks::leaper_attacks::<Shogi9x9>(sq, &[(1, 2 * fwd), (-1, 2 * fwd)])
    }

    /// The Shogi Pawn's attack/movement square: the single square straight
    /// forward (it both moves and captures there).
    fn pawn_attacks(color: Color, sq: Square<Shogi9x9>) -> Bitboard<Shogi9x9> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = Bitboard::<Shogi9x9>::EMPTY;
        if let Some(dest) = sq.offset(0, fwd) {
            bb.set(dest);
        }
        bb
    }

    /// The last rank for `color` (rank 8 white / rank 0 black) — a Pawn or Lance
    /// there has no further move (forced promotion / no drop).
    fn last_rank(color: Color) -> u8 {
        match color {
            Color::White => Shogi9x9::HEIGHT - 1,
            Color::Black => 0,
        }
    }

    /// `true` if `rank` is in the last two ranks for `color` — a Knight there has
    /// no further move.
    fn in_last_two(color: Color, rank: u8) -> bool {
        match color {
            Color::White => rank >= Shogi9x9::HEIGHT - 2,
            Color::Black => rank <= 1,
        }
    }
}

impl WideVariant<Shogi9x9> for ShogiRules {
    fn starting_position() -> (Board<Shogi9x9>, GenericState<Shogi9x9>) {
        let board = Board::<Shogi9x9>::from_fen_placement(SHOGI_PLACEMENT)
            .expect("the Shogi starting placement is valid on a 9x9 board");
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
        sq: Square<Shogi9x9>,
        occupancy: Bitboard<Shogi9x9>,
    ) -> Bitboard<Shogi9x9> {
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
            WideRole::Lance => attacks::lance_attacks::<Shogi9x9>(color, sq, occupancy),
            WideRole::Bishop => attacks::bishop_attacks::<Shogi9x9>(sq, occupancy),
            WideRole::Rook => attacks::rook_attacks::<Shogi9x9>(sq, occupancy),
            WideRole::King => attacks::king_attacks::<Shogi9x9>(sq),
            // Dragon (+R): rook plus one diagonal step in each direction.
            WideRole::Dragon => {
                attacks::rook_attacks::<Shogi9x9>(sq, occupancy)
                    | attacks::leaper_attacks::<Shogi9x9>(sq, &FERZ_OFFSETS)
            }
            // Dragon Horse (+B): bishop plus one orthogonal step in each direction.
            WideRole::DragonHorse => {
                attacks::bishop_attacks::<Shogi9x9>(sq, occupancy)
                    | attacks::leaper_attacks::<Shogi9x9>(sq, &WAZIR_OFFSETS)
            }
            _ => Bitboard::EMPTY,
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // Every forward-biased piece: the Pawn (forward capture), the Gold and
        // Silver Generals, the Knight, the Lance, and the Gold-moving promoted
        // minors. Their attack sets point forward, so the attacker scan must
        // project the opposite color from the target. The Rook, Bishop, King, and
        // their (color-symmetric) promoted forms are not directional.
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
        // The Rook, Bishop, and their promoted forms slide and so can pin / be
        // pinned along a ray; the Lance slides on its forward file. Every stepper
        // (Gold family, Silver, Knight, Pawn, King) does not.
        matches!(
            role,
            WideRole::Rook
                | WideRole::Bishop
                | WideRole::Dragon
                | WideRole::DragonHorse
                | WideRole::Lance
        )
    }

    fn promotion_config() -> PromotionConfig {
        // Shogi's promotions are per-piece (each base role has exactly one
        // promoted form, handled by the generic per-piece promotion path); this
        // static set is unused, but the trait requires it.
        PromotionConfig {
            roles: alloc::vec![WideRole::Gold],
        }
    }

    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        match color {
            Color::White => rank >= Shogi9x9::HEIGHT - ZONE_DEPTH,
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
        // already promoted does not promote again.
        matches!(
            role,
            WideRole::Pawn
                | WideRole::Lance
                | WideRole::Knight
                | WideRole::Silver
                | WideRole::Rook
                | WideRole::Bishop
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

    // NOTE on *uchifuzume* (no pawn-drop mate): real Shogi forbids a pawn drop
    // that delivers immediate checkmate, but **Fairy-Stockfish's `shogi` move
    // generation does not enforce it** — its `go perft` lists a mating pawn drop
    // as a legal move (confirmed: a pawn drop that is checkmate appears in FSF's
    // perft divide). Since this variant is validated *node-for-node against FSF
    // perft*, mce matches FSF and therefore does **not** apply the uchifuzume
    // filter (`pawn_drop_mate_forbidden` stays at its `false` default). The engine
    // has the machinery (the hook + the mate test) should a strict-rules mode ever
    // be wanted, but enabling it here would diverge from FSF by exactly the count
    // of mating pawn drops.

    fn drop_targets(role: WideRole, color: Color, board: &Board<Shogi9x9>) -> Bitboard<Shogi9x9> {
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

impl ShogiRules {
    /// The mask of every square on `rank`.
    fn rank_mask(rank: u8) -> Bitboard<Shogi9x9> {
        let mut bb = Bitboard::<Shogi9x9>::EMPTY;
        for file in 0..Shogi9x9::WIDTH {
            if let Some(sq) = Square::<Shogi9x9>::from_file_rank(file, rank) {
                bb.set(sq);
            }
        }
        bb
    }

    /// The mask of every square on `file`.
    fn file_mask(file: u8) -> Bitboard<Shogi9x9> {
        let mut bb = Bitboard::<Shogi9x9>::EMPTY;
        for rank in 0..Shogi9x9::HEIGHT {
            if let Some(sq) = Square::<Shogi9x9>::from_file_rank(file, rank) {
                bb.set(sq);
            }
        }
        bb
    }

    /// The mask of the last two ranks for `color` (where a Knight has no move).
    fn last_two_mask(color: Color) -> Bitboard<Shogi9x9> {
        let (a, b) = match color {
            Color::White => (Shogi9x9::HEIGHT - 1, Shogi9x9::HEIGHT - 2),
            Color::Black => (0, 1),
        };
        Self::rank_mask(a) | Self::rank_mask(b)
    }
}

/// Shogi (Japanese chess) as a [`GenericPosition`] over the 9x9 geometry.
///
/// Construct the starting position with
/// [`Shogi::startpos`](GenericPosition::startpos) or parse a FEN — the placement
/// may carry the hand as a `[..]` holdings bracket — with
/// [`Shogi::from_fen`](GenericPosition::from_fen). See the [module docs](self)
/// for the hand, drops, and promotion zone.
pub type Shogi = GenericPosition<Shogi9x9, ShogiRules>;
