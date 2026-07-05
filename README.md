# mcr — a clean-room chess rules library

`mcr` is a permissively licensed (MIT OR Apache-2.0), clean-room chess
move-generation and rules library written in Rust. It is an original
implementation built from public chess algorithms and specifications, and it
carries no copyleft obligation — making it safe to use in permissive,
proprietary, or any other projects alike.

The library covers standard chess and eight classic variants on its frozen
`u64` 8x8 engine, plus **60+ fairy / pychess-class variants** (Xiangqi, Shogi,
Makruk, Capablanca, Grand, Chu Shogi, and many more) on a parallel generic
geometry layer, plus the standalone Ataxx game. Move generation for every
variant is verified against published perft node counts and, where an oracle
exists, node-for-node against
[Fairy-Stockfish](https://github.com/fairy-stockfish/Fairy-Stockfish) (and HaChu
for the large shogis). It is rules-and-move-generation only: there is no search,
evaluation, GUI, or network play.

For how the engine is built — the frozen 8x8 `u64` core and the parallel generic
geometry layer the fairy variants ride — see [ARCHITECTURE.md](ARCHITECTURE.md).

## Features

- Perft-correct move generation for standard chess, eight classic variants, and
  60+ fairy variants spanning board sizes from 3x4 up to 12x12
- FEN, UCI, and SAN (standard algebraic notation) parsing and serialization,
  including SAN/UCI/PGN for the fairy geometry layer
- Incremental Zobrist hashing
- Game outcomes and draw detection — checkmate, stalemate, insufficient
  material, the fifty/seventy-five-move rules, and three/fivefold repetition —
  with precise per-variant end reasons
- A move-validating [`Game`] driver and an [`AnyVariant`] enum for runtime
  variant dispatch, plus `AnyWideVariant` for the fairy geometry layer
- WASM, Python, and C-FFI bindings (see [`bindings/`](bindings))
- No `unsafe` code; no copyleft dependencies

## Core variants (frozen 8x8 `u64` engine)

These ride the original concrete `Position` / [`Variant`] / [`AnyVariant`]
layer. Every entry is verified against published perft node counts in the test
suite under [`tests/`](tests).

| Variant            | Selector name(s)                  | Perft-verified |
| ------------------ | --------------------------------- | :------------: |
| Standard chess     | `chess`, `standard`               |       ✓        |
| Chess960           | `chess960`, `fischerandom`, `960` |       ✓        |
| King of the Hill   | `kingofthehill`, `koth`           |       ✓        |
| Three-check        | `threecheck`, `3check`            |       ✓        |
| Racing Kings       | `racingkings`, `racing`           |       ✓        |
| Atomic             | `atomic`                          |       ✓        |
| Antichess          | `antichess`, `giveaway`           |       ✓        |
| Horde              | `horde`                           |       ✓        |
| Crazyhouse         | `crazyhouse`, `zh`, `house`       |       ✓        |

## Fairy variant matrix (generic geometry layer)

> For a **complete, always-current** reference of every registered variant —
> board size, start FEN, notable pieces, special rules, and validation oracle —
> see **[docs/variants.md](docs/variants.md)**. That table is generated straight
> from the registries and drift-checked in CI, so it never falls behind the code.
> For per-variant perft/node-rate figures, see
> **[docs/perf-variants.md](docs/perf-variants.md)**.

The variants below ride the parallel **generic geometry layer**
([`mcr::geometry`]): `GenericPosition<G, V>` over a compile-time
[`Geometry`]-parametrised `Bitboard<G>` / `Square<G>`,
with a per-variant [`WideVariant`] rule layer. They
are selectable at runtime through `AnyWideVariant` / `WideVariantId`. See
[ARCHITECTURE.md](ARCHITECTURE.md) for the layer's design.

The tables here are a **representative selection** grouped by board geometry;
the complete, drift-checked list of all 60+ variants is in
[docs/variants.md](docs/variants.md).

The **Validation** column records how each variant's move generation is pinned.
"perft vs FSF" means mcr's perft node counts were checked **node-for-node**
against Fairy-Stockfish `go perft` on byte-identical positions (the
`compare-fairy/` harness, below); the test suite then pins those FSF-confirmed
numbers so regressions are caught even without FSF present. The few variants
with no usable FSF oracle (imperfect information or stochastic reveal) use the
tailored method noted. No engine-speed figures are claimed here; the
correctness story is what is documented.

### 8x8 boards (`Chess8x8`, `u64`)

