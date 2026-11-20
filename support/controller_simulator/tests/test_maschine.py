# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The protocol of the Maschine simulators without Qt: controls -> template MIDI, LED feedback,
displays, strips, meters and dbm-nihost's handshake."""

import os
import sys
import unittest

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
sys.path.insert(0, ROOT)

from maschine_simulator import layout, ni, templates  # noqa: E402
from maschine_simulator.device import MaschineDevice  # noqa: E402

HEADER = os.path.join(ROOT, "..", "linux", "ni_host", "src", "template_map.h")
JAVA = os.path.expanduser("~/bastel/github/DrivenByMoss")
REPO = os.path.abspath(os.path.join(ROOT, "..", ".."))


class Base(unittest.TestCase):
    def make(self, model):
        self.sent: list[bytes] = []
        self.ports: list[str] = []

        def send(data, port):
            self.sent.append(data)
            self.ports.append(port)
        return MaschineDevice(model, send=send)

    def key(self, d, template=None, label=None, kind=None):
        for c in d.controls:
            if (template is None or c.template == template) and (label is None or c.label == label) \
                    and (kind is None or c.kind == kind):
                return c.key
        raise KeyError(template or label)


class TemplateTest(unittest.TestCase):
    @unittest.skipUnless(os.path.exists(HEADER), "dbm-nihost's template_map.h not found")
    def test_in_sync_with_dbm_nihost(self):
        sys.path.insert(0, os.path.join(ROOT, "tools"))
        import gen_maschine_templates
        with open(HEADER) as f:
            tables = gen_maschine_templates.parse(f.read())
        self.assertEqual(tuple(tables["mk2"]), templates.MK2)
        self.assertEqual(tuple(tables["jam"]), templates.JAM)

    @unittest.skipUnless(os.path.isdir(os.path.join(JAVA, "resources", "Maschine Mk3")), "the Java DrivenByMoss is not there")
    def test_in_sync_with_the_ni_templates(self):
        sys.path.insert(0, os.path.join(ROOT, "tools"))
        import gen_maschine_templates
        for name, rel in gen_maschine_templates.NI_TEMPLATES:
            rows, disabled = gen_maschine_templates.parse_ni_template(os.path.join(JAVA, rel))
            self.assertEqual(tuple(rows), getattr(templates, name.upper()), name)
            self.assertEqual(frozenset(disabled), getattr(templates, name.upper() + "_DISABLED"), name)

    def test_mk3_templates_match_the_core(self):
        """The CCs of the Mk3 template are the core's (src/controller/maschine/mk/surface.rs)."""
        surface = os.path.join(REPO, "src", "controller", "maschine", "mk", "surface.rs")
        if not os.path.exists(surface):
            self.skipTest("the Rust sources are not there")
        import re
        with open(surface) as f:
            rust = {name: int(value) for name, value in re.findall(r"pub const (\w+): i32 = (\d+);", f.read())}
        mk3 = {e[0]: e for e in templates.MK3 if e[1] == "input"}
        pairs = {"Play": "PLAY", "Rec": "REC", "Stop": "STOP", "Restart": "RESTART", "Erase": "ERASE", "Tap": "TAP_METRO",
                 "Follow": "FOLLOW", "Encoder": "ENCODER", "EncoderPush": "ENCODER_PUSH", "EncoderCap": "ENCODER_TOUCH",
                 "EncoderUp": "CURSOR_UP", "EncoderRight": "CURSOR_RIGHT", "EncoderDown": "CURSOR_DOWN",
                 "EncoderLeft": "CURSOR_LEFT", "Macro": "GROUP", "Auto": "AUTO", "Lock": "LOCK", "NoteRep": "NOTE_REPEAT",
                 "File": "PROJECT", "Setting": "FAVORITES", "Browser": "BROWSER", "Channel": "CHANNEL",
                 "Arranger": "ARRANGER", "Mixer": "MIXER", "Volume": "VOLUME", "Plugin": "PLUGIN", "Swing": "SWING",
                 "Sampling": "SAMPLING", "Tempo": "TEMPO", "Pitch": "PITCH", "Mod": "MOD", "Perform": "PERFORM",
                 "Notes": "NOTES", "FixedVel": "FIXED_VEL", "PadMode": "PAD_MODE", "Keyboard": "KEYBOARD",
                 "Chords": "CHORDS", "Step": "STEP", "Scene": "SCENE", "Pattern": "PATTERN", "Events": "EVENTS",
                 "Variation": "VARIATION", "Duplicate": "DUPLICATE", "Select": "SELECT", "Solo": "SOLO", "Mute": "MUTE",
                 "Left": "PAGE_LEFT", "Right": "PAGE_RIGHT", "Touchstrip": "TOUCHSTRIP", "Button1": "MODE_BUTTON_1",
                 "Knob1": "MODE_KNOB_1", "KnobCap1": "MODE_KNOB_TOUCH_1", "GroupA": "BANK_1"}
        for ident, const in pairs.items():
            self.assertEqual(mk3[ident][4], rust[const], ident)
        # The touch strip touch: the core binds CC 2, the template has poly aftertouch 2, disabled
        self.assertEqual(rust["TOUCHSTRIP_TOUCH"], 2)
        self.assertEqual(mk3["TouchstripCap"][2:5], ("polyat", 0, 2))
        self.assertIn("TouchstripCap", templates.MK3_DISABLED)

    def test_layout(self):
        for model in layout.MODELS:
            controls = layout.build_controls(model)
            w, h = layout.SURFACE[model]
            self.assertTrue(all(c.x >= 0 and c.y >= 0 and c.x + c.w <= w and c.y + c.h <= h for c in controls), model)
            self.assertEqual([c.key for c in controls], list(range(len(controls))))
            used = [c.template for c in controls if c.template]
            self.assertEqual(len(used), len(set(used)), model)
        self.assertEqual(len([c for c in layout.build_controls("mk2") if c.kind == "pad"]), 16)
        self.assertEqual(len([c for c in layout.build_controls("jam") if c.kind == "pad"]), 64)
        self.assertEqual(layout.midi_names("mk2", " (sim)"), ("Maschine MK2 (sim)", "Maschine MK2 In (sim)", "Maschine MK2 Out (sim)"))
        self.assertEqual(layout.midi_names("jam")[2], "Maschine Jam - 1 Output")


