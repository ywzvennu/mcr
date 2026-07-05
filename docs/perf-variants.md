# Variant performance sweep (issue #409)

Benchmark of every registered fairy (wide) variant, added to confirm that the
recently landed variants — the Capablanca family, courier, caparandom,
chancellor, tencubed, opulent, and the U256-backed **Chu Shogi** with its exact
Lion mechanics (#400/#412) — carry no performance or memory cliff, and that the
established variants have not regressed.

## Harness

The `variants` criterion bench now sweeps the whole registry instead of a
hand-picked subset: it iterates [`WideVariantId::ALL`] through
[`AnyWideVariant`], benching each variant from its starting position for

* **movegen** — a single `legal_moves()` call, and
* **perft depth 2** — one make-move ply plus a fresh generation (a bounded but
  representative aggregate move-gen signal).

Because it walks the registry, any newly registered variant is covered
automatically. The classic 8x8 Atomic / Crazyhouse benches are unchanged.

Run it with:

```text
cargo bench --bench variants -- wide_variant
```

## Method / environment

Numbers below are criterion medians on an Intel Core Ultra 7 155H, `rustc`
1.95.0, `--release` (`lto = "thin"`, `codegen-units = 1`), default
hyperbola-quintessence sliders. To keep the 59-variant sweep quick the run used
`--warm-up-time 0.4 --measurement-time 1.0 --sample-size 10`; absolute figures
carry the usual few-percent run-to-run noise, but the relative picture (which is
what a cliff would show up in) is stable. These are measured, not modelled.

## Results — all 59 wide variants

| variant | movegen (µs) | perft d2 (µs) | | variant | movegen (µs) | perft d2 (µs) |
|---|--:|--:|---|---|--:|--:|
| alice | 26.28 | 609.3 | | manchu | 15.10 | 1084.3 |
| almost | 4.40 | 100.0 | | mansindam | 10.74 | 319.6 |
| amazon | 3.59 | 101.5 | | minishogi | 5.07 | 117.5 |
| asean | 3.24 | 139.7 | | minixiangqi | 11.12 | 253.3 |
| bughouse | 5.43 | 126.3 | | opulent | 8.98 | 470.3 |
| cambodian | 3.46 | 110.6 | | orda | 3.83 | 123.1 |
| cannonshogi | 36.67 | 3064.0 | | ordamirror | 3.78 | 187.5 |
| capablanca | 7.94 | 241.9 | | placement | 0.49 | 16.8 |
| capahouse | 9.65 | 289.7 | | seirawan | 3.83 | 124.6 |
| caparandom | 7.67 | 211.9 | | shako | 28.94 | 1364.4 |
| chak | 29.74 | 2416.6 | | shatar | 3.73 | 85.3 |
| chancellor | 7.61 | 397.2 | | shatranj | 3.72 | 97.2 |
| chennis | 14.61 | 665.0 | | shinobi | 6.85 | 667.9 |
| chigorin | 6.05 | 450.8 | | shogi | 9.90 | 271.7 |
| **chu** | **78.70** | **10755.0** | | shogun | 13.27 | 124.7 |
| courier | 7.55 | 722.2 | | shoshogi | 26.24 | 659.9 |
| dobutsu | 1.45 | 32.3 | | shouse | 6.63 | 594.5 |
| dragon | 5.46 | 385.6 | | sittuyin | 0.65 | 42.9 |
| duck | 2.33 | 8787.6 | | spartan | 14.13 | 300.8 |
| embassy | 8.73 | 993.7 | | synochess | 9.27 | 259.5 |
| empire | 12.44 | 1046.3 | | tencubed | 10.48 | 303.6 |
| fogofwar | 1.76 | 132.1 | | tori | 9.90 | 176.6 |
| gorogoro | 5.75 | 711.4 | | xiangfu | 35.80 | 447.0 |
| gothic | 7.68 | 753.6 | | xiangqi | 17.66 | 787.2 |
| grand | 7.73 | 2637.1 | | jieqi | 106.04 | 14609.0 |
| grandhouse | 12.90 | 2989.6 | | janggi | 19.16 | 1798.9 |
| hoppelpoppel | 3.78 | 393.2 | | janus | 7.66 | 1363.3 |
| khans | 4.69 | 337.8 | | knightmate | 3.65 | 289.9 |
| kyotoshogi | 4.55 | 275.0 | | makpong | 4.59 | 340.5 |
| | | | | makruk | 3.42 | 314.9 |

## Memory footprint (`size_of`)

| type | bytes | note |
|---|--:|---|
| `WideMove` | **8** | packed `u64`; unchanged by the #400 Lion addition |
| `Seirawan` position (8x8, u64) | 1336 | |
| `Shogi` / `Xiangqi` / `Jieqi` / `Grand` / `Courier` position (u128) | 2400 | uniform across the u128 geometries |
| `Chu` position (12x12, **U256**) | 4512 | ~1.9× a u128 variant |
| `AnyWideVariant` (runtime facade) | 4512 | sized by its largest arm (Chu) |

Chu's ~1.9× footprint over a u128 variant is proportionate to its two-limb
(32-byte) backing plus its larger role set — not a step change. Because
`AnyWideVariant` stores every arm inline, its size equals Chu's.

## Findings

* **`size_of::<WideMove>() == 8` still holds.** The #400 Lion representation
  packs the `LionMove` kind and its mid-square into the existing move word; it
  did not grow the move. Guarded permanently by the
  `wide_move_is_eight_bytes` unit test.
* **The Lion path is off the common movegen path.**
  `WideVariant::has_lion_moves()` defaults to `false` and is overridden only by
  Chu. `gen_lion_moves` runs solely under that gate, so for every other variant
  the branch is a monomorphised `if false` and elides entirely — their move
  generation stays byte-identical (also enforced by the pinned per-variant perft
  suites, which are unchanged here).
* **No cliff.** Throughput tracks board size and rule complexity, with no piece
  or backing showing disproportionate per-node cost:
  * The high-cost variants are the ones inherently expected to be — **jieqi**
    (hidden pieces enumerate every possible identity), **chu** (144 squares, ~21
    piece types, ranging sliders + Lion), **cannonshogi / chak / shako / xiangfu**
    (large boards, cannon/ranging pieces), **alice** (dual-board legality), and
    **duck** at perft (each move spawns a duck placement, so the *tree* fans out
    while a single `legal_moves` stays cheap at 2.33 µs).
  * The specifically-flagged new leapers are unremarkable: **opulent**
    (Wizard/Champion) at 8.98 µs / 470 µs sits alongside the other 10×10 boards;
    the **Capablanca family** (almost, amazon, gothic, embassy, janus, chigorin)
    and **chancellor / tencubed / courier / caparandom** all land in the normal
    8×8–10×10 band.
  * **Chu on U256** is proportionate, not pathological: its per-`legal_moves`
    cost (~78.7 µs) is ~10× a 10×10 u128 variant for a board that is larger, far
    richer in piece types, and pays two-limb U256 shifts, plus the extra Lion
    pass. Its perft depth-2/movegen ratio (~137) reflects a high branching
    factor at the root, not an inefficient hot path.

**Conclusion: no regression.** No source change was required; this is a valid
"no cliff" result. The only change in this issue is the benchmark harness gaining
full-registry coverage, so future variants and any future regression in these
paths now have an in-repo, FSF-free signal.

# Optimization sweep (issue #420)

Issue #409 (above) established the no-regression baseline; issue #420 then
*optimized* against it. The measured result is one memory win and one honest
speed negative.

## Memory: box the U256 Chu arm of `AnyWideVariant`

`AnyWideVariant` is a plain enum with one arm per shipped variant, each holding
that variant's concrete position. A Rust enum is sized by its **largest** arm,
and every arm was stored inline — so the whole facade was sized by its single
U256 arm, the 12x12 Chu position (~4512 B, nearly 2x a u128 position). Every
common u64/u128 variant therefore paid the Chu footprint even though its own
position is ≤ 2400 B.

The fix boxes only the Chu arm (`Chu(Box<Chu>)`), leaving every u64/u128 arm
inline. The enum is now sized by a u128 position plus its discriminant, and only
Chu — already the heaviest variant to compute and rarely instantiated — pays a
single heap indirection.

| type | before | after | note |
|---|--:|--:|---|
| `WideMove` | 8 | **8** | unchanged (guarded by `wide_move_is_eight_bytes`) |
| `Chu` position (concrete, U256) | 4512 | 4512 | concrete type unchanged |
| `Shogi` / other u128 position | 2400 | 2400 | unchanged |
| **`AnyWideVariant` (runtime facade)** | **4512** | **2416** | **−46.5%** |

`size_of` measured with a throwaway `examples/` binary on the same toolchain;
the concrete positions are untouched, so all `size_of` values except the facade
are identical to the #409 table. Byte-identity is covered node-for-node by the
`enum_dispatch_matches_typed_path_for_every_variant` and
`make_unmake_round_trips_deep` tests (both exercise the Chu arm), and by the
broad deep `--ignored` perft suite — no node count changes for any variant.

## Speed: honest negative — the one-hot slider reversal

The hyperbola-quintessence slider core (`attacks::sliding`) reverses the one-hot
source bit `s` via `s.reverse_bits()` on every call. Since `s` is a single bit
at `sq.index()`, its full-width reversal is exactly `bit(BITS - 1 - index)`, so
the `reverse_bits` (two 128-bit reversals plus a limb swap on U256) can be
replaced with a direct `bit()`. It is byte-identical for a one-hot `s`.

Measured, it **regressed** the very variants it targeted: chu movegen +13%,
xiangqi movegen +11%, jieqi movegen +3% (the low-noise single-`legal_moves`
benches). The cause is that `U256::from_bit` carries a `< 128` branch, whereas
`reverse_bits` is branchless and CSE-friendly; the branch is slower than the
reversal it was meant to save. Reverted, not shipped — a #310/#364-style
negative. (The larger cannon-path per-node cost, e.g. the make/unmake scratch
clone, is intrinsic to the `&self` legal-move API and was already minimized by
#193/#353.)

# Exhaustive per-variant perf + memory sweep (issue #503)

Issues #409/#420 established throughput coverage over the whole `WideVariantId`
registry, but the **memory** picture was thin: only Chu had a `size_of` bench,
Dai/Tenjiku had no large-board perft, the footprint table hard-coded three rows,
and no wide variant had an allocation-count number. Issue #503 closes those gaps
so every variant now carries a size *and* space figure.

## What the sweep now emits, per variant

* **`size_of` / `align_of`** of the variant's concrete `GenericPosition`, read
  from the new [`WideVariantId::position_footprint`] accessor (a `const fn`
  matching the enum arm to `size_of`/`align_of` of its concrete position, plus a
  forwarding [`AnyWideVariant::position_footprint`] instance method). The
  `largeboard` bench's footprint table now iterates `WideVariantId::ALL` through
  it instead of hard-coding Seirawan/Grand/Chu.
* **Heap traffic** for a shallow move generation, measured by a counting
  `#[global_allocator]` installed in the new `benches/footprint.rs`: the byte
  size of the `legal_moves()` result buffer, and the allocation count / bytes a
  fixed depth-2 perft performs from the start position.
* alongside the existing **throughput** number from the `variants` sweep.

The new `footprint` bench walks both the 69 wide variants and the 9 concrete 8×8
variants; run it with `cargo bench --bench footprint` (add `-- --test` to print
the table and self-check without timing).

## Throughput — the 10 variants added since #409

Same reduced-sample methodology as the #409 table (`--warm-up-time 0.4
--measurement-time 1.0 --sample-size 10`); criterion medians, a few-percent
run-to-run noise. These complete the registry table to all **69** wide variants.

| variant | movegen (µs) | perft d2 (µs) |
|---|--:|--:|
| ai-wok | 11.37 | 165.7 |
| centaur | 15.68 | 349.5 |
| checkshogi | 27.57 | 629.7 |
| dai | 213.8 | 10909.0 |
| euroshogi | 18.33 | 244.6 |
| judkins | 11.90 | 185.6 |
| karouk | 9.29 | 199.3 |
| micro | 10.85 | 94.1 |
| tenjiku | 6092.6 | 281550.0 |
| washogi | 16.16 | 1112.0 |

**Dai** (15×15) and **Tenjiku** (16×16) are the two U256 large-shogi boards that
had no benchmark before; they now also have pinned large-board perft benches in
`largeboard` (`dai_15x15/startpos_d{1,2}` = 71 / 5041, `tenjiku_16x16/startpos_d{1,2}`
= 72 / 5663 — HaChu-validated, asserted in-bench before timing). Their cost tracks
board size and the ranging-slider / Lion (and, for Tenjiku, jumping-general and
Fire-Demon) rule richness — Tenjiku's 256-square board fills the two-limb U256
backing exactly, so it is the heaviest position in the project, as expected. No
cliff; the wider boards are simply proportionately more work.

## Memory footprint (`size_of`, current toolchain)

The concrete positions have grown since the #409/#420 tables (added variant
state and role sets), so these are the current measured sizes across the whole
registry via the new accessor. The value depends only on the backing integer:

| backing | `GenericPosition` size (B) | align | example variants |
|---|--:|--:|---|
| `u64` (≤ 8×8) | **1528** | 8 | seirawan, makruk, dobutsu, micro, spartan |
| `u128` (9×9 … 11×11) | **2752** | 16 | shogi, xiangqi, grand, capablanca, washogi |
| U256 (12×12 … 16×16) | **5168** | 16 | chu, dai, tenjiku |

`size_of::<WideMove>() == 8` and `size_of::<Move>() == 2` still hold; the
concrete `AnyVariant` runtime facade is 96 B (sized by its largest 8×8 arm).

## Allocation counts — a shallow-perft finding

The counting allocator confirms the alloc-free-perft property holds for **most**
variants: a depth-2 start-position perft performs **zero** heap allocations for
the large majority (all the big shogi boards — chu, dai, tenjiku — included). A
small set does allocate on this path, and the numbers pinpoint exactly which:

| variant | perft d2 nodes | allocs | bytes |
|---|--:|--:|--:|
| duck | 379440 | 7692 | 2789632 |
| shouse | 7944 | 93 | 744 |
| seirawan | 784 | 29 | 232 |
| spartan | 640 | 21 | 84 |

`duck` is inherent — every move spawns a duck-placement sub-enumeration, so the
tree (not a single `legal_moves`) fans out and touches the heap. The others are
the gating / pocket-carrying variants whose make-move clones a small owned buffer.
This is a *measurement*, not a regression: it turns the previously unenforced
"alloc-free perft" claim into a concrete per-variant number that a future change
can be checked against.

[`WideVariantId::ALL`]: https://docs.rs/mcr/latest/mcr/geometry/enum.WideVariantId.html
[`AnyWideVariant`]: https://docs.rs/mcr/latest/mcr/geometry/enum.AnyWideVariant.html
[`WideVariantId::position_footprint`]: https://docs.rs/mcr/latest/mcr/geometry/enum.WideVariantId.html#method.position_footprint
[`AnyWideVariant::position_footprint`]: https://docs.rs/mcr/latest/mcr/geometry/enum.AnyWideVariant.html#method.position_footprint
