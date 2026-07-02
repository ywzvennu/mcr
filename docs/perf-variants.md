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

[`WideVariantId::ALL`]: https://docs.rs/mce/latest/mce/geometry/enum.WideVariantId.html
[`AnyWideVariant`]: https://docs.rs/mce/latest/mce/geometry/enum.AnyWideVariant.html
