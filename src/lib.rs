//! # mce — Modular Chess Engine
//!
//! A permissively licensed (MIT OR Apache-2.0) chess move-generation and rules
//! library. The goal is full, perft-correct coverage of standard chess plus the
//! major variants (Chess960, Atomic, Antichess, Crazyhouse, King of the Hill,
//! Three-check, Racing Kings, Horde), with FEN/UCI/SAN support — an original,
//! clean-room implementation that carries no copyleft obligation.
//!
//! The public API is built up incrementally; see the repository's milestones and
//! issues for the current surface.
//!
//! This release provides the board-geometry primitives — [`Color`], [`Role`],
//! [`Piece`], [`File`], [`Rank`], [`Square`] — a [`Bitboard`] set type with the
//! usual bitwise operators and edge-masked directional shifts, and a [`Board`]
//! piece-placement type with FEN piece-field parsing and serialization.
#![doc(html_root_url = "https://docs.rs/mce")]

pub mod attacks;
mod bitboard;
mod board;
mod color;
mod file;
mod piece;
mod rank;
mod square;

pub use crate::attacks::{
    between, bishop_attacks, king_attacks, knight_attacks, line, pawn_attacks, queen_attacks,
    rook_attacks,
};
pub use crate::bitboard::{Bitboard, Squares};
pub use crate::board::{Board, ParseBoardError};
pub use crate::color::Color;
pub use crate::file::File;
pub use crate::piece::{Piece, Role};
pub use crate::rank::Rank;
pub use crate::square::{InvalidSquareIndex, ParseSquareError, Square};
