# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: zynthian's ctrldev manager loads the in-process Maschine drivers
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

"""The HID drivers of zynthian-ui's ``dbm`` branch (zynthian_ctrldev_manager.update_hid_drivers):
a driver with ``hid_ids`` is loaded without a MIDI port when its device is in /sys/class/hidraw
(here a fake sysfs). Skipped with a zynthian-ui without them."""

import os
import tempfile
import time
import unittest

import zynstubs

zynstubs.install()

from zyngine import zynthian_ctrldev_manager as manager_module  # noqa: E402
from zyngine.ctrldev.dbm_zynthian import nihost  # noqa: E402
from zyngine.ctrldev.zynthian_ctrldev_dbm_launchcontrol_xl import zynthian_ctrldev_dbm_launchcontrol_xl  # noqa: E402
from zyngine.ctrldev.zynthian_ctrldev_dbm_maschine_jam import zynthian_ctrldev_dbm_maschine_jam  # noqa: E402
from zyngine.ctrldev.zynthian_ctrldev_dbm_maschine_mk2 import zynthian_ctrldev_dbm_maschine_mk2  # noqa: E402

from test_nihost_inprocess import FakeNihostLib  # noqa: E402

JAM_UID = "HID:17cc:1500"
MK2_UID = "HID:17cc:1140"


@unittest.skipUnless(hasattr(manager_module.zynthian_ctrldev_manager, "update_hid_drivers"),
                     "zynthian-ui without HID drivers")
class HidManagerTest(unittest.TestCase):

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="dbm-zynthian-hid-")
        self.sysfs = os.path.join(self.temp.name, "hidraw")
        os.makedirs(self.sysfs)
        self.old_config = os.environ.get("ZYNTHIAN_CONFIG_DIR")
        os.environ["ZYNTHIAN_CONFIG_DIR"] = self.temp.name
        self.old_load = nihost.load_library
        self.libs = []

        def load():
            lib = FakeNihostLib()
            self.libs.append(lib)
            return lib

        nihost.load_library = load
        self.state = zynstubs.project()
        # The manager without its __init__ (which imports the drivers of the installed zynthian)
        cls = manager_module.zynthian_ctrldev_manager
        self.manager = cls.__new__(cls)
        self.manager.state_manager = self.state
        self.manager.driver_classes = {c.__name__: c for c in (
            zynthian_ctrldev_dbm_launchcontrol_xl, zynthian_ctrldev_dbm_maschine_mk2, zynthian_ctrldev_dbm_maschine_jam)}
        self.manager.available_drivers = {}
        self.manager.drivers = {}
        self.manager.hid_drivers = {}
        self.manager.disabled_devices = []

    def tearDown(self):
        self.manager.unload_all_drivers()
        nihost.load_library = self.old_load
        if self.old_config is None:
            os.environ.pop("ZYNTHIAN_CONFIG_DIR", None)
        else:
            os.environ["ZYNTHIAN_CONFIG_DIR"] = self.old_config
        self.temp.cleanup()

    def plug(self, node, hid_id):
        os.makedirs(f"{self.sysfs}/{node}/device")
        with open(f"{self.sysfs}/{node}/device/uevent", "w") as f:
            f.write(f"DRIVER=hid-generic\nHID_ID={hid_id}\nHID_NAME=NI\n")

    def unplug(self, node):
        os.remove(f"{self.sysfs}/{node}/device/uevent")

    def test_load_when_present(self):
        self.plug("hidraw0", "0003:0000046D:0000C52B")
        self.assertEqual(self.manager.update_hid_drivers(self.sysfs), [])
        self.plug("hidraw3", "0003:000017CC:00001500")
        self.assertEqual(self.manager.update_hid_drivers(self.sysfs), [JAM_UID])
        driver = self.manager.hid_drivers[JAM_UID]
        self.assertIsInstance(driver, zynthian_ctrldev_dbm_maschine_jam)
        self.assertTrue(driver.in_process)
        self.assertIsNotNone(driver.nihost)
        self.assertEqual(self.manager.drivers, {}, "No zmip")
        # Once
        self.assertEqual(self.manager.update_hid_drivers(self.sysfs), [])
        self.plug("hidraw4", "0003:000017CC:00001140")
        self.assertEqual(self.manager.update_hid_drivers(self.sysfs), [MK2_UID])
        # Unplugged: the driver stays (dormant until the library's hello)
        self.unplug("hidraw3")
        self.assertEqual(self.manager.update_hid_drivers(self.sysfs), [])
        self.assertIn(JAM_UID, self.manager.hid_drivers)

    def test_disabled(self):
        self.plug("hidraw3", "0003:000017CC:00001500")
        self.manager.update_hid_drivers(self.sysfs)
        driver = self.manager.hid_drivers[JAM_UID]
        self.assertTrue(self.manager.unload_hid_driver(JAM_UID, True))
        self.assertIsNone(driver.nihost, "Closed")
        self.assertTrue(self.libs[0].closed)
        self.assertEqual(self.manager.update_hid_drivers(self.sysfs), [], "Disabled: not loaded again")
        self.assertTrue(self.manager.load_hid_driver(JAM_UID, zynthian_ctrldev_dbm_maschine_jam))
        self.assertNotIn(JAM_UID, self.manager.disabled_devices)

    def test_state(self):
        self.plug("hidraw3", "0003:000017CC:00001500")
        self.manager.update_hid_drivers(self.sysfs)
        driver = self.manager.hid_drivers[JAM_UID]
        driver.document = {"a": 1}
        self.assertEqual(self.manager.get_state_drivers(), {JAM_UID: {"dbm_document": {"a": 1}}})
        received = []
        driver.set_state = received.append
        self.manager.set_state_drivers({JAM_UID: {"dbm_document": {"b": 2}}, MK2_UID: {"dbm_document": {}}})
        self.assertEqual(received, [{"dbm_document": {"b": 2}}])

    def test_sleep_and_unload_all(self):
        self.plug("hidraw3", "0003:000017CC:00001500")
        self.manager.update_hid_drivers(self.sysfs)
        driver = self.manager.hid_drivers[JAM_UID]
        self.manager.sleep_on()
        self.assertTrue(driver.sleeping)
        self.manager.sleep_off()
        self.assertFalse(driver.sleeping)
        thread = driver.thread
        self.manager.unload_all_drivers()
        self.assertEqual(self.manager.hid_drivers, {})
        time.sleep(0.1)
        self.assertFalse(thread.is_alive())


if __name__ == "__main__":
    unittest.main()
