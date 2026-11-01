#!/bin/sh
# Requires Rust (cargo, https://rustup.rs), CMake and a C++ compiler

mkdir -p build
cmake -S "CMake" -B "build" -D CMAKE_BUILD_TYPE=Release
cmake --build "build" --verbose
