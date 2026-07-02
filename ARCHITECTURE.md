# Architecture

This document describes how `mce` is built as shipped. It complements
[`docs/fairy-variants-architecture.md`](docs/fairy-variants-architecture.md),
the Milestone-10 *design* doc that proposed the geometry layer; this file
records the *as-built* result and the terminology used in the code.

`mce` has **two parallel engines** that share no hot-path code:

1. **The frozen 8x8 `u64` core.** The original concrete `Bitboard(u64)` /
   `Square(u8)` / `Board` / `Position` types, the hyperbola-quintessence (or
   optional magic) sliders, the packed `Move(u16)`, and the pin/check-mask
   fast-legality generator. Standard chess and the eight classic variants
   (Chess960, King of the Hill, Three-check, Racing Kings, Atomic, Antichess,
   Horde, Crazyhouse) ride this through the `Variant` / `VariantPosition<V>` /
   `AnyVariant` layer. Every square index is `& 7` / `>> 3`; this path is
   deliberately specialised and is **never re-parametrised** — its proven
   codegen is frozen.

2. **The parallel generic geometry layer** ([`src/geometry/`](src/geometry)).
   A second, independent hierarchy parametrised over a compile-time `Geometry`,
   built for fairy / pychess-class variants that need wider boards (up to 100
   squares) and new piece roles. The 47 fairy variants ride this layer. It is
   the subject of the rest of this document.

A third, fully self-contained module, [`src/ataxx.rs`](src/ataxx.rs), implements
the standalone 7x7 Ataxx stones game; it shares none of the chess machinery (no
`Board`, `Position`, `Bitboard`, `Variant`, or `Geometry`) and is built on a
single `u64`.

---

## The generic geometry layer

The decisive design bet (see the design doc, §0) was **not** to retrofit the
8x8 path but to introduce a parallel, monomorphised generic layer with the
backing integer chosen per board. The 8x8 `u64` path stays frozen; everything
wider rides `u128`. All modules below live under [`src/geometry/`](src/geometry).

### `Geometry`, `Bitboard<G>`, `Square<G>`, `Board<G>`

A [`Geometry`](src/geometry/mod.rs) is a compile-time, zero-sized description of
a board — its width, height, square count, backing integer type, and the derived
file / rank / edge masks:

```text
trait Geometry: Copy + 'static {
    type Bits: BitboardBacking;   // u64 for 8x8 / small boards, u128 for wide
    const WIDTH: u8;              // files
    const HEIGHT: u8;             // ranks
    const SQUARES: u8;            // WIDTH * HEIGHT, <= Bits::BITS
    const FILE_A_MASK: Self::Bits;
    const LAST_FILE_MASK: Self::Bits;
    const BOARD_MASK: Self::Bits;
}
```

Because every constant is `const`, the geometry is **monomorphised per board**:
there is no runtime geometry dispatch. The `geometry!` macro fills in the masks
from `WIDTH` / `HEIGHT`, so an implementor only supplies the dimensions.

- `Bitboard<G>` wraps `G::Bits` and carries the geometry's masks, giving set
  operations, iteration, and **edge-masked directional shifts** that work for
  any width, including non-power-of-two widths like 9 or 10 (a shift off the
  last file must vanish, not wrap to the next rank).
- `Square<G>` is a thin `u8` newtype whose `file` / `rank` use `% WIDTH` /
  `/ WIDTH`; for an 8x8 geometry these const-fold to `& 7` / `>> 3`, identical
  codegen to the concrete path. A `tests/` suite checks the `Chess8x8`
  instantiation is value-equivalent to the frozen concrete `Bitboard(u64)`.
- `Board<G>` is the generic piece-placement type (occupancy by color and by
  role) and a `WidePiece` of `(Color, WideRole)`.

The concrete 8x8 `Bitboard` / `Square` / `Board` / `Position` types are **not**
re-parametrised; `Geometry` is a separate hierarchy whose `Chess8x8` (`Bits =
u64`) instantiation is offered to fairy code that wants the generic surface
while the standard engine keeps its frozen types.

