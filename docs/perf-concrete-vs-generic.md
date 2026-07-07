# Concrete 8x8 engine vs generic `Chess8x8` — architecture spike (issue #563)

**Question.** mcr implements standard 8x8 chess **twice**: the concrete,
hand-written engine ([`Position`] over frozen `u64` bitboards — `src/position.rs`,
`src/board.rs`, `src/variant/`) and the generic/wide engine
(`GenericPosition<Chess8x8, StandardChess>` — `src/geometry/`, parametrized over a
[`Geometry`] and a [`WideVariant`], monomorphized to the same `u64` width). The
generic path is *documented* to const-fold its `& 7` / `>> 3` coordinate math to
the same ops the concrete path hand-writes. So: **is a single unified fast
implementation achievable (retire the concrete layer onto the generic one), or is
the split justified?** This spike answers it with data.

**Outcome.** The split is **justified and should be kept.** The generic
`Chess8x8` path is **~32–49× slower** at identical standard-chess work and its
position is **~22× larger** in memory. This is nowhere near the "negligible gap"
that would make retiring the proven concrete engine defensible. The actionable
follow-up is **not** deleting an implementation — it is a **shared standard-chess
conformance test** so the two impls can never silently diverge.

## Harness

A new criterion bench, `benches/concrete_vs_generic.rs`
(`cargo bench --bench concrete_vs_generic`), runs the **same positions at the same
depths** through both engines so the gap is directly readable:

* **movegen** — `legal_moves()` on the startpos, a quiet middlegame, and Kiwipete
  (the three FENs the concrete `movegen` bench uses).
* **perft** — startpos to depth 4/5 and Kiwipete to depth 3/4 (the anchors the
  concrete `perft` bench uses). Each perft group sets criterion's
  `Throughput::Elements` to the known node count, so the report is in **nodes/sec**.

The node counts' *equality* between the two engines is a standing test invariant
(`tests/perft_generic.rs`), not re-asserted in the bench. This bench fills the one
gap the existing suite left open: `perft`/`movegen`/`variants` bench only the
concrete path, and `geometry` benches only the **wider** fairy geometries — none
pit the two 8x8 paths against each other on identical work.

## Method / environment

Criterion medians on an Intel Core Ultra 7 155H, `rustc` 1.95.0, `--release`
(`lto = "thin"`, `codegen-units = 1`), default features (no `magic` — both paths
use the hyperbola-quintessence sliders). Reduced sample sizes were used to keep
the run short; the gap is large enough that sampling noise is irrelevant.

## Results — move generation

`legal_moves()`, time per call (lower is better):

| position | concrete | generic | generic / concrete |
|---|--:|--:|--:|
| startpos | 165 ns  | 5.26 µs | **31.9×** |
| midgame  | 187 ns  | 6.89 µs | **36.8×** |
| kiwipete | 203 ns  | 8.91 µs | **44.0×** |

## Results — perft (nodes/sec)

Nodes/sec (higher is better):

| case | nodes | concrete | generic | slowdown |
|---|--:|--:|--:|--:|
| startpos d4 | 197,281   | 111.8 M/s | 3.45 M/s | **32.4×** |
| startpos d5 | 4,865,609 | 126.2 M/s | 3.97 M/s | **31.8×** |
| kiwipete d3 | 97,862    | 260.6 M/s | 5.33 M/s | **48.9×** |
| kiwipete d4 | 4,085,603 | 220.8 M/s | 5.07 M/s | **43.5×** |

The gap *widens* on Kiwipete (dense, slider- and pin-heavy) — the generic path
loses most where each node does the most work.

## Results — memory footprint

`size_of`, measured directly:

| type | bytes | generic / concrete |
|---|--:|--:|
| concrete `Position` | 72 | — |
| generic `GenericPosition<Chess8x8, StandardChess>` | 1568 | **21.8×** |
| concrete `Move` | 2 | — |
| generic `WideMove` | 8 | **4×** |

## Where the generic layer pays

The const-fold claim is true but beside the point: folding `& 7` / `>> 3` was
never the bottleneck. The cost is structural.

1. **Fat board representation dominates.** `Board<G>` stores
   `by_role: [Bitboard<G>; WideRole::COUNT]` (`src/geometry/board.rs`), and
   `WideRole::COUNT == 149` (`src/geometry/role.rs`) — the **global union of every
   fairy role in the library**. A standard-chess position that uses six roles
   still carries 149 bitboard slots: 149 × 8 = 1192 bytes of board alone, which is
   why the position is 1568 bytes vs the concrete engine's 72. (This is the same
   role-array cost the #506 spike measured at 95%+ of every wide position; #563
   shows its *throughput* consequence on the 8x8 path.)
2. **The 1.5 KB position is copied per node.** Generic `perft` advances via
   `position.play(mv) -> Self`, so **every node copies ~1.5 KB** and make-move
   touches the wide role array — versus the concrete engine's 72-byte state and
   6-piece bitboard set. This blows the L1/L2 working set and is the primary
   driver of the 30–50× gap; it compounds exactly where node work is heaviest
   (Kiwipete).
3. **Generic role dispatch.** Movegen dispatches over the 149-wide `WideRole`
   enum through geometry-parametrized attack helpers, rather than the concrete
   engine's fixed six-piece specialization with its pin-aware staged generator.
4. **Wider move encoding.** `WideMove` is 8 bytes vs `Move`'s 2 (4×), inflating
   the per-node move buffer.

Points 1–2 are the bulk of the gap and are *inherent* to a representation sized
for the whole variant zoo; const-folding coordinate math cannot remove them.
Shrinking them (e.g. the per-variant role-span idea from #506) would narrow the
memory gap but not close a 30–50× throughput gap on its own.

## Recommendation

**Keep the concrete engine. Do not retire it onto the generic path.** The
concrete engine is proven, hand-tuned, and 32–49× faster at the same work; the
generic path exists to host the ~90 fairy variants over arbitrary geometries, a
job the concrete engine cannot do. Both facts are load-bearing. A gap this size
settles the "unify?" question decisively: unification would mean a **30–50×
regression** on the most-exercised path in the library.

The real risk the split creates is **divergence** — two standard-chess impls that
could drift apart in behavior. The fix for that is a safety net, not a deletion:

* **Add a shared standard-chess conformance test** (default outcome, regardless of
  the gap): over a position corpus (startpos, Kiwipete, the CPW perft suite, plus
  random-playout positions), assert that the concrete `Position` and
  `GenericPosition<Chess8x8, StandardChess>` produce **byte-identical legal-move
  sets** (same moves, same encoding once normalized) and **equal perft** at each
  depth. `tests/perft_generic.rs` already checks perft *equality*; the conformance
  test would generalize it to the full legal-move set and a broader corpus, so the
  two engines are pinned together as a single specified behavior with two
  implementations. That is the property worth having — one spec, two impls that
  cannot silently diverge — and it is cheaper and safer than collapsing to one.

* **Retiring concrete onto generic is a possible *future* option only**, and only
  if the measured gap ever becomes genuinely negligible (say **< ~5%**) *and* the
  conformance test is already in place as a safety net. Today the gap is 30–50×,
  so this is firmly "could consider someday," not "should do." If it is ever
  revisited, the levers are the four cost centers above — chiefly the 149-role
  board width and the per-node full-position copy — none of which is a quick win.

[`Position`]: https://docs.rs/mcr/latest/mcr/struct.Position.html
[`Geometry`]: https://docs.rs/mcr/latest/mcr/geometry/trait.Geometry.html
[`WideVariant`]: https://docs.rs/mcr/latest/mcr/geometry/trait.WideVariant.html
