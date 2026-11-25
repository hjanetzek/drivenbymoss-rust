# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The base of the Qt objects the simulators give to QML as ``sim``.

``SimulatorBase`` wraps a device model (without Qt) which has ``listeners`` (called with what
changed: "leds", "strip", "text", "display", "settings", ...), ``monitor`` (called with direction,
port, data, note for every message), ``receive(data, port)`` and ``has_animation()``. MIDI from
the ALSA thread arrives through a queued signal (``post_midi``) and is handled in the GUI thread.

The QML components of ``SimCommon`` (Pad, LedButton, Encoder, StatusBar, MidiMonitor, SimWindow)
use this interface: ``revision`` (bumped when LEDs change or animate), ``padColor(note)``,
``buttonColor(cc)``, ``padPress/padPressure/padSlide/padRelease``, ``mpeActive()``,
``buttonPress/buttonRelease(cc)``, ``encoderTurn(cc, steps)``, ``encoderTouch(note, touched)``,
``encoderRevision``/``encoderState(note)`` (the value side channel or an LED ring),
``faderMove(cc, value)``/``faderValue(cc)`` (with ``revision``), ``statusText``,
``monitorVisible``, ``logLines``, ``controls``, ``surfaceWidth``/``surfaceHeight``, ``model``,
``title``. A control is identified by the ``cc`` and ``note`` fields of its dict in ``controls``;
what they mean is up to the subclass.

The help boxes (doc/CONTROLLER_HELP_PLAN.md, ``HelpLayer`` of SimCommon): ``helpVisible``,
``helpItems`` (with ``helpRevision``), ``helpStatus``, ``helpToggleExpanded(id)``, ``helpHideGlobal``,
``helpViewName`` / ``helpModeName`` (the legend). A subclass
enables them with ``enable_help(resolve, centre)`` (see ``sim_common.help.HelpModel``); the
entries come from the host over ``start_help_server(path)``.

