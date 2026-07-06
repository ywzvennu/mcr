//! Orda (8x8) on the generic engine — a standard White army against the Black
//! **Orda** (Mongolian cavalry) army of leaper pieces, plus the **flag-win**
//! (campmate) terminal rule. Asymmetric like Spartan
//! (`docs/fairy-variants-architecture.md` §4.4). Validated against
//! Fairy-Stockfish `UCI_Variant orda`.
//!
//! ## Armies
//!
//! * **White = standard chess.** The six standard pieces with standard castling
//!   (the only side that castles) and the standard pawn. Every White movement is
//!   the trait default. (White pawns promote per the Orda promotion rule below.)
//! * **Black = Orda.** A Mongolian cavalry army whose distinctive pieces every
//!   **move like a knight** but **capture along a slider line** — confirmed
//!   square-for-square against FSF:
//!   * **Lancer** ([`WideRole::Lancer`], FSF `kniroo` `l`, mcr `f`) — moves like a
//!     knight to an empty square; captures like a **rook**.
//!   * **Kheshig** ([`WideRole::Kheshig`], FSF `centaur` `h`, mcr `w`) — a **King +
//!     Knight** leaper (sixteen squares); moves and captures alike.
//!   * **Archer** ([`WideRole::Archer`], FSF `knibis` `a`, mcr `y`) — moves like a
//!     knight to an empty square; captures like a **bishop**.
//!   * **Yurt** ([`WideRole::Silver`], FSF `silver` `y`, mcr `s`) — a **silver
//!     general**: the four diagonals plus one straight-forward step (its forward
//!     is toward White, so its attack set is color-directional).
//!   * **King** ([`WideRole::King`], `k`) — a standard king (one).
//!   * **Pawns** ([`WideRole::Pawn`], `p`) — standard pawns.
//!
//! ## Promotion
//!
//! A pawn (of **either** colour) reaching the last rank promotes to a **Queen** or
//! a **Kheshig** (FSF `promotionPieceTypes = qh`, applied to both sides) — never to
//! a Rook/Bishop/Knight, and the Orda leaper pieces themselves never promote. So
//! White can acquire a Kheshig (the only Orda piece reachable by White).
//!
//! ## Flag win (campmate)
//!
//! White wins the instant its king reaches the **last rank**; Black wins the
//! instant its king reaches the **first rank** (FSF `flagPiece = k`,
//! `flagRegionWhite = *8`, `flagRegionBlack = *1`). The win is **purely
//! positional** — a king on its goal rank wins even while in check — and FSF
//! adjudicates it on the **losing** side's turn: a node where the side to move's
//! opponent already stands on its goal rank is terminal (no children), exactly as
//! the [`WideVariant::has_flag_win`] / [`WideVariant::flag_rank`] hooks express
//! (the engine's shared `flag_win_reached` test). This is what makes mcr's perft
//! match FSF's `go perft` at a flag node.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant orda`, `position startpos`) renders the start as
//!
//! ```text
//! lhaykahl/8/pppppppp/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1
//! ```
//!
//! with FSF's Orda letters `l h a y k` (Lancer, Kheshig, Archer, Yurt, King). mcr
//! reuses `l`/`h`/`a` for its Lance/Hoplite/Hawk, so the Orda pieces take distinct
//! letters — Lancer `f`, Kheshig `w`, Archer `y`, Yurt `s` (the existing Silver):
//!
//! ```text
//! fwyskywf/8/pppppppp/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1
//! ```
//!
//! Note the **asymmetry**: the Black Orda pawns start on the **6th rank** (one
//! rank advanced, with the 7th rank empty) and never make a double step — they
//! are not on the standard Black double-push rank, so the trait default already
//! gives them a single step only, matching FSF. The two FENs are the same
//! position; the `compare-fairy/` harness translates the Orda letters when driving
//! FSF. Only White has castling rights (`KQ`).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Chess8x8, Geometry, PromotionConfig, Square, StandardChess, WideRole,
    WideVariant,
};
use crate::Color;

/// The Orda rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides the Orda piece movements (Lancer / Kheshig / Archer / Yurt), the
/// `q`/Kheshig promotion target set, the silver-general attacker direction, and
/// the flag-win terminal rule. White's pieces, castling, and pawn pushes are the
/// trait defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct OrdaRules;

/// The confirmed Orda starting placement in mcr's role letters: White standard
/// (`RNBQKBNR`/`PPPPPPPP` on ranks 1-2), Black `f w y s k y w f` on the back rank
/// (Lancer, Kheshig, Archer, Yurt, King, Archer, Kheshig, Lancer) with its pawns
/// **one rank advanced** on the 6th rank (the 7th rank is empty) — the Orda
/// asymmetry, confirmed against FSF.
const ORDA_START_PLACEMENT: &str = "fwyskywf/8/pppppppp/8/8/8/PPPPPPPP/RNBQKBNR";

/// The four ferz (one diagonal step) offsets — the Yurt / silver-general's
/// diagonal component.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

