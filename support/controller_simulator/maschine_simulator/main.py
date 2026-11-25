# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Entry point: ``python3 -m maschine_simulator --model mk2|jam`` (from support/controller_simulator)
or ``run.sh --model mk2|jam``."""

import argparse
import os
import sys

from sim_common import app as sim_app

from . import layout
from .device import IN_PORT, OUT_PORT, MaschineDevice

SIM_SUFFIX = " (sim)"


def parse_args(argv=None):
    parser = argparse.ArgumentParser(prog="maschine_simulator", description="NI Maschine Mk2/JAM/Mk3/Mikro Mk3 simulator for "
                                     "DrivenByMoss (the MIDI of dbm-nihost / NI's driver with the DrivenByMoss templates)")
    parser.add_argument("--model", choices=layout.MODELS, default="mk2")
    parser.add_argument("--suffix", default=None,
                        help="appended to the client and port names (default: none, or \" (sim)\" if a client with the "
                             "original name exists, e.g. dbm-nihost's)")
    parser.add_argument("--input-port-name", default=None, help="override the name of the port the DAW reads from")
    parser.add_argument("--output-port-name", default=None, help="override the name of the port the DAW writes to")
    parser.add_argument("--send-disabled", action="store_true",
                        help="Mk3: also send the controls the NI template disables (the touch strip touch, as CC 2 which "
                             "the core binds); NI's driver doesn't send them")
    parser.add_argument("--detached", action="store_true",
                        help="start without the device: no hello, no answer to the query (as dbm-nihost without the device)")
    parser.add_argument("--tap", action="append", default=[], metavar="LABEL",
                        help="for automated runs: press and release the button with this label after --action-delay "
                             "(repeatable, in the order given)")
    parser.add_argument("--latch", action="append", default=[], metavar="LABEL",
                        help="for automated runs: press and hold the button with this label after --action-delay")
    parser.add_argument("--action-delay", type=float, default=1.5, help="seconds before --tap / --latch (default 1.5)")
    sim_app.add_common_args(parser)
    return parser.parse_args(argv)


def client_exists(name: str) -> bool:
    from sim_common import alsa_seq
    try:
        probe = alsa_seq.AlsaSeq("dbm-maschine-sim-probe", [])
    except alsa_seq.AlsaError:
        return False
    try:
        return any(cname == name for _, _, cname, _ in probe.list_ports())
    finally:
        probe.close()


def main(argv=None) -> int:
    args = parse_args(argv)
    from sim_common.alsa_seq import READ_CAPS, WRITE_CAPS

    from .qt_bridge import MaschineSimulator

    app = sim_app.create_app("maschine_simulator")
    seq = None
    port_ids: dict[str, int] = {}

    def send(data: bytes, port: str) -> None:
        # While mirroring a real device: to the host over the help socket (or dropped)
        if sim.mirror_output(data):
            return
        if seq is not None and port in port_ids:
            try:
                seq.send(data, port_ids[port])
            except Exception as ex:  # noqa: BLE001 - shown in the monitor, the app goes on
                sim.log(f"MIDI send failed: {ex}")

    device = MaschineDevice(args.model, send=send, send_disabled=args.send_disabled)
    sim = MaschineSimulator(device)
    sim.monitorVisible = args.monitor

    if not args.no_alsa:
        suffix = args.suffix
        if suffix is None:
            suffix = SIM_SUFFIX if client_exists(layout.PORT_NAMES[args.model][0]) else ""
            if suffix:
                sim.log(f"MIDI: '{layout.PORT_NAMES[args.model][0]}' exists (dbm-nihost?), using the suffix '{suffix}'")
        client, daw_in, daw_out = layout.midi_names(args.model, suffix)
        daw_in, daw_out = args.input_port_name or daw_in, args.output_port_name or daw_out
        if daw_in == daw_out:
            # One duplex port (Mk3, Mikro Mk3: NI's driver names in- and output alike)
            seq, port_ids = sim_app.open_ports(sim, args.client_name or client, {IN_PORT: daw_in}, IN_PORT, IN_PORT, args.connect)
        else:
            seq, port_ids = sim_app.open_ports(sim, args.client_name or client,
                                               {IN_PORT: (daw_in, READ_CAPS), OUT_PORT: (daw_out, WRITE_CAPS)},
                                               IN_PORT, OUT_PORT, args.connect)
    if not args.detached:
        device.attach()

    if args.tap or args.latch:
        from PySide6.QtCore import QTimer

        def key_of(label: str) -> int:
            matches = [c.key for c in device.controls if c.kind == "button" and label in (c.label, c.help_label)]
            if not matches:
                raise SystemExit(f"no button {label!r}")
            return matches[0]

        def act() -> None:
            for label in args.latch:
                device.press_button(key_of(label))
            for label in args.tap:
                device.press_button(key_of(label))
                device.release_button(key_of(label))

        QTimer.singleShot(int(args.action_delay * 1000), act)

    def cleanup() -> None:
        device.detach()
        if seq:
            seq.close()

    return sim_app.run(app, sim, os.path.join(os.path.dirname(__file__), "qml", "Main.qml"), args, cleanup=cleanup)


if __name__ == "__main__":
    sys.exit(main())
