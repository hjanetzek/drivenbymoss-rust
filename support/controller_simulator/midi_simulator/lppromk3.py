# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The Novation Launchpad Pro Mk3 in programmer mode as DrivenByMoss uses it (Rust core
src/controller/launchpad/, Java controller/novation/launchpad).

- Three duplex ports like the device: "LPProMK3 MIDI" (the one the core opens: Linux discovery
  "Launchpad Pro MK3 LPProMK3 MIDI" or, with older kernels, "Launchpad Pro MK3 MIDI 1"), "LPProMK3
  DIN" and "LPProMK3 DAW" (only logged).
- At its start the core sends the device inquiry, "10 00" (leave the DAW mode, standalone) and
  "0E 01" (programmer mode) with the header ``F0 00 20 29 02 0E``, on exit "0E 00".
- Programmer mode: the pads are the notes 11..88 (row × 10 + column, 11 bottom left) with velocity
  and poly (or channel) aftertouch, released with velocity 0; all buttons are CCs (127/0): the top
  row 91..98, the left column 90 (Shift) .. 10, the scene buttons 89..19, the bottom rows 101..108
  (track select) and 1..8.
- LEDs: note or CC with the palette index, channel 1 static, 2 flashing (between the static colour
  and the new one), 3 pulsing; SysEx "03" with (type, index, data) entries: 0 static, 1 flashing
  (colour B, colour A), 2 pulsing, 3 RGB (0..127 each). The logo is index 99 ("03 00 63 <colour>").
"""

from .base import Control, GridDevice

HEADER = bytes([0xF0, 0x00, 0x20, 0x29, 0x02, 0x0E])
TOP = ((91, "◀"), (92, "▶"), (93, "Session"), (94, "Note"), (95, "Chord"), (96, "Custom"), (97, "Sequencer"),
       (98, "Projects"))
LEFT = ((90, "Shift"), (80, "▲"), (70, "▼"), (60, "Clear"), (50, "Duplicate"), (40, "Quantise"), (30, "Fixed Length"),
        (20, "Play"), (10, "Record"))
BOTTOM = ((1, "Record Arm"), (2, "Mute"), (3, "Solo"), (4, "Volume"), (5, "Pan"), (6, "Sends"), (7, "Device"),
          (8, "Stop Clip"))
LOGO = 99


class LaunchpadProMk3(GridDevice):
    MODEL = "lppromk3"
    TITLE = "Launchpad Pro Mk3 Simulator"
    SURFACE = (212.0, 226.0)
    CLIENT = "Launchpad Pro MK3"
    PORTS = {"midi": "Launchpad Pro MK3 LPProMK3 MIDI", "din": "Launchpad Pro MK3 LPProMK3 DIN",
             "daw": "Launchpad Pro MK3 LPProMK3 DAW"}
    NUMBERED_PORTS = {"midi": "Launchpad Pro MK3 MIDI 1", "din": "Launchpad Pro MK3 MIDI 2", "daw": "Launchpad Pro MK3 MIDI 3"}
    UUID = "4EDED44C-7817-4C66-A334-66A9E342AAA0"

    def build_controls(self) -> list[Control]:
        c: list[Control] = []

        def button(label, number, x, y, w=18.0, h=16.0):
            c.append(Control("button", label, x, y, w, h, "cc", 0, number, "rgb"))

        for i, (number, label) in enumerate(TOP):
            button(label, number, 24 + i * 20, 4)
        for i, (number, label) in enumerate(LEFT):
            button(label, number, 4, 4 + i * 20)
        for row in range(8):
            button("▶", 89 - row * 10, 188, 24 + row * 20)
            for col in range(8):
                c.append(Control("pad", "", 24 + col * 20, 24 + row * 20, 18, 18, "note", 0, (8 - row) * 10 + col + 1, "rgb"))
        for i in range(8):
            button(f"{i + 1}", 101 + i, 24 + i * 20, 188, 18, 14)
        for i, (number, label) in enumerate(BOTTOM):
            button(label, number, 24 + i * 20, 206, 18, 14)
        # The logo LED (no button)
        c.append(Control("button", "", 190, 6, 14, 12, "cc", 0, -1, "rgb", const="logo"))
        return c

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.logo_key = next(c.key for c in self.controls if c.const == "logo")
        self.daw_mode = None
        self.programmer = False
        self.layout = None

    def _send(self, control, value, off=False):
        # The Launchpads release notes with velocity 0
        super()._send(control, value, off=False)

    def led_for(self, kind, channel, number):
        if channel > 2:
            return None
        key = self.by_message.get((kind, 0, number))
        if key is None and kind == "cc" and number == LOGO:
            key = self.logo_key
        return (key, channel) if key is not None else None

    def animation(self, channel):
        return ("flash", 0.5) if channel == 1 else ("pulse", 1.0)

    def _index_key(self, index: int):
        if index == LOGO:
            return self.logo_key
        return self.by_message.get(("note", 0, index), self.by_message.get(("cc", 0, index)))

    def handle_sysex(self, msg, port):
        if not msg.startswith(HEADER) or len(msg) < len(HEADER) + 2:
            return ""
        command, data = msg[len(HEADER)], msg[len(HEADER) + 1:-1]
        if command == 0x0E and data:
            self.programmer = data[0] == 1
            self.update_mode()
            return "programmer mode" if self.programmer else "live mode"
        if command == 0x10 and data:
            self.daw_mode = data[0]
            self.update_mode()
            return "DAW mode on" if data[0] else "standalone (DAW mode off)"
        if command == 0x00 and data:
            self.layout = data[0]
            self.update_mode()
            return f"layout {data[0]}"
        if command == 0x03:
            return self.led_spec(data)
        return ""

    def led_spec(self, data: bytes) -> str:
        sizes = {0: 2, 1: 3, 2: 2, 3: 4}
        pos, count = 0, 0
        while pos < len(data):
            kind = data[pos]
            size = sizes.get(kind)
            if size is None or pos + size >= len(data) + 1 or pos + 1 + size > len(data):
                break
            index, values = data[pos + 1], data[pos + 2:pos + 1 + size]
            key = self._index_key(index)
            if key is not None:
                led = self.leds[key]
                if kind == 0:
                    self.set_led(key, 0, values[0])
                elif kind == 1:
                    # colour B (flashing), colour A (static)
                    self.set_led(key, 0, values[1])
                    self.set_led(key, 1, values[0])
                elif kind == 2:
                    self.set_led(key, 2, values[0])
                else:
                    led.rgb = tuple(min(255, v * 2 + (v >> 6)) for v in values)
                    led.anim_channel = 0
                    self.changed("leds")
                count += 1
            pos += 1 + size
        return f"LED update ({count} LEDs)"

    def update_mode(self):
        parts = ["programmer mode" if self.programmer else "live mode"]
        if self.daw_mode is not None:
            parts.append("DAW mode" if self.daw_mode else "standalone")
        if self.layout is not None:
            parts.append(f"layout {self.layout}")
        self.mode = ", ".join(parts)
        self.changed("settings")

    def identity_reply(self):
        # Novation (00 20 29), family 23 01, member 00 00, firmware 0 5 3 1
        return bytes([0xF0, 0x7E, 0x00, 0x06, 0x02, 0x00, 0x20, 0x29, 0x23, 0x01, 0x00, 0x00, 0x00, 0x05, 0x03, 0x01, 0xF7])

    def status_text(self):
        return self.mode or "live mode (the core switches to programmer mode)"
