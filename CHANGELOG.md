# Changelog

All notable changes to `mcr` are documented in this file.

**Status: in development — unversioned, not released.** `mcr` has never been
published to crates.io and carries no version tag: the crate is `version =
"0.0.0"` and `publish = false`. There are no release sections below; this file
is a single running log of the current surface and recent changes. Depend on it
from git, not a version number. A version and a `CHANGELOG` split into releases
will come only when the API is deliberately frozen for a first release.

The library is a clean-room chess **move-generation and rules library** at
Fairy-Stockfish parity — rules and move generation only. There is **no search,
no evaluation, no GUI, and no network play**; it is the foundation such tools
build on, not one of them.

## In development

### Variants

- **79 registered variants** across two layers (the exhaustive, always-in-sync
  list is generated into [`docs/variants.md`](docs/variants.md)):
  - **9 on the concrete 8×8 engine** (`mcr::AnyVariant` / `VariantId`): standard
    chess, Chess960, King of the Hill, Three-check, Racing Kings, Atomic,
    Antichess, Horde, Crazyhouse.
  - **~70 on the generic geometry layer** (`mcr::geometry`, `AnyWideVariant` /
    `WideVariantId`): the Xiangqi/Janggi/Jieqi/Minixiangqi cannon family; the
    Shogi family (Shogi, Mini/Sho/Tori/Kyoto/Dobutsu/Gorogoro+/EuroShogi/
    Checkshogi/Cannon Shogi and the large shogis **Chu**, **Dai**, **Tenjiku**);
    Makruk and cousins (Sittuyin, Cambodian/Ouk, ASEAN, Makpong, Karouk,
    Ai-Wok); the Capablanca family (Capablanca, Grand, Gothic/Embassy/Janus,
    Almost, Amazon, Chigorin, Seirawan, Grandhouse, Capahouse, S-House,
    Chancellor 9×9); the pychess armies (Orda, Synochess, Shinobi, Ordamirror,
    Empire, Khan's, Mansindam, Chak, Xiang Fu); Shatar, Shatranj, Dragon,
    Knightmate, Hoppel-Poppel, Manchu, Shogun, Chennis, Courier, Centaur,
    Caparandom, Placement, Judkins, Micro; and the imperfect-information /
    standalone specials Alice, Fog of War, Bughouse, Duck, and Ataxx.
- Board geometries from **3×4 up to 16×16** (Tenjiku fills a 256-square U256
  board exactly); an 8-bit role field (wire-format v2) accommodates the
  large-shogi piece sets.
- **Validation:** move generation is verified node-for-node against
  [Fairy-Stockfish](https://github.com/fairy-stockfish/Fairy-Stockfish) where an
  oracle exists, against **HaChu** source tables for the large shogis (which FSF
  cannot run), and by hand-derived/brute-force means for the specials
  (Alice, Fog of War, Bughouse, Jieqi, Ataxx). All confirmed perft counts are
  pinned in the test suite.
- **Known modeling approximations** (documented in each variant's module docs):
  Tenjiku's Fire Demon area-burn/igui and the jump-capturing Generals'
  jump-over-and-capture are not modeled (they cannot be packed into the current
  `WideMove`); Janggi implements a faithful subset of the perpetual-chase rule.

### Core & rules (concrete 8×8)

- Board-geometry primitives — `Color`, `Role`, `Piece`, `File`, `Rank`,
  `Square`, the `Bitboard` set type, the `Board` piece-placement type.
- A full standard-chess `Position`: legal move generation, in-place
  `play_unchecked` / immutable `play`, six-field FEN, UCI.
- Perft node counters (`perft`, `perft_divide`) verified against published
  reference counts and an independent generator.
- Standard algebraic notation — `Position::san` / `parse_san`.
- Incremental `Zobrist` hashing.
- Outcomes and draw detection — `Outcome`, precise `EndReason` labels, threefold/
  fivefold repetition, fifty/seventy-five-move rules, insufficient material, and
  the move-validating `Game` driver.

### Notation & formats

- SAN / UCI / PGN for both the concrete engine and the fairy geometry layer,
  with lossless round-trip coverage across every variant.
- Polyglot (`.bin`) opening-book reading (`book` module): the standard Polyglot
  Zobrist key (`polyglot_key`), a `Book` reader with binary-search `lookup`,
  move decoding (incl. the castling-as-king-takes-rook quirk and promotions),
  and a `weighted_pick` helper.
- EPD parsing/serialization and an EPD/perft suite runner.
- Optional `serde` (behind the `serde` feature): public value types gain
  `Serialize`/`Deserialize`; `Position`/`Board` round-trip as FEN and
  `AnyVariant` as a `{ variant, fen }` pair.

### Bindings & reach

- WASM, Python (pyo3), and C-FFI bindings in the sibling `bindings/` crates
  (not part of the library crate), each under CI (build + smoke tests +
  cbindgen header-drift check).
- `no_std` (+`alloc`) support for the core and variants; a `mcr-uci` variant
  perft/divide adapter binary; optional `rayon` parallel perft.

### Performance

- Stack-allocated `MoveList`, allocation-free perft, packed 16-bit `Move`,
  compact `Position`, fast-legality generators for every variant, perft bulk
  leaf-counting, and bulk bitboard-shift pawn/king-danger generation.
- Generic large-board engine tuning: a stack-backed reusable move buffer,
  allocation-free perft, bulk leaf-counting, scan-free make-move mutation,
  closed-form slider line masks, and an inline pin set — large boards stay perft
  byte-identical while running materially faster. `AnyWideVariant` boxes only the
  U256 (large-shogi) arm to keep its footprint small.
- Optional `magic` cargo feature: magic-bitboard sliders (the default build keeps
  the lean hyperbola tables). With `magic`, move generation outperforms the
  reference engine on every variant while the default build stays the leaner.

### Tooling & validation

- criterion benchmarks (incl. large-board U256 perft + `size_of` footprint);
  cargo-fuzz targets (FEN, UCI, SAN, movegen, and wide-layer variants);
  property tests across all wide variants (FEN/make-unmake/perft-children/
  legal⊆pseudo-legal invariants).
- A comprehensive mcr-vs-reference comparison harness (CPU, memory, multi-hundred
  position parity) and a Fairy-Stockfish / HaChu differential harness — both in
  separate, GPL-fenced, non-published crates (`compare`, `compare-fairy`),
  excluded from the package alongside `fuzz`.
- CI: fmt, clippy (`-D warnings`), the doc-gate, a scheduled rotating-seed
  differential fuzzer, an enforcing `cargo-mutants` gate over `src/board.rs`, and
  a line-coverage floor.

### Recent notable additions

- Large shogis **Dai** (15×15) and **Tenjiku** (16×16), validated against HaChu
  source tables; the 8-bit role field / wire-format v2 that they required.
- Western/regional long-tail: **Chancellor** (9×9), **Courier** (12×8),
  **Caparandom**, **Centaur**, **Karouk** & **Ai-Wok**, **Judkins** (6×6) &
  **Micro** (4×5) shogi, **EuroShogi** & **Checkshogi**, the Capablanca-family
  **Almost/Amazon/Gothic/Embassy/Janus/Chigorin**.
- Correctness fixes surfaced by the differential fuzzer / property tests /
  variant audits: Synochess flying-general blocker handling, Wa/Tori pinned-leaper
  check-evasion, S-House double-check gating, and notation round-trip
  disambiguation for promoted/overflow-role drops.