| Variant | Board | Distinctive mechanic | Validation |
| ------- | :---: | -------------------- | ---------- |
| Makruk | 8x8 | Thai chess: Met (ferz) + Khon (silver-general); pawn promotes to Met; counting endgame | perft vs FSF |
| Makpong | 8x8 | Makruk where the king may not move out of check | perft vs FSF |
| Cambodian (Ouk) | 8x8 | Makruk + one-time first-move leaps for the king and Neang | perft vs FSF |
| ASEAN | 8x8 | Modernised Makruk: FIDE-style promotion and starting array | perft vs FSF |
| Sittuyin | 8x8 | Burmese chess: hand-placement setup phase + movement-driven Met promotion | perft vs FSF |
| Shatranj | 8x8 | Medieval ancestor: weak Ferz/Alfil, no double push, en passant, or castling | perft vs FSF |
| Shatar | 8x8 | Mongolian chess: queen replaced by the limited Bers; variant pawn/castling/draw rules | perft vs FSF |
| Knightmate | 8x8 | The Knight is royal; the kings are replaced by non-royal Commoners | perft vs FSF |
| Hoppel-Poppel | 8x8 | Knight and bishop swap their capture methods | perft vs FSF |
| Seirawan (S-Chess) | 8x8 | Gating: Hawk (B+N) / Elephant (R+N) held in reserve, gated onto a vacated back-rank square | perft vs FSF |
| Shogun | 8x8 | Crazyhouse hand + drops + a shogi-style far-rank promotion zone | perft vs FSF |
| S-House | 8x8 | Seirawan gating composed with crazyhouse drops on one shared hand | perft vs FSF |
| Shinobi | 8x8 | Fixed-reserve hand with drops + mandatory promotion zone (asymmetric clan vs standard army) | perft vs FSF |
| Dragon | 8x8 | Standard chess + a Dragon (B+N) in pocket, droppable only onto the own back rank | perft vs FSF |
| Duck | 8x8 | Neutral Duck blocker; two-part move (piece then duck); no check, king-capture win | perft vs FSF |
| Spartan | 8x8 | Asymmetric armies, two black kings + duple-check, Berolina Hoplite pawns | perft vs FSF |
| Orda | 8x8 | Asymmetric: standard White vs Black Orda cavalry (leapers) + flag-win | perft vs FSF |
| Ordamirror | 8x8 | Symmetric Orda-vs-Orda horde armies + flag-win | perft vs FSF |
| Khan's | 8x8 | Standard White vs Black Khan cavalry army + flag-win | perft vs FSF |
| Empire | 8x8 | Standard Black vs asymmetric White Empire (move-far/capture-close) + flag-win, flying-general | perft vs FSF |
| Synochess | 8x8 | Western "Kingdom" vs a Chinese/Korean "Dynasty" amalgam army | perft vs FSF |
| Placement | 8x8 | Pre-Chess: a deployment phase places the back-rank pieces, then normal chess | perft vs FSF |
| Alice | 8x8 (×2) | Two mirror boards; a piece moves, then transfers to the same square on the other board | Alice rules-validated (no FSF oracle); perft cross-checked vs an independent movegen |
| Bughouse | 8x8 | Single board of a 2-board team game: crazyhouse with the hand fed from the partner board | single-board perft vs FSF + hand-transfer tests |
| Fog of War | 8x8 | Standard movement with a non-royal, capturable king; no check (hidden information) | movegen perft vs FSF (variants.ini) + visibility tests |

### 10x8 boards (`Cap10x8`, `u128`)

| Variant | Board | Distinctive mechanic | Validation |
| ------- | :---: | -------------------- | ---------- |
| Capablanca | 10x8 | Adds the Archbishop (B+N) and Chancellor (R+N) compounds; king castles three squares | perft vs FSF |
| Capahouse | 10x8 | Capablanca plus crazyhouse captures-to-hand and drops | perft vs FSF |

### 10x10 boards (`Grand10x10`, `u128`)

| Variant | Board | Distinctive mechanic | Validation |
| ------- | :---: | -------------------- | ---------- |
| Grand | 10x10 | Marshal (R+N) + Cardinal (B+N); three-rank promotion zone; promote only to an already-captured type | perft vs FSF |
| Grandhouse | 10x10 | Grand plus crazyhouse captures-to-hand and drops | perft vs FSF |
| Shako | 10x10 | 10x10 chess plus the Cannon and Elephant fairy pieces | perft vs FSF |

### 9x10 boards — Xiangqi family (`Xiangqi9x10`, `u128`)

