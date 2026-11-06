# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: the driver's dev_id and zynthian's PipeWire port names
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

"""The Launch Control XL driver's dev_id against the uids zynthian gives PipeWire's MIDI bridge
ports (zynautoconnect/zynthian_pipewire_ports.py of the zynthian-ui checkout; its own tests are
there). The port names are those PipeWire showed with the device connected (pw-link -o,
2026-11-06): the controls ("Launch Contro") and HUI, in this order."""

import importlib.util
import unittest

import zynstubs

zynstubs.install()

from zyngine.ctrldev.zynthian_ctrldev_dbm_launchcontrol_xl import zynthian_ctrldev_dbm_launchcontrol_xl as driver  # noqa: E402
from zyngine.ctrldev.zynthian_ctrldev_dbm_maschine_mk2 import zynthian_ctrldev_dbm_maschine_mk2 as mk2  # noqa: E402

PORTS = zynstubs.UI_DIR / "zynautoconnect" / "zynthian_pipewire_ports.py"
CAPTURE = [
    "Midi-Bridge:Midi Through: Port-0 (capture)",
    "Midi-Bridge:Launch Control XL: Launch Contro (capture)",
    "Midi-Bridge:Launch Control XL: HUI (capture)",
    # dbm-nihost's clients (pw-link -o, 2026-11-06) and the Mk2's own USB MIDI port
    "Midi-Bridge:Maschine MK2: In (capture)",
    "Midi-Bridge:Maschine Jam - 1: Input (capture)",
    "Midi-Bridge:Maschine Controller MK2: MIDI 1 (capture)",
]


@unittest.skipUnless(PORTS.exists(), "zynthian-ui without the PipeWire port names")
class DeviceIdTest(unittest.TestCase):

    def test_the_controls_port_not_hui(self):
        spec = importlib.util.spec_from_file_location("zynthian_pipewire_ports", PORTS)
        ports = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(ports)
        uid, _ = ports.pipewire_midi_port_name(CAPTURE[1], CAPTURE)
        self.assertEqual(driver.dev_ids, [uid.split("/", 1)[1]], "zynautoconnect.get_midi_in_devid matches the driver")
        hui_uid, _ = ports.pipewire_midi_port_name(CAPTURE[2], CAPTURE)
        self.assertNotIn(hui_uid.split("/", 1)[1], driver.dev_ids, "No driver on the HUI port")


@unittest.skipUnless(PORTS.exists(), "zynthian-ui without the PipeWire port names")
class MaschineDeviceIdTest(unittest.TestCase):

    def load(self):
        spec = importlib.util.spec_from_file_location("zynthian_pipewire_ports", PORTS)
        ports = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(ports)
        return ports

    def test_dbm_nihost_port_not_the_jam_or_the_midi_mode(self):
        ports = self.load()
        dev_ids = {name: ports.pipewire_midi_port_name(name, CAPTURE)[0].split("/", 1)[1] for name in CAPTURE[3:]}
        self.assertEqual(dev_ids["Midi-Bridge:Maschine MK2: In (capture)"], "Maschine MK2 IN 1")
        self.assertEqual(mk2.dev_ids, ["Maschine MK2 IN 1"])
        self.assertNotIn(dev_ids["Midi-Bridge:Maschine Jam - 1: Input (capture)"], mk2.dev_ids)
        self.assertNotIn(dev_ids["Midi-Bridge:Maschine Controller MK2: MIDI 1 (capture)"], mk2.dev_ids)
        out_uid, _ = ports.pipewire_midi_port_name("Midi-Bridge:Maschine MK2: Out (playback)")
        self.assertEqual(ports.dev_out_uid("PW:Maschine MK2/Maschine MK2 IN 1"), out_uid)


if __name__ == "__main__":
    unittest.main()
