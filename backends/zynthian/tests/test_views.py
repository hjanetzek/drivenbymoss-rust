# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: tests of the controller's views on zynthian's screens
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

"""The controller's views on zynthian's screens and the core's note grid window on the pattern
editor (dbm_zynthian/views.py, zynthian-ui docs/controller-view-sync.md): the adapter with a
stand-in UI; the commands (ControllerView, ClipEditWindow) once the binding has them."""

import unittest

import zynstubs

zynstubs.install()

from dbm_host import Banks  # noqa: E402
from drivenbymoss.protocol import Cmd  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.commands import ZynCommandHandler  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.model import ZynthianModel  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.reporter import ZynReporter  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.views import (VIEW_DEVICE, VIEW_DRUM_SEQUENCER, VIEW_LAUNCHER, VIEW_MIXER, VIEW_PLAY,  # noqa: E402
                                                VIEW_SEQUENCER, ViewsUI)


class FakeUI:
    """zynthian-ui's Qt UI as the adapter sees it: the current screen, the pattern editor's
    pattern, the window CUIA."""

    def __init__(self, screen="mixer", window=True):
        self.current_screen = screen
        self.pattern_editor = type("PE", (), {"pattern": None})()
        if window:
            self.cuia_pattern_editor_window = lambda params=None: None


class Host:

    def __init__(self, screen="mixer", window=True, ui=True):
        self.state = zynstubs.project()
        self.state.zynseq = zynstubs.FakeZynseq()
        self.libseq = self.state.zynseq.libseq
        for chan in (0, 1):
            self.libseq.addPattern(0, 0, chan, 0, 0, self.libseq.createPattern(), False)
        self.model = ZynthianModel(self.state.chain_manager, zynseq=self.state.zynseq)
        self.ui = FakeUI(screen, window)
        if ui:
            self.model.views.ui = ViewsUI(self.state, get_ui=lambda: self.ui)
        self.views = self.model.views

    def cuias(self):
        cuias, self.state.cuias[:] = list(self.state.cuias), []
        return cuias

    def pattern(self, chan, phrase=0):
        return self.libseq.getPattern(0, phrase, chan, 0, 0)


class ControllerViewTest(unittest.TestCase):

    def test_launcher(self):
        host = Host("mixer")
        host.views.controller_view(VIEW_LAUNCHER)
        self.assertEqual(host.cuias(), [("SCREEN_ZYNPAD", None)])
        host.ui.current_screen = "launcher"
        host.views.controller_view(VIEW_LAUNCHER)
        self.assertEqual(host.cuias(), [], "The launcher is shown")

    def test_sequencer(self):
        host = Host("launcher")
        for kind in (VIEW_SEQUENCER, VIEW_DRUM_SEQUENCER):
            host.views.controller_view(kind)
            self.assertEqual(host.cuias(), [("SCREEN_PATTERN_EDITOR", None)])
        host.ui.current_screen = "pattern_editor"
        host.ui.pattern_editor.pattern = host.pattern(1)
        host.views.controller_view(VIEW_SEQUENCER)
        self.assertEqual(host.cuias(), [("SCREEN_PATTERN_EDITOR", None)], "Another pattern shown: the cursor slot's")
        host.ui.pattern_editor.pattern = host.pattern(0)
        host.views.controller_view(VIEW_SEQUENCER)
        self.assertEqual(host.cuias(), [], "The cursor slot's pattern is shown")

    def test_no_cursor_slot(self):
        host = Host("mixer")
        host.state.chain_manager.active_chain = host.state.chain_manager.chains[3]   # Audio In
        host.views.controller_view(VIEW_SEQUENCER)
        self.assertEqual(host.cuias(), [])

    def test_other_views(self):
        host = Host("launcher")
        for kind in (VIEW_PLAY, VIEW_MIXER, VIEW_DEVICE, 99):
            host.views.controller_view(kind)
        self.assertEqual(host.cuias(), [])


class EditWindowTest(unittest.TestCase):

    def test_window_on_the_cursor_pattern(self):
        host = Host("mixer")
        host.views.edit_window(2.0, 2.0, 48, 60)
        self.assertEqual(host.cuias(), [("PATTERN_EDITOR_WINDOW", [2.0, 2.0, 48, 60, host.pattern(0)])],
                         "Kept by the pattern editor; no screen switch")

    def test_clip_changed(self):
        host = Host("pattern_editor")
        host.ui.pattern_editor.pattern = host.pattern(1)
        host.views.edit_window(0, 4, 36, 51)
        self.assertEqual(host.cuias(), [("SCREEN_PATTERN_EDITOR", None), ("PATTERN_EDITOR_WINDOW", [0.0, 4.0, 36, 51, host.pattern(0)])])

    def test_tk(self):
        host = Host("mixer", window=False)
        host.views.edit_window(0, 4, 36, 51)
        self.assertEqual(host.cuias(), [], "Tk has no window CUIA")

    def test_no_ui(self):
        host = Host(ui=False)
        self.assertFalse(host.views.available)
        self.assertFalse(host.views.edit_window(0, 4, 36, 51))
        self.assertFalse(host.views.controller_view(VIEW_LAUNCHER))


