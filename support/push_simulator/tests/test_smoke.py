# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Smoke run: the app offscreen, fed with a frame over the socket and LEDs over ALSA, grabbed to a
PNG (PUSH_SIM_SCREENSHOT, default: a temporary file). Skipped without PySide6 or ALSA."""

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

from push_simulator import alsa_seq  # noqa: E402

try:
    import PySide6  # noqa: F401
    HAVE_QT = True
except ImportError:
    HAVE_QT = False


@unittest.skipUnless(HAVE_QT and alsa_seq.available(), "needs PySide6 and an ALSA sequencer")
class SmokeTest(unittest.TestCase):
    def test_render(self):
        tmp = tempfile.mkdtemp(dir=os.environ.get("XDG_RUNTIME_DIR"))
        self.addCleanup(shutil.rmtree, tmp, True)
        sock = os.path.join(tmp, "display.sock")
        shot = os.environ.get("PUSH_SIM_SCREENSHOT") or os.path.join(tmp, "push2.png")
        env = dict(os.environ, QT_QPA_PLATFORM="offscreen")
        app = subprocess.Popen([sys.executable, "-m", "push_simulator", "--quit-after", "5", "--monitor",
                                "--display-socket", sock, "--screenshot", shot, "--size", "1320x900"],
                               cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        try:
            for _ in range(50):
                if os.path.exists(sock):
                    break
                time.sleep(0.1)
            time.sleep(0.5)
            # With the encoder state (the rings); the connection stays open until the screenshot
            feed = subprocess.run([sys.executable, os.path.join(ROOT, "tools", "demo_feed.py"), "--display-socket", sock,
                                   "--frames", "3", "--values", "--hold", "6"], capture_output=True, text=True, timeout=30)
            self.assertEqual(feed.returncode, 0, feed.stderr)
            self.assertIn("identity reply: f0 7e 01 06 02", feed.stdout)
            output, _ = app.communicate(timeout=30)
        finally:
            if app.poll() is None:
                app.kill()
        self.assertEqual(app.returncode, 0, output)
        from PySide6.QtGui import QImage
        image = QImage(shot)
        self.assertFalse(image.isNull())
        # Something yellow: the yellow bar of the test frame (and the yellow pads)
        colors = {(c.red() > 200, c.green() > 200, c.blue() < 60)
                  for c in (image.pixelColor(x, y) for x in range(0, image.width(), 7) for y in range(0, image.height(), 7))}
        self.assertIn((True, True, True), colors)
        # The LED rings: their cyan above the display (the encoder row)
        top = int(image.height() * 0.09)
        ring = {(c.red() < 120, c.green() > 160, c.blue() > 200)
                for c in (image.pixelColor(x, y) for x in range(0, image.width(), 2) for y in range(0, top, 2))}
        self.assertIn((True, True, True), ring)
        if os.environ.get("PUSH_SIM_SCREENSHOT"):
            print(f"\nscreenshot: {shot}")


if __name__ == "__main__":
    unittest.main()
