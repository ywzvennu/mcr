//! Ordamirror (8x8) on the generic engine — a **symmetric** mirror match in
//! which **both** armies are Orda-style horde pieces (unlike Orda #214, where a
//! standard White army faced the Black Orda). Both sides field the same five
//! piece types and share the **flag-win** (campmate) terminal rule. Validated
//! square-for-square against Fairy-Stockfish `UCI_Variant ordamirror`.
//!
//! ## Armies (identical for both colours)
//!
//! The back rank is `Lancer Kheshig Archer Falcon King Archer Kheshig Lancer`.
//! Four of the five distinct pieces are exactly the Orda cavalry, reused verbatim
//! ([`OrdaRules`](super::orda::OrdaRules) defines the same movements):
//!
//! * **Lancer** ([`WideRole::Lancer`], FSF `kniroo` `l`, mcr `f`) — moves like a
//!   knight to an empty square; captures like a **rook**.
//! * **Kheshig** ([`WideRole::Kheshig`], FSF `centaur` `h`, mcr `w`) — a **King +
//!   Knight** leaper (sixteen squares); moves and captures alike.
//! * **Archer** ([`WideRole::Archer`], FSF `knibis` `a`, mcr `y`) — moves like a
//!   knight to an empty square; captures like a **bishop**.
//! * **Falcon** ([`WideRole::Falcon`], FSF `customPiece1 = f:mQcN` `f`, mcr
//!   overflow `*f`) — the **inverse** of the Lancer / Archer: it **moves like a
//!   queen** (any distance along a rank, file, or diagonal, to an empty square)
//!   but **captures like a knight** (a 2-1 leap). Its quiet queen slides are
//!   non-capturing; its only attacking / checking squares are the knight jumps.
//!   This is the one genuinely-new piece (added via the overflow-letter scheme).
//! * **King** ([`WideRole::King`], `k`) — a standard king (one each).
//! * **Pawns** ([`WideRole::Pawn`], `p`) — standard pawns, single-step only (see
//!   the start FEN below).
//!
//! ## Promotion
//!
//! A pawn of **either** colour reaching the last rank promotes to a **Lancer**,
//! **Kheshig**, **Archer**, or **Falcon** (FSF `promotionPieceTypes = lhaf`) —
//! never to a Queen/Rook/Bishop/Knight. The horde leaper pieces themselves never
//! promote.
//!
//! ## Flag win (campmate)
//!
//! White wins the instant its king reaches the **last rank**; Black wins the
//! instant its king reaches the **first rank** (FSF `flagPiece = k`,
//! `flagRegionWhite = *8`, `flagRegionBlack = *1`) — exactly the Orda rule, and
//! the generic [`WideVariant::has_flag_win`] / [`WideVariant::flag_rank`]
//! defaults express it (the shared `flag_win_reached` termination on the losing
//! side's turn). This is what makes mcr's perft match FSF's at a flag node.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant ordamirror`, `position startpos`) renders the start as
//!
//! ```text
//! lhafkahl/8/pppppppp/8/8/PPPPPPPP/8/LHAFKAHL w - - 0 1
//! ```
//!
//! with FSF's letters `l h a f k` (Lancer, Kheshig, Archer, Falcon, King). mcr
//! reuses `l`/`h`/`a` for its Lance/Hoplite/Hawk, so the horde pieces take the
//! distinct letters Lancer `f`, Kheshig `w`, Archer `y`, and the Falcon takes the
//! overflow token `*f` (its base letter `f` is the FSF mnemonic; the `*` prefix
//! keeps it distinct from the bare Lancer `f`):
//!
//! ```text
//! fwy*fkywf/8/pppppppp/8/8/PPPPPPPP/8/FWY*FKYWF w - - 0 1
//! ```
//!
//! Note the **symmetric asymmetry of the pawns**: both armies' pawns start one
//! rank advanced (White on the 3rd rank with the 2nd empty, Black on the 6th with
//! the 7th empty), so neither side is on its standard double-push rank — the
//! trait default gives each a single step only, with no en passant, matching FSF.
//! There is **no castling** (the back ranks are all horde pieces). The two FENs
//! are the same position; the `compare-fairy/` harness translates the letters
//! when driving FSF.

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Chess8x8, PromotionConfig, Square, StandardChess, WideRole,
    WideVariant,
};
use crate::Color;

/// The Ordamirror rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It defines the four shared horde movements — the Orda Lancer / Kheshig /
/// Archer (reused from [`OrdaRules`](super::orda::OrdaRules)) plus the new Falcon
/// — the `lhaf` promotion target set, and the flag-win terminal rule. There is no
/// castling and no double pawn step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct OrdamirrorRules;

/// The confirmed Ordamirror starting placement in mcr's role letters: both back
/// ranks `f w y *f k y w f` (Lancer, Kheshig, Archer, Falcon, King, Archer,
/// Kheshig, Lancer) with each side's pawns **one rank advanced** (White on the
/// 3rd rank, Black on the 6th; the 2nd and 7th ranks are empty) — the symmetric
/// horde layout, confirmed against FSF.
const ORDAMIRROR_START_PLACEMENT: &str = "fwy*fkywf/8/pppppppp/8/8/PPPPPPPP/8/FWY*FKYWF";

