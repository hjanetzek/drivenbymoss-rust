# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The Qt object of the Maschine simulators for QML (``sim``): the SimCommon interface (see
sim_common.qt_base) with the control keys of layout.py as ``cc``/``note``, plus the Mk2 displays
and the JAM's touch strips and level meters."""

from PySide6.QtCore import Property, Signal, Slot

from sim_common.qt_base import SimulatorBase, hex_color

from . import layout, ni
from .device import MaschineDevice

TITLES = {"mk2": "Maschine Mk2 Simulator", "jam": "Maschine JAM Simulator"}


class MaschineSimulator(SimulatorBase):
    # Own notify signals (a notify signal of the base class crashes PySide6 6.12)
    maschineTextChanged = Signal()
    maschineStripChanged = Signal()

    def __init__(self, device: MaschineDevice, parent=None):
        super().__init__(device, [c.as_dict() for c in device.controls], layout.SURFACE[device.model],
                         TITLES[device.model], parent)
        self._text_revision = 0
        self._strip_revision = 0

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
