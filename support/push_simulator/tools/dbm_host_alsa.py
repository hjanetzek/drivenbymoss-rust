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

The real Maschine Mk2 / JAM on Linux (``--transport``, default ``auto``): the core drives the
device itself through libdbm_nihost (in-process HID, no dbm-nihost, no ALSA ports), unless the
simulator's (or dbm-nihost's) ports are there: a running simulator always wins, a connected
device never takes over a simulator run. ``--transport midi`` needs the ports (as before),
``--transport hid`` never looks for them (DBM_MASCHINE_TRANSPORT=hid: the core waits for the
device, also while another process holds it). Without the library, or while dbm-nihost drives
the device, the core falls back to the ports and logs why.

The APC40 mkII, Launchpad Pro Mk3 and Launch Control XL simulators (``--model
apc40mk2|lppromk3|lcxl``): one duplex port, "APC40 mkII MIDI 1" / "Launchpad Pro MK3 LPProMK3 MIDI" /
"Launch Control XL Launch Contro" (with " (sim)" first, then without). The Maschine Mk3 and Mikro
Mk3 simulators (``--model mk3|mikromk3``) too: "Maschine MK3 Ctrl MIDI" / "Maschine Mikro MK3".

The Electra One (``--model electra``, the Mk1/Mk2 controller; ``electramini`` for the Mini): the
emulator of the real firmware (the electra-one project, ``build/electra``) is the ALSA client
"Electra Controller". The runner has two ports, "DrivenByMoss" for the core's port 0 (connected
with "Electra Port 1", the controls) and "DrivenByMoss CTRL" for its port 1 (with "Electra
CTRL", the SysEx API). ``--electra-client N`` picks the emulator by its ALSA client number (its
log says it) when several run; its ports 0 and 2 are Port 1 and CTRL.

