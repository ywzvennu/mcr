//! Okisaki Shogi (王妃将棋, "Queen's Shogi") — a **10x10** Shogi variant with
//! drops, on the generic engine. It reuses the Shogi / Minishogi persistent
//! **hand**, **drops**, and **promotion-zone** machinery on the ten-by-ten
//! (100-square) [`Grand10x10`] geometry (the board Grand chess already validates),
//! and adds two pieces absent from ordinary Shogi: a **Queen** and a **vertical
//! rook** (a rook that slides along its file in *both* directions).
//!
//! Okisaki Shogi is Fairy-Stockfish's built-in `okisakishogi`, derived there from
//! the Minishogi rule base (`minishogi_variant_base`) — the same hand / drop /
//! nifu / dead-piece / sennichite / perpetual-check / stalemate-is-loss rules —
//! widened to 10x10 with a larger army. **The available Fairy-Stockfish binary is
//! a non-large-board build and does not implement `okisakishogi`** (it silently
//! falls back to standard chess), so this variant carries **no live FSF perft
//! oracle**. It is instead *rules-validated*: `tests/perft_okisakishogi.rs`
//! hand-derives the start-position move count and cross-checks the engine's perft
//! against a fully **independent, from-scratch 10x10 move generator** (issue #500's
//! two-implementations-agree pattern), exactly as the other oracle-less variants
//! (Wa Shogi, Alice, Tenjiku) are handled.
//!
//! ## Pieces (confirmed against FSF `okisakishogi_variant()`; promoted forms in
//! parentheses)
//!
//! Standard Shogi army plus a Queen and a vertical rook; **note the Knight is the
//! ordinary chess knight here, not the forward-only Shogi knight**, and the corner
//! piece `l` is a vertical rook, **not** a Lance:
//!
//! * **King (K)** — a standard king.
//! * **Queen (Q)** — the orthodox chess queen (rook + bishop slides). **Never
//!   promotes.**
//! * **Rook (R, → Dragon King `+R`)** — a rook; promotes to a Dragon King (rook
//!   plus one diagonal step each direction).
//! * **Bishop (B, → Dragon Horse `+B`)** — a bishop; promotes to a Dragon Horse
//!   (bishop plus one orthogonal step each direction).
//! * **Vertical rook (L, `vR`, → Gold `+L`)** — slides any number of squares along
//!   its **file, forward *and* backward** (a bidirectional Lance / rook-confined-
//!   to-its-file). FSF's `CUSTOM_PIECE_1 = "vR"`. Promotes to a Gold General.
//! * **Knight (N, → Gold `+N`)** — the **ordinary chess knight** (all eight 2-1
//!   leaps), unlike the forward-only Shogi knight. Promotes to a Gold General.
//! * **Gold General (G)** — one step orthogonally or one step diagonally forward
//!   (six directions). Never promotes.
//! * **Silver General (S, → Gold `+S`)** — the four diagonals or one step straight
//!   forward (five directions).
//! * **Pawn (P, → Tokin `+P`)** — one step straight forward, capturing straight
//!   forward too (like Shogi, unlike chess).
//!
//! The vertical rook reuses the [`WideRole::Lance`] slot (FSF spells both `l`, and
//! the Lance↔[`WideRole::PromotedLance`] hand pair is exactly what a Gold-promoting
//! file-slider needs) but is given **bidirectional** file movement in this
//! variant's [`role_attacks`](WideVariant::role_attacks) — movement is per-variant
//! here, just as the [`WideRole::Knight`] slot is the ordinary chess knight in this
//! variant but a forward-only jumper in Shogi. No new [`WideRole`] is introduced.
//!
//! ## Promotion zone
//!
//! The furthest **three** ranks from each side (FSF `Rank8/9/10` for White,
//! `Rank1/2/3` for Black): 0-based ranks 7-9 for White, 0-2 for Black. A move that
//! **starts or ends** in the zone *may* promote (optional) — except a Pawn reaching
//! the last rank is **forced** to promote (it would otherwise have no move). The
//! chess Knight and the bidirectional vertical rook are never immobile, so neither
//! is ever force-promoted; the Gold, King, and Queen never promote, and a dropped
//! or already-promoted piece does not promote again.
//!
//! ## Hand and drops
//!
//! A captured piece is banked **unpromoted** and flipped to the captor's side; on a
//! turn a side may drop a held piece onto any empty square, subject to:
//!
//! 1. **No dead piece** — a Pawn may not be dropped on the last rank. (The chess
//!    Knight and the vertical rook always have a move from any square, so neither
//!    carries a last-rank drop restriction.)
//! 2. **Nifu** — no two *unpromoted* Pawns of the same side on one file.
//! 3. A dropped piece is always unpromoted.
//!
//! As in Shogi / Minishogi, **FSF does not enforce *uchifuzume*** (the no-pawn-drop-
//! mate rule) in its perft, so this variant matches FSF and leaves
//! `pawn_drop_mate_forbidden` at its `false` default.
//!
//! ## Confirmed starting FEN
//!
//! FSF `okisakishogi_variant()` gives the start as
//!
//! ```text
//! lnsgkqgsnl/1r6b1/pppppppppp/10/10/10/10/PPPPPPPPPP/1B6R1/LNSGQKGSNL[-] w 0 1
//! ```
//!
//! mcr uses the same board placement and an empty `[]` holdings bracket.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Geometry, PromotionConfig, Square, WideRole, WideVariant,
};
use crate::Color;

