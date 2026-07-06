//! Shako (10x10) on the generic engine — standard chess on a ten-by-ten board
//! plus two fairy pieces: the **Cannon** and the **Elephant**
//! (`docs/fairy-variants-architecture.md`, Phase 2/3 bridge). Its real value is
//! the **reusable cannon-attack primitive** ([`attacks::cannon_quiet_moves`] /
//! [`attacks::cannon_capture_targets`]) that the future Xiangqi and Janggi
//! variants inherit, proven here on the already-validated `u128` [`Grand10x10`]
//! geometry.
//!
//! Shako (Esperanto for "chess") is played on files a..j, ranks 1..10. The army
//! is the full standard chess set on a wider board, plus:
//!
//! * **Cannon** (`c`) — moves like a rook along empty squares but **captures only
//!   by jumping exactly one intervening piece** (the "screen"/"mount") of either
//!   colour, landing on the first piece beyond it. This is the Xiangqi cannon,
//!   implemented as the geometry-only [`attacks::cannon_capture_targets`] (its
//!   attack/threat set) plus [`attacks::cannon_quiet_moves`] (its non-capturing
//!   rook-rays, wired through the [`WideVariant::quiet_only_targets`] hook).
//! * **Elephant** (`v` in the mcr dialect, `e` in FSF) — a **Fers-Alfil**
//!   leaper: one diagonal step (Ferz) or a two-square diagonal jump over the
//!   intervening square (Alfil). mcr's [`WideRole::FersAlfil`]. Distinct from the
//!   Rook+Knight [`WideRole::Elephant`] (the Capablanca/Grand marshal, letter
//!   `e`), so it takes the free letter `v`.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant shako` (its `shako_variant()`
//! `startFen`):
//!
//! ```text
//! FSF dialect: c8c/ernbqkbnre/pppppppppp/10/10/10/10/PPPPPPPPPP/ERNBQKBNRE/C8C w KQkq - 0 1
//! mcr dialect: c8c/vrnbqkbnrv/pppppppppp/10/10/10/10/PPPPPPPPPP/VRNBQKBNRV/C8C w KQkq - 0 1
//! ```
//!
//! The two strings differ only in the elephant's letter (`e` in FSF, `v` in mcr).
//! The **cannons** sit in the four corners (a1/j1, a10/j10). Rank 2 (white) and
//! rank 9 (black) hold, a-file to j-file: Elephant, Rook, Knight, Bishop, Queen,
//! King, Bishop, Knight, Rook, Elephant — so the king is on the **f-file**
//! (file 5) and the rooks on the b/i files. Pawns sit on ranks 3 and 8.
//!
//! ## Rules that differ from standard chess
//!
//! * **Castling happens on rank 2** (white) / rank 9 (black) — the rank the king
//!   and rooks occupy, since the cannons hold the back rank. Wired through the
//!   default-off [`WideVariant::castle_rank`] hook. Kingside the king (f-file)
//!   goes to the **h-file** (file 7) with the i-file rook landing on the g-file
//!   (file 6); queenside the king goes to the **d-file** (file 3) with the b-file
//!   rook landing on the e-file (file 4) — matching FSF's `castlingKingsideFile =
//!   FILE_H`, `castlingQueensideFile = FILE_D`, rook beside the king toward
//!   centre.
//! * **Pawns** double-push from rank 3 / rank 8, take en passant, and **promote
//!   on the last rank** (rank 10 / rank 1) to Queen, Rook, Bishop, Knight,
//!   **Cannon, or Elephant** — FSF's `promotionPieceTypes`.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Grand10x10, PromotionConfig, RoyalSlider, Square, WideRole,
};
use crate::geometry::{StandardChess, WideVariant};
use crate::Color;

/// The Shako rule layer: a zero-sized [`WideVariant`] over [`Grand10x10`].
///
/// It overrides only what Shako changes from the standard generic engine: the
/// 10x10 starting array, the Cannon and Fers-Alfil Elephant movement, the
/// rank-2 castle geometry, and the wider promotion set. Standard roles delegate
/// to [`StandardChess`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ShakoRules;

/// The confirmed Shako starting placement in the mcr dialect (elephant = `v`/`V`),
/// byte-for-byte equivalent to Fairy-Stockfish's
/// `c8c/ernbqkbnre/.../ERNBQKBNRE/C8C` modulo the elephant's letter.
const SHAKO_START_PLACEMENT: &str =
    "c8c/vrnbqkbnrv/pppppppppp/10/10/10/10/PPPPPPPPPP/VRNBQKBNRV/C8C";

/// The kingside / queenside castle side indices, matching the position layer.
const KINGSIDE: usize = 0;
const QUEENSIDE: usize = 1;

/// The Fers (one diagonal step) plus Alfil (two-square diagonal jump) offsets of
/// the Shako elephant (Betza `FA`).
const FERS_ALFIL_OFFSETS: [(i8, i8); 8] = [
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
    (2, 2),
    (2, -2),
    (-2, 2),
    (-2, -2),
];

