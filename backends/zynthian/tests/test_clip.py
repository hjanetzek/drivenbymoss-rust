# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: tests of the step grid on zynseq patterns
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

"""The cursor clip and its step grids on the zynseq pattern of the selected sequence
(dbm_zynthian/clip.py, doc/ZYNTHIAN_PLAN.md §3.5): the adapter without the core, then the
Maschine Mk2's STEP mode with the core."""

import tempfile
import unittest

import zynstubs

zynstubs.install()

from dbm_host import Banks  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.bridge import ZynBridge  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.commands import ZynCommandHandler  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.model import ZynthianModel  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.reporter import ZynReporter  # noqa: E402
from zyngine.ctrldev.zynthian_ctrldev_dbm_maschine_mk2 import zynthian_ctrldev_dbm_maschine_mk2 as mk2  # noqa: E402

GRID = "16x128"
NEW_STEP = "0.0 1.0 0.0 0.0 0.0 0.0 0.0 1 1.0 1 0 1 1 1 1 0 0.0 0.0 0.0"
"""The fields after state, selected, muted, duration, velocity of a new note (release velocity 1.0,
chance on 1.0, occurrence, recurrence 1 1, repeat on 0, …: those of a new Bitwig note)."""


def project():
    """test_model's project with zynseq: the Synth's sequence of phrase 1 has a pattern of 4
    beats at 4 steps per beat (zynthian's default)."""
    state = zynstubs.project()
    state.zynseq = zynstubs.FakeZynseq()
    libseq = state.zynseq.libseq
    state.pattern = libseq.createPattern()
    libseq.addPattern(0, 0, 0, 0, 0, state.pattern, False)
    return state


class Host:

    def __init__(self, state=None):
        self.state = state or project()
        self.libseq = self.state.zynseq.libseq
        self.model = ZynthianModel(self.state.chain_manager, palette=["#0000FF"], zynseq=self.state.zynseq)
        self.banks = Banks.from_model_setup({"numTracks": 8, "noteClips": [[16, 128], [0, 0]]})
        self.reporter = ZynReporter(self.model, self.banks)
        self.commands = ZynCommandHandler(self.model, self.banks, self.reporter)
        self.commands.configure()
        self.reporter.features = self.commands.host_features

    def report(self):
        return dict(line.split(" ", 1) for line in self.reporter.collect().splitlines())

    def command(self, command, value=None):
        self.commands.handle("clip", command, value)

    def events(self):
        return [(e["position"], round(e["offset"], 4), e["val1_start"], e["val2_start"], round(e["duration"], 4))
                for e in self.libseq.patterns[self.state.pattern]["events"]]


