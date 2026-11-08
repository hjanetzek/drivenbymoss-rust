# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: the Launch Control XL with the core on a mocked zynthian
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

"""The Novation Launch Control XL, run by the DrivenByMoss core (libdrivenbymoss), against the
model adapter on a mocked zynthian state, with simulated time (``ZynBridge.advance``): the
phase 1 scope of doc/ZYNTHIAN_PLAN.md (volume, pan, sends, mute, solo, arm, selection, paging).

The device is simulated as in the reference host's test (bindings/python/tests/test_host.py):
it echoes the template selection; the startup selects the factory template 1 (Track Mix, MIDI
channel 9: CC B8, notes 98/88).
"""

import tempfile
import unittest

import zynstubs

zynstubs.install()

from zyngine.ctrldev.dbm_zynthian.bridge import ZynBridge  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.model import ZynthianModel  # noqa: E402
from zyngine.ctrldev.zynthian_ctrldev_dbm_launchcontrol_xl import zynthian_ctrldev_dbm_launchcontrol_xl as driver  # noqa: E402

TEMPLATE_HEADER = bytes.fromhex("F0002029021177")

# The Track Mix template (channel 9)
FADERS = range(0x4D, 0x55)
SEND_A = range(0x0D, 0x15)
SEND_B = range(0x1D, 0x25)
PAN = range(0x31, 0x39)
TRACK_FOCUS = [0x29, 0x2A, 0x2B, 0x2C, 0x39, 0x3A, 0x3B, 0x3C]
TRACK_CONTROL = [0x49, 0x4A, 0x4B, 0x4C, 0x59, 0x5A, 0x5B, 0x5C]
MUTE, SOLO, RECARM = 0x6A, 0x6B, 0x6C
TRACK_RIGHT = 0x6B


