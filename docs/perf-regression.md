# Perf & memory regression protocol

This crate guards against **memory / allocation** regressions with cheap,
compile-time-stable per-PR tests, and against **timing** regressions with a
documented manual A/B protocol (timing is deliberately *not* a per-PR hard gate —
CI runners are too noisy for a stable wall-clock threshold). This file describes
both so a reviewer knows what runs automatically and what to run by hand on a
performance-sensitive PR.

## What runs automatically (every PR)

These are ordinary `#[test]`s in the `cargo test --all-features` gate. They cost
nothing at runtime — they read `size_of` / `align_of` and count allocations — and
fail immediately if a size-sensitive type grows or a per-node heap allocation
creeps back in.

### Memory — `size_of` ceilings (`src/geometry/any.rs`)

- **`any_wide_variant_size_ceiling`** — `size_of::<AnyWideVariant>()` (the runtime
  variant facade enum) must stay `<=` its current measured value. The enum is
  sized by its widest *inline* arm; the three U256 large-shogi arms (Chu, Dai,
  Tenjiku) are `Box`ed so they do not inflate it. Adding a variant, widening a
  position's inline state, or un-boxing a large arm trips this.
- **`wide_move_size_is_eight`** — `size_of::<WideMove>() == 8` (a packed `u64`).
  The structural guard also lives beside the type in
  `src/geometry/wide_move.rs::wide_move_is_eight_bytes`; this centralizes it in
  the memory gate. It is an exact equality, not a ceiling — the packed bit layout
  and the binary wire format both depend on the word being exactly eight bytes.
- **`per_backing_position_size_ceiling`** — for each bitboard backing (`u64` /
  `u128` / `U256`), the largest concrete position over that backing must stay
  within its current measured size. It buckets every variant in
  `WideVariantId::ALL` by `WideVariantId::position_backing_bits()` and checks the
  max `WideVariantId::position_footprint().0` per bucket. Growing any position's
  role array / inline state past its geometry class's current widest trips this.

Each ceiling is a **measured current value** with a `// Bump only deliberately`
comment. Raising one is allowed *only* as a conscious, justified decision — never
to paper over an accidental bloat. To refresh the numbers after an intended
change, print them with a throwaway `WideVariantId::ALL` sweep (see the tests for
the exact accessors).

### Allocation — alloc-free wide perft (`src/geometry/position.rs`)

- **`wide_perft_is_allocation_free_below_root`** — a fixed shallow perft
  (`perft(startpos, 3)`, 8902 nodes) over the generic (wide) engine on a
  spill-free variant (standard-via-wide, `GenericPosition<Chess8x8,
  StandardChess>`) performs **zero heap allocations** below the root. It uses the
  crate-test-scoped counting allocator (`alloc_probe`, promoted from
  `compare/src/alloc.rs`). This extends the alloc-free claim from a single shallow
  `legal_moves_into` to a whole perft walk, so a stray per-node `Vec<WideMove>` in
  the generator or make-move is caught. (A drop-heavy shogi could legitimately
  spill the inline buffer and allocate, which is why a spill-free variant is
  pinned.)

## What to run by hand (timing A/B, perf-sensitive PRs)

Timing is **not** a hard gate — a wall-clock threshold on shared CI runners is
flaky. Instead, on a PR that touches a hot path (move generation, make/unmake,
perft, the geometry layer), run an A/B comparison locally and paste the numbers
into the PR description.

1. **Criterion benches** — build once, then baseline before/after:

   ```sh
   # On the base commit (e.g. `main`):
   cargo bench --bench perft --bench variants --bench footprint -- --save-baseline before
   # On the PR branch:
   cargo bench --bench perft --bench variants --bench footprint -- --baseline before
   ```

   Criterion prints the per-benchmark delta and flags changes it considers
   statistically significant. `footprint` additionally prints a per-variant
   `size_of` / allocation table to stderr — a quick visual check that no
   variant's footprint or per-node alloc count moved.

2. **`compare` / `compare-fairy` throughput** (optional, GPL-fenced, run as a
   subprocess only) — the nodes/sec harnesses used for cross-engine A/B. See
   their READMEs. These are heavier and are *not* part of the workspace build.

A meaningful regression is a consistent, well-outside-the-noise slowdown across
repeated runs on a quiet machine — not a single noisy sample.

## Optional CI baseline job

CI has an **informational, non-blocking** `perf-baseline` job that runs on manual
`workflow_dispatch` only (never per-PR). It runs `cargo bench` and prints the
result; because runner timing is noisy it does **not** assert a threshold — it
exists so a maintainer can capture a baseline on demand. See
`.github/workflows/ci.yml`.
