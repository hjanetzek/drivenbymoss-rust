# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Smoke runs: the APC40 mkII, Launchpad Pro Mk3 and Launch Control XL simulators offscreen (names with the suffix
" (smoke)"), fed by tools/midi_feed.py, grabbed to PNGs (into MIDI_SIM_SCREENSHOTS if set, else a
temporary directory) and checked for the fed colours; started through the Push simulator's
run.sh, which hands the models over. Skipped without PySide6 or ALSA."""

import os
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.join(HERE, "..")
sys.path.insert(0, ROOT)

from sim_common import alsa_seq  # noqa: E402

try:
    import PySide6  # noqa: F401
    HAVE_QT = True
except ImportError:
    HAVE_QT = False

SUFFIX = " (smoke)"
PUSH_RUN = os.path.join(ROOT, "..", "push_simulator", "run.sh")


@unittest.skipUnless(HAVE_QT and alsa_seq.available(), "needs PySide6 and an ALSA sequencer")
class MidiSmokeTest(unittest.TestCase):
    def run_model(self, model: str, expected: list[tuple[int, int, int]], ring: bool = False):
        directory = os.environ.get("MIDI_SIM_SCREENSHOTS")
        if not directory:
            directory = tempfile.mkdtemp()
            self.addCleanup(shutil.rmtree, directory, True)
        shot = os.path.join(directory, f"{model}.png")
        env = dict(os.environ, QT_QPA_PLATFORM="offscreen")
        app = subprocess.Popen([PUSH_RUN, "--model", model, "--suffix", SUFFIX, "--quit-after", "4", "--screenshot", shot,
                                "--size", "1320x900"], env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        try:
            time.sleep(1.5)
            feed = subprocess.run([sys.executable, os.path.join(ROOT, "tools", "midi_feed.py"), "--model", model,
                                   "--suffix", SUFFIX], capture_output=True, text=True, timeout=30)
            self.assertEqual(feed.returncode, 0, feed.stderr)
            self.assertIn("identity reply: f0 7e 00 06 02", feed.stdout)
            output, _ = app.communicate(timeout=30)
        finally:
            if app.poll() is None:
                app.kill()
        self.assertEqual(app.returncode, 0, output)
        from PySide6.QtGui import QImage
        image = QImage(shot)
        self.assertFalse(image.isNull())
        colors = {(c.red(), c.green(), c.blue())
                  for c in (image.pixelColor(x, y) for x in range(0, image.width(), 3) for y in range(0, image.height(), 3))}
        for rgb in expected:
            self.assertTrue(any(all(abs(a - b) <= 6 for a, b in zip(rgb, c)) for c in colors), f"{model}: no {rgb}")
        if ring:
            # The LED rings of the encoders (thin arcs; every pixel of the top part)
            top = {(c.red(), c.green(), c.blue()) for c in (image.pixelColor(x, y) for x in range(image.width())
                                                            for y in range(image.height() // 6))}
            self.assertTrue(any(r < 110 and 170 < g < 225 and b > 225 for r, g, b in top), f"{model}: no LED ring")
        if os.environ.get("MIDI_SIM_SCREENSHOTS"):
            print(f"\nscreenshot: {shot}")

    def test_apc40mk2(self):
        # Pads in palette colours (5: red, 13: yellow) and the LED rings of the shared Encoder
        self.run_model("apc40mk2", [(255, 0, 0), (255, 255, 0)], ring=True)

    def test_lppromk3(self):
        # Palette colours of the pads and the RGB pad (magenta from the SysEx LED update)
        self.run_model("lppromk3", [(255, 0, 0), (255, 0, 255)])

    def test_lcxl(self):
        # The knob LEDs (full red and full green) and the selected template (white), as LedButton
        # blends them into its cap
        self.run_model("lcxl", [(255, 29, 32), (28, 255, 32), (233, 233, 233)])


if __name__ == "__main__":
    unittest.main()
