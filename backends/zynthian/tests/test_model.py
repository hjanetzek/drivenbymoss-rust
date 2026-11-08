# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: tests of the model, the reporter and the commands
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

"""The adapter against a mocked zynthian state, without the core.

Run from the zynthian-ui checkout::

    python3 backends/zynthian/tests/run_tests.py
"""

import unittest

import zynstubs

zynstubs.install()

from dbm_host import Banks  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.commands import ZynCommandHandler  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.model import ZynthianModel, dbfs_to_vu, format_level  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.reporter import ZynReporter  # noqa: E402


class Host:
    """The model with a reporter and a command handler, as ZynBridge creates them."""

    def __init__(self, state=None, setup=None):
        self.state = state or zynstubs.project()
        self.clock = 0.0
        self.model = ZynthianModel(self.state.chain_manager, palette=["#0000FF", "#BBBB00", "#FF00FF"])
        self.banks = Banks.from_model_setup(setup or {"numTracks": 8, "numSends": 2})
        self.reporter = ZynReporter(self.model, self.banks, clock=lambda: self.clock)
        self.commands = ZynCommandHandler(self.model, self.banks, self.reporter, clock=lambda: self.clock)
        self.commands.configure()
        self.reporter.features = self.commands.host_features

    def report(self):
        # A full pass: the state is changed directly, without zynthian's change signals (the
        # two rates: test_rates.py)
        self.reporter.request_full()
        return dict(line.split(" ", 1) for line in self.reporter.collect().splitlines())

    def command(self, processor, command, value=None):
        self.commands.handle(processor, command, value)


class ModelTest(unittest.TestCase):

    def test_tracks_effect_tracks_master(self):
        model = Host().model
        self.assertEqual([t.name for t in model.tracks()], ["Synth", "Piano", "Audio In", "MIDI Tool"])
        self.assertEqual([t.name for t in model.effect_tracks()], ["Reverb", "Delay"])
        self.assertEqual(model.master().name, "Main")
        self.assertEqual([t.type_name for t in model.track_list(True)],
                         ["INSTRUMENT", "INSTRUMENT", "AUDIO", "INSTRUMENT", "AUDIO", "EFFECT", "EFFECT", "MASTER"],
                         "The add chain placeholder after the chains")
        self.assertEqual(model.track_list(True)[4].name, "+")

    def test_values_and_texts(self):
        host = Host()
        synth = host.model.tracks()[0]
        self.assertAlmostEqual(synth.volume.value, 0.8)
        self.assertEqual(synth.volume.displayed(), "-1.9 dB")
        self.assertEqual(synth.pan.value, 0.5)
        self.assertEqual(synth.pan.displayed(), "0.00 %")
        self.assertEqual(format_level(0), "-Inf dB")
        self.assertEqual(format_level(1.0), "0.0 dB")
        self.assertEqual(synth.color, (0, 0, 255))
        self.assertEqual(host.model.tracks()[2].color, (128, 128, 128), "An audio chain has no MIDI channel")
        self.assertEqual([s.name for s in synth.sends], ["Reverb", "Delay"])

    def test_vu(self):
        host = Host()
        self.assertEqual(dbfs_to_vu(-200), 0.0)
        self.assertEqual(dbfs_to_vu(0), 1.0)
        self.assertEqual(dbfs_to_vu(-30), 0.5)
        host.state.zynmixer_chan.lib_zynmixer.dpm[(0, 0)] = -30.0
        self.assertEqual(host.model.tracks()[0].vu(), (0.5, 0.0))


