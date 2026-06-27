//! Capablanca chess (10x8) on the generic engine — the first **larger-board**
//! variant on the [`WideVariant`] layer
//! (`docs/fairy-variants-architecture.md`, Phase 2). It validates the `u128`
//! geometry path ([`Cap10x8`]) end-to-end against Fairy-Stockfish.
//!
//! Capablanca chess is played on a ten-files by eight-ranks board (files a..j).
//! Beyond the standard chess army it adds two compound pieces:
//!
//! * **Archbishop** (Bishop + Knight) — mce's [`WideRole::Hawk`], whose default
//!   movement (`bishop | knight`) is already the archbishop's. FEN letter `a`.
//! * **Chancellor** (Rook + Knight) — mce's [`WideRole::Elephant`], whose
//!   default movement (`rook | knight`) is already the chancellor's. FEN letter
//!   `e` (mce's letter for the rook-knight compound; Fairy-Stockfish spells it
//!   `c`, a dialect difference the `compare-fairy/` harness reconciles).
//!
//! Every other rule is standard chess: pawns push one (or two from their second
//! rank), capture diagonally, take en passant, and promote on the last rank —
//! here to Queen, Rook, Bishop, Knight, **Archbishop, or Chancellor**. The king
//! and rooks castle, but on the Capablanca files (see below), which is the one
//! place the generic engine needed a geometry hook
//! ([`WideVariant::castle_dest_files`]).
//!
//! ## Confirmed starting FEN
//!
//! Pinned against Fairy-Stockfish's `UCI_Variant capablanca` (its
//! `capablanca_variant()` `startFen`, `castlingKingsideFile = FILE_I`,
//! `castlingQueensideFile = FILE_C`):
//!
//! ```text
//! FSF dialect: rnabqkbcnr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBCNR w KQkq - 0 1
//! mce dialect: rnabqkbenr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBENR w KQkq - 0 1
//! ```
//!
//! The two strings differ only in the chancellor's letter (`c` in FSF, `e` in
//! mce). Back rank, a-file to j-file: Rook, Knight, Archbishop, Bishop, Queen,
//! King, Bishop, Chancellor, Knight, Rook. The king stands on the f-file
//! (file 5); the castling rooks are the a-file (file 0) and j-file (file 9)
//! rooks.
//!
//! ## Castling geometry
//!
//! On castling the king lands two files from its start and the rook ends beside
//! it, exactly as Fairy-Stockfish does (`king = castling{kingside,queenside}File`,
//! `rook = king + (kingside ? west : east)`):
//!
//! * **Kingside**: king f1 -> **i1** (file 8), rook j1 -> **h1** (file 7).
//! * **Queenside**: king f1 -> **c1** (file 2), rook a1 -> **d1** (file 3).

use crate::geometry::position::{GenericCastling, GenericPosition, GenericState};
use crate::geometry::{Board, Cap10x8, PromotionConfig, WideRole, WideVariant};
use crate::Color;

/// The Capablanca rule layer: a zero-sized [`WideVariant`] over [`Cap10x8`].
///
/// It overrides only what Capablanca changes from the standard generic engine:
/// the 10x8 starting array, the wider promotion set (adding Archbishop and
/// Chancellor), and the castle destination files. The Archbishop ([`WideRole::Hawk`])
/// and Chancellor ([`WideRole::Elephant`]) movement is already the trait default,
/// so no `role_attacks` override is needed; pawns, knights, sliders, and the
/// king are standard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CapablancaRules;

/// The confirmed Capablanca starting FEN placement in the mce dialect
/// (chancellor = `e`/`E`), byte-for-byte equivalent to Fairy-Stockfish's
/// `rnabqkbcnr/.../RNABQKBCNR` modulo the chancellor's letter.
const CAPABLANCA_START_PLACEMENT: &str = "rnabqkbenr/pppppppppp/10/10/10/10/PPPPPPPPPP/RNABQKBENR";

/// The kingside castle side index, matching the position layer's `KINGSIDE`.
const KINGSIDE: usize = 0;

impl WideVariant<Cap10x8> for CapablancaRules {
    fn starting_position() -> (Board<Cap10x8>, GenericState<Cap10x8>) {
        let board = Board::<Cap10x8>::from_fen_placement(CAPABLANCA_START_PLACEMENT)
            .expect("the Capablanca starting placement is valid on a 10x8 board");
        let state = GenericState {
            turn: Color::White,
            // Both colors castle with both rooks (a/j files); the generic
            // castling layer reads the rook files from the back rank, so the
            // standard `KQkq` field maps to the a-file (queenside) and j-file
            // (kingside) rooks on this 10-wide board.
            castling: GenericCastling::standard::<Cap10x8>(),
            ep_square: None,
            halfmove_clock: 0,
            fullmove_number: 1,
        };
        (board, state)
    }

    fn promotion_config() -> PromotionConfig {
        // A Capablanca pawn promotes to any of the six non-pawn, non-king roles
        // of the army: the four standard plus the two compounds. The order
        // matches Fairy-Stockfish's promotion set (knight, bishop, rook, queen,
        // archbishop, chancellor); the order only affects move enumeration order,
        // not the perft leaf count.
        PromotionConfig {
            roles: alloc::vec![
                WideRole::Knight,
                WideRole::Bishop,
                WideRole::Rook,
                WideRole::Queen,
                WideRole::Hawk,     // Archbishop (B+N)
                WideRole::Elephant, // Chancellor (R+N)
            ],
        }
    }

    fn castle_dest_files(side: usize) -> (u8, u8) {
        // Capablanca castling, matching Fairy-Stockfish's
        // castlingKingsideFile = FILE_I (8) / castlingQueensideFile = FILE_C (2),
        // with the rook ending beside the king toward the centre.
        if side == KINGSIDE {
            // King f1 -> i1 (file 8); rook j1 -> h1 (file 7).
            (8, 7)
        } else {
            // King f1 -> c1 (file 2); rook a1 -> d1 (file 3).
            (2, 3)
        }
    }
}

/// Capablanca chess as a [`GenericPosition`] over the 10x8 [`Cap10x8`] geometry.
///
/// Construct the starting position with
/// [`Capablanca::startpos`](GenericPosition::startpos) or parse a FEN with
/// [`Capablanca::from_fen`](GenericPosition::from_fen). The Archbishop and
/// Chancellor reuse the [`StandardChess`](crate::geometry::StandardChess)
/// compound defaults, so only the array, promotion set, and castle files
/// distinguish it.
pub type Capablanca = GenericPosition<Cap10x8, CapablancaRules>;
