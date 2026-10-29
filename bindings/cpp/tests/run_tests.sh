#!/bin/sh
# Builds the static library, compiles the C++ wrapper test against it and runs it.
set -e
cd "$(dirname "$0")/../../.."
cargo build --lib
OUT="${TMPDIR:-/tmp}/dbm-cpp-test-$$"
mkdir -p "$OUT"
${CXX:-c++} -std=c++14 -Wall -Wextra -Werror -I include -I bindings/cpp/include \
    bindings/cpp/tests/wrapper_test.cpp target/debug/libdrivenbymoss.a \
    -lgcc_s -lutil -lrt -lpthread -lm -ldl -lc -o "$OUT/wrapper_test"
"$OUT/wrapper_test" "$OUT"
rm -rf "$OUT"
