# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: the Maschine JAM driver
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

"""The Maschine JAM driver (doc/ZYNTHIAN_PLAN.md §7c): its dev_id against dbm-nihost's port
names under PipeWire and the other DrivenByMoss drivers (it takes the JAM's port only, the Mk2's
driver not the JAM's), and the JAM with the core against the adapter on simulated time: the touch
strips (the levels they show, the faders they move), the session grid on zynseq's launcher, the
scene buttons, the play mode into the active chain, the step sequencer on the pattern. The
device side is the MIDI dbm-nihost sends (the JAM's MIDI mode: buttons are CCs on channel 1,
pads notes 36..99 from the lower left, Shift a sysex) and the MIDI the core sends."""

import importlib
import importlib.util
import re
import tempfile
import unittest

import zynstubs

zynstubs.install()

import drivenbymoss  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.bridge import ZynBridge  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.launcher import PLAYING, STARTING  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.model import ZynthianModel  # noqa: E402
from zyngine.ctrldev.zynthian_ctrldev_base_dbm import launcher_palette  # noqa: E402
from zyngine.ctrldev.zynthian_ctrldev_dbm_maschine_jam import zynthian_ctrldev_dbm_maschine_jam as jam  # noqa: E402
from zyngine.ctrldev.zynthian_ctrldev_dbm_maschine_mk2 import zynthian_ctrldev_dbm_maschine_mk2 as mk2  # noqa: E402

PORTS = zynstubs.UI_DIR / "zynautoconnect" / "zynthian_pipewire_ports.py"
# The MIDI bridge ports of this host (pw-link -o / -i, 2026-11-07): dbm-nihost's two clients (the
# Mk2 and the JAM), the Mk2's own USB MIDI port and the Launch Control XL
CAPTURE = [
    "Midi-Bridge:Midi Through: Port-0 (capture)",
    "Midi-Bridge:Maschine MK2: In (capture)",
    "Midi-Bridge:Maschine Jam - 1: Input (capture)",
    "Midi-Bridge:Maschine Controller MK2: MIDI 1 (capture)",
    "Midi-Bridge:Launch Control XL: Launch Contro (capture)",
    "Midi-Bridge:Launch Control XL: HUI (capture)",
]
PLAYBACK = [name.replace("(capture)", "(playback)").replace(": In ", ": Out ").replace(": Input ", ": Output ") for name in CAPTURE]

# The JAM in MIDI mode (src/controller/maschine/jam/surface.rs)
FADER1, FADER_TOUCH1 = 8, 20
SONG, STEP, PAD_MODE = 30, 31, 32
LEVEL, AUX, CONTROL = 37, 38, 39
SCENE1, GROUP_A = 80, 70
PLAY, MUTE = 90, 97
STRIP_SETUP = bytes.fromhex("F000210915004D50000105")
TYPE_SINGLE, TYPE_PAN, TYPE_DUAL = 0, 2, 3


def dbm_drivers():
    """The DrivenByMoss driver classes of this checkout."""
    classes = []
    for path in sorted(zynstubs.CTRLDEV_DIR.glob("zynthian_ctrldev_dbm_*.py")):
        name = path.stem
        classes.append(getattr(importlib.import_module(f"zyngine.ctrldev.{name}"), name))
    return classes


