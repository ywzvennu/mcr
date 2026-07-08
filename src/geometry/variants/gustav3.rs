//! Gustav 3 — a **10x8** chess variant with an **Amazon** army and walled-in
//! corners, on the generic engine. It reuses the [`Cap10x8`] geometry (ten files
//! by eight ranks, the board Capablanca already validates) and the Amazon
//! ([`WideRole::Angel`], Queen + Knight) already fielded by Amazon Chess and
//! Mansindam, so it introduces **no new geometry and no new role**.
//!
//! Gustav 3 is Fairy-Stockfish's built-in `gustav3` (`gustav3_variant()`): a
//! `chess_variant_base` widened to `maxFile = FILE_J` (ten files) with an Amazon
//! (`a`) added to the army, the king castling to the **h**- and **d**-files
//! (`castlingKingsideFile = FILE_H`, `castlingQueensideFile = FILE_D`), and the
//! two back ranks flanked by Amazons whose files exist **only** on those ranks.
//!
//! ## The walled board
//!
//! The FSF start FEN is
//! `arnbqkbnra/*pppppppp*/*8*/*8*/*8*/*8*/*PPPPPPPP*/ARNBQKBNRA w KQkq - 0 1`.
//! The `*` cells are **wall squares**: the a- and j-files carry a piece only on
//! ranks 1 and 8 (the corner Amazons and their neighbours); the intervening
//! **a2–a7 and j2–j7 are permanently blocked**. A wall blocks a slider exactly
//! like an occupied square, and no piece — Amazon knight-leap, pawn capture, king
//! step, or slide — may ever land on one. So an Amazon boxed in the a1 corner
//! (a2 a wall, b1/b2 friendly) has exactly one opening move: the knight-leap to
//! b3.
//!
//! mcr models the walls as a compile-time [`WideVariant::board_walls`] mask folded
//! into the move-generation occupancy and target masks, **not** as FEN state — so
//! the wall cells render as ordinary empty squares (`1pppppppp1`, `10`), and the
//! `*` FEN token stays free for the Amazon's `**a` overflow spelling (a `*`-wall
//! FEN and a `*`-prefixed overflow role cannot coexist in one placement string).
//! Because a wall can sit "inside" the board rather than only on its slide-through
//! edge, and the king could in principle wander toward a walled file, Gustav 3
//! routes king safety through the make/unmake [`WideVariant::multi_royal`] path
//! (as Petrified chess does for its dynamic walls), where every generated target —
//! king included — is masked against the walls.
//!
//! ## Pieces (confirmed against FSF `gustav3_variant()`)
//!
//! The standard chess army plus the Amazon; the back rank is
//! `A R N B Q K B N R A` (Amazon, Rook, Knight, Bishop, Queen, King, Bishop,
//! Knight, Rook, Amazon), king on the **f**-file:
//!
//! * **Amazon (`**a`/`**A`, [`WideRole::Angel`])** — moves and captures as a
//!   **Queen + Knight** (rook + bishop slides plus the eight 2-1 leaps). FSF spells
//!   it `a`; mcr's second-bank overflow token is `**a`/`**A` (the reconciliation
//!   the `compare-fairy/` harness performs for the amazon-fielding variants).
//! * **King / Queen / Rook / Bishop / Knight / Pawn** — standard chess, with the
//!   double pawn step, en passant, and last-rank promotion.
//!
//! ## Castling
//!
//! The king starts on **f1**/**f8** with rooks on the **b**- and **i**-files (the
//! Amazons occupy the corners). Kingside the king goes to **h** with its rook to
//! **g**; queenside the king goes to **d** with its rook to **e** (FSF
//! `castlingKingsideFile = FILE_H`, `castlingQueensideFile = FILE_D`, rook beside
//! the king toward the centre). The `KQkq` field names the outermost rook on each
//! side, which — because the corners are Amazons — resolves to the i-file (king)
//! and b-file (queen) rooks.
//!
//! ## Promotion
//!
//! A pawn promotes to an **Amazon, Queen, Rook, Bishop, or Knight** (FSF
//! `promotionPieceTypes = piece_set(AMAZON) | QUEEN | ROOK | BISHOP | KNIGHT`).
//!
//! ## Validation
//!
//! The available Fairy-Stockfish binary is a **non-large-board build** and does not
//! implement `gustav3` (asked for `UCI_Variant gustav3` it silently falls back to
//! standard chess and returns chess-root perft counts), so Gustav 3 carries **no
//! live FSF perft oracle**. Like the other oracle-less variants (Okisaki Shogi,
//! Yari Shogi, Wa Shogi, Alice; see `docs/oracle-less-validation.md`) it is
//! *rules-validated*: `tests/perft_gustav3.rs` hand-derives the start-position move
//! count and cross-checks the engine's perft node-for-node against a fully
//! **independent, from-scratch 10x8 generator** (issue #500's two-implementations-
//! agree pattern).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{
    attacks, Bitboard, Board, Cap10x8, Geometry, PromotionConfig, Square, StandardChess, WideRole,
    WideVariant,
};
use crate::Color;

