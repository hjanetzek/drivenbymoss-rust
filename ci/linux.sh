#!/bin/sh
# Builds and tests everything that builds on Linux: the Rust crate, the C++ wrapper, the Python
# binding, the NI host emulation (dbm-nihost), the Reaper extension and the Bitwig extension.
# Needs Rust, CMake, a C++17 compiler, Python 3.11, the ALSA headers, and Maven with a JDK 25 for
# the Bitwig extension (skipped without Maven). Build directories go to $BUILD_DIR (default
# target/ci).
set -e
cd "$(dirname "$0")/.."
BUILD_DIR="${BUILD_DIR:-target/ci}"

echo "--- Rust crate"
cargo test

echo "--- C++ wrapper"
bindings/cpp/tests/run_tests.sh

echo "--- Python binding"
cargo build --lib
python3 -m unittest discover -s bindings/python/tests

echo "--- Conformance on the Python reference host (the files which pass; their *-java.ref.json are committed)"
(cd bindings/python && python3 -m conformance -q launchcontrol_xl track_mix)

echo "--- dbm-nihost"
cmake -S support/linux/ni_host -B "$BUILD_DIR/ni_host" -D CMAKE_BUILD_TYPE=Release
cmake --build "$BUILD_DIR/ni_host"
ctest --test-dir "$BUILD_DIR/ni_host" --output-on-failure

echo "--- Reaper extension"
cmake -S backends/reaper/CMake -B "$BUILD_DIR/reaper" -D CMAKE_BUILD_TYPE=Release
cmake --build "$BUILD_DIR/reaper"

echo "--- Bitwig extension"
if command -v mvn >/dev/null; then
    mvn -q -f backends/bitwig/pom.xml package
else
    echo "Maven not found, skipped"
fi