@unittest.skipUnless(PORTS.exists(), "zynthian-ui without the PipeWire port names")
class MatchingTest(unittest.TestCase):

    def ports(self):
        spec = importlib.util.spec_from_file_location("zynthian_pipewire_ports", PORTS)
        ports = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(ports)
        return ports

    def test_the_core_runs_the_jam(self):
        controller = next(c for c in drivenbymoss.list_controllers() if c["uuid"] == jam.dbm_controller_id)
        self.assertEqual(controller["model"], "Maschine JAM")
        self.assertEqual(jam.get_driver_name(), "DrivenByMoss Maschine JAM")
        self.assertTrue(jam.get_autoload_flag(), "zynthian has no JAM driver")
        self.assertTrue(jam.unroute_from_chains)
        # dbm-nihost's ALSA client "Maschine Jam - 1" with its ports "… Input" / "… Output": the
        # core's Linux names, the PipeWire bridge's too
        inputs = [name for pair in controller["discovery"]["linux"] for name in pair["inputs"]]
        self.assertIn("Maschine Jam - 1 Input", inputs)
        self.assertIn("Midi-Bridge:Maschine Jam - 1: Input (capture)", inputs)

    def test_dev_id_of_dbm_nihost_port(self):
        ports = self.ports()
        dev_ids = {name: ports.pipewire_midi_port_name(name, CAPTURE)[0].split("/", 1)[1] for name in CAPTURE}
        self.assertEqual(dev_ids["Midi-Bridge:Maschine Jam - 1: Input (capture)"], "Maschine Jam - 1 IN 1")
        self.assertEqual(jam.dev_ids, ["Maschine Jam - 1 IN 1"])
        out_uid, _ = ports.pipewire_midi_port_name("Midi-Bridge:Maschine Jam - 1: Output (playback)", PLAYBACK)
        self.assertEqual(ports.dev_out_uid("PW:Maschine Jam - 1/Maschine Jam - 1 IN 1"), out_uid, "dev_in_2_dev_out finds the output")

    def test_each_port_its_driver(self):
        """zynthian_ctrldev_manager: per dev_id the first class with the autoload flag (exact
        match): the JAM driver only on the JAM's port, the Mk2's not there."""
        ports = self.ports()
        available = {}
        for cls in dbm_drivers():
            for dev_id in cls.dev_ids:
                available.setdefault(dev_id, []).append(cls)

        def autoload(name):
            dev_id = ports.pipewire_midi_port_name(name, CAPTURE)[0].split("/", 1)[1]
            return next((cls for cls in available.get(dev_id, []) if cls.get_autoload_flag()), None)

        self.assertIs(autoload("Midi-Bridge:Maschine Jam - 1: Input (capture)"), jam)
        self.assertIs(autoload("Midi-Bridge:Maschine MK2: In (capture)"), mk2)
        for name in CAPTURE:
            if "Maschine Jam" not in name:
                self.assertIsNot(autoload(name), jam, name)
        self.assertEqual(len(available["Maschine Jam - 1 IN 1"]), 1, "No other driver on the JAM's port")
        self.assertNotIn("Maschine Jam - 1 IN 1", mk2.dev_ids)


def project():
    """zynstubs' project with zynseq (4 phrases): Synth (channel 1, the active chain) has a pattern
    with a note in phrase 1, Piano (channel 2) an empty one; a synth engine on Synth."""
    state = zynstubs.project()
    state.zynseq = zynstubs.FakeZynseq()
    state.synth = state.add_processor(1, "Synth A", [(f"screen {n}", [f"p{n}{i}" for i in range(4)]) for n in (1, 2)])
    libseq = state.zynseq.libseq
    for chan in (0, 1):
        libseq.addPattern(0, 0, chan, 0, 0, libseq.createPattern(), False)
    libseq.add_test_note(libseq.getPattern(0, 0, 0, 0, 0), 0, 60)
    return state


