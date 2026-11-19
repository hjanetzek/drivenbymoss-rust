# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""ALSA loopback: the simulator's ports and a test client (skipped without an ALSA sequencer)."""

import os
import sys
import time
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

from push_simulator import alsa_seq, layout  # noqa: E402
from push_simulator.device import PushDevice  # noqa: E402


def wait_for(predicate, timeout=2.0):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if predicate():
            return True
        time.sleep(0.01)
    return predicate()


@unittest.skipUnless(alsa_seq.available(), "no ALSA sequencer")
class AlsaLoopbackTest(unittest.TestCase):
    def setUp(self):
        client, live, user = layout.midi_names("push2")
        # A test-only client name, so a running simulator is not confused with this one
        self.sim_seq = alsa_seq.AlsaSeq(client + " (test)", [live + " test", user + " test"])
        ports = {self.sim_seq.ports[live + " test"]: "live", self.sim_seq.ports[user + " test"]: "user"}
        ids = {v: k for k, v in ports.items()}
        self.device = PushDevice("push2", send=lambda d, p: self.sim_seq.send(d, ids[p]))
        self.sim_seq.start_reader(lambda d, port: self.received.append((d, ports[port])))
        self.received = []
        self.host = alsa_seq.AlsaSeq("dbm-push-test-host", ["io"])
        self.host_in = []
        found = self.host.find_port(live + " test")
        self.assertIsNotNone(found)
        self.assertEqual(found[0], self.sim_seq.client_id)
        self.host.connect_to(0, *found)
        self.host.connect_from(0, *found)
        self.host.start_reader(lambda d, port: self.host_in.append(d))

    def tearDown(self):
        self.host.close()
        self.sim_seq.close()

    def pump(self):
        pending, self.received = self.received, []
        for data, port in pending:
            self.device.receive(data, port)

    def test_led_note_and_cc(self):
        self.host.send(b"\x90\x24\x05", 0)
        self.host.send(b"\xB0\x55\x15", 0)
        self.assertTrue(wait_for(lambda: len(self.received) == 2))
        self.pump()
        self.assertEqual(self.device.pads[36].color, 5)
        self.assertEqual(self.device.button_leds[85].color, 0x15)

    def test_sysex_roundtrip(self):
        self.host.send(b"\xF0\x7E\x7F\x06\x01\xF7", 0)
        self.host.send(b"\xF0\x00\x21\x1D\x01\x01\x04\x05\xF7", 0)
        self.assertTrue(wait_for(lambda: len(self.received) == 2))
        self.pump()
        self.assertTrue(wait_for(lambda: len(self.host_in) == 2))
        self.assertEqual(self.host_in[0][:8], bytes.fromhex("F0 7E 01 06 02 00 21 1D"))
        self.assertEqual(len(self.host_in[1]), 17)

    def test_controls_reach_the_host(self):
        self.device.press_button(85)
        self.device.press_pad(36, 100)
        self.device.turn_encoder(71, -1)
        self.assertTrue(wait_for(lambda: len(self.host_in) == 3))
        self.assertEqual(self.host_in, [b"\xB0\x55\x7F", b"\x90\x24\x64", b"\xB0\x47\x7F"])


if __name__ == "__main__":
    unittest.main()
