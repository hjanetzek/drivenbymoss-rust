# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The Qt side: exposes a ``PushDevice`` to QML (``sim``) and the display frames as an image
provider (``image://display/<n>``). MIDI from the ALSA thread and frames from the socket thread
arrive through queued signals and are handled in the GUI thread. The log, the monitor, the LED
revision and the encoder side channel state come from ``sim_common.qt_base.SimulatorBase``."""

from PySide6.QtCore import Property, Signal, Slot
from PySide6.QtGui import QColor, QImage
from PySide6.QtQuick import QQuickImageProvider

from . import display, layout
from .device import PushDevice
from sim_common.help import HelpTarget
from sim_common.qt_base import SimulatorBase, hex_color as _hex_color


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


class Simulator(SimulatorBase):
    frameChanged = Signal()
    # Own signals: a notify signal of the base class crashes PySide6 6.12 when QML reads the property
    pushStripChanged = Signal()
    pushTextChanged = Signal()
    pushDisplayChanged = Signal()

    frameReceived = Signal(bytes)
    encodersReceived = Signal(object)

    def __init__(self, device: PushDevice, image_provider: DisplayImageProvider, parent=None):
        names = {"push1": "Push 1", "push2": "Push 2", "push3": "Push 3"}
        super().__init__(device, [c.as_dict() for c in device.controls], layout.surface(device.model),
                         names[device.model] + " Simulator", parent)
        self.image_provider = image_provider
        self._frame_revision = 0
        self._frames = 0
        self._fps = display.FpsCounter()
        self._display_status = "display: off"
        self.frameReceived.connect(self._on_frame)
        self.encodersReceived.connect(self._on_encoders)
        self._init_help()

    # The help of the controls (doc/CONTROLLER_HELP_PLAN.md)

    def _init_help(self) -> None:
        """The help entries go to the controls by their MIDI binding (buttons and knobs: CCs on
        channel 1), the touch strip by its id; the pads (``grid``) and the 8 knobs above the
        display (``knobs``) are areas."""
        controls = self.device.controls
        self._help_by_binding: dict[tuple[str, int, int], tuple[int, layout.Control]] = {}
        for index, control in enumerate(controls):
            if control.kind in ("button", "encoder") and control.cc >= 0:
                self._help_by_binding.setdefault(("cc", 0, control.cc), (index, control))
        self._help_strip = next(((i, c) for i, c in enumerate(controls) if c.kind == "strip"), None)

        def area(parts):
            if not parts:
                return None
            x0, y0 = min(c.x for c in parts), min(c.y for c in parts)
            return x0, y0, max(c.x + c.w for c in parts) - x0, max(c.y + c.h for c in parts) - y0
        pads = area([c for c in controls if c.kind == "pad"])
        knobs = area([c for c in controls if c.kind == "encoder" and 71 <= c.cc <= 78])
        self._help_areas = {"grid": (pads, "bottom", "Pads"), "knobs": (knobs, "top", "Knobs")}
        centre = (pads[0], pads[1], pads[0] + pads[2], pads[1] + pads[3]) if pads else (0, 0, 0, 0)
        self.enable_help(self._help_target, centre)

    def _help_target(self, entry: dict) -> HelpTarget | None:
        if entry.get("id") in self._help_areas:
            rect, side, label = self._help_areas[entry["id"]]
            return HelpTarget(-1, *rect, side=side, label=label) if rect else None
        found = None
        if entry.get("id") == "Touchstrip":
            found = self._help_strip
        elif entry.get("bind") == "cc":
            found = self._help_by_binding.get(("cc", entry.get("channel", 0), entry.get("control", -1)))
        if found is None:
            return None
        index, control = found
        label = control.help_label or (control.label if len(control.label) > 1 else "")
        return HelpTarget(index, control.x, control.y, control.w, control.h, side=control.help_side or None, label=label)

    # Thread entry points (any thread)

    def post_frame(self, frame: bytes) -> None:
        self.frameReceived.emit(frame)

    def post_encoders(self, states) -> None:
        """The encoder state of the side channel (a list of sim_values.EncoderState), None: the
        host (dis)connected, no state."""
        self.encodersReceived.emit(states)

    # GUI thread

    def _on_frame(self, frame: bytes) -> None:
        self.image_provider.image = frame_to_qimage(frame)
        self._frames += 1
        self._frame_revision += 1
        self._fps.tick()
        self.frameChanged.emit()

    def _on_encoders(self, states) -> None:
        self.set_encoders({s.index: s.as_dict() for s in states} if states else {})

    def tick_dirty(self, dirty: set[str]) -> None:
        if "strip" in dirty:
            self.pushStripChanged.emit()
        if "text" in dirty:
            self.pushTextChanged.emit()
        if "display" in dirty:
            self.pushDisplayChanged.emit()

    def status_message(self, text: str) -> None:
        if text.startswith("display:"):
            self._display_status = text
        else:
            super().status_message(text)

    def status_parts(self) -> list[str]:
        parts = super().status_parts() + [self._display_status]
        if self.device.model != "push1":
            parts.append(f"frames {self._frames}  {self._fps.fps():4.1f} fps")
        parts.append(" ".join(self.device.status_lines()[1:2]))
        return parts

    # Properties

    @Property(int, notify=frameChanged)
    def frameRevision(self):
        return self._frame_revision

    @Property(int, notify=frameChanged)
    def frames(self):
        return self._frames

    @Property("QVariantList", notify=pushStripChanged)
    def stripLevels(self):
        return self.device.strip_levels()

    @Property("QStringList", notify=pushTextChanged)
    def textLines(self):
        return list(self.device.text_lines)

    @Property(float, notify=pushDisplayChanged)
    def displayBrightness(self):
        return self.device.settings.display_brightness / 255.0

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
