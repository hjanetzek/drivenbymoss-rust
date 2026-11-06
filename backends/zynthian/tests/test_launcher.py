# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: tests of the clip launcher on zynseq
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

"""The clip launcher on zynseq (dbm_zynthian/launcher.py, doc/ZYNTHIAN_PLAN.md §3.2, §3.3): the
slots' play states, launch and stop, clip remove/duplicate/paste, the scene commands; the
adapter without the core, then the Maschine Mk2's PATTERN and SCENE modes with the core."""

import tempfile
import unittest

import zynstubs

zynstubs.install()

from dbm_host import Banks  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.bridge import ZynBridge  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.commands import ZynCommandHandler  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.launcher import (CHILD_PLAYING, CHILD_STOPPING, FORCED_STOP, PHRASE_CHANNEL, PLAYING,  # noqa: E402
                                                   STARTING, STOPPING, STOPPING_SYNC, slot_states)
from zyngine.ctrldev.dbm_zynthian.model import ZynthianModel  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.reporter import ZynReporter  # noqa: E402
from zyngine.ctrldev.zynthian_ctrldev_dbm_maschine_mk2 import zynthian_ctrldev_dbm_maschine_mk2 as mk2  # noqa: E402

STOPPED = 0


def project():
    """zynstubs' project with zynseq (4 phrases): Synth (channel 1) has a pattern with a note in
    phrase 1, Piano (channel 2) an empty one in phrase 1."""
    state = zynstubs.project()
    state.zynseq = zynstubs.FakeZynseq()
    libseq = state.zynseq.libseq
    for chan in (0, 1):
        pattern = libseq.createPattern()
        libseq.addPattern(0, 0, chan, 0, 0, pattern, False)
    libseq.add_test_note(libseq.getPattern(0, 0, 0, 0, 0), 0, 60)
    return state


class Host:

    def __init__(self):
        self.state = project()
        self.zynseq = self.state.zynseq
        self.libseq = self.zynseq.libseq
        self.model = ZynthianModel(self.state.chain_manager, zynseq=self.zynseq)
        self.banks = Banks.from_model_setup({"numTracks": 8, "numScenes": 8})
        self.reporter = ZynReporter(self.model, self.banks)
        self.commands = ZynCommandHandler(self.model, self.banks, self.reporter)
        self.commands.configure()
        self.reporter.features = self.commands.host_features
        self.values = {}

    def report(self):
        """All values reported so far (the reporter reports changes)."""
        self.values.update(line.split(" ", 1) for line in self.reporter.collect().splitlines())
        return self.values

    def command(self, processor, command, value=None):
        self.commands.handle(processor, command, value)

    def state_of(self, phrase, chan):
        return self.libseq.getPlayState(0, phrase, chan)

    def set_state(self, phrase, chan, state):
        self.libseq.play_states[(0, phrase, chan)] = state

    def slot(self, report, track, slot):
        return tuple(report[f"/track/{track}/slot/{slot}/{field}"] for field in ("playing", "playQueued", "stopQueued"))

    def notes(self, phrase, chan):
        pattern = self.libseq.getPattern(0, phrase, chan, 0, 0)
        return [(e["position"], e["val1_start"]) for e in self.libseq.patterns[pattern]["events"]] if pattern != self.libseq.NO_PATTERN else None


class PlayStateTest(unittest.TestCase):
    """zynseq's play states as the slot's playing, playQueued, stopQueued (Bitwig: a slot with a
    queued stop still plays)."""

    def test_states(self):
        expected = {STOPPED: (False, False, False), PLAYING: (True, False, False), STARTING: (False, True, False),
                    STOPPING: (True, False, True), FORCED_STOP: (False, False, False), STOPPING_SYNC: (True, False, True),
                    CHILD_PLAYING: (True, False, False), CHILD_STOPPING: (True, False, True)}
        for state, states in expected.items():
            self.assertEqual(slot_states(state), states, f"state {state}")

    def test_reports(self):
        host = Host()
        for state, reported in ((STOPPED, ("0", "0", "0")), (STARTING, ("0", "1", "0")), (PLAYING, ("1", "0", "0")),
                                (STOPPING_SYNC, ("1", "0", "1")), (STOPPING, ("1", "0", "1")), (FORCED_STOP, ("0", "0", "0"))):
            host.set_state(0, 0, state)
            self.assertEqual(host.slot(host.report(), 0, 0), reported, f"state {state}")
        host.libseq.midi_record = True
        host.libseq.selected_sequence = (0, 0, 0)
        self.assertEqual(host.report()["/track/0/slot/0/recording"], "1", "MIDI record into the selected sequence")
        self.assertEqual(host.report()["/track/0/slot/1/recording"], "0")


