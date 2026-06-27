# mce C ABI (`bindings/c`)

A C ABI for [`mce`](../..) so the chess engine can be embedded in C/C++ tools
and GUIs. It is a **separate, nested crate** (like `fuzz/` and `compare/`):
`publish = false`, a path dependency on `mce`, and never a dependency *of* `mce`.
The core `mce` library stays `unsafe`-free; the unavoidable FFI `unsafe` lives
only here, each block carrying a `// SAFETY:` comment.

## Build

```sh
cd bindings/c
cargo build            # debug: target/debug/libmce.{a,so}
cargo build --release  # release
```

`crate-type = ["staticlib", "cdylib"]`, so the build produces both a static
archive (`libmce.a`) for static linking and a shared library (`libmce.so` /
`.dylib` / `mce.dll`) for dynamic linking.

## Header

The generated header is committed at [`include/mce.h`](include/mce.h).
Regenerate it after changing the FFI surface (needs `cargo install cbindgen`):

```sh
cd bindings/c
cbindgen --config cbindgen.toml --crate mce-c --output include/mce.h
```

## Smoke test

[`test.c`](test.c) builds the startpos, prints its FEN and legal moves, checks
the legal-move count and perft, and plays Fool's mate to a checkmate outcome.

```sh
cd bindings/c
./build_test.sh        # cargo build --release, then cc test.c + libmce.a, then run
```

Manual compile/link against the static lib:

```sh
cargo build --release
cc -std=c11 -I include test.c -o test_runner \
   target/release/libmce.a -lpthread -ldl -lm
./test_runner
```

(The static archive embeds the Rust std runtime, hence `-lpthread -ldl -lm` on
Linux.)

## API

All functions operate on an opaque `McePosition*`. Variant names accept the
canonical names and aliases of `mce`'s `VariantId` (`"chess"`, `"chess960"` /
`"960"`, `"atomic"`, `"antichess"` / `"giveaway"`, `"crazyhouse"` / `"zh"`,
`"kingofthehill"` / `"koth"`, `"threecheck"` / `"3check"`, `"racingkings"` /
`"racing"`, `"horde"`).

| Function | Returns | Notes |
| --- | --- | --- |
| `mce_position_startpos(variant)` | `McePosition*` | NULL on unknown/NULL variant. Caller owns it. |
| `mce_position_new_from_fen(fen, variant)` | `McePosition*` | NULL on bad FEN/variant/NULL. Caller owns it. |
| `mce_position_free(pos)` | — | Releases the handle. `NULL` is a no-op. |
| `mce_position_to_fen(pos, buf, buflen)` | `size_t` | Needed length incl. NUL (two-call contract). |
| `mce_position_legal_moves(pos, buf, buflen)` | `size_t` | Space-separated UCI; needed length incl. NUL. |
| `mce_position_play_uci(pos, uci)` | `int` | `0` ok; `1` bad pointer/UTF-8; `2` illegal/malformed. Mutates `pos` in place. |
| `mce_position_is_check(pos)` | `int` | `1` if side to move is in check, else `0`. |
| `mce_position_outcome(pos)` | `MceOutcome` | `ONGOING`/`DRAW`/`WHITE_WINS`/`BLACK_WINS`. |
| `mce_perft(pos, depth)` | `uint64_t` | Leaf-node count; `depth == 0` returns `1`. |

### Ownership

A `McePosition*` from `mce_position_startpos` / `mce_position_new_from_fen` is
**owned by the caller** and must be released with exactly one
`mce_position_free`. Every other function only **borrows** the handle.
`mce_position_play_uci` is the only call that mutates the handle (advancing it
one ply); on a nonzero return the position is left unchanged.

### Buffer / output-string contract

`mce_position_to_fen` and `mce_position_legal_moves` use a two-call contract:
they write into `buf`/`buflen` and **return the number of bytes the full string
needs including the NUL terminator**. Pass `buf = NULL, buflen = 0` to query the
length, allocate that many bytes, then call again:

```c
size_t need = mce_position_legal_moves(pos, NULL, 0);
char *buf = malloc(need);
mce_position_legal_moves(pos, buf, need);
```

When `buflen` is too small the buffer is left holding a valid (truncated)
NUL-terminated string and the return value is still the full needed length. A
return of `0` signals an error (e.g. a NULL handle).

### Safety

No function unwinds across the FFI boundary: engine calls run inside
`catch_unwind`, and a panic becomes the documented error value (NULL / `0` /
nonzero). All pointers are null-checked and C strings are read via `CStr`.
