# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Entry point: ``python3 -m push_simulator`` (from support/push_simulator) or ``run.sh``."""

import argparse
import os
import signal
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
    from PySide6.QtCore import QTimer, QUrl
    from PySide6.QtGui import QGuiApplication
    from PySide6.QtQml import QQmlApplicationEngine

    from .qt_bridge import DisplayImageProvider, Simulator

    app = QGuiApplication(sys.argv[:1])
    app.setApplicationName("push_simulator")
    signal.signal(signal.SIGINT, lambda *_: app.quit())

    seq = None
    port_by_id: dict[int, str] = {}
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
        from . import alsa_seq
        client, live, user = layout.midi_names(args.model, args.port_style)
        client = args.client_name or client
        live = args.live_port_name or live
        user = args.user_port_name or user
        try:
            seq = alsa_seq.AlsaSeq(client, [live, user])
            port_ids = {"live": seq.ports[live], "user": seq.ports[user]}
            port_by_id = {v: k for k, v in port_ids.items()}
            seq.start_reader(lambda data, port: sim.post_midi(data, port_by_id.get(port, "live")))
            sim.post_status(f"MIDI: ALSA {seq.client_id}:{port_ids['live']} '{live}', {seq.client_id}:{port_ids['user']} '{user}'")
            primary = port_ids[device.primary_ports[0]]
            for name in args.connect:
                if ":" in name and all(part.isdigit() for part in name.split(":", 1)):
                    target = tuple(int(part) for part in name.split(":", 1))
                else:
                    target = seq.find_port(name)
                if target is None:
                    sim.log(f"MIDI: port {name!r} not found")
                    continue
                seq.connect_to(primary, *target)
                seq.connect_from(primary, *target)
                sim.log(f"MIDI: connected to {target[0]}:{target[1]} ({name})")
        except alsa_seq.AlsaError as ex:
            seq = None
            sim.post_status(f"MIDI: no ALSA sequencer ({ex})")

    server = None
    if args.model != "push1" and not args.no_display:
        path = args.display_socket or display.default_socket_path()
        server = display.DisplayServer(path, sim.post_frame, sim.post_status)
        try:
            server.start()
        except OSError as ex:
            server = None
            sim.post_status(f"display: can't listen on {path} ({ex})")

    engine = QQmlApplicationEngine()
    engine.addImageProvider("display", provider)
    engine.rootContext().setContextProperty("sim", sim)
    engine.load(QUrl.fromLocalFile(os.path.join(os.path.dirname(__file__), "qml", "Main.qml")))
    if not engine.rootObjects():
        print("can't load the QML", file=sys.stderr)
        return 1
    window = engine.rootObjects()[0]
    if args.size:
        w, h = (int(v) for v in args.size.lower().split("x"))
        window.setWidth(w)
        window.setHeight(h)

    # Let Python handle signals while Qt runs
    keepalive = QTimer()
    keepalive.start(200)
    keepalive.timeout.connect(lambda: None)

    if args.quit_after > 0:
        def finish():
            if args.screenshot:
                window.grabWindow().save(args.screenshot)
                print(f"screenshot: {args.screenshot}")
            app.quit()
        QTimer.singleShot(int(args.quit_after * 1000), finish)

    try:
        result = app.exec()
    finally:
        if server:
            server.stop()
        if seq:
            seq.close()
    return result


if __name__ == "__main__":
    sys.exit(main())
