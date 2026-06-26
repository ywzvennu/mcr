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
atomic, antichess, horde, and crazyhouse, it runs a **curated basket** of
positions (opening / midgame / tactical / endgame, 2–7 per variant, ~1–20M
nodes each) through **both** engines. For **every** position it **asserts the
node counts match** — a broad, independent re-validation of mce's move
generation against shakmaty as well as a fair throughput comparison.

CPU timing uses an **interleaved A/B/A/B…** schedule (each iteration times one
mce and one shakmaty run, alternating which goes first) so slow thermal/clock
drift is shared evenly between the engines instead of biasing one block. Each
engine is warmed up and then sampled many times; the report shows the **median**
throughput, the **peak** (fastest sample), the **mce/shakmaty ratio**, and a
spread (IQR + coefficient of variation) so real gaps are distinguishable from
noise.

### Parity caveat (koth / three-check)

For king-of-the-hill and three-check, shakmaty stops expanding a line the moment
its variant terminal fires (a king reaches a hill square; a side is checked for
the third time), whereas mce's `perft_variant` keeps counting. The basket
therefore uses positions and depths at which those terminals never fire within
the search, so the counts stay identical and comparable. The terminal check
still runs on every node — it simply never triggers — so the variant code path
is fully exercised. Crazyhouse drop positions keep both pockets within
shakmaty's material limit (no double-pawn pockets), which it rejects.

## Running

Comprehensive report (run in `--release`):

```sh
cargo run --release -p mce-compare                     # default (HQ sliders)
cargo run --release --features magic --bin mce-compare # magic-bitboard sliders
cargo run --release --bin mce-compare -- --csv         # + machine-readable CSV
cargo run --release --bin mce-compare -- --json        # + machine-readable JSON
```

Rigorous criterion benches (one representative position per variant):

```sh
cargo bench -p mce-compare
```

The binary prints, in order: a **per-position CPU table** (median/peak Mn/s per
engine, ratio, spread), a **per-variant aggregated CPU table** (node-weighted
median throughput + ratio), a **per-variant + aggregate memory table** (heap
allocs + bytes per engine), each engine's **static lookup-table footprint**, the
process **peak RSS**, and a **parity summary** (positions checked, all matched).
It exits non-zero if any node count disagrees between the engines. The optional
`--csv` / `--json` flag appends one machine-readable record per position for
tracking the numbers over time.
