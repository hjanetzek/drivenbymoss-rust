# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The Maschine Mk2 and JAM as DrivenByMoss sees them through dbm-nihost (or NI's driver), without
Qt: the template MIDI of the controls, the LED feedback, the Mk2's MCU text displays with the
button labels, the JAM's touch strips and level meters, and dbm-nihost's handshake (hello, query,
gone, the Mk2's Shift).

The behaviour follows dbm-nihost (support/linux/ni_host/src/mk2_host.cpp, jam_host.cpp,
text_display.cpp, midi.h), which emulates NI's driver with the DrivenByMoss templates. Additions
of the simulator: the JAM's level meters (CC 64/65 of the core, not yet shown by dbm-nihost) and
the Mk2's pad pressure as poly aftertouch (the template's "Pressure" controls).

``send(data, port)`` gets the MIDI to the DAW with the port "in" (the port the DAW reads, "...
In"/"... Input"); ``receive(data, port)`` takes the MIDI of the DAW (port "out").

The mirror of a real device which the core drives (doc/CONTROLLER_HELP_PLAN.md §4): its "out"
messages go to ``receive`` as the DAW's; its "in" messages (what the hardware sent) go to
``mirror_input``, which only tells which controls they press, touch or move.
"""

import time
from dataclasses import dataclass
from typing import Callable

from sim_common.midi import MidiParser

from . import layout, ni, templates

IN_PORT = "in"
OUT_PORT = "out"
STRIP_LEDS = 11
METER_LEDS = 8
MCU_LINE = 56

STRIP_SINGLE, STRIP_DOT, STRIP_PAN, STRIP_DUAL = range(4)


@dataclass(frozen=True)
class TemplateControl:
    id: str
    direction: str
    kind: str
    channel: int
    number: int
    relative: bool
    on: int
    off: int


@dataclass
class Strip:
    type: int = STRIP_SINGLE
    colour: int = 0
    value: int = 0
    dual_value: int = 0
    touched: bool = False


TABLES = {"mk2": templates.MK2, "jam": templates.JAM, "mk3": templates.MK3, "mikromk3": templates.MIKRO_MK3}
DISABLED = {"mk3": templates.MK3_DISABLED, "mikromk3": templates.MIKRO_MK3_DISABLED}
# The models behind dbm-nihost (Linux); the others emulate NI's driver directly
NIHOST_MODELS = ("mk2", "jam")
# The models with the MCU text display (two displays)
MCU_MODELS = ("mk2", "mk3")
# The controls which send but have no LED
NO_LED = ("Push", "PshBrowse", "CapBrowse", "EncoderPush", "EncoderCap", "Navigation Push", "Navigation Touch", "TouchstripCap")


def _table(model: str) -> list[TemplateControl]:
    return [TemplateControl(*entry) for entry in TABLES[model]]


class MaschineDevice:
    """One simulated Maschine (``model`` "mk2", "jam", "mk3" or "mikromk3"). ``send_disabled``: also
    send the controls which the NI template disables (the Mk3's touch strip touch), as the CC the
    core binds."""

    def __init__(self, model: str, send: Callable[[bytes, str], None] | None = None,
                 clock: Callable[[], float] = time.monotonic, send_disabled: bool = False):
        if model not in layout.MODELS:
            raise ValueError(f"unknown model {model!r}")
        self.model = model
        self.send_callback = send
        self.clock = clock
        self.controls = layout.build_controls(model)
        self.by_key = {c.key: c for c in self.controls}
        table = _table(model)
        self.inputs = {t.id: t for t in table if t.direction == "input"}
        self.led_templates = {t.id: t for t in table if t.direction == "led"}
        self.disabled = DISABLED.get(model, frozenset())
        self.send_disabled = send_disabled
        self.touchstrip = 0
        """The value of the touch strip's LEDs (Mk3, Mikro Mk3: the template's TouchstripIDX)."""
        self.leds: dict[int, int] = {c.key: 0 for c in self.controls}
        self.shift_key = next((c.key for c in self.controls if c.label == "Shift" and not c.template), -1)
        self.shift_down = False
        # (status kind 0x90/0xB0, number) -> keys whose LED it sets
        self.led_map: dict[tuple[int, int], list[int]] = {}
        self._build_led_map()
        self.present = False
        """The simulated device is "attached": it answers the query (dbm-nihost doesn't without it)."""
        self.mcu_text = [" "] * (2 * MCU_LINE)
        self.labels = [" "] * MCU_LINE
        self.strips = [Strip() for _ in range(8)]
        self.meters = [0, 0]
        self.parser = MidiParser()
        self.listeners: list[Callable[[str], None]] = []
        self.monitor: Callable[[str, str, bytes, str], None] | None = None
        self.unknown: list[bytes] = []
        self.mirroring = False
        """The device mirrors a real one: it answers no query (the core talks to the hardware)."""
        # (status kind 0x90/0xB0/0xA0, channel, number) -> (control, template, role) of the
        # messages the controls send ("press", "turn", "touch", "move", "pressure")
        self.input_map: dict[tuple[int, int, int], tuple[layout.Control, TemplateControl, str]] = {}
        self._build_input_map()

    # ------------------------------------------------------------------------------------------
    # Templates

    def template(self, control: layout.Control) -> TemplateControl | None:
        return self.inputs.get(control.template) if control.template else None

    def _led_template(self, control: layout.Control) -> TemplateControl | None:
        """The LED message of a control: pads, groups and scenes have their own LED controls in
        the template (Mk2: "<id>H" on channel 1, the hue; JAM: "<id>IDX"), the other buttons are
        lit with the message of the button (dbm-nihost's ccButtons, and the Mk2's 7th display
        button with its note)."""
        if control.kind in ("pad", "button") and control.rgb:
            return self.led_templates.get(control.template + ("H" if self.model == "mk2" else "IDX"))
        if control.kind == "button" and control.template:
            t = self.template(control)
            if t is None or t.id in NO_LED:
                return None
            return t
        return None

    def _build_led_map(self) -> None:
        for control in self.controls:
            t = self._led_template(control)
            if t is None or t.kind == "polyat":
                continue
            status = 0xB0 if t.kind == "cc" else 0x90
            self.led_map.setdefault((status, t.number), []).append(control.key)
        if self.model == "jam":
            self.strip_ccs = {self.led_templates[f"Tst{chr(ord('A') + i)}IDX"].number: i for i in range(8)}
        strip_led = self.led_templates.get("TouchstripIDX")
        self.touchstrip_cc = strip_led.number if strip_led is not None and self.model != "jam" else -1

    def _build_input_map(self) -> None:
        def add(control: layout.Control, template_id: str, role: str) -> None:
            t = self.inputs.get(template_id)
            if t is None:
                return
            status = {"cc": 0xB0, "note": 0x90, "polyat": 0xA0}.get(t.kind)
            if status is not None:
                self.input_map.setdefault((status, t.channel, t.number), (control, t, role))

        for control in self.controls:
            if not control.template or control.key == self.shift_key:
                continue
            role = {"pad": "press", "button": "press", "encoder": "turn", "strip": "move"}.get(control.kind)
            if role is None:
                continue
            add(control, control.template, role)
            if control.touch:
                add(control, control.touch, "touch")
            if control.kind == "pad" and self.model != "jam":
                add(control, "Pressure" + control.template[3:], "pressure")

    # ------------------------------------------------------------------------------------------
    # Output

    def emit(self, data: bytes, note: str = "") -> None:
        if self.monitor:
            self.monitor("out", IN_PORT, data, note)
        if self.send_callback:
            self.send_callback(bytes(data), IN_PORT)

    def changed(self, what: str) -> None:
        for listener in self.listeners:
            listener(what)

    def send_control(self, t: TemplateControl, value: int) -> None:
        """dbm-nihost's sendControl: a note with value 0 is a note off."""
        value = max(0, min(127, int(value)))
        if t.kind == "cc":
            status = 0xB0
        elif t.kind == "note":
            status = 0x90 if value > 0 else 0x80
        else:
            status = 0xA0
        self.emit(bytes([status | (t.channel & 0x0F), t.number & 0x7F, value]))

    # Handshake

    def hello(self) -> bytes:
        """The Mk2's dbm-nihost hello, else NI's "return from host" of the device."""
        return ni.MK2_HELLO if self.model == "mk2" else ni.ni_return_from_host(self.model)

    def attach(self) -> None:
        """The device is there: send the hello (the Mk2: Shift is sent; the JAM: NI's "return from
        host", the core refreshes it), as dbm-nihost does when the device is attached."""
        self.present = True
        self.emit(self.hello(), "hello")

    def detach(self) -> None:
        """The device is gone (the simulator quits): F0 7D 44 42 4D 13 F7."""
        if self.present:
            self.present = False
            if self.model in NIHOST_MODELS:
                self.emit(ni.GONE, "gone")

    # ------------------------------------------------------------------------------------------
    # Controls -> MIDI

    def press_button(self, key: int) -> None:
        self._button(key, True)

    def release_button(self, key: int) -> None:
        self._button(key, False)

    def _button(self, key: int, down: bool) -> None:
        if key == self.shift_key:
            # Not in the template: the Mk2 sends it as DrivenByMoss SysEx, the JAM as NI's SysEx;
            # the LED is lit by the host (dbm-nihost)
            self.shift_down = down
            self.emit(ni.mk2_shift(down) if self.model == "mk2" else ni.ni_shift(self.model, down), "Shift " + ("down" if down else "up"))
            self.changed("leds")
            return
        control = self.by_key.get(key)
        t = self.template(control) if control else None
        if t is not None:
            self.send_control(t, t.on if down else t.off)

    def turn_encoder(self, key: int, steps: int) -> None:
        control = self.by_key.get(key)
        t = self.template(control) if control else None
        if t is not None and steps:
            self.send_control(t, ni.twos_complement(steps))

    def touch_encoder(self, key: int, touched: bool) -> None:
        control = self.by_key.get(key)
        if control and control.touch and self._can_send(control.touch):
            t = self.inputs[control.touch]
            self.send_control(t, t.on if touched else t.off)

    def _can_send(self, template_id: str) -> bool:
        return template_id not in self.disabled or self.send_disabled

    def press_pad(self, key: int, velocity: int) -> None:
        control = self.by_key.get(key)
        t = self.template(control) if control else None
        if t is not None:
            self.send_control(t, max(1, min(127, int(velocity))))

    def pad_pressure(self, key: int, pressure: int) -> None:
        """The Mk2's pad pressure: the template's "Pressure<n>" (poly aftertouch)."""
        control = self.by_key.get(key)
        if control and self.model != "jam":
            t = self.inputs.get("Pressure" + control.template[3:])
            if t is not None:
                self.send_control(t, pressure)

    def release_pad(self, key: int) -> None:
        control = self.by_key.get(key)
        t = self.template(control) if control else None
        if t is not None:
            self.send_control(t, 0)

    def touch_strip(self, index: int, touched: bool) -> None:
        strip = self._strip_control(index)
        if strip is None:
            return
        state = self.strips[index]
        if touched == state.touched:
            return
        state.touched = touched
        if not self._can_send(strip.touch):
            return
        t = self.inputs[strip.touch]
        if t.id in self.disabled:
            # Disabled in the template (NI's driver sends nothing); sent as the CC the core binds
            t = TemplateControl(t.id, t.direction, "cc", t.channel, t.number, False, t.on, t.off)
        self.send_control(t, t.on if touched else t.off)

    def move_strip(self, index: int, value: int) -> None:
        """The finger at 0 (bottom) .. 127 (top); touches the strip first (as dbm-nihost)."""
        strip = self._strip_control(index)
        if strip is None:
            return
        self.touch_strip(index, True)
        self.send_control(self.inputs[strip.template], value)

    def _strip_control(self, index: int) -> layout.Control | None:
        return next((c for c in self.controls if c.kind == "strip" and c.index == index), None)

    # ------------------------------------------------------------------------------------------
    # The mirror: what the hardware sent

    def mirror_input(self, msg: bytes) -> list[tuple[int, bool | None]]:
        """The controls a message of the mirrored hardware acts on: (key, True pressed / touched,
        False released, None moved: a short flash). Nothing is sent and no LED changes (the
        core's "out" messages do that); the Shift SysEx sets the Shift LED as the host does."""
        if not msg:
            return []
        if msg[0] == 0xF0:
            if self.shift_key < 0:
                return []
            for down in (True, False):
                if msg == (ni.mk2_shift(down) if self.model == "mk2" else ni.ni_shift(self.model, down)):
                    self.shift_down = down
                    self.changed("leds")
                    return [(self.shift_key, down)]
            return []
        if len(msg) != 3 or not 0x80 <= msg[0] < 0xF0:
            return []
        kind, channel, value = msg[0] & 0xF0, msg[0] & 0x0F, msg[2]
        if kind == 0x80:
            kind, value = 0x90, 0
        found = self.input_map.get((kind, channel, msg[1]))
        if found is None:
            return []
        control, t, role = found
        if role == "pressure":
            return []
        if role == "press" and t.kind == "note":
            return [(control.key, value > 0)]
        if role in ("press", "touch"):
            return [(control.key, value != t.off)]
        return [(control.key, None)]

    def mirror_reset(self) -> None:
        """Forget what the device shows (a new mirror sends the complete state)."""
        self.leds = {c.key: 0 for c in self.controls}
        self.shift_down = False
        self.touchstrip = 0
        self.mcu_text = [" "] * (2 * MCU_LINE)
        self.labels = [" "] * MCU_LINE
        self.strips = [Strip() for _ in range(8)]
        self.meters = [0, 0]
        for what in ("leds", "strip", "text"):
            self.changed(what)

    # ------------------------------------------------------------------------------------------
    # MIDI from the DAW

    def receive(self, data: bytes, port: str = OUT_PORT) -> None:
        for message in self.parser.feed(data):
            self.handle_message(message, port)

    def handle_message(self, msg: bytes, port: str = OUT_PORT) -> None:
        note = ""
        if msg[0] == 0xF0:
            note = self.handle_sysex(msg)
        elif len(msg) == 3 and msg[0] < 0xF0:
            kind, channel, number, value = msg[0] & 0xF0, msg[0] & 0x0F, msg[1], msg[2]
            # dbm-nihost: the Mk2 takes channel 1 only, the JAM ignores the channel; the Mk3 and
            # the Mikro Mk3 (NI's driver) as the Mk2
            if self.model != "jam" and channel != 0:
                pass
            elif kind == 0xB0:
                self._cc(number, value)
            elif kind in (0x90, 0x80):
                self._note(number, 0 if kind == 0x80 else value)
        if self.monitor:
            self.monitor("in", port, msg, note)

    def _cc(self, number: int, value: int) -> None:
        if number == self.touchstrip_cc:
            self.touchstrip = value
            self.changed("strip")
            return
        if self.model == "jam":
            if number in self.strip_ccs:
                self.strips[self.strip_ccs[number]].value = value
                self.changed("strip")
                return
            if number in (64, 65):
                self.meters[number - 64] = value
                self.changed("strip")
                return
        keys = self.led_map.get((0xB0, number))
        if keys:
            for key in keys:
                self.leds[key] = value
            self.changed("leds")

    def _note(self, number: int, value: int) -> None:
        keys = self.led_map.get((0x90, number))
        if keys:
            for key in keys:
                self.leds[key] = value
            self.changed("leds")

    def handle_sysex(self, msg: bytes) -> str:
        if msg == ni.QUERY and self.model not in NIHOST_MODELS:
            return "query of dbm-nihost (NI's driver ignores it)"
        if msg == ni.QUERY:
            if self.mirroring:
                return "query (mirror: the hardware answers)"
            if self.present:
                self.emit(self.hello(), "hello")
                return "query: answered with the hello"
            return "query (not attached: no answer)"
        if self.model in MCU_MODELS:
            if msg.startswith(ni.DBM_HEADER + bytes([ni.DBM_LABELS])) and len(msg) >= 9:
                self._set_chars(self.labels, msg[6], msg[7:-1])
                return "button labels"
            if msg.startswith(ni.MCU_HEADER) and len(msg) >= 8 and msg[5] == ni.MCU_DISPLAY:
                self._set_chars(self.mcu_text, msg[6], msg[7:-1])
                return "MCU display text"
        elif self.model == "jam":
            if msg.startswith(ni.JAM_HEADER) and len(msg) >= len(ni.JAM_HEADER) + 2:
                command = msg[len(ni.JAM_HEADER)]
                data = msg[ni.JAM_STRIP_VALUE_OFFSET:-1]
                if command == ni.JAM_STRIP_SETUP:
                    for i in range(min(8, len(data) // 2)):
                        self.strips[i].type = data[2 * i]
                        self.strips[i].colour = data[2 * i + 1]
                    self.changed("strip")
                    return "strip setup"
                if command == ni.JAM_STRIP_DUAL_VALUES:
                    for i in range(min(8, len(data))):
                        self.strips[i].dual_value = data[i]
                    self.changed("strip")
                    return "strip dual values"
        self.unknown.append(msg)
        del self.unknown[:-50]
        return "unhandled SysEx"

    def _set_chars(self, target: list[str], position: int, data: bytes) -> None:
        for i, b in enumerate(data):
            if position + i >= len(target):
                break
            target[position + i] = chr(b) if 32 <= b < 127 else " "
        self.changed("text")

    # ------------------------------------------------------------------------------------------
    # What the device shows

    def has_animation(self) -> bool:
        return False

    def led_rgb(self, key: int) -> tuple[int, int, int]:
        control = self.by_key.get(key)
        if control is None:
            return (0, 0, 0)
        if key == self.shift_key:
            return (230, 230, 230) if self.shift_down else (0, 0, 0)
        value = self.leds.get(key, 0)
        if control.rgb:
            return ni.palette_rgb(value)
        # A brightness (dbm-nihost maps MIDI 1..127 to the JAM's hardware 2..85 with a gamma of
        # 1.5, its LEDs are full from about 85 on; the simulator shows the value itself)
        level = max(0, min(127, value)) / 127.0
        return tuple(int(c * level) for c in (235, 235, 225))

    def display_lines(self, display: int) -> list[str]:
        """The text of a Mk2 display (0 left, 1 right): its 28 characters of the button labels and
        of both MCU lines (NI's driver shows each half of the lines on one display)."""
        half = MCU_LINE // 2
        start = display * half
        text = "".join(self.mcu_text)
        return ["".join(self.labels[start:start + half]),
                text[start:start + half], text[MCU_LINE + start:MCU_LINE + start + half]]

    def strip_colours(self, index: int) -> list[int]:
        """The 11 LEDs of a JAM touch strip (bottom first) as palette indices (dbm-nihost's
        drawStrip)."""
        leds = STRIP_LEDS
        if self.model != "jam":
            # The touch strip of the Mk3 / Mikro Mk3: a white bar of the host's value
            bar = (self.touchstrip * leds + 63) // 127
            return [ni.colour(ni.HUE_WHITE, ni.LEVEL_FULL) if i < bar else 0 for i in range(leds)]
        s = self.strips[index]

        def led_of(value):
            return value * (leds - 1) // 127

        if s.type == STRIP_DOT:
            return [s.colour if i == led_of(s.value) else 0 for i in range(leds)]
        if s.type == STRIP_PAN:
            centre, led = leds // 2, led_of(s.value)
            return [s.colour if (centre <= i <= led) or (led <= i <= centre) else 0 for i in range(leds)]
        bar = (s.value * leds + 63) // 127
        if s.type == STRIP_DUAL:
            dot = led_of(s.dual_value)
            white = ni.colour(ni.HUE_WHITE, ni.LEVEL_FULL)
            return [white if i == dot else (s.colour if i < bar else 0) for i in range(leds)]
        return [s.colour if i < bar else 0 for i in range(leds)]

    def meter_leds(self, index: int) -> int:
        """The number of lit LEDs of a level meter (0 left, 1 right)."""
        return (self.meters[index] * METER_LEDS + 63) // 127
