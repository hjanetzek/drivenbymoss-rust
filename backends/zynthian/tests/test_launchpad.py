# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: the Launchpad drivers (matching, the stock drivers, the views)
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

"""The Launchpad drivers (doc/ZYNTHIAN_PLAN.md §7b): their dev_ids against the core's discovery
names and zynthian's stock drivers, the stock driver of the same device unloaded when a
DrivenByMoss driver is loaded, and the Launchpad X (and the Mini MkIII) with the core against
the adapter: the session view launches and stops zynseq's sequences with their LEDs, the scene
buttons start phrases, the note view plays the active chain, the sequencers edit the pattern, the
clip length view, the mixer and device views. No Launchpad was connected: the device side is the
MIDI the core sends and the MIDI the Launchpad sends in programmer mode (pads: notes 11..88, the
buttons: CCs)."""

import importlib
import importlib.util
import os
import re
import tempfile
import time
import unittest

import zynstubs

zynstubs.install()

import drivenbymoss  # noqa: E402
import zynautoconnect  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.bridge import ZynBridge  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.launcher import PHRASE_CHANNEL, PLAYING, STARTING, STOPPING_SYNC  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.model import ZynthianModel  # noqa: E402
from zyngine.ctrldev.zynthian_ctrldev_base_dbm import launcher_palette  # noqa: E402

STOPPED = 0

# module, the core's model name, the stock driver of zynthian on another port of the device
MODELS = [
    ("launchpad_x", "Launchpad X", "zynthian_ctrldev_launchpad_x"),
    ("launchpad_mini_mk3", "Launchpad Mini MkIII", "zynthian_ctrldev_launchpad_mini_mk3"),
    ("launchpad_pro_mk3", "Launchpad Pro Mk3", "zynthian_ctrldev_launchpad_pro_mk3"),
    ("launchpad_pro", "Launchpad Pro", "zynthian_ctrldev_launchpad_pro_mk2"),
    ("launchpad_mk2", "Launchpad MkII", None),
]

# The Launchpad's controls in programmer mode (X, Mini MkIII)
SESSION, NOTE, DEVICE, SHIFT = 95, 96, 97, 98
SCENE1 = 89
GREEN, GREEN_LO = 22, 23


def driver_class(module):
    name = f"zynthian_ctrldev_dbm_{module}"
    return getattr(importlib.import_module(f"zyngine.ctrldev.{name}"), name)


def stock_dev_ids(module):
    """The dev_ids of a stock driver of the zynthian-ui checkout, read from its source (importing
    it would need zynthian's GUI)."""
    source = (zynstubs.UI_DIR / "zyngine" / "ctrldev" / f"{module}.py").read_text()
    return re.findall(r'"([^"]+)"', re.search(r"dev_ids\s*=\s*\[([^\]]*)\]", source).group(1))


def core_dev_ids(controller):
    """The dev_ids the core's Linux discovery names give (doc/ZYNTHIAN_PLAN.md §2.1): ``<card>
    MIDI <n>`` → ``<card> IN <n>``. The other names (``<card> <ALSA port name>``) don't carry the
    port's index."""
    names = [name for pair in controller["discovery"]["linux"] for name in pair["inputs"]]
    return {f"{m.group(1)} IN {m.group(2)}" for m in (re.fullmatch(r"(.+) MIDI (\d+)", name) for name in names) if m}


def project():
    """zynstubs' project with zynseq (4 phrases), a synth on Synth (channel 1, the active chain)
    whose pattern in phrase 1 has a note, and an empty pattern of Piano (channel 2)."""
    state = zynstubs.project()
    state.zynseq = zynstubs.FakeZynseq()
    state.synth = state.add_processor(1, "Synth A", [(f"screen {n}", [f"p{n}{i}" for i in range(4)]) for n in (1, 2)])
    libseq = state.zynseq.libseq
    for chan in (0, 1):
        libseq.addPattern(0, 0, chan, 0, 0, libseq.createPattern(), False)
    libseq.add_test_note(libseq.getPattern(0, 0, 0, 0, 0), 0, 60)
    return state


