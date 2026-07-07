# Per-variant position role-array sizing — spike (issue #565)

**Question.** Every generic position stores `Board<G>::by_role: [Bitboard<G>;
WideRole::COUNT]` with `COUNT == 149` (`src/geometry/role.rs`) — the global union
of *every* variant's roles — plus `GenericPlacement`'s two `[u8; COUNT]` hand
tallies. A 6-role standard-chess position is therefore as role-wide as Tenjiku,
and every new fairy role inflates *every* variant's position. #563 showed this
149-wide array is both the memory cost (generic position = 1568 B vs concrete
72 B) and was named as the dominant perf cost of the generic layer. Can a
per-variant array size (size-bucketing / macro-generated arrays) shrink the
common position and narrow the generic-vs-concrete perft gap — and does it get
past the #506 dead end?

**Outcome.** The array shrink is **feasible but should not be implemented now.**
The spike found something more important: the perf headline #563 attributed to
the fat array is actually **93 %** attributable to an *unbounded role loop* on
one variant — a free, one-line fix with no memory or type-system cost. Once that
loop is bounded, the array shrink is a real but *secondary* win (a further
~1.6× perft, position −72 %) that still requires the same large const-generic
refactor #506 rejected. Recommendation: **ship the one-line loop fix first
(separate issue); defer the array/bucket shrink to a later, scoped
implementation.** All numbers below are measured on this branch (throwaway
prototype, reverted), best-of-5, `--release`, this machine — not modelled unless
flagged.

## 1. The #506 post-mortem — what was tried, why it didn't ship, what changed