class LaunchTest(unittest.TestCase):
    """zynseq's launch is a toggle; the core's launch starts and never stops (Bitwig's relaunch
    of a playing clip keeps it playing)."""

    def test_launch_starts_once(self):
        host = Host()
        host.command("track", "0/clip/0/launch")
        self.assertEqual(host.state_of(0, 0), STARTING)
        host.command("track", "0/clip/0/launch")
        self.assertEqual(host.state_of(0, 0), STARTING, "A second launch does not stop the starting sequence")
        host.set_state(0, 0, PLAYING)
        host.command("track", "0/clip/0/launchAlt")
        host.command("track", "0/clip/0/launchRelease")
        self.assertEqual(host.state_of(0, 0), PLAYING, "Nor a playing one")
        host.set_state(0, 0, STOPPING_SYNC)
        host.command("track", "0/clip/0/launch")
        self.assertEqual(host.state_of(0, 0), PLAYING, "A queued stop is taken back")
        host.set_state(0, 0, FORCED_STOP)
        host.command("track", "0/clip/0/launch")
        self.assertEqual(host.state_of(0, 0), STARTING)
        self.assertEqual(host.commands.unhandled, [])

    def test_launch_an_empty_slot_starts_it(self):
        """Every zynseq slot is a sequence: an empty one starts too (the core launches right
        after createClip and record)."""
        host = Host()
        host.command("track", "1/clip/2/launch")
        self.assertEqual(host.state_of(2, 1), STARTING)

    def test_record_starts(self):
        host = Host()
        host.set_state(1, 0, STOPPING_SYNC)
        host.command("track", "0/clip/1/record")
        self.assertEqual(host.state_of(1, 0), PLAYING)
        self.assertTrue(host.libseq.midi_record)


class StopTest(unittest.TestCase):

    def test_track_stop(self):
        host = Host()
        host.set_state(0, 0, PLAYING)
        host.set_state(1, 0, STARTING)
        host.set_state(0, 1, PLAYING)
        host.command("track", "0/stop")
        self.assertEqual((host.state_of(0, 0), host.state_of(1, 0)), (STOPPING_SYNC, STOPPED),
                         "Playing: at the next sync point (as zynseq's toggle); starting: at once")
        self.assertEqual(host.state_of(0, 1), PLAYING, "Only the track's sequences")
        self.assertEqual(host.slot(host.report(), 0, 0), ("1", "0", "1"))
        host.command("track", "0/stopAlt")
        self.assertEqual(host.state_of(0, 0), STOPPED, "stopAlt: at once")
        host.command("track", "2/stop")
        self.assertEqual(host.commands.unhandled, ["track <n>/stop"], "Audio In has no sequences")

    def test_stop_all(self):
        host = Host()
        host.set_state(0, 0, PLAYING)
        host.set_state(2, 1, STARTING)
        host.set_state(1, 17, PLAYING)  # an audio launcher (clippy)
        host.set_state(0, PHRASE_CHANNEL, PLAYING)
        host.set_state(3, PHRASE_CHANNEL, CHILD_PLAYING)
        host.command("scene", "stop")
        self.assertEqual(host.state_of(0, PHRASE_CHANNEL), CHILD_STOPPING, "The phrase launcher stops with its sequences")
        self.assertEqual(host.state_of(3, PHRASE_CHANNEL), CHILD_STOPPING)
        self.assertEqual(host.state_of(0, 0), STOPPING, "Stopped through its phrase launcher (zynseq: at its end)")
        self.assertEqual((host.state_of(2, 1), host.state_of(1, 17)), (STOPPED, STOPPING_SYNC))
        host.command("scene", "stopAlt")
        self.assertEqual({state for state in host.libseq.play_states.values()}, {STOPPED})
        self.assertEqual(host.commands.unhandled, [])


