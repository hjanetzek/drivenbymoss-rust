# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The Novation Launch Control XL as DrivenByMoss uses it (Rust core src/controller/launchcontrol/,
Java controller/novation/launchcontrol).

- One duplex port; the core's Linux discovery names are "Launch Control XL" and "Launch Control
  XL Launch Contro" (the kernel's port name, cut to 31 characters), the default here.
- 16 templates: 8 user templates (MIDI channels 1-8) and 8 factory templates (channels 9-16).
  Every control sends on the channel of the active template, and the LEDs are set on it. At its
  start the core selects factory template 1 with ``F0 00 20 29 02 11 77 08 F7``; the device
  switches and answers with the same message (a template change on the device sends it too), and
  only then the core binds its controls to the template's channel and lights the LEDs. Factory
  templates 1-5 select the core's modes (mixer, equalizer, layer mixer, drum and note sequencer).
  The simulator has a template selector below the controls (on the device: Templates + a button).
- Knobs: Send A CC 13..20, Send B CC 29..36, Pan CC 49..56 (absolute); faders CC 77..84.
- Buttons: Track Focus notes 41..44, 57..60, Track Control notes 73..76, 89..92, Device/Mute/Solo/
  Record Arm notes 105..108 (on 127, off: note off 0), Send ▲▼ and Track ◀▶ CC 104..107 (127/0).
