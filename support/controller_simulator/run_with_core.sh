#!/bin/bash
# Start a Maschine simulator together with the Rust core (tools/dbm_host_alsa.py, with its
# simulated DAW) and the help boxes (doc/CONTROLLER_HELP_PLAN.md):
#
#   run_with_core.sh [--model jam|mk2|mk3|mikromk3|push3] [--hw|--sim] [--suffix " (sim)"] [--no-help]
#                    [--no-mirror-input] [--release]
#
# --hw (the default while the model's device is plugged in; the Mk2 and the JAM): the
#   core drives the real device in-process; the simulator mirrors it (its LEDs, strips, level
#   meters and displays, the presses on the hardware highlighted) and shows the help boxes.
#   Clicks in the simulator act on the core as the device's input (--no-mirror-input: ignored).
# --sim (the default without one): the core drives the simulator. The Push 3 (push_simulator, with
#   its display socket) runs in --sim only.
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
mirror_input=1
while [ $# -gt 0 ]; do
    case "$1" in
        --model) model="$2"; shift ;;
        --model=*) model="${1#--model=}" ;;
        --suffix) suffix="$2"; shift ;;
        --suffix=*) suffix="${1#--suffix=}" ;;
        --no-help) help=0 ;;
        --no-mirror-input) mirror_input=0 ;;
        --release) profile=release ;;
        --hw) hw=1 ;;
        --sim) hw=0 ;;
        -h|--help) sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
    shift
done
case "$model" in
    jam|mk2|mk3|mikromk3|push3) ;;
    *) echo "--model: jam, mk2, mk3, mikromk3 or push3 (other simulators: run.sh)" >&2; exit 2 ;;
esac
if [ "$model" = push3 ]; then
    [ "$hw" = 1 ] && { echo "--hw: the core drives the Mk2 and the JAM in-process only (--sim for the push3)" >&2; exit 2; }
    hw=0
fi

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
if [ "$model" = push3 ]; then
    # The Push simulator: its own client and port names with the suffix, its display socket
    display_socket="${XDG_RUNTIME_DIR:-/tmp}/dbm-push-display-$$.sock"
    client="Ableton Push 3$suffix"
    sim_package=push_simulator
    sim_dir="$repo/support/push_simulator"
    sim_args=(--model push3 --client-name "$client" --live-port-name "$client Live Port" --user-port-name "$client User Port"
              --display-socket "$display_socket" --help-socket "$socket")
    host_args=(--model push3 --port "$client Live Port" --display-socket "$display_socket" --bindings "$repo/bindings/python"
               --library "$library")
else
    display_socket=
    sim_package=maschine_simulator
    sim_dir="$here"
    sim_args=(--model "$model" --suffix "$suffix" --help-socket "$socket")
    host_args=(--model "$model" --suffix "$suffix" --bindings "$repo/bindings/python" --library "$library")
fi
if [ "$hw" = auto ]; then
    # The model's device on USB (Native Instruments 17cc: the Mk2 1140, the JAM 1500)
    case "$model" in
        jam) usb_id=17cc:1500 ;;
        mk2) usb_id=17cc:1140 ;;
        *) usb_id= ;;
    esac
    if [ -n "$usb_id" ] && lsusb 2>/dev/null | grep -qi " $usb_id"; then hw=1; else hw=0; fi
fi
if [ "$hw" = 1 ]; then
    case "$model" in
        jam|mk2) ;;
        *) echo "--hw: the core drives the Mk2 and the JAM in-process only (--sim for the $model)" >&2; exit 2 ;;
    esac
    echo "The core drives the real $model (in-process); the simulator mirrors it with the help (--sim: the simulator instead)"
    host_args+=(--transport hid)
    # The mirror over the help socket: no MIDI ports, no hello of the simulator
    sim_args+=(--no-alsa --detached)
    [ "$mirror_input" = 1 ] || sim_args+=(--no-mirror-input)
    [ "$help" = 1 ] || { echo "--hw without help: start dbm_host_alsa.py --transport hid alone" >&2; exit 2; }
fi
if [ "$help" = 1 ]; then
    sim_args+=(--show-help)
    host_args+=(--help-socket "$socket")
else
    host_args+=(--no-help)
fi
if [ "$hw" = 0 ]; then
    if [ "$model" = push3 ]; then
        echo "The core drives the simulator"
    else
        echo "The core drives the simulator (--hw: the real device)"
        host_args+=(--transport midi)
    fi
fi

(cd "$sim_dir" && exec python3 -m "$sim_package" "${sim_args[@]}") &
sim=$!
host=
cleanup() {
    [ -n "$host" ] && kill "$host" 2>/dev/null
    kill "$sim" 2>/dev/null
    rm -f "$socket"
    [ -n "$display_socket" ] && rm -f "$display_socket"
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
