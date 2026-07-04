# mcr — Python bindings

Fast, Rust-backed chess move generation and rules — a drop-in-flavoured
alternative to python-chess for the move-gen / rules layer, covering standard
chess, Chess960, and the major variants (atomic, antichess, crazyhouse,
king-of-the-hill, three-check, racing kings, horde).

These are [pyo3](https://pyo3.rs) bindings over the `mcr` Rust engine. The
extension is built with [maturin](https://www.maturin.rs).

## Build / install

```sh
pip install maturin
cd bindings/python
maturin develop --release   # compile and install into the active venv
# or, to produce a wheel:
maturin build --release     # wheel lands in target/wheels/
```

A plain `cargo build` in this directory compiles the `cdylib` as a quick
compile check, but `import mcr` requires the maturin step (which names and
places the artifact correctly).

## Quick start

```python
import mcr

pos = mcr.Position()                  # standard chess start position
assert len(pos.legal_moves()) == 20   # ["a2a3", "a2a4", ...]
assert mcr.perft(pos, 2) == 400

pos.push("e2e4")                      # mutate in place (UCI)
nxt = pos.play("e7e5")                # or get a new position, leaving pos as is

print(pos.fen)                        # FEN string (property)
print(pos.turn)                       # "white" / "black"
print(pos.san("g1f3"))                # "Nf3"
print(pos.parse_san("Nf3"))           # "g1f3"
print(pos.zobrist())                  # 64-bit hash
print(pos)                            # ASCII board

# Variants:
atomic = mcr.Position(variant="atomic")
zh = mcr.Position.startpos("crazyhouse")
```

## API

`mcr.Position(fen=None, variant="chess")` / `Position.startpos(variant="chess")`

| member | returns | notes |
| --- | --- | --- |
| `legal_moves()` | `list[str]` | UCI |
| `legal_moves_san()` | `list[str]` | SAN |
| `push(uci)` | `None` | mutates in place |
| `play(uci)` | `Position` | new position |
| `fen` | `str` | property |
| `turn` | `str` | `"white"` / `"black"` |
| `variant` | `str` | canonical variant name |
| `is_check()` | `bool` | |
| `is_checkmate()` | `bool` | also decisive variant ends |
| `is_stalemate()` | `bool` | |
| `outcome()` | `str \| None` | `"1-0"`, `"0-1"`, `"1/2-1/2"` |
| `end_reason()` | `str \| None` | e.g. `"checkmate"`, `"stalemate"` |
| `status()` | `str` | consolidated `GameStatus`: `"ongoing"`/`"checkmate"`/`"stalemate"`/`"variant_win"`/`"draw"` |
| `is_attacked(square, color)` | `bool` | whether `color` attacks `square` (analysis, issue #373) |
| `attackers(square, color)` | `list[str]` | squares of `color` pieces attacking `square` |
| `attacks_from(square)` | `list[str]` | squares the piece on `square` attacks |
| `mobility(square)` | `int` | count of squares the piece on `square` attacks |
| `san(uci)` | `str` | |
| `parse_san(san)` | `str` | returns UCI |
| `zobrist()` | `int` | 64-bit |
| `str(pos)` | `str` | ASCII board |

`mcr.perft(position, depth) -> int`

`mcr.FairyPosition(variant, fen=None)` mirrors `Position` for the geometry-layer
fairy variants (xiangqi, shogi, janggi, …) and also exposes `status()`;
`mcr.variants()` lists the fairy names. The analysis queries are 8x8-only.
Squares are algebraic (`"e4"`); colours are `"white"` / `"black"`.

Invalid input (bad FEN, illegal/malformed UCI or SAN, unknown variant, a bad
square or colour) raises `ValueError`; nothing panics across the boundary.

## End-to-end example

Load a FEN, list moves, play one, run perft, read the status — the full loop:

```python
import mcr

pos = mcr.Position("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
print(pos.legal_moves()[:5])          # ['a2a3', 'a2a4', 'b2b3', ...]
print(pos.is_attacked("f3", "white")) # True (analysis query)
pos.push("e2e4")
print(mcr.perft(pos, 2))              # node count after 1. e4
print(pos.status())                   # 'ongoing'
```

## Tests

```sh
maturin develop
pytest tests/
```