class MatchingTest(unittest.TestCase):
    """The dev_ids of the drivers: the port the core uses, not the one of zynthian's own driver."""

    def setUp(self):
        self.controllers = {c["uuid"]: c for c in drivenbymoss.list_controllers()}

    def test_the_core_runs_each_model(self):
        for module, model, _ in MODELS:
            cls = driver_class(module)
            controller = self.controllers.get(cls.dbm_controller_id)
            self.assertIsNotNone(controller, module)
            self.assertEqual(controller["model"], model)
            self.assertEqual(cls.get_driver_name(), f"DrivenByMoss {model}")
            self.assertTrue(cls.unroute_from_chains)

    def test_dev_ids_from_the_core_discovery_names(self):
        for module, _, _ in MODELS:
            cls = driver_class(module)
            self.assertEqual(len(cls.dev_ids), 1)
            self.assertIn(cls.dev_ids[0], core_dev_ids(self.controllers[cls.dbm_controller_id]), module)

    def test_not_on_the_port_of_the_stock_driver(self):
        expected = {"launchpad_x": ("Launchpad X IN 2", "Launchpad X IN 1"),
                    "launchpad_mini_mk3": ("Launchpad Mini MK3 IN 2", "Launchpad Mini MK3 IN 1"),
                    "launchpad_pro_mk3": ("Launchpad Pro MK3 IN 1", "Launchpad Pro MK3 IN 3"),
                    "launchpad_pro": ("Launchpad Pro IN 2", "Launchpad Pro IN 1")}
        for module, _, stock in MODELS:
            cls = driver_class(module)
            if stock is None:
                self.assertEqual(cls.replaces_dev_ids, [])
                self.assertTrue(cls.get_autoload_flag(), "No stock driver: it autoloads")
                continue
            ids = stock_dev_ids(stock)
            self.assertEqual((cls.dev_ids[0], ids[0]), expected[module])
            self.assertEqual(cls.replaces_dev_ids, ids, "It replaces the stock driver of the device")
            self.assertFalse(cls.get_autoload_flag(), "The stock driver autoloads, this one is chosen by the user")
        self.assertEqual(driver_class("launchpad_mk2").dev_ids, ["Launchpad MK2 IN 1"])

    def test_autoload_as_the_manager_does_it(self):
        """zynthian_ctrldev_manager: per dev_id the first class with the autoload flag (exact
        match); the stock drivers autoload (zynthian_ctrldev_base.autoload_flag)."""
        available = {}
        for module, _, stock in MODELS:
            cls = driver_class(module)
            for dev_id in cls.dev_ids:
                available.setdefault(dev_id, []).append(cls)
            if stock:
                for dev_id in stock_dev_ids(stock):
                    available.setdefault(dev_id, []).append(type(stock, (), {"get_autoload_flag": classmethod(lambda c: True)}))

        def autoload(dev_id):
            return next((cls.__name__ for cls in available.get(dev_id, []) if cls.get_autoload_flag()), None)

        self.assertEqual(autoload("Launchpad X IN 1"), "zynthian_ctrldev_launchpad_x")
        self.assertIsNone(autoload("Launchpad X IN 2"), "Not both on one device")
        self.assertIsNone(autoload("Launchpad Mini MK3 IN 2"))
        self.assertEqual(autoload("Launchpad MK2 IN 1"), "zynthian_ctrldev_dbm_launchpad_mk2")

    @unittest.skipUnless((zynstubs.UI_DIR / "zynautoconnect" / "zynthian_pipewire_ports.py").exists(), "zynthian-ui without the PipeWire port names")
    def test_pipewire_dev_ids(self):
        """[inferred] PipeWire's MIDI bridge names the ports of a card "<card>: <port>" in ALSA's
        order (seen for the Launch Control XL); the port names are Novation's (not seen here)."""
        spec = importlib.util.spec_from_file_location("zynthian_pipewire_ports", zynstubs.UI_DIR / "zynautoconnect" / "zynthian_pipewire_ports.py")
        ports = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(ports)
        cards = {"launchpad_x": ("Launchpad X", ["LPX DAW Out", "LPX MIDI Out"], 1),
                 "launchpad_mini_mk3": ("Launchpad Mini MK3", ["LPMiniMK3 DAW Out", "LPMiniMK3 MIDI Out"], 1),
                 "launchpad_pro_mk3": ("Launchpad Pro MK3", ["LPProMK3 MIDI", "LPProMK3 DIN", "LPProMK3 DAW"], 0),
                 "launchpad_pro": ("Launchpad Pro", ["MIDI 1", "MIDI 2", "MIDI 3"], 1),
                 "launchpad_mk2": ("Launchpad MK2", ["MIDI 1"], 0)}
        for module, (card, names, index) in cards.items():
            capture = [f"Midi-Bridge:{card}: {name} (capture)" for name in names]
            uids = [ports.pipewire_midi_port_name(name, capture)[0] for name in capture]
            cls = driver_class(module)
            self.assertEqual([uid.split("/", 1)[1] for uid in uids if uid.split("/", 1)[1] in cls.dev_ids], cls.dev_ids, module)
            self.assertEqual(uids[index].split("/", 1)[1], cls.dev_ids[0])
            out = ports.pipewire_midi_port_name(capture[index].replace("(capture)", "(playback)"), [c.replace("(capture)", "(playback)") for c in capture])[0]
            self.assertEqual(ports.dev_out_uid(uids[index]), out, "dev_in_2_dev_out finds the output")