use super::super::Grand10x10;

/// The Okisaki Shogi rule layer: a zero-sized [`WideVariant`] over [`Grand10x10`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct OkisakiShogiRules;

/// The confirmed Okisaki Shogi starting placement (the hand is empty at the start).
const OKISAKI_PLACEMENT: &str =
    "lnsgkqgsnl/1r6b1/pppppppppp/10/10/10/10/PPPPPPPPPP/1B6R1/LNSGQKGSNL";

/// The four diagonal one-step (ferz) offsets.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// The four orthogonal one-step (wazir) offsets.
const WAZIR_OFFSETS: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// The depth of the promotion zone: the furthest three ranks from each side.
const ZONE_DEPTH: u8 = 3;

impl OkisakiShogiRules {
    /// The Gold General's attack set from `sq` for `color`: one step orthogonally
    /// (four directions) plus one step diagonally **forward** (two directions) —
    /// six squares. Every promoted minor (+P, +S, +N, +L) moves identically.
    fn gold_attacks(color: Color, sq: Square<Grand10x10>) -> Bitboard<Grand10x10> {
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
        attacks::leaper_attacks::<Grand10x10>(sq, &offsets)
    }

    /// The Silver General's attack set: the four diagonals plus one straight
    /// forward step (five squares).
    fn silver_attacks(color: Color, sq: Square<Grand10x10>) -> Bitboard<Grand10x10> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = attacks::leaper_attacks::<Grand10x10>(sq, &FERZ_OFFSETS);
        if let Some(dest) = sq.offset(0, fwd) {
            bb.set(dest);
        }
        bb
    }

    /// The Pawn's attack/movement square: the single square straight forward (it
    /// both moves and captures there).
    fn pawn_attacks(color: Color, sq: Square<Grand10x10>) -> Bitboard<Grand10x10> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = Bitboard::<Grand10x10>::EMPTY;
        if let Some(dest) = sq.offset(0, fwd) {
            bb.set(dest);
        }
        bb
    }

    /// The **vertical rook** (`vR`) attack set: the blocker-aware rook ray confined
    /// to `sq`'s file, in **both** directions. Reuses the Lance primitive twice —
    /// the forward file ray (as White) unioned with the backward file ray (the
    /// White ray for Black) — so it is a rook restricted to its file.
    fn vertical_rook_attacks(
        sq: Square<Grand10x10>,
        occupancy: Bitboard<Grand10x10>,
    ) -> Bitboard<Grand10x10> {
        attacks::lance_attacks::<Grand10x10>(Color::White, sq, occupancy)
            | attacks::lance_attacks::<Grand10x10>(Color::Black, sq, occupancy)
    }

    /// The last rank for `color` (rank 9 white / rank 0 black) — a Pawn there has no
    /// further move (forced promotion / no drop).
    fn last_rank(color: Color) -> u8 {
        match color {
            Color::White => Grand10x10::HEIGHT - 1,
            Color::Black => 0,
        }
    }
}