class Jam:
    """The JAM against the core and the adapter on simulated time, with zynseq's stand-in clock."""

    def __init__(self, test):
        self.temp = tempfile.TemporaryDirectory(prefix="dbm-zynthian-jam-")
        test.addCleanup(self.temp.cleanup)
        self.state = project()
        self.libseq = self.state.zynseq.libseq
        self.model = ZynthianModel(self.state.chain_manager, palette=launcher_palette(), zynseq=self.state.zynseq)
        self.midi_out = []
        self.bridge = ZynBridge(self.model, jam.dbm_controller_id, self.temp.name, "zynthian", midi_sink=self.midi_out.append)
        test.addCleanup(self.bridge.close)
        self.bridge.start()
        self.bridge.wait_for_startup()
        self.advance(500)

    def advance(self, ms=200):
        while ms > 0:
            self.libseq.clock(min(10, ms))
            self.bridge.advance(min(10, ms))
            ms -= 10

    def send(self, *data):
        self.bridge.receive(bytes(data))
        self.advance(30)

    def button(self, cc, wait=200):
        self.send(0xB0, cc, 127)
        self.send(0xB0, cc, 0)
        self.advance(wait)

    def pad(self, note, wait=200):
        self.send(0x90, note, 127)
        self.send(0x90, note, 0)
        self.advance(wait)

    def strip(self, index, value):
        """A touch strip: touch, value, release."""
        self.send(0xB0, FADER_TOUCH1 + index, 127)
        self.send(0xB0, FADER1 + index, value)
        self.send(0xB0, FADER_TOUCH1 + index, 0)
        self.advance(200)

    def last_cc(self, cc):
        return next((d[2] for d in reversed(self.midi_out) if len(d) == 3 and d[0] == 0xB0 and d[1] == cc), None)

    def last_note(self, note):
        return next((d[2] for d in reversed(self.midi_out) if len(d) == 3 and d[0] == 0x90 and d[1] == note), None)

    def strip_setup(self):
        """[(type, color)] of the 8 strips from the last setup sysex."""
        data = next((bytes(d) for d in reversed(self.midi_out) if bytes(d).startswith(STRIP_SETUP)), None)
        return None if data is None else [(data[11 + 2 * i], data[12 + 2 * i]) for i in range(8)]

    def errors(self):
        return list(self.bridge.core_errors())


class TouchStripTest(unittest.TestCase):
    """The touch strips show zynthian's mixer and move it (the JAM's level, aux and control modes)."""

    def setUp(self):
        self.jam = Jam(self)
        self.tracks = self.jam.model.tracks()

    def level(self, chain_id):
        return self.jam.state.zctrl(chain_id, "level").value

    def assert_shows(self, index, value):
        self.assertLessEqual(abs(self.jam.last_cc(FADER1 + index) - value * 127), 1, f"strip {index + 1}")

    def test_volume_strips_show_and_move_the_faders(self):
        jam = self.jam
        self.jam.state.zctrl(2, "level").set_value(0.5)
        jam.advance(300)
        # The volume mode is the startup mode: one strip per chain in its color, the level of its fader
        setup = jam.strip_setup()
        self.assertEqual([kind for kind, _ in setup[:3]], [TYPE_SINGLE] * 3)
        self.assertNotEqual(setup[0][1], 0, "In the chain's color")
        self.assert_shows(0, 0.8)
        self.assert_shows(1, 0.5)
        # A strip moves the fader
        jam.strip(0, 32)
        self.assertAlmostEqual(self.level(1), 32 / 127, places=2)
        # zynthian's fader moves the strip
        self.jam.state.zctrl(1, "level").set_value(0.25)
        jam.advance(300)
        self.assert_shows(0, 0.25)
        self.assertEqual(jam.errors(), [])

    def test_volume_strips_show_the_meters_while_playing(self):
        jam = self.jam
        jam.button(PLAY)
        self.assertTrue(jam.model.transport.playing)
        self.assertEqual([kind for kind, _ in jam.strip_setup()[:3]], [TYPE_DUAL] * 3, "VU and fader while playing")

    def test_pan_and_sends(self):
        jam = self.jam
        jam.button(LEVEL)  # the second press of LEVEL: panorama
        self.assertEqual([kind for kind, _ in jam.strip_setup()[:3]], [TYPE_PAN] * 3)
        self.assert_shows(0, 0.5)
        jam.strip(1, 100)
        self.assertAlmostEqual(self.tracks[1].pan.value, 100 / 127, places=2)
        jam.button(AUX)  # the first send: the mixbus Reverb
        jam.strip(0, 90)
        self.assertAlmostEqual(self.tracks[0].sends[0].volume.value, 90 / 127, places=2)
        self.assert_shows(0, 90 / 127)
        self.assertEqual(jam.errors(), [])

    def test_parameters(self):
        jam = self.jam
        jam.button(CONTROL)
        zctrl = jam.state.synth.ctrl_screens_dict["screen 1"][0]
        jam.strip(0, 127)
        self.assertEqual(zctrl.value, zctrl.value_max, "The first parameter of the synth")
        self.assertEqual(jam.last_cc(FADER1), 127)
        self.assertEqual(jam.errors(), [])