| Variant | Board | Distinctive mechanic | Validation |
| ------- | :---: | -------------------- | ---------- |
| Xiangqi | 9x10 | Chinese chess: cannons, blockable horse/elephant, palace, river, flying-general | perft vs FSF |
| Janggi | 9x10 | Korean chess: palace diagonals, no river, cannon must screen to move, bikjang draw | perft vs FSF |
| Manchu | 9x10 | Asymmetric Xiangqi: one side's rook/cannon/horse cluster becomes a single Banner super-piece | perft vs FSF |
| Jieqi | 9x10 | Hidden Xiangqi: every non-general piece starts face-down, revealing on its first move | xiangqi-core perft vs FSF + seeded reveal tests |

### 9x9 boards — Shogi-board family (`Shogi9x9`, `u128`)

| Variant | Board | Distinctive mechanic | Validation |
| ------- | :---: | -------------------- | ---------- |
| Shogi | 9x9 | Persistent capture-fed hand + drops; far-three-rank promotion (Nifu / Uchifuzume drop rules) | perft vs FSF |
| Sho Shogi | 9x9 | Old 9x9 Shogi *without* drops — captured pieces are removed, not pocketed | perft vs FSF |
| Cannon Shogi | 9x9 | Shogi hand/drops plus five cannon-type pieces; the Pawn becomes a sideways-stepping Soldier | perft vs FSF |
| Mansindam | 9x9 | Crazyhouse drops on 9x9 + mandatory far-rank promotion + a campmate flag win | perft vs FSF |
| Chak | 9x9 | Mayan chess: six new pieces, temple-half king promotion, region-confined pieces, the Quetzal cannon, temple-square win | perft vs FSF |
| Xiang Fu | 9x9 | Xiangqi-themed drop variant: a central 5x5 ring replaces the palaces; pseudo-royal duple-check win | perft vs FSF |

### 7x7 boards (`u128`)

| Variant | Board | Distinctive mechanic | Validation |
| ------- | :---: | -------------------- | ---------- |
| Minixiangqi | 7x7 | Reduced Xiangqi: cannon / horse / palace / flying-general, but no river, advisors, or elephants | perft vs FSF |
| Tori Shogi | 7x7 | Bird-army shogi with the full Shogi hand, drops, and per-piece promotion | perft vs FSF |
| Chennis | 7x7 | Tennis-themed Kyoto-style per-move flip + hand + dual-form drops + a king mobility region | perft vs FSF |

### Small shogi boards (`u64` / `u128`)

| Variant | Board | Distinctive mechanic | Validation |
| ------- | :---: | -------------------- | ---------- |
| Gorogoro Shogi Plus | 5x6 | Compact Shogi: hand, drops, promotion zone, with a Lance and Knight starting in hand | perft vs FSF |
| Minishogi | 5x5 | Shogi minus the Knight and Lance; far-one-rank promotion | perft vs FSF |
| Kyoto Shogi | 5x5 | Every piece flips to its alternate form after each move it makes | perft vs FSF |
| Dobutsu | 3x4 | Animal shogi: drops, a non-royal Lion, and a "try" (flag) win on the far rank | perft vs FSF |

### Ataxx (standalone module)

[`mcr::ataxx`] is **not** a chess variant and shares none of the engine's
machinery — no pieces, king, attacks, or [`Geometry`]
type. It is a self-contained 7x7 stones game built on a single `u64`, with its
own square / move / position / FEN / perft. Its perft node counts are validated
node-for-node against Fairy-Stockfish `UCI_Variant ataxx`.

## Validation against Fairy-Stockfish

