//! Chaturanga — the ancient Indian ancestor of chess, as Fairy-Stockfish models
//! it (`UCI_Variant chaturanga`, a built-in). It is **Shatranj with the
//! baring-the-king rule removed**: FSF defines it as `shatranj_variant()` with
//! only two rule overrides — the standard-chess starting array and
//! `extinctionValue = VALUE_NONE` (which disables the bare-king loss) — so every
//! other rule is Shatranj's.
//!
//! Its pieces are exactly Shatranj's (see [`ShatranjRules`] for the full
//! description): the standard Rukh (rook), Faras (knight), and Shah (king), the
//! **Farzin / Ferz** (counselor, [`WideRole::Met`]) stepping one square
//! diagonally, the **Pil / Alfil** ([`WideRole::Alfil`]) leaping exactly two
//! squares diagonally over any intervening piece, and the **Baidaq** (pawn) that
//! moves one square straight (no double push, hence no en passant) and promotes
//! **only to a Ferz**. There is **no castling**.
//!
//! ## The two differences from Shatranj
//!
//! * **Starting array.** Chaturanga uses the *standard* chess placement, with the
//!   King on the e-file and the Ferz beside it on the d-file — the left-right
//!   mirror of Shatranj's (King on d, Ferz on e). Because perft is invariant under
//!   a board reflection, the two variants have **identical startpos node counts at
//!   every depth**.
//! * **No baring loss** ([`WideVariant::has_bare_king_loss`] is `false`). Stripping
//!   the enemy down to a lone king is **not** a win in chaturanga (FSF
//!   `extinctionValue = VALUE_NONE`). Where Shatranj truncates a bared node to a
//!   terminal perft leaf (zero moves, a decisive result), chaturanga plays on and
//!   generates the ordinary moves — so the two variants' perft **diverges** in any
//!   position a baring claim would fire (a sparse endgame), even though it agrees
//!   everywhere a game is still contested.
//!
//! **Stalemate is still a loss** ([`WideVariant::stalemate_is_loss`] is `true`) for
//! the stalemated side, exactly as in Shatranj: FSF does not override shatranj's
//! `stalemateValue = -VALUE_MATE`, so chaturanga keeps it. This affects only the
//! reported outcome, not move generation or perft (a stalemated node already
//! generates zero moves).
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant chaturanga` / `position
//! startpos`. As with Shatranj, mcr renders the shared pieces with distinct
//! tokens — the Ferz reuses the Makruk Met `m`, and the Alfil takes the
//! `*`-prefixed overflow token `*x` (see the [Shatranj module docs](super::shatranj)
//! for why). The `compare-fairy` harness rewrites these (`*x → b`, `m → q`) when
//! driving FSF:
//!
//! ```text
//! FSF dialect: rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1
//! mcr dialect: rn*xmk*xnr/pppppppp/8/8/8/8/PPPPPPPP/RN*XMK*XNR w - - 0 1
//! ```
//!
//! The King sits on the e-file and the Ferz beside it on the d-file (both
//! colours), with the Alfils on the c- and f-files — the standard chess array.

use crate::geometry::position::{GenericCastling, GenericGating, GenericPlacement, GenericState};
use crate::geometry::{
    Bitboard, Board, Chess8x8, GenericPosition, PromotionConfig, ShatranjRules, Square, WideRole,
    WideVariant,
};
use crate::Color;

/// The Chaturanga rule layer: [`ShatranjRules`] with the baring-the-king loss
/// removed, a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// Every move-generation and terminal hook forwards to [`ShatranjRules`] except
/// the starting array (the standard-chess placement, the mirror of Shatranj's) and
/// [`has_bare_king_loss`](WideVariant::has_bare_king_loss), which chaturanga turns
/// **off**. The forwarding keeps chaturanga's pieces, promotion, single-step pawn,
/// no-castling, and stalemate-is-loss rules identical to Shatranj's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ChaturangaRules;

/// The confirmed Chaturanga starting FEN placement, validated against
/// Fairy-Stockfish `UCI_Variant chaturanga` (mcr dialect — see the [module
/// docs](self)). The standard chess array: King on e, Ferz (Met) on d.
const CHATURANGA_START_PLACEMENT: &str = "rn*xmk*xnr/pppppppp/8/8/8/8/PPPPPPPP/RN*XMK*XNR";

impl WideVariant<Chess8x8> for ChaturangaRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field — identical to Shatranj's army. See
    /// [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = ShatranjRules::ROLE_SPAN;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(CHATURANGA_START_PLACEMENT)
            .expect("the Chaturanga starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            // Chaturanga has no castling.
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
        // The pieces are Shatranj's (Ferz, Alfil, and the standard rest).
        <ShatranjRules as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy)
    }

    fn promotion_config() -> PromotionConfig {
        // A Baidaq promotes only to a Ferz (Met), exactly as in Shatranj.
        <ShatranjRules as WideVariant<Chess8x8>>::promotion_config()
    }

    fn double_push_rank(color: Color) -> u8 {
        // No double advance, as in Shatranj (so no en-passant target is ever set).
        <ShatranjRules as WideVariant<Chess8x8>>::double_push_rank(color)
    }

    fn has_castling() -> bool {
        <ShatranjRules as WideVariant<Chess8x8>>::has_castling()
    }

    /// The one rule chaturanga drops: baring the enemy king is **not** a win
    /// (FSF `extinctionValue = VALUE_NONE`). Left `false`, so a bared node is not a
    /// terminal leaf — chaturanga plays on where Shatranj would truncate.
    fn has_bare_king_loss() -> bool {
        false
    }

    fn stalemate_is_loss() -> bool {
        // Kept from Shatranj (FSF does not override `stalemateValue`): the
        // stalemated side has lost. Terminal-only, so perft is unaffected.
        <ShatranjRules as WideVariant<Chess8x8>>::stalemate_is_loss()
    }
}