class ClipTest(unittest.TestCase):

    def test_remove(self):
        host = Host()
        host.set_state(0, 0, PLAYING)
        host.command("track", "0/clip/0/remove")
        self.assertEqual(host.state_of(0, 0), STOPPED, "Stopped at once")
        self.assertEqual(host.notes(0, 0), [], "The pattern is cleared")
        self.assertEqual(host.report()["/track/0/slot/0/hasContent"], "0")
        host.command("track", "0/clip/3/remove")
        self.assertEqual(host.commands.unhandled, [], "A slot without a pattern: nothing to clear")

    def test_duplicate(self):
        host = Host()
        host.command("track", "0/clip/0/duplicate")
        self.assertEqual(host.notes(1, 0), [(0, 60)], "A copy in the slot below")
        self.assertNotEqual(host.libseq.getPattern(0, 1, 0, 0, 0), host.libseq.getPattern(0, 0, 0, 0, 0), "Its own pattern")
        host.command("track", "0/clip/3/duplicate")
        host.command("track", "1/clip/0/duplicate")
        self.assertIsNone(host.notes(1, 1), "Nothing for an empty slot and the last one")
        self.assertEqual(host.commands.unhandled, [])

    def test_paste(self):
        host = Host()
        host.command("track", "1/clip/2/paste", "0/0")
        self.assertEqual(host.notes(2, 1), [(0, 60)], "Synth's slot 1 into Piano's slot 3")
        host.command("track", "1/clip/2/paste", "2/0")
        self.assertEqual(host.commands.unhandled, ["track <n>/clip/<n>/paste"], "Audio In has no slots")

    def test_duplicate_the_cursor_clip(self):
        host = Host()
        host.command("clip", "duplicate")
        self.assertEqual(host.notes(1, 0), [(0, 60)], "The cursor slot (Synth, phrase 1) into the slot below")
        self.assertEqual(host.commands.unhandled, [])


class SceneTest(unittest.TestCase):

    def test_launch(self):
        host = Host()
        host.command("scene", "1/launch")
        self.assertEqual((host.zynseq.phrase, host.state_of(1, PHRASE_CHANNEL)), (1, STARTING))
        host.set_state(1, PHRASE_CHANNEL, PLAYING)
        host.command("scene", "1/launchAlt")
        host.command("scene", "1/launchRelease")
        self.assertEqual(host.state_of(1, PHRASE_CHANNEL), PLAYING, "A playing phrase stays")
        host.set_state(1, PHRASE_CHANNEL, STOPPING_SYNC)
        host.command("scene", "1/launch")
        self.assertEqual(host.state_of(1, PHRASE_CHANNEL), PLAYING, "A queued stop is taken back")
        host.set_state(2, PHRASE_CHANNEL, CHILD_PLAYING)
        host.command("scene", "2/launch")
        self.assertEqual(host.state_of(2, PHRASE_CHANNEL), STARTING, "Only some of its sequences play: the phrase starts")
        self.assertEqual(host.commands.unhandled, [])

    def test_create_remove_duplicate(self):
        host = Host()
        host.command("project", "createScene", 4)
        self.assertEqual((host.zynseq.phrases, host.zynseq.phrase), (5, 4), "A new phrase at the end, selected")
        self.assertEqual(host.report()["/scene/count"], "5")
        host.command("scene", "0/duplicate")
        self.assertEqual(host.zynseq.phrases, 6)
        self.assertEqual(host.report()["/scene/5/exists"], "1")
        self.assertEqual((host.notes(0, 0), host.notes(1, 0)), ([(0, 60)], [(0, 60)]), "zynseq's copy is inserted before the phrase")
        host.command("scene", "1/remove")
        self.assertEqual(host.zynseq.phrases, 5)
        self.assertEqual(host.zynseq.calls, [("insert_phrase", 4), ("duplicate_phrase", 0), ("insert_phrase", 0), ("remove_phrase", 1)])
        report = host.report()
        self.assertEqual(report["/scene/count"], "5")
        self.assertEqual(report["/scene/5/exists"], "0", "The removed scene")
        host.command("scene", "9/remove")
        self.assertEqual(host.commands.unhandled, ["scene <n>/remove"], "No such phrase")

    def test_create_scene_from_playing_clips(self):
        host = Host()
        host.command("track", "0/clip/0/duplicate")
        host.libseq.add_test_note(host.libseq.getPattern(0, 1, 0, 0, 0), 2, 64)
        host.set_state(1, 0, PLAYING)
        host.command("project", "createSceneFromPlayingLauncherClips")
        self.assertEqual((host.zynseq.phrases, host.zynseq.phrase), (5, 4))
        self.assertEqual(host.notes(4, 0), [(0, 60), (2, 64)], "The playing pattern of Synth")
        self.assertIsNone(host.notes(4, 1), "Piano plays nothing")
        self.assertEqual(host.commands.unhandled, [])