The fairy variants are pinned against
[Fairy-Stockfish](https://github.com/fairy-stockfish/Fairy-Stockfish) (FSF) as a
black-box **perft oracle**. For each variant, a corpus of positions is run
through both engines at increasing depth and mcr's node counts are asserted
**equal to FSF's**, node for node; on a mismatch the per-move `divide` localises
the diverging move. The deterministic, full-information variants are validated
this way directly; the imperfect-information and stochastic specials use the
tailored methods noted in the matrix (Alice has no FSF oracle and is cross-checked
against an independent move generator; Bughouse uses single-board perft plus
hand-transfer tests; Jieqi validates its Xiangqi core via FSF plus seeded reveal
tests; Fog of War adds visibility tests on top of its movegen perft).

This head-to-head lives in a separate **`compare-fairy/`** crate that drives an
externally provided `fairy-stockfish` UCI binary purely as a **subprocess** —
it spawns the process, writes `position` / `go perft` over stdin, and reads node
counts from stdout.

**The GPL fence.** Fairy-Stockfish is GPL-3.0-or-later. `compare-fairy/` is a
**separate, non-workspace crate** (`publish = false`, like `compare/` and
`fuzz/`); it is **not in mcr's dependency graph**, and no FSF source or binary is
read, copied, vendored, or linked. The licensing never crosses the process
boundary, so the `mcr` library itself stays clean-room MIT OR Apache-2.0. Verify
with:

```sh
cargo tree -e normal -p mcr   # no FSF, no compare-fairy
cargo package --list          # zero compare-fairy/ files
```

Run the comparison (it **skips gracefully**, exit 0, if no FSF binary is found,
so it never blocks CI):

```sh
# Point the harness at a fairy-stockfish binary and run every shared variant:
MCR_FSF_BIN=/path/to/fairy-stockfish cargo run --release --manifest-path compare-fairy/Cargo.toml

# Or let it clone + build FSF into a git-ignored build/ dir (needs git, make, C++):
cargo run --release --manifest-path compare-fairy/Cargo.toml -- --build
```

## Quick start

```toml
[dependencies]
# In development — not on crates.io yet; depend on it from git:
mcr = { git = "https://github.com/ywzvennu/mcr" }
```

Parse a FEN, generate legal moves, play one, and read the outcome:

```rust
use mcr::{Color, Outcome, Position};

// Fool's mate, one move from the end: Black plays Qh4#.
let pos = Position::from_fen(
    "rnbqkbnr/pppp1ppp/8/4p3/6P1/5P2/PPPPP2P/RNBQKBNR b KQkq g3 0 2",
)
.unwrap();
assert!(pos.outcome().is_none());

let mate = pos.parse_uci("d8h4").unwrap();
assert_eq!(pos.san(&mate), "Qh4#");

let after = pos.play(&mate);
assert_eq!(after.outcome(), Some(Outcome::Decisive { winner: Color::Black }));
```

Choose a variant at runtime through `AnyVariant` and `VariantId`:

```rust
use mcr::{AnyVariant, VariantId};

// Pick a variant from a name, then use the same move-gen / play surface.
let id: VariantId = "atomic".parse().unwrap();
let pos = AnyVariant::startpos(id);
assert_eq!(pos.variant_id(), VariantId::Atomic);
assert_eq!(pos.legal_moves().len(), 20);

let e4 = pos.parse_uci("e2e4").unwrap();
let after = pos.play(&e4);
assert!(after.outcome().is_none());
```

Drive a fairy variant on a non-8x8 board through the parallel `AnyWideVariant`
surface (same shape, under `mcr::geometry`):

```rust
use mcr::geometry::{AnyWideVariant, WideVariantId};

let id: WideVariantId = "shogi".parse().unwrap();
let pos = AnyWideVariant::startpos(id);
assert_eq!(pos.variant_id(), WideVariantId::Shogi);
assert_eq!(pos.dimensions(), (9, 9)); // Shogi is a 9x9 board
assert!(!pos.legal_moves().is_empty());
```

The two runtime-dispatch families (`AnyVariant` / `VariantId` for the concrete
8x8 games; `AnyWideVariant` / `WideVariantId` for the fairy geometry layer)
expose parallel method sets — `startpos` / `from_fen`, `variant_id`, `turn`,
`legal_moves` / `legal_moves_from`, `play`, `outcome`, `end_reason`, `is_check`,
`to_fen`, `perft`, and so on — so code written against one reads the same against
the other. (The fairy family also carries a richer analysis surface — per-color
`is_in_check`, `attackers_of`, `attack_map`, pins — some of which is mirrored on
the concrete *core* `Position`; see [ARCHITECTURE.md](ARCHITECTURE.md).)

## Building and testing

Build the library:

```sh
cargo build
```

Run the test suite (unit tests, integration tests, and doctests):

```sh
cargo test --all-features
```

The deepest perft suites are marked `#[ignore]` because they are slow; run them
in release mode:

```sh
cargo test --release -- --ignored
```

Check that the criterion benchmarks compile:

```sh
cargo bench --no-run
```

Build the fuzz targets (requires a nightly toolchain and `cargo-fuzz`):

```sh
cargo +nightly fuzz build
```

The integration and perft suites live in [`tests/`](tests); the benchmarks live
in [`benches/`](benches); the fuzz targets live in [`fuzz/`](fuzz).

## Documentation

```sh
cargo doc --no-deps --open
```

## License

Licensed under either of

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.

Because `mcr` is a clean-room implementation, it carries no copyleft obligation
from upstream chess engines. You may use it freely in permissive, proprietary,
or any other projects.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for branch conventions, commit style, and
the clean-room rule.
