//! Shatranj — the medieval Persian/Arabic ancestor of chess, on the generic
//! engine. Validated node-for-node against Fairy-Stockfish (`UCI_Variant
//! shatranj`, a built-in), an 8x8 variant ([`Chess8x8`] geometry).
//!
//! Shatranj keeps the standard Rook, Knight, and King, but replaces the Queen
//! and Bishop with their weak medieval forebears and strips the modern
//! conveniences (no double pawn push, no en passant, no castling). Its pieces
//! are:
//!
//! * **Rukh** (rook) — a standard rook. ([`WideRole::Rook`])
//! * **Asb / Faras** (knight) — a standard knight. ([`WideRole::Knight`])
//! * **Pil / Alfil** (elephant, [`WideRole::Alfil`]) — leaps **exactly two
//!   squares diagonally**, jumping over any intervening piece. A pure two-step
//!   diagonal leaper with no one-step move, it reaches only the four `(±2, ±2)`
//!   squares (eight squares of the whole board over a game) — far weaker than the
//!   modern bishop. Distinct from the Shako [`WideRole::FersAlfil`] (Ferz +
//!   Alfil), which *also* steps one square diagonally.
//! * **Farzin / Ferz** (counselor, [`WideRole::Met`]) — one step to any of the
//!   four diagonals, reusing the Makruk Met. The ancestor of the queen.
//! * **Shah** (king, [`WideRole::King`]) — a standard king (but no castling).
//! * **Baidaq** (pawn) — moves one square straight forward only (**no** double
//!   push, hence **no** en passant), captures one square diagonally forward, and
//!   **promotes to a Ferz** ([`WideRole::Met`]) — the only promotion choice — on
//!   reaching the far rank.
//!
//! There is **no castling**.
//!
//! ## Terminal rules (outcome only — perft-irrelevant move set)
//!
//! Shatranj decides games two ways the standard engine does not, and both are
//! modelled here because Fairy-Stockfish applies them as *claims* that truncate
//! `go perft` (so matching FSF node counts in sparse endgames requires them):
//!
//! * **Baring the king** ([`WideVariant::has_bare_king_loss`]): a side stripped
//!   of every piece but its king has **lost**, with the single "bare-back"
//!   exception FSF grants — see
//!   [`bare_king_loss_loser`](GenericPosition::bare_king_loss_loser). The bared
//!   node generates **zero** moves (a perft leaf), exactly as FSF's
//!   `extinctionValue = -VALUE_MATE` / `extinctionClaim` truncates it.
//! * **Stalemate is a loss** ([`WideVariant::stalemate_is_loss`]) for the
//!   stalemated side (FSF `stalemateValue = -VALUE_MATE`); this affects only the
//!   reported outcome, not move generation or perft (a stalemated node already
//!   generates zero moves).
//!
//! Neither rule changes the **legal move set** of a non-terminal position, so
//! the move generation is byte-identical to standard chess wherever the game has
//! not yet been decided.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant shatranj` / `position
//! startpos`. mcr and FSF render the same position with different piece letters:
//! FSF uses `b` for the Alfil and `q` for the Ferz, but mcr reuses `b`/`q` for
//! its Bishop/Queen, so the Shatranj pieces take distinct tokens — the Ferz reuses
//! the Makruk Met `m`, and the Alfil, landing past the exhausted single-letter
//! alphabet, takes the `*`-prefixed overflow token `*x` (the
//! [`OVERFLOW_PREFIX`](crate::geometry::OVERFLOW_PREFIX) plus the recycled letter
//! `x`, its case carrying the colour). The `compare-fairy` harness rewrites these
//! (`*x → b`, `m → q`) when driving FSF:
//!
//! ```text
//! FSF dialect: rnbkqbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBKQBNR w - - 0 1
//! mcr dialect: rn*xkm*xnr/pppppppp/8/8/8/8/PPPPPPPP/RN*XKM*XNR w - - 0 1
//! ```
//!
//! The king sits on file 3 and the Ferz beside it on file 4 (both colours), with
//! the Alfils on files 2 and 5 — the same array FSF and pychess use.

use crate::geometry::attacks::leaper_attacks;
use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    Bitboard, Board, Chess8x8, Geometry, PromotionConfig, Square, StandardChess, WideRole,
    WideVariant,
};
use crate::Color;

