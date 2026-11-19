#!/bin/sh
# Start the Push simulator: run.sh [--model push2|push3|push1] [options], see --help
# The other models (mk2, jam) are handed to support/controller_simulator/run.sh.
here="$(cd "$(dirname "$0")" && pwd)"
model=push2
prev=
for arg in "$@"; do
    case "$prev" in --model) model="$arg" ;; esac
    case "$arg" in --model=*) model="${arg#--model=}" ;; esac
    prev="$arg"
done
case "$model" in
    push*) cd "$here" && exec python3 -m push_simulator "$@" ;;
    *) exec "$here/../controller_simulator/run.sh" "$@" ;;
esac
