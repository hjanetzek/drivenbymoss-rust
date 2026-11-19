# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Tests of the simulator side channel (dbm_set_simulator_callback, Backend.simulator_data_out,
UsbOutput.write_simulator): only the socket transport takes it, the socket replays the last
message after a connect, and the reference host with a Push 2 sends the encoder state (DBMV)
between the frames, decoded with the simulator's parser.

Run from the repository root (after 'cargo build')::

    python3 -m unittest discover -s bindings/python/tests
"""

import ctypes
import os
import socket
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
# The simulator's parser (support/push_simulator, no Qt needed)
sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "support" / "push_simulator"))

import drivenbymoss  # noqa: E402
from drivenbymoss import usb  # noqa: E402
from push_simulator import display, sim_values  # noqa: E402

PUSH2 = "15176AA0-C476-11E6-9598-0800200C9A66"
MESSAGE = sim_values.encode_encoders([sim_values.EncoderState(0, True, 512, 1023, name="Drums")])


class FakeClock:
    def __init__(self):
        self.now = 0.0

    def __call__(self):
        return self.now


def read_some(connection: socket.socket, size: int) -> bytes:
    data = bytearray()
    while len(data) < size:
        chunk = connection.recv(size - len(data))
        if not chunk:
            break
        data += chunk
    return bytes(data)


class TransportTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="dbm-sim-test-")
        self.path = os.path.join(self.temp.name, "display.sock")

    def tearDown(self):
        self.temp.cleanup()

    def listen(self) -> socket.socket:
        server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        server.bind(self.path)
        server.listen(4)
        server.settimeout(5)
        self.addCleanup(server.close)
        return server

    def test_only_the_socket_takes_it(self):
        self.assertFalse(usb.UsbOutput(None).write_simulator(MESSAGE))
        self.assertFalse(usb.UsbOutput(lambda v, p, i: object()).write_simulator(MESSAGE))

    def test_written_and_replayed_after_a_reconnect(self):
        clock = FakeClock()
        server = self.listen()
        output = usb.UsbOutput.socket(self.path, clock)
        try:
            self.assertTrue(output.write_simulator(MESSAGE))
            connection, _ = server.accept()
            connection.settimeout(5)
            self.assertEqual(read_some(connection, len(MESSAGE)), MESSAGE)
            self.assertTrue(output.flush())
            connection.close()
            # The writes into the closed connection fail; the next connect starts with the message
            for _ in range(3):
                output.write(0x2982, 0x1967, 0, 0x01, usb.PUSH_FRAME_HEADER + b"\x00" * 64)
                self.assertTrue(output.flush())
            clock.now += usb.RETRY_SECONDS + 0.1
            frame = usb.PUSH_FRAME_HEADER + b"\x01" * 64
            output.write(0x2982, 0x1967, 0, 0x01, frame)
            connection, _ = server.accept()
            connection.settimeout(5)
            self.assertEqual(read_some(connection, len(MESSAGE) + len(frame)), MESSAGE + frame)
            connection.close()
        finally:
            output.close()

    def test_environment(self):
        cases = [
            ({"DBM_PUSH_DISPLAY_SOCKET": "/tmp/x.sock"}, True),
            ({"DBM_PUSH_DISPLAY_SOCKET": "/tmp/x.sock", "DBM_PUSH_SIM_VALUES": "0"}, False),
            ({"DBM_PUSH_DISPLAY_SOCKET": "/tmp/x.sock", "DBM_USB_TRANSPORT": "usbdevfs"}, False),
            ({"DBM_USB_TRANSPORT": "socket"}, True),
            ({}, False),
        ]
        base = {k: v for k, v in os.environ.items() if k not in ("DBM_PUSH_DISPLAY_SOCKET", "DBM_PUSH_SIM_VALUES", "DBM_USB_TRANSPORT")}
        for env, expected in cases:
            with mock.patch.dict(os.environ, dict(base, **env), clear=True):
                self.assertEqual(usb.simulator_data_enabled(), expected, env)
                self.assertEqual(drivenbymoss.Backend().wants_simulator_data(), expected, env)


class BindingTests(unittest.TestCase):
    def test_callback_reaches_the_default_output(self):
        class Output:
            def __init__(self):
                self.messages = []

            def write_simulator(self, data):
                self.messages.append(data)
                return True

            def close(self):
                pass

        backend = drivenbymoss.Backend()
        backend.usb_output = output = Output()
        trampolines = drivenbymoss._Trampolines(backend)
        data = (ctypes.c_uint8 * len(MESSAGE)).from_buffer_copy(MESSAGE)
        self.assertEqual(trampolines.simulator(None, 0, data, len(MESSAGE)), 1)
        self.assertEqual(output.messages, [MESSAGE])

    def test_reference_host_sends_the_encoder_state_between_the_frames(self):
        if PUSH2 not in {k.upper() for k in drivenbymoss.list_usb_devices()}:
            self.skipTest("the library has no Push 2")
        from dbm_host import Bridge, SimDaw

        with tempfile.TemporaryDirectory(prefix="dbm-sim-host-") as temp:
            path = os.path.join(temp, "display.sock")
            server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            server.bind(path)
            server.listen(4)
            server.settimeout(5)
            daw = SimDaw().build(tracks=[("Synth", "INSTRUMENT"), ("Drums", "INSTRUMENT")], scenes=8)
            env = {"DBM_PUSH_DISPLAY_SOCKET": path, "DBM_USB_TRANSPORT": "", "DBM_PUSH_SIM_VALUES": ""}
            with mock.patch.dict(os.environ, env):
                bridge = Bridge(daw, PUSH2, temp).start()
                try:
                    bridge.wait_for_startup()
                    bridge.advance(1500)
                    connection, _ = server.accept()
                    connection.settimeout(1)
                    assembler = display.FrameAssembler()
                    items = []
                    deadline = time.monotonic() + 5
                    while time.monotonic() < deadline and not ({"frame", "encoders"} <= {kind for kind, _ in items}):
                        bridge.advance(60)
                        try:
                            chunk = connection.recv(1 << 20)
                        except socket.timeout:
                            continue
                        if not chunk:
                            break
                        items += assembler.feed_items(chunk)
                    connection.close()
                finally:
                    bridge.close()
                    server.close()
            kinds = {kind for kind, _ in items}
            self.assertEqual(kinds, {"frame", "encoders"})
            states = next(item for kind, item in items if kind == "encoders")
            self.assertEqual([s.index for s in states[:8]], list(range(8)))
            self.assertIn(sim_values.MASTER, [s.index for s in states])
            self.assertEqual(assembler.skipped_bytes, 0)

    def test_use_simulator_data(self):
        with tempfile.TemporaryDirectory() as config:
            app = drivenbymoss.App.create_controller(drivenbymoss.Backend(), PUSH2, config)
            try:
                self.assertTrue(app.use_simulator_data())
            finally:
                app.close()


if __name__ == "__main__":
    unittest.main()
