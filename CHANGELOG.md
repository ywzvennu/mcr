# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-06-25

The first published release. A complete, permissively licensed, clean-room
chess move-generation and rules library.

### Added

- Board-geometry primitives: `Color`, `Role`, `Piece`, `File`, `Rank`,
  `Square`, the `Bitboard` set type, and the `Board` piece-placement type.
- A full standard-chess `Position` with legal move generation, make-move, and
  six-field FEN and UCI parsing/serialization.
- Perft node counters (`perft`, `perft_divide`), verified against published
  reference counts.
- Standard algebraic notation (SAN): `Position::san` and `Position::parse_san`.
- Incremental `Zobrist` hashing.
- Game outcomes and draw detection: `Outcome`, the precise `EndReason` labels,
  repetition tracking (`count_repetitions`, `is_repetition`), and the
  move-validating `Game` driver.
- Eight perft-verified variants — Chess960, King of the Hill, Three-check,
  Racing Kings, Atomic, Antichess, Horde, and Crazyhouse — exposed both as
  concrete `VariantPosition` types and through the `AnyVariant` runtime-dispatch
  enum with its `VariantId` selector.
- criterion benchmarks for move generation, perft, and variants.
- cargo-fuzz targets for the FEN, UCI, and SAN parsers.

[Unreleased]: https://github.com/ywzvennu/mce/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/ywzvennu/mce/releases/tag/v0.1.0