/// Chaturanga (ancient Indian chess) as a [`GenericPosition`] over the 8x8
/// geometry.
///
/// Construct the starting position with
/// [`Chaturanga::startpos`](GenericPosition::startpos) or parse a FEN (mcr dialect)
/// with [`Chaturanga::from_fen`](GenericPosition::from_fen). It behaves exactly
/// like [`Shatranj`](super::Shatranj) except that it starts from the standard chess
/// array and baring the enemy king is not a win. See the [module docs](self).
pub type Chaturanga = GenericPosition<
    Chess8x8,
    ChaturangaRules,
    { <ChaturangaRules as WideVariant<Chess8x8>>::ROLE_SPAN },
>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::perft as gperft;
    use crate::geometry::position::WideOutcome;
    use crate::geometry::Shatranj;

    /// The canonical start FEN round-trips through the mcr dialect (King on e, Ferz
    /// on d — the standard array, the mirror of Shatranj's).
    #[test]
    fn startpos_round_trips() {
        let pos = Chaturanga::startpos();
        assert_eq!(
            pos.to_fen(),
            "rn*xmk*xnr/pppppppp/8/8/8/8/PPPPPPPP/RN*XMK*XNR w - - 0 1"
        );
    }

    /// The FSF-confirmed shallow startpos perft counts. They equal Shatranj's at
    /// every depth (chaturanga's array is Shatranj's left-right mirror, and perft is
    /// reflection-invariant): depth 1 is 8 single pawn pushes + 4 knight moves + 4
    /// Alfil jumps = 16.
    #[test]
    fn startpos_perft_matches_fsf() {
        let pos = Chaturanga::startpos();
        assert_eq!(gperft::<Chess8x8, _, _>(&pos, 1), 16);
        assert_eq!(gperft::<Chess8x8, _, _>(&pos, 2), 256);
        assert_eq!(gperft::<Chess8x8, _, _>(&pos, 3), 4176);
        // Same numbers Shatranj produces from its (mirrored) startpos.
        let shatranj = Shatranj::startpos();
        assert_eq!(
            gperft::<Chess8x8, _, _>(&pos, 3),
            gperft::<Chess8x8, _, _>(&shatranj, 3)
        );
    }

    /// The defining difference from Shatranj: baring the enemy king is **not** a
    /// win. The same bared position that Shatranj adjudicates as a baring win (a
    /// terminal, zero-move leaf) is an ongoing game in chaturanga — the side to move
    /// keeps its full move set and no `VariantWin` is reported.
    #[test]
    fn bared_king_is_not_a_loss() {
        // Black is reduced to a lone king; White keeps its king, an Alfil, and four
        // pawns. FSF chaturanga `go perft 1` here is 13 (4 pawn pushes + 4 Alfil
        // jumps + 5 king moves); FSF shatranj truncates it to 0.
        const FEN: &str = "4k3/8/8/2P1P3/3*X4/2P1P3/8/4K3 w - - 0 1";

        let chaturanga = Chaturanga::from_fen(FEN).expect("valid FEN");
        assert_eq!(
            chaturanga.legal_moves().len(),
            13,
            "chaturanga plays on from a bared position"
        );
        assert_eq!(
            chaturanga.outcome(),
            None,
            "baring the enemy king is not a decisive result in chaturanga"
        );

        // Contrast: Shatranj adjudicates the identical position as a baring win for
        // White, truncating it to a zero-move terminal leaf.
        let shatranj = Shatranj::from_fen(FEN).expect("valid FEN");
        assert!(shatranj.legal_moves().is_empty());
        assert_eq!(
            shatranj.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::White
            })
        );
    }

    /// Stalemate is a loss for the stalemated side — kept from Shatranj (the
    /// coverage-gate adjudication test for this variant).
    #[test]
    fn stalemate_is_a_loss() {
        // Black king a8 stalemated by the white ferz b6 and king c7: no legal move,
        // not in check. Black's lone king vs White's two pieces is the bare-back
        // case (never a baring loss even in Shatranj), so this is a clean stalemate.
        let pos = Chaturanga::from_fen("k7/2K5/1M6/8/8/8/8/8 b - - 0 1").expect("valid FEN");
        assert!(pos.legal_moves().is_empty());
        assert!(!pos.is_check());
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::White
            })
        );
    }

    /// A Baidaq promotes only to a Ferz (Met) on the last rank, as in Shatranj.
    #[test]
    fn pawn_promotes_only_to_ferz() {
        // White pawn on b7 promotes on b8; the only promotion role is the Ferz. A
        // black pawn keeps the board off a trivial endgame.
        let pos = Chaturanga::from_fen("4k3/1P5p/8/8/8/8/8/4K3 w - - 0 1").expect("valid FEN");
        let promos: Vec<_> = pos
            .legal_moves()
            .iter()
            .filter_map(|m| m.promotion())
            .collect();
        assert!(!promos.is_empty(), "the pawn can promote");
        assert!(
            promos.iter().all(|&r| r == WideRole::Met),
            "promotion is only to the Ferz (Met), got {promos:?}"
        );
    }
}