class FakeDriver:

    def __init__(self, name):
        self.name = name

    def get_driver_name(self):
        return self.name


class FakeManager:
    """zynthian_ctrldev_manager's drivers and unload_driver."""

    def __init__(self):
        self.drivers = {}
        self.unloaded = []

    def unload_driver(self, izmip, disable=False):
        self.drivers.pop(izmip)
        self.unloaded.append((izmip, disable))
        return True


class ReplaceStockDriverTest(unittest.TestCase):
    """Loading the DrivenByMoss driver on the Launchpad X's MIDI port unloads zynthian's driver on
    the DAW port of the same device, as "disabled" (the ZS3 keeps it), before the core starts."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="dbm-zynthian-lpx-")
        self.old_config = os.environ.get("ZYNTHIAN_CONFIG_DIR")
        os.environ["ZYNTHIAN_CONFIG_DIR"] = self.temp.name
        self.state = zynstubs.project()
        self.state.ctrldev_manager = FakeManager()
        zynautoconnect.devices.update({
            7: ("Launchpad X IN 1", "USB:1.4/Launchpad X IN 1"),
            8: ("Launchpad X IN 2", "USB:1.4/Launchpad X IN 2"),
            9: ("Launchpad X IN 1", "USB:1.5/Launchpad X IN 1"),     # a second Launchpad X
            10: ("Launch Control XL IN 1", "USB:1.6/Launch Control XL IN 1"),
        })
        self.driver = None

    def tearDown(self):
        if self.driver is not None:
            self.driver.end()
        for idev in (7, 8, 9, 10):
            zynautoconnect.devices.pop(idev, None)
        if self.old_config is None:
            os.environ.pop("ZYNTHIAN_CONFIG_DIR", None)
        else:
            os.environ["ZYNTHIAN_CONFIG_DIR"] = self.old_config
        self.temp.cleanup()

    def test_the_stock_driver_of_the_device_is_unloaded(self):
        manager = self.state.ctrldev_manager
        manager.drivers = {7: FakeDriver("Launchpad X"), 9: FakeDriver("Launchpad X"), 10: FakeDriver("Launch Control XL")}
        cls = driver_class("launchpad_x")
        self.driver = cls(self.state, 8, 8)
        manager.drivers[8] = self.driver
        self.driver.init()
        self.assertEqual(manager.unloaded, [(7, True)], "Only the one on the same device, disabled")
        self.assertEqual(sorted(manager.drivers), [8, 9, 10])

    def test_without_a_stock_driver(self):
        manager = self.state.ctrldev_manager
        cls = driver_class("launchpad_mini_mk3")
        self.driver = cls(self.state, 8, 8)
        manager.drivers[8] = self.driver
        self.driver.init()
        self.assertEqual(manager.unloaded, [])


class Device:
    """A Launchpad in programmer mode against the core and the adapter on simulated time: the
    MIDI it sends and the state of its pads from the MIDI it gets (notes: static colors; the LED
    sysex 03: static, flashing, pulsing)."""

    def __init__(self, test, module="launchpad_x", state=None):
        self.cls = driver_class(module)
        self.temp = tempfile.TemporaryDirectory(prefix="dbm-zynthian-lp-")
        test.addCleanup(self.temp.cleanup)
        self.state = state or project()
        self.libseq = self.state.zynseq.libseq
        self.model = ZynthianModel(self.state.chain_manager, palette=launcher_palette(), zynseq=self.state.zynseq)
        self.midi_out = []
        self.bridge = ZynBridge(self.model, self.cls.dbm_controller_id, self.temp.name, "zynthian", midi_sink=self.midi_out.append)
        test.addCleanup(self.bridge.close)
        self.bridge.start()
        self.bridge.wait_for_startup()
        self.bridge.advance(300)
        self.led_header = bytes(self.midi_out[1][:6]) + b"\x03"  # after the device inquiry: the Launchpad's header

    def button(self, cc, wait=300):
        self.bridge.send([0xB0, cc, 127])
        self.bridge.send([0xB0, cc, 0], wait)

    def pad(self, note, wait=200):
        self.bridge.send([0x90, note, 127])
        self.bridge.send([0x90, note, 0], wait)

    def advance(self, ms=200):
        self.bridge.advance(ms)

    def led(self, note):
        """(color, None) for a static pad, (color, "flash", color B), (color, "pulse")."""
        state = None
        for data in self.midi_out:
            data = bytes(data)
            if len(data) == 3 and data[0] == 0x90 and data[1] == note:
                state = (data[2], None)
            elif data.startswith(self.led_header):
                i = len(self.led_header)
                while data[i] != 0xF7:
                    kind = data[i]
                    if kind == 1:
                        if data[i + 1] == note:
                            state = (data[i + 3], "flash", data[i + 2])
                        i += 4
                    else:
                        if data[i + 1] == note:
                            state = (data[i + 2], "pulse" if kind == 2 else None)
                        i += 3
        return state

    def notifications(self):
        return self.bridge.take_notifications()

    def unhandled(self, *processors):
        return [u for u in self.bridge.commands.unhandled if u.split(" ")[0] in processors]

    def errors(self):
        return list(self.bridge.core_errors())

    def events(self, phrase=0, chan=0):
        pattern = self.libseq.getPattern(0, phrase, chan, 0, 0)
        return [(e["position"], e["val1_start"]) for e in self.libseq.patterns[pattern]["events"]]

    def select_note_view(self, pad):
        """Note: the view selection (from a note view), then the pad of the view."""
        self.button(NOTE)
        self.pad(pad)


class LaunchpadXSessionTest(unittest.TestCase):
    """The session view: the 8x8 pads are the slots (columns: the tracks, rows: zynseq's phrases,
    top to bottom), the scene buttons the phrases."""

    MODULE = "launchpad_x"

    def setUp(self):
        self.lp = Device(self, self.MODULE)
        self.lp.button(SESSION)
        self.assertIn("Session", self.lp.notifications())

    def state_of(self, phrase, chan=0):
        return self.lp.libseq.getPlayState(0, phrase, chan)

    def test_launch_and_leds(self):
        lp = self.lp
        stopped = lp.led(81)
        self.assertNotEqual(stopped[0], 0, "Synth's slot 1 has notes: lit in the chain's color")
        self.assertIsNone(stopped[1], "not blinking")
        self.assertEqual(lp.led(82), (0, None), "Piano's empty slot is off")
        # Pad 1: the sequence starts at the next sync point; the pad flashes green
        lp.pad(81)
        self.assertEqual(self.state_of(0), STARTING)
        self.assertEqual(lp.led(81)[1:], ("flash", GREEN_LO))
        # Again: still starting (zynseq's own launch would stop it)
        lp.pad(81)
        self.assertEqual(self.state_of(0), STARTING)
        # Playing: the pad pulses
        lp.libseq.play_states[(0, 0, 0)] = PLAYING
        lp.advance()
        self.assertEqual(lp.led(81)[1], "pulse")
        lp.pad(81)
        self.assertEqual(self.state_of(0), PLAYING, "A playing sequence keeps playing")
        # An empty slot starts too (every zynseq slot is a sequence)
        lp.pad(82)
        self.assertEqual(self.state_of(0, 1), STARTING)
        self.assertEqual(lp.errors(), [])
        self.assertEqual(lp.unhandled("track", "scene", "clip"), [])

    def test_queued_and_playing_pads_blink_in_green(self):
        """Java's Launchpad session view ignores the clip color for playing and recording
        (SessionView: ignoreClipColorForPlayAndRecord): a queued slot flashes between GREEN and
        GREEN_LO, a playing one pulses GREEN. The core sent the blinking pads' base color as 0,
        so they flashed and pulsed against black (fixed in src/framework/grid.rs send_state,
        BlinkMode::Collect)."""
        lp = self.lp
        lp.pad(81)
        self.assertEqual(lp.led(81), (GREEN, "flash", GREEN_LO))
        lp.libseq.play_states[(0, 0, 0)] = PLAYING
        lp.advance()
        self.assertEqual(lp.led(81), (GREEN, "pulse"))

    def test_scene_launch(self):
        lp = self.lp
        lp.button(SCENE1 - 10)  # scene 2
        self.assertEqual((lp.state.zynseq.phrase, self.state_of(1, PHRASE_CHANNEL)), (1, STARTING))
        self.assertEqual(lp.unhandled("scene"), [])

    def test_stop_from_the_mix_view(self):
        """The X has no stop buttons: Shift + Session is the Mix view, its fifth row stops the
        tracks (the Stop Clip mode of the session view is reached through the Shift view, which
        gives a core error: doc/ZYNTHIAN_PLAN.md §7b)."""
        lp = self.lp
        lp.libseq.play_states[(0, 0, 0)] = PLAYING
        lp.advance()
        lp.bridge.send([0xB0, SHIFT, 127])
        lp.button(SESSION)
        lp.bridge.send([0xB0, SHIFT, 0], 300)
        self.assertIn("Mix", lp.notifications())
        lp.pad(41)
        self.assertEqual(self.state_of(0), STOPPING_SYNC, "At the next sync point")
        lp.button(SESSION)
        self.assertEqual(lp.led(81)[1:], ("flash", GREEN_LO), "The queued stop flashes")
        lp.libseq.play_states[(0, 0, 0)] = STOPPED
        lp.advance()
        self.assertIsNone(lp.led(81)[1], "Stopped: static again")
        self.assertEqual(lp.errors(), [])


class LaunchpadMiniMk3SessionTest(LaunchpadXSessionTest):

    MODULE = "launchpad_mini_mk3"


class LaunchpadXNoteViewTest(unittest.TestCase):
    """The driver of the Launchpad X as the ctrldev manager runs it: the note view (the start
    view) plays the pads into the active chain, at once (§3.4)."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="dbm-zynthian-lpx-driver-")
        self.old_config = os.environ.get("ZYNTHIAN_CONFIG_DIR")
        os.environ["ZYNTHIAN_CONFIG_DIR"] = self.temp.name
        self.state = project()
        self.zyncore = zynstubs.lib_zyncore()
        self.zyncore.notes.clear()
        self.driver = driver_class("launchpad_x")(self.state, 3, 5)
        self.driver.init()
        end = time.monotonic() + 5
        while time.monotonic() < end and not (self.driver.bridge and self.driver.bridge.note_inputs):
            time.sleep(0.02)

    def tearDown(self):
        self.driver.end()
        if self.old_config is None:
            os.environ.pop("ZYNTHIAN_CONFIG_DIR", None)
        else:
            os.environ["ZYNTHIAN_CONFIG_DIR"] = self.old_config
        self.temp.cleanup()

    def test_pads_play_the_active_chain(self):
        bridge = self.driver.bridge
        self.assertIsNotNone(bridge)
        self.assertEqual(bridge.note_inputs[0].filters, ["80????", "90????"])
        expected = bridge.translate_note(b"\x90\x0b\x64")
        self.assertEqual(expected, (0x90, 36, 100), "The play view: the lowest pad is C1")
        self.driver.midi_event(b"\x90\x0b\x64")
        self.assertEqual(self.zyncore.notes, [("on", 0, 36, 100)], "Synth: MIDI channel 1, at once")
        self.driver.midi_event(b"\x90\x0b\x00")
        self.assertEqual(self.zyncore.notes[-1], ("off", 0, 36, 0))
        self.state.chain_manager.set_active_chain_by_id(2)
        self.driver.midi_event(b"\x90\x0c\x64")
        self.assertEqual(self.zyncore.notes[-1][:2], ("on", 1), "Then Piano")
        self.driver.midi_event(b"\x80\x0c\x00")
        self.assertEqual(self.zyncore.notes[-1][:2], ("off", 1))


