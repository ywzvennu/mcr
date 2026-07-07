# CI: Fairy-Stockfish differential + perft-NPS signal

This documents how the Fairy-Stockfish (FSF) cross-engine differential and the
perft throughput signal are automated in CI (issue #561), and the four
constraints the design is built around. All of it lives in
[`.github/workflows/ci.yml`](../.github/workflows/ci.yml); the harness it drives
is the nested, GPL-fenced `compare-fairy/` crate (see its
[README](../compare-fairy/README.md)).

## Job map

| Job | Trigger | What it runs | Cost |
|---|---|---|---|
| `fairy-differential-pr` | pull_request, workflow_dispatch | **Shallow** FSF differential: bounded difffuzz (1 fixed seed × 2 games × 24 plies), depth-4 `--quick` corpus, and the shallow `--moveset` set differential | ~2-3 min cold cache, <1 min FSF work warm |
| `fairy-differential` | schedule (weekly), workflow_dispatch | **Deep** FSF differential: full corpus, rotating-seed difffuzz (12 × 90), held-back-variant fuzz, and the deep `--include-ignored` perft sweep | multi-minute |
| `perft-nps` | push-to-main, schedule, workflow_dispatch | perft criterion bench → nodes/sec metric (logged + `perft-nps.json` artifact) with a lenient catastrophic floor | seconds + bench |

The split is deliberate: the differential is expensive (a large C++ FSF build
plus a 90-variant sweep at depth), so the **per-PR** path runs a strictly
bounded slice that still catches a shallow movegen regression, while the
**deep** sweep is gated to the weekly schedule (and on-demand
`workflow_dispatch`) so it does not spend per-PR CI minutes. "Nightly" here maps
to the repo's existing **weekly** cron to bound Actions minutes; bump the cron
to daily if the coverage/latency trade changes.

## The four constraints, and how each is handled

### 1. CI cost — cache + shallow-PR / deep-nightly

The FSF binary is a large GPL C++ build (~2 min with `largeboards=yes`). The
per-PR job **caches** the built binary + its `variants.ini` with
`actions/cache`, keyed on the pinned FSF commit:

```
key: fsf-${{ runner.os }}-x86-64-largeboards-${{ env.FSF_REF }}
```

Only the first PR run after a cache miss (or an `FSF_REF` bump) pays the build;
subsequent runs restore it in seconds. On top of that the PR differential is
**bounded** — a fixed-seed difffuzz, a depth-4 corpus slice (`--quick`), and a
depth-2 move-set walk (`--moveset`) — a few seconds of actual cross-checking.
The unbounded sweep stays on the weekly job.

### 2. External-oracle pinning

FSF's own updates change perft counts, so the per-PR oracle is **pinned** to a
specific commit via `FSF_REF` in `ci.yml`:

```
FSF_REF: fb78cb561aa01708338e35b3dc3b65a42149a3c4
```

Pinning makes both the cache key stable and the oracle reproducible. Bump it
deliberately (a new commit forces one rebuild, then caches again). The weekly
`fairy-differential` job clones FSF `master` at `--depth 1` instead — it wants
the latest oracle and is not on the per-PR critical path, so it is not pinned or
cached.

### 3. License separation (GPL fence)

FSF is **GPL-3.0-or-later**; `mcr` is permissive (MIT OR Apache-2.0). FSF is
built and driven **purely as a subprocess over a process boundary** — the CI job
clones/builds it into a throwaway `fsf/` dir and passes the path via
`MCR_FSF_BIN` (+ `MCR_FSF_VARIANTS_INI`). It is **never** linked, vendored, or
added to `mcr`'s dependency graph, and the binary is never committed — only
ephemerally stored in the Actions cache. The `compare-fairy/` harness that
speaks UCI to it is a separate, non-workspace, `publish = false` crate. The
`cargo-deny` job independently enforces that `mcr`'s dependency tree stays
permissive-only.

### 4. Known FSF artifacts — allow-lists, not hard-red

Some divergences are FSF *oracle* quirks, not mcr bugs (e.g. gardner en passant,
duck-fallback, S-Chess corner-castle, Empire no-queenside-castle, and the
`HELD_BACK` variant follow-ups). CI does **not** hard-red on those: every mode
reuses the harness's existing artifact handling — the per-move discounts of
`fsf_omits_move`, the whole-node artifact skips
(`is_schess_corner_castle_artifact`, `is_empire_no_queenside_castle_artifact`),
and the `HELD_BACK` hold-outs — so only a genuine mcr divergence exits non-zero.
Nothing in CI adds or overrides those lists; the harness is the single source of
truth.

## perft-NPS signal

Perft is a pure move-generation + make/unmake tree walk, so its throughput is
the best single aggregate signal for a movegen performance regression. The
`perft-nps` job runs the `perft` criterion bench and converts each position's
mean time-per-iteration (from `target/criterion/perft/<id>/new/estimates.json`,
field `.mean.point_estimate`, in ns) into nodes/sec using the exact perft node
counts:

- `startpos_d4` = 197281 nodes (startpos perft(4))
- `kiwipete_d3` = 97862 nodes (kiwipete perft(3))

It logs a table to the job summary and uploads `perft-nps.json` so the trend can
be charted across runs. It is **not** a per-PR gate — shared runners are too
noisy for a tight wall-clock threshold (the same reasoning as
[`perf-regression.md`](perf-regression.md) and the `perf-baseline` job), so it
runs on push-to-main / schedule / manual to leave an NPS breadcrumb per merge.

The floor (`NPS_FLOOR`, 10 Mn/s on `startpos_d4`) is **lenient and
catastrophic-only**: it sits ~5-8× below a normal runner reading, so it fires on
an order-of-magnitude regression (an accidental `O(n²)` in movegen, a per-node
heap allocation) but never on ordinary noise. It is **not a ratchet** — do not
raise it toward the observed value or it will flake. The tight, deterministic
memory/allocation regression guards remain the cheap per-PR `#[test]`s in the
`ci` job (see `perf-regression.md`).
