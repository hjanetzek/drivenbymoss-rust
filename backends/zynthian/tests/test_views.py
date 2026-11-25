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


if __name__ == "__main__":
    unittest.main()