- LEDs: two-colour red/green, velocity = red (bits 0-1) + flags (bits 2-3: 4 copy, 8 clear) +
  16 × green (bits 4-5), 0..3 each; the knob LEDs are the notes 13 + row + 16 × column (13, 29 .. 125 the first row), the buttons
  their own note or CC. Flags 8 without copy (8 + colour) flash when flashing is on (CC 0 = 0x28 on
  the template's channel); CC 0 = 0 resets the template's LEDs. SysEx LED update
  ``F0 00 20 29 02 11 78 <template> <index> <value> F7`` (index 0-23 knobs, 24-31 Track Focus,
  32-39 Track Control, 40-43 Device/Mute/Solo/Record Arm, 44-47 ▲▼◀▶).
"""

from .base import CC, NOTE_OFF, NOTE_ON, Control, GridDevice, Led

HEADER = bytes([0xF0, 0x00, 0x20, 0x29, 0x02, 0x11])
TEMPLATE_CHANGE = 0x77
LED_UPDATE = 0x78
FACTORY = 8

KNOB_ROWS = ((13, "LAUNCHCONTROL_KNOB_SEND_A_1"), (29, "LAUNCHCONTROL_KNOB_SEND_B_1"),
             (49, "LAUNCHCONTROL_KNOB_PAN_1"))  # first CC, Rust constant


def knob_led_note(row: int, column: int) -> int:
    """The note of a knob LED: 13 + row + 16 × column (``set_knob_led_color``), so the first row is
    13, 29, 45 .. 125, the second 14, 30 .., the third 15, 31 .."""
    return 13 + row + 16 * column
FOCUS = [41, 42, 43, 44, 57, 58, 59, 60]
CONTROL = [73, 74, 75, 76, 89, 90, 91, 92]
SIDE = ((105, "Device", "LAUNCHCONTROL_DEVICE"), (106, "Mute", "LAUNCHCONTROL_MUTE"), (107, "Solo", "LAUNCHCONTROL_SOLO"),
        (108, "Record Arm", "LAUNCHCONTROL_RECORD_ARM"))
ARROWS = ((104, "▲", "LAUNCHCONTROL_SEND_PREV"), (105, "▼", "LAUNCHCONTROL_SEND_NEXT"),
          (106, "◀", "LAUNCHCONTROL_TRACK_PREV"), (107, "▶", "LAUNCHCONTROL_TRACK_NEXT"))


def decode_led(value: int) -> tuple[int, int, int]:
    """(red 0..3, green 0..3, flags 0..3) of an LED velocity."""
    return value & 3, (value >> 4) & 3, (value >> 2) & 3


def led_rgb(red: int, green: int) -> tuple[int, int, int]:
    """The colour of a red/green LED: red, green and their mixtures (amber, yellow)."""
    level = (0, 90, 170, 255)
    r, g = level[red], level[green]
    return (max(r, g // 3) if r else 0, g, 0)


class LaunchControlXl(GridDevice):
    MODEL = "lcxl"
    TITLE = "Launch Control XL Simulator"
    SURFACE = (264.0, 170.0)
    CLIENT = "Launch Control XL"
    PORTS = {"midi": "Launch Control XL Launch Contro"}
    NUMBERED_PORTS = {"midi": "Launch Control XL MIDI 1"}
    UUID = "5AFA5045-FF92-4737-A091-5B371CD5E529"

    def build_controls(self) -> list[Control]:
        c: list[Control] = []
        for row, (cc, const) in enumerate(KNOB_ROWS):
            for i in range(8):
                x = 8 + i * 26
                c.append(Control("encoder", "", x + 4, 8 + row * 22, 16, 16, "cc", 0, cc + i, "none", const=const))
                # The knob's LED: a button without a message, lit by the note
                c.append(Control("button", "", x + 21, 8 + row * 22, 4, 4, "led", 0, knob_led_note(row, i), "rg", const="knob-led"))
        for i in range(8):
            x = 8 + i * 26
            c.append(Control("fader", "", x + 7, 76, 10, 40, "cc", 0, 77 + i, "none", const="LAUNCHCONTROL_FADER_1"))
            c.append(Control("button", "Focus", x + 1, 122, 22, 10, "note", 0, FOCUS[i], "rg",
                             const="LAUNCHCONTROL_TRACK_FOCUS_1" if i < 4 else "LAUNCHCONTROL_TRACK_FOCUS_5"))
            c.append(Control("button", "Control", x + 1, 136, 22, 10, "note", 0, CONTROL[i], "rg",
                             const="LAUNCHCONTROL_TRACK_CONTROL_1" if i < 4 else "LAUNCHCONTROL_TRACK_CONTROL_5"))
        for (number, label, const), (x, y) in zip(ARROWS, ((226, 8), (226, 20), (214, 34), (238, 34))):
            c.append(Control("button", label, x, y, 20, 10, "cc", 0, number, "rg", const=const))
        for i, (number, label, const) in enumerate(SIDE):
            c.append(Control("button", label, 220, 82 + i * 16 if i < 2 else 122 + (i - 2) * 14, 36, 10, "note", 0, number, "rg",
                             const=const))
        # The template selector (on the device: Templates + a Track Focus/Control button)
        for t in range(16):
            label = f"U{t + 1}" if t < FACTORY else f"F{t - FACTORY + 1}"
            c.append(Control("button", label, 8 + t * 15.5, 154, 14, 10, "template", 0, t, "rgb", const="template"))
        return c

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.template = 0
        self.templates = {t: {key: Led() for key in self.leds} for t in range(16)}
        self.leds = self.templates[0]
        self.flashing = {t: False for t in range(16)}
        self.template_keys = {c.number: c.key for c in self.controls if c.msg == "template"}
        self.led_index = self._led_index()

    def _led_index(self) -> list[int]:
        """The keys of the SysEx LED indices 0-47."""
        def key(msg, number):
            return self.by_message[(msg, 0, number)]
        knobs = [key("led", knob_led_note(row, i)) for row in range(3) for i in range(8)]
        return (knobs + [key("note", n) for n in FOCUS] + [key("note", n) for n in CONTROL]
                + [key("note", n) for n, _, _ in SIDE] + [key("cc", n) for n, _, _ in ARROWS])

    # Controls: on the channel of the active template

    def _send(self, control, value, off=False):
        value = max(0, min(127, int(value)))
        ch = self.template
        if control.msg == "cc":
            self.emit(bytes([CC | ch, control.number, value]))
        else:
            self.emit(bytes([(NOTE_OFF if off else NOTE_ON) | ch, control.number, value]))

    def press_button(self, key):
        control = self.by_key.get(key)
        if control and control.msg == "template":
            self.select_template(control.number, announce=True)
            return
        if control and control.msg != "led":
            super().press_button(key)

    def release_button(self, key):
        control = self.by_key.get(key)
        if control and control.msg not in ("template", "led"):
            super().release_button(key)

    def select_template(self, template: int, announce: bool) -> None:
        """Switch to a template (its LEDs come back); the device sends the template change."""
        self.template = template & 15
        self.leds = self.templates[self.template]
        self.mode = ("user template " + str(self.template + 1) if self.template < FACTORY
                     else "factory template " + str(self.template - FACTORY + 1))
        if announce:
            self.emit(HEADER + bytes([TEMPLATE_CHANGE, self.template, 0xF7]), note="template change")
        self.changed("leds")
        self.changed("settings")

    # Host -> LEDs

    def handle_message(self, msg, port):
        if len(msg) == 3 and msg[0] < 0xF0 and msg[0] & 0xF0 in (NOTE_ON, NOTE_OFF, CC):
            kind, channel, number, value = msg[0] & 0xF0, msg[0] & 0x0F, msg[1], msg[2]
            note = ""
            if kind == CC and number == 0:
                note = self._control_cc(channel, value)
            else:
                msg_kind = "cc" if kind == CC else "note"
                key = self.by_message.get((msg_kind, 0, number))
                if key is None and msg_kind == "note":
                    key = self.by_message.get(("led", 0, number))
                if key is None:
                    self.remember_unknown(msg)
                    note = "no LED"
                else:
                    self._set(channel, key, 0 if kind == NOTE_OFF else value)
            if self.monitor:
                self.monitor("in", port, msg, note)
            return
        super().handle_message(msg, port)

    def _control_cc(self, channel: int, value: int) -> str:
        if value == 0:
            for led in self.templates[channel].values():
                led.value = 0
            self.changed("leds")
            return f"reset LEDs of template {channel + 1}"
        if value & 0x28 == 0x28 or value & 0x20:
            self.flashing[channel] = bool(value & 0x08)
            self.changed("leds")
            return "flashing " + ("on" if self.flashing[channel] else "off")
        return "buffer control"

    def _set(self, template: int, key: int, value: int) -> None:
        self.templates[template][key].value = value
        if template == self.template:
            self.changed("leds")

    def handle_sysex(self, msg, port):
        if msg.startswith(HEADER) and len(msg) == 9 and msg[6] == TEMPLATE_CHANGE:
            # The host selects a template: switch and answer with the same message
            self.select_template(msg[7], announce=True)
            return f"select template {msg[7]}"
        if msg.startswith(HEADER) and len(msg) == 11 and msg[6] == LED_UPDATE:
            template, index, value = msg[7] & 15, msg[8], msg[9]
            if index < len(self.led_index):
                self._set(template, self.led_index[index], value)
                return f"LED {index} of template {template + 1}"
        return ""

    def identity_reply(self):
        # Novation (00 20 29), family 61 00 (Launch Control XL), member 00 00, version
        return bytes([0xF0, 0x7E, 0x00, 0x06, 0x02, 0x00, 0x20, 0x29, 0x61, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0xF7])

    # What the device shows

    def base_rgb(self, control, value):
        if control.msg == "template":
            return (230, 230, 230) if control.number == self.template else (0, 0, 0)
        if control.led == "rg":
            red, green, _ = decode_led(value)
            return led_rgb(red, green)
        return super().base_rgb(control, value)

    def led_rgb(self, key, now=None):
        control = self.by_key.get(key)
        if control is None:
            return (0, 0, 0)
        rgb = self.base_rgb(control, self.leds[key].value)
        if control.led == "rg" and self.flashing[self.template] and decode_led(self.leds[key].value)[2] == 2:
            now = self.clock() if now is None else now
            if (now / 0.25) % 2.0 >= 1.0:
                return (0, 0, 0)
        return rgb

    def has_animation(self):
        return self.flashing[self.template]

    def encoder_state(self, key):
        """No ring: the indicator shows the knob's absolute value."""
        if key not in self.knob_values:
            return super().encoder_state(key)
        return {"hasValue": True, "value": self.knob_values[key] / 127.0, "touched": False, "bipolar": False, "dot": False,
                "ring": False, "name": "", "text": str(self.knob_values[key])}

    def turn_encoder(self, key, steps):
        super().turn_encoder(key, steps)
        self.changed("rings")

    def status_text(self):
        return self.mode or "template not selected yet (the core selects factory template 1)"
