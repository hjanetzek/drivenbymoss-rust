#!/bin/sh
# Builds and tests everything that builds on Linux: the Rust crate, the C++ wrapper, the NI host
# emulation (dbm-nihost) and the Reaper extension. Needs Rust, CMake, a C++17 compiler and the
# ALSA headers. Build directories go to $BUILD_DIR (default target/ci).
set -e
cd "$(dirname "$0")/.."
BUILD_DIR="${BUILD_DIR:-target/ci}"

echo "--- Rust crate"
cargo test

echo "--- C++ wrapper"
bindings/cpp/tests/run_tests.sh

echo "--- dbm-nihost"
cmake -S support/linux/ni_host -B "$BUILD_DIR/ni_host" -D CMAKE_BUILD_TYPE=Release
cmake --build "$BUILD_DIR/ni_host"
ctest --test-dir "$BUILD_DIR/ni_host" --output-on-failure

echo "--- Reaper extension"
cmake -S backends/reaper/CMake -B "$BUILD_DIR/reaper" -D CMAKE_BUILD_TYPE=Release
cmake --build "$BUILD_DIR/reaper"
