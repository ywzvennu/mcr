//! Shinobi (8x8) on the generic engine — a **fixed-reserve hand with drops** and
//! a **mandatory per-piece promotion zone** over an otherwise standard board.
//! Validated against Fairy-Stockfish `UCI_Variant shinobi`.
//!
//! Shinobi pits a standard chess **Black** army against a White **clan** that
//! starts with fewer back-rank pieces but holds a **reserve in hand** it may
//! **drop** onto its own half, and whose pieces **promote** into standard chess
//! pieces when they reach the far ranks. White moves first.
//!
//! ## Armies
//!
//! * **Black = the standard chess army** (`rnbqkbnr` / `pppppppp`), with standard
//!   castling — the only side that castles. Its pawns, however, promote into a
//!   **Commoner** (see below), not into `N/B/R/Q`.
//! * **White = the clan.** A back rank of Lances, Shogi Knights, a Commoner and
//!   the King (`L*N1*UK1*NL` in mcr letters), a rank of standard pawns, and a
//!   **starting hand** `[L*NMMDA]` (two Fers, a Lance, a Shogi Knight, a Bers, and
//!   an Archbishop) it may drop. The clan pieces, in mcr roles:
//!   * **Commoner** ([`WideRole::Commoner`]) — a non-royal king-mover (one step in
//!     any of the eight directions); it may be captured freely and never defines
//!     check. A promoting Pawn (either colour) becomes a Commoner.
//!   * **Bers** ([`WideRole::General`]) — Rook + Ferz (the Shogi dragon-king move):
//!     orthogonal slides plus a single diagonal step. Identical to the Spartan
//!     General, and FSF spells both `d`.
//!   * **Archbishop** ([`WideRole::Hawk`]) — Bishop + Knight, the census compound
//!     (Seirawan's Hawk / Capablanca's Archbishop).
//!   * **Fers** ([`WideRole::Met`]) — one diagonal step; promotes into a Bishop.
//!   * **Shogi Knight** ([`WideRole::ShogiKnight`]) — a forward-only 2-1 leaper
//!     (two targets); promotes into a standard Knight.
//!   * **Lance** ([`WideRole::Lance`]) — a forward-only rook slider; promotes into
//!     a standard Rook.
//!
//! ## Hand and drops
//!
//! Unlike Shogi/crazyhouse, **a capture does not feed the hand** — captured pieces
//! simply vanish. The hand is the **fixed starting reserve**, depleted only by
//! drops. A held piece may be dropped onto any empty square in the dropping side's
//! own half: ranks 1-4 for White, ranks 5-8 for Black (Black never naturally has a
//! hand, but the rule is colour-symmetric). A Pawn may not be dropped on the last
//! rank (it would be immobile); every other clan piece always has a move within
//! its drop region, so no further drop restriction bites. Drops giving check — and
//! even checkmate — are legal (no *uchifuzume*).
//!
//! ## Promotion zone (mandatory)
//!
//! The promotion zone is the **far two ranks** from each side: ranks 7-8 for
//! White, ranks 1-2 for Black. A Pawn, Fers, Shogi Knight, or Lance whose move
//! **starts or ends** in the zone **must** promote (`mandatoryPiecePromotion`):
//! Pawn → Commoner, Fers → Bishop, Shogi Knight → Knight, Lance → Rook. There is
//! never a non-promoting alternative on a zone move. The Pawn rides the standard
//! pawn generator (its only promotion target is the Commoner); the other three
//! ride the generic per-piece promotion path.
//!
//! ## Flag win
//!
//! A king that reaches the opponent's back rank — White on rank 8, Black on rank 1
//! — wins immediately. Once a king sits on its flag rank the game is over, so the
//! opponent has no reply: such a node is terminal (zero perft children), matching
//! FSF's `go perft`.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant shinobi`, `position startpos`) renders the start as
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/LH1CK1HL[LHMMDJ] w kq - 0 1
//! ```
//!
//! with FSF's clan letters `c d h j` (Commoner, Bers, Shogi Knight, Archbishop).
//! mcr uses the same board but its own role tokens. The Commoner and Shogi Knight
//! land past the exhausted single-letter alphabet, so they are **overflow** roles
//! written with the `*` prefix and a recycled base letter: Commoner `*u`
//! (recycling the Advisor's `u`, shared with Synochess's Commoner) and Shogi
//! Knight `*n` (recycling the Knight's `n`). The Bers is `d` (General), and the
//! Archbishop `a` (Hawk):
//!
//! ```text
//! rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/L*N1*UK1*NL[L*NMMDA] w kq - 0 1
//! ```
//!
//! The two are the same position; the `compare-fairy/` harness translates the clan
//! letters when driving FSF. Only Black has castling rights (`kq`).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Chess8x8, Geometry, PromotionConfig, Square, StandardChess, WideRole,
    WideVariant,
};
use crate::Color;