Several controllers in one process (``--model electra --model jam``): each gets its own ALSA
client ("DrivenByMoss dbm_host MODEL") and its own controller instance of the same library, on
one simulated DAW. That is how the surface link (doc/SURFACE_LINK.md) is tried: the Electra One
provides its pages, the JAM shows them (Select + SONG).
"""

import argparse
import os
import signal
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
    "apc40mk2": "14787D10-35DE-11E4-8C21-0800200C9A66",
    "lppromk3": "4EDED44C-7817-4C66-A334-66A9E342AAA0",
    "lcxl": "5AFA5045-FF92-4737-A091-5B371CD5E529",
    "mk3": "9055C36B-0A41-48AD-8675-4D3F133E53AC",
    "mikromk3": "E87516A3-5401-44B1-9952-B4F821ED3DD5",
    "electra": "24342204-B891-4F62-BC43-8ABA1DE1D690",
    "electramini": "610CBC4D-E7D0-4D62-BCEB-97E91419C73F",
}
# The Electra One (emulator): the ports of the core's port 0 (Port 1) and port 1 (CTRL), and their
# numbers in the emulator's client
ELECTRA_PORTS = (("Electra Port 1", 0), ("Electra CTRL", 2))
ELECTRA_MODELS = ("electra", "electramini")
# The plain MIDI simulators (support/controller_simulator, midi_simulator): the duplex port the
# core opens (also searched with the suffixes of MASCHINE_SUFFIXES; the Launchpad also by its
# older kernel name)
MIDI_PORTS = {
    "apc40mk2": ("APC40 mkII MIDI 1",),
    "lppromk3": ("Launchpad Pro MK3 LPProMK3 MIDI", "Launchpad Pro MK3 MIDI 1"),
    "lcxl": ("Launch Control XL Launch Contro", "Launch Control XL MIDI 1"),
    # The Maschine Mk3 and Mikro Mk3 simulators: NI's driver names, one duplex port
    "mk3": ("Maschine MK3 Ctrl MIDI",),
    "mikromk3": ("Maschine Mikro MK3",),
}
# The Maschine simulators (support/controller_simulator): (DAW input, DAW output) port names,
# first with the suffix the simulator takes next to dbm-nihost
MASCHINE_PORTS = {
    "mk2": ("Maschine MK2 In", "Maschine MK2 Out"),
    "jam": ("Maschine Jam - 1 Input", "Maschine Jam - 1 Output"),
}
MASCHINE_SUFFIXES = (" (sim)", "")
TICK_MS = 30
# The interval of reading an in-process device (the Maschines over libdbm_nihost)
POLL_MS = 3


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


def electra_project(daw) -> None:
    """Something for each page of the Electra One: a synth with remote controls on the first
    track, an EQ+ on the second, clips, scene names and cue markers."""
    from dbm_host.device import Device, RemoteControl, eq_plus
    from dbm_host.parameter import Parameter

    names = ["Cutoff", "Resonance", "Env Amt", "Drive", "Attack", "Decay", "Sustain", "Release",
             "Osc 1", "Osc 2", "Detune", "Noise", "LFO Rate", "LFO Amt", "Glide", "Volume"]
    controls = [RemoteControl(name, Parameter(daw.preferences, value=(i % 8) / 8, name=name)) for i, name in enumerate(names)]
    daw.tracks[0].devices.add(Device("Polysynth", pages=["Filter", "Osc"], remote_controls=controls))
    daw.tracks[1].devices.add(eq_plus(daw.preferences))
    for track, slot, name in ((0, 0, "Intro"), (0, 1, "Lead"), (1, 0, "Beat"), (2, 2, "Bassline")):
        clip = daw.tracks[track].slots[slot]
        clip.create_clip(4.0)
        clip.name = name
    for index, name in enumerate(("Verse", "Chorus", "Bridge")):
        daw.scenes[index].name = name
    for index, name in enumerate(("Start", "Drop", "Outro")):
        daw.markers.add(name, index * 16.0)


def connect_model(args, model: str, client_name: str):
    """The ALSA client of one controller, connected to its simulator (or emulator)."""
    is_electra = model in ELECTRA_MODELS
    seq = alsa_seq.AlsaSeq(client_name, ["DrivenByMoss", "DrivenByMoss CTRL"] if is_electra else ["DrivenByMoss"])
    if is_electra:
        for port, (name, number) in enumerate(ELECTRA_PORTS):
            target = (args.electra_client, number) if args.electra_client is not None else seq.find_port(name)
            if target is None:
                sys.exit(f"the emulator port {name!r} was not found; is build/electra running (without --no-alsa)?")
            seq.connect_both(port, port, target)
            print(f"{model}: connected port {port} to {target[0]}:{target[1]} '{name}'")
    elif model in MASCHINE_PORTS:
        # Two ports as NI's driver and dbm-nihost: the DAW reads "... In(put)", writes "... Out(put)"
        daw_in, daw_out = MASCHINE_PORTS[model]
        if args.transport == "hid":
            print(f"{model}: the core drives the device in-process (--transport hid)")
            return seq, is_electra, True
        suffixes = (args.suffix,) if args.suffix is not None else MASCHINE_SUFFIXES
        found = None
        for suffix in suffixes:
            source = seq.find_port(args.port or daw_in + suffix)
            sink = seq.find_port(daw_out + suffix)
            if source and sink:
                found = (source, sink, suffix)
                break
        if found is None:
            if args.transport == "auto":
                # No simulator: the real device, through the core's in-process transport
                print(f"{model}: no simulator ports {daw_in!r}/{daw_out!r}: the core drives the device in-process if it can "
                      f"(see its log; --transport midi to wait for the ports)")
                return seq, is_electra, True
            sys.exit(f"the simulator ports {daw_in!r}/{daw_out!r} were not found; is the simulator running?")
        source, sink, suffix = found
        seq.connect_from(0, *source)
        seq.connect_to(0, *sink)
        print(f"{model}: connected to {source[0]}:{source[1]} '{daw_in}{suffix}' and {sink[0]}:{sink[1]} '{daw_out}{suffix}'")
    elif model in MIDI_PORTS:
        suffixes = (args.suffix,) if args.suffix is not None else MASCHINE_SUFFIXES
        candidates = [args.port] if args.port else [name + suffix for suffix in suffixes for name in MIDI_PORTS[model]]
        found = next(((name, seq.find_port(name)) for name in candidates if seq.find_port(name)), None)
        if found is None:
            sys.exit(f"the simulator port {candidates[0]!r} was not found; is the simulator running?")
        port_name, target = found
        seq.connect_to(0, *target)
        seq.connect_from(0, *target)
        print(f"{model}: connected to {target[0]}:{target[1]} '{port_name}'")
    else:
        _, live, user = layout.midi_names(model, args.port_style)
        port_name = args.port or (user if model == "push1" else live)
        target = seq.find_port(port_name)
        if target is None:
            sys.exit(f"the simulator port {port_name!r} was not found; is the simulator running?")
        seq.connect_to(0, *target)
        seq.connect_from(0, *target)
        print(f"{model}: connected to {target[0]}:{target[1]} '{port_name}'")
    return seq, is_electra, False


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--model", choices=layout.MODELS + tuple(MASCHINE_PORTS) + tuple(MIDI_PORTS) + ELECTRA_MODELS, action="append",
                        help="the controller (default push2); several --model run several controllers in one process (one "
                             "library, one simulated DAW), e.g. --model electra --model jam for the surface link")
    parser.add_argument("--controller", default=None, help="controller UUID (default: the one of the model; with one --model only)")
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
    parser.add_argument("--electra-client", type=int, default=None, help="Electra One: the ALSA client number of the emulator")
    parser.add_argument("--transport", choices=("auto", "hid", "midi"), default="auto",
                        help="Maschine Mk2/JAM: auto (default): the simulator's ports if they are there, else the real device "
                             "in-process (libdbm_nihost); hid: the real device in-process; midi: the ports only")
    parser.add_argument("--host-pages", action="store_true",
                        help="the host provides parameter pages (dbm_host/host_page.py, doc/HOST_PAGE_PLAN.md): the JAM shows them "
                             "with Select + SONG; the simulated screen's selection moves with SIGUSR1 (next page) and SIGUSR2 "
                             "(previous page), and is printed when it changes")
    parser.add_argument("--verbose", action="store_true", help="print the MIDI")
    args = parser.parse_args()
    models = args.model or ["push2"]
    if len(set(models)) != len(models):
        sys.exit("each --model only once")
    if args.controller and len(models) > 1:
        sys.exit("--controller works with one --model only")

    if args.library:
        os.environ["DBM_LIBRARY"] = os.path.abspath(args.library)
    sys.path.insert(0, os.path.abspath(args.bindings))
    import drivenbymoss
    from dbm_host import Bridge, HostPages, SimDaw

    known = {c.get("id", c.get("uuid", "")).upper() for c in drivenbymoss.list_controllers()}
    for model in models:
        controller = args.controller or CONTROLLERS[model]
        if known and controller.upper() not in known:
            sys.exit(f"the library has no controller {controller} (is it the build of the Push port? --library)")

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
    if any(model in ELECTRA_MODELS for model in models):
        electra_project(daw)
    screen_steps = []
    if args.host_pages:
        daw.host_pages = HostPages(daw)
        # The simulated screen of the host: its own encoders move the selection
        signal.signal(signal.SIGUSR1, lambda *_: screen_steps.append(1))
        signal.signal(signal.SIGUSR2, lambda *_: screen_steps.append(-1))
        print(f"host pages: SIGUSR1 / SIGUSR2 to pid {os.getpid()} move the screen's selection")
    last_screen = None

    # One ALSA client and one bridge per controller; all share the library (and with it the
    # registry of the surface link) and the simulated DAW
    runners = []
    for model in models:
        controller = args.controller or CONTROLLERS[model]
        client_name = "DrivenByMoss dbm_host" if len(models) == 1 else f"DrivenByMoss dbm_host {model}"
        seq, is_electra, in_process = connect_model(args, model, client_name)
        if in_process and args.transport == "hid":
            os.environ["DBM_MASCHINE_TRANSPORT"] = "hid"
        config = args.config or tempfile.mkdtemp(prefix=f"dbm-{model}-sim-")
        if args.config and len(models) > 1:
            config = os.path.join(args.config, model)
            os.makedirs(config, exist_ok=True)
        bridge = AlsaBridge(daw, controller, config, "Bitwig Studio 5.3")
        # The core drives the real device (or falls back to the ports, which are not connected)
        bridge.device_transport = in_process

        def to_device(port: int, data: bytes, seq=seq, is_electra=is_electra, model=model) -> None:
            if args.verbose:
                print(f"{model} ->{port}", data.hex(" "))
            seq.send(data, port if is_electra else 0)

        bridge.on_port_midi_out = to_device

        def from_device(data: bytes, port: int, bridge=bridge, is_electra=is_electra, model=model) -> None:
            if args.verbose:
                print(f"{model} <-{port}", data.hex(" "))
            bridge.receive(data, port if is_electra else 0)

        seq.start_reader(from_device)
        bridge.start()
        print(f"{model}: controller {controller} running (config {config})")
        runners.append((model, seq, bridge))
    print("Ctrl+C to stop")
    try:
        next_tick = time.monotonic()
        while True:
            next_tick += TICK_MS / 1000.0
            if len(runners) == 1:
                runners[0][2].advance(TICK_MS)
            else:
                # The DAW moves once per tick, each controller steps once
                daw.advance(TICK_MS)
                for _, _, bridge in runners:
                    bridge.clock_ms += TICK_MS
                    bridge.step()
            if daw.host_pages is not None:
                pages = daw.host_pages
                while screen_steps:
                    pages.step_on_screen(screen_steps.pop(0))
                all_pages = pages.pages()
                if all_pages:
                    section, page = all_pages[pages.screen.page]
                    control = page.controls[pages.screen.slot] if pages.screen.slot < len(page.controls) else None
                    screen = (pages.chain.name, pages.chain.sections[section].name, page.title, control.name if control else "-",
                              control.text if control else "")
                    if screen != last_screen:
                        last_screen = screen
                        print("host screen: {} · {} · {} · {} {}".format(*screen))
            for model, _, bridge in runners:
                prefix = "" if len(runners) == 1 else f"{model}: "
                for line in bridge.notifications:
                    print(f"{prefix}notification:", line)
                bridge.notifications.clear()
                for line in bridge.logs:
                    print(f"{prefix}log:", line)
                bridge.logs.clear()
            delay = next_tick - time.monotonic()
            # An in-process device is read every few ms (its pads play without the tick's delay)
            polled = [bridge for _, _, bridge in runners if bridge.device_transport and bridge.app is not None]
            while polled and delay > POLL_MS / 1000.0:
                time.sleep(POLL_MS / 1000.0)
                for bridge in polled:
                    bridge.app.poll_devices()
                delay = next_tick - time.monotonic()
            if delay > 0:
                time.sleep(delay)
            else:
                next_tick = time.monotonic()
    except KeyboardInterrupt:
        pass
    finally:
        for _, seq, bridge in runners:
            bridge.close()
            seq.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
