# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: the two rates of the reporter
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

"""The two rates of ZynReporter (doc/ZYNTHIAN_CPU.md option 2): the fast pass reads only what
changes by itself (transport, peak meters, playing notes, playing steps); the slow part after a
command, a change signal of zynthian, a due "report again", a dump, else every
SLOW_INTERVAL_MS. The reporter on a controlled clock; the signals through a ZynBridge."""

import math
import tempfile
import unittest

import zynstubs

zynstubs.install()

from dbm_host import Banks  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.bridge import ZynBridge, change_signals  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.commands import ZynCommandHandler  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.model import ZynthianModel  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.reporter import ALL_SUBTREES, SLOW_INTERVAL_MS, ZynReporter  # noqa: E402
from zyngine.ctrldev.zynthian_ctrldev_dbm_launchcontrol_xl import zynthian_ctrldev_dbm_launchcontrol_xl as lcxl  # noqa: E402
from zyngine.zynthian_signal_manager import zynsigman  # noqa: E402


class Host:
    def __init__(self):
        self.state = zynstubs.project()
        self.clock = 0.0
        self.model = ZynthianModel(self.state.chain_manager, palette=["#0000FF", "#BBBB00", "#FF00FF"])
        self.banks = Banks.from_model_setup({"numTracks": 8, "numSends": 2})
        self.reporter = ZynReporter(self.model, self.banks, clock=lambda: self.clock)
        self.commands = ZynCommandHandler(self.model, self.banks, self.reporter, clock=lambda: self.clock)
        self.commands.configure()
        self.reporter.features = self.commands.host_features

    def tick(self, ms=30):
        self.clock += ms
        return dict(line.split(" ", 1) for line in self.reporter.collect().splitlines())


class RatesTest(unittest.TestCase):

    def test_fast_between_full_passes(self):
        host = Host()
        self.assertIn("/track/0/name", host.tick(), "The first pass is a dump")
        full = host.reporter.full_passes
        for _ in range(math.ceil(SLOW_INTERVAL_MS / 30) - 1):
            host.tick()
        self.assertEqual(host.reporter.full_passes, full, "Idle: fast passes")
        host.tick()
        self.assertEqual(host.reporter.full_passes, full + 1, f"A full pass every {SLOW_INTERVAL_MS:.0f} ms")

    def test_meters_and_notes_on_every_pass(self):
        host = Host()
        host.tick()
        host.state.zynmixer_chan.lib_zynmixer.dpm[(0, 0)] = -30.0
        host.model.note_played(0x90, 60, 100)
        full = host.reporter.full_passes
        report = host.tick()
        self.assertEqual(host.reporter.full_passes, full, "A fast pass")
        self.assertEqual(report["/track/0/vuleft"], "0.500000")
        self.assertEqual(report["/track/0/playingnotes"], "0:0:0:0:0:60:100")
        host.state.zynmixer_bus.lib_zynmixer.dpm[(0, 1)] = -30.0
        self.assertEqual(host.tick()["/master/vuright"], "0.500000", "The master's too")

    def test_unsignalled_values_at_the_slow_rate(self):
        """A send level (zynmixer has no signal for it): with the next full pass."""
        host = Host()
        host.tick()
        host.state.zctrl(1, "send_6_level").set_value(0.5)
        reported = []
        ms = 0
        while "/track/0/send/1/volume" not in reported and ms <= SLOW_INTERVAL_MS + 30:
            reported += list(host.tick())
            ms += 30
        self.assertIn("/track/0/send/1/volume", reported)
        self.assertGreater(ms, 30, "Not in the next fast pass")

    def test_command_makes_the_next_pass_full(self):
        host = Host()
        host.tick()
        host.commands.handle("track", "1/mute", 1)
        self.assertEqual(host.tick(), {"/track/1/mute": "1"})

    def test_report_again_after_a_value_command(self):
        host = Host()
        host.tick()
        host.commands.handle("track", "0/volume", 0.5)  # at 30 ms: again at 330 ms
        self.assertEqual(host.tick()["/track/0/volume"], "0.500000")
        host.tick(240)  # 300 ms: fast
        host.tick(20)   # 320 ms: the slow full pass
        full = host.reporter.full_passes
        report = host.tick(15)  # 335 ms: due, a full pass between the slow ones
        self.assertEqual(host.reporter.full_passes, full + 1)
        self.assertEqual(report["/track/0/volume"], "0.500000", "Reported again")

    def test_subtrees(self):
        """Each subtree of the slow part is collected on its own; a full pass is all of them."""
        host = Host()
        host.tick()
        host.state.zctrl(2, "mute").set_value(1)
        host.state.chain_manager.chains[1].title = "Renamed"
        host.reporter.collect_subtrees({"device"})
        self.assertEqual(host.reporter.take(), "", "Neither the mixer nor the names")
        host.reporter.collect_subtrees({"tracks"})
        self.assertEqual(host.reporter.take(), "/track/0/name Renamed\n", "The track list without the mixer values")
        host.reporter.collect_subtrees({"mixer"})
        self.assertEqual(host.reporter.take(), "/track/1/mute 1\n")
        # The meters stay for the fast passes
        host.state.zynmixer_chan.lib_zynmixer.dpm[(1, 0)] = -30.0
        host.reporter.run_collectors(host.reporter.fast_collectors)
        self.assertIn("/track/1/vuleft 0.500000", host.reporter.take())

    def test_request_records_the_subtrees(self):
        host = Host()
        host.tick()
        host.reporter.dirty.clear()
        host.reporter.request_full({"slots"})
        self.assertEqual(host.reporter.dirty, {"slots"})
        host.tick()
        self.assertEqual(host.reporter.dirty, set(), "Collected")
        for _signal, _subsignal, subtrees in change_signals():
            self.assertTrue(set(subtrees) <= ALL_SUBTREES)