The mirror of a real device (doc/CONTROLLER_HELP_PLAN.md §4) on the same socket: after the host's
``{"mirror": ...}`` line, ``{"midi": "out"}`` lines go to ``device.receive`` exactly as the MIDI of
the ALSA port, ``{"midi": "in"}`` lines (the hardware's presses) to ``device.mirror_input`` (if the
device has it), which returns the controls to show as pressed (``controlPressed(key)``, with
``revision``) and whose help box expands; they are never sent anywhere. While mirroring, the
simulator's own MIDI (``mirror_output``) goes to the host as ``{"midi": "in"}`` lines, which plays
it in as the device's input (``mirror_input_enabled``, ``--no-mirror-input``: dropped), not to the
ALSA port.
"""

import json

import time
from collections import deque

from PySide6.QtCore import Property, QObject, QTimer, Signal, Slot

from . import help as sim_help
from .midi import describe, hexstr

LOG_LINES = 400
# How long a move of the mirrored hardware (an encoder turn, a strip move) shows as pressed
MIRROR_FLASH_SECONDS = 0.2

EMPTY_ENCODER = {"hasValue": False, "value": 0.0, "touched": False, "bipolar": False, "name": "", "text": ""}


def hex_color(rgb: tuple) -> str:
    return "#%02x%02x%02x" % tuple(max(0, min(255, int(v))) for v in rgb)


class SimulatorBase(QObject):
    # A subclass needs its own notify signals for its properties: a notify signal of this base
    # class crashes PySide6 (6.12) when QML reads the subclass' property. Emit them in tick_dirty.
    revisionChanged = Signal()
    logChanged = Signal()
    statusChanged = Signal()
    monitorVisibleChanged = Signal()
    encodersChanged = Signal()
    helpChanged = Signal()
    helpVisibleChanged = Signal()
    helpLineReceived = Signal(str)
    helpConnectReceived = Signal(bool)

    midiReceived = Signal(bytes, str)
    statusReceived = Signal(str)

    def __init__(self, device, controls: list[dict], surface_size: tuple[float, float], title: str, parent=None):
        super().__init__(parent)
        self.device = device
        self._controls = controls
        self._surface_size = surface_size
        self._title = title
        self._revision = 0
        self._log: deque[str] = deque(maxlen=LOG_LINES)
        self._log_dirty = False
        self._monitor_visible = False
        self._monitor_paused = False
        self._midi_status = "MIDI: off"
        self._dirty: set[str] = set()
        self._start = time.monotonic()
        self._encoders: dict[int, dict] = {}
        self._encoder_revision = 0
        self._last_half_second = None
        self.help_model: sim_help.HelpModel | None = None
        self._help_visible = False
        self._help_dirty = False
        self._help_server: sim_help.HelpServer | None = None
        self._help_status = "help: no socket"
        # The mirror of a real device: the host's {"mirror": ...} (None: not mirroring)
        self._mirror: dict | None = None
        self.mirror_input_enabled = True
        # The controls the mirrored hardware presses: key -> None (held) or the end of a flash
        self._mirror_pressed: dict[int, float | None] = {}
        self.helpLineReceived.connect(self._on_help_line)
        self.helpConnectReceived.connect(self._on_help_connect)
        device.listeners.append(self._on_device_change)
        device.monitor = self._on_monitor
        self.midiReceived.connect(self._on_midi)
        self.statusReceived.connect(self._on_status)
        self._timer = QTimer(self)
        self._timer.setInterval(33)
        self._timer.timeout.connect(self._tick)
        self._timer.start()

    # Thread entry points (any thread)

    def post_midi(self, data: bytes, port: str) -> None:
        self.midiReceived.emit(bytes(data), port)

    def post_status(self, text: str) -> None:
        self.statusReceived.emit(text)

    # GUI thread

    def _on_midi(self, data: bytes, port: str) -> None:
        self.device.receive(data, port)

    def _on_status(self, text: str) -> None:
        self.status_message(text)
        self.log(text)
        self.statusChanged.emit()

    def status_message(self, text: str) -> None:
        """A status text of a thread (``post_status``): the MIDI status by default."""
        self._midi_status = text

    def _on_device_change(self, what: str) -> None:
        self._dirty.add(what)

    def _on_monitor(self, direction: str, port: str, data: bytes, note: str) -> None:
        if self._monitor_paused:
            return
        arrow = "<-" if direction == "in" else "->"
        text = hexstr(data) if len(data) <= 24 else hexstr(data[:24]) + f" … ({len(data)} bytes)"
        desc = note or describe(data)
        self.log(f"{arrow} {port:4s} {text:40s} {desc}")

    def log(self, line: str) -> None:
        elapsed = time.monotonic() - self._start
        self._log.append(f"{elapsed:8.3f}  {line}")
        self._log_dirty = True

    def set_encoders(self, encoders: dict[int, dict]) -> None:
        """The value side channel: the state of the encoders by their ``note`` (see EMPTY_ENCODER)."""
        if encoders != self._encoders:
            self._encoders = encoders
            self._encoder_revision += 1
            self.encodersChanged.emit()

    def tick_dirty(self, dirty: set[str]) -> None:
        """Signals of a subclass for what changed since the last tick."""

    # Help (doc/CONTROLLER_HELP_PLAN.md)

    def enable_help(self, resolve, centre=(0, 0, 0, 0)) -> None:
        """Map the host's help entries to the controls: ``resolve(entry)`` returns a
        ``sim_common.help.HelpTarget`` or None; ``centre`` is the central area (x0, y0, x1, y1,
        surface units, e.g. the pads) the sides are found around."""
        self.help_model = sim_help.HelpModel(resolve, self._surface_size, centre)

    def start_help_server(self, path: str) -> bool:
        """Listen for the host's help entries on a UNIX socket. Not if another simulator listens
        on the path already (its socket is left alone)."""
        if self.help_model is None:
            return False
        import socket
        probe = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        # A listener with a full backlog (another simulator with its host connected) makes a
        # blocking connect wait forever: a timeout counts as in use
        probe.settimeout(0.5)
        try:
            probe.connect(path)
            in_use = True
        except (socket.timeout, BlockingIOError):
            in_use = True
        except OSError:
            in_use = False
        finally:
            probe.close()
        if in_use:
            self._help_status = f"help: {path} is used by another simulator"
            self.log(self._help_status)
            return False
        self._help_server = sim_help.HelpServer(path, self.helpLineReceived.emit, self.helpConnectReceived.emit)
        try:
            self._help_server.start()
        except OSError as ex:
            self._help_server = None
            self._help_status = f"help: {ex}"
            self.log(self._help_status)
            return False
        self._help_status = f"help: waiting for the host on {path}"
        self.log(self._help_status)
        return True

    def stop_help_server(self) -> None:
        if self._help_server is not None:
            self._help_server.stop()
            self._help_server = None

    def _on_help_line(self, line: str) -> None:
        try:
            entry = json.loads(line)
        except ValueError:
            return
        if not isinstance(entry, dict):
            return
        if "midi" in entry:
            try:
                data = bytes.fromhex(entry.get("data", ""))
            except (TypeError, ValueError):
                return
            if self._mirror is not None and data:
                self.mirror_midi(entry["midi"], data)
            return
        if "mirror" in entry:
            self.start_mirror(entry)
            return
        if self.help_model is not None:
            self.help_model.apply(entry)
            self._help_dirty = True

    def _on_help_connect(self, connected: bool) -> None:
        path = self._help_server.path if self._help_server else ""
        self._help_status = "help: host connected" if connected else f"help: waiting for the host on {path}"
        if self.help_model is not None:
            # A new connection sends all entries again
            self.help_model.clear()
        self.stop_mirror()
        self._help_dirty = True

    # The mirror (doc/CONTROLLER_HELP_PLAN.md §4)

    @property
    def mirroring(self) -> bool:
        return self._mirror is not None

    def start_mirror(self, info: dict) -> None:
        """The host mirrors a real device: forget the shown state (the complete state follows)."""
        self._mirror = dict(info)
        self._mirror_pressed.clear()
        self.device.mirroring = True
        if hasattr(self.device, "mirror_reset"):
            self.device.mirror_reset()
        self.log(f"mirror: {self.mirror_text()}")
        self._dirty.add("leds")
        self.statusChanged.emit()

    def stop_mirror(self) -> None:
        if self._mirror is None:
            return
        self._mirror = None
        self._mirror_pressed.clear()
        self.device.mirroring = False
        self.log("mirror: stopped")
        self._dirty.add("leds")
        self.statusChanged.emit()

    def mirror_text(self) -> str:
        if self._mirror is None:
            return ""
        source = self._mirror.get("source") or "hardware"
        clicks = "clicks act on the core" if self.mirror_input_enabled and self._mirror.get("input", True) else "clicks ignored"
        return f"Mirror: {self._mirror.get('mirror', '?')} ({source}), {clicks}"

    def mirror_midi(self, direction: str, data: bytes) -> None:
        """A mirrored message: "out" (the core to the device) as from the ALSA port, "in" (the
        hardware's input) shown as pressed, not sent anywhere."""
        if direction == "out":
            self.device.receive(data)
            return
        if direction != "in" or not hasattr(self.device, "mirror_input"):
            return
        self._on_monitor("out", "hw", data, "")
        now = time.monotonic()
        for key, pressed in self.device.mirror_input(data):
            if pressed is False:
                self._mirror_pressed.pop(key, None)
            else:
                self._mirror_pressed[key] = None if pressed else now + MIRROR_FLASH_SECONDS
                if self.help_model is not None:
                    control = getattr(self.device, "by_key", {}).get(key)
                    rect = (control.x, control.y, control.w, control.h) if control is not None else None
                    if self.help_model.expand_control(key, rect):
                        self._help_dirty = True
            self._dirty.add("leds")

    def mirror_output(self, data: bytes) -> bool:
        """The simulator's own MIDI while mirroring: to the host as the device's input (or
        dropped); True if it was taken (it must not go to the ALSA port then)."""
        if self._mirror is None:
            return False
        if self.mirror_input_enabled and self._mirror.get("input", True) and self._help_server is not None:
            self._help_server.send(json.dumps({"midi": "in", "data": bytes(data).hex()}))
        return True

    @Slot(int, result=bool)
    def controlPressed(self, key):
        """The control is pressed (touched, moved) on the mirrored hardware."""
        return key in self._mirror_pressed

    def _get_help_visible(self):
        return self._help_visible

    def _set_help_visible(self, visible):
        if visible != self._help_visible:
            self._help_visible = visible
            self.helpVisibleChanged.emit()

    helpVisible = Property(bool, _get_help_visible, _set_help_visible, notify=helpVisibleChanged)

    @Property("QVariantList", notify=helpChanged)
    def helpItems(self):
        return self.help_model.items() if self.help_model is not None else []

    @Property(int, notify=helpChanged)
    def helpRevision(self):
        return self.help_model.revision if self.help_model is not None else 0

    @Property(str, notify=helpChanged)
    def helpStatus(self):
        return self._help_status

    @Property(str, notify=helpChanged)
    def helpViewName(self):
        return self.help_model.context.get("view", "") if self.help_model is not None else ""

    @Property(str, notify=helpChanged)
    def helpModeName(self):
        return self.help_model.context.get("mode", "") if self.help_model is not None else ""

    def _get_help_hide_global(self):
        return self.help_model.hide_global if self.help_model is not None else False

    def _set_help_hide_global(self, hide):
        if self.help_model is not None and hide != self.help_model.hide_global:
            self.help_model.set_hide_global(hide)
            self.helpChanged.emit()

    helpHideGlobal = Property(bool, _get_help_hide_global, _set_help_hide_global, notify=helpChanged)

    @Property(bool, constant=True)
    def helpAvailable(self):
        return self.help_model is not None

    @Slot(str)
    def helpToggleExpanded(self, entry_id):
        if self.help_model is not None:
            self.help_model.toggle_expanded(entry_id)
            self.helpChanged.emit()

    def _tick(self) -> None:
        if self._mirror_pressed:
            now = time.monotonic()
            ended = [key for key, until in self._mirror_pressed.items() if until is not None and until <= now]
            for key in ended:
                del self._mirror_pressed[key]
            if ended:
                self._dirty.add("leds")
        dirty, self._dirty = self._dirty, set()
        if "leds" in dirty or self.device.has_animation():
            self._revision += 1
            self.revisionChanged.emit()
        if dirty & {"display", "settings"}:
            self.statusChanged.emit()
        self.tick_dirty(dirty)
        if self._help_dirty:
            self._help_dirty = False
            self.helpChanged.emit()
        if self._log_dirty and self._monitor_visible:
            self._log_dirty = False
            self.logChanged.emit()
        half_second = int(time.monotonic() * 2)
        if half_second != self._last_half_second:
            self._last_half_second = half_second
            self.statusChanged.emit()

    def status_parts(self) -> list[str]:
        if self._mirror is not None:
            return [self.mirror_text(), self._midi_status]
        return [self._midi_status]

    # Properties

    @Property(str, constant=True)
    def model(self):
        return self.device.model

    @Property(str, constant=True)
    def title(self):
        return self._title

    @Property("QVariantList", constant=True)
    def controls(self):
        return self._controls

    @Property(float, constant=True)
    def surfaceWidth(self):
        return self._surface_size[0]

    @Property(float, constant=True)
    def surfaceHeight(self):
        return self._surface_size[1]

    @Property(int, notify=revisionChanged)
    def revision(self):
        return self._revision

    @Property(int, notify=encodersChanged)
    def encoderRevision(self):
        return self._encoder_revision

    @Property("QStringList", notify=logChanged)
    def logLines(self):
        return list(self._log)

    @Property(str, notify=statusChanged)
    def statusText(self):
        return "   |   ".join(part for part in self.status_parts() if part)

    def _get_monitor_visible(self):
        return self._monitor_visible

    def _set_monitor_visible(self, visible):
        if visible != self._monitor_visible:
            self._monitor_visible = visible
            self._log_dirty = True
            self.monitorVisibleChanged.emit()

    monitorVisible = Property(bool, _get_monitor_visible, _set_monitor_visible, notify=monitorVisibleChanged)

    # Slots for QML (defaults: nothing to do)

    @Slot(int, result="QVariantMap")
    def encoderState(self, note):
        return self._encoders.get(note) or EMPTY_ENCODER

    @Slot(int, result=str)
    def padColor(self, note):
        return "#000000"

    @Slot(int, result=str)
    def buttonColor(self, cc):
        return "#000000"

    @Slot(int, int)
    def padPress(self, note, velocity):
        pass

    @Slot(int, int)
    def padPressure(self, note, pressure):
        pass

    @Slot(int, float, float)
    def padSlide(self, note, x, y):
        pass

    @Slot(int)
    def padRelease(self, note):
        pass

    @Slot(result=bool)
    def mpeActive(self):
        return False

    @Slot(int)
    def buttonPress(self, cc):
        pass

    @Slot(int)
    def buttonRelease(self, cc):
        pass

    @Slot(int, int)
    def encoderTurn(self, cc, delta):
        pass

    @Slot(int, bool)
    def encoderTouch(self, note, touched):
        pass

    @Slot(int, int)
    def faderMove(self, key, value):
        pass

    @Slot(int, result=int)
    def faderValue(self, key):
        return 0

    @Slot()
    def clearLog(self):
        self._log.clear()
        self.logChanged.emit()

    @Slot(bool)
    def pauseLog(self, paused):
        self._monitor_paused = paused