class LaunchpadXViewsTest(unittest.TestCase):
    """The note and drum sequencers on the zynseq pattern of the cursor slot, the clip length
    view, the drum, mixer and device views."""

    def setUp(self):
        self.lp = Device(self)

    def test_sequencer_edits_the_pattern(self):
        lp = self.lp
        lp.select_note_view(41)
        self.assertIn("Sequencer", lp.notifications())
        before = lp.led(71)
        # The top row of the sequencer: step 1 of its highest note
        lp.pad(71)
        self.assertEqual(len(lp.events()), 2)
        step = [e for e in lp.events() if e != (0, 60)][0]
        self.assertEqual(step[0], 0, "Step 1")
        lp.advance()
        self.assertNotEqual(lp.led(71), before, "The pad shows the note")
        lp.pad(72)
        self.assertIn((1, step[1]), lp.events(), "Step 2")
        lp.pad(71)
        self.assertNotIn(step, lp.events(), "Off again")
        self.assertEqual(lp.unhandled("clip"), [])
        self.assertEqual(lp.errors(), [])

    def test_drum_view_plays_and_sequences(self):
        lp = self.lp
        lp.select_note_view(61)
        self.assertIn("Drum", lp.notifications())
        self.assertEqual(lp.bridge.translate_note(b"\x90\x0b\x64"), (0x90, 36, 100), "The lower left pad: drum note 36")
        # The upper half: the steps of the selected drum note
        before = set(lp.events())
        lp.pad(81)
        added = set(lp.events()) - before
        self.assertEqual(len(added), 1)
        self.assertEqual(next(iter(added))[0], 0, "Step 1")
        self.assertIn(next(iter(added))[1], range(36, 52), "A note of the 16 drum pads")
        lp.pad(81)
        self.assertEqual(set(lp.events()), before)
        self.assertEqual(lp.unhandled("clip", "drum"), [])
        self.assertEqual(lp.errors(), [])

    def test_clip_length(self):
        """The clip length view: a range from the first pad is the pattern's length; zynseq's
        patterns start at 0, so a range from another pad changes nothing (before the fix the
        end of that range became the length: 228 beats)."""
        lp = self.lp
        lp.select_note_view(21)
        self.assertIn("Clip Length", lp.notifications())
        pattern = lp.libseq.getPattern(0, 0, 0, 0, 0)
        # Pads 1 and 2 held together: 2 bars
        lp.bridge.send([0x90, 81, 127])
        lp.pad(82)
        lp.bridge.send([0x90, 81, 0], 200)
        self.assertEqual(lp.libseq.getBeatsInPattern(pattern), 8)
        lp.pad(81)
        self.assertEqual(lp.libseq.getBeatsInPattern(pattern), 4)
        lp.pad(11)
        self.assertEqual(lp.libseq.getBeatsInPattern(pattern), 4, "A range from bar 57: not applied")
        self.assertEqual(sorted(lp.unhandled("clip")), ["clip end", "clip loopLength", "clip loopStart", "clip start"],
                         "The range is logged as not handled")
        lp.pad(82)
        self.assertEqual(lp.libseq.getBeatsInPattern(pattern), 4, "Bar 2 alone: not applied either")
        lp.pad(81)
        self.assertEqual(lp.libseq.getBeatsInPattern(pattern), 4)
        lp.bridge.send([0x90, 81, 127])
        lp.pad(83)
        lp.bridge.send([0x90, 81, 0], 200)
        self.assertEqual(lp.libseq.getBeatsInPattern(pattern), 12, "The next range from the first pad applies again")

    def test_mix_view(self):
        lp = self.lp
        lp.bridge.send([0xB0, SHIFT, 127])
        lp.button(SESSION)
        lp.bridge.send([0xB0, SHIFT, 0], 300)
        self.assertIn("Mix", lp.notifications())
        lp.pad(31)  # row 6: mute
        self.assertEqual(lp.state.zctrl(1, "mute").value, 1)
        lp.pad(22)  # row 7: solo of the second track
        self.assertEqual(lp.state.zctrl(2, "solo").value, 1)
        self.assertEqual(lp.unhandled("track"), [])
        self.assertEqual(lp.errors(), [])

    def test_device_view(self):
        """The device view: the 8 columns are faders of the cursor device's 8 parameters (two of
        zynthian's screens)."""
        lp = self.lp
        lp.button(DEVICE)
        self.assertIn("Device", lp.notifications())
        zctrl = lp.state.synth.controllers_dict["p10"]
        lp.pad(81, 1500)  # the top of the first column
        self.assertEqual(zctrl.value, 127)
        lp.pad(18, 1500)  # the bottom of the last one: the 4th parameter of the 2nd screen
        self.assertLess(lp.state.synth.controllers_dict["p23"].value, 16)
        self.assertEqual(lp.unhandled("device"), [])
        self.assertEqual(lp.errors(), [])


if __name__ == "__main__":
    unittest.main()
