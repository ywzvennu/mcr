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

## HaChu — the large-shogi differential oracle (issue #379)

Large shogi (Chu / Dai / Tenjiku) is **not** covered by Fairy-Stockfish.
[HaChu](https://github.com/ddugovic/hachu) (H.G. Muller's reference engine) is
the oracle for those, and is wired in here as a **second** differential oracle.

The **same GPL-style fence** applies: HaChu is driven **purely as a subprocess**
— never linked, never source-copied into `mce` or this crate. It speaks the
**XBoard/WinBoard (CECP)** protocol (not UCI), so it has its own locator
(`locate_hachu.rs`) and driver (`xboard.rs`). Any cloned/compiled HaChu lives
under the git-ignored `build/hachu/` dir and is never committed. (Upstream HaChu
is CC0 / public domain, but it is fenced as a subprocess oracle regardless.)

```sh
# Point at an existing hachu binary:
MCE_HACHU_BIN=/path/to/hachu cargo run --release -- --hachu

# Or let the harness clone + build HaChu into build/hachu/ (needs git + make +
# a C compiler; HaChu builds from a few plain C files with one Makefile):
cargo run --release -- --hachu --build-hachu

# Build it manually:
git clone https://github.com/ddugovic/hachu
cd hachu && make hachu
MCE_HACHU_BIN=$PWD/hachu cargo run --release -- --hachu
```

If no HaChu binary can be obtained, `--hachu` **skips gracefully** (prints
build instructions and exits 0), exactly like the FSF-absent skip.

### What `--hachu` does today, and what is gated on #380

The mode locates/builds HaChu, completes the `protover 2` handshake, captures
its advertised `variants="..."` list, confirms the large-shogi variants
(`chu`, `dai`, `tenjiku`) are present, and drives HaChu to a concrete
large-shogi position (the Chu-Shogi start position via `variant chu`),
confirming the oracle is positioned and responsive.

The **node-by-node perft/divide comparison** is a **scaffold gated on issue
#380**. HaChu has no native `perft` command, so a perft is driven *externally*:
the harness walks the move tree itself and uses HaChu as a per-move oracle
(`usermove` legality via `xboard.rs`). That walk needs mce's own **Chu-Shogi
move generator** to compare against — and mce currently has only the 12x12
`Chu12x12` board geometry, not a Chu-Shogi *variant*. Adding those rules is
issue #380. Until then the mce side is gated behind the `large-shogi` cargo
feature and `--hachu` reports the oracle as **READY** instead of running an
unbacked comparison. Once #380 lands, enable it with
`cargo run --release -- --hachu --features large-shogi` and wire the mce perft
into `hachu.rs` (the integration point is marked there).

The `mce` library still **does not depend on HaChu** in any form (it is only a
subprocess spawned by this nested, unpublished crate).