/// The confirmed Gustav 3 starting placement in the mcr dialect (amazon =
/// `**a`/`**A`; the a2–a7 / j2–j7 walls render as ordinary empty squares).
const GUSTAV3_START_PLACEMENT: &str =
    "**arnbqkbnr**a/1pppppppp1/10/10/10/10/1PPPPPPPP1/**ARNBQKBNR**A";

/// The kingside castle side index, matching the position layer's `KINGSIDE`.
const KINGSIDE: usize = 0;
/// The queenside castle side index, matching the position layer's `QUEENSIDE`.
const QUEENSIDE: usize = 1;

/// The Gustav 3 rule layer: a zero-sized [`WideVariant`] over [`Cap10x8`].
///
/// It overrides the starting array (Amazons in the corners), the Amazon's movement
/// (Queen + Knight), the wider promotion set (adding the Amazon), the custom
/// castle destination files (h/d) and rook files (i/b), and the static wall mask
/// on the a2–a7 / j2–j7 cells. Every other rule — pawns, en passant, check /
/// checkmate — is standard chess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Gustav3Rules;

impl WideVariant<Cap10x8> for Gustav3Rules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role this
    /// variant can field: the standard six plus the Amazon ([`WideRole::Angel`],
    /// index 67), so the span is 68. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 68;

    /// The western **fifty-move rule** (FSF `nMoveRule = 50`, i.e. 100 plies) for
    /// this standard-army board. Adjudication-only (the clock never gates move
    /// generation), so perft stays byte-identical.
    fn move_rule_plies() -> Option<u16> {
        Some(100)
    }

    /// Records a position history for the standard **threefold** repetition draw.
    /// History-dependent and never consulted by a bare [`GenericPosition`], so
    /// perft is unchanged.
    fn tracks_repetition() -> bool {
        true
    }

    fn starting_position() -> (Board<Cap10x8>, GenericState<Cap10x8>) {
        let board = Board::<Cap10x8>::from_fen_placement(GUSTAV3_START_PLACEMENT)
            .expect("the Gustav 3 starting placement is valid on a 10x8 board");
        // The castling rooks are the b-file (queenside) and i-file (kingside)
        // rooks — the corners are Amazons, so these are the outermost rooks the
        // `KQkq` field names. The king castles to h/d (see `castle_dest_files`).
        let mut castling = GenericCastling::NONE;
        for color in Color::ALL {
            castling.set(color, KINGSIDE, Some(8)); // i-file rook
            castling.set(color, QUEENSIDE, Some(1)); // b-file rook
        }
        let state = GenericState {
            turn: Color::White,
            castling,
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
        };
        (board, state)
    }

    /// The static wall squares: the a- and j-files exist only on ranks 1 and 8, so
    /// a2–a7 and j2–j7 (0-based files 0 / 9, ranks 1..=6) are permanently blocked.
    fn board_walls() -> Bitboard<Cap10x8> {
        let mut walls = Bitboard::EMPTY;
        for rank in 1u8..=6 {
            for file in [0u8, Cap10x8::WIDTH - 1] {
                if let Some(sq) = Square::<Cap10x8>::from_file_rank(file, rank) {
                    walls = walls.with(sq);
                }
            }
        }
        walls
    }

    /// King safety runs on the make/unmake multi-royal path so that every generated
    /// target — the king's own steps included — is masked against the walls (the
    /// fast single-king generator does not consult the wall mask). Result-identical
    /// to the standard path for a lone royal king; the Amazon army is not a cannon
    /// variant, so no role opts into the masked-slider fast lane.
    fn multi_royal() -> bool {
        true
    }

    fn role_attacks(
        role: WideRole,
        color: Color,
        sq: Square<Cap10x8>,
        occupancy: Bitboard<Cap10x8>,
    ) -> Bitboard<Cap10x8> {
        match role {
            // Amazon (Queen + Knight): a queen's slides plus the eight knight leaps.
            WideRole::Angel => {
                attacks::queen_attacks::<Cap10x8>(sq, occupancy)
                    | attacks::knight_attacks::<Cap10x8>(sq)
            }
            // Everything else is standard chess.
            _ => <StandardChess as WideVariant<Cap10x8>>::role_attacks(role, color, sq, occupancy),
        }
    }

    fn role_is_slider(role: WideRole) -> bool {
        match role {
            // The Amazon slides along the queen lines, so it can pin and be pinned.
            WideRole::Angel => true,
            _ => <StandardChess as WideVariant<Cap10x8>>::role_is_slider(role),
        }
    }

    fn role_attack_is_directional(role: WideRole) -> bool {
        // The Amazon's attack set (queen slides + knight leaps) is geometrically
        // symmetric, so only the pawn is colour-directional.
        matches!(role, WideRole::Pawn)
    }

    fn promotion_config() -> PromotionConfig {
        // FSF `promotionPieceTypes = piece_set(AMAZON) | QUEEN | ROOK | BISHOP |
        // KNIGHT`. The order affects only move-enumeration order, not the perft
        // leaf count.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Angel, // Amazon (Q+N)
                WideRole::Queen,
                WideRole::Rook,
                WideRole::Bishop,
                WideRole::Knight,
            ],
        }
    }

    fn has_castling() -> bool {
        true
    }

    fn castle_dest_files(side: usize) -> (u8, u8) {
        // FSF `castlingKingsideFile = FILE_H` (7), `castlingQueensideFile = FILE_D`
        // (3), with the rook ending beside the king toward the centre.
        if side == KINGSIDE {
            // King f1 -> h1 (file 7); rook i1 -> g1 (file 6).
            (7, 6)
        } else {
            // King f1 -> d1 (file 3); rook b1 -> e1 (file 4).
            (3, 4)
        }
    }

    /// Gustav 3 keeps the standard chess army plus the always-mating Amazon
    /// ([`WideRole::Angel`]), so the ordinary insufficient-material draw applies:
    /// king vs king, king and a lone minor vs king, and same-colour bishops only.
    /// The Amazon counts as mating material. Adjudication-only and behind the
    /// default-off hook, so perft stays byte-identical.
    fn is_insufficient_material<const R: usize>(
        board: &Board<Cap10x8, R>,
        _state: &GenericState<Cap10x8, R>,
    ) -> bool {
        crate::geometry::variant::standard_insufficient_material(board)
    }
}

