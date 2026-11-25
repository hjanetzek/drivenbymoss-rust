# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The application scaffolding of the simulators: the common command line options, the ALSA
client with ``--connect``, the QML engine (with the ``SimCommon`` module) and the run loop with
``--quit-after``/``--screenshot`` for offscreen runs."""

import argparse
import signal
import sys
from typing import Callable

from . import QML_IMPORT_PATH


def add_common_args(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--client-name", default=None, help="override the ALSA client name")
    parser.add_argument("--connect", action="append", default=[], metavar="PORT",
                        help="connect the port the controls send on both ways to this ALSA port (name or client:port), "
                             "e.g. a snd-virmidi port for Bitwig; repeatable")
    parser.add_argument("--no-alsa", action="store_true", help="no MIDI ports")
    parser.add_argument("--monitor", action="store_true", help="show the MIDI monitor")
    parser.add_argument("--screenshot", default=None, help="grab the window to this PNG before quitting")
    parser.add_argument("--quit-after", type=float, default=0.0, help="quit after this many seconds")
    parser.add_argument("--size", default=None, help="window size WxH")
    parser.add_argument("--help-socket", default=None, metavar="PATH",
                        help="the socket for the help of the controls from the host (default: $DBM_SIM_HELP_SOCKET, else "
                             "$XDG_RUNTIME_DIR/dbm-sim-help.sock; doc/CONTROLLER_HELP_PLAN.md)")
    parser.add_argument("--show-help", action="store_true", help="start with the help boxes shown (key H toggles them)")
    parser.add_argument("--hide-global", action="store_true", help="start with the boxes of the global functions hidden (key G)")
    parser.add_argument("--help-file", default=None, metavar="PATH",
                        help="help entries to show at the start (JSON lines as the host sends them; for tests and screenshots)")


def create_app(name: str):
    from PySide6.QtGui import QGuiApplication
    app = QGuiApplication(sys.argv[:1])
    app.setApplicationName(name)
    signal.signal(signal.SIGINT, lambda *_: app.quit())
    return app


def open_ports(sim, client: str, ports: dict[str, object], send_port: str, receive_port: str, connect: list[str]):
    """Create the ALSA client with ``ports`` (logical name -> port name or (name, capabilities)),
    hand the received MIDI to ``sim.post_midi(data, logical name)`` and connect the ``--connect``
    targets (our ``send_port`` to them, them to our ``receive_port``). Returns (seq, port ids by
    logical name), (None, {}) without a sequencer."""
    from . import alsa_seq
    try:
        seq = alsa_seq.AlsaSeq(client, list(ports.values()))
    except alsa_seq.AlsaError as ex:
        sim.post_status(f"MIDI: no ALSA sequencer ({ex})")
        return None, {}
    names = {key: spec if isinstance(spec, str) else spec[0] for key, spec in ports.items()}
    ids = {key: seq.ports[name] for key, name in names.items()}
    by_id = {v: k for k, v in ids.items()}
    seq.start_reader(lambda data, port: sim.post_midi(data, by_id.get(port, receive_port)))
    sim.post_status("MIDI: ALSA " + ", ".join(f"{seq.client_id}:{ids[k]} '{names[k]}'" for k in ports))
    for name in connect:
        target = seq.resolve(name)
        if target is None:
            sim.log(f"MIDI: port {name!r} not found")
            continue
        try:
            seq.connect_both(ids[send_port], ids[receive_port], target)
            sim.log(f"MIDI: connected to {target[0]}:{target[1]} ({name})")
        except alsa_seq.AlsaError as ex:
            sim.log(f"MIDI: {ex}")
    return seq, ids


def run(app, sim, qml_file: str, args, image_providers: dict | None = None,
        cleanup: Callable[[], None] | None = None, on_window: Callable[[object], None] | None = None) -> int:
    """Load the QML (``sim`` as context property), apply ``--size``, run until quit; ``cleanup``
    runs at the end."""
    from PySide6.QtCore import QTimer, QUrl
    from PySide6.QtQml import QQmlApplicationEngine
    # Imported so that the root object comes as a QQuickWindow (with grabWindow), not as a QWindow
    from PySide6 import QtQuick  # noqa: F401

    # The help of the controls, for simulators which map it to their controls
    if getattr(sim, "help_model", None) is not None:
        from . import help as sim_help
        sim.start_help_server(getattr(args, "help_socket", None) or sim_help.default_socket_path())
        sim.helpVisible = bool(getattr(args, "show_help", False))
        sim.helpHideGlobal = bool(getattr(args, "hide_global", False))
        if getattr(args, "help_file", None):
            with open(args.help_file, encoding="utf-8") as lines:
                for line in lines:
                    if line.strip():
                        sim.help_model.apply_line(line)

    engine = QQmlApplicationEngine()
    engine.addImportPath(QML_IMPORT_PATH)
    for name, provider in (image_providers or {}).items():
        engine.addImageProvider(name, provider)
    engine.rootContext().setContextProperty("sim", sim)
    engine.load(QUrl.fromLocalFile(qml_file))
    if not engine.rootObjects():
        print("can't load the QML", file=sys.stderr)
        if cleanup:
            cleanup()
        return 1
    window = engine.rootObjects()[0]
    if args.size:
        w, h = (int(v) for v in args.size.lower().split("x"))
        window.setWidth(w)
        window.setHeight(h)
    if on_window:
        on_window(window)

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
        return app.exec()
    finally:
        if getattr(sim, "help_model", None) is not None:
            sim.stop_help_server()
        if cleanup:
            cleanup()
