//! Khan's Chess (8x8) on the generic engine — a standard White army against the
//! Black **Khan** army (an Orda-family asymmetric cavalry), plus the **flag-win**
//! (campmate) terminal rule. Validated against Fairy-Stockfish
//! `UCI_Variant khans`.
//!
//! ## Armies
//!
//! * **White = standard chess.** The six standard pieces with standard castling
//!   (the only side that castles) and the standard pawn (double step, en passant,
//!   promotion to Knight / Bishop / Rook / Queen). Every White movement is the
//!   trait default.
//! * **Black = Khan.** An Orda-family army whose pieces every **move like a
//!   knight** (or part of one) but **capture** differently — confirmed
//!   square-for-square against FSF:
//!   * **Lancer** ([`WideRole::Lancer`], FSF `kniroo` `l`, mcr `f`) — moves like a
//!     knight to an empty square; captures like a **rook**. (Shared with Orda.)
//!   * **Kheshig** ([`WideRole::Kheshig`], FSF `centaur` `h`, mcr `w`) — a **King +
//!     Knight** leaper (sixteen squares); moves and captures alike. (Shared with
//!     Orda.)
//!   * **Archer** ([`WideRole::Archer`], FSF `knibis` `a`, mcr `y`) — moves like a
//!     knight to an empty square; captures like a **bishop**. (Shared with Orda.)
//!   * **Khan** ([`WideRole::Khan`], FSF `t` = `mNcK`, mcr `=t`) — **moves** like a
//!     knight to an empty square but **captures** like a **king** (one step to any
//!     of the eight adjacent squares). The Khan replaces the Orda Yurt on the back
//!     rank (the d8 square) and is the soldier's promotion target.
//!   * **King** ([`WideRole::King`], `k`) — a standard king (one).
//!   * **Khan soldiers** ([`WideRole::KhanSoldier`], FSF `s` = `mfhNcfW`, mcr `=s`)
//!     — eight of them on the 7th rank. Each **moves** like a *forward* half-knight
//!     (the four knight leaps with a forward component, to an empty square) and
//!     **captures** one square straight forward (a forward Wazir step). It never
//!     double-steps and has no en passant.
//!
//! ## Promotion
//!
//! * **White pawns** promote to a Knight, Bishop, Rook or Queen on the last rank
//!   (the standard default; FSF inherits `chess`'s `promotionPieceTypes`).
//! * **Black Khan soldiers** promote to a **Khan** on the first rank (FSF
//!   `promotionPawnTypesBlack = s`, `promotionPieceTypesBlack = t`) — and the
//!   promotion is **forced** (a soldier on the last rank would otherwise be
//!   immobile), so the non-promoting alternative is never offered. The soldier is
//!   not a pawn, so it promotes through the engine's piece-promotion path
//!   ([`has_piece_promotion`](WideVariant::has_piece_promotion) +
//!   [`role_can_promote`](WideVariant::role_can_promote) +
//!   [`role_promoted_to`](WideVariant::role_promoted_to)), confined here to a single
//!   forced target by [`promotion_mandatory_in_zone`](WideVariant::promotion_mandatory_in_zone).
//!   The Khan and the other Khan pieces themselves never promote.
//!
//! ## Flag win (campmate)
//!
//! White wins the instant its king reaches the **last rank**; Black wins the
//! instant its king reaches the **first rank** (FSF `flagPiece = k`,
//! `flagRegionWhite = *8`, `flagRegionBlack = *1`) — identical to Orda. The win is
//! **purely positional** and FSF adjudicates it on the **losing** side's turn,
//! exactly as the [`WideVariant::has_flag_win`] / [`WideVariant::flag_rank`] hooks
//! express. Stalemate is a **loss** for the stalemated side (FSF
//! `stalemateValue = loss`).
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant khans`, `position startpos`) renders the start as
//!
//! ```text
//! lhatkahl/ssssssss/8/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1
//! ```
//!
//! with FSF's letters `l h a t k s` (Lancer, Kheshig, Archer, Khan, King, soldier).
//! mcr reuses `l`/`h`/`a` for its Lance/Hoplite/Hawk, so the shared Orda pieces take
//! the distinct letters Lancer `f`, Kheshig `w`, Archer `y`, and the two new Khan
//! pieces take overflow-3 tokens `=t` (Khan) / `=s` (soldier):
//!
//! ```text
//! fwy=tkywf/=s=s=s=s=s=s=s=s/8/8/8/8/PPPPPPPP/RNBQKBNR w KQ - 0 1
//! ```
//!
//! The two FENs are the same position; the `compare-fairy/` harness translates the
//! Khan letters when driving FSF. Only White has castling rights (`KQ`); Black's
//! back rank holds no rooks and never castles.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Chess8x8, Geometry, Square, StandardChess, WideRole, WideVariant,
};
use crate::Color;

