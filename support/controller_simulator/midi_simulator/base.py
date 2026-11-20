# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The common part of the simulated "plain MIDI" controllers (APC40 mkII, Launchpad Pro Mk3), without
Qt: controls with their MIDI message and LED kind, the LED state with the per-channel animations,
absolute and relative encoders with LED rings, faders, and the MIDI plumbing (``receive``,
``send(data, port)``, ``listeners``, ``monitor``) the shared Qt bridge expects.

A device class describes its controls (``build_controls``), its ports, how a host message lights
an LED (``led_for``), its animation channels (``animation``), its SysEx (``handle_sysex``) and its
identity reply.
"""

import time
from dataclasses import dataclass
from typing import Callable

from sim_common.midi import MidiParser
from sim_common.palettes import velocity_rgb

NOTE_OFF, NOTE_ON, POLY_AT, CC, CHANNEL_AT = 0x80, 0x90, 0xA0, 0xB0, 0xD0
IDENTITY_REQUEST = bytes([0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7])


@dataclass
class Control:
    kind: str  # "pad", "button", "encoder", "fader"
    label: str
    x: float
    y: float
    w: float
    h: float
    msg: str = "note"  # "note" or "cc"
    channel: int = 0
    number: int = -1
    led: str = "single"
    """"rgb" (palette velocity), "single" (0 off, 1 on, 2 blinking), "ab" (APC A/B: 1 A, 2 B), "none"."""
    colour: tuple = (255, 170, 40)
    """The colour of a single colour LED."""
    relative: bool = False
    """An encoder which sends relative steps (two's complement), else absolute values."""
    ring: tuple | None = None
    """The CCs of the LED ring of an encoder: (value CC, mode CC)."""
    const: str = ""
    """The name of the constant in the Rust core (src/controller/...), for the sync test."""
    key: int = -1

    def as_dict(self) -> dict:
        return {"kind": self.kind, "label": self.label, "x": self.x, "y": self.y, "w": self.w, "h": self.h,
                "cc": self.key, "note": self.key, "rgb": self.led == "rgb", "index": self.key}


@dataclass
class Led:
    value: int = 0
    anim_value: int = 0
    anim_channel: int = 0
    started: float = 0.0
    rgb: tuple | None = None
    """An RGB colour set by SysEx (Launchpad), replaces the palette value."""


@dataclass
class Ring:
    value: int = 0
    mode: int = 0


@dataclass
class PadTouch:
    velocity: int = 0
    pressure: int = 0


class GridDevice:
    MODEL = ""
    TITLE = ""
    SURFACE = (100.0, 100.0)
    # logical port name -> port name; the controls send on SEND_PORT
    PORTS: dict = {}
    SEND_PORT = "midi"

    def __init__(self, send: Callable[[bytes, str], None] | None = None, clock: Callable[[], float] = time.monotonic,
                 poly_pressure: bool = True):
        self.send_callback = send
        self.clock = clock
        self.poly_pressure = poly_pressure
        self.controls = self.build_controls()
        for key, control in enumerate(self.controls):
            control.key = key
        self.by_key = {c.key: c for c in self.controls}
        self.by_message: dict[tuple[str, int, int], int] = {}
        for c in self.controls:
            if c.number >= 0:
                self.by_message[(c.msg, c.channel, c.number)] = c.key
        self.leds = {c.key: Led() for c in self.controls}
        self.rings = {c.key: Ring() for c in self.controls if c.ring}
        self.ring_ccs: dict[int, tuple[int, str]] = {}
        for c in self.controls:
            if c.ring:
                self.ring_ccs[c.ring[0]] = (c.key, "value")
                self.ring_ccs[c.ring[1]] = (c.key, "mode")
        self.knob_values = {c.key: 0 for c in self.controls if c.kind == "encoder" and not c.relative}
        self.fader_values = {c.key: 0 for c in self.controls if c.kind == "fader"}
        self.touches: dict[int, PadTouch] = {}
        self.parsers: dict[str, MidiParser] = {}
        self.listeners: list[Callable[[str], None]] = []
        self.monitor: Callable[[str, str, bytes, str], None] | None = None
        self.unknown: list[bytes] = []
        self.mode = ""
        """The mode the host selected (e.g. programmer mode), shown in the status."""
        self.bpm = 120.0

    # Description (overridden)

    def build_controls(self) -> list[Control]:
        raise NotImplementedError

    def led_for(self, kind: str, channel: int, number: int) -> tuple[int, int] | None:
        """The key of the control a host message lights and the animation channel (0 steady)."""
        key = self.by_message.get((kind, channel, number))
        return (key, 0) if key is not None else None

    def animation(self, channel: int) -> tuple[str, float]:
        """("oneshot"|"pulse"|"blink"|"flash", seconds) of an animation channel."""
        return ("blink", 0.5)

    def handle_sysex(self, msg: bytes, port: str) -> str:
        return ""

    def identity_reply(self) -> bytes | None:
        return None

    def status_text(self) -> str:
        return self.mode

    # ------------------------------------------------------------------------------------------
    # Output

    def emit(self, data: bytes, port: str | None = None, note: str = "") -> None:
        port = port or self.SEND_PORT
        if self.monitor:
            self.monitor("out", port, data, note)
        if self.send_callback:
            self.send_callback(bytes(data), port)

    def changed(self, what: str) -> None:
        for listener in self.listeners:
            listener(what)

    def _send(self, control: Control, value: int, off: bool = False) -> None:
        value = max(0, min(127, int(value)))
        if control.msg == "cc":
            self.emit(bytes([CC | control.channel, control.number, value]))
        else:
            self.emit(bytes([(NOTE_OFF if off else NOTE_ON) | control.channel, control.number, value]))

    # Controls -> MIDI

    def press_button(self, key: int) -> None:
        control = self.by_key.get(key)
        if control and control.number >= 0:
            self._send(control, 127)

    def release_button(self, key: int) -> None:
        control = self.by_key.get(key)
        if control and control.number >= 0:
            self._send(control, 0, off=True)

    def press_pad(self, key: int, velocity: int) -> None:
        control = self.by_key.get(key)
        if control:
            self.touches[key] = PadTouch(max(1, min(127, int(velocity))))
            self._send(control, self.touches[key].velocity)

    def pad_pressure(self, key: int, pressure: int) -> None:
        control, touch = self.by_key.get(key), self.touches.get(key)
        if control is None or touch is None:
            return
        pressure = max(0, min(127, int(pressure)))
        if pressure == touch.pressure:
            return
        touch.pressure = pressure
        if self.poly_pressure:
            self.emit(bytes([POLY_AT | control.channel, control.number, pressure]))
        else:
            self.emit(bytes([CHANNEL_AT | control.channel, pressure]))

    def release_pad(self, key: int) -> None:
        control, touch = self.by_key.get(key), self.touches.pop(key, None)
        if control is None or touch is None:
            return
        if touch.pressure:
            if self.poly_pressure:
                self.emit(bytes([POLY_AT | control.channel, control.number, 0]))
            else:
                self.emit(bytes([CHANNEL_AT | control.channel, 0]))
        self._send(control, 0, off=True)

    def turn_encoder(self, key: int, steps: int) -> None:
        control = self.by_key.get(key)
        if control is None or not steps:
            return
        if control.relative:
            steps = max(-63, min(63, int(steps)))
            self._send(control, steps + 128 if steps < 0 else steps)
            return
        value = max(0, min(127, self.knob_values[key] + int(steps)))
        if value != self.knob_values[key]:
            self.knob_values[key] = value
            if key in self.rings:
                self.rings[key].value = value
                self.changed("rings")
            self._send(control, value)

    def move_fader(self, key: int, value: int) -> None:
        control = self.by_key.get(key)
        value = max(0, min(127, int(value)))
        if control is None or self.fader_values.get(key) == value:
            return
        self.fader_values[key] = value
        self._send(control, value)
        self.changed("faders")

    # ------------------------------------------------------------------------------------------
    # MIDI from the host

    def receive(self, data: bytes, port: str | None = None) -> None:
        port = port or self.SEND_PORT
        parser = self.parsers.setdefault(port, MidiParser())
        for message in parser.feed(data):
            self.handle_message(message, port)

    def handle_message(self, msg: bytes, port: str) -> None:
        note = ""
        status = msg[0]
        if status == 0xF0:
            if msg == IDENTITY_REQUEST or (len(msg) == 6 and msg[1] == 0x7E and msg[3:5] == b"\x06\x01"):
                reply = self.identity_reply()
                if reply:
                    self.emit(reply, port, "identity reply")
                note = "identity request"
            else:
                note = self.handle_sysex(msg, port) or "unhandled SysEx"
                if note == "unhandled SysEx":
                    self.remember_unknown(msg)
        elif status == 0xF8:
            return
        elif len(msg) == 3 and status < 0xF0:
            kind, channel, number, value = status & 0xF0, status & 0x0F, msg[1], msg[2]
            if kind == CC and number in self.ring_ccs and channel == 0:
                key, what = self.ring_ccs[number]
                ring = self.rings[key]
                if what == "value":
                    ring.value = value
                    self.knob_values[key] = value
                else:
                    ring.mode = value
                self.changed("rings")
            elif kind in (NOTE_ON, NOTE_OFF, CC):
                target = self.led_for("cc" if kind == CC else "note", channel, number)
                if target is None:
                    self.remember_unknown(msg)
                    note = "no LED"
                else:
                    self.set_led(target[0], target[1], 0 if kind == NOTE_OFF else value)
        if self.monitor:
            self.monitor("in", port, msg, note)

    def remember_unknown(self, msg: bytes) -> None:
        self.unknown.append(bytes(msg))
        del self.unknown[:-100]

    def set_led(self, key: int, channel: int, value: int) -> None:
        led = self.leds[key]
        led.rgb = None
        if channel == 0:
            led.value = value
            led.anim_channel = 0
        else:
            led.anim_value = value
            led.anim_channel = channel
            led.started = self.clock()
        self.changed("leds")

    # ------------------------------------------------------------------------------------------
    # What the device shows

    def has_animation(self) -> bool:
        return any(led.anim_channel for led in self.leds.values()) or any(
            led.value == 2 for key, led in self.leds.items() if self.by_key[key].led == "single")

    def base_rgb(self, control: Control, value: int) -> tuple:
        if control.led == "rgb":
            return velocity_rgb(value)
        if control.led == "ab":
            return {1: (255, 210, 0), 2: (255, 110, 0)}.get(value, (0, 0, 0))
        if control.led == "single":
            return control.colour if value > 0 else (0, 0, 0)
        return (0, 0, 0)

    def led_rgb(self, key: int, now: float | None = None) -> tuple:
        control = self.by_key.get(key)
        if control is None or control.led == "none":
            return (0, 0, 0)
        now = self.clock() if now is None else now
        led = self.leds[key]
        if led.rgb is not None:
            return led.rgb
        start = self.base_rgb(control, led.value)
        if control.led == "single" and led.value == 2:
            return start if (now / 0.25) % 2.0 < 1.0 else (0, 0, 0)
        if not led.anim_channel:
            return start
        target = self.base_rgb(control, led.anim_value)
        kind, seconds = self.animation(led.anim_channel)
        elapsed = max(0.0, now - led.started)
        if kind == "oneshot":
            f = min(1.0, elapsed / seconds)
        elif kind == "pulse":
            # the target colour pulsing from dark to full
            phase = (elapsed / seconds) % 1.0
            f = 1.0 - abs(2.0 * phase - 1.0)
            return tuple(int(c * (0.15 + 0.85 * f)) for c in target)
        else:  # blink / flash between the steady and the animation colour
            f = 1.0 if (elapsed / (seconds / 2)) % 2.0 < 1.0 else 0.0
        return tuple(int(a + (b - a) * f) for a, b in zip(start, target))

    def encoder_state(self, key: int) -> dict:
        """The ring of an encoder for the shared Encoder component (mode 0: no ring)."""
        ring = self.rings.get(key)
        if ring is None or ring.mode == 0:
            return {"hasValue": False, "value": 0.0, "touched": False, "bipolar": False, "dot": False, "name": "", "text": ""}
        return {"hasValue": True, "value": ring.value / 127.0, "touched": False, "bipolar": ring.mode == 3,
                "dot": ring.mode == 1, "name": "", "text": str(ring.value)}
