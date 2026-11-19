# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Entry point: ``python3 -m push_simulator`` (from support/push_simulator) or ``run.sh``."""

import argparse
import os
import sys

from . import display, layout
from .device import PushDevice


def parse_args(argv=None):
    parser = argparse.ArgumentParser(prog="push_simulator", description="Ableton Push 1/2/3 simulator for DrivenByMoss")
    parser.add_argument("--model", choices=layout.MODELS, default="push2")
    parser.add_argument("--display-socket", default=None,
                        help="UNIX socket for the display frames (default: $DBM_PUSH_DISPLAY_SOCKET or "
                             "$XDG_RUNTIME_DIR/dbm-push-display.sock)")
    parser.add_argument("--port-style", choices=layout.PORT_STYLES, default="jack",
                        help='ALSA port names: "jack" = "<client> Live Port"/"User Port" (default), '
                             '"numbered" = "<client> MIDI 1"/"MIDI 2"')
    parser.add_argument("--client-name", default=None, help="override the ALSA client name")
    parser.add_argument("--live-port-name", default=None, help="override the name of the Live port")
    parser.add_argument("--user-port-name", default=None, help="override the name of the User port")
    parser.add_argument("--connect", action="append", default=[], metavar="PORT",
                        help="connect the port the controls send on (Push 2/3: Live, Push 1: User) both ways to "
                             "this ALSA port (name or client:port), e.g. a snd-virmidi port for Bitwig; repeatable")
    parser.add_argument("--no-alsa", action="store_true", help="no MIDI ports")
    parser.add_argument("--no-display", action="store_true", help="no display socket")
    parser.add_argument("--monitor", action="store_true", help="show the MIDI monitor")
    parser.add_argument("--screenshot", default=None, help="grab the window to this PNG before quitting")
    parser.add_argument("--quit-after", type=float, default=0.0, help="quit after this many seconds")
    parser.add_argument("--size", default=None, help="window size WxH")
    return parser.parse_args(argv)


def main(argv=None) -> int:
    args = parse_args(argv)
    from sim_common import app as sim_app

    from .qt_bridge import DisplayImageProvider, Simulator

    app = sim_app.create_app("push_simulator")
    seq = None
    port_ids: dict[str, int] = {}

    def send(data: bytes, port: str) -> None:
        if seq is not None and port in port_ids:
            try:
                seq.send(data, port_ids[port])
            except Exception as ex:  # noqa: BLE001 - shown in the monitor, the app goes on
                sim.log(f"MIDI send failed: {ex}")

    device = PushDevice(args.model, send=send)
    provider = DisplayImageProvider()
    sim = Simulator(device, provider)
    sim.monitorVisible = args.monitor

    if not args.no_alsa:
        client, live, user = layout.midi_names(args.model, args.port_style)
        primary = device.primary_ports[0]
        seq, port_ids = sim_app.open_ports(sim, args.client_name or client,
                                           {"live": args.live_port_name or live, "user": args.user_port_name or user},
                                           primary, primary, args.connect)

    server = None
    if args.model != "push1" and not args.no_display:
        path = args.display_socket or display.default_socket_path()
        server = display.DisplayServer(path, sim.post_frame, sim.post_status, sim.post_encoders)
        try:
            server.start()
        except OSError as ex:
            server = None
            sim.post_status(f"display: can't listen on {path} ({ex})")

    def cleanup() -> None:
        if server:
            server.stop()
        if seq:
            seq.close()

    return sim_app.run(app, sim, os.path.join(os.path.dirname(__file__), "qml", "Main.qml"), args,
                       image_providers={"display": provider}, cleanup=cleanup)

if __name__ == "__main__":
    sys.exit(main())
