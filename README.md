# mce — Modular Chess Engine

`mce` is a permissively licensed (MIT OR Apache-2.0), clean-room chess
move-generation and rules library written in Rust. It is an original
implementation built from public chess algorithms and specifications, and it
carries no copyleft obligation — making it safe to use in permissive,
proprietary, or any other projects alike.

The library covers standard chess and eight variants, each verified against
published perft node counts. It is rules-and-move-generation only: there is no
search, evaluation, GUI, or network play.

## Features

- Perft-correct move generation for standard chess and eight variants
- FEN, UCI, and SAN (standard algebraic notation) parsing and serialization
- Incremental Zobrist hashing
- Game outcomes and draw detection — checkmate, stalemate, insufficient
  material, the fifty/seventy-five-move rules, and three/fivefold repetition —
  with precise per-variant end reasons
- A move-validating [`Game`] driver and an [`AnyVariant`] enum for runtime
  variant dispatch
- No `unsafe` code; no copyleft dependencies

## Feature matrix

Every entry below is verified against published perft node counts in the test
suite under [`tests/`](tests).

| Variant            | Selector name(s)                  | Perft-verified |
| ------------------ | --------------------------------- | :------------: |
| Standard chess     | `chess`, `standard`               |       ✓        |
| Chess960           | `chess960`, `fischerandom`, `960` |       ✓        |
| King of the Hill   | `kingofthehill`, `koth`           |       ✓        |
| Three-check        | `threecheck`, `3check`            |       ✓        |
| Racing Kings       | `racingkings`, `racing`           |       ✓        |
| Atomic             | `atomic`                          |       ✓        |
| Antichess          | `antichess`, `giveaway`           |       ✓        |
| Horde              | `horde`                           |       ✓        |
| Crazyhouse         | `crazyhouse`, `zh`, `house`       |       ✓        |

## Quick start

```toml
[dependencies]
mce = "0.1"
```

Parse a FEN, generate legal moves, play one, and read the outcome:

```rust
use mce::{Color, Outcome, Position};

// Fool's mate, one move from the end: Black plays Qh4#.
let pos = Position::from_fen(
    "rnbqkbnr/pppp1ppp/8/4p3/6P1/5P2/PPPPP2P/RNBQKBNR b KQkq g3 0 2",
)
.unwrap();
assert!(pos.outcome().is_none());

let mate = pos.parse_uci("d8h4").unwrap();
assert_eq!(pos.san(&mate), "Qh4#");

let after = pos.play(&mate);
assert_eq!(after.outcome(), Some(Outcome::Decisive { winner: Color::Black }));
```

Choose a variant at runtime through `AnyVariant` and `VariantId`:

```rust
use mce::{AnyVariant, VariantId};

// Pick a variant from a name, then use the same move-gen / play surface.
let id: VariantId = "atomic".parse().unwrap();
let pos = AnyVariant::startpos(id);
assert_eq!(pos.variant_id(), VariantId::Atomic);
assert_eq!(pos.legal_moves().len(), 20);

let e4 = pos.parse_uci("e2e4").unwrap();
let after = pos.play(&e4);
assert!(after.outcome().is_none());
```

## Building and testing

Build the library:

```sh
cargo build
```

Run the test suite (unit tests, integration tests, and doctests):

```sh
cargo test --all-features
```

The deepest perft suites are marked `#[ignore]` because they are slow; run them
in release mode:

```sh
cargo test --release -- --ignored
```

Check that the criterion benchmarks compile:

```sh
cargo bench --no-run
```

Build the fuzz targets (requires a nightly toolchain and `cargo-fuzz`):

```sh
cargo +nightly fuzz build
```

The integration and perft suites live in [`tests/`](tests); the benchmarks live
in [`benches/`](benches); the fuzz targets live in [`fuzz/`](fuzz).

## Documentation

```sh
cargo doc --no-deps --open
```

## License

Licensed under either of

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.

Because `mce` is a clean-room implementation, it carries no copyleft obligation
from upstream chess engines. You may use it freely in permissive, proprietary,
or any other projects.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for branch conventions, commit style, and
the clean-room rule.
