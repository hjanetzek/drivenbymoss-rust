#!/usr/bin/env python3
# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
"""Run a DrivenByMoss controller of the Rust core in the Python reference host (dbm_host, with
its simulated DAW) in real time, its MIDI port 0 connected to the simulator's ALSA ports.

    tools/dbm_host_alsa.py --model push2 \\
        --bindings ~/bastel/github/drivenbymoss-rust-push/bindings/python \\
        --library ~/bastel/github/drivenbymoss-rust-push/target/debug/libdrivenbymoss.so

The display: the bridge gets a ``usb_bulk_out(device_index, endpoint, data)`` method which writes
the bytes to the simulator's display socket (``Backend::usb_bulk_out`` of the core; it is called
once the binding forwards that callback). The encoder state of the Push 2/3 (the simulator side
channel, ``Backend.simulator_data_out``: ``DBMV`` messages) goes to the same socket between the
frames, unless ``--no-values`` or ``DBM_PUSH_SIM_VALUES=0``. ``--controller`` runs any other
controller UUID.

The Maschine simulators (support/controller_simulator, ``--model mk2|jam``): the runner reads
from their "... In"/"... Input" port and writes to "... Out"/"... Output" (with the suffix
" (sim)" which they take next to dbm-nihost, else the plain names). The Mk2's core asks at its
start whether Shift is sent (F0 7D 44 42 4D 11 F7); the simulator answers with the hello.
"""

import argparse
import os
import socket
import sys
import tempfile
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))

from push_simulator import alsa_seq, display, layout  # noqa: E402

REPO = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
CONTROLLERS = {
    # The UUIDs of the Java controller definitions
    "push1": "DBED9610-C474-11E6-9598-0800200C9A66",
    "push2": "15176AA0-C476-11E6-9598-0800200C9A66",
    "push3": "F8D561F2-722D-4DC7-975A-C562788C63FA",
    "mk2": "015E4908-C049-4808-A645-A7A44BEFE36B",
    "jam": "72F32A83-A697-446B-9016-64AA3F9476E4",
}
# The Maschine simulators (support/controller_simulator): (DAW input, DAW output) port names,
# first with the suffix the simulator takes next to dbm-nihost
MASCHINE_PORTS = {
    "mk2": ("Maschine MK2 In", "Maschine MK2 Out"),
    "jam": ("Maschine Jam - 1 Input", "Maschine Jam - 1 Output"),
}
MASCHINE_SUFFIXES = (" (sim)", "")
TICK_MS = 30