/// The Shinobi rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ShinobiRules;

/// The confirmed Shinobi starting placement, in mcr's role letters (Black = the
/// standard army; White = `L *N _ *U K _ *N L` on the back rank — Lance, Shogi
/// Knight, Commoner, King — over a rank of standard pawns). The Shogi Knight and
/// Commoner are **overflow** roles with no bare letter, so they carry the `*`
/// prefix (`*N` = Shogi Knight, `*U` = Commoner); a plain `Y`/`F` would parse as
/// the Orda Archer / Lancer instead. The hand rides in the FEN's `[..]` holdings
/// bracket, not here.
const SHINOBI_START_PLACEMENT: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/L*N1*UK1*NL";

/// The four diagonal one-step (ferz) offsets — the Bers's diagonal component and
/// the Fers's whole move.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// The depth of the promotion zone: the far two ranks from each side.
const ZONE_DEPTH: u8 = 2;

impl ShinobiRules {
    /// The Shogi Knight's attack set for `color` on `sq`: the two forward 2-1
    /// jumps only (it never moves backward or sideways).
    fn shogi_knight_attacks(color: Color, sq: Square<Chess8x8>) -> Bitboard<Chess8x8> {
        let fwd: i8 = if color.is_white() { 1 } else { -1 };
        attacks::leaper_attacks::<Chess8x8>(sq, &[(1, 2 * fwd), (-1, 2 * fwd)])
    }
}

