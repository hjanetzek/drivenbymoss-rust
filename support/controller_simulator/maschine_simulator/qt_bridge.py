# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The Qt object of the Maschine simulators for QML (``sim``): the SimCommon interface (see
sim_common.qt_base) with the control keys of layout.py as ``cc``/``note``, plus the Mk2 displays
and the JAM's touch strips and level meters."""

from PySide6.QtCore import Property, Signal, Slot

from sim_common.help import HelpTarget
from sim_common.qt_base import SimulatorBase, hex_color

from . import layout, ni
from .device import MaschineDevice

TITLES = {"mk2": "Maschine Mk2 Simulator", "jam": "Maschine JAM Simulator", "mk3": "Maschine Mk3 Simulator",
          "mikromk3": "Maschine Mikro Mk3 Simulator"}


class MaschineSimulator(SimulatorBase):
    # Own notify signals (a notify signal of the base class crashes PySide6 6.12)
    maschineTextChanged = Signal()
    maschineStripChanged = Signal()

    def __init__(self, device: MaschineDevice, parent=None):
        super().__init__(device, [c.as_dict() for c in device.controls], layout.SURFACE[device.model],
                         TITLES[device.model], parent)
        self._text_revision = 0
        self._strip_revision = 0
        self._init_help()

    # The help of the controls (doc/CONTROLLER_HELP_PLAN.md)

    def _init_help(self) -> None:
        """The help entries go to the controls by their MIDI binding (the template's message),
        the JAM's Shift (NI's SysEx) by its id, the pads (``grid``) and the strips (``strips``)
        as areas."""
        d = self.device
        self._help_by_binding: dict[tuple[str, int, int], layout.Control] = {}
        for control in d.controls:
            template = d.template(control)
            if template is None or control.kind not in ("button", "encoder"):
                continue
            self._help_by_binding.setdefault((template.kind, template.channel, template.number), control)
        def area(kind: str):
            parts = [c for c in d.controls if c.kind == kind]
            if not parts:
                return None
            x0, y0 = min(c.x for c in parts), min(c.y for c in parts)
            return x0, y0, max(c.x + c.w for c in parts) - x0, max(c.y + c.h for c in parts) - y0
        self._help_areas = {"grid": (area("pad"), "top", "Pads"), "strips": (area("strip"), "bottom", "Strips")}
        pads = area("pad")
        centre = (pads[0], pads[1], pads[0] + pads[2], pads[1] + pads[3]) if pads else (0, 0, 0, 0)
        self.enable_help(self._help_target, centre)
        if d.model == "jam":
            # The lines of the areas into the gaps between the columns (the photo's pixels, see
            # layout._jam): the strips between C and D, the group row between E and F (through
            # the bottom row and the strips), the scenes between C and D (the pads' line passes
            # between 4 and 5)
            self.help_model.area_anchors = {"strips": 577 * 0.264, "RowSelect1..RowSelect8": 829 * 0.264, "Scene1..Scene8": 577 * 0.264}

    def _help_target(self, entry: dict) -> HelpTarget | None:
        if entry.get("id") in self._help_areas:
            rect, side, label = self._help_areas[entry["id"]]
            return HelpTarget(-1, *rect, side=side, label=label) if rect else None
        control = None
        if entry.get("id") == "Shift" and self.device.shift_key >= 0:
            control = self.device.by_key[self.device.shift_key]
        elif entry.get("bind"):
            control = self._help_by_binding.get((entry["bind"], entry.get("channel", 0), entry.get("control", -1)))
        if control is None:
            return None
        label = control.help_label or (control.label if len(control.label) > 1 else "")
        return HelpTarget(control.key, control.x, control.y, control.w, control.h, side=control.help_side or None, label=label)

    def tick_dirty(self, dirty: set[str]) -> None:
        if "text" in dirty:
            self._text_revision += 1
            self.maschineTextChanged.emit()
        if "strip" in dirty:
            self._strip_revision += 1
            self.maschineStripChanged.emit()

    def status_parts(self) -> list[str]:
        d = self.device
        return super().status_parts() + ["attached (hello sent)" if d.present else "not attached"]

    @Property(int, notify=maschineTextChanged)
    def textRevision(self):
        return self._text_revision

    @Property(int, notify=maschineStripChanged)
    def stripRevision(self):
        return self._strip_revision

    # SimCommon

    @Slot(int, result=str)
    def padColor(self, key):
        return hex_color(self.device.led_rgb(key))

    @Slot(int, result=str)
    def buttonColor(self, key):
        return hex_color(self.device.led_rgb(key))

    @Slot(int, int)
    def padPress(self, key, velocity):
        self.device.press_pad(key, velocity)

    @Slot(int, int)
    def padPressure(self, key, pressure):
        self.device.pad_pressure(key, pressure)

    @Slot(int)
    def padRelease(self, key):
        self.device.release_pad(key)

    @Slot(int)
    def buttonPress(self, key):
        self.device.press_button(key)

    @Slot(int)
    def buttonRelease(self, key):
        self.device.release_button(key)

    @Slot(int, int)
    def encoderTurn(self, key, steps):
        self.device.turn_encoder(key, steps)

    @Slot(int, bool)
    def encoderTouch(self, key, touched):
        self.device.touch_encoder(key, touched)

    # Maschine

    @Slot(int, result="QStringList")
    def displayLines(self, index):
        return self.device.display_lines(index)

    @Slot(int, result="QStringList")
    def stripColours(self, index):
        return [hex_color(ni.palette_rgb(c)) for c in self.device.strip_colours(index)]

    @Slot(int, result=int)
    def meterLeds(self, index):
        return self.device.meter_leds(index)

    @Slot(int, bool)
    def stripTouch(self, index, touched):
        self.device.touch_strip(index, touched)

    @Slot(int, float)
    def stripMove(self, index, position):
        self.device.move_strip(index, round(max(0.0, min(1.0, position)) * 127))