class ReporterTest(unittest.TestCase):

    def test_first_report_is_complete(self):
        report = Host().report()
        self.assertEqual(report["/host/features"], "valueInc valueReset immediately soloToggle deviceCommands",
                         "No transportCommands without zynseq")
        self.assertEqual((report["/project/canUndo"], report["/project/canRedo"]), ("0", "0"))
        self.assertEqual(report["/track/offset"], "0")
        self.assertEqual(report["/track/count"], "5", "The chains and the add chain placeholder")
        self.assertEqual(report["/track/0/name"], "Synth")
        self.assertEqual(report["/track/0/select"], "1")
        self.assertEqual(report["/track/1/select"], "0")
        self.assertEqual(report["/track/0/volume"], "0.800000")
        self.assertEqual(report["/track/0/volume/str"], "-1.9 dB")
        self.assertEqual(report["/track/0/volume/str/6"], "-1.9")
        self.assertEqual(report["/track/0/pan"], "0.500000")
        self.assertEqual(report["/track/0/send/count"], "2")
        self.assertEqual(report["/track/0/send/1/name"], "Delay")
        self.assertEqual(report["/track/3/canHoldAudio"], "0", "The MIDI chain has no mixer strip")
        self.assertEqual(report["/fxtrack/count"], "2")
        self.assertEqual(report["/fxtrack/0/name"], "Reverb")
        self.assertEqual(report["/master/name"], "Main")
        self.assertNotIn("/track/0/active", report, "No activation in zynthian")
        self.assertNotIn("/play", report, "No transport without zynseq")
        self.assertEqual(report["/device/exists"], "0", "No processors in the chains")

    def test_only_changes(self):
        host = Host()
        host.report()
        self.assertEqual(host.report(), {})
        host.state.zctrl(2, "mute").set_value(1)
        self.assertEqual(host.report(), {"/track/1/mute": "1"})

    def test_selection_change_in_one_batch(self):
        host = Host()
        host.report()
        host.state.chain_manager.set_active_chain_by_id(3)
        host.reporter.request_full()
        lines = host.reporter.collect().splitlines()
        self.assertEqual(lines, ["/track/0/select 0", "/track/2/select 1", "/track/2/playingnotes "],
                         "The playing notes of the new cursor track are reported again")

    def test_removed_chain(self):
        host = Host()
        host.report()
        host.state.remove_chain(3)
        report = host.report()
        self.assertEqual(report["/track/count"], "4")
        self.assertEqual(report["/track/4/exists"], "0")
        self.assertEqual(report["/track/2/name"], "MIDI Tool")
        self.assertEqual(report["/track/3/name"], "+", "The placeholder moves up")

    def test_new_mixbus_adds_sends(self):
        host = Host()
        host.report()
        host.state.add_chain(7, "Chorus", mixbus=True)
        report = host.report()
        self.assertEqual(report["/fxtrack/count"], "3")
        self.assertNotIn("/track/0/send/count", report, "Still 2: limited by numSends of the model setup")

    def test_refresh_dumps(self):
        host = Host()
        host.report()
        host.command("refresh", "")
        self.assertEqual(host.report()["/track/0/name"], "Synth")


class CommandTest(unittest.TestCase):

    def test_volume_pan_with_suffixes(self):
        host = Host()
        host.command("track", "0/volume", 0.5)
        self.assertAlmostEqual(host.state.zctrl(1, "level").value, 0.5)
        host.command("track", "0/volume/inc", 0.1)
        self.assertAlmostEqual(host.state.zctrl(1, "level").value, 0.6)
        host.command("track", "0/volume/reset")
        self.assertAlmostEqual(host.state.zctrl(1, "level").value, 0.8)
        host.command("track", "1/pan", 0.0)
        self.assertEqual(host.state.zctrl(2, "balance").value, -1.0)
        host.command("track", "1/pan/immediately", 0.75)
        self.assertEqual(host.state.zctrl(2, "balance").value, 0.5)
        host.command("master", "volume", 1.0)
        self.assertEqual(host.state.zctrl(0, "level").value, 1.0)
        self.assertEqual(host.commands.unhandled, [])

    def test_mute_solo_arm_select(self):
        host = Host()
        host.command("track", "0/mute", 1)
        host.command("track", "1/solo/toggle")
        host.command("track", "2/recarm", 1)
        host.command("track", "2/select", 1)
        self.assertEqual(host.state.zctrl(1, "mute").value, 1)
        self.assertEqual(host.state.zctrl(2, "solo").value, 1)
        self.assertEqual(host.state.zctrl(3, "record").value, 1)
        self.assertEqual(host.state.chain_manager.active_chain.chain_id, 3)
        host.command("track", "selectPrevious")
        self.assertEqual(host.state.chain_manager.active_chain.chain_id, 2)
        host.command("track", "0/solo", 0)
        self.assertEqual(host.commands.unhandled, [])

    def test_sends(self):
        host = Host()
        host.command("track", "0/send/1/volume", 0.25)
        self.assertEqual(host.state.zctrl(1, "send_6_level").value, 0.25)
        host.command("track", "0/send/1/active", 0.25)
        self.assertEqual(host.commands.unhandled, ["track <n>/send/<n>/active"], "zynthian's sends have no switch")

    def test_effect_track(self):
        host = Host()
        host.command("fxtrack", "1/volume", 0.5)
        self.assertEqual(host.state.zctrl(6, "level").value, 0.5)

    def test_chain_without_mixer_ignores_mixer_commands(self):
        host = Host()
        host.command("track", "3/volume", 0.5)
        host.command("track", "3/mute", 1)
        self.assertEqual(host.commands.unhandled, [])

    def test_report_again_after_value_command(self):
        host = Host()
        host.report()
        host.command("track", "0/volume", 0.5)
        self.assertEqual(host.report()["/track/0/volume"], "0.500000")
        host.clock = 400
        self.assertEqual(host.report()["/track/0/volume"], "0.500000", "Reported again after 300 ms")

    def test_unknown_processors_are_logged_once(self):
        host = Host()
        host.command("transport", "play")
        host.command("transport", "play")
        host.command("scene", "0/launch", 1)
        self.assertEqual(host.commands.unhandled, ["transport play", "scene <n>/launch"])

    def test_bank(self):
        host = Host()
        host.command("bank", "track", 8)
        self.assertEqual(host.report()["/track/offset"], "8")


if __name__ == "__main__":
    unittest.main()