impl WideVariant<Grand10x10> for OkisakiShogiRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field — its highest-indexed role is the Dragon Horse
    /// (`WideRole::DragonHorse`, index 28), so the span is 29 (the same prefix Shogi
    /// and Minishogi field). See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 29;

    fn starting_position() -> (Board<Grand10x10>, GenericState<Grand10x10>) {
        let board = Board::<Grand10x10>::from_fen_placement(OKISAKI_PLACEMENT)
            .expect("the Okisaki Shogi starting placement is valid on a 10x10 board");
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
            checks_against: [0, 0],
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
            WideRole::Pawn => Self::pawn_attacks(color, sq),
            WideRole::Silver => Self::silver_attacks(color, sq),
            // Gold and every promoted minor (+P, +S, +N, +L) move as a Gold General.
            WideRole::Gold
            | WideRole::Tokin
            | WideRole::PromotedSilver
            | WideRole::PromotedKnight
            | WideRole::PromotedLance => Self::gold_attacks(color, sq),
            // The ordinary chess knight (all eight leaps), not the Shogi knight.
            WideRole::Knight => attacks::knight_attacks::<Grand10x10>(sq),
            // The vertical rook (`vR`): a bidirectional file slider; reuses the
            // Lance role slot but moves full-file both ways here.
            WideRole::Lance => Self::vertical_rook_attacks(sq, occupancy),
            WideRole::Queen => attacks::queen_attacks::<Grand10x10>(sq, occupancy),
            WideRole::Bishop => attacks::bishop_attacks::<Grand10x10>(sq, occupancy),
            WideRole::Rook => attacks::rook_attacks::<Grand10x10>(sq, occupancy),
            WideRole::King => attacks::king_attacks::<Grand10x10>(sq),
            // Dragon King (+R): rook plus one diagonal step in each direction.
            WideRole::Dragon => {
                attacks::rook_attacks::<Grand10x10>(sq, occupancy)
                    | attacks::leaper_attacks::<Grand10x10>(sq, &FERZ_OFFSETS)
            }
            // Dragon Horse (+B): bishop plus one orthogonal step in each direction.
            WideRole::DragonHorse => {
                attacks::bishop_attacks::<Grand10x10>(sq, occupancy)
                    | attacks::leaper_attacks::<Grand10x10>(sq, &WAZIR_OFFSETS)
            }
            _ => Bitboard::EMPTY,
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // The forward-biased pieces: the Pawn (forward capture), the Silver and Gold
        // Generals, and the Gold-moving promoted minors. Their attack sets point
        // forward, so the attacker scan must project the opposite colour from the
        // target. The chess Knight, the (symmetric) vertical rook reusing the Lance
        // slot, the Queen, Rook, Bishop, King, and the colour-symmetric Dragon /
        // Dragon Horse are not directional.
        matches!(
            role,
            WideRole::Pawn
                | WideRole::Silver
                | WideRole::Gold
                | WideRole::Tokin
                | WideRole::PromotedSilver
                | WideRole::PromotedKnight
                | WideRole::PromotedLance
        )
    }

    fn role_is_slider(role: WideRole) -> bool {
        // The Rook, Bishop, Queen, the vertical rook (Lance slot), and the promoted
        // Dragon / Dragon Horse slide along a ray and so can pin / be pinned; every
        // stepper (Gold family, Silver, chess Knight, Pawn, King) does not.
        matches!(
            role,
            WideRole::Rook
                | WideRole::Bishop
                | WideRole::Queen
                | WideRole::Lance
                | WideRole::Dragon
                | WideRole::DragonHorse
        )
    }

    fn promotion_config() -> PromotionConfig {
        // Okisaki's promotions are per-piece (each promotable role has exactly one
        // promoted form, handled by the generic per-piece promotion path); this
        // static set is unused, but the trait requires it.
        PromotionConfig {
            roles: alloc::vec![WideRole::Gold],
        }
    }

    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        match color {
            Color::White => rank >= Grand10x10::HEIGHT - ZONE_DEPTH,
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
        // The promotable base pieces. Gold, King, and Queen never promote, and a
        // piece already promoted does not promote again. The Lance slot here is the
        // vertical rook (promotes to a Gold, via WideRole::PromotedLance); the
        // Knight is the chess knight (promotes to a Gold, via
        // WideRole::PromotedKnight).
        matches!(
            role,
            WideRole::Pawn
                | WideRole::Silver
                | WideRole::Knight
                | WideRole::Lance
                | WideRole::Rook
                | WideRole::Bishop
        )
    }

    fn role_promotion_forced(role: WideRole, color: Color, to_rank: u8) -> bool {
        // Only the Pawn can become immobile: it alone is a pure forward stepper. The
        // chess Knight (eight leaps) and the bidirectional vertical rook always have
        // a move from any square, so neither is ever force-promoted.
        match role {
            WideRole::Pawn => to_rank == Self::last_rank(color),
            _ => false,
        }
    }

    fn drop_targets<const R: usize>(
        role: WideRole,
        color: Color,
        board: &Board<Grand10x10, R>,
    ) -> Bitboard<Grand10x10> {
        let mut mask = !board.occupied();
        // Dead-piece rule: a dropped Pawn may not land on the last rank (it would
        // then have no move). The Knight and vertical rook are never immobile.
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

    fn stalemate_is_loss() -> bool {
        // Stalemate is a loss for the stalemated side (inherited from
        // `minishogi_variant_base`, FSF `stalemateValue = -VALUE_MATE`); adjudication
        // only, so perft is byte-identical.
        true
    }
}

