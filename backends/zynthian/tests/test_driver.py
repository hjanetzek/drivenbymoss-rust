# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: tests of the driver (tick thread, MIDI, state, sleep)
#
# Copyright (C) 2026 Hannes Janetzek
#
# ******************************************************************************
#
# This program is free software; you can redistribute it and/or
# modify it under the terms of the GNU General Public License as
# published by the Free Software Foundation; either version 2 of
# the License, or any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
# GNU General Public License for more details.
#
# For a full copy of the GNU General Public License see the LICENSE.txt file.
#
# ******************************************************************************

"""The Launch Control XL driver as the ctrldev manager runs it: ``__init__``, ``init`` (the
tick thread starts the core), ``midi_event`` from another thread, ``get_state``/``set_state``,
``sleep_on``/``sleep_off`` and ``end``. Real time: the controller starts after the core's 1 s
startup delay. The MIDI to the device goes to the FakeZyncore of zynstubs."""

import os
import sys
import tempfile
import threading
import time
import unittest

import zynstubs

zynstubs.install()

from zyngine.ctrldev.zynthian_ctrldev_dbm_launchcontrol_xl import zynthian_ctrldev_dbm_launchcontrol_xl  # noqa: E402
from zyngine.ctrldev.zynthian_ctrldev_dbm_maschine_mk2 import zynthian_ctrldev_dbm_maschine_mk2  # noqa: E402

TEMPLATE_SELECTION = bytes.fromhex("F000202902117708F7")
IDEV_IN, IDEV_OUT = 3, 5


def wait_for(condition, timeout=5.0):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if condition():
            return True
        time.sleep(0.02)
    return False


class DriverTest(unittest.TestCase):

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="dbm-zynthian-driver-")
        self.old_config = os.environ.get("ZYNTHIAN_CONFIG_DIR")
        os.environ["ZYNTHIAN_CONFIG_DIR"] = self.temp.name
        self.state = zynstubs.project()
        self.zyncore = zynstubs.lib_zyncore()
        self.zyncore.sent.clear()
        self.driver = zynthian_ctrldev_dbm_launchcontrol_xl(self.state, IDEV_IN, IDEV_OUT)
        self.driver.init()

    def tearDown(self):
        self.driver.end()
        if self.old_config is None:
            os.environ.pop("ZYNTHIAN_CONFIG_DIR", None)
        else:
            os.environ["ZYNTHIAN_CONFIG_DIR"] = self.old_config
        self.temp.cleanup()

    def sent(self):
        return [data for idev, data in list(self.zyncore.sent) if idev == IDEV_OUT]

    def started(self):
        """The device answers the template selection, as the hardware does."""
        if TEMPLATE_SELECTION in self.sent():
            return True
        return False

    def start_device(self):
        self.assertTrue(wait_for(self.started), "The core selects the template after its startup delay")
        self.driver.midi_event(TEMPLATE_SELECTION)
        time.sleep(0.2)

    def test_class(self):
        cls = zynthian_ctrldev_dbm_launchcontrol_xl
        self.assertEqual(cls.dev_ids, ["Launch Control XL IN 1"])
        self.assertTrue(cls.get_autoload_flag())
        self.assertTrue(cls.unroute_from_chains)
        self.assertEqual(cls.get_driver_name(), "DrivenByMoss Launch Control XL")

    def test_midi_from_another_thread(self):
        self.start_device()
        self.assertEqual(self.driver.thread.name, "dbm DrivenByMoss Launch Control XL")
        # The fast thread of the state manager calls midi_event; the driver owns the device
        result = []
        fast = threading.Thread(target=lambda: result.append(self.driver.midi_event(b"\xb8\x4d\x7f")))
        fast.start()
        fast.join()
        self.assertEqual(result, [True])
        self.assertTrue(wait_for(lambda: self.state.zctrl(1, "level").value == 1.0))
        # A word of a SysEx without status byte (§9 Z4) is dropped, but still owned
        self.assertTrue(self.driver.midi_event(b"\x11\xf7\x00"))
        # A mute in zynthian lights the LED of its button (note 0x4A of the second track)
        self.zyncore.sent.clear()
        self.state.zctrl(2, "mute").set_value(1)
        self.assertTrue(wait_for(lambda: any(d[:2] == b"\x98\x4a" for d in self.sent())))

    def test_no_core_calls_while_zynthian_is_busy(self):
        """While a snapshot loads (or zynthian is busy otherwise) the tick thread doesn't call
        into the core: its reports read zynseq, whose lists are being replaced (a crash on the
        Duet). The device's MIDI stays queued and is handled after the load."""
        self.start_device()
        self.state.busy.add("load snapshot")
        time.sleep(0.1)
        self.driver.midi_event(b"\xb8\x4d\x7f")
        time.sleep(0.3)
        self.assertNotEqual(self.state.zctrl(1, "level").value, 1.0, "Not handled while busy")
        self.state.busy.clear()
        self.assertTrue(wait_for(lambda: self.state.zctrl(1, "level").value == 1.0), "Handled after the load")

    def test_state_and_sleep(self):
        self.start_device()
        self.assertTrue(wait_for(lambda: "dbm_document" in self.driver.get_state(), 3.0))
        document = self.driver.get_state()["dbm_document"]
        # A ZS3 recall leaves the document settings alone, a snapshot load restores them
        self.driver.set_state({"dbm_document": "LAUNCH_CONTROL_XL=\"from a ZS3\"\n"})
        self.assertIsNone(self.driver.pending_document)
        self.assertEqual(self.driver.get_state()["dbm_document"], document)
        self.state.busy.add("load snapshot")
        self.driver.set_state({"dbm_document": document})
        self.state.busy.clear()
        self.assertTrue(wait_for(lambda: self.driver.pending_document is None), "Applied by the tick thread")
        self.driver.set_state({})

        self.driver.sleep_on()
        self.assertTrue(wait_for(lambda: self.driver.bridge is None), "Sleep stops the controller")
        self.driver.sleep_off()
        self.assertTrue(wait_for(lambda: self.driver.bridge is not None), "Wake up starts it again")
        self.assertTrue(wait_for(lambda: self.driver.pending_document is None), "The document settings were applied")

    def test_end_stops_the_thread(self):
        thread = self.driver.thread
        self.driver.end()
        self.assertFalse(thread.is_alive())
        self.assertIsNone(self.driver.bridge)
        self.assertTrue(os.path.isdir(os.path.join(self.temp.name, "ctrldev", "dbm")))


