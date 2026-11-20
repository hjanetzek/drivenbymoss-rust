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
"""

import time
from collections import deque

from PySide6.QtCore import Property, QObject, QTimer, Signal, Slot

from .midi import describe, hexstr

LOG_LINES = 400

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

    def _tick(self) -> None:
        dirty, self._dirty = self._dirty, set()
        if "leds" in dirty or self.device.has_animation():
            self._revision += 1
            self.revisionChanged.emit()
        if dirty & {"display", "settings"}:
            self.statusChanged.emit()
        self.tick_dirty(dirty)
        if self._log_dirty and self._monitor_visible:
            self._log_dirty = False
            self.logChanged.emit()
        half_second = int(time.monotonic() * 2)
        if half_second != self._last_half_second:
            self._last_half_second = half_second
            self.statusChanged.emit()

    def status_parts(self) -> list[str]:
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
