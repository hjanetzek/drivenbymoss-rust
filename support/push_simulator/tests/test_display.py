# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The display stream: masking, pixel format, resynchronisation, the socket server (headless)."""

import os
import socket
import sys
import tempfile
import threading
import time
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

from push_simulator import display  # noqa: E402


def pattern(x, y):
    """Distinct colours per region, exact in 565 (multiples of 8/4 with the low bits repeated)."""
    if x < 320:
        return (248, 0, 0)
    if x < 640:
        return (0, 252, 0)
    return (0, 0, 248) if y < 80 else (255, 255, 255)


class DisplayTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.stream = display.encode_frame(pattern)

    def test_sizes(self):
        self.assertEqual(display.FRAME_BYTES, 327680)
        self.assertEqual(display.FRAME_BYTES, 20 * 0x4000)
        self.assertEqual(len(self.stream), 16 + 327680)
        self.assertEqual(self.stream[:16], bytes.fromhex("FFCCAA88") + bytes(12))

    def test_pixel_format_as_dbm(self):
        # PushUsbDisplay.sPixelFromRGB: blue in the high 5 bits, red in the low 5 bits
        self.assertEqual(display.rgb_to_pixel(255, 0, 0), 0x001F)
        self.assertEqual(display.rgb_to_pixel(0, 255, 0), 0x07E0)
        self.assertEqual(display.rgb_to_pixel(0, 0, 255), 0xF800)
        self.assertEqual(display.pixel_to_rgb(0xFFFF), (255, 255, 255))

    def test_masking(self):
        # A black first pixel is the mask itself; the filler bytes are masked too
        self.assertEqual(display.encode_frame(lambda x, y: (0, 0, 0))[16:20], bytes.fromhex("E7F3E7FF"))
        frame = display.xor_mask(self.stream[16:])
        self.assertEqual(display.xor_mask(frame), self.stream[16:])
        self.assertEqual(frame[1920:2048], bytes(128))
        self.assertEqual(display.xor_mask(b"\x00" * 6), bytes.fromhex("E7F3E7FFE7F3"))

    def test_decode(self):
        frames = display.FrameAssembler().feed(self.stream)
        self.assertEqual(len(frames), 1)
        f = frames[0]
        self.assertEqual(display.frame_pixel(f, 0, 0), (255, 0, 0))  # 248 -> 255 by bit repeat
        self.assertEqual(display.frame_pixel(f, 400, 10), (0, 255, 0))
        self.assertEqual(display.frame_pixel(f, 700, 10), (0, 0, 255))
        self.assertEqual(display.frame_pixel(f, 959, 159), (255, 255, 255))
        rgb = display.frame_to_rgb888(f)
        self.assertEqual(len(rgb), 960 * 160 * 3)
        self.assertEqual(rgb[:3], b"\xFF\x00\x00")

    def test_resync_and_partial_reads(self):
        a = display.FrameAssembler()
        garbage = b"\x00\xFF\xCC\xAA" + os.urandom(1000).replace(b"\xFF\xCC", b"\x00\x00")
        # A truncated frame (the header without its data) followed by a complete one
        data = garbage + self.stream + self.stream[:5000]
        frames = []
        for i in range(0, len(data), 7777):
            frames += a.feed(data[i:i + 7777])
        self.assertEqual(len(frames), 1)
        self.assertGreaterEqual(a.skipped_bytes, len(garbage))
        # A reset (new connection) drops the partial frame
        a.reset()
        frames = a.feed(self.stream[:3] + self.stream[3:])
        self.assertEqual(len(frames), 1)

    def test_server_sequential_connections(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get("XDG_RUNTIME_DIR")) as tmp:
            path = os.path.join(tmp, "d.sock")
            frames = []
            done = threading.Event()

            def on_frame(f):
                frames.append(f)
                if len(frames) == 3:
                    done.set()

            server = display.DisplayServer(path, on_frame)
            server.start()
            try:
                for count in (1, 2):
                    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                    s.connect(path)
                    for _ in range(count):
                        s.sendall(self.stream[:16])
                        s.sendall(self.stream[16:])
                    s.sendall(self.stream[:100])  # a partial frame at the end is dropped
                    s.close()
                    time.sleep(0.3)
                self.assertTrue(done.wait(3))
                self.assertEqual(len(frames), 3)
                self.assertEqual(server.connections, 2)
                self.assertEqual(display.frame_pixel(frames[2], 700, 100), (255, 255, 255))
            finally:
                server.stop()
            self.assertFalse(os.path.exists(path))

    def test_default_socket_path(self):
        old = os.environ.get("DBM_PUSH_DISPLAY_SOCKET")
        os.environ["DBM_PUSH_DISPLAY_SOCKET"] = "/tmp/x.sock"
        try:
            self.assertEqual(display.default_socket_path(), "/tmp/x.sock")
        finally:
            if old is None:
                del os.environ["DBM_PUSH_DISPLAY_SOCKET"]
            else:
                os.environ["DBM_PUSH_DISPLAY_SOCKET"] = old

    def test_fps(self):
        c = display.FpsCounter()
        for i in range(11):
            c.tick(10.0 + i * 0.1)
        self.assertAlmostEqual(c.fps(11.0), 10.0, places=3)


class QtImageTest(unittest.TestCase):
    def test_qimage_conversion(self):
        try:
            from PySide6.QtGui import QGuiApplication  # noqa: F401
            from push_simulator.qt_bridge import frame_to_qimage
        except ImportError:
            self.skipTest("PySide6 not available")
        frame = display.FrameAssembler().feed(display.encode_frame(pattern))[0]
        image = frame_to_qimage(frame)
        self.assertEqual((image.width(), image.height()), (960, 160))
        c = image.pixelColor(0, 0)
        self.assertEqual((c.red() > 240, c.green(), c.blue()), (True, 0, 0))
        c = image.pixelColor(700, 10)
        self.assertEqual((c.red(), c.green(), c.blue() > 240), (0, 0, True))


if __name__ == "__main__":
    unittest.main()
