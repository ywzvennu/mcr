# mce-compare-fairy — perft comparison vs Fairy-Stockfish

A correctness + throughput harness that compares the `mce` chess engine's perft
node counts against [Fairy-Stockfish](https://github.com/fairy-stockfish/Fairy-Stockfish)
(FSF) on the variants both engines share, and reports head-to-head timing
(mce Mn/s vs FSF Mn/s). FSF is the **oracle** for the fairy-variants work
(issue #158); this harness validates the variants mce already shares with it.

## ⚠️ GPL fence (important)

Fairy-Stockfish is licensed **GPL-3.0-or-later**. This harness keeps the `mce`
library clean-room (MIT OR Apache-2.0) and free of any copyleft obligation by a
**structural** fence:

- **No FSF source or binary is read, copied, vendored, or linked.** The harness
  drives an externally provided `fairy-stockfish` UCI binary **purely as a
  subprocess** — it spawns the process, writes UCI commands to its stdin, and
  reads node counts from its stdout. The GPL licensing does not cross that
  process boundary.
- This is a **separate, nested crate** (`compare-fairy/`), not part of the
  published `mce` package and not in any workspace — the same arrangement as
  `compare/` and `fuzz/`. It is `publish = false` and never distributed.
- The `mce` library **does not depend on FSF** in any form. Confirm:

  ```sh
  cargo tree -e normal -p mce     # no FSF / compare-fairy
  cargo package --list            # zero compare-fairy/ files
  ```

- The FSF binary (and any clone/build under `build/`) is **git-ignored and
  never committed**.

## Usage

```sh
# Locate FSF via $MCE_FSF_BIN, then PATH, then a prebuilt build/ binary:
MCE_FSF_BIN=/path/to/fairy-stockfish cargo run --release

# Or let the harness clone + build FSF into a git-ignored build/ dir
# (needs git + make + a C++ compiler):
cargo run --release -- --build

# One ply deeper per position:
cargo run --release -- --full

# Measure mce's magic-bitboard sliders instead of hyperbola-quintessence:
cargo run --release --features magic
```

If no FSF binary can be obtained, the harness **skips gracefully** (prints
install/build instructions and exits 0) — it never blocks or fails hard on FSF
absence, so it is safe to wire into CI behind a binary that may be missing.

## What it checks

For each shared variant — standard, chess960, king-of-the-hill, three-check,
racing-kings, atomic, antichess (FSF: `giveaway`), horde, crazyhouse — it runs
perft on a basket of identical positions (reused from the mce regression tests)
at a few depths and **asserts mce's node count equals FSF's**. On a mismatch it
prints the FEN + depth and FSF's per-move divide to localise the diverging move.
It exits non-zero if any position mismatches.

### Variant + FEN dialect mapping

| mce `VariantId` | FSF `UCI_Variant` | notes |
|---|---|---|
| Standard | `chess` | identical FEN |
| Chess960 | `fischerandom` | `UCI_Chess960 true`; X-FEN castling letters pass through |
| KingOfTheHill | `kingofthehill` | identical FEN |
| ThreeCheck | `3check` | mce trailing `W+B` relocated to FSF's after-en-passant field |
| RacingKings | `racingkings` | identical FEN |
| Atomic | `atomic` | identical FEN |
| Antichess | `giveaway` | identical FEN (FSF's `antichess` is a different ruleset) |
| Horde | `horde` | identical FEN |
| Crazyhouse | `crazyhouse` | bracketed pocket `[..]` identical in both |

The harness records which FSF binary it ran against in its output header. FSF
also accepts a `variants.ini`; the shared variants here are all FSF built-ins,
so none is required.
