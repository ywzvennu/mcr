//! # mce — Modular Chess Engine
//!
//! A permissively licensed (MIT OR Apache-2.0) chess move-generation and rules
//! library. It is an original, clean-room implementation built from public
//! algorithms and specifications, so it carries no copyleft obligation and is
//! safe to use in permissive and proprietary projects alike.
//!
//! The library is rules-and-move-generation only — there is no search or
//! evaluation, no GUI, and no network play.
//!
//! ## Coverage
//!
//! Standard chess and eight variants, each perft-verified:
//!
//! - **Chess960** (Fischer Random) — arbitrary back-rank shuffles and X-FEN castling.
//! - **King of the Hill** — win by marching a king to a central square.
//! - **Three-check** — win by delivering the third check.
//! - **Racing Kings** — win by racing a king to the eighth rank.
//! - **Atomic** — captures detonate, taking the adjacent non-pawns with them.
//! - **Antichess** (Giveaway) — captures are forced and shedding all pieces wins.
//! - **Horde** — White's pawn army against a full Black side.
//! - **Crazyhouse** — captured pieces flip sides and can be dropped back in.
//!
//! ## Public surface
//!
//! - **Geometry primitives** — [`Color`], [`Role`], [`Piece`], [`File`],
//!   [`Rank`], [`Square`], the [`Bitboard`] set type with bitwise operators and
//!   edge-masked shifts, and the [`Board`] piece-placement type.
//! - **Core position** — [`Position`] with legal [`Move`] generation, make-move,
//!   six-field FEN and UCI parsing/serialization, and the [`perft`] /
//!   [`perft_divide`] node counters.
//! - **SAN** — standard algebraic notation via [`Position::san`] and
//!   [`Position::parse_san`].
//! - **Zobrist hashing** — incrementally maintained [`Zobrist`] keys.
//! - **Outcomes and draws** — [`Outcome`], the precise [`EndReason`] labels,
//!   repetition tracking ([`count_repetitions`], [`is_repetition`]), and the
//!   move-validating [`Game`] driver.
//! - **Variants** — a generic [`VariantPosition`] over the [`Variant`] trait with
//!   one type per variant ([`Chess`], [`Chess960`], [`KingOfTheHill`],
//!   [`ThreeCheck`], [`RacingKings`], [`Horde`], [`Atomic`], [`Antichess`],
//!   [`Crazyhouse`]), plus the [`AnyVariant`] runtime dispatch enum and its
//!   [`VariantId`] selector for choosing a variant from a string or value.
//!
//! ## Quick start
//!
//! Parse a FEN, generate legal moves, play one, and read the outcome:
//!
//! ```
//! use mce::{Color, Outcome, Position};
//!
//! // Fool's mate, one move from the end: Black plays Qh4#.
//! let pos = Position::from_fen(
//!     "rnbqkbnr/pppp1ppp/8/4p3/6P1/5P2/PPPPP2P/RNBQKBNR b KQkq g3 0 2",
//! )
//! .unwrap();
//! assert!(pos.outcome().is_none());
//!
//! let mate = pos.parse_uci("d8h4").unwrap();
//! assert_eq!(pos.san(&mate), "Qh4#");
//!
//! let after = pos.play(&mate);
//! assert_eq!(after.outcome(), Some(Outcome::Decisive { winner: Color::Black }));
//! ```
//!
//! Drive a variant chosen at runtime through [`AnyVariant`]:
//!
//! ```
//! use mce::{AnyVariant, VariantId};
//!
//! // Pick a variant from a name, then use the same move-gen / play surface.
//! let id: VariantId = "atomic".parse().unwrap();
//! let pos = AnyVariant::startpos(id);
//! assert_eq!(pos.variant_id(), VariantId::Atomic);
//! assert_eq!(pos.legal_moves().len(), 20);
//!
//! let e4 = pos.parse_uci("e2e4").unwrap();
//! let after = pos.play(&e4);
//! assert!(after.outcome().is_none());
//! ```
//!
//! ## `no_std`
//!
//! The crate is `#![no_std]` with the default `std` feature turned off
//! (`--no-default-features`); it then draws its owned containers (`Vec`,
//! `String`) from `alloc`, so an allocator is still required. The whole core —
//! the geometry primitives, the [`Board`]/[`Position`] types, legal move
//! generation (hyperbola-quintessence sliders), SAN/FEN/UCI/PGN/EPD
//! parsing and rendering, Zobrist hashing, outcomes, and all variants — builds
//! and runs without `std`, including on bare-metal and `wasm` targets.
//!
//! The default-on `std` feature only adds the parts that genuinely need the
//! standard library: the `std::error::Error` impls on the error types (their
//! [`core::fmt::Display`] impls are always present), the filesystem
//! `Book::open` loader (the in-memory [`Book::from_bytes`] reader is
//! `no_std`), and the runtime-built `magic` slider table. Accordingly the
//! `magic`, `book`, and `parallel` features imply `std`.
#![doc(html_root_url = "https://docs.rs/mce")]
// The crate is `no_std` by default; the on-by-default `std` feature opts back
// into the standard library (for `std::error::Error`, `std::fs` book loading,
// and the magic-table runtime init). The core geometry, rules, and move
// generation compile without `std`, drawing `Vec`/`String`/`format!` from
// `alloc`.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod attacks;
mod bitboard;
mod board;
pub mod book;
mod chess_move;
mod color;
mod epd;
mod file;
pub mod geometry;
#[cfg(feature = "magic")]
mod magic;
mod movelist;
mod outcome;
mod pgn;
mod piece;
mod position;
mod rank;
mod san;
#[cfg(feature = "serde")]
mod serde_impls;
mod square;
mod staged;
mod variant;
mod zobrist;