### `WideRole` and the three-tier overflow-letter FEN scheme

[`WideRole`](src/geometry/role.rs) is the geometry layer's role enum. It spans
**76 roles** (`WideRole::COUNT == 76`) — far more than the 26 letters of the
single-letter FEN alphabet — so FEN piece tokens use a three-tier
overflow-prefix scheme to stay unambiguous:

| Tier | Prefix | Constant | Used once the prior tier's letters are exhausted |
| ---- | :----: | -------- | ------------------------------------------------- |
| 1 | (none) | — | the bare alphabet `a..=z` |
| 2 | `*` | `OVERFLOW_PREFIX` | a recycled base letter (e.g. `*j`) |
| 2b | `**` | `OVERFLOW_PREFIX` doubled | a recycled base letter after `*…` is used up |
| 3 | `=` | `OVERFLOW_PREFIX_3` | a recycled base letter (e.g. `=d`) |

Case carries color, as in standard FEN. The prefix is part of the token, so a
piece such as a compound or a region-confined role spells itself as the prefix
plus a recycled letter rather than colliding with a base piece.

### `WideVariant` — default-off rule hooks

Each variant is a zero-sized rule layer implementing
[`WideVariant`](src/geometry/variant.rs). Mirroring the concrete `Variant`
trait, **every hook has a default** that reproduces standard chess behaviour, so
a variant overrides only what it changes and unused hooks cost nothing — every
variant is byte-identical to the shared core except where it opts in. The hooks
cover drops / hands, promotion zones, region masks (palace / river /
promotion-zone), per-piece movement filters (palace/river confinement,
blockable-leaper legs), royal-square sets (two kings, non-royal king), cannon
machinery, counting / flag-win terminals, and the reverse-projection guards
(`role_attack_is_leg_asymmetric`, `role_attack_is_directional`) discussed below.

### `GenericPosition<G, V>` and `GenericGame<G, V>`

[`GenericPosition<G, V>`](src/geometry/position.rs) is the generic analogue of
`VariantPosition<V>`: a `Geometry`-parametrised board position plus a per-variant
`WideVariant` rule layer, with its own pin/check-mask fast-legality generator
retargeted to `G::Bits`. It is deliberately **history-free** — `outcome()` is
answered from the board and state alone — so `perft` never allocates a history
and stays byte-identical.

The history-dependent terminal rules therefore live in a separate wrapper,
[`GenericGame<G, V>`](src/geometry/game.rs): an opt-in history-recording driver
that resolves repetition (Xiangqi / Janggi), Shogi *sennichite* and its
perpetual-check exception, and the Makruk / Cambodian counting countdown. This
mirrors how the concrete `Game` wraps `Position`.

### Runtime dispatch: `AnyWideVariant` / `WideVariantId`

Each shipped fairy variant is a distinct compile-time type `GenericPosition<G,
V>` — exactly what a consumer that knows its variant at compile time wants. But
a CLI, a binding, or a server picks the variant from a string at runtime and
cannot name `G` / `V`, and (unlike the concrete 8x8 engine) the geometries
differ, so a single generic type cannot erase them. [`AnyWideVariant`](src/geometry/any.rs)
is the type-erased enum wrapper that does this, selected by a `WideVariantId`.
Dynamic dispatch lives **only** at this outer enum; the inner hot loops stay
monomorphised and dispatch-free.

