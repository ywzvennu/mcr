//! Courier chess (12x8) — the medieval German widening of chess, on the generic
//! engine. Validated node-for-node against Fairy-Stockfish (`UCI_Variant
//! courier`, a built-in built `largeboards=yes`), a 12-files by 8-ranks variant
//! on the [`Courier12x8`] geometry.
//!
//! Courier chess is played on a twelve-files by eight-ranks board (files a..l).
//! It keeps the modern Rook, Knight, Bishop, and (royal) King, and adds the
//! short-range medieval pieces confirmed **empirically** against Fairy-Stockfish
//! (`go perft` / divide on isolated pieces — the FSF piece letters are `R N E B
//! M K W F`):
//!
//! * **Rook** (`R`) — a standard rook. ([`WideRole::Rook`])
//! * **Knight** (`N`) — a standard knight. ([`WideRole::Knight`])
//! * **Courier** (`E`, [`WideRole::Alfil`]) — despite its name, FSF's Courier
//!   moves as the Shatranj **Alfil** (elephant): it leaps **exactly two squares
//!   diagonally** (`(±2, ±2)`), jumping over any intervening piece, with no
//!   one-step move. Confirmed by divide: from f4 it reaches only d2/h2/d6/h6.
//! * **Bishop** (`B`, [`WideRole::Bishop`]) — the **modern** sliding bishop
//!   (long diagonals). Confirmed by divide (14 moves from f4, blocked by pieces),
//!   distinct from the two-square-leaping Courier `E`.
//! * **Man / Mann** (`M`, [`WideRole::Commoner`]) — a **non-royal** piece that
//!   moves and captures exactly like a king (the eight one-step directions).
//!   Confirmed by divide (all eight neighbours of f4).
//! * **King** (`K`) — a standard **royal** king (no castling in Courier).
//! * **Wazir** (`W`, [`WideRole::Wazir`]) — one step **orthogonally** (`(±1, 0)` /
//!   `(0, ±1)`). Confirmed by divide (f3/e4/g4/f5 from f4).
//! * **Ferz** (`F`, [`WideRole::Met`]) — the counsellor: one step **diagonally**
//!   (`(±1, ±1)`), reusing the Makruk Met. Confirmed by divide (e3/g3/e5/g5).
//! * **Pawn** (`P`) — moves one square straight forward only (**no** double
//!   push, hence **no** en passant), captures one square diagonally forward, and
//!   **promotes to a Ferz** ([`WideRole::Met`]) — the only promotion choice — on
//!   reaching the far rank.
//!
//! There is **no castling** (the startpos rights field is `-`), and the initial
//! array is **non-standard**: the a-, g-, and l-file pawns start advanced (the
//! g-file's on rank 4/5), a Ferz sits advanced on g3/g6, and file g of the back
//! rank is empty (the King and the g-file Man/Wazir are split by it).
//!
//! ## Terminal rules (outcome only — perft-irrelevant move set)
//!
//! Like Shatranj, Courier decides games two ways the standard engine does not,
//! both confirmed against FSF (which applies them as *claims* that truncate `go
//! perft`, so matching FSF node counts in sparse endgames requires them):
//!
//! * **Baring the king** ([`WideVariant::has_bare_king_loss`]) — a side stripped
//!   of every piece but its king has **lost**, with FSF's single "bare-back"
//!   exception. Confirmed: `K+2` vs a bare king to move gives `go perft 1 = 0`
//!   (terminal), while `K+1` vs a bare king to move keeps its moves (bare-back).
//! * **Stalemate is a loss** ([`WideVariant::stalemate_is_loss`]) for the
//!   stalemated side. Confirmed: a stalemate node reports `score mate 0` in FSF.
//!
//! Neither rule changes the **legal move set** of a non-terminal position, so the
//! move generation is byte-identical to standard chess wherever the game has not
//! yet been decided.
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant courier` / `position startpos`.
//! mcr and FSF render the same position with different piece letters: FSF uses
//! `E` for the Alfil (Courier), `M` for the Man, `W` for the Wazir, and `F` for
//! the Ferz, but mcr reuses those letters (or their bare forms) for other roles,
//! so the Courier pieces take mcr's overflow / Met tokens — the Alfil `*x`, the
//! Man `*u`, the Wazir `*j`, and the Ferz the Makruk Met `m`. The `compare-fairy`
//! harness rewrites these (`*x → e`, `*u → m`, `*j → w`, `m → f`) when driving
//! FSF:
//!
//! ```text
//! FSF dialect: rnebmk1wbenr/1ppppp1pppp1/6f5/p5p4p/P5P4P/6F5/1PPPPP1PPPP1/RNEBMK1WBENR w - - 0 1
//! mcr dialect: rn*xb*uk1*jb*xnr/1ppppp1pppp1/6m5/p5p4p/P5P4P/6M5/1PPPPP1PPPP1/RN*XB*UK1*JB*XNR w - - 0 1
//! ```

