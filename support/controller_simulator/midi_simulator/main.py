# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Entry point: ``python3 -m midi_simulator --model apc40mk2|lppromk3`` (from
support/controller_simulator) or ``run.sh --model apc40mk2|lppromk3``."""

import argparse
import os
import sys

from sim_common import app as sim_app

from . import DEVICES


def parse_args(argv=None):
    parser = argparse.ArgumentParser(prog="midi_simulator", description="APC40 mkII / Launchpad Pro Mk3 simulator for DrivenByMoss")
    parser.add_argument("--model", choices=tuple(DEVICES), default="apc40mk2")
    parser.add_argument("--suffix", default="", help="appended to the client and port names (e.g. \" (sim)\" next to a real device)")
    parser.add_argument("--port-style", choices=("jack", "numbered"), default="jack",
                        help="Launchpad Pro Mk3: \"jack\" = \"... LPProMK3 MIDI\" (current kernels), \"numbered\" = \"... MIDI 1\"")
    parser.add_argument("--port-name", default=None, help="override the name of the port the core uses")
    parser.add_argument("--channel-pressure", action="store_true", help="send channel instead of poly aftertouch")
    sim_app.add_common_args(parser)
    return parser.parse_args(argv)


def port_names(device_class, style: str, suffix: str) -> dict[str, str]:
    ports = getattr(device_class, "NUMBERED_PORTS", None) if style == "numbered" else None
    return {key: name + suffix for key, name in (ports or device_class.PORTS).items()}


def main(argv=None) -> int:
    args = parse_args(argv)
    from .qt_bridge import MidiSimulator

    device_class = DEVICES[args.model]
    app = sim_app.create_app("midi_simulator")
    seq = None
    port_ids: dict[str, int] = {}

    def send(data: bytes, port: str) -> None:
        if seq is not None and port in port_ids:
            try:
                seq.send(data, port_ids[port])
            except Exception as ex:  # noqa: BLE001 - shown in the monitor, the app goes on
                sim.log(f"MIDI send failed: {ex}")

    device = device_class(send=send, poly_pressure=not args.channel_pressure)
    sim = MidiSimulator(device)
    sim.monitorVisible = args.monitor
    if not args.no_alsa:
        ports = port_names(device_class, args.port_style, args.suffix)
        if args.port_name:
            ports[device_class.SEND_PORT] = args.port_name
        seq, port_ids = sim_app.open_ports(sim, args.client_name or device_class.CLIENT + args.suffix, ports,
                                           device_class.SEND_PORT, device_class.SEND_PORT, args.connect)

    def cleanup() -> None:
        if seq:
            seq.close()

    return sim_app.run(app, sim, os.path.join(os.path.dirname(__file__), "qml", "Main.qml"), args, cleanup=cleanup)


if __name__ == "__main__":
    sys.exit(main())
