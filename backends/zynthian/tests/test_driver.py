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
import tempfile
import threading
import time
import unittest

import zynstubs

zynstubs.install()

from zyngine.ctrldev.zynthian_ctrldev_dbm_launchcontrol_xl import zynthian_ctrldev_dbm_launchcontrol_xl  # noqa: E402

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


if __name__ == "__main__":
    unittest.main()