impl WideVariant<Grand10x10> for ShakoRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 19;

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
        let board = Board::<Grand10x10>::from_fen_placement(SHAKO_START_PLACEMENT)
            .expect("the Shako starting placement is valid on a 10x10 board");
        // Both colours castle with both rooks. The castling layer stores the
        // rook *files* (rank-independent); Shako's castling rooks are the i-file
        // (kingside, file 8) and b-file (queenside, file 1) — the outermost rooks
        // on the castle rank, since the a/j files hold elephants and the corners
        // hold cannons.
        let mut castling = GenericCastling::NONE;
        for color in Color::ALL {
            castling.set(color, KINGSIDE, Some(8));
            castling.set(color, QUEENSIDE, Some(1));
        }
        let state = GenericState {
            turn: Color::White,
            castling,
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
        sq: Square<Grand10x10>,
        occupancy: Bitboard<Grand10x10>,
    ) -> Bitboard<Grand10x10> {
        match role {
            // The Cannon's *attack set* — the squares it threatens and may
            // capture on — is the over-one-screen capture set. Its quiet
            // rook-rays are not attacks (they cannot capture), so they are not
            // here; they are emitted via `quiet_only_targets`.
            WideRole::Cannon => attacks::cannon_capture_targets::<Grand10x10>(sq, occupancy),
            // The Fers-Alfil Elephant: a one-diagonal-step / two-diagonal-jump
            // leaper (the jump is unobstructed, like every leaper).
            WideRole::FersAlfil => attacks::leaper_attacks::<Grand10x10>(sq, &FERS_ALFIL_OFFSETS),
            // Standard roles (pawn, knight, sliders, king) are the trait default.
            _ => {
                <StandardChess as WideVariant<Grand10x10>>::role_attacks(role, color, sq, occupancy)
            }
        }
    }

    fn quiet_only_targets(
        role: WideRole,
        _color: Color,
        sq: Square<Grand10x10>,
        occupancy: Bitboard<Grand10x10>,
    ) -> Bitboard<Grand10x10> {
        match role {
            // The Cannon's non-capturing moves: the empty rook-ray squares. (Its
            // captures are the over-screen set in `role_attacks`.) The generic
            // generator restricts these to empty squares already, so they never
            // collide with the capture set.
            WideRole::Cannon => attacks::cannon_quiet_moves::<Grand10x10>(sq, occupancy),
            _ => Bitboard::EMPTY,
        }
    }

    fn role_is_slider(role: WideRole) -> bool {
        // The Cannon is *not* a line slider for pin purposes: it cannot pin a
        // lone blocker the way a rook does (it needs a screen to capture). The
        // Fers-Alfil Elephant is a leaper. Everything else keeps the standard
        // classification.
        <StandardChess as WideVariant<Grand10x10>>::role_is_slider(role)
    }

    fn role_attack_is_leg_asymmetric(role: WideRole) -> bool {
        // The Cannon's over-screen capture set lands only on an *occupied* square
        // (the captured piece), so it is occupancy-asymmetric: reverse-projecting
        // the cannon pattern from a target `t` treats `t` as a cannon origin and
        // reports a cannon attacker even when `t` is *empty*, where a cannon —
        // capturing nothing — does not attack. That phantom is harmless on an
        // occupied royal square but is a genuine asymmetry, so attacker detection
        // forward-projects from each cannon (exactly as the move generator does),
        // keeping `attackers_to` the true forward relation on every square. The
        // Fers-Alfil Elephant is a plain symmetric leaper and needs no special
        // handling. (Issue #202.)
        matches!(role, WideRole::Cannon)
    }

    fn royal_slider_kind(role: WideRole) -> Option<RoyalSlider> {
        // The Rook, Bishop, and Queen are the plain standard sliders here (the
        // `role_attacks` default), so the cannon king-safety verify can reverse-
        // project them from the king with its precomputed line masks rather than
        // rebuilding the slider masks every sibling move. Identical result, no
        // per-move diagonal fill. The Cannon (asymmetric, screen-dependent) and the
        // Fers-Alfil Elephant (a leaper) are not standard sliders.
        match role {
            WideRole::Rook => Some(RoyalSlider::Rook),
            WideRole::Bishop => Some(RoyalSlider::Bishop),
            WideRole::Queen => Some(RoyalSlider::Queen),
            _ => None,
        }
    }

    fn promotion_config() -> PromotionConfig {
        // A Shako pawn promotes to any of Queen, Rook, Bishop, Knight, Cannon, or
        // Elephant — FSF's `promotionPieceTypes`. Order affects only enumeration,
        // not the perft leaf count.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Knight,
                WideRole::Bishop,
                WideRole::Rook,
                WideRole::Queen,
                WideRole::Cannon,
                WideRole::FersAlfil,
            ],
        }
    }

    fn has_cannons() -> bool {
        // Shako fields cannons, whose screen-dependent check and king-danger
        // require the engine's pseudo-legal + verify king-safety path.
        true
    }

    fn double_push_rank(color: Color) -> u8 {
        // Pawns start on (and double-push from) rank 3 (index 2) for white and
        // rank 8 (index 7) for black — FSF's `doubleStepRegion` Rank3/Rank8.
        match color {
            Color::White => 2,
            Color::Black => 7,
        }
    }

    fn castle_rank(color: Color) -> u8 {
        // King and rooks live on rank 2 (index 1) for white, rank 9 (index 8) for
        // black — the cannons hold the back rank.
        match color {
            Color::White => 1,
            Color::Black => 8,
        }
    }

    fn castle_dest_files(side: usize) -> (u8, u8) {
        // Shako castling, matching FSF: castlingKingsideFile = FILE_H (7),
        // castlingQueensideFile = FILE_D (3), with the rook ending beside the king
        // toward the centre. The king starts on the f-file (5); the kingside rook
        // on the i-file (8) lands on g (6), the queenside rook on the b-file (1)
        // lands on e (4).
        if side == KINGSIDE {
            (7, 6)
        } else {
            (3, 4)
        }
    }
}

/// Shako as a [`GenericPosition`] over the 10x10 [`Grand10x10`] geometry.
///
/// Construct the starting position with
/// [`Shako::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Shako::from_fen`](GenericPosition::from_fen). The Cannon uses the reusable
/// cannon primitive in [`attacks`]; the Elephant is a
/// Fers-Alfil leaper. Everything else is standard chess on a wider board, with
/// castling on rank 2 / 9.
pub type Shako = GenericPosition<Grand10x10, ShakoRules>;