class Mk2Test(Base):
    def test_handshake(self):
        d = self.make("mk2")
        d.receive(ni.QUERY)
        self.assertEqual(self.sent, [])  # not attached: no answer, as dbm-nihost without the device
        d.attach()
        self.assertEqual(self.sent, [bytes.fromhex("F0 7D 44 42 4D 11 01 F7")])
        d.receive(bytes.fromhex("F0 7D 44 42 4D 11 F7"))
        self.assertEqual(self.sent[-1], ni.MK2_HELLO)
        d.detach()
        self.assertEqual(self.sent[-1], bytes.fromhex("F0 7D 44 42 4D 13 F7"))
        d.detach()
        self.assertEqual(len(self.sent), 3)
        self.assertEqual(set(self.ports), {"in"})

    def test_buttons_and_shift(self):
        d = self.make("mk2")
        play = self.key(d, "Play")
        d.press_button(play)
        d.release_button(play)
        self.assertEqual(self.sent, [b"\xB0\x39\x7F", b"\xB0\x39\x00"])
        self.sent.clear()
        # The 7th display button is a note
        d.press_button(self.key(d, "Button7"))
        d.release_button(self.key(d, "Button7"))
        self.assertEqual(self.sent, [b"\x90\x1C\x7F", b"\x80\x1C\x00"])
        self.sent.clear()
        shift = self.key(d, label="Shift")
        d.press_button(shift)
        self.assertEqual(d.led_rgb(shift), (230, 230, 230))
        d.release_button(shift)
        self.assertEqual(self.sent, [bytes.fromhex("F0 7D 44 42 4D 12 01 F7"), bytes.fromhex("F0 7D 44 42 4D 12 00 F7")])
        self.assertEqual(d.led_rgb(shift), (0, 0, 0))

    def test_encoders_and_pads(self):
        d = self.make("mk2")
        d.turn_encoder(self.key(d, "Knob1"), 1)
        d.turn_encoder(self.key(d, "Knob1"), -2)
        d.turn_encoder(self.key(d, "Dial"), 100)
        d.touch_encoder(self.key(d, "Knob1"), True)  # the Mk2's encoders have no touch
        pad1 = self.key(d, "Pad1")
        d.press_pad(pad1, 100)
        d.pad_pressure(pad1, 50)
        d.release_pad(pad1)
        self.assertEqual(self.sent, [b"\xB0\x46\x01", b"\xB0\x46\x7E", b"\xB0\x07\x3F", b"\x90\x24\x64", b"\xA0\x24\x32",
                                     b"\x80\x24\x00"])
        # Pad 1 is at the bottom left, pad 16 at the top right
        pads = {c.template: c for c in d.controls if c.kind == "pad"}
        self.assertGreater(pads["Pad1"].y, pads["Pad13"].y)
        self.assertGreater(pads["Pad16"].x, pads["Pad13"].x)

    def test_leds(self):
        d = self.make("mk2")
        d.receive(b"\x90\x24\x1E")  # pad 1: green (palette 30)
        self.assertEqual(d.led_rgb(self.key(d, "Pad1")), ni.PALETTE[30])
        d.receive(b"\x80\x24\x40")
        self.assertEqual(d.led_rgb(self.key(d, "Pad1")), (0, 0, 0))
        d.receive(b"\xB0\x64\x06")  # group A: red
        self.assertEqual(d.led_rgb(self.key(d, "GroupA")), (255, 0, 0))
        d.receive(b"\xB0\x39\x7F")  # Play: white, full
        self.assertEqual(d.led_rgb(self.key(d, "Play")), (235, 235, 225))
        d.receive(b"\x90\x1C\x7F")  # the 7th display button
        self.assertNotEqual(d.led_rgb(self.key(d, "Button7")), (0, 0, 0))
        # Channel 2/3 (saturation, brightness of the template) and other channels are ignored
        d.receive(b"\xB1\x64\x00")
        d.receive(b"\x91\x25\x1E")
        self.assertEqual(d.led_rgb(self.key(d, "GroupA")), (255, 0, 0))
        self.assertEqual(d.led_rgb(self.key(d, "Pad2")), (0, 0, 0))

    def test_display(self):
        d = self.make("mk2")
        line1 = "".join(f"{t:7s}" for t in ("Synth", "Drums", "Bass", "Vocals", "Audio5", "FX 1", "FX 2", "Master"))
        d.receive(ni.MCU_HEADER + bytes([0x14, 0x12, 0]) + line1.encode() + b"\xF7")
        d.receive(ni.MCU_HEADER + bytes([0x14, 0x12, 56 + 7]) + b"-6.0dB\x01" + b"\xF7")
        d.receive(ni.DBM_HEADER + bytes([0x10, 0]) + b">Volume Pan" + b"\xF7")
        left, right = d.display_lines(0), d.display_lines(1)
        self.assertEqual(left[1], line1[:28])
        self.assertEqual(right[1], line1[28:])
        self.assertEqual(left[2], " " * 7 + "-6.0dB " + " " * 14)
        self.assertTrue(left[0].startswith(">Volume Pan"))
        self.assertEqual(len(right[0]), 28)


