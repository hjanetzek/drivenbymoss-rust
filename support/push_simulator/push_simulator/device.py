# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The device side of the Push 1/2/3 MIDI protocol, without Qt.

``PushDevice`` keeps the state the host sets (LEDs with their animations, the palette, the touch
strip, brightness and pad settings, the Push 1 text display) from the MIDI it receives
(``receive``) and creates the MIDI of the controls (``press_pad``, ``press_button``,
``turn_encoder``, ``move_strip``, ...), which it hands to ``send(data, port)``.

References: Ableton's "Push 2 MIDI and Display Interface Manual" (sections 2.4 MIDI mode, 2.5 LEDs
and animations, 2.6 palette, 2.9 pads, 2.10 touch strip, 2.11 identity) and what DrivenByMoss
sends (``PushControlSurface``, ``ColorPalette``, ``ColorPaletteEntry``, ``Push1Display``,
``PushControllerSetup.startup``).
"""

import time
from dataclasses import dataclass, field
from typing import Callable

from . import layout
from .midi import CC, CHANNEL_PRESSURE, NOTE_OFF, NOTE_ON, PITCHBEND, POLY_PRESSURE, MidiParser, hexstr
from .palette import Palette, encode_entry_reply, push1_row1, push1_single

SYSEX_PUSH2 = bytes([0xF0, 0x00, 0x21, 0x1D, 0x01, 0x01])
SYSEX_PUSH1 = bytes([0xF0, 0x47, 0x7F, 0x15])
IDENTITY_REQUEST_PREFIX = bytes([0xF0, 0x7E])

# Durations of the LED animations in whole notes, by the channel within a group of 5
# (24th, 16th, 8th, quarter, half note)
ANIMATION_NOTES = (1 / 24, 1 / 16, 1 / 8, 1 / 4, 1 / 2)

# Touch strip configuration bits (Push 2 manual 2.10.1)
STRIP_HOST_LEDS = 1 << 0
STRIP_HOST_SYSEX = 1 << 1
STRIP_MODWHEEL = 1 << 2
STRIP_POINT = 1 << 3
STRIP_BAR_CENTER = 1 << 4
STRIP_AUTORETURN = 1 << 5
STRIP_RETURN_CENTER = 1 << 6
STRIP_DEFAULT = STRIP_POINT | STRIP_AUTORETURN | STRIP_RETURN_CENTER  # 0x68
STRIP_LED_COUNT = 31

# Push 1 ribbon modes (DrivenByMoss sends its PUSH_RIBBON_* index with "63 00 01 0x") as the
# equivalent Push 2 configuration (setRibbonMode: 122, 1, 17, 9)
PUSH1_RIBBON_CONFIG = {0: 122, 1: 1, 2: 17, 3: 9}

AFTERTOUCH_CHANNEL = 0
AFTERTOUCH_POLY = 1
AFTERTOUCH_MPE = 2

MIDI_MODE_LIVE = 0
MIDI_MODE_USER = 1
MIDI_MODE_DUAL = 2

# Device family and member codes of the identity reply
IDENTITY_FAMILY = {"push2": (0x67, 0x32, 0x02, 0x00), "push3": (0x67, 0x32, 0x03, 0x00)}
FIRMWARE = (1, 0, 70)  # major, minor, build: what the simulator reports
SERIAL = 0x51A1


@dataclass
class Led:
    """An LED as the host set it: ``color`` from channel 1 (no animation), ``anim_color`` and
    ``anim_channel`` (1..15 = MIDI channels 2..16) from an animation message."""

    color: int = 0
    anim_color: int = 0
    anim_channel: int = 0
    started: float = 0.0


@dataclass
class PadTouch:
    channel: int = 0
    velocity: int = 0
    pressure: int = 0


@dataclass
class SysexRecord:
    name: str
    data: bytes


@dataclass
class Settings:
    """Device settings the host changes with SysEx (shown in the monitor, read back by gets)."""

    led_brightness: int = 127
    display_brightness: int = 255
    midi_mode: int = MIDI_MODE_LIVE
    aftertouch_mode: int = AFTERTOUCH_CHANNEL
    pad_parameters: bytes = b""
    velocity_curve: list = field(default_factory=lambda: [None] * 128)
    push3_pad_curve: list = field(default_factory=list)
    white_balance: dict = field(default_factory=dict)
    push3: dict = field(default_factory=dict)
    """Push 3: the "26 07 xx" MPE/in-tune settings and "37 xx" audio/pedal settings."""
    push1_sensitivity: bytes = b""


class PushDevice:
    """The state and protocol of one simulated Push."""

    def __init__(self, model: str = "push2", send: Callable[[bytes, str], None] | None = None,
                 clock: Callable[[], float] = time.monotonic):
        if model not in layout.MODELS:
            raise ValueError(f"unknown model {model!r}")
        self.model = model
        self.clock = clock
        self.send_callback = send
        self.controls = layout.build_controls(model)
        self.buttons = layout.button_by_cc(model)
        self.encoders = layout.encoder_by_cc(model)
        self.rgb_ccs = layout.rgb_ccs(model)
        self.palette = Palette()
        self.settings = Settings()
        self.pads = {layout.PAD_FIRST_NOTE + i: Led() for i in range(layout.PAD_COUNT)}
        self.button_leds = {cc: Led() for cc in self.buttons}
        self.strip_config = STRIP_DEFAULT if model != "push1" else PUSH1_RIBBON_CONFIG[0]
        self.strip_host_value = 0
        self.strip_finger = 8192
        self.strip_touched = False
        self.strip_sysex_leds = [0] * STRIP_LED_COUNT
        self.text_lines = [" " * 68 for _ in range(4)]
        self.pad_touches: dict[int, PadTouch] = {}
        self.next_mpe_channel = 1
        self.bpm = 120.0
        self._clock_ticks: list[float] = []
        self.sysex_log: list[SysexRecord] = []
        self.parsers = {"live": MidiParser(), "user": MidiParser()}
        self.listeners: list[Callable[[str], None]] = []
        self.monitor: Callable[[str, str, bytes, str], None] | None = None
        """Called with (direction "in"/"out", port, data, note) for every message."""

    # ------------------------------------------------------------------------------------------
    # Output

    @property
    def primary_ports(self) -> tuple[str, ...]:
        """The ports the controls send on: Push 1 on its User port (DrivenByMoss uses that one),
        Push 2/3 by the MIDI mode (SysEx 0x0A: Live, User or both)."""
        if self.model == "push1":
            return ("user",)
        mode = self.settings.midi_mode
        if mode == MIDI_MODE_USER:
            return ("user",)
        if mode == MIDI_MODE_DUAL:
            return ("live", "user")
        return ("live",)

    def emit(self, data: bytes, port: str | None = None) -> None:
        ports = (port,) if port else self.primary_ports
        for p in ports:
            if self.monitor:
                self.monitor("out", p, data, "")
            if self.send_callback:
                self.send_callback(bytes(data), p)

    def changed(self, what: str) -> None:
        for listener in self.listeners:
            listener(what)

    # ------------------------------------------------------------------------------------------
    # Controls -> MIDI

    def press_button(self, cc: int) -> None:
        self.emit(bytes([CC, cc & 127, 127]))

    def release_button(self, cc: int) -> None:
        self.emit(bytes([CC, cc & 127, 0]))

    def turn_encoder(self, cc: int, delta: int) -> None:
        """Relative value: 1..63 clockwise, 127..65 counter-clockwise (two's complement)."""
        delta = max(-63, min(63, int(delta)))
        if delta:
            self.emit(bytes([CC, cc & 127, delta & 0x7F]))

    def touch_encoder(self, note: int, touched: bool) -> None:
        if note >= 0:
            self.emit(bytes([NOTE_ON, note & 127, 127 if touched else 0]))

    @property
    def mpe_active(self) -> bool:
        return self.model == "push3" and self.settings.aftertouch_mode == AFTERTOUCH_MPE

    def press_pad(self, note: int, velocity: int) -> None:
        velocity = max(1, min(127, int(velocity)))
        channel = 0
        if self.mpe_active:
            channel = self.next_mpe_channel
            self.next_mpe_channel = self.next_mpe_channel % 15 + 1
            self.emit(bytes([PITCHBEND | channel, 0, 64]))
            self.emit(bytes([CC | channel, 74, 64]))
        self.pad_touches[note] = PadTouch(channel, velocity, 0)
        self.emit(bytes([NOTE_ON | channel, note & 127, velocity]))

    def pad_pressure(self, note: int, pressure: int) -> None:
        touch = self.pad_touches.get(note)
        if touch is None:
            return
        pressure = max(0, min(127, int(pressure)))
        if pressure == touch.pressure:
            return
        touch.pressure = pressure
        if self.mpe_active:
            self.emit(bytes([CHANNEL_PRESSURE | touch.channel, pressure]))
        elif self.poly_pressure:
            self.emit(bytes([POLY_PRESSURE, note & 127, pressure]))
        else:
            self.emit(bytes([CHANNEL_PRESSURE, pressure]))

    @property
    def poly_pressure(self) -> bool:
        return self.settings.aftertouch_mode == AFTERTOUCH_POLY

    def pad_slide(self, note: int, x: float, y: float) -> None:
        """MPE (Push 3): x -1..1 = per note pitch bend, y 0..1 = slide (CC 74)."""
        touch = self.pad_touches.get(note)
        if touch is None or not self.mpe_active:
            return
        bend = max(0, min(16383, int(8192 + x * 8191)))
        self.emit(bytes([PITCHBEND | touch.channel, bend & 127, bend >> 7]))
        self.emit(bytes([CC | touch.channel, 74, max(0, min(127, int(y * 127)))]))

    def release_pad(self, note: int) -> None:
        touch = self.pad_touches.get(note)
        if touch is None:
            return
        if touch.pressure:
            self.pad_pressure(note, 0)
        del self.pad_touches[note]
        self.emit(bytes([NOTE_OFF | touch.channel, note & 127, 0]))

    def touch_strip(self, touched: bool) -> None:
        self.strip_touched = touched
        self.emit(bytes([NOTE_ON, layout.STRIP_TOUCH_NOTE, 127 if touched else 0]))
        if not touched and self.strip_config & STRIP_AUTORETURN:
            self.move_strip(8192 if self.strip_config & STRIP_RETURN_CENTER else 0, internal=True)
        self.changed("strip")

    def move_strip(self, value: int, internal: bool = False) -> None:
        """The finger position 0 (bottom) .. 16383 (top)."""
        value = max(0, min(16383, int(value)))
        self.strip_finger = value
        if self.strip_config & STRIP_MODWHEEL:
            self.emit(bytes([CC, 1, value >> 7]))
        else:
            self.emit(bytes([PITCHBEND, value & 127, value >> 7]))
        self.changed("strip")

    # ------------------------------------------------------------------------------------------
    # MIDI from the host

    def receive(self, data: bytes, port: str = "live") -> None:
        """Raw bytes from a port (may be several or partial messages)."""
        parser = self.parsers.setdefault(port, MidiParser())
        for message in parser.feed(data):
            self.handle_message(message, port)

    def handle_message(self, msg: bytes, port: str = "live") -> None:
        if not msg:
            return
        note = ""
        status = msg[0]
        if status == 0xF0:
            note = self.handle_sysex(msg, port)
        elif status == 0xF8:
            self._midi_clock()
            return  # not logged: 24 per beat
        elif status < 0xF0 and len(msg) >= 2:
            kind = status & 0xF0
            channel = status & 0x0F
            if kind in (NOTE_ON, NOTE_OFF) and len(msg) == 3:
                led = self.pads.get(msg[1])
                if led is not None:
                    self._set_led(led, channel, msg[2] if kind == NOTE_ON else 0)
                    self.changed("leds")
            elif kind == CC and len(msg) == 3:
                led = self.button_leds.get(msg[1])
                if led is not None:
                    self._set_led(led, channel, msg[2])
                    self.changed("leds")
            elif kind == PITCHBEND and len(msg) == 3 and channel == 0:
                self.strip_host_value = msg[1] | msg[2] << 7
                self.changed("strip")
        if self.monitor:
            self.monitor("in", port, msg, note)

    def _set_led(self, led: Led, channel: int, value: int) -> None:
        if channel == 0:
            led.color = value
            led.anim_channel = 0
        else:
            led.anim_color = value
            led.anim_channel = channel
            led.started = self.clock()

    def _midi_clock(self) -> None:
        now = self.clock()
        ticks = self._clock_ticks
        ticks.append(now)
        if len(ticks) > 48:
            del ticks[:-48]
        if len(ticks) >= 24 and ticks[-1] > ticks[0]:
            self.bpm = 60.0 * (len(ticks) - 1) / 24.0 / (ticks[-1] - ticks[0])

    # SysEx

    def record(self, name: str, data: bytes) -> str:
        self.sysex_log.append(SysexRecord(name, data))
        if len(self.sysex_log) > 200:
            del self.sysex_log[:-200]
        return name

    def handle_sysex(self, msg: bytes, port: str) -> str:
        if len(msg) >= 6 and msg[1] == 0x7E and msg[3] == 0x06 and msg[4] == 0x01:
            self.emit(self.identity_reply(), port)
            return self.record("identity request", msg)
        if msg.startswith(SYSEX_PUSH2) and len(msg) >= 8 and self.model != "push1":
            return self.handle_push2_sysex(msg[6:-1], msg, port)
        if msg.startswith(SYSEX_PUSH1) and len(msg) >= 8 and self.model == "push1":
            return self.handle_push1_sysex(msg[4:-1], msg)
        return self.record("unknown SysEx", msg)

    def identity_reply(self) -> bytes:
        major, minor, build = FIRMWARE
        if self.model == "push1":
            # 35 bytes; DrivenByMoss reads the version as data[9..12] (two decimal digits each)
            data = [0xF0, 0x7E, 0x00, 0x06, 0x02, 0x47, 0x15, 0x00, 0x19, major // 10, major % 10, minor // 10, minor % 10]
            data += [0] * (34 - len(data))
            return bytes(data + [0xF7])
        family = IDENTITY_FAMILY[self.model]
        serial = [(SERIAL >> (7 * i)) & 127 for i in range(5)]
        return bytes([0xF0, 0x7E, 0x01, 0x06, 0x02, 0x00, 0x21, 0x1D, *family, major, minor, build & 127, build >> 7,
                      *serial, 0x01, 0xF7])

    def reply(self, port: str, *data: int) -> None:
        self.emit(SYSEX_PUSH2 + bytes(d & 127 for d in data) + b"\xF7", port)

    def handle_push2_sysex(self, p: bytes, msg: bytes, port: str) -> str:
        s = self.settings
        cmd = p[0]
        args = p[1:]
        if len(p) == 128 and self.model == "push3":
            # PushControlSurface.sendPadSensitivityPush3 sends the 128 curve values without the
            # command byte 0x43 (it builds "data" with 0x43 but sends "curve")
            s.push3_pad_curve = list(p)
            return self.record("pad curve without command byte (DrivenByMoss sends no 0x43)", msg)
        if cmd == 0x03 and len(args) >= 9:
            v = [args[i] | args[i + 1] << 7 for i in range(1, 9, 2)]
            self.palette.set_entry(args[0], *v)
            return self.record(f"set palette entry {args[0]}", msg)
        if cmd == 0x04 and len(args) >= 1:
            self.reply(port, 0x04, *encode_entry_reply(args[0], self.palette.entry(args[0])))
            return self.record(f"get palette entry {args[0]}", msg)
        if cmd == 0x05:
            count = self.palette.apply()
            self.changed("leds")
            return self.record(f"reapply palette ({count} changed)", msg)
        if cmd == 0x06 and args:
            s.led_brightness = args[0]
            self.changed("leds")
            return self.record(f"LED brightness {args[0]}", msg)
        if cmd == 0x07:
            self.reply(port, 0x07, s.led_brightness)
            return self.record("get LED brightness", msg)
        if cmd == 0x08 and len(args) >= 2:
            s.display_brightness = args[0] | args[1] << 7
            self.changed("display")
            return self.record(f"display brightness {s.display_brightness}", msg)
        if cmd == 0x09:
            self.reply(port, 0x09, s.display_brightness & 127, s.display_brightness >> 7)
            return self.record("get display brightness", msg)
        if cmd == 0x0A and args:
            s.midi_mode = args[0]
            return self.record(f"MIDI mode {('Live', 'User', 'Dual')[args[0]] if args[0] < 3 else args[0]}", msg)
        if cmd == 0x0B:
            self.reply(port, 0x0B, s.midi_mode)
            return self.record("get MIDI mode", msg)
        if cmd == 0x14 and len(args) >= 3:
            s.white_balance[args[0]] = args[1] | args[2] << 7
            return self.record(f"white balance group {args[0]}", msg)
        if cmd == 0x15 and args:
            value = s.white_balance.get(args[0], 1024)
            self.reply(port, 0x15, args[0], value & 127, value >> 7)
            return self.record("get white balance", msg)
        if cmd == 0x17 and args:
            self.strip_config = args[0]
            self.changed("strip")
            return self.record(f"touch strip config 0x{args[0]:02X}", msg)
        if cmd == 0x18:
            self.reply(port, 0x18, self.strip_config)
            return self.record("get touch strip config", msg)
        if cmd == 0x19 and len(args) >= 16:
            leds = []
            for b in args[:16]:
                leds += [b & 7, (b >> 3) & 7]
            self.strip_sysex_leds = leds[:STRIP_LED_COUNT]
            self.changed("strip")
            return self.record("touch strip LEDs", msg)
        if cmd == 0x1A:
            self.reply(port, 0x1A, 0x01, 0, 0, 0, 0, 0, 0)
            return self.record("request statistics", msg)
        if cmd == 0x1B:
            s.pad_parameters = bytes(args)
            return self.record("pad parameters (threshold, cpmin, cpmax)", msg)
        if cmd == 0x1C:
            self.reply(port, 0x1C, *(s.pad_parameters or bytes(8)))
            return self.record("get pad parameters", msg)
        if cmd == 0x1E and args:
            s.aftertouch_mode = args[0]
            return self.record(f"aftertouch mode {('channel', 'poly', 'MPE')[args[0]] if args[0] < 3 else args[0]}", msg)
        if cmd == 0x1F:
            self.reply(port, 0x1F, s.aftertouch_mode)
            return self.record("get aftertouch mode", msg)
        if cmd == 0x20 and len(args) >= 2:
            start = args[0]
            for i, v in enumerate(args[1:]):
                if start + i < 128:
                    s.velocity_curve[start + i] = v
            return self.record(f"velocity curve entries {start}..{start + len(args) - 2}", msg)
        if cmd == 0x21 and args:
            start = args[0] & 0x70
            self.reply(port, 0x21, start, *((v if v is not None else 0) for v in s.velocity_curve[start:start + 16]))
            return self.record("get velocity curve", msg)
        if cmd == 0x26 and len(args) >= 3:
            s.push3[(0x26, args[0], args[1])] = list(args[2:])
            names = {0x08: "per pad pitch bend", 0x0E: "in tune location", 0x14: "in tune width", 0x24: "slide height"}
            return self.record(f"MPE setting: {names.get(args[1], hex(args[1]))} {hexstr(bytes(args[2:]))}", msg)
        if cmd == 0x37 and len(args) >= 2:
            s.push3[(0x37, args[0])] = args[1]
            names = {0x26: "pedals", 0x1A: "preamp 1 type", 0x1B: "preamp 2 type", 0x02: "preamp 1 gain",
                     0x03: "preamp 2 gain", 0x11: "audio outputs"}
            return self.record(f"audio setting: {names.get(args[0], hex(args[0]))} = {args[1]}", msg)
        if cmd == 0x43:
            s.push3_pad_curve = list(args)
            return self.record("pad curve", msg)
        return self.record(f"unhandled Push SysEx 0x{cmd:02X}", msg)

    def handle_push1_sysex(self, p: bytes, msg: bytes) -> str:
        cmd = p[0]
        data = p[3:] if len(p) >= 3 else b""
        if 0x18 <= cmd <= 0x1B:
            text = bytes(data[1:69]).decode("latin-1")
            self.text_lines[cmd - 0x18] = text.ljust(68)[:68]
            self.changed("text")
            return self.record(f"display line {cmd - 0x18 + 1}", msg)
        if 0x1C <= cmd <= 0x1F:
            self.text_lines[cmd - 0x1C] = " " * 68
            self.changed("text")
            return self.record(f"clear display line {cmd - 0x1C + 1}", msg)
        if cmd == 0x5C and data:
            self.settings.aftertouch_mode = AFTERTOUCH_POLY if data[0] == 0 else AFTERTOUCH_CHANNEL
            return self.record("pressure mode " + ("poly" if data[0] == 0 else "channel"), msg)
        if cmd == 0x5D:
            self.settings.push1_sensitivity = bytes(data)
            return self.record("pad sensitivity", msg)
        if cmd == 0x63 and data:
            self.strip_config = PUSH1_RIBBON_CONFIG.get(data[0], STRIP_DEFAULT)
            self.changed("strip")
            return self.record(f"ribbon mode {data[0]}", msg)
        return self.record(f"unhandled Push 1 SysEx 0x{cmd:02X}", msg)

    # ------------------------------------------------------------------------------------------
    # What the LEDs show

    def animation_seconds(self, channel: int) -> float:
        whole_note = 4 * 60.0 / max(20.0, self.bpm)
        return ANIMATION_NOTES[(channel - 1) % 5] * whole_note

    def has_animation(self) -> bool:
        if self.model == "push1":
            return True  # the blink states are values, cheap enough to always refresh
        return any(led.anim_channel for led in self.pads.values()) or any(led.anim_channel for led in self.button_leds.values())

    def _animated_value(self, led: Led, now: float, to_rgb: Callable[[int], tuple]) -> tuple:
        start = to_rgb(led.color)
        if not led.anim_channel:
            return start
        target = to_rgb(led.anim_color)
        duration = self.animation_seconds(led.anim_channel)
        elapsed = max(0.0, now - led.started)
        if led.anim_channel <= 5:  # one shot transition
            f = min(1.0, elapsed / duration)
        elif led.anim_channel <= 10:  # pulsing
            phase = (elapsed / (2 * duration)) % 1.0
            f = 1.0 - abs(2.0 * phase - 1.0)
        else:  # blinking
            f = 1.0 if (elapsed / duration) % 2.0 >= 1.0 else 0.0
            # starts with the animation colour
            f = 1.0 - f
        return tuple(int(a + (b - a) * f) for a, b in zip(start, target))

    def _scale(self, rgb: tuple) -> tuple:
        k = 0.25 + 0.75 * self.settings.led_brightness / 127.0
        return tuple(int(v * k) for v in rgb)

    def pad_rgb(self, note: int, now: float | None = None) -> tuple:
        led = self.pads.get(note)
        if led is None:
            return (0, 0, 0)
        now = self.clock() if now is None else now
        return self._scale(self._animated_value(led, now, self.palette.rgb))

    def button_rgb(self, cc: int, now: float | None = None) -> tuple:
        led = self.button_leds.get(cc)
        if led is None:
            return (0, 0, 0)
        now = self.clock() if now is None else now
        if self.model == "push1":
            return self._push1_button_rgb(cc, led, now)
        if cc in self.rgb_ccs:
            to_rgb = self.palette.rgb
        else:
            def to_rgb(index):
                w = self.palette.white(index)
                return (w, w, w)
        return self._scale(self._animated_value(led, now, to_rgb))

    def _push1_button_rgb(self, cc: int, led: Led, now: float) -> tuple:
        if 102 <= cc <= 109:
            return self.palette.rgb(led.color)
        if 20 <= cc <= 27 or 36 <= cc <= 43:
            rgb, blink = push1_row1(led.color)
        else:
            rgb, blink = push1_single(led.color, self.buttons[cc].base)
        if blink:
            period = 0.5 if blink == 1 else 0.2
            if (now / period) % 2.0 >= 1.0:
                return (0, 0, 0)
        return rgb

    def strip_levels(self) -> list[float]:
        """The 31 touch strip LEDs (index 0 at the bottom), brightness 0..1."""
        cfg = self.strip_config
        host_leds = bool(cfg & STRIP_HOST_LEDS)
        if host_leds and cfg & STRIP_HOST_SYSEX:
            return [v / 7.0 for v in self.strip_sysex_leds]
        value = self.strip_host_value if host_leds else self.strip_finger
        pos = value / 16383.0 * (STRIP_LED_COUNT - 1)
        levels = [0.0] * STRIP_LED_COUNT
        if cfg & STRIP_POINT:
            i = int(round(pos))
            levels[i] = 1.0
            for j in (i - 1, i + 1):
                if 0 <= j < STRIP_LED_COUNT:
                    levels[j] = 0.25
        else:
            start = (STRIP_LED_COUNT - 1) / 2.0 if cfg & STRIP_BAR_CENTER else 0.0
            lo, hi = sorted((start, pos))
            for i in range(STRIP_LED_COUNT):
                if lo - 0.5 <= i <= hi + 0.5:
                    levels[i] = 1.0
        return levels

    def status_lines(self) -> list[str]:
        s = self.settings
        lines = [f"Model {self.model}  LED brightness {s.led_brightness}  display brightness {s.display_brightness}",
                 f"MIDI mode {s.midi_mode}  aftertouch {('channel', 'poly', 'MPE')[s.aftertouch_mode] if s.aftertouch_mode < 3 else s.aftertouch_mode}"
                 f"  strip 0x{self.strip_config:02X}  tempo {self.bpm:.1f}"]
        if self.palette.applied_count:
            lines.append(f"palette re-applied {self.palette.applied_count}x")
        return lines
