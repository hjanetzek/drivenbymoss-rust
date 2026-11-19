# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The side channel on the display socket (DBMV messages): the format shared with the Rust core,
the parser with frames and messages interleaved, garbage and split reads, and the older parser
which only knows frames."""

import os
import random
import socket
import sys
import tempfile
import threading
import time
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))

from push_simulator import display, sim_values  # noqa: E402
from push_simulator.sim_values import EncoderState  # noqa: E402

FIXTURE = os.path.join(HERE, "data", "dbmv_encoders.hex")


def fixture_bytes() -> bytes:
    with open(FIXTURE) as file:
        return bytes.fromhex("".join(file.read().split()))


def fixture_states() -> list[EncoderState]:
    """The states of the fixture, as in the Rust test (sim_values.rs, fixture_states)."""
    return [
        EncoderState(0, True, 512, 1023, True, False, "Cutoff", "50.0 %"),
        EncoderState(1, True, 255, 1023, False, True, "Pan", ""),
        EncoderState(2, name="Größe"),
        EncoderState(0),
        EncoderState(sim_values.MASTER, True, 1023, 1023, False, False, "Master", "0.0 dB"),
        EncoderState(sim_values.TEMPO, name="Tempo", text="120.00"),
    ]


def old_feed(buffer: bytearray, chunk: bytes, state: dict) -> list[bytes]:
    """The parser of the simulator before the side channel (it only scans for the frame header)."""
    buffer += chunk
    frames = []
    while True:
        if not state["in_frame"]:
            i = buffer.find(display.HEADER)
            if i < 0:
                keep = len(display.HEADER) - 1
                if len(buffer) > keep:
                    del buffer[:-keep]
                break
            del buffer[:i + len(display.HEADER)]
            state["in_frame"] = True
        if len(buffer) < display.FRAME_BYTES:
            break
        frames.append(display.xor_mask(bytes(buffer[:display.FRAME_BYTES])))
        del buffer[:display.FRAME_BYTES]
        state["in_frame"] = False
    return frames


class FormatTest(unittest.TestCase):
    def test_fixture_round_trip(self):
        data = fixture_bytes()
        decoded = sim_values.decode_message(data)
        self.assertEqual(decoded, (fixture_states(), len(data)))
        self.assertEqual(sim_values.encode_encoders(fixture_states()), data)
        self.assertEqual(data[:6], b"DBMV\x01\x01")

    def test_no_0xff_and_cut_texts(self):
        states = [EncoderState(i, True, 16383, 16383, True, True, "x" * 300 + "é", "ÿ￿") for i in range(11)]
        data = sim_values.encode_encoders(states)
        self.assertNotIn(0xFF, data)
        self.assertNotIn(display.HEADER, data)
        decoded, size = sim_values.decode_message(data)
        self.assertEqual(size, len(data))
        self.assertEqual(len(decoded[0].name.encode()), 127)
        self.assertEqual(decoded[0].text, "ÿ￿")

    def test_invalid_and_unknown(self):
        data = fixture_bytes()
        for end in range(len(data)):
            self.assertIsNone(sim_values.decode_message(data[:end]))
        message, size = sim_values.decode_message(sim_values.encode_message(9, b"\x01\x02"))
        self.assertEqual((message, size), (sim_values.UnknownMessage(1, 9), 11))
        self.assertIsNone(sim_values.decode_message(sim_values.encode_message(9, b"\xFF")))
        self.assertIsNone(sim_values.decode_message(data[:6] + b"\x80" + data[7:]))
        self.assertAlmostEqual(fixture_states()[0].normalized, 512 / 1023)
        self.assertEqual(fixture_states()[2].normalized, 0.0)


class ParserTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.frame = display.encode_frame(lambda x, y: (248, 0, 0) if x < 480 else (0, 0, 248))

    def stream(self):
        message = fixture_bytes()
        other = sim_values.encode_encoders([EncoderState(3, True, 7, 1023, name="Bass")])
        unknown = sim_values.encode_message(42, b"later")
        garbage = b"DBMV\x01\x01\x7f\x7f\x00 not a message " + b"DBM"
        return (message + self.frame + unknown + other + garbage + self.frame + message), [
            ("encoders", fixture_states()), ("frame", None), ("encoders", [EncoderState(3, True, 7, 1023, name="Bass")]), ("frame", None), ("encoders", fixture_states())]

    def kinds(self, items):
        return [(kind, item if kind == "encoders" else None) for kind, item in items]

    def test_interleaved_in_one_chunk(self):
        data, expected = self.stream()
        a = display.FrameAssembler()
        items = a.feed_items(data)
        self.assertEqual(self.kinds(items), expected)
        self.assertEqual(display.frame_pixel(items[1][1], 900, 10), (0, 0, 255))
        # The unknown message is skipped by its length, the invalid one as garbage
        self.assertEqual(a.messages, 4)
        self.assertGreater(a.skipped_bytes, 0)

    def test_split_reads(self):
        data, expected = self.stream()
        rng = random.Random(1)
        for _ in range(5):
            a = display.FrameAssembler()
            items, pos = [], 0
            while pos < len(data):
                size = rng.choice([1, 2, 5, 9, 17, 4000, 70000])
                items += a.feed_items(data[pos:pos + size])
                pos += size
            self.assertEqual(self.kinds(items), expected)

    def test_garbage_between(self):
        message = fixture_bytes()
        noise = bytes(rng % 256 for rng in random.Random(2).sample(range(100000), 3000))
        # A false magic in the garbage with a long length, then a frame: the frame's 0xFF ends it
        data = noise + b"DBMV\x01\x01\x00\x00\x01" + self.frame + noise + message + self.frame
        items = display.FrameAssembler().feed_items(data)
        self.assertEqual([kind for kind, _ in items], ["frame", "encoders", "frame"])
        # feed() returns only the frames
        self.assertEqual(len(display.FrameAssembler().feed(data)), 2)

    def test_older_parser_skips_the_messages(self):
        data, _ = self.stream()
        buffer, state = bytearray(), {"in_frame": False}
        frames = []
        for pos in range(0, len(data), 3333):
            frames += old_feed(buffer, data[pos:pos + 3333], state)
        self.assertEqual(len(frames), 2)
        self.assertEqual(display.frame_pixel(frames[1], 0, 0), (255, 0, 0))

    def test_server_reports_encoders_and_clears_them(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get("XDG_RUNTIME_DIR")) as tmp:
            path = os.path.join(tmp, "d.sock")
            events = []
            done = threading.Event()

            def on_encoders(states):
                events.append(states)
                if len(events) == 3:
                    done.set()

            server = display.DisplayServer(path, lambda f: events.append("frame"), on_encoders=on_encoders)
            server.start()
            try:
                s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                s.connect(path)
                s.sendall(fixture_bytes())
                time.sleep(0.2)
                s.close()
                self.assertTrue(done.wait(3))
            finally:
                server.stop()
            # Cleared on connect, the state, cleared on disconnect
            self.assertEqual(events, [None, fixture_states(), None])


if __name__ == "__main__":
    unittest.main()
