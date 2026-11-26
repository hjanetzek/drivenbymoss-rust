# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The protocol of PushDevice: controls -> MIDI, MIDI -> LED state, SysEx (headless)."""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

from push_simulator import layout  # noqa: E402
from push_simulator.device import (AFTERTOUCH_MPE, SYSEX_PUSH2, PushDevice)  # noqa: E402
from push_simulator.midi import MidiParser  # noqa: E402
from push_simulator.palette import DEFAULT_RGB  # noqa: E402


class Clock:
    def __init__(self):
        self.t = 100.0

    def __call__(self):
        return self.t


def sx(*data: int) -> bytes:
    return SYSEX_PUSH2 + bytes(data) + b"\xF7"


class DeviceTest(unittest.TestCase):
    def make(self, model="push2"):
        self.clock = Clock()
        self.sent: list[tuple[bytes, str]] = []
        return PushDevice(model, send=lambda d, p: self.sent.append((d, p)), clock=self.clock)

    def out(self):
        return [d for d, _ in self.sent]

    # Controls -> MIDI

    def test_buttons(self):
        d = self.make()
        d.press_button(85)
        d.release_button(85)
        self.assertEqual(self.out(), [b"\xB0\x55\x7F", b"\xB0\x55\x00"])
        self.assertEqual({p for _, p in self.sent}, {"live"})

    def test_encoder_relative_and_touch(self):
        d = self.make()
        d.touch_encoder(0, True)
        d.turn_encoder(71, 1)
        d.turn_encoder(71, -1)
        d.turn_encoder(71, 5)
        d.turn_encoder(71, -200)
        d.turn_encoder(71, 0)
        d.touch_encoder(0, False)
        self.assertEqual(self.out(), [b"\x90\x00\x7F", b"\xB0\x47\x01", b"\xB0\x47\x7F", b"\xB0\x47\x05", b"\xB0\x47\x41",
                                      b"\x90\x00\x00"])

    def test_pad_channel_and_poly_pressure(self):
        d = self.make()
        d.press_pad(36, 100)
        d.pad_pressure(36, 50)
        d.release_pad(36)
        self.assertEqual(self.out(), [b"\x90\x24\x64", b"\xD0\x32", b"\xD0\x00", b"\x80\x24\x00"])
        self.sent.clear()
        d.receive(sx(0x1E, 0x01))
        d.press_pad(40, 0)  # velocity at least 1
        d.pad_pressure(40, 20)
        d.pad_pressure(40, 20)  # unchanged: nothing sent
        d.release_pad(40)
        self.assertEqual(self.out(), [b"\x90\x28\x01", b"\xA0\x28\x14", b"\xA0\x28\x00", b"\x80\x28\x00"])

    def test_push3_mpe(self):
        d = self.make("push3")
        d.receive(sx(0x1E, 0x02))
        self.assertEqual(d.settings.aftertouch_mode, AFTERTOUCH_MPE)
        d.press_pad(36, 90)
        d.press_pad(37, 90)
        d.pad_slide(36, 0.5, 1.0)
        d.pad_pressure(37, 70)
        d.release_pad(36)
        out = self.out()
        self.assertEqual(out[:3], [b"\xE1\x00\x40", b"\xB1\x4A\x40", b"\x91\x24\x5A"])
        self.assertEqual(out[5], b"\x92\x25\x5A")
        self.assertIn(b"\xD2\x46", out)
        self.assertIn(b"\xB1\x4A\x7F", out)
        self.assertEqual(out[-1], b"\x81\x24\x00")
        # The MPE mode is Push 3 only
        p2 = self.make("push2")
        p2.receive(sx(0x1E, 0x02))
        self.assertFalse(p2.mpe_active)

    def test_touch_strip(self):
        d = self.make()
        d.touch_strip(True)
        d.move_strip(16383)
        d.touch_strip(False)  # default config: auto return to the center
        self.assertEqual(self.out(), [b"\x90\x0C\x7F", b"\xE0\x7F\x7F", b"\x90\x0C\x00", b"\xE0\x00\x40"])
        self.assertEqual(d.strip_finger, 8192)
        levels = d.strip_levels()
        self.assertEqual(levels[15], 1.0)
        # Host LEDs, bar from the bottom (DrivenByMoss' volume mode), value by pitch bend
        d.receive(sx(0x17, 0x01))
        d.receive(b"\xE0\x7F\x7F")
        self.assertEqual(d.strip_levels(), [1.0] * 31)
        d.receive(b"\xE0\x00\x00")
        self.assertEqual(sum(d.strip_levels()), 1.0)
        # Host sends the LEDs by SysEx: 31 LEDs of 3 bits
        d.receive(sx(0x17, 0x03))
        d.receive(sx(0x19, 0x3F, *([0] * 14), 0x07))
        levels = d.strip_levels()
        self.assertEqual(levels[0], 1.0)
        self.assertEqual(levels[1], 1.0)
        self.assertEqual(levels[30], 1.0)
        self.assertEqual(levels[2], 0.0)
        # Without auto return the finger stays
        self.sent.clear()
        d.receive(sx(0x17, 0x00))
        d.move_strip(1000)
        d.touch_strip(False)
        self.assertEqual(self.out()[-1], b"\x90\x0C\x00")

    def test_midi_mode_selects_the_ports(self):
        d = self.make()
        d.receive(sx(0x0A, 0x01))
        d.press_button(3)
        d.receive(sx(0x0A, 0x02))
        d.press_button(3)
        self.assertEqual([p for _, p in self.sent], ["user", "live", "user"])
        p1 = self.make("push1")
        p1.press_button(3)
        self.assertEqual(self.sent[-1][1], "user")

    # MIDI -> LEDs

    def test_pad_leds_from_notes(self):
        d = self.make()
        d.receive(b"\x90\x24\x05")
        self.assertEqual(d.pad_rgb(36), DEFAULT_RGB[5])
        d.receive(b"\x80\x24\x40")
        self.assertEqual(d.pad_rgb(36), (0, 0, 0))
        # Running status: two notes in one chunk
        d.receive(b"\x90\x25\x15\x26\x2D")
        self.assertEqual(d.pad_rgb(37), DEFAULT_RGB[0x15])
        self.assertEqual(d.pad_rgb(38), DEFAULT_RGB[0x2D])

    def test_button_leds_rgb_and_white(self):
        d = self.make()
        d.receive(b"\xB0\x55\x15")  # Play: RGB
        self.assertEqual(d.button_rgb(85), DEFAULT_RGB[0x15])
        d.receive(b"\xB0\x31\x7F")  # Shift: white LED, W of the palette entry
        self.assertEqual(d.button_rgb(49), (254, 254, 254))
        d.receive(b"\xB0\x31\x1E")
        self.assertEqual(d.button_rgb(49), (60, 60, 60))
        d.receive(b"\xB0\x31\x00")
        self.assertEqual(d.button_rgb(49), (0, 0, 0))

    def test_blink_and_pulse(self):
        d = self.make()
        d.receive(b"\x90\x63\x05")  # red on channel 1
        d.receive(b"\x9E\x63\x15")  # blinking quarter (channel 15) to green
        t0 = self.clock.t
        self.assertTrue(d.has_animation())
        self.assertEqual(d.pad_rgb(99, t0 + 0.1), DEFAULT_RGB[0x15])
        self.assertEqual(d.pad_rgb(99, t0 + 0.6), DEFAULT_RGB[5])  # 120 BPM: a quarter is 0.5 s
        d.receive(b"\x9A\x63\x2D")  # pulsing half note to blue
        t0 = self.clock.t
        start, mid = d.pad_rgb(99, t0), d.pad_rgb(99, t0 + 1.0)
        self.assertEqual(start, DEFAULT_RGB[5])
        self.assertEqual(mid, DEFAULT_RGB[0x2D])
        d.receive(b"\x90\x63\x05")  # channel 1 stops the animation
        self.assertFalse(d.has_animation())
        # One shot transition (channel 2: a 24th) ends on the target colour
        d.receive(b"\x91\x63\x15")
        self.assertEqual(d.pad_rgb(99, self.clock.t + 5), DEFAULT_RGB[0x15])

    def test_midi_clock_sets_the_animation_tempo(self):
        d = self.make()
        for _ in range(48):
            self.clock.t += 60.0 / 90.0 / 24.0
            d.receive(b"\xF8")
        self.assertAlmostEqual(d.bpm, 90.0, places=3)

    def test_led_brightness(self):
        d = self.make()
        d.receive(b"\x90\x24\x03")
        d.receive(sx(0x06, 0x00))
        self.assertEqual(d.pad_rgb(36), (63, 63, 63))

    # SysEx

    def test_identity(self):
        d = self.make()
        d.receive(b"\xF0\x7E\x7F\x06\x01\xF7", "user")
        reply, port = self.sent[-1]
        self.assertEqual(port, "user")
        self.assertEqual(reply[:12], bytes.fromhex("F0 7E 01 06 02 00 21 1D 67 32 02 00"))
        self.assertEqual(len(reply), 23)
        # As DeviceInquiry/handleDeviceInquiryResponse read it: data[12] major, [13] minor
        self.assertEqual((reply[12], reply[13]), (1, 0))
        p1 = self.make("push1")
        p1.receive(b"\xF0\x7E\x7F\x06\x01\xF7")
        reply = self.sent[-1][0]
        self.assertEqual(len(reply), 35)
        self.assertEqual(reply[10] + reply[9] * 10, 1)

    def test_palette_read_write_reapply(self):
        d = self.make()
        d.receive(sx(0x04, 0x05))
        reply = self.sent[-1][0]
        # ColorPaletteEntry.isValid: 17 bytes, data[6] == 0x04, index at 7
        self.assertEqual(len(reply), 17)
        self.assertEqual(reply[6:8], b"\x04\x05")
        r, g, b = DEFAULT_RGB[5]
        self.assertEqual(reply[8] | reply[9] << 7, r)
        self.assertEqual(reply[10] | reply[11] << 7, g)
        self.assertEqual(reply[12] | reply[13] << 7, b)
        # Write entry 5 = (200, 100, 50, 255): read back at once, shown after re-apply
        d.receive(b"\x90\x24\x05")
        d.receive(sx(0x03, 0x05, 200 & 127, 1, 100, 0, 50, 0, 127, 1))
        d.receive(sx(0x04, 0x05))
        reply = self.sent[-1][0]
        self.assertEqual(reply[8] | reply[9] << 7, 200)
        self.assertEqual(reply[14] | reply[15] << 7, 255)
        self.assertEqual(d.pad_rgb(36), DEFAULT_RGB[5])
        d.receive(sx(0x05))
        self.assertEqual(d.pad_rgb(36), (200, 100, 50))

    def test_settings_and_gets(self):
        d = self.make()
        d.receive(sx(0x08, 0x7F, 0x01))
        self.assertEqual(d.settings.display_brightness, 255)
        d.receive(sx(0x09))
        self.assertEqual(self.sent[-1][0], sx(0x09, 0x7F, 0x01))
        d.receive(sx(0x06, 0x40))
        d.receive(sx(0x07))
        self.assertEqual(self.sent[-1][0], sx(0x07, 0x40))
        d.receive(sx(0x17, 0x7A))
        d.receive(sx(0x18))
        self.assertEqual(self.sent[-1][0], sx(0x18, 0x7A))
        d.receive(sx(0x1B, 33, 0, 31, 0, 0x72, 0x0C, 0x02, 0x10))
        self.assertEqual(d.settings.pad_parameters, bytes([33, 0, 31, 0, 0x72, 0x0C, 0x02, 0x10]))
        for start in range(0, 128, 16):
            d.receive(sx(0x20, start, *range(start, start + 16)))
        self.assertEqual(d.settings.velocity_curve, list(range(128)))
        d.receive(sx(0x14, 0x02, 0x10, 0x05))
        self.assertEqual(d.settings.white_balance[2], 0x10 | 5 << 7)

    def test_push3_settings(self):
        d = self.make("push3")
        d.receive(sx(0x26, 0x07, 0x08, 0x02, 0x00))
        d.receive(sx(0x37, 0x26, 0x43, *([0] * 13)))
        self.assertEqual(d.settings.push3[(0x26, 0x07, 0x08)], [0x02, 0x00])
        self.assertEqual(d.settings.push3[(0x37, 0x26)], 0x43)
        # DrivenByMoss sends the pad curve without the command byte
        d.receive(sx(*([1] * 10 + list(range(10, 128)))))
        self.assertEqual(len(d.settings.push3_pad_curve), 128)
        self.assertIn("without command byte", d.sysex_log[-1].name)

    def test_push1_text_display_and_ribbon(self):
        d = self.make("push1")
        text = "Hello Push 1".ljust(68)
        d.receive(bytes([0xF0, 0x47, 0x7F, 0x15, 0x19, 0x00, 0x45, 0x00]) + text.encode() + b"\xF7")
        self.assertEqual(d.text_lines[1], text)
        d.receive(bytes([0xF0, 0x47, 0x7F, 0x15, 0x1D, 0x00, 0x00, 0xF7]))
        self.assertEqual(d.text_lines[1], " " * 68)
        d.receive(bytes.fromhex("F0 47 7F 15 63 00 01 02 F7"))
        self.assertEqual(d.strip_config, 17)
        d.receive(bytes.fromhex("F0 47 7F 15 5C 00 01 00 F7"))
        self.assertTrue(d.poly_pressure)
        # Two-colour LED: value 4 = red high
        d.receive(b"\xB0\x14\x04")
        self.assertEqual(d.button_rgb(20, 0.0), (255, 0, 0))

    def test_split_sysex(self):
        d = self.make()
        message = sx(0x06, 0x20)
        d.receive(message[:3])
        d.receive(message[3:])
        self.assertEqual(d.settings.led_brightness, 0x20)


