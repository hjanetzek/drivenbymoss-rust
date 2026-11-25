# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: the controller's views on zynthian's screens
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

"""The view the user picks on the controller on zynthian's screens, and the window of the core's
step sequencer on zynthian's pattern editor (zynthian-ui docs/controller-view-sync.md).

``ControllerView`` (the core sends it only when the user picks a view with a button: the JAM's
SONG, STEP, pad and mixer mode buttons):

| Kind | zynthian |
|---|---|
| Launcher | the pads view (CUIA ``SCREEN_ZYNPAD``), unless the launcher is shown |
| Sequencer, DrumSequencer | the pattern editor of the cursor slot (``SCREEN_PATTERN_EDITOR``: the active chain's sequence in the selected phrase, the core's cursor clip), unless it shows that pattern; nothing without a cursor slot |
| the others (Play, Mixer, Device, Other, later kinds) | nothing |

``ClipEditWindow`` (start, length in beats, the lowest and highest MIDI note of the core's note
grid; sent with a sequencer view, on every change of its window and of the edited clip): the
CUIA ``PATTERN_EDITOR_WINDOW start length low high pattern`` (zynthian-ui's Qt UI only: the
pattern editor keeps it for the pattern and shows it when it shows that pattern; Tk has no such
CUIA). If the pattern editor is shown with another pattern (the core's clip changed: a track or
slot selected on the controller), it shows the cursor slot's first. The window never switches
screens otherwise: zynthian's screens follow the controller only on its view buttons.

Only controller to zynthian: zynthian's grid scrolls and zooms freely, the core's window moves
by pages and octaves."""

import logging

logger = logging.getLogger("dbm_zynthian")

# The core's view kinds (ControllerView); later kinds are ignored
VIEW_LAUNCHER = 0
VIEW_PLAY = 1
VIEW_SEQUENCER = 2
VIEW_DRUM_SEQUENCER = 3
VIEW_MIXER = 4
VIEW_DEVICE = 5
VIEW_OTHER = 6

LAUNCHER_SCREENS = ("launcher", "zynpad")
PATTERN_EDITOR_SCREENS = ("pattern_editor", "pated")
WINDOW_CUIA = "PATTERN_EDITOR_WINDOW"


class ViewsUI:
    """zynthian's UI for the views: CUIAs (the state manager's queue, run in the UI's thread) and
    what it shows (read only)."""

    def __init__(self, state_manager, get_ui=None):
        self.state_manager = state_manager
        self.get_ui = get_ui or self._zyngui

    @staticmethod
    def _zyngui():
        try:
            from zyngui import zynthian_gui_config
            return getattr(zynthian_gui_config, "zyngui", None)
        except Exception:
            return None

    def available(self):
        return self.get_ui() is not None

    def send(self, cuia, params=None):
        self.state_manager.send_cuia(cuia, params)

    def current_screen(self):
        ui = self.get_ui()
        return getattr(ui, "current_screen", None) if ui is not None else None

    def has_window(self):
        """The UI shows a controller's window (zynthian-ui's Qt UI: ``cuia_pattern_editor_window``)."""
        ui = self.get_ui()
        return ui is not None and callable(getattr(ui, "cuia_" + WINDOW_CUIA.lower(), None))

    def shown_pattern(self):
        """The pattern the pattern editor shows (the Qt UI's ``pattern_editor``, Tk's ``pated``
        screen), None if unknown."""
        ui = self.get_ui()
        if ui is None:
            return None
        editor = getattr(ui, "pattern_editor", None)
        if editor is None:
            editor = (getattr(ui, "screens", None) or {}).get("pattern_editor")
        pattern = getattr(editor, "pattern", None)
        return pattern if isinstance(pattern, int) else None


class ZynViews:
    """The core's ControllerView and ClipEditWindow on zynthian's screens (the module's doc)."""

    def __init__(self, model, ui=None):
        self.model = model
        self.ui = ui
        """ViewsUI, set by the driver; None: no UI (the views are not handled)."""
        self.window = None
        """The last window (start, length, low, high), applied again when the clip changes."""

    @property
    def available(self):
        return self.ui is not None

    def cursor_slot(self):
        """(phrase, channel, pattern) of the cursor slot (the core's cursor clip), None without
        one (no zynseq, an audio or effect chain, the main chain)."""
        launcher = self.model.launcher
        if launcher is None or not launcher.available:
            return None
        selected = launcher.selected_sequence()
        if selected is None or not 0 <= selected[1] <= 15:
            return None
        return selected[0], selected[1], launcher.pattern(*selected)

    def controller_view(self, kind):
        """The view the user picked on the controller."""
        if self.ui is None:
            return False
        screen = self.ui.current_screen()
        if kind == VIEW_LAUNCHER:
            if screen not in LAUNCHER_SCREENS:
                self.ui.send("SCREEN_ZYNPAD")
        elif kind in (VIEW_SEQUENCER, VIEW_DRUM_SEQUENCER):
            slot = self.cursor_slot()
            if slot is None:
                return True
            if screen not in PATTERN_EDITOR_SCREENS or self.ui.shown_pattern() != slot[2]:
                self.ui.send("SCREEN_PATTERN_EDITOR")
        return True

    def edit_window(self, start, length, low, high):
        """The window of the core's note grid: on the cursor slot's pattern; if the pattern
        editor shows another pattern, it shows the cursor slot's first."""
        if self.ui is None:
            return False
        self.window = (float(start), float(length), int(low), int(high))
        slot = self.cursor_slot()
        if slot is None or slot[2] is None:
            return True
        if self.ui.current_screen() in PATTERN_EDITOR_SCREENS and self.ui.shown_pattern() != slot[2]:
            self.ui.send("SCREEN_PATTERN_EDITOR")
        if self.ui.has_window():
            self.ui.send(WINDOW_CUIA, [*self.window, slot[2]])
        return True