use crate::geometry::attacks;
use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    Bitboard, Board, Courier12x8, Geometry, PromotionConfig, Square, StandardChess, WideRole,
    WideVariant,
};
use crate::Color;

/// The Courier rule layer: a zero-sized [`WideVariant`] over [`Courier12x8`].
///
/// It overrides only what Courier changes from standard chess — the Ferz (Met),
/// Alfil (Courier), Wazir, and Man (Commoner) movement, the non-standard starting
/// array, the pawn rules (single-step, promote to Ferz only), the absence of
/// castling, and the baring / stalemate-loss terminal rules. The Rook, Knight,
/// Bishop, King, and pawn capture/step are the trait default (standard chess), so
/// they need no override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CourierRules;

/// The confirmed Courier starting FEN placement, validated against
/// Fairy-Stockfish `UCI_Variant courier` (mcr dialect — see the [module
/// docs](self)).
const COURIER_START_PLACEMENT: &str =
    "rn*xb*uk1*jb*xnr/1ppppp1pppp1/6m5/p5p4p/P5P4P/6M5/1PPPPP1PPPP1/RN*XB*UK1*JB*XNR";

/// The four Ferz (diagonal one-step) offsets — the Ferz (Met) movement.
const FERZ_OFFSETS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// The four Alfil offsets — a pure two-square diagonal jump, over any intervening
/// piece (no one-step component). FSF's Courier (`E`) piece.
const ALFIL_OFFSETS: [(i8, i8); 4] = [(2, 2), (2, -2), (-2, 2), (-2, -2)];

/// The four Wazir (orthogonal one-step) offsets — the Wazir (`W`) movement.
const WAZIR_OFFSETS: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

impl WideVariant<Courier12x8> for CourierRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 58;

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

    fn starting_position() -> (Board<Courier12x8>, GenericState<Courier12x8>) {
        let board = Board::<Courier12x8>::from_fen_placement(COURIER_START_PLACEMENT)
            .expect("the Courier starting placement is valid on a 12x8 board");
        let state = GenericState {
            turn: Color::White,
            // Courier has no castling.
            castling: GenericCastling::NONE,
            ep_square: None,
            ep_captured: None,
            gating: GenericGating::NONE,
            duck: None,
            placement: GenericPlacement::NONE,
            halfmove_clock: 0,
            fullmove_number: 1,
            consecutive_passes: 0,
            board_b: Bitboard::EMPTY,
            petrified: Bitboard::EMPTY,
            checks_against: [0, 0],
            jieqi_seed: None,
        };
        (board, state)
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Courier12x8>,
        occupancy: Bitboard<Courier12x8>,
    ) -> Bitboard<Courier12x8> {
        match role {
            // Ferz (counsellor, `F`) = Met: the four diagonal one-steps.
            WideRole::Met => attacks::leaper_attacks::<Courier12x8>(sq, &FERZ_OFFSETS),
            // Courier (`E`) = Alfil (elephant): the four two-square diagonal jumps,
            // leaping over any intervening piece (no one-step Ferz move).
            WideRole::Alfil => attacks::leaper_attacks::<Courier12x8>(sq, &ALFIL_OFFSETS),
            // Wazir (`W`): the four orthogonal one-steps.
            WideRole::Wazir => attacks::leaper_attacks::<Courier12x8>(sq, &WAZIR_OFFSETS),
            // Man / Mann (`M`) = Commoner: a non-royal king (all eight one-steps).
            WideRole::Commoner => attacks::king_attacks::<Courier12x8>(sq),
            // Rook / Knight / Bishop / King and the pawn are standard chess: defer
            // to the trait default. `StandardChess` overrides no movement, so its
            // `role_attacks` *is* the trait default — keeping these pieces
            // byte-identical to standard chess.
            _ => <StandardChess as WideVariant<Courier12x8>>::role_attacks(
                role, color, sq, occupancy,
            ),
        }
    }

    fn promotion_config() -> PromotionConfig {
        // A pawn promotes only to a Ferz (Met); there is no choice of role.
        PromotionConfig {
            roles: alloc::vec![WideRole::Met],
        }
    }

    fn double_push_rank(_color: Color) -> u8 {
        // Courier pawns never make a double advance. Return a rank no pawn can
        // stand on (one past the last rank), so the generic pawn generator's
        // `from.rank() == double_push_rank` guard is never satisfied — there is no
        // double push and therefore no en-passant target is ever set.
        Courier12x8::HEIGHT
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
        // Stalemate is a loss for the stalemated side (FSF reports `score mate 0`
        // at a stalemate node); this affects only the reported outcome, not perft.
        true
    }

    fn confine_pins_to_segment() -> bool {
        // Courier's Alfil is a two-square diagonal *leaper*: pinned along a
        // diagonal, it could otherwise jump past its own king onto a collinear
        // square. Confine every pinned piece to the king–pinner segment so the
        // Alfil (and any other leaper) cannot escape the pin by leaping over the
        // king. For sliders this is equivalent to the full-line default.
        true
    }
}