/// The Shatranj rule layer: a zero-sized [`WideVariant`] over [`Chess8x8`].
///
/// It overrides only what Shatranj changes from standard chess — the Ferz (Met)
/// and Alfil movement, the starting array, the pawn rules (single-step, promote
/// to Ferz only), the absence of castling, and the baring / stalemate-loss
/// terminal rules. Everything else is the trait default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ShatranjRules;

/// The confirmed Shatranj starting FEN placement, validated against
/// Fairy-Stockfish `UCI_Variant shatranj` (mcr dialect — see the [module
/// docs](self)).
const SHATRANJ_START_PLACEMENT: &str = "rn*xkm*xnr/pppppppp/8/8/8/8/PPPPPPPP/RN*XKM*XNR";

/// The four ferz (diagonal one-step) offsets — the Ferz (Met) movement.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// The four Alfil offsets — a pure two-square diagonal jump, over any
/// intervening piece (no one-step component).
const ALFIL_OFFSETS: [(i8, i8); 4] = [(2, 2), (2, -2), (-2, 2), (-2, -2)];

impl WideVariant<Chess8x8> for ShatranjRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 58;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(SHATRANJ_START_PLACEMENT)
            .expect("the Shatranj starting placement is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            // Shatranj has no castling.
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
            // Ferz (counselor) = Met: the four diagonal one-steps.
            WideRole::Met => leaper_attacks::<Chess8x8>(sq, &FERZ_OFFSETS),
            // Alfil (elephant): the four two-square diagonal jumps, leaping over
            // any intervening piece (no eye-block, no one-step Ferz move).
            WideRole::Alfil => leaper_attacks::<Chess8x8>(sq, &ALFIL_OFFSETS),
            // Rook / Knight / King and the pawn (Baidaq) are standard chess: defer
            // to the trait default. `StandardChess` overrides no movement, so its
            // `role_attacks` *is* the trait default — keeping these pieces
            // byte-identical to standard chess.
            _ => <StandardChess as WideVariant<Chess8x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn promotion_config() -> PromotionConfig {
        // A Baidaq promotes only to a Ferz (Met); there is no choice of role.
        PromotionConfig {
            roles: alloc::vec![WideRole::Met],
        }
    }

    fn double_push_rank(_color: Color) -> u8 {
        // The Baidaq never makes a double advance. Return a rank no pawn can stand
        // on (one past the last rank), so the generic pawn generator's
        // `from.rank() == double_push_rank` guard is never satisfied — there is no
        // double push and therefore no en-passant target is ever set.
        Chess8x8::HEIGHT
    }

    fn has_castling() -> bool {
        false
    }

    fn has_bare_king_loss() -> bool {
        // Baring the king decides the game: a side reduced to its lone king has
        // lost (with FSF's single bare-back reply). The terminal-node truncation
        // matches FSF's `extinctionValue = -VALUE_MATE` / `extinctionClaim`.
        true
    }

    fn stalemate_is_loss() -> bool {
        // Stalemate is a loss for the stalemated side (FSF `stalemateValue =
        // -VALUE_MATE`); this affects only the reported outcome, not perft.
        true
    }
}

