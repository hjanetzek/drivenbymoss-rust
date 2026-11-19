# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: tests of the Push drivers and their USB display
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


"""The Push drivers: their ports, controller ids and the USB display. The display frames of the
core go through the reference host's Bridge.usb_bulk_out (ZynBridge inherits it) to
drivenbymoss.usb.UsbOutput, on zynthian the usbdevfs writer. No Push was connected: the frames
go to a recording output; with a Push factory in the core, the Push 2 runs against the adapter
and its frames are checked."""

import importlib
import tempfile
import unittest

import zynstubs

zynstubs.install()

import drivenbymoss  # noqa: E402
from dbm_host import Bridge  # noqa: E402
from drivenbymoss.usb import PUSH_FRAME_HEADER, UsbOutput  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.bridge import ZynBridge  # noqa: E402

DRIVERS = [
    ("push1", ["Ableton Push User Port", "Ableton Push IN 2"], "DBED9610-C474-11E6-9598-0800200C9A66"),
    ("push2", ["Ableton Push 2 Live Port", "Ableton Push 2 IN 1"], "15176AA0-C476-11E6-9598-0800200C9A66"),
    ("push3", ["Ableton Push 3 Live Port", "Ableton Push 3 IN 1"], "F8D561F2-722D-4DC7-975A-C562788C63FA"),
]


class RecordingOutput(UsbOutput):
    def __init__(self):
        super().__init__(None)
        self.writes = []

    def write(self, vendor_id, product_id, interface, endpoint, data):
        self.writes.append((vendor_id, product_id, interface, endpoint, len(data), bytes(data[:16])))
        return True


class PushDriverTests(unittest.TestCase):
    def test_drivers(self):
        for key, dev_ids, instance in DRIVERS:
            module = importlib.import_module(f"zyngine.ctrldev.zynthian_ctrldev_dbm_{key}")
            cls = getattr(module, f"zynthian_ctrldev_dbm_{key}")
            self.assertEqual(cls.dev_ids, dev_ids)
            self.assertEqual(cls.dbm_controller_id, instance)
            self.assertTrue(cls.autoload_flag)
            self.assertIsNone(cls.nihost_kind, "the Push is class compliant USB MIDI")

    def test_zynthian_bridge_sends_the_usb_writes(self):
        self.assertIs(ZynBridge.usb_bulk_out, Bridge.usb_bulk_out)

    def test_push2_display_frames(self):
        """With a Push 2 factory in the core: its frames reach the USB output of the bridge."""
        from zyngine.ctrldev.zynthian_ctrldev_dbm_push2 import zynthian_ctrldev_dbm_push2 as driver
        from zyngine.ctrldev.dbm_zynthian.model import ZynthianModel
        controllers = {c["uuid"].lower() for c in drivenbymoss.list_controllers()}
        if driver.dbm_controller_id.lower() not in controllers:
            self.skipTest("no Push 2 in the core")
        state = zynstubs.project()
        with tempfile.TemporaryDirectory(prefix="dbm-zyn-push-") as config:
            model = ZynthianModel(state.chain_manager, zynseq=getattr(state, "zynseq", None))
            bridge = ZynBridge(model, driver.dbm_controller_id, config, "zynthian", midi_sink=lambda data: None)
            output = bridge.usb_output = RecordingOutput()  # Backend.usb_output, the default writer
            bridge.start()
            try:
                bridge.wait_for_startup()
                bridge.advance(2000)
                self.assertTrue(output.writes)
                self.assertEqual(output.writes[0][:4], (0x2982, 0x1967, 0, 0x01))
                self.assertEqual(output.writes[0][5], PUSH_FRAME_HEADER)
            finally:
                bridge.close()


if __name__ == "__main__":
    unittest.main()