/// Courier chess as a [`GenericPosition`] over the 12x8 [`Courier12x8`] geometry.
///
/// Construct the starting position with
/// [`Courier::startpos`](GenericPosition::startpos) or parse a FEN (mcr dialect)
/// with [`Courier::from_fen`](GenericPosition::from_fen). See the [module
/// docs](self) for the pieces, the no-castling / single-step-pawn rules, and the
/// baring / stalemate-loss terminal rules.
pub type Courier = GenericPosition<
    Courier12x8,
    CourierRules,
    { <CourierRules as WideVariant<Courier12x8>>::ROLE_SPAN },
>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::perft as gperft;
    use crate::geometry::position::WideOutcome;
    use crate::geometry::WidePiece;

    const STARTPOS: &str =
        "rn*xb*uk1*jb*xnr/1ppppp1pppp1/6m5/p5p4p/P5P4P/6M5/1PPPPP1PPPP1/RN*XB*UK1*JB*XNR w - - 0 1";

    /// The canonical start FEN round-trips through the mcr dialect.
    #[test]
    fn startpos_round_trips() {
        let pos = Courier::startpos();
        assert_eq!(pos.to_fen(), STARTPOS);
        assert_eq!(pos.turn(), Color::White);
        assert!(!pos.castling().has_any(Color::White));
        assert!(pos.ep_square().is_none());
    }

    /// The FSF-confirmed shallow startpos perft counts.
    #[test]
    fn startpos_perft_matches_fsf() {
        let pos = Courier::startpos();
        assert_eq!(gperft::<Courier12x8, _, _>(&pos, 1), 26);
        assert_eq!(gperft::<Courier12x8, _, _>(&pos, 2), 678);
        assert_eq!(gperft::<Courier12x8, _, _>(&pos, 3), 18406);
    }

    /// The back-rank array, a..l: R N E(Alfil) B M(Man) K _ W E... mirror.
    #[test]
    fn startpos_back_rank_array() {
        let pos = Courier::startpos();
        let board = pos.board();
        let expect: [(u8, Option<WideRole>); 12] = [
            (0, Some(WideRole::Rook)),
            (1, Some(WideRole::Knight)),
            (2, Some(WideRole::Alfil)),
            (3, Some(WideRole::Bishop)),
            (4, Some(WideRole::Commoner)),
            (5, Some(WideRole::King)),
            (6, None),
            (7, Some(WideRole::Wazir)),
            (8, Some(WideRole::Bishop)),
            (9, Some(WideRole::Alfil)),
            (10, Some(WideRole::Knight)),
            (11, Some(WideRole::Rook)),
        ];
        for (file, role) in expect {
            let sq = Square::<Courier12x8>::from_file_rank(file, 0).unwrap();
            assert_eq!(
                board.piece_at(sq).map(|p| p.role),
                role,
                "white back rank file {file}",
            );
            if let Some(r) = role {
                assert_eq!(board.piece_at(sq), Some(WidePiece::new(Color::White, r)));
            }
        }
        // The advanced Ferz sits on g3 (file 6, rank 2).
        let g3 = Square::<Courier12x8>::from_file_rank(6, 2).unwrap();
        assert_eq!(
            board.piece_at(g3),
            Some(WidePiece::new(Color::White, WideRole::Met)),
        );
    }

    /// The Courier (`E`) jumps exactly two squares diagonally, over any intervening
    /// piece, and has no one-step move — an Alfil, not the sliding Bishop.
    #[test]
    fn courier_is_a_two_step_diagonal_leaper() {
        // White Alfil on f4 (file 5, rank 3); both kings armed so no baring leaf.
        let pos =
            Courier::from_fen("6r4k/12/12/12/5*X6/12/12/K11 w - - 0 1").expect("valid Courier FEN");
        let from = Square::<Courier12x8>::from_file_rank(5, 3).unwrap();
        let mut targets: Vec<(u8, u8)> = pos
            .legal_moves()
            .iter()
            .filter(|m| m.from::<Courier12x8>() == from)
            .map(|m| {
                let t = m.to::<Courier12x8>();
                (t.file(), t.rank())
            })
            .collect();
        targets.sort_unstable();
        // d2, d6, h2, h6.
        assert_eq!(targets, alloc::vec![(3, 1), (3, 5), (7, 1), (7, 5)]);
    }

    /// The Wazir steps one square orthogonally; the Man steps like a king.
    #[test]
    fn wazir_and_man_step_one() {
        let pos = Courier::from_fen("6r4k/12/12/12/5*J6/12/12/K11 w - - 0 1").expect("valid FEN");
        let from = Square::<Courier12x8>::from_file_rank(5, 3).unwrap();
        let wazir = pos
            .legal_moves()
            .iter()
            .filter(|m| m.from::<Courier12x8>() == from)
            .count();
        assert_eq!(wazir, 4, "Wazir: four orthogonal steps");

        let pos = Courier::from_fen("6r4k/12/12/12/5*U6/12/12/K11 w - - 0 1").expect("valid FEN");
        let man = pos
            .legal_moves()
            .iter()
            .filter(|m| m.from::<Courier12x8>() == from)
            .count();
        assert_eq!(man, 8, "Man: eight king steps");
    }

    /// A pawn promotes only to a Ferz (Met) on the last rank.
    #[test]
    fn pawn_promotes_only_to_ferz() {
        // White pawn on b7 promotes on b8; a black rook keeps Black off a bare king.
        let pos = Courier::from_fen("3r7k/1P10/12/12/12/12/12/K11 w - - 0 1").expect("valid FEN");
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

    /// Pawns never make a double push (so no en-passant target is ever set).
    #[test]
    fn pawns_single_step_only() {
        use crate::geometry::WideMoveKind;
        let pos = Courier::startpos();
        let two_up = pos
            .legal_moves()
            .iter()
            .any(|m| matches!(m.kind(), WideMoveKind::DoublePawnPush));
        assert!(!two_up, "no pawn advances two squares");
    }

    /// A bared king is a loss, with FSF's bare-back exception when the opponent
    /// holds only two pieces (matching Shatranj's terminal rule).
    #[test]
    fn baring_the_king() {
        // Black lone king; White K + two Men (3 pieces): Black to move is terminal.
        let lost = Courier::from_fen("4k7/12/12/12/12/12/3*U*U7/4K7 b - - 0 1").expect("valid FEN");
        assert!(lost.legal_moves().is_empty(), "bared side has no moves");
        assert_eq!(
            lost.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::White
            })
        );

        // Bare-back reply: opponent has only two pieces (K + one Man), so the bared
        // side keeps its moves and the node is not terminal.
        let alive = Courier::from_fen("4k7/12/12/12/12/12/4*U7/4K7 b - - 0 1").expect("valid FEN");
        assert!(!alive.legal_moves().is_empty(), "bare-back keeps moves");
        assert_eq!(alive.outcome(), None);
    }

    /// Stalemate is a loss for the stalemated side.
    #[test]
    fn stalemate_is_a_loss() {
        // Black king a8 stalemated by white Ferz b6 and king c7.
        let pos = Courier::from_fen("k11/2K9/1M10/12/12/12/12/12 b - - 0 1").expect("valid FEN");
        assert!(pos.legal_moves().is_empty());
        assert!(!pos.is_check());
        assert_eq!(
            pos.outcome(),
            Some(WideOutcome::Decisive {
                winner: Color::White
            })
        );
    }
}