/// The Khan's Chess rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides the Black Khan-army movements (Lancer / Kheshig / Archer / Khan /
/// Khan soldier), the soldier's forced promotion to a Khan, the forward-biased
/// attacker directions, the flag-win terminal rule, and stalemate-as-loss. White's
/// pieces, castling, pawns and pawn promotion are the trait defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct KhansRules;

/// The confirmed Khan's Chess starting placement in mcr's role letters: White
/// standard (`RNBQKBNR`/`PPPPPPPP` on ranks 1-2), Black `f w y =t k y w f` on the
/// back rank (Lancer, Kheshig, Archer, Khan, King, Archer, Kheshig, Lancer) with
/// eight Khan soldiers (`=s`) on the 7th rank.
const KHANS_START_PLACEMENT: &str = "fwy=tkywf/=s=s=s=s=s=s=s=s/8/8/8/8/PPPPPPPP/RNBQKBNR";

impl KhansRules {
    /// The Khan soldier's **quiet** move set: a *forward* half-knight — the four
    /// knight leaps with a forward component (two squares forward + one sideways,
    /// or one forward + two sideways). Color-directional (forward is toward the
    /// enemy back rank), and move-only (its captures are the straight-forward Wazir
    /// step in [`role_attacks`](WideVariant::role_attacks)).
    fn khan_soldier_quiet(color: Color, sq: Square<Chess8x8>) -> Bitboard<Chess8x8> {
        let f: i8 = if color.is_white() { 1 } else { -1 };
        let offsets: [(i8, i8); 4] = [(1, 2 * f), (-1, 2 * f), (2, f), (-2, f)];
        attacks::leaper_attacks::<Chess8x8>(sq, &offsets)
    }

    /// The Khan soldier's **capture** set: a single straight-forward Wazir step
    /// (toward the enemy back rank). Color-directional, so it must also be listed in
    /// [`role_attack_is_directional`](WideVariant::role_attack_is_directional).
    fn khan_soldier_attacks(color: Color, sq: Square<Chess8x8>) -> Bitboard<Chess8x8> {
        let f: i8 = if color.is_white() { 1 } else { -1 };
        let mut bb = Bitboard::EMPTY;
        if let Some(dest) = sq.offset(0, f) {
            bb.set(dest);
        }
        bb
    }
}

impl WideVariant<Chess8x8> for KhansRules {
    /// The tightest prefix of [`WideRole::ALL`] that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 72;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(KHANS_START_PLACEMENT)
            .expect("the Khan's Chess starting placement is valid on an 8x8 board");
        // Only White (the standard army) has castling rights; Black's Khan back
        // rank holds no rooks and never castles. The kingside rook sits on the last
        // file, the queenside rook on file 0.
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
            // Lancer: captures like a rook (its only capturing / checking squares);
            // its non-capturing knight jumps are quiet-only.
            WideRole::Lancer => attacks::rook_attacks::<Chess8x8>(sq, occupancy),
            // Archer: captures like a bishop; knight jumps are quiet-only.
            WideRole::Archer => attacks::bishop_attacks::<Chess8x8>(sq, occupancy),
            // Kheshig: King + Knight leaper — moves and captures alike.
            WideRole::Kheshig => {
                attacks::king_attacks::<Chess8x8>(sq) | attacks::knight_attacks::<Chess8x8>(sq)
            }
            // Khan: captures like a king (one step any direction); its
            // non-capturing knight jumps are quiet-only.
            WideRole::Khan => attacks::king_attacks::<Chess8x8>(sq),
            // Khan soldier: captures one square straight forward (color-directional);
            // its forward half-knight leaps are quiet-only.
            WideRole::KhanSoldier => Self::khan_soldier_attacks(color, sq),
            // White's army and the king are standard.
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn quiet_only_targets(
        role: WideRole,
        color: Color,
        sq: Square<Chess8x8>,
        _occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        // The Lancer / Archer / Khan **move** like a knight (to empty squares) but
        // never capture there — their capture sets (rook / bishop slide / king
        // step) live in `role_attacks`. The Khan soldier moves like a *forward*
        // half-knight. The generic generator filters these by emptiness, so they are
        // emitted only as quiet moves.
        match role {
            WideRole::Lancer | WideRole::Archer | WideRole::Khan => {
                attacks::knight_attacks::<Chess8x8>(sq)
            }
            WideRole::KhanSoldier => Self::khan_soldier_quiet(color, sq),
            _ => Bitboard::EMPTY,
        }
    }

