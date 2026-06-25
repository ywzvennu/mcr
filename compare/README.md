# mce-compare — perft benchmark (mce vs shakmaty)

A reusable benchmark that compares the `mce` chess engine's perft
(move-generation) throughput against [`shakmaty`](https://crates.io/crates/shakmaty)
for standard chess and all eight supported variants.

## ⚠️ Licensing isolation (important)

`shakmaty` is licensed **GPL-3.0+**. This `mce-compare` crate links it **for
benchmarking only**. To keep the `mce` library clean-room and free of any
copyleft obligation:

- This crate is a **separate, nested crate** (`compare/`), not part of the
  published `mce` package and not in any workspace — the same arrangement as the
  `fuzz/` crate.
- It is marked `publish = false`. It is **never published or distributed**.
- The `mce` library **does not depend on shakmaty** — not as a dependency and
  not as a dev-dependency. You can confirm this:

  ```sh
  cargo tree -e normal -p mce        # shows no shakmaty
  cargo publish --dry-run            # packages zero compare/ files
  ```

Because this crate is never distributed, linking GPL-3.0+ `shakmaty` here
imposes **no licensing obligation** on the `mce` library, which remains
**MIT OR Apache-2.0**. (This crate's own manifest is marked
`GPL-3.0-or-later` to honour shakmaty's terms for the benchmark binary itself.)

## What it does

For each of standard, chess960, king-of-the-hill, three-check, racing-kings,
atomic, antichess, horde, and crazyhouse, it runs perft on an identical
position/depth through **both** engines and **asserts the node counts match** —
this both ensures a fair comparison and independently re-validates mce's move
generation against shakmaty.

## Running

Headline timing table (run in `--release`):

```sh
cargo run --release -p mce-compare        # from the repo root
cargo run --release --bin mce-compare     # from inside compare/
```

Rigorous criterion benches:

```sh
cargo bench -p mce-compare
```

The binary prints a table:
`variant | depth | nodes | mce ms | shakmaty ms | ratio(shakmaty/mce) |
mce Mnodes/s | shakmaty Mnodes/s`, timed as the median of several runs, and
exits non-zero if any node count disagrees between the engines.
