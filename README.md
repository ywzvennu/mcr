# mce — Modular Chess Engine

**Status: in progress** — the library is under active development and is not yet feature-complete.

`mce` is a permissively licensed (MIT OR Apache-2.0), clean-room chess move-generation and rules library written in Rust.
It is an original implementation built from public chess algorithms and carries no copyleft obligation, making it safe to use in permissive and proprietary projects alike.

## Goals

- Perft-correct standard chess and Chess960
- Major variants: Atomic, Antichess, Crazyhouse, King of the Hill, Three-check, Racing Kings, Horde
- FEN, UCI, and SAN support
- No unsafe code; no copyleft dependencies

## Non-goals (for now)

- Engine search / evaluation — this library is move-generation and rules only
- GUI or network play

## Building

```sh
cargo build
```

## Testing

```sh
cargo test --all-features
```

## License

Licensed under either of

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.

Because `mce` is a clean-room implementation, it carries no copyleft obligation from upstream chess engines.
You may use it freely in permissive, proprietary, or any other projects.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for branch conventions, commit style, and the clean-room rule.