class ClipTest(unittest.TestCase):

    def test_clip_properties(self):
        report = Host().report()
        self.assertEqual(report["/clip/exists"], "1")
        self.assertEqual((report["/clip/start"], report["/clip/end"]), ("0.000000", "4.000000"))
        self.assertEqual((report["/clip/loop"], report["/clip/loopStart"], report["/clip/loopLength"]), ("1", "0.000000", "4.000000"))
        self.assertEqual(report["/clip/color"], "0 0 255", "The Synth's color")
        self.assertEqual(report[f"/clip/grid/{GRID}/playingStep"], "-1")

    def test_no_clip_without_a_midi_chain(self):
        host = Host()
        host.state.chain_manager.set_active_chain_by_id(3)  # Audio In
        self.assertEqual(host.report()["/clip/exists"], "0")

    def test_steps_from_zynseq(self):
        """A note of the pattern editor: step 4 (beat 1), 2 steps long (half a beat), velocity
        100: at the core's 1/16 the grid steps 4 (start) and 5 (sustain)."""
        host = Host()
        host.libseq.add_test_note(host.state.pattern, 4, 60, velocity=100, duration=2.0)
        report = host.report()
        velocity = repr(100 / 127)
        self.assertEqual(report[f"/clip/grid/{GRID}/step/0/4/60"], f"1 0 0 0.5 {velocity} {NEW_STEP}")
        self.assertEqual(report[f"/clip/grid/{GRID}/step/0/5/60"], f"2 0 0 0.5 {velocity} {NEW_STEP}")
        self.assertNotIn(f"/clip/grid/{GRID}/step/0/6/60", report)
        # The note removed in zynseq: its steps are reported empty once
        host.libseq.selectPattern(host.state.pattern)
        host.libseq.removeNote(4, 60)
        report = host.report()
        self.assertEqual((report[f"/clip/grid/{GRID}/step/0/4/60"], report[f"/clip/grid/{GRID}/step/0/5/60"]), ("0", "0"))
        self.assertEqual(host.report(), {})

    def test_toggle_and_resolutions(self):
        host = Host()
        host.command(f"grid/{GRID}/step/0/2/60/toggle", 127)
        self.assertEqual(host.events(), [(2, 0.0, 60, 127, 1.0)], "1/16: zynseq step 2, one step long")
        host.command(f"grid/{GRID}/step/0/2/60/toggle", 127)
        self.assertEqual(host.events(), [])
        # 1/3 beat (triplets) on a pattern of 4 steps per beat: position + offset
        host.command(f"grid/{GRID}/stepSize", 1.0 / 3.0)
        host.command(f"grid/{GRID}/step/0/1/62/toggle", 64)
        self.assertEqual(host.events(), [(1, 0.3333, 62, 64, 1.3333)])
        report = host.report()
        self.assertTrue(report[f"/clip/grid/{GRID}/step/0/1/62"].startswith("1 1 0 0.333"), "Read back at the same core step")
        # 1/2 beat: an even multiple
        host.command(f"grid/{GRID}/stepSize", 0.5)
        host.command(f"grid/{GRID}/step/0/3/64/toggle", 100)
        self.assertIn((6, 0.0, 64, 100, 2.0), host.events())
        self.assertEqual(host.commands.unhandled, [])

    def test_finer_than_zynseq_one_note_per_key_and_step(self):
        """With 1/32 on a 1/16 pattern two core steps share a zynseq step: zynseq keeps one note
        per key and step, the second replaces the first (clip.py)."""
        host = Host()
        host.command(f"grid/{GRID}/stepSize", 0.125)
        host.command(f"grid/{GRID}/step/0/0/60/toggle", 100)
        host.command(f"grid/{GRID}/step/0/1/60/toggle", 100)
        self.assertEqual(host.events(), [(0, 0.5, 60, 100, 0.5)])
        host.command(f"grid/{GRID}/step/0/2/61/toggle", 100)
        host.command(f"grid/{GRID}/step/0/3/62/toggle", 100)
        self.assertEqual(len(host.events()), 3, "Other keys in the same zynseq step are kept")

    def test_attributes(self):
        host = Host()
        host.command(f"grid/{GRID}/step/0/0/60/toggle", 127)
        step = f"grid/{GRID}/step/0/0/60/"
        host.command(step + "velocity", 0.5)
        host.command(step + "duration", 0.75)
        host.command(step + "chance", 0.25)
        host.command(step + "muted", 1)
        event = host.libseq.patterns[host.state.pattern]["events"][0]
        self.assertEqual((event["val2_start"], event["duration"], event["play_chance"], event["play_freq"]), (64, 3.0, 0.25, 0))
        line = host.report()[f"/clip/grid/{GRID}/step/0/0/60"].split()
        self.assertEqual(line[2:5], ["1", "0.75", repr(64 / 127)])
        self.assertEqual(line[13], "0.25")
        # Attributes zynseq does not have: accepted, nothing changes, the core sees the default
        host.command(step + "pan", -1.0)
        host.command(step + "repeatCount", 4)
        self.assertEqual(host.report(), {})
        self.assertEqual(host.commands.unhandled, [])

    def test_set_update_move_clear(self):
        host = Host()
        host.command(f"grid/{GRID}/step/0/1/60/set", "90 0.5")
        self.assertEqual(host.events(), [(1, 0.0, 60, 90, 2.0)])
        values = "1 1 0 0.25 0.5 0.0 1.0 0.0 0.0 0.0 0.0 0.0 1 0.5 1 0 1 1 1 1 0 0.0 0.0 0.0".split()
        host.command(f"grid/{GRID}/step/0/1/60/update", " ".join(values))
        event = host.libseq.patterns[host.state.pattern]["events"][0]
        self.assertEqual((event["val2_start"], event["duration"], event["play_chance"]), (64, 1.0, 0.5))
        host.command(f"grid/{GRID}/step/0/1/60/moveY", 67)
        self.assertEqual([e[2] for e in host.events()], [67])
        host.command(f"grid/{GRID}/step/0/5/67/toggle", 100)
        host.command(f"grid/{GRID}/clearRow/0/67")
        self.assertEqual(host.events(), [])
        host.command(f"grid/{GRID}/step/0/5/60/toggle", 100)
        host.command(f"grid/{GRID}/step/0/5/62/toggle", 100)
        host.command(f"grid/{GRID}/clearColumn/0/5")
        self.assertEqual(host.events(), [])
        host.command(f"grid/{GRID}/step/0/5/60/toggle", 100)
        host.command(f"grid/{GRID}/clearAll")
        self.assertEqual(host.events(), [])
        self.assertEqual(host.commands.unhandled, [])

    def test_length_duplicate_transpose_clear(self):
        host = Host()
        host.command(f"grid/{GRID}/step/0/1/60/toggle", 100)
        host.command("loopLength", 8.0)
        self.assertEqual(host.libseq.patterns[host.state.pattern]["beats"], 8)
        host.command("loopLength/inc", -6.0)
        self.assertEqual(host.libseq.patterns[host.state.pattern]["beats"], 2)
        self.assertEqual(host.report()["/clip/loopLength"], "2.000000")
        host.command("duplicateContent")
        self.assertEqual(host.libseq.patterns[host.state.pattern]["beats"], 4)
        self.assertEqual([e[0] for e in host.events()], [1, 9], "The notes again after the old length")
        host.command("transpose", 2)
        self.assertEqual({e[2] for e in host.events()}, {62})
        host.command("clear")
        self.assertEqual(host.events(), [])
        host.command("start", 0.0)
        host.command("loop", 1)
        self.assertEqual(host.commands.unhandled, [])
        host.command("start", 1.0)
        host.command("duplicate")
        self.assertEqual(host.commands.unhandled, ["clip start", "clip duplicate"])

    def test_playing_step(self):
        host = Host()
        libseq = host.libseq
        libseq.play_states[(0, 0, 0)] = 1
        libseq.playheads[(0, 0, 0)] = 6  # zynseq step 6 = beat 1.5
        self.assertEqual(host.report()[f"/clip/grid/{GRID}/playingStep"], "6")
        host.command(f"grid/{GRID}/stepSize", 0.5)
        self.assertEqual(host.report()[f"/clip/grid/{GRID}/playingStep"], "3")
        host.command(f"grid/{GRID}/scroll", 2)
        self.assertEqual(host.report()[f"/clip/grid/{GRID}/playingStep"], "1", "Relative to the scroll position")

    def test_the_pattern_editors_selection_is_kept(self):
        """zynseq's functions work on the selected pattern, which the pattern editor uses: the
        adapter selects the clip's pattern for each access and the previous one again."""
        host = Host()
        other = host.libseq.createPattern()
        host.libseq.selectPattern(other)
        host.command(f"grid/{GRID}/step/0/1/60/toggle", 100)
        host.report()
        self.assertEqual(host.libseq.selected_pattern, other)
        self.assertEqual(host.events(), [(1, 0.0, 60, 100, 1.0)])
        self.assertTrue(host.libseq.modified, "zynseq marks the pattern modified: the pattern editor redraws")

    def test_selection_of_notes(self):
        host = Host()
        host.command(f"grid/{GRID}/step/0/1/60/toggle", 100)
        host.command(f"grid/{GRID}/step/0/2/62/toggle", 100)
        report = host.report()
        self.assertEqual(report[f"/clip/grid/{GRID}/step/0/2/62"].split()[1], "1", "The new note is selected")
        self.assertEqual(report[f"/clip/grid/{GRID}/step/0/1/60"].split()[1], "0")

    def test_zynseq_state_refreshed(self):
        host = Host()
        before = host.state.zynseq.refreshes
        host.command(f"grid/{GRID}/step/0/1/60/toggle", 100)
        host.model.launcher.last_refresh = 0.0
        host.report()
        self.assertGreater(host.state.zynseq.refreshes, before, "zynseq's Python state (the launcher GUI) after a change")


