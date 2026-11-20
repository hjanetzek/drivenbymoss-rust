# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The real core against the Maschine simulators, in-process (dbm_host's Bridge with the
simulated DAW, no ALSA, no Qt): everything the core sends must be understood, every control is
operated without a core error. Skipped without the built library (cargo build)."""

import os
import sys
import tempfile
import unittest

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
REPO = os.path.abspath(os.path.join(ROOT, "..", ".."))
sys.path.insert(0, ROOT)

from maschine_simulator.device import MaschineDevice  # noqa: E402

UUIDS = {"mk2": "015E4908-C049-4808-A645-A7A44BEFE36B", "jam": "72F32A83-A697-446B-9016-64AA3F9476E4",
         "mk3": "9055C36B-0A41-48AD-8675-4D3F133E53AC", "mikromk3": "E87516A3-5401-44B1-9952-B4F821ED3DD5"}


def library_available() -> bool:
    lib = os.environ.get("DBM_LIBRARY") or os.path.join(REPO, "target", "debug", "libdrivenbymoss.so")
    return os.path.exists(lib)


@unittest.skipUnless(library_available(), "the core library is not built (cargo build)")
class MaschineCoreTest(unittest.TestCase):
    def run_core(self, model: str, send_disabled: bool = False):
        sys.path.insert(0, os.path.join(REPO, "bindings", "python"))
        from dbm_host import Bridge, SimDaw
        daw = SimDaw().build(tracks=[("Synth", "INSTRUMENT"), ("Drums", "INSTRUMENT"), ("Bass", "AUDIO")],
                             effect_tracks=["FX 1"], scenes=8)
        daw.tracks[0].select()
        box = []
        device = MaschineDevice(model, send=lambda data, port: box[0].receive(data), send_disabled=send_disabled)
        with tempfile.TemporaryDirectory() as config:
            bridge = Bridge(daw, UUIDS[model], config, "Bitwig Studio 5.3")
            box.append(bridge)
            bridge.on_midi_out = lambda data: device.receive(bytes(data))
            bridge.start()
            device.attach()
            bridge.wait_for_startup()
            bridge.advance(300)
            self.assertEqual(device.unknown, [], [m.hex(" ") for m in device.unknown[:5]])
            running = {"lit": sum(1 for key in device.leds if device.led_rgb(key) != (0, 0, 0)),
                       "display": device.display_lines(0), "logs": list(bridge.logs)}
            for c in device.controls:
                if c.kind == "pad":
                    device.press_pad(c.key, 100)
                    device.pad_pressure(c.key, 50)
                    device.release_pad(c.key)
                elif c.kind == "button":
                    device.press_button(c.key)
                    bridge.advance(30)
                    device.release_button(c.key)
                elif c.kind == "encoder":
                    device.touch_encoder(c.key, True)
                    device.turn_encoder(c.key, 2)
                    device.touch_encoder(c.key, False)
                elif c.kind == "strip":
                    device.move_strip(c.index, 90)
                    device.touch_strip(c.index, False)
                bridge.advance(30)
            self.assertEqual(bridge.core_errors(), [])
            self.assertEqual(device.unknown, [], [m.hex(" ") for m in device.unknown[:5]])
            bridge.close()
        return device, running

    def test_mk3(self):
        device, running = self.run_core("mk3")
        self.assertGreater(running["lit"], 10)
        # The MCU display shows the tracks of the volume mode
        self.assertIn("Synth", running["display"][1])

    def test_mk3_with_the_strip_touch(self):
        self.run_core("mk3", send_disabled=True)

    def test_mikromk3(self):
        device, running = self.run_core("mikromk3")
        self.assertGreater(running["lit"], 10)

    def test_mk2(self):
        device, running = self.run_core("mk2")
        self.assertIn("dbm-nihost sends the Shift button, STEP LEFT is Stop", running["logs"])

    def test_jam(self):
        self.run_core("jam")


if __name__ == "__main__":
    unittest.main()