class MaschineMk2DriverTest(unittest.TestCase):
    """The Maschine Mk2 driver: its note input plays the pads into the active chain at once, in
    the thread which calls midi_event (the state manager's fast thread), through the core's key
    and velocity tables."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="dbm-zynthian-mk2-")
        self.old_config = os.environ.get("ZYNTHIAN_CONFIG_DIR")
        os.environ["ZYNTHIAN_CONFIG_DIR"] = self.temp.name
        self.state = zynstubs.project()
        self.zyncore = zynstubs.lib_zyncore()
        self.zyncore.notes.clear()
        self.driver = zynthian_ctrldev_dbm_maschine_mk2(self.state, IDEV_IN, IDEV_OUT)
        self.driver.init()
        self.assertTrue(wait_for(lambda: self.driver.bridge is not None and self.driver.bridge.note_inputs))

    def tearDown(self):
        self.driver.end()
        if self.old_config is None:
            os.environ.pop("ZYNTHIAN_CONFIG_DIR", None)
        else:
            os.environ["ZYNTHIAN_CONFIG_DIR"] = self.old_config
        self.temp.cleanup()

    def test_class(self):
        cls = zynthian_ctrldev_dbm_maschine_mk2
        self.assertEqual(cls.dev_ids, ["Maschine MK2 IN 1"])
        self.assertEqual(cls.dbm_controller_id, "015E4908-C049-4808-A645-A7A44BEFE36B")
        self.assertTrue(cls.get_autoload_flag())

    def test_pads_play_the_active_chain(self):
        bridge = self.driver.bridge
        self.assertEqual(bridge.note_inputs[0].filters, ["80????", "90????"], "The Mk2's note input: notes on channel 1")
        expected = bridge.translate_note(b"\x90\x24\x64")
        self.assertIsNotNone(expected, "The core lets the first pad through")
        self.driver.midi_event(b"\x90\x24\x64")
        # Played in the calling thread, before the tick
        self.assertEqual(self.zyncore.notes, [("on", 0, expected[1], expected[2])], "Synth: MIDI channel 1")
        self.assertEqual(self.state.chain_manager.active_chain.chain_id, 1)
        # The active chain changes while the pad is held: the note-off still goes to the Synth
        self.state.chain_manager.set_active_chain_by_id(2)
        self.driver.midi_event(b"\x80\x24\x00")
        self.assertEqual(self.zyncore.notes[-1], ("off", 0, expected[1], 0))
        self.driver.midi_event(b"\x90\x25\x50")
        self.assertEqual(self.zyncore.notes[-1][:2], ("on", 1), "Then Piano: MIDI channel 2")
        # The core sees the pads too, and the pad lights from the playing notes
        self.assertTrue(wait_for(lambda: bridge.reporter.last_values.get("/track/1/playingnotes", "").endswith(":80")))

    def test_no_hanging_notes(self):
        self.driver.midi_event(b"\x90\x26\x64")
        on = self.zyncore.notes[-1]
        self.driver.sleep_on()
        self.assertTrue(wait_for(lambda: self.driver.bridge is None))
        self.assertEqual(self.zyncore.notes[-1], ("off", on[1], on[2], 0), "Stopping the controller ends the note")

    def test_no_chain_no_note(self):
        self.state.chain_manager.set_active_chain_by_id(3)  # Audio In: no MIDI channel
        self.driver.midi_event(b"\x90\x24\x64")
        self.driver.midi_event(b"\x80\x24\x00")
        self.assertEqual(self.zyncore.notes, [])

    def test_cost_of_a_note(self):
        """The Python part of the note path (midi_event: translate, send, queue): reported, not
        asserted (machine dependent; the fast thread's 10 ms poll and the JACK period dominate)."""
        n = 2000
        start = time.perf_counter()
        for i in range(n):
            self.driver.midi_event(bytes([0x90, 0x24, 100]))
            self.driver.midi_event(bytes([0x80, 0x24, 0]))
        per_note = (time.perf_counter() - start) / (2 * n) * 1e6
        print(f"\nmidi_event of a pad note: {per_note:.1f} µs", file=sys.stderr)
        self.assertLess(per_note, 1000)


if __name__ == "__main__":
    unittest.main()