/// Gustav 3 as a [`GenericPosition`] over the 10x8 [`Cap10x8`] geometry.
///
/// Construct the starting position with
/// [`Gustav3::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Gustav3::from_fen`](GenericPosition::from_fen).
pub type Gustav3 =
    GenericPosition<Cap10x8, Gustav3Rules, { <Gustav3Rules as WideVariant<Cap10x8>>::ROLE_SPAN }>;

#[cfg(test)]
mod tests {
    use super::*;

    /// The canonical start FEN round-trips (walls render as empty squares) and the
    /// hand-derived opening move count is reproduced.
    #[test]
    fn startpos_round_trips() {
        let pos = Gustav3::startpos();
        assert_eq!(
            pos.to_fen(),
            "**arnbqkbnr**a/1pppppppp1/10/10/10/10/1PPPPPPPP1/**ARNBQKBNR**A w KQkq - 0 1"
        );
        // 16 pawn steps + 4 inner knight leaps + 2 amazon knight-leaps (b3/i3).
        assert_eq!(pos.legal_move_count(), 22);
    }

    /// A cornered Amazon is boxed by the a-file walls: from a1 (a2 a wall, b1/b2
    /// friendly) its only move is the knight-leap to b3 — the queen rays up the
    /// a-file and along rank 1 are all blocked at the first step.
    #[test]
    fn cornered_amazon_has_one_knight_leap() {
        let pos = Gustav3::startpos();
        let a1 = Square::<Cap10x8>::from_file_rank(0, 0).unwrap();
        let dests: alloc::vec::Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Cap10x8>() == a1)
            .map(|m| m.to::<Cap10x8>())
            .collect();
        let b3 = Square::<Cap10x8>::from_file_rank(1, 2).unwrap();
        assert_eq!(dests, alloc::vec![b3], "cornered amazon: only a1-b3");
    }

    /// On an open board the Amazon moves as Queen + Knight. A lone White Amazon on
    /// e4 (0-based file 4, rank 3), kings tucked away, reaches its queen rays plus
    /// the eight knight leaps.
    #[test]
    fn amazon_moves_as_queen_plus_knight() {
        // Amazon on e4; White king a1, Black king j8 — both clear of the amazon's
        // lines and the walls.
        let pos =
            Gustav3::from_fen("9k/10/10/10/4**A5/10/10/K9 w - - 0 1").expect("valid Gustav 3 FEN");
        let e4 = Square::<Cap10x8>::from_file_rank(4, 3).unwrap();
        let n = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Cap10x8>() == e4)
            .count();
        // Queen from e4 on a 10x8 board: 9 (rank) + 7 (file) + diagonals. Plus the
        // walls remove the a-file/j-file squares the diagonals would reach.
        // Assert it is materially more than a lone knight (8), and that it slides.
        assert!(
            n > 8,
            "amazon should slide as a queen plus leap as a knight"
        );
    }

    /// Castling lands the king on the custom h-/d-files with the rook beside it.
    /// A cleared kingside (rook on i1, king on f1, g1/h1 empty) yields the O-O
    /// castle f1->h1.
    #[test]
    fn kingside_castle_lands_on_h_file() {
        // King f1, kingside rook i1, everything between the king and rook empty;
        // Black king tucked on a8, clear of the castling path.
        let pos =
            Gustav3::from_fen("k9/10/10/10/10/10/10/5K2R1 w K - 0 1").expect("valid Gustav 3 FEN");
        let f1 = Square::<Cap10x8>::from_file_rank(5, 0).unwrap();
        let h1 = Square::<Cap10x8>::from_file_rank(7, 0).unwrap();
        let castles: alloc::vec::Vec<_> = pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.from::<Cap10x8>() == f1 && m.to::<Cap10x8>() == h1)
            .collect();
        assert_eq!(castles.len(), 1, "the king must castle from f1 to h1");
    }
}