    fn role_attacks_are_capture_only(role: WideRole) -> bool {
        // The Lancer (rook), Archer (bishop), Khan (king step) and Khan soldier
        // (forward Wazir) capture along / on their `role_attacks` squares but
        // **move** elsewhere (their `quiet_only_targets`), so those squares are
        // reachable only by capture.
        matches!(
            role,
            WideRole::Lancer | WideRole::Archer | WideRole::Khan | WideRole::KhanSoldier
        )
    }

    fn role_is_slider(role: WideRole) -> bool {
        match role {
            // The Lancer (rook capture) and Archer (bishop capture) slide along
            // their capture lines, so they can be pinned.
            WideRole::Lancer | WideRole::Archer => true,
            // The Kheshig, Khan and Khan soldier are pure leapers / steppers.
            WideRole::Kheshig | WideRole::Khan | WideRole::KhanSoldier => false,
            _ => <StandardChess as WideVariant<Chess8x8>>::role_is_slider(role),
        }
    }

    // --- promotion: Khan soldier -> Khan (forced), White pawns standard --------

    fn has_piece_promotion() -> bool {
        true
    }

    fn role_can_promote(role: WideRole) -> bool {
        // Only the Black Khan soldier promotes (to a Khan). White pawns promote via
        // the standard pawn path (`promotion_targets`), not this hook.
        role == WideRole::KhanSoldier
    }

    fn role_promoted_to(role: WideRole) -> WideRole {
        match role {
            WideRole::KhanSoldier => WideRole::Khan,
            other => other,
        }
    }

    fn promotion_mandatory_in_zone() -> bool {
        // A Khan soldier reaching the last rank **must** promote (it would otherwise
        // be immobile), so the non-promoting alternative is never offered. This hook
        // is consulted only for piece-promotion roles (the Khan soldier); White
        // pawns keep the standard `promotion_is_forced` path.
        true
    }

    // --- attacker-detection consistency ---------------------------------------

    fn role_attack_is_directional(role: WideRole) -> bool {
        // The Khan soldier's straight-forward Wazir capture is forward-biased, so a
        // piece of one colour attacking a square is found by reverse-projecting the
        // *opposite* colour's pattern (exactly as a pawn's diagonal capture is). The
        // standard Pawn keeps its default directional flag. The Khan captures like a
        // king (geometrically symmetric), and the Lancer / Archer capture sets are
        // plain rook / bishop slides — none of those is directional.
        matches!(role, WideRole::Pawn | WideRole::KhanSoldier)
    }

    // --- flag win (campmate) + stalemate-as-loss ------------------------------

    fn has_flag_win() -> bool {
        true
    }

    fn stalemate_is_loss() -> bool {
        true
    }

    // The flag goal ranks (White's last, Black's first) are the generic `flag_rank`
    // default, so Khan's Chess does not override it.
}

/// Khan's Chess as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position (standard White vs the Black Khan army) with
/// [`Khans::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Khans::from_fen`](GenericPosition::from_fen). See the [module docs](self) for
/// the piece movements, the soldier's forced promotion to a Khan, and the flag-win
/// rule.
pub type Khans = GenericPosition<Chess8x8, KhansRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::position::WideOutcome;

    /// Stalemate is scored as a **loss** for the side to move (FSF
    /// `stalemateValue = loss`, issue #498). The lone Black king on a8 (its far
    /// corner — rank 8 is not the rank-1 flag goal) has no legal move and is not in
    /// check, boxed by White's standard-army queen b6 and king c7. Black is
    /// stalemated, so Black loses and White wins. (Standard chess would call this a
    /// draw.)
    #[test]
    fn stalemate_is_a_loss() {
        let pos = Khans::from_fen("k7/2K5/1Q6/8/8/8/8/8 b - - 0 1").expect("valid khans FEN");
        assert!(pos.legal_moves().is_empty(), "Black has no legal move");
        assert!(!pos.is_check(), "Black is not in check — a true stalemate");
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::White
            })
        );
    }
}