class MaschineMk2StepTest(unittest.TestCase):
    """The Maschine Mk2's STEP mode (the drum sequencer) with the core on a zynseq pattern."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="dbm-zynthian-mk2-step-")
        self.state = project()
        self.libseq = self.state.zynseq.libseq
        self.model = ZynthianModel(self.state.chain_manager, zynseq=self.state.zynseq)
        self.midi_out = []
        self.bridge = ZynBridge(self.model, mk2.dbm_controller_id, self.temp.name, "zynthian", midi_sink=self.midi_out.append)
        self.bridge.start()
        self.bridge.wait_for_startup()
        self.bridge.advance(100)

    def tearDown(self):
        self.bridge.close()
        self.temp.cleanup()

    def led(self, note):
        for data in reversed(self.midi_out):
            if len(data) == 3 and data[0] == 0x90 and data[1] == note:
                return data[2]
        return None

    def events(self):
        return [(e["position"], e["val1_start"]) for e in self.libseq.patterns[self.state.pattern]["events"]]

    def test_steps_both_ways(self):
        # PAD MODE (drum mode), a pad selects the drum cell (C1, 36), STEP: the drum sequencer
        self.bridge.send([0xB0, 0x51, 127])
        self.bridge.send([0xB0, 0x51, 0], 600)
        self.bridge.send([0x90, 0x24, 100])
        self.bridge.send([0x90, 0x24, 0], 400)
        self.bridge.send([0xB0, 0x53, 127])
        self.bridge.send([0xB0, 0x53, 0], 600)
        off = self.led(0x30)
        # Pad 13 (0x30) = step 13 (index 12) of the drum cell
        self.bridge.send([0x90, 0x30, 127])
        self.bridge.send([0x90, 0x30, 0], 400)
        self.assertEqual(self.events(), [(12, 36)], "A note in the zynseq pattern: step 12 of 1/16 = zynseq step 12")
        on = self.led(0x30)
        self.assertNotEqual(on, off, "The pad of the step lights")
        # A note added in zynseq (the pattern editor) at step 13 lights pad 14
        self.assertEqual(self.led(0x31), off)
        self.libseq.add_test_note(self.state.pattern, 13, 36)
        self.bridge.advance(200)
        self.assertEqual(self.led(0x31), on)
        # The playhead while zynseq plays (zynseq step 15: pad 16)
        before = self.led(0x33)
        self.libseq.play_states[(0, 0, 0)] = 1
        self.libseq.playheads[(0, 0, 0)] = 15
        self.bridge.advance(200)
        self.assertEqual(self.bridge.reporter.last_values[f"/clip/grid/{GRID}/playingStep"], "15")
        self.assertNotEqual(self.led(0x33), before, "Pad 16 shows the playing step")
        # Pad 13 again: the step off
        self.bridge.send([0x90, 0x30, 127])
        self.bridge.send([0x90, 0x30, 0], 400)
        self.assertEqual(self.events(), [(13, 36)])
        self.assertEqual(list(self.bridge.core_errors()), [])
        self.assertFalse([k for k in self.bridge.commands.unhandled if k.startswith("clip")], self.bridge.commands.unhandled)


if __name__ == "__main__":
    unittest.main()
