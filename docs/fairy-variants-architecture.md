# Architecture: Fairy / pychess-class Variants in `mcr`

Status: design (no implementation in this PR). Drives the Milestone 10 issues.

Reference variant set: [pychess-variants](https://github.com/gbtami/pychess-variants),
powered by [Fairy-Stockfish](https://github.com/fairy-stockfish/Fairy-Stockfish).

## 0. Summary and recommendation

`mcr` today is a specialised 8x8 engine: a single `u64` `Bitboard`, a `Square(u8)`
indexed `0..64`, a `Board { by_color[2], by_role[6] }`, hyperbola-quintessence (or
optional magic) sliders, a packed `Move(u16)`, and a pin/check-mask fast-legality
generator that beats shakmaty. Every square index in that path is `& 7` / `>> 3`;
every move packs a 6-bit `from`/`to`; every `Role` is one of six. None of it
generalises to 80/81/90/100 squares or to new piece roles by accident, and that is
deliberate — it is why it is fast.

**Recommendation (decisive).** Do **not** retrofit the 8x8 path. Introduce a
**parallel, generic geometry layer** parametrised over a compile-time `Geometry`
trait, with the bitboard backing type chosen per geometry as an associated type:

- **8x8 stays exactly as is** — `u64`, untouched, unregressed. The existing
  `Variant`/`VariantPosition`/`AnyVariant` layer keeps running on it verbatim. All
  nine current variants and all new **8x8 fairy** variants (Makruk, Sittuyin,
  Seirawan, Spartan, Duck) live here.
- **9–100 squares use `u128` bitboards** as the single primary large-board backing.
  One `u128` covers 80, 81, 90, and 100 squares (all `<= 128`). This is the
  recommended primary representation: it keeps the whole bitboard algebra in two
  hardware registers, reuses the hyperbola-quintessence slider math unchanged (it is
  pure integer arithmetic, no `unsafe`), and avoids the per-word carry/borrow
  plumbing a `[u64; N]` multi-word board forces into every shift and slider.
- The generic layer is monomorphised over `Geometry` (zero-sized), exactly like the
  current `Variant<V>` layer is monomorphised over `V`. No dynamic dispatch on the
  hot path. `[u64; 2]` / mailbox are evaluated and **rejected** as the primary (see
  §2); `u128` wins on register pressure and code reuse for every board we need.
- Drops, promotion zones, palace/river/flying-general, cannon jump-capture, and
  blockable leapers slot in as **new generic hooks** modelled on the existing
  crazyhouse drop template and the `extra_moves`/`apply_extra`/`extra_terminal`
  hooks — see §4.
- Correctness and perf are pinned against **Fairy-Stockfish** via a GPL-fenced,
  subprocess-only `compare-fairy/` crate that shells out to the FSF UCI binary's
  `go perft` (§6). No GPL code is linked into `mcr`.

The single architectural bet: **one `u128` generic board for everything larger than
8x8, monomorphised per geometry, with the 8x8 `u64` path frozen and specialised.**
Everything else in this document follows from that bet.

---

## 1. Variant census and grouping

Grouped by board geometry, because geometry — not mechanics — is what forces the
board-representation decision. Mechanics (drops, promotion, special regions) are
cross-cutting and handled by hooks (§4). Movement vocabulary recurs heavily, so it
is named once and reused:

- **Ferz** — one diagonal step (4 destinations).
- **Wazir** — one orthogonal step (4 destinations).
- **Silver-general mover** — one diagonal step (4) or one straight-forward step (5
  destinations total).
- **Gold-general mover** — the three forward squares + two sideways + one straight
  back (6 destinations; no diagonal-backward).
- **Archbishop / Cardinal / Janus / Warlord / Princess** — Bishop + Knight.
- **Chancellor / Marshal / Empress** — Rook + Knight.
- **Cannon** — moves as a rook over empty squares; captures by jumping exactly one
  intervening "screen".
- **Blockable leaper (horse/elephant)** — a leap that is cancelled if a specific
  intervening square is occupied.

These compounds are implemented once and shared; only the *board* and the
*regions* differ across variants.

### Group A — 8x8, 64 squares (runs on the existing `u64` engine)

| Variant | New pieces | Drops | Promotion | Special |
|---|---|---|---|---|
| Makruk | Met (Ferz), Khon (silver-general) | no | pawn -> Met on rank 6 | counting endgame rule |
| Sittuyin | Sit-ke/Met (Ferz), Sin (silver-general) | no (has a *placement phase*) | in-place -> General, only after own general captured | promotion diagonals ("X"); hand-placement setup |
| Seirawan | Hawk (B+N), Elephant (R+N) | reserve gating, not drops | pawn -> Q/R/B/N/Hawk/Elephant | gating onto vacated back-rank square |
| Spartan | Lieutenant, Captain, General (R+Ferz), Warlord (B+N), Hoplite (Berolina pawn) | no | Hoplite -> any Spartan; King only if one king | asymmetric armies; **two black kings**; duple-check |
| Duck | the Duck (neutral universal blocker) | duck placement each turn | normal | no check; king-capture win; stalemate = win for stalemated side |

These need new **roles** and a few new **king-safety / terminal** rules, but no new
geometry. They are the cheapest phase precisely because the 8x8 bitboard, sliders,
and fast-legality core are reused unchanged (§7, Phase 1).

### Group B — 10x8, 80 squares (`u128`)

| Variant | New pieces | Notes |
|---|---|---|
| Capablanca | Archbishop (B+N), Chancellor (R+N) | king castles 3 squares; verify starting array against target `variants.ini` |
| Gothic | Archbishop, Chancellor | different array, identical moves |
| Caparandom | Archbishop, Chancellor | 960-style shuffle |
| Janus | Janus x2 (B+N), no Chancellor | pawns may promote to Janus |

### Group C — 10x10, 100 squares (`u128`)

| Variant | New pieces | Notes |
|---|---|---|
| Grand | Marshal (R+N), Cardinal (B+N) | "promote only to an already-captured piece"; optional rank-8/9, forced rank-10 |
| Shako | Cannon, Elephant (1-or-2 diagonal leap) | corner cannons; elephant has no river limit |

### Group D — 9x10, 90 points (`u128`)

| Variant | New pieces | Drops | Promotion | Special regions |
|---|---|---|---|---|
| Xiangqi | General, Advisor, Elephant (blockable, river-bound), Horse (blockable), Cannon, Soldier | no | none | 3x3 palace (diagonals for advisor), river, flying-general illegal |
| Janggi | General, Guard, Elephant (1+2 blockable leap), Horse, Cannon (screen to move *and* capture, can't touch cannons), Soldier | no | none | palace diagonals for general/guard/chariot; **no river**; bikjang draw |

Pieces sit on intersections; 9x10 = 90 points fits one `u128` with 38 bits unused.

### Group E — 9x9 + drops, 81 squares (`u128`)

| Variant | New pieces | Drops | Promotion |
|---|---|---|---|
| Shogi | Dragon King (+R), Dragon Horse (+B), Gold, Silver, Knight (forward-only), Lance, Pawn, Tokin | yes (from hand, flip color) | zone = furthest 3 ranks; into/within/out-of-zone; forced when stuck |
| Mini-shogi | same minus Knight & Lance | yes | zone = furthest 1 rank |

Drop legality (Nifu, Uchifuzume, dead-piece) is the most intricate hook work; it
generalises the crazyhouse pocket template (§4.1). Mini-shogi (25 squares) also fits
`u128` (or even `u64`, but it shares the shogi geometry path for code reuse).

---

## 2. Board representation at scale

### 2.1 What the 8x8 path bakes in (must not move)

From the current code:

- `Bitboard(pub u64)`, `#[repr(transparent)]`, with `FILE_A = 0x0101…`, `RANK_1 =
  0xff`, directional shifts that mask the A/H file to stop wrap-around. The whole
  algebra (`BitAnd`/`Shl`/`count`/`lsb`) is `u64`-native.
- `Square(u8)` indexed `0..64`, `from_file_rank = rank*8 + file`, `assert!(index <
  64)`. File/Rank are `& 7` / `>> 3`.
- `Board { by_color: [Bitboard; 2], by_role: [Bitboard; 6] }` — occupancy is a
  `u64` OR.
- `Move(u16)`: 6-bit `to` (bits 0..6), 6-bit `from` (bits 6..12), 4-bit flag (bits
  12..16, codes `0..=11`). **A 6-bit square field cannot index past 63, and a 4-bit
  flag has no room for arbitrary promotion/drop roles beyond the current set.** This
  is the single hardest constraint: the large-board move type must be wider.
- The fast generator computes a `king_danger` bitboard (`attacked_by(them,
  occ_without_king)`), a `check_mask` (`checkers | between(king, checker)`), and
  per-piece pin lines via `attacks::line`, all over `u64`. It is generic over a
  `MoveSink` so the same code materialises moves or bulk-counts leaves.

None of this changes. The large-board work is **additive**.

### 2.2 The three candidate representations

**(a) `u128` bitboard (RECOMMENDED primary for 9–100 squares).**
A single `u128` holds 128 bits, covering every board we need (80, 81, 90, 100 all
`<= 128`). Rust lowers `u128` to a pair of `u64` registers with compiler-generated
carry handling; the operations we use — `&`, `|`, `^`, `!`, `<<`, `>>`,
`count_ones`, `trailing_zeros` — are all available and all `safe`. Crucially, the
**hyperbola-quintessence slider identity reuses verbatim**: `(o - 2s) ^
reverse(reverse(o) - 2·reverse(s))` is pure integer arithmetic; widening `o`/`s`
from `u64` to `u128` changes only the word width and the per-line masks, not the
algorithm or its safety. File-wrap masking generalises to a per-`Geometry` set of
file/rank/edge constants (computed `const` from width/height). Register pressure is
two words, fixed, with no loop.

**(b) `[u64; N]` multi-word bitboard (REJECTED as primary).**
A 90-square board needs `[u64; 2]`; 100 needs `[u64; 2]`; nothing we target needs
`N > 2`, so multi-word buys no headroom over `u128`. It *costs*: every shift becomes
a cross-word shift-with-carry written by hand, every `lsb`/`popcount` becomes a
loop or an unrolled pair, and the hyperbola subtraction `o - 2s` must propagate
borrow across words manually. That is exactly the carry/borrow plumbing the compiler
already writes for `u128`, but now in our `safe` code, slower and more error-prone.
Multi-word only wins above 128 squares, which no pychess variant reaches. **Rejected**
for our square counts; kept on the table only as the escape hatch if a future >128
board appears (e.g. 11x11 = 121 still fits u128; 12x12 = 144 would not).

**(c) Mailbox / `[Piece; N]` (REJECTED as primary, used as a side table).**
Mailbox loses move generation: slider attacks become per-square ray walks instead of
one branchless bitboard identity, and the king-danger / pin-mask machinery that makes
`mcr` fast has no bitboard to operate on. Mailbox is, however, the natural backing for
the **piece-on-square lookup** every variant needs (e.g. "what is on the screen square
for a cannon capture") and may be carried as a redundant `[Option<Piece>; N]` side
table alongside the bitboards, exactly as many bitboard engines do. It is **not** the
primary representation.

### 2.3 The `Geometry` abstraction

Generalise the geometry as a compile-time trait, monomorphised per board so there is
no runtime dispatch (mirroring how `Variant` is monomorphised today):

```text
trait Geometry: Copy + 'static {
    type Bits: BitboardBacking;     // u64 for 8x8, u128 for 9..100
    const WIDTH: u8;                // files
    const HEIGHT: u8;               // ranks
    const SQUARES: u8;              // WIDTH * HEIGHT
    // const file/rank/edge masks, derived const from WIDTH/HEIGHT
}
```

- `Square<G>` is a thin `u8` newtype whose `file`/`rank` use `% WIDTH` / `/ WIDTH`
  (const-folded per geometry). For 8x8 these fold to `& 7` / `>> 3` — identical
  codegen to today.
- `Bitboard<G>` wraps `G::Bits` and carries the `const` masks. The `u64`
  specialisation must produce byte-identical machine code to the current
  `Bitboard(u64)`; this is enforceable by keeping `Chess8x8` geometry's `Bits = u64`
  and benchmarking against the frozen path (§6, regression gate).
- **The existing `Bitboard`/`Square`/`Board`/`Position` types are NOT
  re-parametrised.** They remain the concrete `u64` 8x8 types. `Geometry` is a new,
  separate hierarchy; the 8x8 generic instantiation `Geometry<Bits = u64>` is offered
  for *fairy variants that want the generic surface*, but standard chess and the nine
  existing variants keep using the concrete types so their proven codegen is frozen.

This is the key decision that protects the hot path: **generics are introduced as a
parallel layer, not by rewriting the concrete 8x8 types.** A regression there is a
benchmark failure, caught in CI, not a silent slowdown.

### 2.4 The wider move type

The large-board move cannot be `Move(u16)`. Introduce `WideMove(u32)` (or a small
struct) for the generic layer: 8-bit `from`, 8-bit `to` (covers `0..128`), and a
wider tag carrying move kind + promotion/drop role index over an extended role set.
The concrete 8x8 `Move(u16)` is untouched. `MoveKind::Drop { role }` already exists
and is the template; the wide tag widens the role field and adds the new kinds
(palace-diagonal, cannon-capture marker if needed, gating, duck-placement).

---

## 3. Slider and leaper attacks at scale

### 3.1 Sliders on `u128`

Reuse hyperbola quintessence with `u128` operands. The const ray tables (`between`,
`line`, per-direction line masks) are regenerated per `Geometry` at compile time from
width/height. This keeps the default build `no_std` and `unsafe`-free, exactly as the
current `attacks.rs` is ("only wrapping/normal integer arithmetic, no `unsafe` or
out-of-range indexing").

**Magic on `u128`.** The existing magic path is `std`-only, behind a feature flag,
and built lazily into a `LazyLock` table (~841 KiB for 8x8). Magic generalises to
`u128` in principle (a 128-bit multiply-shift index), but: (i) attack-table sizes
grow with the slider's relevant-occupancy bit count, which is larger on bigger boards
(more squares per ray), inflating memory; (ii) 128-bit magic multiplies are two-word
operations. **Recommendation:** ship the large-board sliders on **hyperbola
quintessence first** (it is the default, `no_std`, and already fast), and treat
`u128` magic as an *optional, later, `std`+`magic`-feature* optimisation evaluated per
geometry against the FSF perf target — never on the critical correctness path. The
8x8 magic table is unaffected.

### 3.2 Leapers, blockable leapers, and cannons

- **Plain leapers** (knight, ferz, wazir, gold/silver movers, archbishop/chancellor
  knight-component) are `const` precomputed attack tables per geometry, exactly like
  the current `KNIGHT_ATTACKS`/`KING_ATTACKS`. No occupancy needed.
- **Blockable leaper (Xiangqi/Janggi horse & elephant).** Precompute, per source
  square and per leap target, the **single blocking square** ("horse leg" /
  "elephant eye"). At generation time the target is admissible iff the blocking
  square is empty: one bitboard test per candidate. Janggi's elephant (1 orthogonal +
  2 diagonal) has a *path* of blockers — precompute the path mask and test it against
  occupancy in one AND. This stays branch-light and table-driven.
- **Cannon (jump-capture).** Movement is rook-style over empties — reuse the rook
  ray. Capture is the rook ray *past the first blocker* to the next piece along the
  line. Compute as: rook-attacks gives the first blocker; the cannon's capture target
  is the first occupied square **beyond** that blocker on the same ray. This is two
  ray lookups (or one ray plus a "next set bit beyond the screen" via masked
  `lsb`/`msb`), all bitboard ops. Janggi adds: the screen and target may not be
  cannons, and the cannon needs a screen even to move — extra masks against the
  cannon bitboard (`by_role[cannon]`), no new geometry.
- **Flying-general (Xiangqi).** The two generals may not face on an open file:
  treat it as a rook-style attack from one general along its file; if it reaches the
  other general with no screen between, the position is illegal. One ray test in the
  legality hook.

All of the above are bitboard operations on `G::Bits`, so they inherit the `u128`
backing for free and stay `unsafe`-free.

---

## 4. Exotic mechanics — how they slot into the trait layer

The existing `Variant` trait already proves the pattern: zero-sized rule layers,
each overriding only the hooks it needs, monomorphised so unused hooks cost nothing.
The generic layer gets a `GenericVariant` trait with the same shape plus the hooks
below. Each maps to an existing analogue.

### 4.1 Drops and pockets (Shogi) — generalise crazyhouse

Crazyhouse is the working template. Its `CrazyhouseState { pockets: [[u8; 5]; 2],
promoted: Bitboard }`, the `MoveKind::Drop { role }`, and the hooks
`extra_moves` (emit drops), `apply_extra` (apply a drop), `capture_side_effects`
(fill pocket), `hash_state`, and the placement read/write are exactly what Shogi
needs, widened:

- Pocket array widens to the shogi role set; counts stay `u8`.
- Shogi captures **flip color and ignore promotion** (a captured Dragon goes to hand
  as a Rook) — the crazyhouse `promoted` mask already implements "revert to the base
  piece on capture"; reuse it directly.
- Drop legality adds **Nifu** (no two unpromoted friendly pawns per file: test the
  file mask against the unpromoted-pawn bitboard), **dead-piece** bans (pawn/lance
  not on last rank, knight not on last two — a per-role drop-zone mask), and
  **Uchifuzume** (no pawn-drop delivering immediate mate: emit the drop, test for
  mate via the existing terminal machinery, reject). These live in the drop-emit
  hook (`extra_moves` analogue), which crazyhouse already owns.
- Drops land unpromoted; promotion-on-move is a separate hook (§4.2).

No new architecture — a wider pocket and a stricter drop filter on the proven path.

### 4.2 Promotion zones

Standard chess promotes on a single rank via `MoveKind::Promotion`. Generalise to a
per-`Geometry`/per-variant **promotion-zone mask** and rules:

- A `promotion_zone(color) -> Bitboard<G>` hook (the furthest 1 rank for mini-shogi
  / makruk-style, furthest 3 for shogi, ranks 8–10 for grand).
- A `promotion_kind` hook deciding optional vs forced (shogi forces when the piece
  would otherwise be stuck; grand forces on the last rank) and the legal target
  roles (`promotion_roles` already exists as a hook — widen its return). Grand's
  "promote only to an already-captured piece" reads the captured-pieces tally from
  variant state, exactly as crazyhouse reads its pocket.
- Shogi promotion triggers on **entering, moving within, or leaving** the zone — the
  hook receives both `from` and `to` and tests both against the zone mask.

### 4.3 Palace, river, flying-general (Xiangqi / Janggi)

These are **region masks** plus **movement restrictions**, all expressible as
const `Bitboard<G>` constants per geometry and a movement-filter hook:

- **Palace**: a 3x3 mask per side; the general/advisor/guard movegen ANDs its raw
  destinations with the palace mask. Palace diagonals (advisor in Xiangqi; general,
  guard, chariot inside palace in Janggi) are extra precomputed step/ray tables
  restricted to the palace mask.
- **River**: a rank threshold per side. The elephant's destinations AND with the
  own-half mask (Xiangqi); the soldier gains sideways steps once its square is past
  the river (test square against the crossed-river mask). Janggi has no river — the
  same hook returns `FULL`, so the soldier gets sideways from the start.
- **Flying-general**: a legality hook (§3.2) run alongside king-safety.

None of these are new *machinery*; they are masks fed to a per-piece destination
filter and a legality hook, both of which the trait already supports in spirit
(`filter_forced`, `is_legal_after`, `extra_terminal`).

### 4.4 Duck, gating, two-kings, placement phase

- **Duck**: a neutral blocker carried in variant state as one bit (or a `Square`).
  It is added to `occupied` for all sliders/steppers (knights ignore it — exclude it
  from the knight-block test). Each turn emits a **duck-placement sub-move**; model
  as a second move kind in the wide tag, applied via `apply_extra`. No check concept:
  set `king_is_royal = false`-style handling and a king-capture `extra_terminal`;
  stalemate-is-a-win is another `extra_terminal` branch. The hooks already exist.
- **Gating (Seirawan)**: reserves held in variant state (like a pocket); when a
  back-rank piece first moves, optionally emit a gating sub-move onto the vacated
  square. `extra_moves` + `apply_extra` + a small reserve state. Stays on the 8x8
  `u64` path.
- **Two kings + duple-check (Spartan)**: `king_is_royal` and the king-safety hooks
  generalise to a **set** of royal squares; "in check" becomes "any royal attacked,"
  and duple-check is "a single move attacks two royals" — a count over the royal
  set in the legality hook. State carries the royal-square set.
- **Placement phase (Sittuyin)**: a pre-game setup mode emitting placement moves
  until both sides are deployed; model as a distinct phase flag in variant state with
  its own `extra_moves` until the flag clears. 8x8 path.

### 4.5 New hooks summary

Added to the generic variant trait (each defaulting to a no-op / standard behaviour
so a variant overrides only what it changes, mirroring today's defaults):

- `promotion_zone(color) -> Bitboard<G>` and `promotion_is_forced(from, to, role)`.
- `region_mask(Region) -> Bitboard<G>` (palace, river-half, promotion-zone) — const
  per geometry/variant.
- `move_filter(piece, from, raw_targets, occ) -> Bitboard<G>` — applies palace/river
  confinement and blockable-leaper legs (default: identity).
- `royal_squares(core) -> Bitboard<G>` — generalises the single king (default: the
  one king square).
- Drop/gating/duck/placement reuse the existing `extra_moves` + `apply_extra` +
  state pattern with wider state and the wide move tag.

---

## 5. How the existing `Variant` layer extends

The current layer is a single generic `VariantPosition<V>` wrapping the concrete
`Position`, with `V: Variant` monomorphised (zero-sized), an `AnyVariant` enum for
dynamic dispatch at the API boundary, and a `VariantId`. Standard chess overrides
nothing and is byte-identical to `Position`.

**Decision: a parallel generic layer, not a re-parametrisation of the existing one.**

- The existing `Variant` / `VariantPosition<V>` / `AnyVariant` / `VariantId` stay
  bound to the concrete 8x8 `Position` and are **untouched**. All nine current
  variants and the **8x8 fairy** variants (Group A) live here, because they need no
  new geometry — only new roles and a few new terminal/king-safety rules, which the
  existing 15-hook trait already accommodates (add roles to `Role`, override
  `extra_terminal` / `king_is_royal` / `promotion_roles` / movement via a new 8x8
  `move_filter` hook).
- A new `GenericPosition<G, V>` (G: `Geometry`, V: `GenericVariant`) is introduced for
  **9–100 square** variants. It mirrors `VariantPosition<V>` — a core generic board
  position plus a per-variant rule layer — but over `Geometry`-parametrised
  `Bitboard<G>` / `Square<G>` / `Move`-wide types. It carries its own fast-legality
  path (the same pin/check-mask design, retargeted to `G::Bits`).
- `AnyVariant` (the API boundary enum) gains arms for the new geometries, dispatching
  to the right `GenericPosition<G, V>` monomorphisation. Dynamic dispatch lives
  **only** at this outer enum, exactly as today — the inner hot loops are
  monomorphised and dispatch-free.

This keeps two promises at once: the 8x8 fast path is frozen and unregressed, and the
large-board path gets the *same* zero-overhead generic discipline rather than a
vtable per hook.

### Why not one unified `GenericPosition<G, V>` for everything (including 8x8)?

Tempting, but it forces the proven `u64` codegen to flow through a generic
instantiation, risking subtle regressions (the compiler must re-prove the `& 7` /
`>> 3` foldings, the `MoveSink` inlining, the bulk-count specialisation). The cost of
a parallel layer is some duplicated structure; the cost of unifying is putting the
*one path that beats shakmaty* at the mercy of monomorphisation quality. Given
"performance is paramount and the 8x8 path must not regress," the parallel layer is
the conservative, correct call. If, after the generic layer is mature and benchmarked
at parity on 8x8, unification proves free, it can be revisited — but it is explicitly
out of scope for Milestone 10.

---

## 6. Reference and benchmarking — Fairy-Stockfish as oracle

`mcr` is permissively licensed (MIT OR Apache-2.0) and clean-room; Fairy-Stockfish is
GPL. **No FSF/Stockfish source is read, copied, or linked.** FSF is used only as a
black-box oracle over a process boundary.

### `compare-fairy/` crate (GPL-fenced, subprocess-only)

- A **separate workspace crate**, not a dependency of `mcr` proper, so the GPL fence
  is structural: `mcr` never links FSF. The crate spawns the `fairy-stockfish` UCI
  binary as a subprocess, sends `setoption name UCI_Variant value <variant>`,
  `position fen <fen>`, `go perft <depth>`, and parses the per-move divide and total
  from stdout.
- It drives **differential perft**: for a corpus of positions per variant, compare
  `mcr`'s `perft` / `perft_divide` against FSF's node counts at increasing depth.
  Any mismatch is a movegen bug, localised by the divide. This is the same role
  shakmaty-derived perft suites play for the 8x8 engine today, now with FSF as the
  authority for fairy rules.
- It also drives **perf comparison**: time `mcr`'s perft vs FSF's `go perft` on the
  same positions/depths to confirm the "beat FSF" target per variant.

### FSF build / availability feasibility

- FSF is an actively maintained open-source C++ project with a standard Makefile
  build and published release binaries; it builds on Linux/macOS/Windows. It accepts
  variants from a `variants.ini`, which also pins the exact starting arrays (resolving
  the Capablanca-array ambiguity flagged in §1) — `compare-fairy/` should record the
  `variants.ini` it tested against.
- Feasibility: building or downloading a `fairy-stockfish` binary in CI is
  straightforward; the crate locates it via an env var / config path and **skips
  gracefully** (test marked ignored) when the binary is absent, so `cargo test` for
  `mcr` itself is never blocked by FSF availability. The default `mcr` build and test
  remain wholly independent of FSF.

### 8x8 non-regression gate

Independently, a benchmark gate compares the (frozen) 8x8 `u64` path before/after the
generic layer lands, and asserts the generic `Geometry<Bits = u64>` instantiation
matches the concrete path's perft node counts and stays within a perf tolerance. This
is what enforces "the 8x8 path must not regress" mechanically rather than by promise.

---

## 7. Phased roadmap (cheapest-first)

Each phase states its variants, the new machinery, and the perf target (beat FSF on
`go perft`; never regress 8x8).

### Phase 1 — 8x8 fairy on the existing engine

- **Variants:** Makruk, Sittuyin, Seirawan, Spartan, Duck.
- **Machinery:** new `Role`s (Met, Khon, Hawk, Elephant, Spartan pieces, Duck) and
  their precomputed `u64` leaper tables; an 8x8 `move_filter` hook; `royal_squares`
  generalisation (two kings, non-royal duck); placement-phase and gating sub-moves on
  the existing `extra_moves`/`apply_extra`. **No new geometry, no new bitboard width.**
- **Why first:** it exercises the new *role* and *hook* surface while reusing the
  fastest, most-tested path. Validates the trait extensions before any `u128` work.
- **Perf target:** beat FSF on these variants; the 8x8 core is already shakmaty-beating,
  so the only new cost is the extra roles' movegen.

### Phase 2 — 10x8 / 10x10 on `u128`

- **Variants:** Capablanca, Gothic, Caparandom, Janus (10x8); Grand, Shako (10x10).
- **Machinery:** the `Geometry` trait, `Bitboard<G>` / `Square<G>` over `u128`, the
  wide move type, hyperbola sliders on `u128`, `GenericPosition<G, V>` with the
  retargeted fast-legality generator, Archbishop/Chancellor/Marshal/Cardinal compound
  movegen, the cannon and 1-or-2-leap elephant (Shako), Grand's buy-back promotion.
- **Why second:** introduces the *entire* generic-geometry stack but on boards whose
  movement is still mostly slide+leap+promotion — no palace/river/drops yet. Proves
  `u128` perf against FSF before the harder rule engines.
- **Perf target:** beat FSF on `go perft` for each; establish the 8x8 non-regression
  gate (§6) as part of this phase since the generic layer first appears here.

### Phase 3 — 9x10 Xiangqi / Janggi

- **Variants:** Xiangqi, then Janggi.
- **Machinery:** blockable-leaper leg/eye tables, cannon jump-capture (with Janggi's
  screen-to-move and no-cannon-screen rules), palace + palace-diagonal masks, river
  masks, flying-general legality, soldier sideways-after-river. All on the Phase-2
  `u128` `GenericPosition`.
- **Why third:** highest movement complexity, but no drops; reuses the entire Phase-2
  stack and adds region masks + the `move_filter`/legality hooks (§4.3).
- **Perf target:** beat FSF on Xiangqi/Janggi perft — the marquee large-board target.

### Phase 4 — 9x9 Shogi (drops)

- **Variants:** mini-shogi (5x5, smaller, validates the drop machinery), then Shogi.
- **Machinery:** the generalised pocket/hand (from crazyhouse), the wide drop tag,
  Nifu / dead-piece / Uchifuzume drop filters, into/within/out-of-zone promotion,
  forced promotion, captured-piece-flips-and-unpromotes (reuse the crazyhouse
  `promoted` mask). On the `u128` `GenericPosition`.
- **Why last:** drops + zone promotion + drop-mate detection are the most intricate
  rules; doing them last means the geometry, sliders, and generic legality are already
  proven, so this phase is pure mechanics on a stable base.
- **Perf target:** beat FSF on shogi/mini-shogi perft including drop generation.

---

## 8. Risks, open questions, and the recommendation

### Risks

- **Generic-layer 8x8 parity.** The chief risk is the generic stack quietly
  regressing the 8x8 path if the two ever merge. Mitigated by *not* merging them in
  Milestone 10 and by the §6 benchmark gate on the `Geometry<Bits = u64>`
  instantiation.
- **`u128` codegen quality.** `u128` is two registers with compiler carry handling;
  on some targets its shifts/multiplies are slower than hand-tuned `[u64; 2]`. The
  differential perf harness (§6) measures this directly per variant; if a specific
  geometry underperforms FSF, that single geometry can fall back to a `[u64; 2]`
  backing *behind the same `Geometry::Bits` associated type* without touching the rest
  of the architecture. The abstraction is chosen precisely so this swap is local.
- **Magic on large boards.** Larger relevant-occupancy counts inflate magic tables;
  treat `u128` magic as optional and `std`-gated, never on the correctness path.
- **Rule-engine intricacy.** Shogi drop-mate (Uchifuzume), Spartan duple-check, Grand
  buy-back, Janggi bikjang/cannon rules are subtle. Mitigated by FSF differential
  testing catching every divergence at the node-count level.
- **Starting-array ambiguity (Capablanca).** Resolved by pinning to the target
  `variants.ini` and recording it in `compare-fairy/`.

### Open questions

- Wide move type: `u32` packed vs a small `struct` — decide on benchmark, not taste.
- Does mini-shogi share the 9x9 shogi geometry path (code reuse) or take a dedicated
  5x5 `u64`/`u128` geometry (less waste)? Lean reuse; revisit if perf demands.
- Should the duck / placement-phase / gating sub-moves be one extra move kind each or
  a single generic "auxiliary placement" kind in the wide tag? Lean generic.
- Exact `variants.ini` pin and FSF binary provisioning in CI.

### Recommendation (single, decisive)

**Adopt a parallel, monomorphised generic geometry layer with `u128` as the single
primary bitboard backing for all 9–100 square variants, keeping the existing `u64`
8x8 engine frozen, specialised, and unregressed.** 8x8 fairy variants ride the
existing `Variant` layer (Phase 1); everything larger rides a new
`GenericPosition<G, V>` over `Bitboard<u128>`, reusing hyperbola-quintessence sliders
and the crazyhouse drop template, with palace/river/cannon/leaper rules expressed as
region masks and movement-filter hooks. Correctness and performance are pinned against
Fairy-Stockfish through a GPL-fenced, subprocess-only `compare-fairy/` crate. Ship in
the cheapest-first order: 8x8 fairy, then 10x8/10x10, then Xiangqi/Janggi, then Shogi.
The one fallback lever — swapping a single geometry's `Bits` to `[u64; 2]` if `u128`
underperforms FSF there — is local by construction and changes nothing else.