/// Shatranj (medieval chess) as a [`GenericPosition`] over the 8x8 geometry.
///
/// Construct the starting position with
/// [`Shatranj::startpos`](GenericPosition::startpos) or parse a FEN (mcr dialect)
/// with [`Shatranj::from_fen`](GenericPosition::from_fen). See the [module
/// docs](self) for the pieces, the no-castling / single-step-pawn rules, and the
/// baring / stalemate-loss terminal rules.
pub type Shatranj = GenericPosition<Chess8x8, ShatranjRules>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::perft as gperft;
    use crate::geometry::position::WideOutcome;

    /// The canonical start FEN round-trips through the mcr dialect.
    #[test]
    fn startpos_round_trips() {
        let pos = Shatranj::startpos();
        assert_eq!(
            pos.to_fen(),
            "rn*xkm*xnr/pppppppp/8/8/8/8/PPPPPPPP/RN*XKM*XNR w - - 0 1"
        );
    }

    /// The FSF-confirmed shallow startpos perft counts (no double push: 8 single
    /// pawn pushes + 4 knight moves + 4 Alfil jumps = 16 at depth 1).
    #[test]
    fn startpos_perft_matches_fsf() {
        let pos = Shatranj::startpos();
        assert_eq!(gperft::<Chess8x8, _>(&pos, 1), 16);
        assert_eq!(gperft::<Chess8x8, _>(&pos, 2), 256);
        assert_eq!(gperft::<Chess8x8, _>(&pos, 3), 4176);
    }

    /// The Alfil jumps exactly two squares diagonally, over an intervening piece,
    /// and has no one-step move — distinct from a Ferz or a bishop.
    #[test]
    fn alfil_jumps_two_diagonally_only() {
        // White Alfil on d4 (index 27), with friendly pawns blocking c5/e5/c3/e3:
        // it still reaches b6, f6, b2, f2 (jumping) and nothing one step away.
        // A black pawn keeps Black off a bare king (else the node is a baring
        // leaf). FSF `go perft 1` lists d4b2/d4f2/d4b6/d4f6 on this position.
        let pos = Shatranj::from_fen("4k3/7p/8/2P1P3/3*X4/2P1P3/8/4K3 w - - 0 1")
            .expect("valid Shatranj FEN");
        let targets: Vec<_> = pos
            .legal_moves()
            .iter()
            .filter(|m| m.from::<Chess8x8>() == Square::<Chess8x8>::new(27))
            .map(|m| m.to::<Chess8x8>().index())
            .collect();
        // b6=41, f6=45, b2=9, f2=13.
        let mut got = targets.clone();
        got.sort_unstable();
        assert_eq!(got, alloc::vec![9, 13, 41, 45]);
    }

    /// A bared king is a loss: white (lone king) to move, black holds three
    /// pieces, so the node is terminal with zero moves and black has won.
    #[test]
    fn bared_king_loses() {
        // White: lone king e1. Black: king e8 + two ferz (1 piece vs 3).
        let pos = Shatranj::from_fen("4k3/8/8/8/8/8/3mm3/4K3 w - - 0 1").expect("valid FEN");
        assert!(pos.legal_moves().is_empty(), "bared side has no moves");
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::Black
            })
        );
    }

    /// The bare-back reply: the bared side keeps its move while the opponent has
    /// only two pieces (it might capture the lone enemy piece next move), so the
    /// node is **not** terminal.
    #[test]
    fn bare_back_reply_is_not_terminal() {
        // Black: lone king e8. White: king e1 + one ferz e2 (2 pieces). Black to
        // move keeps its five king moves (FSF gives 5 here, not 0).
        let pos = Shatranj::from_fen("4k3/8/8/8/8/8/4M3/4K3 b - - 0 1").expect("valid FEN");
        assert_eq!(pos.legal_moves().len(), 5);
        assert_eq!(pos.outcome(), None);
    }

    /// King-vs-King is not decided by baring (neither side's opponent has two
    /// pieces): play continues, as in FSF.
    #[test]
    fn king_vs_king_is_not_a_baring_loss() {
        let pos = Shatranj::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").expect("valid FEN");
        assert!(!pos.legal_moves().is_empty());
        assert_eq!(pos.outcome(), None);
    }

    /// Stalemate is a loss for the stalemated side.
    #[test]
    fn stalemate_is_a_loss() {
        // Black king a8 stalemated by white ferz b6 and king c7 (a Ferz-only mate
        // analogue): no legal move, not in check. FSF `go perft 1` here is 0.
        let pos = Shatranj::from_fen("k7/2K5/1M6/8/8/8/8/8 b - - 0 1").expect("valid FEN");
        assert!(pos.legal_moves().is_empty());
        assert!(!pos.is_check());
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::White
            })
        );
    }

    /// A Baidaq promotes only to a Ferz (Met) on the last rank.
    #[test]
    fn pawn_promotes_only_to_ferz() {
        // White pawn on b7 promotes on b8; the only promotion role is the Ferz. A
        // black pawn keeps Black off a bare king (else the node is a baring leaf).
        let pos = Shatranj::from_fen("4k3/1P5p/8/8/8/8/8/4K3 w - - 0 1").expect("valid FEN");
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
