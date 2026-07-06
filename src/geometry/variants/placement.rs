//! Placement (Pre-Chess, 8x8) on the generic engine — standard chess preceded by
//! a **deployment phase** in which each side places its eight back-rank pieces
//! onto its own first rank, after which normal chess (movement, castling, en
//! passant, promotion) is played. Validated against Fairy-Stockfish
//! `UCI_Variant placement`.
//!
//! ## Pieces
//!
//! Every piece is a standard chess piece — King, Queen, two Rooks, two Bishops,
//! two Knights, eight Pawns per side — so Placement adds **no new role** and
//! reuses [`StandardChess`](crate::geometry::StandardChess)'s movement, promotion, en passant, and castling
//! wholesale. The only departures from standard chess are the opening deployment
//! and how castling rights are conferred by it.
//!
//! ## Deployment phase
//!
//! The pawns start on their standard ranks (white rank 1, black rank 6); the
//! **eight non-pawn pieces per side start off the board, in hand**. Players then
//! **alternate** placing one held piece per ply onto an empty square of their own
//! first rank (white rank 0, black rank 7) until both pockets are empty, after
//! which normal play begins. Confirmed against FSF, the constraints are:
//!
//! * **First rank only.** Every held piece — King, Queen, Rook, Bishop, Knight —
//!   drops onto an empty square of the player's own back rank.
//! * **Bishops on opposite colors.** Each side has one bishop per square color,
//!   so a bishop may only drop onto a color it has not already covered, and FSF
//!   keeps the deployment **completable**: a held bishop must always retain at
//!   least one empty first-rank square of its color, so a non-bishop may not
//!   occupy the *last* empty square of a color while a bishop of that color is
//!   still in hand.
//! * **No check filtering.** FSF applies none during deployment, so a drop is
//!   legal on any permitted empty square (a side has no king on the board until
//!   it deploys one).
//!
//! The phase is **per side**: a side that has emptied its pocket plays normally
//! even while the opponent is still deploying. The pocket rides in
//! [`GenericPlacement`], and the drop
//! path is gated behind [`WideVariant::has_placement`] (default-off), so every
//! other variant is byte-identical.
//!
//! ## Castling rights
//!
//! Placement grants standard castling, conferred by the deployment itself: a side
//! whose king is dropped on its **e-file** square gains the queenside right when a
//! rook sits on the a-file corner and the kingside right when a rook sits on the
//! h-file corner — assigned incrementally as those pieces reach their squares
//! (FSF renders e.g. `KQq` mid-deployment). A king deployed off the e-file, or a
//! corner left without a rook, yields no castling on that side. This rides the
//! [`WideVariant::placement_castling_king_file`] hook (default `None`, so every
//! non-Placement variant is byte-identical); after deployment the rights then
//! decay through normal king/rook moves like any standard game.
//!
//! ## Confirmed starting FEN
//!
//! FSF (`UCI_Variant placement`, `position startpos`) renders the start as
//!
//! ```text
//! 8/pppppppp/8/8/8/8/PPPPPPPP/8[KQRRBBNNkqrrbbnn] w - - 0 1
//! ```
//!
//! mcr uses the same board and `[..]` pocket but writes the pocket in role-index
//! order (Knights, Bishops, Rooks, Queen, King), so its canonical start FEN is
//!
//! ```text
//! 8/pppppppp/8/8/8/8/PPPPPPPP/8[NNBBRRQKnnbbrrqk] w - - 0 1
//! ```
//!
//! The two are the same position; the standard piece letters (`K Q R B N`) are
//! shared with FSF, so the `compare-fairy/` harness drives FSF with mcr's FEN
//! unchanged (FSF accepts the pocket bracket in any order).

use crate::geometry::position::{
    GenericCastling, GenericGating, GenericPlacement, GenericPosition, GenericState,
};
use crate::geometry::{Bitboard, Board, Chess8x8, Geometry, Square, WideRole, WideVariant};
use crate::Color;

/// The Placement (Pre-Chess) rule layer: a zero-sized [`WideVariant`] over
/// [`Chess8x8`].
///
/// It is standard chess (every trait default) plus the deployment-phase pocket,
/// the first-rank / opposite-color drop targets, and the deployment-conferred
/// castling rights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PlacementRules;

/// The Placement pawn layout: pawns on their standard ranks, both back ranks
/// empty (the non-pawn pieces start in hand).
const PLACEMENT_PAWNS: &str = "8/pppppppp/8/8/8/8/PPPPPPPP/8";

/// The file the king must occupy for its deployment to confer castling rights —
/// the standard e-file.
const KING_CASTLE_FILE: u8 = 4;

