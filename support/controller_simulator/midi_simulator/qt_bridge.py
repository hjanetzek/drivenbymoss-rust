# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The Qt object of the plain MIDI simulators for QML (``sim``): the SimCommon interface with the
control keys as ``cc``/``note``, the LED rings as ``encoderState`` and the faders."""

from PySide6.QtCore import Slot

from sim_common.qt_base import SimulatorBase, hex_color

from .base import GridDevice


class MidiSimulator(SimulatorBase):
    def __init__(self, device: GridDevice, parent=None):
        super().__init__(device, [c.as_dict() for c in device.controls], device.SURFACE, device.TITLE, parent)

    def tick_dirty(self, dirty: set[str]) -> None:
        if "rings" in dirty:
            self.set_encoders({c.key: self.device.encoder_state(c.key) for c in self.device.controls if c.kind == "encoder"})
        if "faders" in dirty and "leds" not in dirty:
            # the fader caps follow sim.revision
            self._revision += 1
            self.revisionChanged.emit()

    def status_parts(self) -> list[str]:
        return super().status_parts() + [self.device.status_text()]

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

    @Slot(int, int)
    def faderMove(self, key, value):
        self.device.move_fader(key, value)

    @Slot(int, result=int)
    def faderValue(self, key):
        return self.device.fader_values.get(key, 0)