class LayoutTest(unittest.TestCase):
    def test_controls(self):
        for model in layout.MODELS:
            controls = layout.build_controls(model)
            pads = sorted(c.note for c in controls if c.kind == "pad")
            self.assertEqual(pads, list(range(36, 100)))
            ccs = [c.cc for c in controls if c.kind == "button"]
            self.assertEqual(len(ccs), len(set(ccs)), model)
            width, height = layout.surface(model)
            self.assertTrue(all(0 <= c.x and 0 <= c.y and c.x + c.w <= width and c.y + c.h <= height for c in controls), model)
        p3 = layout.button_by_cc("push3")
        self.assertIn(92, p3)  # New
        self.assertIn(94, p3)  # jog wheel press
        self.assertNotIn(87, p3)
        self.assertIn(70, layout.encoder_by_cc("push3"))
        self.assertIn(15, layout.encoder_by_cc("push2"))
        self.assertEqual(layout.pad_note(0, 7), 36)
        self.assertEqual(layout.pad_note(7, 0), 99)

    def test_port_names(self):
        self.assertEqual(layout.midi_names("push2"), ("Ableton Push 2", "Ableton Push 2 Live Port", "Ableton Push 2 User Port"))
        self.assertEqual(layout.midi_names("push3", "numbered")[1], "Ableton Push 3 MIDI 1")


class ParserTest(unittest.TestCase):
    def test_parser(self):
        p = MidiParser()
        self.assertEqual(p.feed(b"\x90\x24"), [])
        self.assertEqual(p.feed(b"\x7F\xF8\x25\x00"), [b"\x90\x24\x7F", b"\xF8", b"\x90\x25\x00"])
        self.assertEqual(p.feed(b"\xF0\x01\x02"), [])
        self.assertEqual(p.feed(b"\xF8\x03\xF7\xD0\x10"), [b"\xF8", b"\xF0\x01\x02\x03\xF7", b"\xD0\x10"])


if __name__ == "__main__":
    unittest.main()
