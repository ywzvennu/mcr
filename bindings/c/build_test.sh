#!/bin/sh
# build_test.sh — build the mcr C ABI and compile + run the C smoke test.
#
# Builds the staticlib (libmcr.a) with cargo, then compiles test.c against it
# and runs it. Requires a C compiler (cc/gcc/clang) and a Rust toolchain.
#
# Usage (from this bindings/c/ directory):
#     ./build_test.sh
#
# The header is committed at include/mcr.h; regenerate it after changing the
# FFI surface with:
#     cbindgen --config cbindgen.toml --crate mcr-c --output include/mcr.h
set -eu

DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$DIR"

CC=${CC:-cc}

echo "==> building the static library (cargo build --release)"
cargo build --release

LIB="target/release/libmcr.a"
if [ ! -f "$LIB" ]; then
    echo "error: $LIB not found" >&2
    exit 1
fi

echo "==> compiling and linking test.c with $CC"
# The static lib pulls in the Rust std runtime, hence -lpthread -ldl -lm.
"$CC" -std=c11 -Wall -Wextra -I include test.c -o target/test_runner \
    "$LIB" -lpthread -ldl -lm

echo "==> running the smoke test"
./target/test_runner