class MaschineMk2PatternTest(unittest.TestCase):
    """The Maschine Mk2's PATTERN mode (the clip view of the selected track) and SCENE mode with
    the core: pad = launch, PATTERN + pad = stop the track, ERASE + pad = remove, DUPLICATE + pad =
    duplicate; SCENE + pad = stop all, ERASE/DUPLICATE + pad on a scene."""

    PATTERN, SCENE, ERASE, DUPLICATE = 86, 85, 54, 89

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="dbm-zynthian-mk2-pattern-")
        self.state = project()
        self.zynseq = self.state.zynseq
        self.libseq = self.zynseq.libseq
        self.model = ZynthianModel(self.state.chain_manager, zynseq=self.zynseq)
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

    def button(self, cc, wait=600):
        self.bridge.send([0xB0, cc, 127])
        self.bridge.send([0xB0, cc, 0], wait)

    def pad(self, note, held=None):
        if held is not None:
            self.bridge.send([0xB0, held, 127])
        self.bridge.send([0x90, note, 127])
        self.bridge.send([0x90, note, 0], 200)
        if held is not None:
            self.bridge.send([0xB0, held, 0], 400)

    def state_of(self, phrase, chan=0):
        return self.libseq.getPlayState(0, phrase, chan)

    def test_pattern_mode(self):
        self.button(self.PATTERN)
        stopped = self.led(0x24)
        # Pad 1: Synth's slot 1 starts; again: it keeps starting (zynseq's toggle would stop it)
        self.pad(0x24)
        self.assertEqual(self.state_of(0), STARTING)
        starting = self.led(0x24)
        self.assertNotEqual(starting, stopped, "Pad 1 shows the queued start")
        self.pad(0x24)
        self.assertEqual(self.state_of(0), STARTING)
        self.libseq.play_states[(0, 0, 0)] = PLAYING
        self.bridge.advance(200)
        playing = self.led(0x24)
        self.assertNotIn(playing, (stopped, starting), "Pad 1 shows the playing slot")
        # PATTERN + pad: the track stops at the next sync point, the pad still plays
        self.pad(0x24, held=self.PATTERN)
        self.assertEqual(self.state_of(0), STOPPING_SYNC)
        self.assertEqual(self.bridge.reporter.last_values["/track/0/slot/0/stopQueued"], "1")
        # The pad again takes the stop back
        self.pad(0x24)
        self.assertEqual(self.state_of(0), PLAYING)
        # DUPLICATE + pad 1: a copy into slot 2
        self.pad(0x24, held=self.DUPLICATE)
        pattern = self.libseq.getPattern(0, 1, 0, 0, 0)
        self.assertEqual([e["val1_start"] for e in self.libseq.patterns[pattern]["events"]], [60])
        self.assertEqual(self.bridge.reporter.last_values["/track/0/slot/1/hasContent"], "1")
        # ERASE + pad 1: stopped and cleared
        self.pad(0x24, held=self.ERASE)
        self.assertEqual(self.state_of(0), STOPPED)
        self.assertTrue(self.libseq.isPatternEmpty(self.libseq.getPattern(0, 0, 0, 0, 0)))
        self.bridge.advance(200)
        self.assertEqual(self.bridge.reporter.last_values["/track/0/slot/0/hasContent"], "0")
        self.assertEqual(list(self.bridge.core_errors()), [])
        self.assertFalse([k for k in self.bridge.commands.unhandled if k.split(" ")[0] in ("track", "scene", "clip", "project")],
                         self.bridge.commands.unhandled)

    def test_scene_mode(self):
        self.button(self.SCENE)
        self.libseq.play_states[(0, 0, 0)] = PLAYING
        self.libseq.play_states[(0, 2, 1)] = PLAYING
        # SCENE + pad: stop all clips
        self.pad(0x24, held=self.SCENE)
        self.assertEqual((self.state_of(0), self.state_of(2, 1)), (STOPPING_SYNC, STOPPING_SYNC))
        # Pad 2: phrase 2 starts
        self.pad(0x25)
        self.assertEqual((self.zynseq.phrase, self.state_of(1, PHRASE_CHANNEL)), (1, STARTING))
        # DUPLICATE + pad 1, ERASE + pad 5 (the new last phrase)
        self.pad(0x24, held=self.DUPLICATE)
        self.assertEqual(self.zynseq.phrases, 5)
        self.pad(0x28, held=self.ERASE)
        self.assertEqual(self.zynseq.phrases, 4)
        self.assertEqual([c[0] for c in self.zynseq.calls if c[0] != "insert_phrase"], ["duplicate_phrase", "remove_phrase"])
        self.assertEqual(list(self.bridge.core_errors()), [])
        self.assertFalse([k for k in self.bridge.commands.unhandled if k.split(" ")[0] in ("track", "scene", "clip", "project")],
                         self.bridge.commands.unhandled)


if __name__ == "__main__":
    unittest.main()