class LaunchControlXLTest(unittest.TestCase):

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="dbm-zynthian-test-")
        self.state = zynstubs.project()
        self.model = ZynthianModel(self.state.chain_manager, palette=["#0000FF", "#BBBB00", "#FF00FF"])
        self.midi_out = []
        self.bridge = ZynBridge(self.model, driver.dbm_controller_id, self.temp.name, "zynthian", midi_sink=self.device)
        self.bridge.start()
        self.bridge.wait_for_startup()
        self.bridge.advance(100)

    def tearDown(self):
        self.bridge.close()
        self.temp.cleanup()

    def device(self, data):
        """The device: keeps the LEDs, echoes the template selection."""
        self.midi_out.append(bytes(data))
        if data[: len(TEMPLATE_HEADER)] == TEMPLATE_HEADER:
            self.bridge.receive(data)

    def led(self, status, data1):
        for data in reversed(self.midi_out):
            if len(data) == 3 and data[0] == status and data[1] == data1:
                return data[2]
        return None

    def cc(self, cc, value, ms=30):
        self.bridge.send([0xB8, cc, value], ms)

    def press(self, note):
        self.bridge.send([0x98, note, 127])
        self.bridge.send([0x88, note, 0])

    def assert_clean(self):
        self.assertEqual(list(self.bridge.core_errors()), [], "The core knows all reported addresses")

    def zctrl(self, chain_id, symbol):
        return self.state.zctrl(chain_id, symbol)

    def test_startup(self):
        self.assertIn(bytes.fromhex("F000202902117708F7"), self.midi_out, "Template selection at startup")
        self.assertEqual(self.led(0x98, MUTE), 0x7F, "The Mute mode of the Track Mix is on")
        # The model setup has hasFullFlatTrackList: the effect tracks and the master are tracks too
        self.assertTrue(self.bridge.banks.full_flat_track_list)
        # and the add chain placeholder after the chains
        self.assertEqual(self.bridge.reporter.last_values["/track/count"], "8")
        self.assertEqual(self.bridge.reporter.last_values["/track/4/name"], "+")
        self.assertEqual(self.bridge.reporter.last_values["/track/7/type"], "MASTER")
        self.assert_clean()

    def test_faders_and_knobs(self):
        self.cc(FADERS[0], 127, 400)
        self.assertAlmostEqual(self.zctrl(1, "level").value, 1.0, places=6)
        self.cc(FADERS[2], 0, 400)
        self.assertEqual(self.zctrl(3, "level").value, 0.0)
        self.cc(PAN[1], 0, 400)
        self.assertAlmostEqual(self.zctrl(2, "balance").value, -1.0, places=6)
        self.cc(SEND_A[0], 127, 400)
        self.assertAlmostEqual(self.zctrl(1, "send_5_level").value, 1.0, places=6)
        self.cc(SEND_B[0], 64, 400)
        self.assertAlmostEqual(self.zctrl(1, "send_6_level").value, 64 / 127, places=6)
        # The MIDI chain without mixer strip: its controls do nothing
        self.cc(FADERS[3], 10, 400)
        self.assert_clean()

    def test_mute_solo_arm(self):
        unmuted = self.led(0x98, TRACK_CONTROL[1])
        self.press(TRACK_CONTROL[1])
        self.bridge.advance(100)
        self.assertEqual(self.zctrl(2, "mute").value, 1)
        self.assertNotEqual(self.led(0x98, TRACK_CONTROL[1]), unmuted, "The LED follows the report")
        self.press(TRACK_CONTROL[1])
        self.bridge.advance(100)
        self.assertEqual(self.zctrl(2, "mute").value, 0)

        # A mute in zynthian (GUI, MIDI learn) is shown as well
        self.zctrl(3, "mute").set_value(1)
        self.bridge.advance(100)
        self.assertNotEqual(self.led(0x98, TRACK_CONTROL[2]), unmuted)

        self.press(SOLO)
        self.press(TRACK_CONTROL[0])
        self.bridge.advance(100)
        self.assertEqual(self.zctrl(1, "solo").value, 1)

        self.press(RECARM)
        self.press(TRACK_CONTROL[2])
        self.bridge.advance(100)
        self.assertEqual(self.zctrl(3, "record").value, 1)
        self.assert_clean()

    def test_select_with_track_focus(self):
        self.press(TRACK_FOCUS[2])
        self.bridge.advance(200)
        self.assertEqual(self.state.chain_manager.active_chain.chain_id, 3)
        # A selection in zynthian reaches the controller
        self.state.chain_manager.set_active_chain_by_id(2)
        self.bridge.advance(100)
        self.assertEqual(self.bridge.reporter.last_values["/track/1/select"], "1")
        self.assert_clean()

    def test_paging(self):
        for chain_id in range(10, 16):
            self.state.add_chain(chain_id, f"Chain {chain_id}", midi_chan=chain_id - 7)
        self.bridge.advance(100)
        self.assertEqual(self.bridge.reporter.last_values["/track/count"], "14")
        self.bridge.send([0xB8, TRACK_RIGHT, 127])
        self.bridge.send([0xB8, TRACK_RIGHT, 0])
        self.bridge.advance(300)
        offset = self.bridge.banks.track_offset
        self.assertGreater(offset, 0, "The core paged the tracks (bank track)")
        self.cc(FADERS[0], 0, 400)
        moved = self.model.track_list(True)[offset]
        self.assertEqual(moved.volume.value, 0.0, "The first fader controls the first track of the page")
        self.assert_clean()

    def test_unhandled_are_later_phases(self):
        """Every control of the Track Mix, also with the mode buttons held: what is not handled
        belongs to a later phase (transport, scenes, devices) or is a gap (send active)."""
        mode_buttons = [0x69, MUTE, SOLO, RECARM]
        for note in TRACK_FOCUS + TRACK_CONTROL + mode_buttons:
            self.press(note)
        for knob in [*FADERS, *SEND_A, *SEND_B, *PAN]:
            self.cc(knob, 100)
            self.cc(knob, 20)
        for hold in mode_buttons:
            self.bridge.send([0x98, hold, 127])
            for note in TRACK_FOCUS + TRACK_CONTROL:
                self.press(note)
            for cc in (0x68, 0x69, 0x6A, 0x6B):
                self.cc(cc, 127)
                self.cc(cc, 0)
            self.bridge.send([0x88, hold, 0])
        self.bridge.advance(500)
        # The transport (also the Reaper forms: without transportCommands), devices, clips
        later = {"transport", "scene", "device", "primary", "drum", "clip", "browser", "marker",
                 "application", "project", "groove", "noterepeat", "navigator", "play", "stop", "record",
                 "tempo", "beats", "repeat", "automation", "undo", "redo"}
        later_track = {"track <n>/createClip/-<n>", "track <n>/overdub", "track <n>/send/<n>/active"}
        in_scope = [key for key in self.bridge.commands.unhandled
                    if key.split(" ")[0] not in later and key not in later_track]
        self.assertEqual(in_scope, [], self.bridge.commands.unhandled)
        self.assert_clean()


if __name__ == "__main__":
    unittest.main()