class DisplayWriter:
    """Writes the USB bulk stream to the simulator's socket; reconnects when it was closed. The
    last message of the side channel (``write_simulator``) is written again after a connect (the
    core sends it only on change)."""

    def __init__(self, path: str):
        self.path = path
        self.sock: socket.socket | None = None
        self.lock = threading.Lock()
        self.bytes = 0
        self.simulator_data: bytes | None = None

    def write_simulator(self, data: bytes) -> bool:
        with self.lock:
            self.simulator_data = bytes(data)
        return self.write(data, is_simulator_data=True)

    def write(self, data: bytes, is_simulator_data: bool = False) -> bool:
        with self.lock:
            for _ in range(2):
                try:
                    if self.sock is None:
                        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                        self.sock.connect(self.path)
                        if self.simulator_data is not None and not is_simulator_data:
                            self.sock.sendall(self.simulator_data)
                    self.sock.sendall(data)
                    self.bytes += len(data)
                    return True
                except OSError:
                    if self.sock is not None:
                        self.sock.close()
                    self.sock = None
            return False


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--model", choices=layout.MODELS + tuple(MASCHINE_PORTS), default="push2")
    parser.add_argument("--controller", default=None, help="controller UUID (default: the one of the model)")
    parser.add_argument("--bindings", default=os.path.join(REPO, "bindings", "python"),
                        help="directory with the packages drivenbymoss and dbm_host")
    parser.add_argument("--library", default=None, help="libdrivenbymoss.so (sets DBM_LIBRARY)")
    parser.add_argument("--config", default=None, help="configuration directory (default: a temporary one)")
    parser.add_argument("--display-socket", default=None)
    parser.add_argument("--port-style", choices=layout.PORT_STYLES, default="jack")
    parser.add_argument("--port", default=None, help="name of the simulator port (default: its Live port; Push 1: User port; "
                                                      "Maschine: the port the DAW reads, the other one is found by its name)")
    parser.add_argument("--suffix", default=None, help='Maschine: the suffix of the simulator\'s port names (default: " (sim)", '
                                                       'else none)')
    parser.add_argument("--no-values", action="store_true", help="do not send the encoder state (the side channel)")
    parser.add_argument("--verbose", action="store_true", help="print the MIDI")
    args = parser.parse_args()

    if args.library:
        os.environ["DBM_LIBRARY"] = os.path.abspath(args.library)
    sys.path.insert(0, os.path.abspath(args.bindings))
    import drivenbymoss
    from dbm_host import Bridge, SimDaw

    controller = args.controller or CONTROLLERS[args.model]
    known = {c.get("id", c.get("uuid", "")).upper() for c in drivenbymoss.list_controllers()}
    if known and controller.upper() not in known:
        sys.exit(f"the library has no controller {controller} (is it the build of the Push port? --library)")

    seq = alsa_seq.AlsaSeq("DrivenByMoss dbm_host", ["DrivenByMoss"])
    if args.model in MASCHINE_PORTS:
        # Two ports as NI's driver and dbm-nihost: the DAW reads "... In(put)", writes "... Out(put)"
        daw_in, daw_out = MASCHINE_PORTS[args.model]
        suffixes = (args.suffix,) if args.suffix is not None else MASCHINE_SUFFIXES
        found = None
        for suffix in suffixes:
            source = seq.find_port(args.port or daw_in + suffix)
            sink = seq.find_port(daw_out + suffix)
            if source and sink:
                found = (source, sink, suffix)
                break
        if found is None:
            sys.exit(f"the simulator ports {daw_in!r}/{daw_out!r} were not found; is the simulator running?")
        source, sink, suffix = found
        seq.connect_from(0, *source)
        seq.connect_to(0, *sink)
        print(f"connected to {source[0]}:{source[1]} '{daw_in}{suffix}' and {sink[0]}:{sink[1]} '{daw_out}{suffix}'")
    else:
        _, live, user = layout.midi_names(args.model, args.port_style)
        port_name = args.port or (user if args.model == "push1" else live)
        target = seq.find_port(port_name)
        if target is None:
            sys.exit(f"the simulator port {port_name!r} was not found; is the simulator running?")
        seq.connect_to(0, *target)
        seq.connect_from(0, *target)
        print(f"connected to {target[0]}:{target[1]} '{port_name}'")

    writer = DisplayWriter(args.display_socket or display.default_socket_path())

    send_values = not args.no_values and os.environ.get("DBM_PUSH_SIM_VALUES", "1").strip() != "0"

    class AlsaBridge(Bridge):
        def usb_bulk_out(self, device_index: int, endpoint: int, data: bytes) -> bool:
            return writer.write(bytes(data))

        def wants_simulator_data(self) -> bool:
            # The USB data always goes to the simulator here
            return send_values

        def simulator_data_out(self, data: bytes) -> bool:
            return writer.write_simulator(bytes(data))

    daw = SimDaw().build(
        tracks=[("Synth", "INSTRUMENT"), ("Drums", "INSTRUMENT"), ("Bass", "INSTRUMENT"), ("Vocals", "AUDIO"), ("Audio 5", "AUDIO")],
        effect_tracks=["FX 1", "FX 2"],
        scenes=8,
    )
    daw.tracks[0].select()
    config = args.config or tempfile.mkdtemp(prefix="dbm-push-sim-")
    bridge = AlsaBridge(daw, controller, config, "Bitwig Studio 5.3")

    def to_device(data: bytes) -> None:
        if args.verbose:
            print("->", data.hex(" "))
        seq.send(data, 0)

    bridge.on_midi_out = to_device

    def from_device(data: bytes, _port: int) -> None:
        if args.verbose:
            print("<-", data.hex(" "))
        bridge.receive(data)

    seq.start_reader(from_device)
    bridge.start()
    print(f"controller {controller} running (config {config}); Ctrl+C to stop")
    try:
        next_tick = time.monotonic()
        while True:
            next_tick += TICK_MS / 1000.0
            bridge.advance(TICK_MS)
            for line in bridge.notifications:
                print("notification:", line)
            bridge.notifications.clear()
            for line in bridge.logs:
                print("log:", line)
            bridge.logs.clear()
            delay = next_tick - time.monotonic()
            if delay > 0:
                time.sleep(delay)
            else:
                next_tick = time.monotonic()
    except KeyboardInterrupt:
        pass
    finally:
        bridge.close()
        seq.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