impl WideVariant<Chess8x8> for OrdamirrorRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 36;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(ORDAMIRROR_START_PLACEMENT)
            .expect("the Ordamirror starting placement is valid on an 8x8 board");
        // No castling: both back ranks are horde pieces with no rooks/king path.
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
        sq: Square<Chess8x8>,
        occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        match role {
            // Lancer: captures like a rook; knight jumps are quiet-only.
            WideRole::Lancer => attacks::rook_attacks::<Chess8x8>(sq, occupancy),
            // Archer: captures like a bishop; knight jumps are quiet-only.
            WideRole::Archer => attacks::bishop_attacks::<Chess8x8>(sq, occupancy),
            // Kheshig: King + Knight leaper — moves and captures alike.
            WideRole::Kheshig => {
                attacks::king_attacks::<Chess8x8>(sq) | attacks::knight_attacks::<Chess8x8>(sq)
            }
            // Falcon: captures like a knight (its only capturing / checking
            // squares). Its non-capturing queen slides are quiet-only (see
            // `quiet_only_targets`), so they are NOT in the attack set.
            WideRole::Falcon => attacks::knight_attacks::<Chess8x8>(sq),
            // Kings and pawns are standard.
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn quiet_only_targets(
        role: WideRole,
        _color: Color,
        sq: Square<Chess8x8>,
        occupancy: Bitboard<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        // The Lancer and Archer **move** like a knight (to empty squares) but
        // capture along a slider line (their `role_attacks`). The Falcon is the
        // inverse: it **moves** like a queen (its slides stop at any blocker and
        // are confined to empty squares) but captures like a knight. The generic
        // generator filters these by emptiness, emitting them as quiet moves only.
        match role {
            WideRole::Lancer | WideRole::Archer => attacks::knight_attacks::<Chess8x8>(sq),
            WideRole::Falcon => {
                attacks::rook_attacks::<Chess8x8>(sq, occupancy)
                    | attacks::bishop_attacks::<Chess8x8>(sq, occupancy)
            }
            _ => Bitboard::EMPTY,
        }
    }

    fn role_attacks_are_capture_only(role: WideRole) -> bool {
        // The Lancer (rook slide), Archer (bishop slide), and Falcon (knight leap)
        // reach their `role_attacks` squares only by capturing — they move by
        // their distinct `quiet_only_targets` pattern instead.
        matches!(role, WideRole::Lancer | WideRole::Archer | WideRole::Falcon)
    }

    fn role_is_slider(role: WideRole) -> bool {
        match role {
            // The Lancer (rook capture) and Archer (bishop capture) slide along
            // their *capture* lines, so they can pin and be pinned.
            WideRole::Lancer | WideRole::Archer => true,
            // The Kheshig and the Falcon are leapers for attack purposes: the
            // Kheshig is a pure King+Knight, and the Falcon *attacks* only by
            // knight leaps (its slides are non-capturing quiet moves), so neither
            // can pin nor act as a slider in the attack relation.
            WideRole::Kheshig | WideRole::Falcon => false,
            _ => <StandardChess as WideVariant<Chess8x8>>::role_is_slider(role),
        }
    }

    // --- promotion: pawns -> Lancer / Kheshig / Archer / Falcon (both colours) ---

    fn promotion_config() -> PromotionConfig {
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Lancer,
                WideRole::Kheshig,
                WideRole::Archer,
                WideRole::Falcon,
            ],
        }
    }

    // --- attacker-detection consistency -----------------------------------

    fn role_attack_is_directional(role: WideRole) -> bool {
        // Only the pawn is color-directional here: the Lancer / Archer capture
        // sets are plain rook / bishop slides, the Kheshig is a King+Knight, and
        // the Falcon's attack set is the symmetric knight pattern — all
        // geometrically symmetric, so `attackers_to` reverse-projects them with no
        // colour flip.
        matches!(role, WideRole::Pawn)
    }

    // --- flag win (campmate) ----------------------------------------------

    fn has_flag_win() -> bool {
        true
    }

    // The flag goal ranks (White's last rank, Black's first) are exactly the
    // generic `flag_rank` default, so Ordamirror does not override it.
}

/// Ordamirror as a [`GenericPosition`] over the 8x8 [`Chess8x8`] geometry.
///
/// Construct the starting position (the symmetric Orda-vs-Orda horde) with
/// [`Ordamirror::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Ordamirror::from_fen`](GenericPosition::from_fen). See the [module
/// docs](self) for the piece movements, the `lhaf` promotion, and the flag-win
/// rule.
pub type Ordamirror = GenericPosition<
    Chess8x8,
    OrdamirrorRules,
    { <OrdamirrorRules as WideVariant<Chess8x8>>::ROLE_SPAN },
>;
