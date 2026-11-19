# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The Qt side: exposes a ``PushDevice`` to QML (``sim``) and the display frames as an image
provider (``image://display/<n>``). MIDI from the ALSA thread and frames from the socket thread
arrive through queued signals and are handled in the GUI thread."""

import time
from collections import deque

from PySide6.QtCore import Property, QObject, QTimer, Signal, Slot
from PySide6.QtGui import QColor, QImage
from PySide6.QtQuick import QQuickImageProvider

from . import display, layout
from .device import PushDevice
from .midi import describe, hexstr

LOG_LINES = 400


def _hex_color(rgb: tuple) -> str:
    return "#%02x%02x%02x" % tuple(max(0, min(255, int(v))) for v in rgb)


class DisplayImageProvider(QQuickImageProvider):
    def __init__(self):
        super().__init__(QQuickImageProvider.ImageType.Image)
        self.image = QImage(display.WIDTH, display.HEIGHT, QImage.Format.Format_RGB32)
        self.image.fill(QColor(0, 0, 0))

    def requestImage(self, _id, size, _requested_size):
        if size is not None:
            try:
                size.setWidth(self.image.width())
                size.setHeight(self.image.height())
            except AttributeError:
                pass
        return self.image


def frame_to_qimage(frame: bytes) -> QImage:
    """An unmasked frame (BGR565 little endian, 2048 bytes per line) as a QImage: read as RGB565
    (red in the high bits) and swap red and blue."""
    image = QImage(frame, display.WIDTH, display.HEIGHT, display.LINE_BYTES, QImage.Format.Format_RGB16)
    return image.rgbSwapped().convertToFormat(QImage.Format.Format_RGB32)


class Simulator(QObject):
    revisionChanged = Signal()
    stripChanged = Signal()
    textChanged = Signal()
    frameChanged = Signal()
    logChanged = Signal()
    statusChanged = Signal()
    monitorVisibleChanged = Signal()
    encodersChanged = Signal()

    midiReceived = Signal(bytes, str)
    frameReceived = Signal(bytes)
    encodersReceived = Signal(object)
    statusReceived = Signal(str)

    def __init__(self, device: PushDevice, image_provider: DisplayImageProvider, parent=None):
        super().__init__(parent)
        self.device = device
        self.image_provider = image_provider
        self._controls = [c.as_dict() for c in device.controls]
        self._revision = 0
        self._frame_revision = 0
        self._frames = 0
        self._fps = display.FpsCounter()
        self._log: deque[str] = deque(maxlen=LOG_LINES)
        self._log_dirty = False
        self._monitor_visible = False
        self._monitor_paused = False
        self._display_status = "display: off"
        self._midi_status = "MIDI: off"
        self._dirty = set()
        self._start = time.monotonic()
        # The encoder state of the side channel (sim_values), by touch note
        self._encoders: dict[int, dict] = {}
        self._encoder_revision = 0
        device.listeners.append(self._on_device_change)
        device.monitor = self._on_monitor
        self.midiReceived.connect(self._on_midi)
        self.frameReceived.connect(self._on_frame)
        self.encodersReceived.connect(self._on_encoders)
        self.statusReceived.connect(self._on_status)
        self._timer = QTimer(self)
        self._timer.setInterval(33)
        self._timer.timeout.connect(self._tick)
        self._timer.start()

    # Thread entry points (any thread)

    def post_midi(self, data: bytes, port: str) -> None:
        self.midiReceived.emit(bytes(data), port)

    def post_frame(self, frame: bytes) -> None:
        self.frameReceived.emit(frame)

    def post_status(self, text: str) -> None:
        self.statusReceived.emit(text)

    def post_encoders(self, states) -> None:
        """The encoder state of the side channel (a list of sim_values.EncoderState), None: the
        host (dis)connected, no state."""
        self.encodersReceived.emit(states)

    # GUI thread

    def _on_midi(self, data: bytes, port: str) -> None:
        self.device.receive(data, port)

    def _on_frame(self, frame: bytes) -> None:
        self.image_provider.image = frame_to_qimage(frame)
        self._frames += 1
        self._frame_revision += 1
        self._fps.tick()
        self.frameChanged.emit()

    def _on_encoders(self, states) -> None:
        encoders = {s.index: s.as_dict() for s in states} if states else {}
        if encoders != self._encoders:
            self._encoders = encoders
            self._encoder_revision += 1
            self.encodersChanged.emit()

    def _on_status(self, text: str) -> None:
        if text.startswith("display:"):
            self._display_status = text
        else:
            self._midi_status = text
        self.log(text)
        self.statusChanged.emit()

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

    def _tick(self) -> None:
        dirty, self._dirty = self._dirty, set()
        if "leds" in dirty or self.device.has_animation():
            self._revision += 1
            self.revisionChanged.emit()
        if "strip" in dirty:
            self.stripChanged.emit()
        if "text" in dirty:
            self.textChanged.emit()
        if dirty & {"display", "settings"}:
            self.statusChanged.emit()
        if self._log_dirty and self._monitor_visible:
            self._log_dirty = False
            self.logChanged.emit()
        if int(time.monotonic() * 2) != getattr(self, "_last_half_second", None):
            self._last_half_second = int(time.monotonic() * 2)
            self.statusChanged.emit()

    # Properties

    @Property(str, constant=True)
    def model(self):
        return self.device.model

    @Property("QVariantList", constant=True)
    def controls(self):
        return self._controls

    @Property(float, constant=True)
    def surfaceWidth(self):
        return layout.SURFACE_WIDTH

    @Property(float, constant=True)
    def surfaceHeight(self):
        return layout.SURFACE_HEIGHT

    @Property(int, notify=revisionChanged)
    def revision(self):
        return self._revision

    @Property(int, notify=frameChanged)
    def frameRevision(self):
        return self._frame_revision

    @Property(int, notify=encodersChanged)
    def encoderRevision(self):
        return self._encoder_revision

    @Property(int, notify=frameChanged)
    def frames(self):
        return self._frames

    @Property("QVariantList", notify=stripChanged)
    def stripLevels(self):
        return self.device.strip_levels()

    @Property("QStringList", notify=textChanged)
    def textLines(self):
        return list(self.device.text_lines)

    @Property("QStringList", notify=logChanged)
    def logLines(self):
        return list(self._log)

    @Property(float, notify=statusChanged)
    def displayBrightness(self):
        return self.device.settings.display_brightness / 255.0

    @Property(str, notify=statusChanged)
    def statusText(self):
        fps = self._fps.fps()
        parts = [self._midi_status, self._display_status]
        if self.device.model != "push1":
            parts.append(f"frames {self._frames}  {fps:4.1f} fps")
        parts.append(" ".join(self.device.status_lines()[1:2]))
        return "   |   ".join(parts)

    def _get_monitor_visible(self):
        return self._monitor_visible

    def _set_monitor_visible(self, visible):
        if visible != self._monitor_visible:
            self._monitor_visible = visible
            self._log_dirty = True
            self.monitorVisibleChanged.emit()

    monitorVisible = Property(bool, _get_monitor_visible, _set_monitor_visible, notify=monitorVisibleChanged)

    # Slots for QML

    @Slot(int, result=str)
    def padColor(self, note):
        return _hex_color(self.device.pad_rgb(note))

    @Slot(int, result=str)
    def buttonColor(self, cc):
        return _hex_color(self.device.button_rgb(cc))

    @Slot(int, int)
    def padPress(self, note, velocity):
        self.device.press_pad(note, velocity)

    @Slot(int, int)
    def padPressure(self, note, pressure):
        self.device.pad_pressure(note, pressure)

    @Slot(int, float, float)
    def padSlide(self, note, x, y):
        self.device.pad_slide(note, x, y)

    @Slot(int)
    def padRelease(self, note):
        self.device.release_pad(note)

    @Slot(result=bool)
    def mpeActive(self):
        return self.device.mpe_active

    @Slot(int)
    def buttonPress(self, cc):
        self.device.press_button(cc)

    @Slot(int)
    def buttonRelease(self, cc):
        self.device.release_button(cc)

    @Slot(int, result="QVariantMap")
    def encoderState(self, note):
        """The state of the encoder with the touch note from the side channel: hasValue, value
        (0..1), touched, bipolar, name, text; hasValue false and empty texts without one."""
        return self._encoders.get(note) or {"hasValue": False, "value": 0.0, "touched": False, "bipolar": False, "name": "", "text": ""}

    @Slot(int, int)
    def encoderTurn(self, cc, delta):
        self.device.turn_encoder(cc, delta)

    @Slot(int, bool)
    def encoderTouch(self, note, touched):
        self.device.touch_encoder(note, touched)

    @Slot(bool)
    def stripTouch(self, touched):
        self.device.touch_strip(touched)

    @Slot(float)
    def stripMove(self, position):
        self.device.move_strip(int(position * 16383))

    @Slot()
    def clearLog(self):
        self._log.clear()
        self.logChanged.emit()

    @Slot(bool)
    def pauseLog(self, paused):
        self._monitor_paused = paused
