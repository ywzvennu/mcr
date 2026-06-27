# mce — Python bindings

Fast, Rust-backed chess move generation and rules — a drop-in-flavoured
alternative to python-chess for the move-gen / rules layer, covering standard
chess, Chess960, and the major variants (atomic, antichess, crazyhouse,
king-of-the-hill, three-check, racing kings, horde).

These are [pyo3](https://pyo3.rs) bindings over the `mce` Rust engine. The
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
compile check, but `import mce` requires the maturin step (which names and
places the artifact correctly).

## Quick start

```python
import mce

pos = mce.Position()                  # standard chess start position
assert len(pos.legal_moves()) == 20   # ["a2a3", "a2a4", ...]
assert mce.perft(pos, 2) == 400

pos.push("e2e4")                      # mutate in place (UCI)
nxt = pos.play("e7e5")                # or get a new position, leaving pos as is

print(pos.fen)                        # FEN string (property)
print(pos.turn)                       # "white" / "black"
print(pos.san("g1f3"))                # "Nf3"
print(pos.parse_san("Nf3"))           # "g1f3"
print(pos.zobrist())                  # 64-bit hash
print(pos)                            # ASCII board

# Variants:
atomic = mce.Position(variant="atomic")
zh = mce.Position.startpos("crazyhouse")
```

## API

`mce.Position(fen=None, variant="chess")` / `Position.startpos(variant="chess")`

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
| `san(uci)` | `str` | |
| `parse_san(san)` | `str` | returns UCI |
| `zobrist()` | `int` | 64-bit |
| `str(pos)` | `str` | ASCII board |

`mce.perft(position, depth) -> int`

Invalid input (bad FEN, illegal/malformed UCI or SAN, unknown variant) raises
`ValueError`; nothing panics across the boundary.

## Tests

```sh
maturin develop
pytest tests/
```
