# mcr fuzz targets

Coverage-guided fuzz targets for the `mcr` chess rules library, built on
[`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz) /
[`libFuzzer`](https://llvm.org/docs/LibFuzzer.html).

This is a **separate nested crate** (`mcr-fuzz`, `publish = false`) that depends
on `mcr` by path. It is not part of the published library, the parent crate's
workspace, or its normal `cargo build` / `cargo test`. The generated
`fuzz/target`, `fuzz/corpus`, and `fuzz/artifacts` directories are gitignored and
must never be committed.

## Requirements

Fuzzing needs a **nightly** toolchain and the `cargo-fuzz` subcommand:

```sh
rustup toolchain install nightly
cargo install cargo-fuzz
```

## Building

```sh
cargo +nightly fuzz build            # build all targets
```

## Running

```sh
cargo +nightly fuzz run fen_roundtrip
cargo +nightly fuzz run uci_parse
cargo +nightly fuzz run movegen_play
cargo +nightly fuzz run san_roundtrip
```

Pass `-- -max_total_time=60` (or `-runs=N`) to bound a run. Do not run long
campaigns in CI; these targets are intended for local hardening and triage.

## Targets

| Target           | What it checks |
| ---------------- | -------------- |
| `fen_roundtrip`  | Arbitrary bytes parsed as a FEN (standard chess and several variants); when a FEN parses, `from_fen(to_fen(pos)) == pos` and the Zobrist key is stable across the round-trip. |
| `uci_parse`      | Arbitrary input parsed as UCI against the start position (standard and Crazyhouse); accepted moves must be legal and round-trip through `to_uci`. |
| `movegen_play`   | A fuzzed FEN (or the start position) walked through a fuzzed sequence of legal moves; every reachable position round-trips through FEN and its Zobrist key is preserved. |
| `san_roundtrip`  | Arbitrary input parsed as SAN; accepted moves must be legal, and every legal move round-trips through `san` / `parse_san`. |
