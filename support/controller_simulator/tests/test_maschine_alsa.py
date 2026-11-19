# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""ALSA loopback of the Maschine ports (read-only "In", write-only "Out", as dbm-nihost) with a test
client; the names get the suffix " (test)", so a running dbm-nihost or simulator is not touched.
Skipped without an ALSA sequencer."""

import os
import sys
import time
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

from maschine_simulator import layout, ni  # noqa: E402
from maschine_simulator.device import IN_PORT, OUT_PORT, MaschineDevice  # noqa: E402
from sim_common import alsa_seq  # noqa: E402

SUFFIX = " (test)"


def wait_for(predicate, timeout=2.0):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if predicate():
            return True
        time.sleep(0.01)
    return predicate()


@unittest.skipUnless(alsa_seq.available(), "no ALSA sequencer")
class MaschineAlsaTest(unittest.TestCase):
    def open(self, model):
        client, daw_in, daw_out = layout.midi_names(model, SUFFIX)
        self.seq = alsa_seq.AlsaSeq(client, [(daw_in, alsa_seq.READ_CAPS), (daw_out, alsa_seq.WRITE_CAPS)])
        ids = {IN_PORT: self.seq.ports[daw_in], OUT_PORT: self.seq.ports[daw_out]}
        self.device = MaschineDevice(model, send=lambda d, p: self.seq.send(d, ids[p]))
        self.received = []
        self.seq.start_reader(lambda d, port: self.received.append(d))
        self.host = alsa_seq.AlsaSeq("dbm-maschine-test-host", ["io"])
        self.host_in = []
        source, sink = self.host.find_port(daw_in), self.host.find_port(daw_out)
        self.assertEqual((source[0], sink[0]), (self.seq.client_id, self.seq.client_id))
        self.host.connect_from(0, *source)
        self.host.connect_to(0, *sink)
        self.host.start_reader(lambda d, port: self.host_in.append(d))
        self.addCleanup(self.seq.close)
        self.addCleanup(self.host.close)

    def pump(self, count):
        self.assertTrue(wait_for(lambda: len(self.received) >= count))
        for data in self.received:
            self.device.receive(data)
        self.received.clear()

    def test_mk2(self):
        self.open("mk2")
        self.device.attach()
        self.host.send(ni.QUERY, 0)
        self.host.send(b"\x90\x24\x1E", 0)
        self.host.send(ni.MCU_HEADER + bytes([0x14, 0x12, 0]) + b"Synth" + b"\xF7", 0)
        self.pump(3)
        self.assertEqual(self.device.led_rgb(next(c.key for c in self.device.controls if c.template == "Pad1")), ni.PALETTE[30])
        self.assertTrue(self.device.display_lines(0)[1].startswith("Synth"))
        self.device.press_button(next(c.key for c in self.device.controls if c.template == "Play"))
        self.assertTrue(wait_for(lambda: len(self.host_in) == 3))
        self.assertEqual(self.host_in, [ni.MK2_HELLO, ni.MK2_HELLO, b"\xB0\x39\x7F"])
        # The ports have one direction each: nothing the host sends comes back on "In"
        self.assertNotIn(b"\x90\x24\x1E", self.host_in)

    def test_jam(self):
        self.open("jam")
        self.device.attach()
        self.host.send(b"\xB0\x08\x7F", 0)
        self.host.send(ni.JAM_HEADER + bytes([ni.JAM_STRIP_SETUP]) + bytes([0, 30] * 8) + b"\xF7", 0)
        self.pump(2)
        self.assertEqual(self.device.strip_colours(0), [30] * 11)
        self.device.move_strip(3, 10)
        self.assertTrue(wait_for(lambda: len(self.host_in) == 3))
        self.assertEqual(self.host_in, [ni.JAM_HELLO, b"\xB0\x17\x7F", b"\xB0\x0B\x0A"])


if __name__ == "__main__":
    unittest.main()
