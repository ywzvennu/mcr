# mcr C ABI (`bindings/c`)

A C ABI for [`mcr`](../..) so the chess rules library can be embedded in C/C++ tools
and GUIs. It is a **separate, nested crate** (like `fuzz/` and `compare/`):
`publish = false`, a path dependency on `mcr`, and never a dependency *of* `mcr`.
The core `mcr` library stays `unsafe`-free; the unavoidable FFI `unsafe` lives
only here, each block carrying a `// SAFETY:` comment.

## Build

```sh
cd bindings/c
cargo build            # debug: target/debug/libmcr.{a,so}
cargo build --release  # release
```

`crate-type = ["staticlib", "cdylib"]`, so the build produces both a static
archive (`libmcr.a`) for static linking and a shared library (`libmcr.so` /
`.dylib` / `mcr.dll`) for dynamic linking.

## Header

The generated header is committed at [`include/mcr.h`](include/mcr.h).
Regenerate it after changing the FFI surface (needs `cargo install cbindgen`):

```sh
cd bindings/c
cbindgen --config cbindgen.toml --crate mcr-c --output include/mcr.h
```

## Smoke test

[`test.c`](test.c) builds the startpos, prints its FEN and legal moves, checks
the legal-move count and perft, and plays Fool's mate to a checkmate outcome.

```sh
cd bindings/c
./build_test.sh        # cargo build --release, then cc test.c + libmcr.a, then run
```

Manual compile/link against the static lib:

```sh
cargo build --release
cc -std=c11 -I include test.c -o test_runner \
   target/release/libmcr.a -lpthread -ldl -lm
./test_runner
```

(The static archive embeds the Rust std runtime, hence `-lpthread -ldl -lm` on
Linux.)

## API

All functions operate on an opaque `McrPosition*`. Variant names accept the
canonical names and aliases of `mcr`'s `VariantId` (`"chess"`, `"chess960"` /
`"960"`, `"atomic"`, `"antichess"` / `"giveaway"`, `"crazyhouse"` / `"zh"`,
`"kingofthehill"` / `"koth"`, `"threecheck"` / `"3check"`, `"racingkings"` /
`"racing"`, `"horde"`).

| Function | Returns | Notes |
| --- | --- | --- |
| `mcr_position_startpos(variant)` | `McrPosition*` | NULL on unknown/NULL variant. Caller owns it. |
| `mcr_position_new_from_fen(fen, variant)` | `McrPosition*` | NULL on bad FEN/variant/NULL. Caller owns it. |
| `mcr_position_free(pos)` | — | Releases the handle. `NULL` is a no-op. |
| `mcr_position_to_fen(pos, buf, buflen)` | `size_t` | Needed length incl. NUL (two-call contract). |
| `mcr_position_legal_moves(pos, buf, buflen)` | `size_t` | Space-separated UCI; needed length incl. NUL. |
| `mcr_position_play_uci(pos, uci)` | `int` | `0` ok; `1` bad pointer/UTF-8; `2` illegal/malformed. Mutates `pos` in place. |
| `mcr_position_is_check(pos)` | `int` | `1` if side to move is in check, else `0`. |
| `mcr_position_outcome(pos)` | `McrOutcome` | `ONGOING`/`DRAW`/`WHITE_WINS`/`BLACK_WINS`. |
| `mcr_position_status(pos)` | `McrStatus` | `ONGOING`/`CHECKMATE`/`STALEMATE`/`VARIANT_WIN`/`DRAW` — the consolidated `GameStatus` (issue #372). |
| `mcr_perft(pos, depth)` | `uint64_t` | Leaf-node count; `depth == 0` returns `1`. |
| `mcr_position_is_attacked(pos, square, side)` | `int` | `1`/`0` whether `side` attacks `square`; `-1` on a bad square/colour/handle. |
| `mcr_position_attackers(pos, square, side, buf, buflen)` | `size_t` | Space-separated squares of `side` pieces attacking `square` (two-call contract). |
| `mcr_position_attacks_from(pos, square, buf, buflen)` | `size_t` | Space-separated squares the piece on `square` attacks (two-call contract). |
| `mcr_position_mobility(pos, square)` | `int` | Count of squares the piece on `square` attacks; `-1` on error. |

The fairy (geometry-layer) surface mirrors the same lifecycle and adds
`mcr_fairy_position_status`; the attack-query primitives are 8x8-only, so they
have no fairy counterpart. `square` is algebraic (`"e4"`); `side` is `"white"` /
`"black"` (case-insensitive).

### End-to-end example

`test.c` is a runnable end-to-end example: it loads the start position, lists
legal moves, plays the Fool's-mate line to a checkmate, runs perft, reads the
consolidated status, and exercises the analysis queries. Run it with
`./build_test.sh`.

### Ownership

A `McrPosition*` from `mcr_position_startpos` / `mcr_position_new_from_fen` is
**owned by the caller** and must be released with exactly one
`mcr_position_free`. Every other function only **borrows** the handle.
`mcr_position_play_uci` is the only call that mutates the handle (advancing it
one ply); on a nonzero return the position is left unchanged.

### Buffer / output-string contract

`mcr_position_to_fen` and `mcr_position_legal_moves` use a two-call contract:
they write into `buf`/`buflen` and **return the number of bytes the full string
needs including the NUL terminator**. Pass `buf = NULL, buflen = 0` to query the
length, allocate that many bytes, then call again:

```c
size_t need = mcr_position_legal_moves(pos, NULL, 0);
char *buf = malloc(need);
mcr_position_legal_moves(pos, buf, need);
```

When `buflen` is too small the buffer is left holding a valid (truncated)
NUL-terminated string and the return value is still the full needed length. A
return of `0` signals an error (e.g. a NULL handle).

### Safety

No function unwinds across the FFI boundary: engine calls run inside
`catch_unwind`, and a panic becomes the documented error value (NULL / `0` /
nonzero). All pointers are null-checked and C strings are read via `CStr`.
