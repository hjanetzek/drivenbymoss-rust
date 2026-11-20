# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The APC40 mkII and Launchpad Pro Mk3 simulators without Qt: the protocol, the sync with the Rust
and Java definitions, and a run against the real core in the reference host (in-process, no ALSA;
skipped without the built library)."""

import os
import re
import sys
import tempfile
import unittest

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
REPO = os.path.abspath(os.path.join(ROOT, "..", ".."))
sys.path.insert(0, ROOT)

from midi_simulator import DEVICES  # noqa: E402
from midi_simulator.apc40mk2 import Apc40Mk2  # noqa: E402
from midi_simulator.lppromk3 import HEADER, LaunchpadProMk3  # noqa: E402
from sim_common.palettes import VELOCITY_128  # noqa: E402

JAVA_PALETTE = os.path.expanduser("~/bastel/github/DrivenByMoss/src/main/java/de/mossgrabers/controller/ableton/push/controller/PushColorManager.java")
APC_SURFACE = os.path.join(REPO, "src", "controller", "akai", "apc", "surface.rs")
APC_DEFINITION = os.path.join(REPO, "src", "controller", "akai", "apc", "definition.rs")
LP_DEFINITION = os.path.join(REPO, "src", "controller", "launchpad", "definition.rs")
LP_SURFACE = os.path.join(REPO, "src", "controller", "launchpad", "surface.rs")
LP_SETUP = os.path.join(REPO, "src", "controller", "launchpad", "setup.rs")


class Clock:
    def __init__(self):
        self.t = 100.0

    def __call__(self):
        return self.t


class Base(unittest.TestCase):
    def make(self, cls, **kwargs):
        self.sent: list[bytes] = []
        self.clock = Clock()
        return cls(send=lambda d, p: self.sent.append(d), clock=self.clock, **kwargs)

    def key(self, d, msg, channel, number):
        return d.by_message[(msg, channel, number)]


class SyncTest(unittest.TestCase):
    @unittest.skipUnless(os.path.exists(JAVA_PALETTE), "Java DrivenByMoss not found")
    def test_palette_matches_java(self):
        sys.path.insert(0, os.path.join(ROOT, "tools"))
        import gen_palettes
        with open(JAVA_PALETTE) as f:
            self.assertEqual(tuple(gen_palettes.parse(f.read())), VELOCITY_128)

    @unittest.skipUnless(os.path.exists(APC_SURFACE), "the Rust sources are not there")
    def test_apc_constants(self):
        with open(APC_SURFACE) as f:
            rust = {name: int(value, 0) for name, value in re.findall(r"pub const (APC_\w+): i32 = (0x[0-9A-Fa-f]+|\d+);", f.read())}
        d = Apc40Mk2()
        checked = 0
        for c in d.controls:
            if not c.const:
                continue
            value = rust[c.const]
            if c.const.endswith("_1"):
                self.assertIn(c.number - value, range(8), c)
            else:
                self.assertEqual(c.number, value, c)
            checked += 1
        self.assertGreater(checked, 80)
        with open(APC_DEFINITION) as f:
            text = f.read()
        self.assertIn(d.UUID, text)
        self.assertIn('"APC40 mkII"', text)  # the discovery name; the port is "<name> MIDI 1"
        self.assertTrue(d.PORTS["midi"].startswith("APC40 mkII MIDI "))

    @unittest.skipUnless(os.path.exists(LP_DEFINITION), "the Rust sources are not there")
    def test_launchpad_buttons(self):
        with open(LP_DEFINITION) as f:
            text = f.read()
        defaults = text[text.index("// Defaults of all models"):text.index("match model {")]
        pro3 = text[text.index("LaunchpadModel::ProMk3 => {\n                definition.set_button"):]
        pro3 = pro3[:pro3.index("}")]
        buttons = {name: int(n) for name, n in re.findall(r"set_button\(B::(\w+), (\d+)\)", defaults)}
        buttons.update({name: int(n) for name, n in re.findall(r"set_button\(B::(\w+), (\d+)\)", pro3)})
        shifted = {name: int(n) for name, n in re.findall(r"set_shifted_button\(B::(\w+), (\d+)\)", pro3)}
        scenes = [int(n) for n in re.search(r"SCENE_CONTROLS: \[i32; 8\] = \[([\d, ]+)\]", text).group(1).split(",")]
        with open(LP_SURFACE) as f:
            surface = f.read()
        track1 = int(re.search(r"LAUNCHPAD_TRACK1: i32 = (\d+)", surface).group(1))
        fixed = int(re.search(r"PRO3_LAUNCHPAD_FIXED_LENGTH: i32 = (\d+)", surface).group(1))
        with open(LP_SETUP) as f:
            setup = f.read()
        extra = [int(n) for n in re.findall(r'"(?:Drum Seq|Sequencer)",\s*shared\([^)]*\)\),\s*(\d+),', setup)]
        core = set(buttons.values()) | set(scenes) | {track1 + i for i in range(8)} | {fixed} | set(extra)
        self.assertEqual(sorted(extra), [95, 97])
        d = LaunchpadProMk3()
        simulated = {c.number for c in d.controls if c.kind == "button" and c.number >= 0}
        self.assertEqual(simulated, core)
        self.assertTrue(set(shifted.values()) <= simulated)  # the shifted buttons are Shift + a button
        self.assertIn('"F0 00 20 29 02 0E "', text)
        self.assertEqual(bytes(HEADER).hex(" ").upper() + " ", "F0 00 20 29 02 0E ")
        self.assertIn(d.UUID, text)
        self.assertIn('"Launchpad Pro MK3 LPProMK3 MIDI"', text)
        self.assertIn('"Launchpad Pro MK3 MIDI 1"', text)
        self.assertIn("ProMk3 | LaunchpadModel::X | LaunchpadModel::MiniMkIII => \"0E 01\"", text)

    def test_layouts(self):
        for model, cls in DEVICES.items():
            d = cls()
            w, h = d.SURFACE
            self.assertTrue(all(c.x >= 0 and c.y >= 0 and c.x + c.w <= w and c.y + c.h <= h for c in d.controls), model)
            messages = [(c.msg, c.channel, c.number) for c in d.controls if c.number >= 0]
            self.assertEqual(len(messages), len(set(messages)), model)


class ApcTest(Base):
    def test_buttons_pads_knobs_faders(self):
        d = self.make(Apc40Mk2)
        d.press_button(self.key(d, "note", 3, 0x31))  # solo of track 4
        d.release_button(self.key(d, "note", 3, 0x31))
        pad = self.key(d, "note", 0, 0)
        d.press_pad(pad, 100)
        d.release_pad(pad)
        knob = self.key(d, "cc", 0, 0x30)
        d.turn_encoder(knob, 5)
        d.turn_encoder(knob, -10)  # absolute, at least 0
        d.turn_encoder(self.key(d, "cc", 0, 0x0D), -1)  # tempo: relative
        d.move_fader(self.key(d, "cc", 2, 0x07), 90)
        d.press_button(self.key(d, "cc", 0, 0x40))  # footswitch: a CC
        self.assertEqual(self.sent, [b"\x93\x31\x7F", b"\x83\x31\x00", b"\x90\x00\x64", b"\x80\x00\x00", b"\xB0\x30\x05",
                                     b"\xB0\x30\x00", b"\xB0\x0D\x7F", b"\xB2\x07\x5A", b"\xB0\x40\x7F"])

    def test_rings(self):
        d = self.make(Apc40Mk2)
        knob = self.key(d, "cc", 0, 0x10)
        self.assertFalse(d.encoder_state(knob)["hasValue"])
        d.receive(b"\xB0\x18\x03")  # pan mode
        d.receive(b"\xB0\x10\x40")
        state = d.encoder_state(knob)
        self.assertTrue(state["hasValue"] and state["bipolar"])
        self.assertAlmostEqual(state["value"], 64 / 127)
        d.receive(b"\xB0\x18\x01")
        self.assertTrue(d.encoder_state(knob)["dot"])
        # The knob continues from the value of the ring
        d.turn_encoder(knob, 1)
        self.assertEqual(self.sent[-1], b"\xB0\x10\x41")
        self.assertEqual(d.unknown, [])

    def test_leds(self):
        d = self.make(Apc40Mk2)
        pad = self.key(d, "note", 0, 39)
        d.receive(b"\x90\x27\x05")
        self.assertEqual(d.led_rgb(pad), VELOCITY_128[5])
        d.receive(b"\x9C\x27\x15")  # blinking (channel 13), from red to green
        t = self.clock.t
        colours = {d.led_rgb(pad, t + i * 0.01) for i in range(60)}
        self.assertEqual(colours, {VELOCITY_128[5], VELOCITY_128[0x15]})
        d.receive(b"\x90\x27\x05")
        self.assertFalse(d.has_animation())
        d.receive(b"\x90\x52\x15")  # scene 1: RGB
        self.assertEqual(d.led_rgb(self.key(d, "note", 0, 0x52)), VELOCITY_128[0x15])
        arm = self.key(d, "note", 5, 0x30)
        d.receive(b"\x95\x30\x01")  # arm of track 6
        self.assertNotEqual(d.led_rgb(arm), (0, 0, 0))
        d.receive(b"\x95\x30\x02")  # blinking
        self.assertTrue(d.has_animation())
        d.receive(b"\x92\x42\x02")  # A/B of track 3: B
        self.assertEqual(d.led_rgb(self.key(d, "note", 2, 0x42)), (255, 110, 0))
        self.assertEqual(d.led_rgb(self.key(d, "note", 1, 0x42)), (0, 0, 0))
        self.assertEqual(d.unknown, [])

    def test_sysex(self):
        d = self.make(Apc40Mk2)
        d.receive(bytes.fromhex("F0 47 7F 29 60 00 04 41 08 02 01 F7"))
        self.assertEqual(d.mode, "Ableton Live mode")
        d.receive(bytes.fromhex("F0 7E 7F 06 01 F7"))
        self.assertEqual(len(self.sent[-1]), 15)
        self.assertEqual(self.sent[-1][5], 0x47)


class LaunchpadTest(Base):
    def test_controls(self):
        d = self.make(LaunchpadProMk3)
        pad = self.key(d, "note", 0, 11)
        d.press_pad(pad, 90)
        d.pad_pressure(pad, 40)
        d.release_pad(pad)
        d.press_button(self.key(d, "cc", 0, 90))
        d.release_button(self.key(d, "cc", 0, 90))
        self.assertEqual(self.sent, [b"\x90\x0B\x5A", b"\xA0\x0B\x28", b"\xA0\x0B\x00", b"\x90\x0B\x00", b"\xB0\x5A\x7F",
                                     b"\xB0\x5A\x00"])
        c = self.make(LaunchpadProMk3, poly_pressure=False)
        pad = self.key(c, "note", 0, 88)
        c.press_pad(pad, 90)
        c.pad_pressure(pad, 40)
        self.assertEqual(self.sent[-1], b"\xD0\x28")
        # The top right pad is note 88, the bottom left 11
        self.assertLess(c.by_key[pad].y, c.by_key[self.key(c, "note", 0, 11)].y)

    def test_leds(self):
        d = self.make(LaunchpadProMk3)
        pad = self.key(d, "note", 0, 88)
        d.receive(b"\x90\x58\x05")
        self.assertEqual(d.led_rgb(pad), VELOCITY_128[5])
        d.receive(b"\x91\x58\x15")  # flashing between red and green
        colours = {d.led_rgb(pad, self.clock.t + i * 0.01) for i in range(60)}
        self.assertEqual(colours, {VELOCITY_128[5], VELOCITY_128[0x15]})
        d.receive(b"\xB0\x14\x15")  # Play (CC 20)
        self.assertEqual(d.led_rgb(self.key(d, "cc", 0, 20)), VELOCITY_128[0x15])
        # The SysEx LED update of the core: static, flashing (B, A), pulsing, RGB; the logo
        d.receive(HEADER + bytes([0x03, 0x00, 11, 45, 0x01, 12, 0x15, 0x05, 0x02, 13, 0x2D, 0x03, 14, 0x7F, 0x00, 0x40,
                                  0x00, 99, 3, 0xF7]))
        self.assertEqual(d.led_rgb(self.key(d, "note", 0, 11)), VELOCITY_128[45])
        flash = d.leds[self.key(d, "note", 0, 12)]
        self.assertEqual((flash.value, flash.anim_value, flash.anim_channel), (5, 0x15, 1))
        self.assertEqual(d.leds[self.key(d, "note", 0, 13)].anim_channel, 2)
        self.assertEqual(d.led_rgb(self.key(d, "note", 0, 14)), (255, 0, 129))  # 0x40 * 2 + 1: 0..127 scaled to 0..255
        self.assertEqual(d.led_rgb(d.logo_key), VELOCITY_128[3])
        self.assertEqual(d.unknown, [])

    def test_modes_and_identity(self):
        d = self.make(LaunchpadProMk3)
        d.receive(HEADER + b"\x10\x00\xF7")
        d.receive(HEADER + b"\x0E\x01\xF7")
        self.assertTrue(d.programmer)
        self.assertEqual(d.mode, "programmer mode, standalone")
        d.receive(bytes.fromhex("F0 7E 7F 06 01 F7"))
        reply = self.sent[-1]
        # DeviceInquiry (long form, 17 bytes): the core prints the revision digits as the version
        self.assertEqual(len(reply), 17)
        self.assertEqual(reply[5:8], b"\x00\x20\x29")
        self.assertEqual(list(reply[12:16]), [0, 5, 3, 1])


def library_available() -> bool:
    lib = os.environ.get("DBM_LIBRARY") or os.path.join(REPO, "target", "debug", "libdrivenbymoss.so")
    return os.path.exists(lib)


@unittest.skipUnless(library_available(), "the core library is not built (cargo build)")
class CoreTest(unittest.TestCase):
    """The real core against the device model, in-process: everything the core sends must be
    understood (no unknown messages), every control must be handled (no core errors)."""

    def run_core(self, cls):
        sys.path.insert(0, os.path.join(REPO, "bindings", "python"))
        from dbm_host import Bridge, SimDaw
        daw = SimDaw().build(tracks=[("Synth", "INSTRUMENT"), ("Drums", "INSTRUMENT"), ("Bass", "AUDIO")],
                             effect_tracks=["FX 1"], scenes=8)
        daw.tracks[0].select()
        bridge_box = []
        device = cls(send=lambda data, port: bridge_box[0].receive(data))
        with tempfile.TemporaryDirectory() as config:
            bridge = Bridge(daw, cls.UUID, config, "Bitwig Studio 5.3")
            bridge_box.append(bridge)
            bridge.on_midi_out = lambda data: device.receive(bytes(data))
            bridge.start()
            bridge.wait_for_startup()
            bridge.advance(300)
            self.assertEqual(device.unknown, [], [m.hex(" ") for m in device.unknown[:5]])
            lit = [k for k, led in device.leds.items() if led.value or led.rgb]
            self.assertGreater(len(lit), 10)
            for c in device.controls:
                if c.number < 0:
                    continue
                if c.kind == "pad":
                    device.press_pad(c.key, 100)
                    device.release_pad(c.key)
                elif c.kind == "button":
                    device.press_button(c.key)
                    bridge.advance(30)
                    device.release_button(c.key)
                elif c.kind == "encoder":
                    device.turn_encoder(c.key, 3)
                elif c.kind == "fader":
                    device.move_fader(c.key, 64)
                bridge.advance(30)
            # Shift-combinations, views and modes may have changed; leave them and look
            # No errors of the core (e.g. "CC .. should be handled in framework" for an unbound CC)
            self.assertEqual(bridge.core_errors(), [])
            self.assertEqual(device.unknown, [], [m.hex(" ") for m in device.unknown[:5]])
            running = {"mode": device.mode, "programmer": getattr(device, "programmer", None)}
            bridge.close()
        return device, running

    def test_apc40mk2(self):
        device, running = self.run_core(Apc40Mk2)
        self.assertEqual(running["mode"], "Ableton Live mode")
        self.assertTrue(any(r.mode for r in device.rings.values()))

    def test_lppromk3(self):
        device, running = self.run_core(LaunchpadProMk3)
        self.assertTrue(running["programmer"])
        self.assertIn("standalone", running["mode"])
        # On exit the core resets the Launchpad to its default mode ("0E 00")
        self.assertFalse(device.programmer)


if __name__ == "__main__":
    unittest.main()