class SessionTest(unittest.TestCase):
    """The session grid (SONG): columns the chains, the top row zynseq's first phrase."""

    def setUp(self):
        self.jam = Jam(self)
        self.jam.button(SONG)

    def test_launch_plays_at_the_next_bar(self):
        jam = self.jam
        stopped = jam.last_note(0x5C)
        self.assertNotIn(stopped, (None, 0), "Slot 1 of Synth has notes: lit")
        jam.pad(0x5C, 300)
        self.assertEqual(jam.libseq.play_calls[0], (0, 0, STARTING), "The pad launches the sequence")
        self.assertEqual(jam.libseq.getPlayState(0, 0, 0), PLAYING, "zynseq's clock: at the sync point")
        self.assertTrue(jam.model.transport.playing, "A launch starts zynseq's transport")
        self.assertEqual(jam.last_cc(PLAY), 127, "PLAY lit")
        self.assertNotEqual(jam.last_note(0x5C), stopped, "Playing: green")
        # MUTE + pad stops the chain's sequence at the sync point
        jam.send(0xB0, MUTE, 127)
        jam.pad(0x5C)
        jam.send(0xB0, MUTE, 0)
        jam.advance(2500)
        self.assertEqual(jam.libseq.getPlayState(0, 0, 0), 0)
        self.assertEqual(jam.last_note(0x5C), stopped)
        self.assertEqual(jam.errors(), [])

    def test_scene_button_starts_the_phrase(self):
        jam = self.jam
        jam.button(SCENE1 + 1)
        self.assertEqual(jam.state.zynseq.phrase, 1, "Scene 2 selects the phrase")
        self.assertIn(jam.libseq.getPlayState(0, 1, 32), (STARTING, PLAYING), "and starts its launcher")

    def test_group_buttons_select_the_chain(self):
        jam = self.jam
        jam.button(GROUP_A + 1)
        self.assertEqual(jam.state.chain_manager.active_chain.chain_id, 2)


class PlayAndStepTest(unittest.TestCase):

    def setUp(self):
        self.jam = Jam(self)

    def test_play_mode_plays_the_active_chain(self):
        jam = self.jam
        jam.send(0xB0, PAD_MODE, 127)
        jam.button(SCENE1)  # PAD MODE + scene 1: the note mode
        jam.send(0xB0, PAD_MODE, 0)
        played = jam.bridge.translate_note(bytes([0x90, 36, 100]))
        self.assertIsNotNone(played, "The lower left pad plays a note")
        jam.send(0x90, 36, 100)
        jam.advance(100)
        self.assertEqual(list(jam.model.playing_notes), [played[1]])
        jam.send(0x80, 36, 0)
        self.assertEqual(jam.model.playing_notes, {})

    def test_step_sequencer_edits_the_pattern(self):
        jam = self.jam
        jam.button(GROUP_A + 1)  # Piano, its empty pattern
        jam.send(0xB0, STEP, 127)
        jam.button(SCENE1)  # STEP + scene 1: the note sequencer
        jam.send(0xB0, STEP, 0)
        pattern = jam.libseq.getPattern(0, 0, 1, 0, 0)
        jam.pad(36)  # the lower left pad: step 1, the lowest note
        events = jam.libseq.patterns[pattern]["events"]
        self.assertEqual([e["position"] for e in events], [0])
        self.assertNotIn(jam.last_note(36), (None, 0), "The step lights")
        jam.pad(36)
        self.assertEqual(jam.libseq.patterns[pattern]["events"], [])
        self.assertEqual(jam.errors(), [])


if __name__ == "__main__":
    unittest.main()
