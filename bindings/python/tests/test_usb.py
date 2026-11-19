# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Tests of the USB transports (drivenbymoss/usb.py) and the USB callback of the binding: the
socket of the Push simulator with a test listener (the byte stream, reconnecting, dropping),
the usbdevfs device lookup, and the reference host's frames.

Run from the repository root (after 'cargo build')::

    python3 -m unittest discover -s bindings/python/tests
"""

import os
import socket
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import drivenbymoss  # noqa: E402
from drivenbymoss import usb  # noqa: E402

LAUNCH_CONTROL_XL = "5AFA5045-FF92-4737-A091-5B371CD5E529"
FRAME_BYTES = 16 + 327680


def frame(fill: int) -> bytes:
    return usb.PUSH_FRAME_HEADER + bytes([fill]) * (FRAME_BYTES - 16)


class Listener:
    """A UNIX stream socket as the Push simulator listens: reads frames of FRAME_BYTES."""

    def __init__(self, path: str):
        self.path = path
        self.server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.server.bind(path)
        self.server.listen(4)
        self.server.settimeout(5)

    def accept(self) -> socket.socket:
        connection, _ = self.server.accept()
        connection.settimeout(5)
        return connection

    @staticmethod
    def read_frame(connection: socket.socket) -> bytes:
        data = bytearray()
        while len(data) < FRAME_BYTES:
            chunk = connection.recv(FRAME_BYTES - len(data))
            if not chunk:
                break
            data += chunk
        return bytes(data)

    def close(self):
        self.server.close()


class FakeClock:
    def __init__(self):
        self.now = 0.0

    def __call__(self):
        return self.now


class SplitFrameTests(unittest.TestCase):
    def test_push_frame_header_is_its_own_transfer(self):
        parts = usb.split_frame(frame(1))
        self.assertEqual(parts[0], usb.PUSH_FRAME_HEADER)
        self.assertEqual(len(parts), 1 + 327680 // usb.USB_CHUNK_BYTES)
        self.assertEqual(b"".join(parts), frame(1))

    def test_other_data_is_only_chunked(self):
        data = bytes(range(256)) * 100
        parts = usb.split_frame(data)
        self.assertEqual(len(parts[0]), usb.USB_CHUNK_BYTES)
        self.assertEqual(b"".join(parts), data)


class SocketTransportTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="dbm-usb-test-")
        self.path = os.path.join(self.temp.name, "display.sock")

    def tearDown(self):
        self.temp.cleanup()

    def test_frames_go_back_to_back_as_their_exact_bytes(self):
        listener = Listener(self.path)
        output = usb.UsbOutput.socket(self.path)
        try:
            self.assertTrue(output.write(0x2982, 0x1967, 0, 0x01, frame(1)))
            connection = listener.accept()
            self.assertEqual(Listener.read_frame(connection), frame(1))
            self.assertTrue(output.flush())
            self.assertTrue(output.write(0x2982, 0x1967, 0, 0x01, frame(2)))
            self.assertEqual(Listener.read_frame(connection), frame(2))
            self.assertTrue(output.flush())
            self.assertEqual(output.written, 2)
            connection.close()
        finally:
            output.close()
            listener.close()

    def test_no_listener_drops_without_blocking(self):
        clock = FakeClock()
        output = usb.UsbOutput.socket(self.path, clock)
        try:
            for i in range(5):
                self.assertTrue(output.write(0x2982, 0x1967, 0, 0x01, frame(i)))
            self.assertTrue(output.flush())
            self.assertEqual(output.written, 0)
            # The later frames came before the retry time: not even a connect
            self.assertGreaterEqual(output.failed + output.dropped, 5)
        finally:
            output.close()

    def test_reconnects_after_the_listener_dropped_the_connection(self):
        clock = FakeClock()
        listener = Listener(self.path)
        output = usb.UsbOutput.socket(self.path, clock)
        try:
            output.write(0x2982, 0x1967, 0, 0x01, frame(1))
            connection = listener.accept()
            self.assertEqual(Listener.read_frame(connection), frame(1))
            self.assertTrue(output.flush())
            connection.close()
            # Writes into the closed connection fail (the first may still fit into the buffer)
            for i in range(3):
                output.write(0x2982, 0x1967, 0, 0x01, frame(2))
                self.assertTrue(output.flush())
            self.assertGreaterEqual(output.failed, 1)
            clock.now += usb.RETRY_SECONDS + 0.1
            output.write(0x2982, 0x1967, 0, 0x01, frame(3))
            connection = listener.accept()
            self.assertEqual(Listener.read_frame(connection), frame(3), "a new connection starts with a complete frame")
            connection.close()
        finally:
            output.close()
            listener.close()

    def test_a_newer_frame_replaces_one_not_written_yet(self):
        written = []
        gate = threading.Event()

        class SlowSink:
            def write(self, endpoint, data):
                gate.wait(5)
                written.append(data[16])
                return True

            def close(self):
                pass

        output = usb.UsbOutput(lambda v, p, i: SlowSink())
        try:
            output.write(1, 2, 0, 1, frame(1))
            # The writer thread takes frame 1 and waits; 2 is replaced by 3
            for _ in range(100):
                if not output.slots:
                    break
                threading.Event().wait(0.01)
            output.write(1, 2, 0, 1, frame(2))
            output.write(1, 2, 0, 1, frame(3))
            gate.set()
            self.assertTrue(output.flush())
            self.assertEqual(written, [1, 3])
            self.assertEqual(output.dropped, 1)
        finally:
            output.close()


class EnvironmentTests(unittest.TestCase):
    def test_transport_of_the_environment(self):
        with mock.patch.dict(os.environ, {"DBM_PUSH_DISPLAY_SOCKET": "/tmp/x.sock", "DBM_USB_TRANSPORT": ""}):
            self.assertEqual(usb.default_socket_path(), "/tmp/x.sock")
            output = usb.UsbOutput.from_environment()
            self.assertIsInstance(output.factory(1, 2, 0), object)
            self.assertTrue(output.enabled)
        with mock.patch.dict(os.environ, {"DBM_USB_TRANSPORT": "none"}):
            self.assertFalse(usb.UsbOutput.from_environment().write(1, 2, 0, 1, b"x"))
        env = {k: v for k, v in os.environ.items() if k != "DBM_PUSH_DISPLAY_SOCKET"}
        env["XDG_RUNTIME_DIR"] = "/run/user/1000"
        with mock.patch.dict(os.environ, env, clear=True):
            self.assertEqual(usb.default_socket_path(), "/run/user/1000/dbm-push-display.sock")


class UsbdevfsTests(unittest.TestCase):
    def test_device_lookup_in_sysfs(self):
        with tempfile.TemporaryDirectory() as root:
            for name, vendor, product, bus, number in [("1-1", "17cc", "1140", 1, 4), ("3-2", "2982", "1967", 3, 12)]:
                device = Path(root, name)
                device.mkdir()
                for key, value in [("idVendor", vendor), ("idProduct", product), ("busnum", str(bus)), ("devnum", str(number))]:
                    (device / key).write_text(value + "\n")
            # An interface directory has no IDs
            Path(root, "3-2:1.0").mkdir()
            self.assertEqual(usb.find_usb_device(0x2982, 0x1967, root), "/dev/bus/usb/003/012")
            self.assertIsNone(usb.find_usb_device(0x2982, 0x1969, root))

    def test_missing_device_is_retried_later(self):
        clock = FakeClock()
        lookups = []
        sink = usb.UsbdevfsSink(0x2982, 0x1967, 0, clock, finder=lambda v, p: lookups.append((v, p)))
        self.assertFalse(sink.write(0x01, frame(1)))
        self.assertFalse(sink.write(0x01, frame(1)))
        self.assertEqual(len(lookups), 1)
        clock.now += usb.RETRY_SECONDS
        self.assertFalse(sink.write(0x01, frame(1)))
        self.assertEqual(len(lookups), 2)
        sink.close()

    def test_ioctl_numbers(self):
        # As in linux/usbdevice_fs.h (64 bit: the transfer struct has 24 bytes)
        self.assertEqual(usb.USBDEVFS_CLAIMINTERFACE, 0x8004550F)
        self.assertEqual(usb.USBDEVFS_RELEASEINTERFACE, 0x80045510)
        if ctypes_pointer_bytes() == 8:
            self.assertEqual(usb.USBDEVFS_BULK, 0xC0185502)
            self.assertEqual(usb.USBDEVFS_IOCTL, 0xC0105512)
        else:
            self.assertEqual(usb.USBDEVFS_BULK, 0xC0105502)


def ctypes_pointer_bytes() -> int:
    import ctypes
    return ctypes.sizeof(ctypes.c_void_p)


class BindingTests(unittest.TestCase):
    def test_use_usb_and_device_list(self):
        self.assertIsInstance(drivenbymoss.list_usb_devices(), dict)
        self.assertEqual(drivenbymoss.list_usb_devices("no-such-controller"), {})
        with tempfile.TemporaryDirectory() as config:
            app = drivenbymoss.App.create_controller(drivenbymoss.Backend(), LAUNCH_CONTROL_XL, config)
            try:
                self.assertTrue(app.use_usb())
            finally:
                app.close()

    def test_callback_reaches_the_backend_and_its_default_output(self):
        import ctypes

        class Output:
            def __init__(self):
                self.writes = []
                self.closed = False

            def write(self, *args):
                self.writes.append(args)
                return True

            def close(self):
                self.closed = True

        backend = drivenbymoss.Backend()
        backend.usb_output = output = Output()
        trampolines = drivenbymoss._Trampolines(backend)
        data = (ctypes.c_uint8 * 4)(0xFF, 0xCC, 0xAA, 0x88)
        self.assertEqual(trampolines.usb(None, 0, 0, 0x2982, 0x1967, 0, 0x01, data, 4), 1)
        self.assertEqual(backend.usb_device, drivenbymoss.UsbDevice(0, 0, 0x2982, 0x1967, 0, 0x01))
        self.assertEqual(output.writes, [(0x2982, 0x1967, 0, 0x01, bytes([0xFF, 0xCC, 0xAA, 0x88]))])
        backend.close_usb()
        self.assertTrue(output.closed)

        class Override(drivenbymoss.Backend):
            def __init__(self):
                self.calls = []

            def usb_bulk_out(self, device_index, endpoint, data):
                self.calls.append((device_index, endpoint, data))
                return False

        override = Override()
        trampolines = drivenbymoss._Trampolines(override)
        self.assertEqual(trampolines.usb(None, 0, 1, 0x2982, 0x1969, 0, 0x01, data, 4), 0)
        self.assertEqual(override.calls, [(1, 0x01, bytes([0xFF, 0xCC, 0xAA, 0x88]))])

    def test_reference_host_sends_the_frames_to_the_socket(self):
        """A controller with USB devices (the Push 2/3) on the reference host: its display frames
        reach the simulator's socket. Skipped while no controller has USB devices."""
        devices = drivenbymoss.list_usb_devices()
        if not devices:
            self.skipTest("no controller with USB devices")
        from dbm_host import Bridge, SimDaw

        controller_id = next(iter(devices))
        with tempfile.TemporaryDirectory(prefix="dbm-usb-host-") as temp:
            path = os.path.join(temp, "display.sock")
            listener = Listener(path)
            daw = SimDaw().build(tracks=[("Synth", "INSTRUMENT")], scenes=8)
            with mock.patch.dict(os.environ, {"DBM_PUSH_DISPLAY_SOCKET": path, "DBM_USB_TRANSPORT": ""}):
                bridge = Bridge(daw, controller_id, temp).start()
                try:
                    bridge.wait_for_startup()
                    bridge.advance(1500)
                    self.assertGreater(bridge.usb_frames, 0)
                    connection = listener.accept()
                    data = Listener.read_frame(connection)
                    self.assertEqual(len(data), len(bridge.last_usb_frame))
                    self.assertEqual(data[:16], usb.PUSH_FRAME_HEADER)
                    connection.close()
                finally:
                    bridge.close()
                    listener.close()


if __name__ == "__main__":
    unittest.main()