impl OrdaRules {
    /// The Yurt's attack/move set: a **silver general** — the four diagonals plus
    /// a single straight-forward step (toward the enemy back rank). Color-directional
    /// (the forward step flips with the side), so it must also be listed in
    /// [`role_attack_is_directional`](WideVariant::role_attack_is_directional).
    fn yurt_attacks(color: Color, sq: Square<Chess8x8>) -> Bitboard<Chess8x8> {
        let forward: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = attacks::leaper_attacks::<Chess8x8>(sq, &FERZ_OFFSETS);
        if let Some(dest) = sq.offset(0, forward) {
            bb.set(dest);
        }
        bb
    }
}

impl WideVariant<Chess8x8> for OrdaRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 33;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(ORDA_START_PLACEMENT)
            .expect("the Orda starting placement is valid on an 8x8 board");
        // Only White (the standard army) has castling rights; Black's Orda back
        // rank never castles. The kingside rook sits on the last file, the
        // queenside rook on file 0.
        let mut castling = GenericCastling::NONE;
        castling.set(Color::White, 0, Some(Chess8x8::WIDTH - 1));
        castling.set(Color::White, 1, Some(0));
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
        sq: Square<Chess8x8>,
        occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        match role {
            // Lancer: captures like a rook (its only capturing / checking squares).
            // Its non-capturing knight jumps are quiet-only (see
            // `quiet_only_targets`), so they are NOT in the attack set.
            WideRole::Lancer => attacks::rook_attacks::<Chess8x8>(sq, occupancy),
            // Archer: captures like a bishop (its only capturing / checking
            // squares); knight jumps are quiet-only.
            WideRole::Archer => attacks::bishop_attacks::<Chess8x8>(sq, occupancy),
            // Kheshig: King + Knight leaper — moves and captures alike.
            WideRole::Kheshig => {
                attacks::king_attacks::<Chess8x8>(sq) | attacks::knight_attacks::<Chess8x8>(sq)
            }
            // Yurt: silver general (color-directional).
            WideRole::Silver => Self::yurt_attacks(color, sq),
            // White's army and the king are standard.
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn quiet_only_targets(
        role: WideRole,
        _color: Color,
        sq: Square<Chess8x8>,
        _occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        // The Lancer and Archer **move** like a knight (to empty squares) but never
        // capture there — their capture set (rook / bishop slide) is in
        // `role_attacks`. The generic generator filters these by emptiness, so the
        // knight pattern is emitted only as a quiet move.
        match role {
            WideRole::Lancer | WideRole::Archer => attacks::knight_attacks::<Chess8x8>(sq),
            _ => Bitboard::EMPTY,
        }
    }

    fn role_attacks_are_capture_only(role: WideRole) -> bool {
        // The Lancer (rook slide) and Archer (bishop slide) capture along their
        // slider lines but **move** like a knight (their `quiet_only_targets`), so
        // their `role_attacks` squares are reachable only by capture.
        matches!(role, WideRole::Lancer | WideRole::Archer)
    }

    fn role_is_slider(role: WideRole) -> bool {
        match role {
            // The Lancer (rook capture) and Archer (bishop capture) slide along
            // their capture lines, so they can be pinned.
            WideRole::Lancer | WideRole::Archer => true,
            // The Kheshig and Yurt are pure leapers/steppers.
            WideRole::Kheshig | WideRole::Silver => false,
            _ => <StandardChess as WideVariant<Chess8x8>>::role_is_slider(role),
        }
    }

    // --- promotion: pawns -> Queen or Kheshig (both colours) ---------------

    fn promotion_config() -> PromotionConfig {
        PromotionConfig {
            roles: alloc::vec![WideRole::Queen, WideRole::Kheshig],
        }
    }

    // --- attacker-detection consistency -----------------------------------

    fn role_attack_is_directional(role: WideRole) -> bool {
        // The Yurt (silver general) is forward-biased, so a piece of one colour
        // attacking a square is found by reverse-projecting the *opposite* colour's
        // pattern. The Lancer / Archer capture sets are plain rook / bishop slides
        // (geometrically symmetric, color-non-directional), so they need no flag —
        // their knight *moves* are quiet-only and never enter the attack relation.
        matches!(role, WideRole::Pawn | WideRole::Silver)
    }

    // --- flag win (campmate) ----------------------------------------------

    fn has_flag_win() -> bool {
        true
    }

    // The flag goal ranks (White's last rank, Black's first) are exactly the
    // generic `flag_rank` default, so Orda does not override it: the engine's
    // shared `flag_win_reached` check handles the loser-to-move termination on the
    // standard single-king path.
}

/// Orda as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position (standard White vs the Black Orda cavalry
/// army) with [`Orda::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Orda::from_fen`](GenericPosition::from_fen). See the [module docs](self) for
/// the piece movements, the Queen/Kheshig promotion, and the flag-win rule.
pub type Orda = GenericPosition<Chess8x8, OrdaRules>;