impl PlacementRules {
    /// The back-rank mask of `color` (white rank 0, black rank 7) — the only rank
    /// a held piece may be deployed onto.
    fn back_rank_mask(color: Color) -> Bitboard<Chess8x8> {
        let rank = match color {
            Color::White => 0,
            Color::Black => Chess8x8::HEIGHT - 1,
        };
        let mut bb = Bitboard::<Chess8x8>::EMPTY;
        for file in 0..Chess8x8::WIDTH {
            if let Some(sq) = Square::<Chess8x8>::from_file_rank(file, rank) {
                bb.set(sq);
            }
        }
        bb
    }

    /// The mask of all squares whose color matches `(file + rank)` parity `parity`
    /// (`0` = the a1 color, `1` = the b1 color).
    fn color_mask(parity: u8) -> Bitboard<Chess8x8> {
        let mut bb = Bitboard::<Chess8x8>::EMPTY;
        for rank in 0..Chess8x8::HEIGHT {
            for file in 0..Chess8x8::WIDTH {
                if (file + rank) % 2 == parity {
                    if let Some(sq) = Square::<Chess8x8>::from_file_rank(file, rank) {
                        bb.set(sq);
                    }
                }
            }
        }
        bb
    }
}

impl WideVariant<Chess8x8> for PlacementRules {
    /// The tightest prefix of `WideRole::ALL` that still contains every role
    /// this variant can field (start army, promotions, drops, gating, reveals);
    /// the movegen loops iterate only this far. See [`WideVariant::ROLE_SPAN`].
    const ROLE_SPAN: usize = 6;

    fn starting_position() -> (Board<Chess8x8>, GenericState<Chess8x8>) {
        let board = Board::<Chess8x8>::from_fen_placement(PLACEMENT_PAWNS)
            .expect("the Placement pawn layout is valid on an 8x8 board");
        let state = GenericState {
            turn: Color::White,
            // No piece is on the board yet, so no castling rights — they are
            // conferred by the deployment (see `placement_castling_king_file`).
            castling: GenericCastling::NONE,
            ep_square: None,
            ep_captured: None,
            gating: GenericGating::NONE,
            duck: None,
            placement: Self::initial_placement(),
            halfmove_clock: 0,
            fullmove_number: 1,
            consecutive_passes: 0,
            board_b: Bitboard::EMPTY,
        };
        (board, state)
    }

    // --- deployment phase -------------------------------------------------

    fn has_placement() -> bool {
        true
    }

    fn initial_placement() -> GenericPlacement {
        // Each side deploys, by hand: 2 Knights, 2 Bishops, 2 Rooks, 1 Queen,
        // 1 King — the standard back rank.
        let mut counts = [0u8; WideRole::COUNT];
        counts[WideRole::Knight.index()] = 2;
        counts[WideRole::Bishop.index()] = 2;
        counts[WideRole::Rook.index()] = 2;
        counts[WideRole::Queen.index()] = 1;
        counts[WideRole::King.index()] = 1;
        GenericPlacement::new(counts, counts)
    }

    fn placement_targets(
        role: WideRole,
        color: Color,
        board: &Board<Chess8x8>,
    ) -> Bitboard<Chess8x8> {
        // The empty first-rank squares, split by the two square colors (parities).
        let empty = Self::back_rank_mask(color) & !board.occupied();

        // Each side has exactly one bishop per color. A bishop already on the
        // board of a given parity means that color is taken; the side still
        // **needs** a square of every color it has not yet covered for the
        // bishops left in hand. FSF keeps the deployment completable: a held
        // bishop must always retain at least one empty square of its color, so a
        // non-bishop may not occupy the last such square, and a bishop may only go
        // on a still-needed color.
        let bishops = board.pieces(color, WideRole::Bishop);
        let mut mask = Bitboard::<Chess8x8>::EMPTY;
        for parity in 0u8..2 {
            let color_squares = empty & Self::color_mask(parity);
            let on_board = (bishops & Self::color_mask(parity)).count();
            // Held bishops still needing this color (one per color in total).
            let need = 1u32.saturating_sub(on_board);
            let allowed = if role == WideRole::Bishop {
                // A bishop may deploy onto a color only while that color is still
                // uncovered (no same-color bishop already down).
                need >= 1
            } else {
                // A non-bishop may take a square of this color only if doing so
                // still leaves enough squares for the held bishops of that color.
                color_squares.count() > need
            };
            if allowed {
                mask |= color_squares;
            }
        }
        mask
    }

    fn placement_castling_king_file() -> Option<u8> {
        // The king confers castling from its standard e-file square.
        Some(KING_CASTLE_FILE)
    }
}

/// Placement (Pre-Chess) as a [`GenericPosition`] over the 8x8 geometry.
///
/// Construct the starting position (the standard pawns plus both pockets in hand)
/// with [`Placement::startpos`](GenericPosition::startpos) or parse a FEN — the
/// placement may carry the deployment-phase pocket as a `[..]` holdings bracket —
/// with [`Placement::from_fen`](GenericPosition::from_fen). See the [module
/// docs](self) for the deployment phase and deployment-conferred castling.
pub type Placement = GenericPosition<Chess8x8, PlacementRules>;
