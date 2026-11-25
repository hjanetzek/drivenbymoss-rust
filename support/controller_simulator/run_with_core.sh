#!/bin/bash
# Start a Maschine simulator together with the Rust core (tools/dbm_host_alsa.py, with its
# simulated DAW) and the help boxes (doc/CONTROLLER_HELP_PLAN.md):
#
#   run_with_core.sh [--model jam|mk2|mk3|mikromk3] [--hw|--sim] [--suffix " (sim)"] [--no-help]
#                    [--release]
#
# --hw (the default while a Native Instruments device is plugged in): the core drives the real
#   device in-process; the simulator shows the help boxes for it (they follow the device's
#   presses and modes; its LEDs are not mirrored yet).
# --sim (the default without one): the core drives the simulator.
# The core library is built when it is missing (cargo build --lib); --release uses
# target/release. Both processes stop together (Ctrl+C, or closing the simulator window).
set -u
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
model=jam
suffix=" (sim)"
help=1
profile=debug
hw=auto
while [ $# -gt 0 ]; do
    case "$1" in
        --model) model="$2"; shift ;;
        --model=*) model="${1#--model=}" ;;
        --suffix) suffix="$2"; shift ;;
        --suffix=*) suffix="${1#--suffix=}" ;;
        --no-help) help=0 ;;
        --release) profile=release ;;
        --hw) hw=1 ;;
        --sim) hw=0 ;;
        -h|--help) sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
    shift
done
case "$model" in
    jam|mk2|mk3|mikromk3) ;;
    *) echo "--model: jam, mk2, mk3 or mikromk3 (other simulators: run.sh)" >&2; exit 2 ;;
esac

library="$repo/target/$profile/libdrivenbymoss.so"
if [ ! -f "$library" ]; then
    echo "Building the core library ($profile) ..."
    if [ "$profile" = release ]; then
        (cd "$repo" && cargo build --lib --release -q) || exit 1
    else
        (cd "$repo" && cargo build --lib -q) || exit 1
    fi
fi

# An own socket per run, so a second setup or another simulator doesn't get in the way
socket="${XDG_RUNTIME_DIR:-/tmp}/dbm-sim-help-$model-$$.sock"
sim_args=(--model "$model" --suffix "$suffix" --help-socket "$socket")
host_args=(--model "$model" --suffix "$suffix" --bindings "$repo/bindings/python" --library "$library")
if [ "$hw" = auto ]; then
    # A Native Instruments device on USB (vendor 17cc)
    if lsusb 2>/dev/null | grep -qi " 17cc:"; then hw=1; else hw=0; fi
fi
if [ "$hw" = 1 ]; then
    echo "The core drives the real $model (in-process); the simulator shows the help (--sim: the simulator instead)"
    host_args+=(--transport hid)
    [ "$help" = 1 ] || { echo "--hw without help: start dbm_host_alsa.py --transport hid alone" >&2; exit 2; }
fi
if [ "$help" = 1 ]; then
    sim_args+=(--show-help)
    host_args+=(--help-socket "$socket")
else
    host_args+=(--no-help)
fi
if [ "$hw" = 0 ]; then
    echo "The core drives the simulator (--hw: the real device)"
    host_args+=(--transport midi)
fi

(cd "$here" && exec python3 -m maschine_simulator "${sim_args[@]}") &
sim=$!
host=
cleanup() {
    [ -n "$host" ] && kill "$host" 2>/dev/null
    kill "$sim" 2>/dev/null
    rm -f "$socket"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

# Wait until the simulator listens (its ports and the help socket exist)
for _ in $(seq 50); do
    [ -S "$socket" ] && break
    kill -0 "$sim" 2>/dev/null || { echo "The simulator did not start." >&2; exit 1; }
    sleep 0.1
done

"$repo/support/push_simulator/tools/dbm_host_alsa.py" "${host_args[@]}" &
host=$!
# Stop as soon as one of the two ends
wait -n "$sim" "$host"
