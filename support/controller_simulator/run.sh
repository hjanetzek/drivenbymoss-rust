#!/bin/sh
# Start a controller simulator: run.sh --model mk2|jam|apc40mk2|lppromk3|push2|push3|push1 [options]
# (Push: support/push_simulator, see its --help; Maschine: python3 -m maschine_simulator --help;
# APC40 mkII, Launchpad Pro Mk3: python3 -m midi_simulator --help)
here="$(cd "$(dirname "$0")" && pwd)"
model=mk2
prev=
for arg in "$@"; do
    case "$prev" in --model) model="$arg" ;; esac
    case "$arg" in --model=*) model="${arg#--model=}" ;; esac
    prev="$arg"
done
case "$model" in
    push*) exec "$here/../push_simulator/run.sh" "$@" ;;
    apc40mk2|lppromk3) cd "$here" && exec python3 -m midi_simulator "$@" ;;
    *) cd "$here" && exec python3 -m maschine_simulator "$@" ;;
esac
