# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-06-26

Performance-focused release. Move generation is substantially faster on several
variants with no change to results — every perft node count remains exact and
matches an independent reference engine.

### Added

- `MoveList`: a stack-allocated, fixed-capacity move buffer (with heap spill)
  used throughout the hot generation and perft paths to avoid per-node
  allocation.
- `Variant::BULK_COUNTABLE` and `Variant::slow_legal_into` hooks supporting the
  perft and legality optimizations below.

### Changed

- Chess960 now uses the fast pin-based legal generator (only castling is
  specialized), roughly a 5x move-generation speedup.
- Horde now skips king-safety filtering for the kingless white side and uses the
  fast generator for black, roughly a 3.5x speedup.
- Perft uses bulk leaf-counting at the final ply for the variants where it is
  sound (standard, King of the Hill, Three-check), plus assorted move-generation
  micro-optimizations.

### Fixed

- The Chess960 castle generator is now self-validating for discovered check
  rather than relying on a make-move legality pass.

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

[Unreleased]: https://github.com/ywzvennu/mce/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/ywzvennu/mce/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/ywzvennu/mce/releases/tag/v0.1.0
