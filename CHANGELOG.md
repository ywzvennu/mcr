# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

**Status: in active development — pre-release.** The crate is not yet versioned
or published; everything below is unreleased and the public API may change.

## [Unreleased]

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

### Formats & ecosystem

- Polyglot (`.bin`) opening-book reading in the `book` module: the standard
  Polyglot Zobrist key (`polyglot_key`, separate from the internal `Zobrist`),
  a `Book` reader (`from_bytes`, and `open` behind the `book` feature) with
  binary-search `lookup`, Polyglot move decoding (`decode_move`, including the
  castling-as-king-takes-rook quirk and promotions), and a `weighted_pick`
  helper.

### Variants

- Eight perft-verified variants — Chess960, King of the Hill, Three-check,
  Racing Kings, Atomic, Antichess, Horde, and Crazyhouse — exposed as concrete
  `VariantPosition` types and through the `AnyVariant` runtime-dispatch enum
  with its `VariantId` selector. Built on a `Variant` trait with hooks for
  terminal conditions, legality, captures, drops, FEN, and hashing.

### Performance

- Stack-allocated `MoveList`, allocation-free perft, packed 16-bit `Move`,
  compact `Position`, fast-legality generators for every variant, perft bulk
  leaf-counting, and bulk bitboard-shift pawn/king-danger generation.
- Optional `magic` cargo feature: magic-bitboard sliders (default build keeps
  the lean hyperbola tables). With `magic`, move generation outperforms the
  reference engine on every variant while the default build stays the leaner of
  the two.

### Tooling

- criterion benchmarks; cargo-fuzz targets (FEN, UCI, SAN, movegen); a
  comprehensive mce-vs-reference comparison harness (CPU, memory, and
  multi-hundred-position parity) kept in a separate, non-published crate.

[Unreleased]: https://github.com/ywzvennu/mce/commits/main
