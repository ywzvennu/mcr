//! # mcr — a clean-room chess rules library
//!
//! A permissively licensed (MIT OR Apache-2.0) chess **move-generation and rules**
//! library. It is an original, clean-room implementation built from public
//! algorithms and specifications — it does not derive from any copyleft engine —
//! so it carries no copyleft obligation and is safe to use in permissive and
//! proprietary projects alike.
//!
//! ## What mcr is (and is not)
//!
//! mcr answers the *rules* questions about a position: what are the legal moves,
//! what does a move lead to, is the side to move in check, is the game over and
//! why, how many nodes does the tree hold ([`perft`]). It covers **standard
//! chess, Chess960, and 60+ fairy variants** (Shogi, Xiangqi, Makruk, Capablanca,
//! Chu Shogi, and more), each perft-verified against the reference engines
//! Fairy-Stockfish and HaChu.
//!
//! mcr is emphatically **not** an engine that *plays* chess:
//!
//! - **No search** — no alpha-beta, MCTS, or any tree search.
//! - **No evaluation** — no heuristics, piece-square tables, or scoring. The
//!   analysis helpers ([`Position::attackers_to`], mobility on the fairy side)
//!   are pure geometric *queries*, never value judgements.
//! - **No GUI and no network play.**
//!
//! It is the rules-and-movegen foundation such an engine (or a server, a trainer,
//! a puzzle generator, or a UCI adapter) would be built *on top of*.
//!
//! ## The two variant families
//!
//! Variants come in two parallel families that share the same feel — parse a FEN,
//! list legal moves, play, count perft — but differ in board geometry:
//!
//! 1. **Concrete 8×8** — standard chess and the eight classic 8×8 variants, on the
//!    frozen, proven `u64` [`Bitboard`] / [`Square`] path. One type per variant
//!    ([`Chess`], [`Chess960`], [`KingOfTheHill`], [`ThreeCheck`], [`RacingKings`],
//!    [`Horde`], [`Atomic`], [`Antichess`], [`Crazyhouse`]) over the generic
//!    [`VariantPosition`], plus the runtime-dispatch [`AnyVariant`] enum and its
//!    string-addressable [`VariantId`] selector.
//!
//! 2. **Generic-geometry fairy** — the 60+ wider or differently-shaped variants
//!    (Shogi 9×9, Xiangqi 9×10, Chu Shogi 12×12, Capablanca 10×8, tiny Dobutsu
//!    3×4, …), which need boards beyond 8×8. They live under the [`geometry`](mod@geometry)
//!    module on a *separate*, compile-time-parameterized `Bitboard<G>` /
//!    `Square<G>` hierarchy, with the runtime-dispatch [`AnyWideVariant`](geometry::AnyWideVariant)
//!    enum and its [`WideVariantId`](geometry::WideVariantId) selector.
//!
//! The concrete 8×8 path is frozen and never re-parametrised, so the wide layer
//! costs the standard game nothing; see the module docs on [`geometry`](mod@geometry) for the
//! design. A per-variant reference (boards, pieces, rules, sources) lives in
//! [`docs/variants.md`], and per-variant perft/node-rate figures in
//! [`docs/perf-variants.md`].
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
//!   move-validating [`ChessGame`] driver.
//! - **Unified game** — [`Game`], one variant-agnostic handle spanning **all**
//!   variants (both families) with a single play surface ([`GameMove`],
//!   [`GameOutcome`]); the production entry point for a bot, server, or front-end
//!   that plays any variant without naming its family.
//! - **Concrete variants** — the [`VariantPosition`] / [`Variant`] family and the
//!   [`AnyVariant`] / [`VariantId`] runtime dispatch described above.
//! - **Fairy variants** — the whole generic-geometry layer under [`geometry`](mod@geometry):
//!   the [`Geometry`](geometry::Geometry) trait, `GenericPosition`, the per-variant
//!   types, and [`AnyWideVariant`](geometry::AnyWideVariant) /
//!   [`WideVariantId`](geometry::WideVariantId).
//! - **Ataxx** — the [`ataxx`] module is a self-contained 7×7 stones game
//!   (clone / jump / flip), **not** a chess variant: it shares none of the
//!   chess core's geometry, pieces, or move generator and stands entirely
//!   apart from the types above.
//!
//! ## Quick start
//!
//! Parse a FEN, generate legal moves, play one, and read the outcome:
//!
//! ```
//! use mcr::{perft, Color, Outcome, Position};
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
//! // Node counting (perft) over the legal-move tree.
//! assert_eq!(perft(&Position::startpos(), 4), 197_281);
//!
//! let after = pos.play(&mate);
//! assert_eq!(after.outcome(), Some(Outcome::Decisive { winner: Color::Black }));
//! ```
//!
//! Drive a concrete 8×8 variant chosen at runtime through [`AnyVariant`]:
//!
//! ```
//! use mcr::{AnyVariant, VariantId};
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
//! Drive a fairy variant on a non-8×8 board through the parallel
//! [`AnyWideVariant`](geometry::AnyWideVariant) surface:
//!
//! ```
//! use mcr::geometry::{AnyWideVariant, WideVariantId};
//!
//! // Shogi is a 9×9 board; the surface mirrors `AnyVariant`.
//! let id: WideVariantId = "shogi".parse().unwrap();
//! let pos = AnyWideVariant::startpos(id);
//! assert_eq!(pos.variant_id(), WideVariantId::Shogi);
//! assert_eq!(pos.dimensions(), (9, 9));
//! assert!(!pos.legal_moves().is_empty());
//! ```
//!
//! ## Cargo features
//!
//! - **`std`** *(default)* — opts back into the standard library. With it off
//!   (`--no-default-features`) the crate is `#![no_std]` (see below).
//! - **`magic`** *(implies `std`)* — use magic-bitboard sliders instead of the
//!   default lean hyperbola-quintessence tables; faster on slider-heavy
//!   positions, at the cost of a runtime-built ~841 KiB table. The public API is
//!   unchanged either way.
//! - **`book`** *(implies `std`)* — the filesystem [`Book::open`] Polyglot loader;
//!   the in-memory [`Book::from_bytes`] reader needs no feature.
//! - **`parallel`** *(implies `std`)* — the [`perft_parallel`] /
//!   `perft_variant_parallel` node counters (rayon), byte-identical to the serial
//!   counters. With the feature off, rayon is absent from the dependency graph.
//! - **`serde`** — `Serialize` / `Deserialize` on the public value types
//!   (positions and boards serialize as their FEN strings). With the feature off,
//!   serde is absent from the dependency graph.
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
//!
//! ### WebAssembly (`wasm32-unknown-unknown`)
//!
//! The wasm-safe feature set is simply `--no-default-features` (no `std`, and
//! therefore none of the `std::fs` book loader, rayon `parallel` perft, or
//! `magic` table — the parts that would not link on `wasm32-unknown-unknown`).
//! `serde` is wasm-safe and may be added. The library compiles to wasm with:
//!
//! ```text
//! cargo build --lib --target wasm32-unknown-unknown --no-default-features
//! cargo build --lib --target wasm32-unknown-unknown --no-default-features --features serde
//! ```
//!
//! On wasm, load opening books through the in-memory
//! [`Book::from_bytes`] with caller-supplied bytes (the `book` file loader is
//! unavailable), and use the serial [`perft`] rather than `perft_parallel`. See
//! `tests/wasm_smoke.rs` for a functional smoke test of this surface.
//!
//! [`docs/variants.md`]: https://github.com/ywzvennu/mcr/blob/main/docs/variants.md
//! [`docs/perf-variants.md`]: https://github.com/ywzvennu/mcr/blob/main/docs/perf-variants.md
#![doc(html_root_url = "https://docs.rs/mcr")]
// The crate is `no_std` by default; the on-by-default `std` feature opts back
// into the standard library (for `std::error::Error`, `std::fs` book loading,
// and the magic-table runtime init). The core geometry, rules, and move
// generation compile without `std`, drawing `Vec`/`String`/`format!` from
// `alloc`.
#![cfg_attr(not(feature = "std"), no_std)]
#![deny(missing_docs)]

extern crate alloc;

pub mod ataxx;
pub mod attacks;
mod bitboard;
mod board;
pub mod book;
mod catalog;
mod chess_move;
mod color;
mod epd;
mod file;
mod game;
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
pub use crate::catalog::VariantRef;
pub use crate::chess_move::{Move, MoveKind};
pub use crate::color::Color;
pub use crate::epd::{Epd, EpdError};
pub use crate::file::File;
pub use crate::game::{Game, GameFenError, GameMove, GameOutcome};
pub use crate::geometry::PlayerView;
// The first fairy variant on the generic engine (Makruk / Thai chess). The
// `GenericPosition`-based generic layer lives under `geometry`; its concrete
// variants are surfaced at the crate root for convenience.
pub use crate::geometry::{Makruk, MakrukRules};
#[cfg(feature = "magic")]
pub use crate::magic::attack_table_len;
pub use crate::movelist::MoveList;
pub use crate::outcome::{
    count_repetitions, is_repetition, ChessGame, EndReason, IllegalMove, Outcome,
};
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
