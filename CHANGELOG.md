# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

**Status: 0.x — pre-1.0.** Under SemVer's 0.x rules a minor bump (`0.y`) may
carry breaking changes; patch bumps (`0.y.z`) stay API-compatible. The public
API is still maturing toward 1.0.

## [Unreleased]

- Further engine milestones (M11–M13) are in progress and will land in later
  releases; this section collects changes since 0.3.0.

## [0.3.0] — 2026-06-30

First crates.io-ready release. `mce` is a clean-room chess **move-generation and
rules library** at Fairy-Stockfish parity — there is no search or evaluation,
no GUI, and no network play. Highlights of the shipped surface:

### Fairy-variant geometry layer

- **47 fairy / pychess-class variants** on a parallel generic geometry layer
  (`mce::geometry`): `GenericPosition<G, V>` over a compile-time
  `Geometry`-parametrised `Bitboard<G>` / `Square<G>`, with a per-variant
  `WideVariant` rule layer. Boards span 3×4 up to 10×10 — Xiangqi, Shogi (and
  Mini/Kyoto/Tori/Dobutsu/Gorogoro lines), Makruk, Capablanca, Grand, Shako,
  Janggi, Seirawan, Spartan, Duck, Fog-of-War, and many more.
- Runtime dispatch through `AnyWideVariant` / `WideVariantId`, alongside the
  concrete-engine `AnyVariant` / `VariantId`.
- Move generation verified node-for-node against
  [Fairy-Stockfish](https://github.com/fairy-stockfish/Fairy-Stockfish) where an
  oracle exists, with the confirmed perft counts pinned in the test suite.
- `make`/`unmake` (undo) on the wide hot path, incremental Zobrist hashing, and
  a stable position key for the geometry layer.
- SAN / UCI / PGN notation for the fairy geometry layer.
- Draw, repetition, and adjudication rules wired through the wide layer.
- **Ataxx** — a self-contained 7×7 stones game (clone / jump / flip), exposed
  via the `ataxx` module; not a chess variant.

### Core & rules

- Board-geometry primitives: `Color`, `Role`, `Piece`, `File`, `Rank`,
  `Square`, the `Bitboard` set type, and the `Board` piece-placement type.
- A full standard-chess `Position` with legal move generation, in-place
  `play_unchecked` / immutable `play`, six-field FEN, and UCI.
- Perft node counters (`perft`, `perft_divide`), verified against published
  reference counts and an independent engine.
- Standard algebraic notation (SAN): `Position::san` / `parse_san`.
- Incremental `Zobrist` hashing.
- Game outcomes and draw detection: `Outcome`, precise `EndReason` labels,
  repetition tracking, and the move-validating `Game` driver.
- Eight perft-verified concrete-engine variants — Chess960, King of the Hill,
  Three-check, Racing Kings, Atomic, Antichess, Horde, and Crazyhouse — as
  `VariantPosition` types and through `AnyVariant` / `VariantId`, built on a
  `Variant` trait with hooks for terminal conditions, legality, captures,
  drops, FEN, and hashing.

### Formats & ecosystem

- Board-geometry primitives: `Color`, `Role`, `Piece`, `File`, `Rank`,
  `Square`, the `Bitboard` set type, and the `Board` piece-placement type.
- A full standard-chess `Position` with legal move generation, in-place
  `play_unchecked` / immutable `play`, six-field FEN, and UCI.
- Perft node counters (`perft`, `perft_divide`), verified against published
  reference counts and an independent engine.
- Standard algebraic notation (SAN): `Position::san` / `parse_san`.
- Incremental `Zobrist` hashing.
- Game outcomes and draw detection: `Outcome`, precise `EndReason` labels,
  repetition tracking, and the move-validating `Game` driver.

### Formats & ecosystem

- Polyglot (`.bin`) opening-book reading in the `book` module: the standard
  Polyglot Zobrist key (`polyglot_key`, separate from the internal `Zobrist`),
  a `Book` reader (`from_bytes`, and `open` behind the `book` feature) with
  binary-search `lookup`, Polyglot move decoding (`decode_move`, including the
  castling-as-king-takes-rook quirk and promotions), and a `weighted_pick`
  helper.
- EPD and PGN parsing/serialization.
- Optional `serde` support (behind the `serde` feature): the public value types
  gain `Serialize` / `Deserialize`, with `Position` / `Board` round-tripping as
  FEN and `AnyVariant` as a `{ variant, fen }` pair.
- WASM, Python, and C-FFI bindings (in the sibling `bindings/` crates, not part
  of the published library crate).

### Performance

- Stack-allocated `MoveList`, allocation-free perft, packed 16-bit `Move`,
  compact `Position`, fast-legality generators for every variant, perft bulk
  leaf-counting, and bulk bitboard-shift pawn/king-danger generation.
- Generic large-board engine (`GenericPosition<G, V>`) tuning: a stack-backed
  reusable move buffer with allocation-free perft, perft bulk leaf-counting on
  the standard single-king path (population counts in place of materialised move
  lists), a scan-free make-move board mutation, closed-form / fill-based slider
  line masks in place of per-square ray walks, and an inline pin set. The large
  board variants stay perft byte-identical while running materially faster — the
  10×8 Capablanca / 10×10 Grand / Shako hot paths.
- Optional `magic` cargo feature: magic-bitboard sliders (default build keeps
  the lean hyperbola tables). With `magic`, move generation outperforms the
  reference engine on every variant while the default build stays the leaner of
  the two.

### Tooling

- criterion benchmarks; cargo-fuzz targets (FEN, UCI, SAN, movegen); a
  comprehensive mce-vs-reference comparison harness (CPU, memory, and
  multi-hundred-position parity) and a Fairy-Stockfish differential harness,
  both kept in separate, non-published crates. The GPL-fenced comparison crates
  (`compare`, `compare-fairy`) and the `fuzz` targets are excluded from the
  published package.

[Unreleased]: https://github.com/ywzvennu/mce/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/ywzvennu/mce/releases/tag/v0.3.0