@unittest.skipUnless(hasattr(Cmd, "CONTROLLER_VIEW") and hasattr(Cmd, "CLIP_EDIT_WINDOW"), "The binding has no view commands yet")
class CommandsTest(unittest.TestCase):

    def handler(self, host):
        banks = Banks.from_model_setup({"numTracks": 8, "numScenes": 8})
        commands = ZynCommandHandler(host.model, banks, ZynReporter(host.model, banks))
        commands.configure()
        return commands

    def test_handled_only_with_the_ui(self):
        self.assertTrue({Cmd.CONTROLLER_VIEW, Cmd.CLIP_EDIT_WINDOW} <= self.handler(Host()).handled_commands())
        self.assertFalse({Cmd.CONTROLLER_VIEW, Cmd.CLIP_EDIT_WINDOW} & self.handler(Host(ui=False)).handled_commands())
        self.assertTrue({Cmd.CONTROLLER_VIEW, Cmd.CLIP_EDIT_WINDOW} <= ZynCommandHandler.UNHANDLED_COMMANDS,
                        "Unhandled without the UI (the coverage snapshot)")


@unittest.skipUnless(hasattr(Cmd, "CONTROLLER_VIEW") and hasattr(Cmd, "CLIP_EDIT_WINDOW"), "The binding has no view commands yet")
class JamTest(unittest.TestCase):
    """The JAM with the core: SONG, STEP and its page buttons on zynthian's screens."""

    def setUp(self):
        import tempfile
        from test_maschine_jam import Jam, project
        from zyngine.ctrldev.dbm_zynthian.bridge import ZynBridge
        from zyngine.ctrldev.zynthian_ctrldev_base_dbm import launcher_palette
        from zyngine.ctrldev.zynthian_ctrldev_dbm_maschine_jam import zynthian_ctrldev_dbm_maschine_jam as jam_driver
        jam = Jam.__new__(Jam)
        jam.temp = tempfile.TemporaryDirectory(prefix="dbm-zynthian-views-")
        self.addCleanup(jam.temp.cleanup)
        jam.state = project()
        jam.libseq = jam.state.zynseq.libseq
        jam.model = ZynthianModel(jam.state.chain_manager, palette=launcher_palette(), zynseq=jam.state.zynseq)
        self.ui = FakeUI("mixer")
        jam.model.views.ui = ViewsUI(jam.state, get_ui=lambda: self.ui)
        jam.midi_out = []
        jam.bridge = ZynBridge(jam.model, jam_driver.dbm_controller_id, jam.temp.name, "zynthian", midi_sink=jam.midi_out.append)
        self.addCleanup(jam.bridge.close)
        jam.bridge.start()
        jam.bridge.wait_for_startup()
        jam.advance(500)
        self.jam = jam
        self.cuias()

    def cuias(self):
        cuias, self.jam.state.cuias[:] = list(self.jam.state.cuias), []
        return cuias

    def screens(self):
        return [c for c, _ in self.cuias() if c.startswith("SCREEN_")]

    def windows(self):
        return [p for c, p in self.cuias() if c == "PATTERN_EDITOR_WINDOW"]

    def test_song_shows_the_pads(self):
        from test_maschine_jam import SONG
        self.jam.button(SONG)
        self.assertEqual(self.screens(), ["SCREEN_ZYNPAD"])
        self.ui.current_screen = "launcher"
        self.jam.button(SONG)   # flips the session: zynthian stays
        self.assertEqual(self.screens(), [])
        self.assertEqual(self.jam.errors(), [])

    def test_step_shows_the_pattern_and_its_window(self):
        from test_maschine_jam import STEP
        LEFT, RIGHT = 43, 44   # NAV LEFT / RIGHT: the sequencer's pages
        jam = self.jam
        jam.button(STEP)
        cuias = self.cuias()
        self.assertIn(("SCREEN_PATTERN_EDITOR", None), cuias)
        windows = [p for c, p in cuias if c == "PATTERN_EDITOR_WINDOW"]
        self.assertTrue(windows, f"The window follows the view: {cuias}")
        pattern = jam.libseq.getPattern(0, 0, 0, 0, 0)
        start, length, low, high, shown = windows[-1]
        self.assertEqual((start, shown), (0.0, pattern))
        self.assertGreater(length, 0)
        self.assertLessEqual(low, high)
        self.ui.current_screen, self.ui.pattern_editor.pattern = "pattern_editor", pattern
        jam.button(RIGHT)
        cuias = self.cuias()
        self.assertEqual([c for c, _ in cuias if c.startswith("SCREEN_")], [], "Paging doesn't switch screens")
        windows = [p for c, p in cuias if c == "PATTERN_EDITOR_WINDOW"]
        self.assertEqual([w[:2] for w in windows], [[length, length]], "The next page")
        jam.button(LEFT)
        self.assertEqual([w[0] for w in self.windows()], [0.0])
        self.ui.current_screen = "mixer"
        jam.button(RIGHT)
        self.assertEqual(self.screens(), [], "Elsewhere the window doesn't switch screens")
        self.assertEqual(jam.errors(), [])


if __name__ == "__main__":
    unittest.main()
