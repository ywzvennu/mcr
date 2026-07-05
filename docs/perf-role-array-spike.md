# Per-variant role-array sizing — spike finding (issue #506)

**Outcome: not shipped.** This is a measured negative on the *memory* axis (like
the #310/#420 findings), with one large, unexpected *throughput* result that is
worth a scoped follow-up. Everything below is measured on this branch, not
modelled, unless flagged as a projection.

## The lever under test

Every generic position stores its placement as `by_role: [Bitboard<G>;
WideRole::COUNT]` (`src/geometry/board.rs`) where `WideRole::COUNT == 146`
(`src/geometry/role.rs`) — the **global union of every variant's roles**. A 3×4
Dobutsu position is therefore exactly as role-wide as Tenjiku, and every new
giant-shogi role inflates the position size of *every* variant. The spike asked:
can a per-variant role count shrink the common u64/u128 positions without
regressing movegen (role indexing must stay branch-free)?

## 1. The ceiling of the memory win — measured

`WideRole::COUNT` appears in **two** stored, per-position places, not one:

- `Board<G>::by_role` — `COUNT` bitboards = `COUNT × backing_bytes`.
- `GenericState<G>::placement` (`GenericPlacement`) — two `[u8; COUNT]` hand
  tallies = `2 × COUNT = 292` bytes, carried **unconditionally by every
  position**, drop variant or not (`GenericPlacement::NONE`).

Every position of a given backing is therefore the *same* size, because they all
carry the full 146 (measured via `WideVariantId::position_footprint`):

| backing | position size | `by_role` | pocket | role+pocket share |
|---|--:|--:|--:|--:|
| u64  (e.g. dobutsu, makruk, seirawan) | 1528 | 1168 | 292 | **95.5%** |
| u128 (e.g. shogi, xiangqi, opulent)   | 2752 | 2336 | 292 | **95.5%** |
| U256 (chu, dai, tenjiku)              | 5168 | 4672 | 292 | **96.1%** |

So **95.5–96.1% of every position is the role array plus the role-indexed
pocket.** The rest (board colour masks, side-to-move, castling, ep, gating, duck,
Alice plane, clocks, promoted mask) is ~4%.

### What a position would shrink to if sized to its own role span

A variant's roles are *not* a contiguous prefix of `WideRole`, but the standard
six are deliberately at indices `0..=5` and each variant's used roles cluster at
low indices, so a per-variant **span** `R = (max role index it can field) + 1`
keeps indexing literally identical (`by_role[role.index()]`, branch-free) while
shrinking the array to `R`. Projected size (`1528 − (146−R)·8 − 2·(146−R)` for
u64, `·16` for u128):

| variant | backing | span R | current | projected | shrink |
|---|---|--:|--:|--:|--:|
| makruk  | u64  | 8   | 1528 | **148**  | −90% |
| seirawan| u64  | 12  | 1528 | **188**  | −88% |
| dobutsu | u64  | 24  | 1528 | **404**  | −74% |
| xiangqi | u128 | 23  | 2752 | **310**  | −89% |
| shogi   | u128 | 29  | 2752 | **646**  | −77% |

The **common** position shrinks 74–90%. That is the real ceiling of the win.

### But the #504 per-backing ceilings tighten only modestly

The `per_backing_position_size_ceiling` gate (#504) pins the **largest** position
per backing, and the worst-case variant in each backing uses *high* role indices,
so the ceilings barely move even though the median position collapses. Worst span
per backing (from a static source scan of each variant's `WideRole::` usage):

| backing | worst variant | worst span | current ceiling | projected ceiling | tighten |
|---|---|--:|--:|--:|--:|
| u64  | khans        | 72  | 1528 | ~788  | −48% |
| u128 | opulent      | 110 | 2752 | ~2104 | −24% |
| U256 | tenjiku      | 146 | 5168 | 5168  | **0%** |

`AnyWideVariant` (facade, sized by its widest **inline** u128 arm — the U256 arms
are boxed) would track the u128 ceiling: 2768 → ~2120 (−23%).

The U256 ceiling **cannot** tighten at all: Tenjiku *is* the variant that owns
roles 132..=145, so its span is the full 146. The giant-shogi variants that
motivate the whole concern are precisely the ones the array shrink cannot help.

## 2. The unexpected result — the 146-wide loop is a throughput sink

The movegen hot path iterates the full role set — `for role in WideRole::ALL {
… board.pieces(by, role) … }` — in **~19 loops** in `src/geometry/position.rs`.
For a small variant this probes ~140 always-empty roles per call. To size that
cost, the 19 loops were temporarily bounded to the first 48 roles (arrays left at
146 — a *throughput-only* change) and perft re-run. **Node counts were byte-
identical** (spans of these variants are < 48), so behaviour is preserved:

| workload | baseline | roles bounded to 48 | speedup |
|---|--:|--:|--:|
| dobutsu perft(5)   | 2.737 ms (3.0 Mnps) | 1.010 ms (8.0 Mnps) | **2.7×** |
| shogi   perft(4)   | 391.8 ms (1.8 Mnps) | 156.4 ms (4.6 Mnps) | **2.5×** |
| xiangqi perft(4)   | 1695 ms (1.9 Mnps)  | 1397 ms (2.4 Mnps)  | 1.2× |

(best of 5, `--release`, this machine). The real per-variant spans are *smaller*
than 48 (dobutsu 24, shogi 29), so a true per-variant bound would beat these
numbers. **Bounding role iteration to each variant's span is a 2–3× movegen win
on small variants, entirely separate from the memory question**, and it is a
strict prerequisite for the array shrink (you must iterate only a variant's own
roles before you can store only them).

## 3. Why the memory shrink is not shipped

The throughput half is cheap; the **memory** half — actually resizing the array —
is not, and the memory half is what this issue's gate requires ("position size
clearly smaller AND the #504 ceilings can be tightened"):

- **Per-variant array sizing needs a size threaded through the whole generic
  stack.** `min_const_generics` is stable, but computing `R` from the variant
  (`Board<G, { V::ROLE_SPAN }>`) needs `generic_const_exprs`, which is **not**
  stable. The stable workaround is an explicit `const R: usize` parameter on
  `GenericPosition<G, V, R>` **and** `Board<G, R>`, then supplying a literal `R`
  per arm in the `any!` macro. That is mechanically enormous: `Board<G>` /
  `GenericPosition<G, V>` appear in hundreds of signatures across the ~7 000-line
  `position.rs` plus movegen, and the role-indexed pocket (`GenericPlacement`),
  the zobrist hand table, the binary wire hand decode, and the serde hand codec
  are all separately keyed on `COUNT` and would each need the same treatment.
- **Silent-corruption footgun.** If any variant's `R` is set below a role it can
  ever field — including through a promotion / drop / reveal / gate handled in
  *shared* code, not the variant file — that role's bitboard falls off the end
  and pieces vanish with no type error. The per-variant perft suites are a real
  safety net (a too-small span diverges perft), but proving the span bound for
  all 59 variants, including deep promotion chains, is the bulk of the risk.
- **The payoff on the thing the issue targets is modest.** The #504 per-backing
  ceilings — the explicit acceptance lever — tighten only u64 −48% / u128 −24% /
  U256 0%, because the worst-case variants own the high role indices.

High effort + real correctness risk + a stable-Rust blocker on the clean form +
only a modest tightening of the ceilings the issue names ⇒ this fails the "ship
only if net-positive and safe" bar for a spike. Per the spike guidance, it is
documented rather than forced.

## 4. Recommendation

Split the win and take the safe, high-value half first, as its own issue:

1. **Follow-up (recommended, low risk):** add a per-variant `ROLE_SPAN` (or a
   `&'static [WideRole]`) to `WideVariant` and bound the ~19 hot loops to it,
   leaving `by_role`/pocket at 146. This is *pure throughput* (2–3× on small
   variants, measured above), needs no const generics and no wire/FEN/serde
   changes, and its correctness is guarded by the existing per-variant perft
   suites plus a new meta-test asserting each `ROLE_SPAN` covers every role the
   variant can field.
2. **Only then**, if the memory footprint becomes a real constraint, revisit the
   array/pocket shrink on top of the now-proven per-variant spans — most cheaply
   by shrinking the **pocket** first (292 bytes, 19% of a u64 position, touched
   only by drop code, not by the 19 movegen loops) before attempting the far more
   invasive `by_role` const-parameter threading.

## Reproduction

- Sizes/share: `WideVariantId::position_footprint()` and
  `position_backing_bits()` over `WideVariantId::ALL`; `by_role = COUNT ×
  backing_bytes`, `pocket = 2 × COUNT`.
- Spans: static scan of `src/geometry/variants/*.rs` for `WideRole::<Name>`
  cross-referenced with the enum discriminants in `src/geometry/role.rs` (an
  approximate lower bound — shared-code roles are not counted, which is exactly
  why a shipped span needs the meta-test in the recommendation).
- Throughput: temporarily `sed -i 's/for role in WideRole::ALL {/for role in
  WideRole::ALL.into_iter().take(48) {/g' src/geometry/position.rs`, then perft
  the representatives under `--release` and compare node counts (identical) and
  wall time. Revert after.