For a complete, always-current reference of every registered variant — board
size, start FEN, notable pieces, special rules, and validation oracle — see
[`docs/variants.md`](docs/variants.md). It is generated from the registries
(`VariantId` / `WideVariantId`, `AnyWideVariant::dimensions`, and each start
position's `to_fen()`) by `tests/variants_doc.rs`, which also drift-checks the
committed copy against a fresh render, so the reference cannot fall behind the
code.

### Geometry families

The shipped geometries, by backing integer:

| Geometry | Board | Bits | Hosts |
| -------- | :---: | :--: | ----- |
| `Chess8x8` | 8x8 (64) | `u64` | the 8x8 fairy variants |
| `Cap10x8` | 10x8 (80) | `u128` | Capablanca, Capahouse |
| `Grand10x10` | 10x10 (100) | `u128` | Grand, Grandhouse, Shako |
| `Xiangqi9x10` | 9x10 (90) | `u128` | Xiangqi, Janggi, Manchu, Jieqi |
| `Shogi9x9` | 9x9 (81) | `u128` | Shogi, Sho Shogi, Cannon Shogi, Mansindam, Chak, Xiang Fu |
| `Minixiangqi7x7` | 7x7 (49) | `u128` | Minixiangqi |
| `Tori7x7` | 7x7 (49) | `u128` | Tori Shogi |
| `Chennis7x7` | 7x7 (49) | `u128` | Chennis |
| `Gorogoro5x6` | 5x6 (30) | `u64` | Gorogoro Shogi Plus |
| `Minishogi5x5` | 5x5 (25) | `u64` | Minishogi, Kyoto Shogi |
| `Dobutsu3x4` | 3x4 (12) | `u64` | Dobutsu |

A single `u128` covers every board larger than 64 squares (80, 81, 90, 100 are
all `<= 128`), keeping the whole bitboard algebra in two registers and reusing
the hyperbola-quintessence slider math unchanged. The three same-sized 7x7
geometries are kept **distinct** so, e.g., the Tori bird army never shares masks
with the Xiangqi-on-7x7 palace/river machinery.

### The `attackers_to` consistency guard

`GenericPosition::attackers_to(t, c)` answers "which pieces of color `c` attack
square `t`" by **reverse-projecting** each role's pattern back from `t`. That is
only valid when a role's attack relation is symmetric and color-non-directional.
Two latent check-detection bugs of exactly this class once shipped: the Xiangqi
**Horse**, whose hobbling leg is adjacent to the horse (asymmetric), and the
Xiangqi **Soldier**, whose forward step is color-directional. They are now
handled by the `WideVariant::role_attack_is_leg_asymmetric` /
`role_attack_is_directional` hooks.

[`tests/attackers_consistency.rs`](tests/attackers_consistency.rs) is the
systematic guard so the class cannot recur: for every variant it computes the
**forward** attack relation independently (projecting each occupied piece's
`role_attacks` set forward) and asserts it agrees with the reverse-projecting
`attackers_to` on every square and color.

### Notation and bindings

- **SAN / UCI / PGN.** The geometry layer has its own notation surface
  ([`src/geometry/notation.rs`](src/geometry/notation.rs)): UCI move I/O, SAN,
  and `WidePgn` parsing/serialization adapted to wide boards and the
  overflow-letter role alphabet.
- **Bindings.** The fairy variants are exposed through the WASM, Python, and
  C-FFI bindings under [`bindings/`](bindings) and the `cli/` front end, all
  driven through `AnyWideVariant` so a variant can be chosen by name at runtime.

---

## Validation and the GPL fence

Fairy-variant move generation is pinned against
[Fairy-Stockfish](https://github.com/fairy-stockfish/Fairy-Stockfish) (FSF) as a
black-box perft oracle: mce's node counts are asserted equal to FSF's, node for
node, on byte-identical positions, with the per-move `divide` localising any
divergence. The deterministic, full-information variants use this directly; the
imperfect-information / stochastic specials (Alice, Bughouse, Jieqi, Fog of War)
use the tailored methods recorded in the README matrix.

The head-to-head lives in the separate **`compare-fairy/`** crate, which drives
an external `fairy-stockfish` UCI binary purely as a **subprocess**. FSF is
GPL-3.0-or-later; `compare-fairy/` is a non-workspace crate (`publish = false`),
is **not** in mce's dependency graph, and reads / copies / links no FSF source
— so the licensing never crosses the process boundary and the `mce` library
itself stays clean-room MIT OR Apache-2.0. See the README's *Validation against
Fairy-Stockfish* section for how to run it.
