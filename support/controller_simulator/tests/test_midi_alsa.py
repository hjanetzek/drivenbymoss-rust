# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""ALSA loopback of the APC40 mkII and Launchpad Pro Mk3 ports with a test client; the names get
the suffix " (test)". Skipped without an ALSA sequencer."""

import os
import sys
import time
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

from midi_simulator import DEVICES  # noqa: E402
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
class MidiAlsaTest(unittest.TestCase):
    def open(self, model):
        cls = DEVICES[model]
        names = {key: name + SUFFIX for key, name in cls.PORTS.items()}
        self.seq = alsa_seq.AlsaSeq(cls.CLIENT + SUFFIX, list(names.values()))
        ids = {key: self.seq.ports[name] for key, name in names.items()}
        by_id = {v: k for k, v in ids.items()}
        self.device = cls(send=lambda d, p: self.seq.send(d, ids[p]))
        self.received = []
        self.seq.start_reader(lambda d, port: self.received.append((d, by_id[port])))
        self.host = alsa_seq.AlsaSeq("dbm-midi-test-host", ["io"])
        self.host_in = []
        target = self.host.find_port(names[cls.SEND_PORT])
        self.assertEqual(target[0], self.seq.client_id)
        self.host.connect_to(0, *target)
        self.host.connect_from(0, *target)
        self.host.start_reader(lambda d, port: self.host_in.append(d))
        self.addCleanup(self.seq.close)
        self.addCleanup(self.host.close)

    def pump(self, count):
        self.assertTrue(wait_for(lambda: len(self.received) >= count))
        for data, port in self.received:
            self.device.receive(data, port)
        self.received.clear()

    def test_apc40mk2(self):
        self.open("apc40mk2")
        self.host.send(bytes.fromhex("F0 47 7F 29 60 00 04 41 08 02 01 F7"), 0)
        self.host.send(b"\x90\x00\x15", 0)
        self.host.send(b"\xB0\x38\x03", 0)
        self.host.send(bytes.fromhex("F0 7E 7F 06 01 F7"), 0)
        self.pump(4)
        self.assertEqual(self.device.mode, "Ableton Live mode")
        self.assertEqual(self.device.leds[self.device.by_message[("note", 0, 0)]].value, 0x15)
        self.device.press_button(self.device.by_message[("note", 7, 0x30)])
        self.assertTrue(wait_for(lambda: len(self.host_in) == 2))
        self.assertEqual(self.host_in[0][:6], bytes.fromhex("F0 7E 00 06 02 47"))
        self.assertEqual(self.host_in[1], b"\x97\x30\x7F")

    def test_lppromk3(self):
        self.open("lppromk3")
        self.host.send(bytes.fromhex("F0 00 20 29 02 0E 0E 01 F7"), 0)
        self.host.send(bytes.fromhex("F0 00 20 29 02 0E 03 03 0B 7F 00 00 F7"), 0)
        self.pump(2)
        self.assertTrue(self.device.programmer)
        self.assertEqual(self.device.led_rgb(self.device.by_message[("note", 0, 11)]), (255, 0, 0))
        self.device.press_pad(self.device.by_message[("note", 0, 55)], 64)
        self.assertTrue(wait_for(lambda: len(self.host_in) == 1))
        self.assertEqual(self.host_in, [b"\x90\x37\x40"])


if __name__ == "__main__":
    unittest.main()
