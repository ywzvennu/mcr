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

The suite measures mce against shakmaty over **hundreds of positions** drawn
from three pools, then runs a parity cross-check over all of them, deep-perft
timing over a graded subset, and non-perft micro-benchmarks.

### Position pools

1. **Curated basket** — hand-picked opening / midgame / tactical / endgame
   positions per variant (~1–20M nodes each), used for the deep timing tables.
2. **Standard EPD suite** (`data/perftsuite.epd`) — the classic public-domain
   perft test set widely attributed to **Marcel van Kervinck** (the set shipped
   with many engines and on the Chess Programming Wiki): each line is a FEN plus
   `;Dn <nodes>` published reference perft counts. The data are public-domain
   facts; the loader is our own code. The file is embedded with `include_str!`.
   A few source lines describe positions one engine rejects; the loader/parity
   pass skips and **counts** those, so every retained line is a position both
   engines accept. mce is checked against **both** the published reference count
   and shakmaty.
3. **Seeded generated baskets** — for each variant, a fixed-seed PRNG
   (`splitmix64`) plays random legal games and snapshots FENs at a spread of
   plies (opening → endgame), yielding ~50–100 positions per variant with zero
   hand-curation. Generation is **fully deterministic** (no clock/thread RNG), so
   the corpus — and therefore the numbers — are stable across runs and machines.
   Chess960 games start from seeded **Scharnagl** start positions sampled across
   the 960 ids. Nothing large is committed; the corpus is regenerated from the
   seed at runtime.

### Parity (cheap, over EVERYTHING)

Every position in every pool is run through both engines at a **shallow** perft
depth and the node counts are asserted equal; for EPD positions mce is also
checked against the published reference. This is a broad, independent
re-validation of mce's move generation — hundreds of positions and tens of
millions of nodes. Any mismatch **fails loudly and exits non-zero**.

### Timing (expensive, over a graded subset)

The curated basket plus a per-variant sample of generated positions are timed at
deeper depths using an **interleaved A/B/A/B…** schedule (each iteration times
one mce and one shakmaty run, alternating which goes first) so slow thermal/clock
drift is shared evenly between the engines. Each engine is warmed up and sampled
many times; the report shows the **median** throughput, **mce/shakmaty ratio**,
and a spread (IQR + coefficient of variation).

### Micro-benchmarks (non-perft hot paths)

Over a standard-chess sample the suite also times `legal_moves()` generation,
`play()` make-move, and FEN parse+serialize throughput (mce vs shakmaty), plus
mce-only **SAN** and **Zobrist** throughput.

### Parity caveats (koth / three-check / racing / atomic / antichess / crazyhouse)

For the variants with a *path-dependent terminal* (a king on the hill, the third
check, a completed king race, an atomic king explosion, an antichess side with
no pieces), shakmaty stops expanding a line the instant the terminal fires while
mce's `perft_variant` keeps counting. The curated basket avoids depths where the
terminal fires; the **generated** positions are screened — when a deep count
diverges, the suite confirms a terminal is actually reachable inside the perft
tree and records the position as an **incomparable skip** (counted, never
silently dropped, never a failure) rather than a parity bug. Crazyhouse drop
positions stay within shakmaty's material limit; any rare position shakmaty
rejects is likewise counted as a skip.

## Running

Comprehensive report (run in `--release`):

```sh
cargo run --release -p mce-compare                     # default (HQ sliders)
cargo run --release --features magic --bin mce-compare # magic-bitboard sliders
cargo run --release --bin mce-compare -- --csv         # + machine-readable CSV
cargo run --release --bin mce-compare -- --json        # + machine-readable JSON
cargo run --release --bin mce-compare -- --full        # deeper timing + parity
```

The **default** run cross-checks ~1,050 positions and finishes in a few minutes
(~1.5 min on a modern desktop; the magic build is similar). `--full` deepens both
the parity depth and the per-variant timing sample/depth and takes
correspondingly longer.

Rigorous criterion benches (one representative position per variant):

```sh
cargo bench -p mce-compare
```

The binary prints, in order: a **per-variant parity table** (positions + nodes
verified, references confirmed, positions skipped), a **per-variant aggregated
CPU table** (node-weighted median throughput + ratio + spread), a **per-variant +
aggregate memory table** (heap allocs + bytes per engine), a **micro-benchmark
table**, each engine's **static lookup-table footprint**, the process **peak
RSS**, and an overall summary. It exits non-zero if any node count disagrees
between the engines or against an EPD reference. The optional `--csv` / `--json`
flag appends one machine-readable record per timed position.