impl WideVariant<Chess8x8> for ShinobiRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 35;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(SHINOBI_START_PLACEMENT)
            .expect("the Shinobi starting placement is valid on an 8x8 board");
        // Only Black has castling rights (it is the side with the standard
        // king-and-rook back rank). The kingside rook sits on the last file, the
        // queenside rook on file 0.
        let mut castling = GenericCastling::NONE;
        castling.set(Color::Black, 0, Some(Chess8x8::WIDTH - 1));
        castling.set(Color::Black, 1, Some(0));
        // White's starting reserve `[L*NMMDA]`: a Lance, a Shogi Knight, two Fers
        // (Met), a Bers (General), and an Archbishop (Hawk). Black has none.
        let mut white = [0u8; WideRole::COUNT];
        white[WideRole::Lance.index()] = 1;
        white[WideRole::ShogiKnight.index()] = 1;
        white[WideRole::Met.index()] = 2;
        white[WideRole::General.index()] = 1;
        white[WideRole::Hawk.index()] = 1;
        let placement = GenericPlacement::new(white, [0u8; WideRole::COUNT]);
        let state = GenericState {
            turn: Color::White,
            castling,
            ep_square: None,
            ep_captured: None,
            gating: GenericGating::NONE,
            duck: None,
            placement,
            halfmove_clock: 0,
            fullmove_number: 1,
            consecutive_passes: 0,
            board_b: crate::geometry::Bitboard::EMPTY,
            petrified: crate::geometry::Bitboard::EMPTY,
            checks_against: [0, 0],
            jieqi_seed: None,
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
            // Commoner: a king's eight one-steps, but non-royal.
            WideRole::Commoner => attacks::king_attacks::<Chess8x8>(sq),
            // Shogi Knight: the two forward 2-1 leaps.
            WideRole::ShogiKnight => Self::shogi_knight_attacks(color, sq),
            // Bers (= Spartan General): Rook + Ferz.
            WideRole::General => {
                attacks::rook_attacks::<Chess8x8>(sq, occupancy)
                    | attacks::leaper_attacks::<Chess8x8>(sq, &FERZ_OFFSETS)
            }
            // Fers (= Met): one diagonal step.
            WideRole::Met => attacks::leaper_attacks::<Chess8x8>(sq, &FERZ_OFFSETS),
            // Lance: a forward-only rook slider.
            WideRole::Lance => attacks::lance_attacks::<Chess8x8>(color, sq, occupancy),
            // Archbishop = Hawk (Bishop + Knight): the census compound.
            // Black's standard army and the kings are standard.
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn role_is_slider(role: WideRole) -> bool {
        match role {
            // The Bers slides orthogonally (its rook component can be pinned), and
            // the Lance slides on its forward file.
            WideRole::General | WideRole::Lance => true,
            // The Commoner, Fers, and Shogi Knight are pure steppers/leapers.
            WideRole::Commoner | WideRole::Met | WideRole::ShogiKnight => false,
            // The Archbishop = Hawk and every standard role keep the default
            // classification.
            _ => <StandardChess as WideVariant<Chess8x8>>::role_is_slider(role),
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // The forward-biased pieces whose attack set is not color-symmetric: the
        // Pawn (diagonal capture) and the Shogi Knight and Lance (forward only).
        // The reverse-projection in `attackers_to` must flip the color for these.
        // The Commoner, Fers, Bers, Archbishop are color-symmetric.
        matches!(
            role,
            WideRole::Pawn | WideRole::ShogiKnight | WideRole::Lance
        )
    }

    // --- promotion zone (mandatory, per-piece) ----------------------------

    fn promotion_config() -> PromotionConfig {
        // A promoting Pawn (the only piece on the standard pawn path) becomes a
        // Commoner — the sole target, with no `N/B/R/Q` choice.
        PromotionConfig {
            roles: alloc::vec![WideRole::Commoner],
        }
    }

    fn in_promotion_zone(color: Color, rank: u8) -> bool {
        match color {
            Color::White => rank >= Chess8x8::HEIGHT - ZONE_DEPTH,
            Color::Black => rank < ZONE_DEPTH,
        }
    }

    fn role_can_promote(role: WideRole) -> bool {
        // The non-pawn promotable clan pieces (the Pawn promotes via the standard
        // pawn path, not the generic per-piece path).
        matches!(
            role,
            WideRole::Met | WideRole::ShogiKnight | WideRole::Lance
        )
    }

    fn role_promoted_to(role: WideRole) -> WideRole {
        match role {
            WideRole::Met => WideRole::Bishop,
            WideRole::ShogiKnight => WideRole::Knight,
            WideRole::Lance => WideRole::Rook,
            other => other,
        }
    }

    fn promotion_mandatory_in_zone() -> bool {
        true
    }

    // --- fixed-reserve hand + drops ---------------------------------------

    fn has_hand() -> bool {
        true
    }

    fn pawn_is_stepper() -> bool {
        // Shinobi pawns are ordinary chess pawns (double push, diagonal capture,
        // en passant), not Shogi forward-steppers.
        false
    }

    fn captures_to_hand() -> bool {
        // Captures vanish; the hand is a fixed starting reserve depleted only by
        // drops (FSF `capturesToHand = false`), exactly like Synochess.
        false
    }

    fn drop_targets<const R: usize>(
        role: WideRole,
        color: Color,
        board: &Board<Chess8x8, R>,
    ) -> Bitboard<Chess8x8> {
        // The dropping side's own half: ranks 1-4 (0-based 0-3) for White, ranks
        // 5-8 (0-based 4-7) for Black.
        let mut mask = !board.occupied() & Self::own_half(color);
        // A dropped Pawn may not land on the last rank (it would be immobile).
        // Every other clan piece always has a move within its drop region.
        if role == WideRole::Pawn {
            mask &= !Self::rank_mask(Self::last_rank(color));
        }
        mask
    }

    // --- flag win ---------------------------------------------------------

    fn has_flag_win() -> bool {
        // A king reaching the opponent's back rank wins immediately; once a king
        // sits on its flag rank the opponent (the side to move) has no reply and
        // the node is a perft leaf. This rides the unified flag-win machinery (the
        // generic generator short-circuits to zero moves via `flag_win_reached`),
        // shared with Orda and Synochess; the default `flag_rank` — the opponent's
        // back rank — is already correct, so it needs no override.
        true
    }
}

impl ShinobiRules {
    /// The own-half mask for `color`: ranks 1-4 (White) / ranks 5-8 (Black).
    fn own_half(color: Color) -> Bitboard<Chess8x8> {
        let mut bb = Bitboard::<Chess8x8>::EMPTY;
        let ranks = match color {
            Color::White => 0..(Chess8x8::HEIGHT / 2),
            Color::Black => (Chess8x8::HEIGHT / 2)..Chess8x8::HEIGHT,
        };
        for rank in ranks {
            bb |= Self::rank_mask(rank);
        }
        bb
    }

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

    /// The last rank for `color` (rank 8 White / rank 1 Black) — where a dropped
    /// Pawn would have no move.
    fn last_rank(color: Color) -> u8 {
        match color {
            Color::White => Chess8x8::HEIGHT - 1,
            Color::Black => 0,
        }
    }
}

/// Shinobi as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position (the standard Black army versus the White clan
/// with its drop reserve) with [`Shinobi::startpos`](GenericPosition::startpos) or
/// parse a FEN — the placement may carry the hand as a `[..]` holdings bracket —
/// with [`Shinobi::from_fen`](GenericPosition::from_fen). See the
/// [module docs](self) for the clan pieces, the fixed-reserve hand and drops, the
/// mandatory promotion zone, and the flag win.
pub type Shinobi =
    GenericPosition<Chess8x8, ShinobiRules, { <ShinobiRules as WideVariant<Chess8x8>>::ROLE_SPAN }>;