impl OkisakiShogiRules {
    /// The mask of every square on `rank`.
    fn rank_mask(rank: u8) -> Bitboard<Grand10x10> {
        let mut bb = Bitboard::<Grand10x10>::EMPTY;
        for file in 0..Grand10x10::WIDTH {
            if let Some(sq) = Square::<Grand10x10>::from_file_rank(file, rank) {
                bb.set(sq);
            }
        }
        bb
    }

    /// The mask of every square on `file`.
    fn file_mask(file: u8) -> Bitboard<Grand10x10> {
        let mut bb = Bitboard::<Grand10x10>::EMPTY;
        for rank in 0..Grand10x10::HEIGHT {
            if let Some(sq) = Square::<Grand10x10>::from_file_rank(file, rank) {
                bb.set(sq);
            }
        }
        bb
    }
}

/// Okisaki Shogi (10x10 Shogi with a Queen and a vertical rook) as a
/// [`GenericPosition`] over the 10x10 geometry.
///
/// Construct the starting position with
/// [`OkisakiShogi::startpos`](GenericPosition::startpos) or parse a FEN — the
/// placement may carry the hand as a `[..]` holdings bracket — with
/// [`OkisakiShogi::from_fen`](GenericPosition::from_fen). See the [module
/// docs](self) for the hand, drops, and three-rank promotion zone.
pub type OkisakiShogi = GenericPosition<
    Grand10x10,
    OkisakiShogiRules,
    { <OkisakiShogiRules as WideVariant<Grand10x10>>::ROLE_SPAN },
>;
