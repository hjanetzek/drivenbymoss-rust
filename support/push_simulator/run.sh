#!/bin/sh
# Start the Push simulator: run.sh [--model push2|push3|push1] [options], see --help
cd "$(dirname "$0")" && exec python3 -m push_simulator "$@"
