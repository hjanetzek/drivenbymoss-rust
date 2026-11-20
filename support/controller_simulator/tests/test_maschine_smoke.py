# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Smoke runs: each Maschine simulator offscreen (names with the suffix " (smoke)"), fed by
tools/maschine_feed.py, grabbed to a PNG (in the directory MASCHINE_SIM_SCREENSHOTS if set, else a
temporary one) and checked for the fed colours. Also that run.sh hands Push models to the Push
simulator. Skipped without PySide6 or ALSA."""

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


@unittest.skipUnless(HAVE_QT and alsa_seq.available(), "needs PySide6 and an ALSA sequencer")
class MaschineSmokeTest(unittest.TestCase):
    def run_model(self, model: str, expected: list[tuple[int, int, int]]):
        directory = os.environ.get("MASCHINE_SIM_SCREENSHOTS")
        if not directory:
            directory = tempfile.mkdtemp()
            self.addCleanup(shutil.rmtree, directory, True)
        shot = os.path.join(directory, f"{model}.png")
        env = dict(os.environ, QT_QPA_PLATFORM="offscreen")
        app = subprocess.Popen([os.path.join(ROOT, "run.sh"), "--model", model, "--suffix", SUFFIX, "--monitor",
                                "--quit-after", "4", "--screenshot", shot, "--size", "1320x900"],
                               env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        try:
            time.sleep(1.5)
            feed = subprocess.run([sys.executable, os.path.join(ROOT, "tools", "maschine_feed.py"), "--model", model,
                                   "--suffix", SUFFIX], capture_output=True, text=True, timeout=30)
            self.assertEqual(feed.returncode, 0, feed.stderr)
            self.assertIn("hello: n/a" if model in ("mk3", "mikromk3") else "hello: yes", feed.stdout)
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
        if os.environ.get("MASCHINE_SIM_SCREENSHOTS"):
            print(f"\nscreenshot: {shot}")

    def test_mk2(self):
        # Group A: palette 6 (red), group F: 46 (blue), as LedButton blends them on its cap
        self.run_model("mk2", [(255, 29, 32), (28, 29, 255)])

    def test_jam(self):
        # Scene 1: palette 30 (green), the pads' palette 6 (red)
        self.run_model("jam", [(0, 255, 0), (255, 0, 0)])

    def test_mk3(self):
        # Group A: palette 6 (red, on the cap), the full green pad (palette 30)
        self.run_model("mk3", [(255, 29, 32), (0, 255, 0)])

    def test_mikromk3(self):
        # The pads in full palette colours (6 red, 30 green)
        self.run_model("mikromk3", [(255, 0, 0), (0, 255, 0)])

    def test_run_sh_dispatches_push(self):
        result = subprocess.run([os.path.join(ROOT, "run.sh"), "--model", "push3", "--help"], capture_output=True,
                                text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("push_simulator", result.stdout)


if __name__ == "__main__":
    unittest.main()