class SignalTest(unittest.TestCase):
    """zynthian's change signals reach the reporter of a running bridge: a mute in zynthian's UI
    is reported in the next pass, not with the slow one."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="dbm-zynthian-rates-")
        self.addCleanup(self.temp.cleanup)
        self.state = zynstubs.project()
        self.model = ZynthianModel(self.state.chain_manager)
        self.bridge = ZynBridge(self.model, lcxl.dbm_controller_id, self.temp.name, "zynthian", midi_sink=lambda data: None)
        self.addCleanup(self.bridge.close)
        self.bridge.start()
        self.bridge.wait_for_startup()
        self.bridge.advance(SLOW_INTERVAL_MS)

    def test_mute_in_zynthian(self):
        reporter = self.bridge.reporter
        self.bridge.advance(30)
        full = reporter.full_passes
        self.state.zctrl(3, "mute").set_value(1)
        self.assertTrue(reporter.full_requested)
        self.assertEqual(reporter.dirty & {"mixer"}, {"mixer"})
        self.bridge.advance(30)
        self.assertEqual(reporter.full_passes, full + 1, "One pass")
        self.assertEqual(reporter.last_values["/track/2/mute"], "1")

    def test_active_chain_in_zynthian(self):
        self.bridge.advance(30)
        self.state.chain_manager.set_active_chain_by_id(2)
        self.bridge.advance(30)
        self.assertEqual(self.bridge.reporter.last_values["/track/1/select"], "1")

    def test_unregistered_on_close(self):
        reporter = self.bridge.reporter
        self.bridge.close()
        reporter.full_requested = False
        zynsigman.send(zynsigman.S_MIXER, zynsigman.SS_ZYNMIXER_SET_VALUE, chan=0, symbol="mute", value=1, mixbus=False)
        self.assertFalse(reporter.full_requested)


if __name__ == "__main__":
    unittest.main()