`docs/perf-role-array-spike.md` (#506) tried exactly this lever and shelved the
memory half. Its findings, and what has changed since:

**What #506 tried.** (a) Project the memory win of sizing each position to its
own role *span* `R = (max role index it can field) + 1` — measured a 74–90 %
shrink of the *median* position. (b) A throughput-only probe: bound the ~19 hot
movegen loops (`for role in WideRole::ALL { … }`) to the first 48 roles and
re-run perft — **2–3× faster on small variants, node counts byte-identical**.

**Why the memory half didn't ship (three reasons):**

1. **Stable-Rust blocker.** Sizing the array from the variant
   (`Board<G, { V::ROLE_SPAN }>`) needs `generic_const_exprs`, which is unstable.
   The stable workaround is an explicit `const R: usize` threaded through
   `GenericPosition<G, V, R>` *and* `Board<G, R>` — and `Board<G>` /
   `GenericPosition<G, V>` appear in hundreds of signatures across the ~7 000-line
   `position.rs` plus movegen, and the role-indexed pocket, zobrist hand table,
   binary wire hand codec and serde hand codec are each separately keyed on
   `COUNT`.
2. **Silent-corruption footgun.** If any variant's `R` is set below a role it can
   ever field (including via a promotion / drop / reveal / gate handled in
   *shared* code), that role's bitboard falls off the end and pieces vanish with
   **no type error**.
3. **Modest payoff on the named lever.** The `per_backing_position_size_ceiling`
   gate (#504) pins the *largest* position per backing, and the worst-case
   variant in each backing owns *high* role indices, so the ceilings tighten only
   u64 −48 % / u128 −24 % / **U256 0 %** (Tenjiku *is* the variant that owns the
   top roles, so its span is the full width — the giant-shogi variants that
   motivate the concern are precisely the ones the shrink cannot help).

**What #506 recommended, and what shipped.** #506's recommendation was to split
the win and take the safe throughput half first: add a per-variant `ROLE_SPAN`
and bound the hot loops to it, leaving the arrays at `COUNT`. **That shipped.**
`WideVariant::ROLE_SPAN` now exists, the ~19 loops slice `WideRole::ALL[..ROLE_SPAN]`,
and a `role_span_covers_all_fieldable_roles` meta-test walks every registered
variant from its start position and fails if any reachable role index is
`>= ROLE_SPAN`. So the throughput half is banked and *proven safe*.

**What that rules in/out for a retry.** The stable-Rust blocker (1) and the
mechanical threading cost are unchanged — bucketing does **not** sidestep them
(see §3). But footgun (2) is now **largely defused**: `ROLE_SPAN` plus its
coverage meta-test already establish, per variant and under test, the exact
bound a shrink needs. A bucket `N = next_pow2(ROLE_SPAN)` is a mechanical,
test-guarded function of an already-proven quantity — the thing #506 feared
proving for 59 variants is now a standing invariant. That is the one input that
has genuinely improved since #506.

## 2. The measurement that reframes the problem

#563 attributed the 32–49× generic-vs-concrete perft gap chiefly to the fat
board being *copied per node*. Prototyping the shrink exposed that this is
imprecise for the perft hot path, which uses **zero-copy make/unmake**
(`apply_with_undo` / `undo`, issue #309), *not* `play() -> Self`. `Undo` snapshots
only the touched roles and the two colour masks — it never copies the 149-wide
`by_role` wholesale. So the array width is **not** on the per-node copy path.

Decomposing the generic standard-chess perft (startpos → depth 5, 4 865 609
nodes) into the two independent levers:

| configuration | `size_of` | Mnps | vs baseline |
|---|--:|--:|--:|
| **baseline** (loop bound = `COUNT`, `by_role` = 149) | 1568 B | ~5.0 | 1.0× |
| + role loop bounded to span 8 (`by_role` still 149) | 1568 B | ~38 | **7.6×** |
| + `by_role` array shrunk to 8 (both levers) | 440 B | ~63 | **12.6×** |
| pocket `[u8;149]`→`[u8;8]` only, **no** loop bound | 1280 B | ~5.0 | 1.0× |

Three things fall out:

- **The loop bound is 93 % of the perf win and costs nothing.** Bounding the
  movegen role loop from 149 to the variant's span took generic standard-chess
  perft from ~5.0 to ~38 Mnps with the position **byte-for-byte the same size**
  and movegen byte-identical (node count unchanged). This is the #506 throughput
  lever — and it turns out **StandardChess never opted into it.** Every
  *registered* wide variant sets a tight `ROLE_SPAN`, but `StandardChess` is not
  a `WideVariantId` arm (it exists only as `GenericPosition<Chess8x8,
  StandardChess>`, the generic mirror of the concrete engine from #563), so the
  meta-test never walked it and it silently kept the `ROLE_SPAN = COUNT` default,
  looping all 149 roles per movegen call. **This is the single largest factor in
  #563's 32× headline** and it is a one-line fix (see §4).
- **Shrinking the array is a real but secondary perf win.** On *top* of the loop
  bound, cutting `by_role` from 149 to 8 slots gave a further ~1.65× (38 → 63
  Mnps) and shrank the position −72 % (1568 → 440 B). The gain is cache-footprint,
  not copy elimination: a 440 B position keeps the perft working set in far fewer
  lines than a 1568 B one. Real, but small next to the free 7.6×.
- **The pocket is memory-only.** Shrinking just the `GenericPlacement` pocket
  (`[u8;149]`→`[u8;8]`, −288 B, 18 % of a u64 position) moved perft **0 %** — even
  though `Undo.state` embeds the whole pocket and copies it per node. At 5 Mnps
  the 149-role loop dominates everything; the ~300 B state copy is in the noise.
  The pocket is a memory lever with no throughput payoff.

## 3. Evaluating the stable approaches to the array shrink

Given §1–§2, evaluating the three approaches the issue names:

### (a) Size-bucketing — `Board<G, const N>` with `N ∈ {8,16,32,64,128,COUNT}`

- **Feasibility on stable Rust: yes, but at #506's threading cost.** Bucketing
  does **not** sidestep the blocker. Deriving `N` from `V::ROLE_SPAN` still needs
  `generic_const_exprs`; the stable form is still an explicit `const N: usize`
  parameter on `GenericPosition<G, V, N>` and `Board<G, N>`, with each
  `WideVariantId` arm in the `any!` macro pinning a literal bucket. The role
  indexing stays branch-free and in-bounds (`by_role[role.index()]`, unchanged)
  **iff** `N >= ROLE_SPAN`, which the existing meta-test already guarantees — so
  the correctness contract is clean. But `Board<G>` / `GenericPosition<G, V>`
  thread through hundreds of signatures, and the pocket, zobrist hand table,
  binary hand codec and serde hand codec each independently key on `COUNT` and
  need the same parameter. This is the same large, mechanical refactor #506
  costed.
- **Bucketing's one advantage over exact-`R`:** it bounds monomorphization. Today
  `Board<Chess8x8>` is *one* instantiation shared by every 8x8 variant; making it
  `Board<G, N>` splits it per distinct `N`. Exact-`R` would mint up to ~59
  distinct array widths (heavy code bloat); bucketing caps it at ~6. That is the
  real reason to prefer buckets over exact spans.
- **Bucketing's disadvantages:** it rounds *up*, so it saves less memory than
  exact-`R` and, crucially, **the top bucket must be capped at `COUNT`, not
  `next_pow2`** — Tenjiku/Chu (span 127–149) would otherwise round to 256 and get
  *bigger*. The giant-shogi variants that motivate the concern still cannot
  shrink at all (unchanged from #506 reason 3), and the `#504` ceilings still
  tighten only for the u64/u128 backings.
- **Ergonomic/type cost:** every public signature naming `Board<G>` or
  `GenericPosition<G, V>` gains an `N` (or an inference-hiding wrapper). The
  `AnyWideVariant` facade already boxes its wide arms, so it can absorb the
  per-arm `N` without growing its inline size — but the internal surface is large.

### (b) Macro-generated per-variant fixed-size arrays

Same observable layout as (a); the `any!`/variant macros would stamp the literal
size per variant instead of passing a `const N`. It removes some signature noise
by generating monomorphic types, but multiplies code (one array type per
variant, i.e. exact-`R`'s monomorphization blow-up) and still has to thread the
concrete type through the shared movegen. No better than (a) on the core cost,
worse on bloat. Not preferred.

### (c) `generic_const_exprs` (`Board<G, { V::ROLE_SPAN }>`)

The clean form — length is literally the variant's span. **Unstable**, so it is
**not viable for a stable crate.** Confirmed still the case (this is #506's
blocker (1)); it is the reason (a)/(b) carry their threading cost at all.

**Does #506's failure reason still kill bucketing?** Partly. Footgun (2) is
defused by the now-shipped `ROLE_SPAN` + meta-test. Blocker (1) and the threading
cost are **not** — they are identical for bucketing. And payoff reason (3) is
unchanged for the *ceiling* gate, though §2 adds a new, modest *perf* payoff
(~1.6×) that #506 did not have. Net: bucketing is *more* justified than at #506
time, but still a large refactor for a secondary gain.

## 4. Recommendation

**Do the free win now; defer the array shrink.**

1. **Immediate, separate issue (strongly recommended): give `StandardChess` a
   tight `ROLE_SPAN`.** One line — `const ROLE_SPAN: usize = 8;` on
   `impl WideVariant for StandardChess` (6 is exact — Pawn..King and all
   promotions are indices 0–4; 8 is safe headroom). Measured **~7.6× generic
   standard-chess perft** (5.0 → 38 Mnps), **zero** memory change, **zero**
   type-system change, movegen byte-identical (node counts unchanged). This is
   most of #563's 32× gap and it is essentially free. Guard it by extending the
   `role_span_covers_all_fieldable_roles` meta-test (or the `perft_generic`
   suite) to cover the `StandardChess` mirror, which the `WideVariantId`-only
   walk currently misses — that gap is exactly how the unbounded default went
   unnoticed. **Worth auditing whether any other non-registered generic type
   shares the same unbounded default.**

2. **Array/bucket shrink: worth a full implementation *later*, not now.** It is
   feasible on stable via approach (a) (size-bucketing with `const N`, top bucket
   capped at `COUNT`), the footgun is now contained by the `ROLE_SPAN` meta-test
   (`N = next_pow2(ROLE_SPAN)`, buckets `{8,16,32,64,128,149}`), and it projects a
   further ~1.6× perft on the common variants plus a ~72 % position shrink
   (u64 standard chess 1568 → ~440 B; less for the wide backings, ~0 for
   Tenjiku). **But** it is the same large const-generic refactor #506 costed
   (threading `N` through `Board`/`GenericPosition`/`Undo`/pocket/zobrist/serde/
   binary), for a *secondary* gain now that the dominant perf factor is the free
   loop bound in (1). Sketch, if revisited: introduce `const N` on `Board<G, N>`
   and `GenericPosition<G, V, N>`; set `N` per arm in `any!` from
   `next_pow2(ROLE_SPAN)` capped at `COUNT`; keep `by_role[role.index()]`
   unchanged (in-bounds by the meta-test); shrink the pocket in the same pass
   (cheap, no geometry parameter). **Risks:** signature churn across ~7 000 lines;
   monomorphization growth (bounded to ~6 buckets); the wire/FEN/serde hand
   codecs must agree on the per-variant width; no help for the giant-shogi
   ceilings. Do it only if position memory (e.g. large transposition tables /
   many held positions) or the residual generic perft gap becomes a real
   constraint.

3. **Pocket-only shrink: skip unless memory-constrained.** 18 % of a u64
   position, **0 %** perft. Cheaper than the array (no geometry parameter, but
   still `COUNT`-keyed across pocket/zobrist/serde/binary), yet memory-only. Fold
   it into (2) if (2) ever happens; not worth a standalone change.

**Bottom line:** the array is *not* the perf villain #563 named — an unbounded
role loop on one variant is, and fixing that is free. The per-variant array
shrink is a legitimate memory-and-secondary-perf win that is now safer than at
#506 (footgun defused), but it still carries #506's full stable-Rust threading
cost for a payoff that (1) mostly captures for nothing. **Implement (1); defer
(2).**

## Reproduction

- Sizes: `size_of::<GenericPosition<Chess8x8, StandardChess>>()`.
- Perft: `mcr::geometry::perft(&GenericPosition::<Chess8x8, StandardChess>::from_fen(STARTPOS)…, 5)`,
  best-of-5 wall time, `--release`, via a throwaway `examples/` binary (reverted).
- Levers, each applied in isolation and reverted: (loop) `const ROLE_SPAN = 8` on
  `impl WideVariant for StandardChess`; (array) `by_role: [Bitboard<G>; 8]` in
  `src/geometry/board.rs` with `role_at` bounded to `WideRole::ALL[..8]`; (pocket)
  `GenericPlacement` fields `[u8; 8]`. Node counts were asserted equal to
  4 865 609 in every configuration (movegen byte-identical).

---

## Shipped: exact per-variant role-array sizing (issue #580)

The array/pocket shrink deferred above **shipped** in #580, as **exact
per-variant sizing** (not the bucketing sketched in §3(a)): each variant's
position stores exactly `V::ROLE_SPAN` role bitboards and `2 * V::ROLE_SPAN`
in-hand tallies — "as many as necessary and no more."

### Mechanism — stable Rust, no nightly

The role-array-bearing types gained a plain `const R: usize` const-generic
parameter with a **default of `{ WideRole::COUNT }`**:

- `Board<G, const R = COUNT>` — `by_role: [Bitboard<G>; R]`.
- `GenericPlacement<const R = COUNT>` — `white/black: [u8; R]`.
- `GenericState<G, const R = COUNT>`, `Undo<G, const R = COUNT>`,
  `GenericPosition<G, V, const R = COUNT>`, `GenericGame<G, V, const R = COUNT>`.

Each registered variant alias pins `R` to its span in **type-argument** position —
`GenericPosition<Geom, Rules, { <Rules as WideVariant<Geom>>::ROLE_SPAN }>` — which
is a fully-concrete const expression (no generic parameters), so it needs **no**
`generic_const_exprs` and compiles on the crate's pinned **stable** toolchain. The
default keeps the authoring surface (the 90 `starting_position` / `initial_placement`
overrides, which return `Board<Geom>` / `GenericState<Geom>`) untouched at the full
width; `GenericPosition::startpos` narrows that full-width start once via
`Board::resized` / `GenericState::resized` (a cold, one-time copy). Only the ~14
**hot** `WideVariant` methods that receive the stored `R`-wide board/state (e.g.
`royal_squares`, `promotion_targets`, `drop_targets`, `is_insufficient_material`)
became method-generic `<const R>` over `&Board<G, R>`; this also makes cross-variant
`starting_position` delegation (Karouk→Cambodian, Alice→StandardChess, …) width-
agnostic for free.

`by_role[role.index()]` indexing stays **branch-free** on the movegen hot path: the
existing movegen role loops are already bounded to `WideRole::ALL[..V::ROLE_SPAN]`,
and every fielded / promoted / dropped / gated role has index `< ROLE_SPAN == R` by
the `role_span_covers_all_fieldable_roles` meta-test. The three remaining full-width
scans that touch a board/pocket (`Board::role_at`, `GenericPosition::attack_map`,
the FEN/wire hand codecs) are bounded to `R`, and the four untrusted input
boundaries (FEN board placement, FEN holdings, binary board decode, binary hand
decode) reject any role with index `>= R` rather than index past the array.

### Safety under exact sizing

The meta-test's guarantee is preserved and strengthened: because the array is now
exactly `ROLE_SPAN` wide and every array write is a bounds-checked `by_role[idx]`,
a too-small `ROLE_SPAN` can no longer silently drop a piece — it is caught either by
the graceful `max < ROLE_SPAN` assertion (on the full-width `StandardChess` mirror)
or as a hard index-out-of-bounds panic when `apply` tries to place the out-of-span
role during the walk. Movegen output, perft counts, FEN, Zobrist keys, and
make/unmake are **byte-identical**: `properties` (make/unmake byte+key identity,
Zobrist, FEN/UCI round-trip over all variants), `conformance_concrete_generic`,
`variant_rules`, `concrete_variant_rules`, `coverage_gate`, and the perft suites
(shogi, xiangqi, chu, tenjiku, gardner, seirawan, dobutsu, …) all pass with
unchanged counts.

### Measured shrink (`size_of` of the concrete position, bytes)

| variant | backing | span | before (COUNT-wide) | after (exact) | shrink |
|---|---|--:|--:|--:|--:|
| gardner  | u64  | 6   | 1566 | 136  | −91% |
| makruk   | u64  | 8   | 1562 | 152  | −90% |
| seirawan | u64  | 12  | 1562 | 192  | −88% |
| dobutsu  | u64  | 24  | 1562 | 312  | −80% |
| capablanca | u128 | 12 | 2818 | 352 | −88% |
| xiangqi  | u128 | 23  | 2812 | 544  | −81% |
| shogi    | u128 | 29  | 2816 | 656  | −77% |
| opulent  | u128 | 110 | 2814 | 2112 | −25% |
| chu      | U256 | 127 | 5292 | 4544 | −14% |
| tenjiku  | U256 | 146 | 5302 | 5200 | −2%  |

The common u64/u128 positions shrink **77–91%**. As #506 predicted, the giant-shogi
variants that own the top role indices barely move (Tenjiku's span is essentially
the full `COUNT`, so it cannot shrink) — but they were never the ones held in bulk.
The runtime `AnyWideVariant` facade, sized by its widest inline (u128) arm (Opulent),
shrinks ~2814 → 2112 B (−25%). Perft node counts are unchanged and the perft
throughput delta is within run-to-run noise (the dominant per-node cost was already
the make/unmake role-mask work, not the array width; the array shrink is a cache-
footprint win, most visible when many positions are held resident).

The size-ceiling gates (`per_backing_position_size_ceiling`,
`any_wide_variant_size_ceiling`, #560) were rewritten to derive each position's
role-array contribution from its own `ROLE_SPAN` (a per-variant assertion) rather
than the global `COUNT`; the committed *fixed-overhead* bands are unchanged (fixed
overhead is genuinely role-count-independent), preserving the struct-bloat tripwire.