class JamTest(Base):
    def test_handshake_and_shift(self):
        d = self.make("jam")
        d.attach()
        self.assertEqual(self.sent, [bytes.fromhex("F0 00 21 09 15 00 4D 50 00 01 46 01 F7")])
        d.receive(ni.QUERY)
        self.assertEqual(self.sent[-1], ni.JAM_HELLO)
        shift = self.key(d, label="Shift")
        d.press_button(shift)
        self.assertEqual(self.sent[-1], bytes.fromhex("F0 00 21 09 15 00 4D 50 00 01 4D 01 F7"))

    def test_controls(self):
        d = self.make("jam")
        d.press_button(self.key(d, "BtnA1"))  # top left pad: a note
        d.release_button(self.key(d, "BtnA1"))
        d.press_pad(self.key(d, "BtnH8"), 90)  # bottom right
        d.turn_encoder(self.key(d, "EncBrowse"), -1)
        d.touch_encoder(self.key(d, "EncBrowse"), True)
        d.move_strip(0, 64)
        d.move_strip(0, 100)
        d.touch_strip(0, False)
        self.assertEqual(self.sent, [b"\x90\x5C\x7F", b"\x80\x5C\x00", b"\x90\x2B\x5A", b"\xB0\x39\x7F", b"\xB0\x3B\x7F",
                                     b"\xB0\x14\x7F", b"\xB0\x08\x40", b"\xB0\x08\x64", b"\xB0\x14\x00"])

    def test_leds_and_strips(self):
        d = self.make("jam")
        d.receive(b"\x90\x5C\x2A")  # top left pad, any channel
        d.receive(b"\x9F\x24\x1E")
        self.assertEqual(d.led_rgb(self.key(d, "BtnA1")), ni.PALETTE[0x2A])
        self.assertEqual(d.led_rgb(self.key(d, "BtnA8")), ni.PALETTE[0x1E])
        d.receive(b"\xB0\x50\x06")  # scene 1
        d.receive(b"\xB0\x46\x1E")  # group A
        self.assertEqual(d.led_rgb(self.key(d, "BtnScene1")), (255, 0, 0))
        self.assertEqual(d.led_rgb(self.key(d, "BtnGroupA")), ni.PALETTE[0x1E])
        d.receive(b"\xB0\x5A\x40")  # Play, half brightness
        self.assertEqual(d.led_rgb(self.key(d, "BtnPlay"))[0], 235 * 64 // 127)
        # Strip setup: single, dot, pan, dual (type, colour) and the values
        setup = bytes([0, 6, 1, 6, 2, 6, 3, 6] + [0, 0] * 4)
        d.receive(ni.JAM_HEADER + bytes([ni.JAM_STRIP_SETUP]) + setup + b"\xF7")
        d.receive(ni.JAM_HEADER + bytes([ni.JAM_STRIP_DUAL_VALUES]) + bytes([127] * 8) + b"\xF7")
        for i in range(4):
            d.receive(bytes([0xB0, 8 + i, 127 if i < 2 else 0]))
        self.assertEqual(d.strip_colours(0), [6] * 11)
        self.assertEqual(d.strip_colours(1), [0] * 10 + [6])
        self.assertEqual(d.strip_colours(2), [6] * 6 + [0] * 5)  # from the value (bottom) to the centre
        self.assertEqual(d.strip_colours(3), [0] * 10 + [70])  # no bar, the white dot at the top
        # Level meters (CC 64/65)
        d.receive(b"\xB0\x40\x7F")
        d.receive(b"\xB0\x41\x40")
        self.assertEqual((d.meter_leds(0), d.meter_leds(1)), (8, 4))



class Mk3Test(Base):
    def test_attach_shift_and_no_dbm_nihost_messages(self):
        d = self.make("mk3")
        d.attach()
        self.assertEqual(self.sent, [bytes.fromhex("F0 00 21 09 16 00 4D 50 00 01 46 01 F7")])  # return from host
        d.receive(ni.QUERY)  # dbm-nihost's query: NI's driver ignores it
        self.assertEqual(len(self.sent), 1)
        shift = self.key(d, label="Shift")
        d.press_button(shift)
        d.release_button(shift)
        self.assertEqual(self.sent[1:], [bytes.fromhex("F0 00 21 09 16 00 4D 50 00 01 4D 01 F7"),
                                         bytes.fromhex("F0 00 21 09 16 00 4D 50 00 01 4D 00 F7")])
        d.detach()
        self.assertEqual(len(self.sent), 3)  # no "gone": that is dbm-nihost's

    def test_controls(self):
        d = self.make("mk3")
        knob = self.key(d, "Knob3")
        d.touch_encoder(knob, True)
        d.turn_encoder(knob, -1)
        enc = self.key(d, "Encoder")
        d.touch_encoder(enc, True)
        d.turn_encoder(enc, 2)
        d.press_button(self.key(d, "EncoderPush"))
        d.press_button(self.key(d, "EncoderLeft"))
        d.press_button(self.key(d, "Button7"))  # a note, as on the Mk2
        pad = self.key(d, "Pad16")
        d.press_pad(pad, 80)
        d.pad_pressure(pad, 30)
        d.release_pad(pad)
        self.assertEqual(self.sent, [b"\xB0\x3E\x7F", b"\xB0\x48\x7F", b"\xB0\x09\x7F", b"\xB0\x07\x02", b"\xB0\x08\x7F",
                                     b"\xB0\x21\x7F", b"\x90\x1C\x7F", b"\x90\x33\x50", b"\xA0\x33\x1E", b"\x80\x33\x00"])

    def test_touch_strip(self):
        d = self.make("mk3")
        d.move_strip(0, 64)
        d.touch_strip(0, False)
        # NI's driver sends no touch for the Mk3's strip (disabled in the template)
        self.assertEqual(self.sent, [b"\xB0\x01\x40"])
        e = self.make("mk3")
        e.send_disabled = True
        e.move_strip(0, 64)
        e.touch_strip(0, False)
        self.assertEqual(self.sent, [b"\xB0\x02\x7F", b"\xB0\x01\x40", b"\xB0\x02\x00"])
        e.receive(b"\xB0\x01\x7F")
        self.assertEqual(e.strip_colours(0), [70] * 11)

    def test_leds_and_display(self):
        d = self.make("mk3")
        d.receive(b"\x90\x24\x1E")
        d.receive(b"\xB0\x64\x06")
        d.receive(b"\xB0\x1E\x7F")  # the encoder's up LED
        d.receive(b"\xB0\x39\x7F")  # Play
        self.assertEqual(d.led_rgb(self.key(d, "Pad1")), ni.PALETTE[30])
        self.assertEqual(d.led_rgb(self.key(d, "GroupA")), (255, 0, 0))
        self.assertNotEqual(d.led_rgb(self.key(d, "EncoderUp")), (0, 0, 0))
        self.assertNotEqual(d.led_rgb(self.key(d, "Play")), (0, 0, 0))
        d.receive(ni.MCU_HEADER + bytes([0x14, 0x12, 0]) + b"Synth" + b"\xF7")
        self.assertTrue(d.display_lines(0)[1].startswith("Synth"))
        self.assertEqual(d.unknown, [])


class MikroMk3Test(Base):
    def test_controls_and_leds(self):
        d = self.make("mikromk3")
        d.attach()
        self.assertEqual(self.sent[-1], ni.ni_return_from_host("mikromk3"))
        self.assertNotIn("Shift", [c.label for c in d.controls])  # emulated by the core with Stop
        nav = self.key(d, "Navigation")
        d.touch_encoder(nav, True)
        d.turn_encoder(nav, -3)
        d.press_button(self.key(d, "Navigation Push"))
        d.press_button(self.key(d, "Star"))
        d.move_strip(0, 10)
        d.touch_strip(0, False)
        self.assertEqual(self.sent[1:], [b"\xB0\x09\x7F", b"\xB0\x07\x7D", b"\xB0\x08\x7F", b"\xB0\x27\x7F",
                                         b"\xB0\x02\x7F", b"\xB0\x01\x0A", b"\xB0\x02\x00"])
        d.receive(b"\x90\x33\x06")
        self.assertEqual(d.led_rgb(self.key(d, "Pad16")), (255, 0, 0))
        d.receive(b"\xB0\x01\x40")
        self.assertEqual(d.touchstrip, 64)


if __name__ == "__main__":
    unittest.main()
