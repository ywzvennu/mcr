# mce-wasm — WebAssembly bindings + browser demo

A `wasm-bindgen` shim over the [`mce`](../../) chess engine's public API, plus a
tiny dependency-free browser demo. This is a **separate nested crate**
(`publish = false`, path-dependency on `mce`); it is not part of the `mce`
package, the workspace, or the parent crate's `cargo build`/`cargo test`.

## JS API

A single `Game` class (backed by mce's runtime `AnyVariant` dispatch):

| Method | Returns | Notes |
| --- | --- | --- |
| `Game.startpos(variant?)` | `Game` | `variant` is a name like `"atomic"`, `"koth"`, `"3check"`, `"zh"`; omit for standard chess. |
| `Game.fromFen(fen, variant?)` | `Game` | Parses a six-field FEN. |
| `game.variant()` | `string` | Lowercased variant id (`"standard"`, `"atomic"`, …). |
| `game.fen()` | `string` | Current FEN. |
| `game.turn()` | `string` | `"white"` / `"black"`. |
| `game.legalMoves()` | `string[]` | UCI strings (`"e2e4"`, `"e7e8q"`). |
| `game.legalMovesSan()` | `string[]` | SAN strings. Standard / Chess960 only (throws otherwise). |
| `game.push(uci)` / `game.play(uci)` | `string` | Applies a UCI move in place, returns the new FEN. |
| `game.isCheck()` | `boolean` | |
| `game.isCheckmate()` | `boolean` | Standard mate; variant wins surface via `outcome()`. |
| `game.outcome()` | `GameOutcome \| null` | `{ kind: "decisive"\|"draw", winner: string\|null, reason: string\|null }`. |
| `game.san(uci)` | `string` | UCI → SAN. Standard / Chess960 only. |
| `game.parseSan(san)` | `string` | SAN → UCI. Standard / Chess960 only. |
| `game.zobrist()` | `string` | 16-digit hex (string, since `u64` exceeds JS exact-int range). |
| `game.perft(depth)` | `string` | Node count as a string (precision-safe). |

Every fallible method returns a `Result<_, JsError>` in Rust, i.e. it **throws a
JS exception** on bad input — nothing panics across the boundary.

## Build

### With `wasm-pack` (preferred — emits a ready-to-import `pkg/`)

```sh
cargo install wasm-pack            # one-time, if not already installed
wasm-pack build bindings/wasm --target web
```

This writes `bindings/wasm/pkg/` (`mce_wasm.js` + `mce_wasm_bg.wasm`), which the
demo's `main.js` imports. `pkg/` is gitignored.

### Without `wasm-pack` (raw module)

```sh
rustup target add wasm32-unknown-unknown
cargo build --manifest-path bindings/wasm/Cargo.toml --target wasm32-unknown-unknown
```

This produces `bindings/wasm/target/wasm32-unknown-unknown/debug/mce_wasm.wasm`
(no JS glue — use `wasm-pack`, or run `wasm-bindgen` yourself, to get the shim).

## Run the demo

After `wasm-pack build … --target web`, serve this crate directory over HTTP
(ES-module imports need a server, not `file://`):

```sh
python3 -m http.server -d bindings/wasm 8080
# then open http://localhost:8080/www/
```

The page shows the start position, lists legal moves (SAN where available),
plays a clicked or random move, supports undo, and switches variants.

## Smoke test

The crate also builds as an `rlib`, so the binding surface is tested natively:

```sh
cargo test --manifest-path bindings/wasm/Cargo.toml
```

These tests assert startpos `legalMoves().length == 20`, the standard perft
counts (20 / 400 / 8902), a SAN round-trip (`g1f3` ⇄ `Nf3`), fool's mate, and
variant/FEN round-trips.