pub use crate::attacks::{
    between, bishop_attacks, king_attacks, knight_attacks, line, pawn_attacks, queen_attacks,
    rook_attacks,
};
pub use crate::bitboard::{Bitboard, Squares};
pub use crate::board::{Board, ParseBoardError};
pub use crate::book::{decode_move, polyglot_key, weighted_pick, Book, BookEntry};
pub use crate::chess_move::{Move, MoveKind};
pub use crate::color::Color;
pub use crate::epd::{Epd, EpdError};
pub use crate::file::File;
// The first fairy variant on the generic engine (Makruk / Thai chess). The
// `GenericPosition`-based generic layer lives under `geometry`; its concrete
// variants are surfaced at the crate root for convenience.
pub use crate::geometry::{Makruk, MakrukRules};
#[cfg(feature = "magic")]
pub use crate::magic::attack_table_len;
pub use crate::movelist::MoveList;
pub use crate::outcome::{count_repetitions, is_repetition, EndReason, Game, IllegalMove, Outcome};
pub use crate::pgn::{Pgn, PgnError, PgnMove, PgnResult};
pub use crate::piece::{Piece, Role};
#[cfg(feature = "parallel")]
pub use crate::position::perft_parallel;
pub use crate::position::{
    perft, perft_divide, CastleSide, CastlingRights, FenError, ParseUciError, Position, Undo,
};
pub use crate::rank::Rank;
pub use crate::san::SanError;
pub use crate::square::{InvalidSquareIndex, ParseSquareError, Square};
pub use crate::staged::MoveGenerator;
#[cfg(feature = "parallel")]
pub use crate::variant::perft_variant_parallel;
pub use crate::variant::{
    perft_variant, Antichess, AntichessRules, AnyVariant, Atomic, AtomicRules, CastleGeometry,
    CheckCounters, Chess, Chess960, Chess960Rules, ChessRules, Crazyhouse, CrazyhouseRules,
    CrazyhouseState, Horde, HordeRules, KingOfTheHill, KingOfTheHillRules, RacingKings,
    RacingKingsRules, ThreeCheck, ThreeCheckRules, UnknownVariant, Variant, VariantId,
    VariantMoveGenerator, VariantPosition, VariantState, VariantUndo,
};
pub use crate::zobrist::Zobrist;
