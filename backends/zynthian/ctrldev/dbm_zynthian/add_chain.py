# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: the core's browser on zynthian's Add Chain view
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

"""Browser sessions opened by the host on zynthian's screens for adding a chain
(doc/ZYNTHIAN_PLAN.md §3.2, §3.7): selecting the "add chain" track (model.py) opens zynthian's
Add Chain view and reports the browser active, so a controller which follows the browser's
active state (the Maschine Mk2's browse mode) browses the view's entries; the chosen chain type
goes on to zynthian's engine screen, which the controller browses next.

``ChainFlow`` (``ZynBrowser.add_chain``) follows zynthian's screens: one session at a time, a
gap of at least one report with ``/browser/exists 0`` between two (the core enters its browse
mode again only on a change of the active state, and its encoder push leaves it).

The Add Chain view (``AddChainSession``):

| Core | zynthian |
| --- | --- |
| open (the "+" track's select) | ``ADD_CHAIN`` (the view is shown; the new chain goes after the active one) |
| content type | one: "Add chain" |
| filter column 0 "Chain" and the results | the view's entries (Instrument, Audio Input, Clip Launcher, MIDI, …): one list, one cursor |
| ``column/0/…``, ``result/…`` (``+``, ``-``, ``page/±``, ``reset``) | the cursor; the view's selection follows (``SELECT <index>``) |
| ``stop 1`` (commit) | the entry's action (``SELECT <index>``, ``SELECT_ACTION``: zynthian's ``modify_chain`` with the chain type, as a tap on it); then the engine screen's session |
| ``stop 0`` (cancel) | ``BACK`` (the view is closed, no chain added) |

The engine screen (``EngineSession``, the Qt UI's ``Engine``):

| Core | zynthian |
| --- | --- |
| content type | one: "Engine" |
| filter column 0 "Category" | the screen's categories (``filtered_engines_by_cat`` of the chain type); a move: ``ARROW_RIGHT``/``ARROW_LEFT`` (the screen shows the category, its first engine selected) |
| results | the category's engines as the screen lists them ("None" first for a MIDI or audio effect chain: an empty chain) |
| ``result/…`` | ``SELECT <index>`` |
| ``stop 1`` (commit) | the engine's action (``SELECT``, ``SELECT_ACTION``): zynthian goes on (the MIDI channel of a MIDI chain, else the chain is added and shown with its bank/preset or controllers); the browser closes |
| ``stop 0`` (cancel) | ``BACK``, as zynthian's own: back to the Add Chain view (no chain was created yet), whose session follows |

``preview`` and ``contentType/±`` are accepted (nothing to do). If the user leaves a screen in
zynthian's UI, the session closes; the next follows if zynthian shows its screen (an entry of the
Add Chain view tapped: the engine screen; BACK on the engine screen: the Add Chain view)."""

import logging

from dbm_host.reporter import M
from zyngine.ctrldev.dbm_zynthian.browser import report_session

logger = logging.getLogger("dbm_zynthian")

ADD_CHAIN_SCREEN = "add_chain"
ENGINE_SCREEN = "engine"

# The entries of zynthian's Add Chain view (zynqt/screens/menus.py AddChain, zyngui
# zynthian_gui_add_chain), until the UI's own list is known
ENTRIES = ["Instrument", "Audio Input", "Clip Launcher", "MIDI", "MIDI + Audio", "Audio Generator", "Special", "Mixbus"]

# Reports a session waits for zynthian to show its screen
FOLLOW_REPORTS = 50

# The engines zynqt's Engine screen leaves out (Engine.fill), and the chain types with "None"
EXCLUDED_ENGINES = ("MI", "MR", "MX")
TYPES_WITH_NONE = ("MIDI Tool", "Audio Effect")


def entry_name(title):
    return " ".join(str(title or "").split())


class AddChainUI:
    """zynthian's UI for the chain sessions: CUIAs (the state manager's queue, run in the UI's
    thread) and what its screens show (read only)."""

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

    def screen(self, name):
        """The Qt UI's screen object (``list_screens``) or the Tk UI's (``screens``)."""
        ui = self.get_ui()
        if ui is None:
            return None
        return (getattr(ui, "list_screens", None) or {}).get(name) or (getattr(ui, "screens", None) or {}).get(name)

    def add_chain_entries(self):
        """The Add Chain view's selectable entries as (index, name) and the selected index;
        ([], 0) if it has none (yet)."""
        screen = self.screen(ADD_CHAIN_SCREEN)
        try:
            if screen is not None and hasattr(screen, "entries"):
                rows = list(screen.entries)
                entries = [(i, entry_name(e["title"])) for i, e in enumerate(rows) if not e.get("separator") and e.get("title")]
                return entries, getattr(screen, "index", 0) or 0
            if screen is not None:
                rows = list(getattr(screen, "config", []) or [])
                return [(i, entry_name(e.get("title"))) for i, e in enumerate(rows) if e.get("title")], 0
        except Exception as e:
            logger.debug("No entries of the Add Chain view: %s", e)
        return [], 0

    def engine_lists(self):
        """The Qt UI's engine screen: (categories, category index, the category's engines as a
        function of the category index: [(index in the screen, name)]); None if unknown."""
        ui = self.get_ui()
        screen = self.screen(ENGINE_SCREEN)
        by_cat = getattr(screen, "engines_by_cat", None)
        if ui is None or not isinstance(by_cat, dict) or getattr(screen, "show_all", False):
            return None
        try:
            proc_type = ui.chain_edit.status.get("type", "")
            categories = list(screen.categories)
            category = int(screen.categoryIndex)
        except Exception as e:
            logger.debug("No engine lists: %s", e)
            return None

        def engines(index):
            entries = ["None"] if proc_type in TYPES_WITH_NONE else []
            if 0 <= index < len(categories):
                entries += [info.get("TITLE", code) for code, info in by_cat.get(categories[index], {}).items()
                            if code not in EXCLUDED_ENGINES]
            return list(enumerate(entries))

        return categories, category, engines


class ScreenSession:
    """A browser session on one of zynthian's screens."""

    screen = None
    content_type = ""

    def __init__(self, flow, backs, seen):
        self.flow = flow
        self.ui = flow.ui
        self.backs = backs
        """The BACKs which close the screen (cancel)."""
        self.seen = seen

    def left(self):
        """The user left the screen in zynthian's UI."""
        if self.ui.current_screen() == self.screen:
            self.seen = True
            return False
        return self.seen

    def back(self):
        if not self.seen or self.ui.current_screen() == self.screen:
            for _ in range(self.backs):
                self.ui.send("BACK")

    def refresh(self):
        pass

    @staticmethod
    def moved(cursor, what, page, count):
        """The cursor after ``+``, ``-``, ``page/±``, ``reset``; None if not such a move."""
        steps = {"+": 1, "-": -1, "page/+": page, "page/-": -page}
        if what in steps:
            cursor += steps[what]
        elif what == "reset":
            cursor = 0
        else:
            return None
        return max(0, min(cursor, count - 1))

    def command(self, parts, reset, page_columns, page_results):
        """``column/0/…`` and ``result/…``; False if not handled."""
        if len(parts) >= 2 and parts[0] == "column":
            if parts[1] != "0":
                return False
            return self.move_column("reset" if reset and len(parts) == 2 else "/".join(parts[2:]), page_columns)
        if len(parts) >= 2 and parts[0] == "result":
            return self.move_result("/".join(parts[1:]), page_results)
        return False


class AddChainSession(ScreenSession):
    """The Add Chain view: its entries as column 0 and as the results (one cursor)."""

    screen = ADD_CHAIN_SCREEN
    content_type = "Add chain"

    def __init__(self, flow, backs, seen, entries, cursor):
        super().__init__(flow, backs, seen)
        self.entries = entries
        """The entries: (index in the view, name)."""
        self.cursor = cursor

    def refresh(self):
        """The view's entries (it may have been filled since)."""
        entries, _ = self.ui.add_chain_entries()
        if entries and entries != self.entries:
            name = self.entries[self.cursor][1] if 0 <= self.cursor < len(self.entries) else None
            self.entries = entries
            self.cursor = next((p for p, (_, n) in enumerate(entries) if n == name), 0)
            self.ui.send("SELECT", [self.entries[self.cursor][0]])

    def select(self, position):
        if position is None:
            return False
        if self.entries and position != self.cursor:
            self.cursor = position
            self.ui.send("SELECT", [self.entries[position][0]])
        return True

    def move_column(self, what, page):
        return self.select(self.moved(self.cursor, what, page, len(self.entries)))

    def move_result(self, what, page):
        return self.select(self.moved(self.cursor, what, page, len(self.entries)))

    def stop(self, commit):
        if commit and 0 <= self.cursor < len(self.entries):
            index, name = self.entries[self.cursor]
            logger.info("Browser: adding a chain: %s", name)
            self.ui.send("SELECT", [index])
            self.ui.send("SELECT_ACTION")
            # zynthian goes on with the engine screen (not a Clip Launcher: added at once)
            self.flow.follow(ENGINE_SCREEN, ADD_CHAIN_SCREEN)
        else:
            self.back()

    def collect(self, reporter, page_columns, page_results):
        names = [name for _, name in self.entries]
        report_session(reporter, self.content_type, False, {0: ("Chain", names, self.cursor)}, names, self.cursor,
                       page_columns, page_results)


class EngineSession(ScreenSession):
    """The engine screen: the categories as column 0, the category's engines as the results."""

    screen = ENGINE_SCREEN
    content_type = "Engine"

    def __init__(self, flow, lists):
        # Shown by zynthian after the Add Chain view: BACK returns to it
        super().__init__(flow, 1, True)
        self.categories, self.category, self.engines_of = lists
        self.engines = self.engines_of(self.category)
        self.result = 0 if self.engines else -1

    def move_column(self, what, page):
        if not self.categories:
            return True
        position = self.moved(self.category, what, page, len(self.categories))
        if position is None:
            return False
        delta = position - self.category
        for _ in range(abs(delta)):
            self.ui.send("ARROW_RIGHT" if delta > 0 else "ARROW_LEFT")
        if delta:
            # The screen shows the category with its first engine selected
            self.category = position
            self.engines = self.engines_of(position)
            self.result = 0 if self.engines else -1
        return True

    def move_result(self, what, page):
        position = self.moved(self.result, what, page, len(self.engines))
        if position is None:
            return False
        if self.engines and position != self.result:
            self.result = position
            self.ui.send("SELECT", [self.engines[position][0]])
        return True

    def stop(self, commit):
        if commit and 0 <= self.result < len(self.engines):
            index, name = self.engines[self.result]
            logger.info("Browser: engine %s", name)
            self.ui.send("SELECT", [index])
            self.ui.send("SELECT_ACTION")
        else:
            # zynthian's BACK: the Add Chain view again (no chain created yet)
            self.back()
            self.flow.follow(ADD_CHAIN_SCREEN, ENGINE_SCREEN)

    def collect(self, reporter, page_columns, page_results):
        columns = {0: ("Category", self.categories, self.category)} if self.categories else {}
        names = [name for _, name in self.engines]
        report_session(reporter, self.content_type, False, columns, names, self.result, page_columns, page_results)


class ChainFlow:
    """The chain sessions of one controller (``ZynBrowser.add_chain``), following zynthian's
    screens."""

    def __init__(self, model):
        self.model = model
        self.ui = None
        """zynthian's UI (``AddChainUI``), set by the driver; without it no session opens."""
        self.session = None
        self.pending = None
        """The screen to follow: [screen, the screen shown until then, reports left]."""
        self.add_chain_backs = 0
        self.gap_reported = True
        """``/browser/exists 0`` was reported since the last session closed."""

    @property
    def is_open(self):
        return self.session is not None

    @property
    def busy(self):
        """A session is open or follows."""
        return self.session is not None or self.pending is not None

    def available(self):
        return self.ui is not None and self.ui.available()

    def open(self):
        """Show zynthian's Add Chain view and browse its entries; False without a UI."""
        if self.is_open:
            return True
        if not self.available():
            return False
        self.pending = None
        # The screen before ADD_CHAIN: BACK returns to it
        self.add_chain_backs = 0 if self.ui.current_screen() == ADD_CHAIN_SCREEN else 1
        self.ui.send("ADD_CHAIN")
        # Its fill selects the first entry
        entries, _ = self.ui.add_chain_entries()
        self.session = AddChainSession(self, self.add_chain_backs, False, entries or list(enumerate(ENTRIES)), 0)
        logger.info("Browser: Add chain")
        return True

    def stop(self, commit):
        """``browser stop``; while a session follows (the core commits again when its browse mode
        is left) ignored."""
        session, self.session = self.session, None
        if session is not None:
            session.stop(commit)
        elif not commit:
            self.pending = None

    def close(self):
        """Cancel whatever is open, without following."""
        self.stop(False)
        self.pending = None

    def follow(self, screen, previous):
        self.pending = [screen, previous, FOLLOW_REPORTS]
        self.gap_reported = False

    def poll(self):
        """Before each report."""
        session = self.session
        if session is not None:
            if session.left():
                logger.info("Browser: %s closed by the UI", session.content_type)
                self.session = None
                # The UI went on (an entry tapped) or back (BACK on the engine screen)
                if isinstance(session, AddChainSession):
                    self.follow(ENGINE_SCREEN, ADD_CHAIN_SCREEN)
                else:
                    self.follow(ADD_CHAIN_SCREEN, ENGINE_SCREEN)
            else:
                session.refresh()
            return
        if self.pending is None:
            return
        screen, previous, left = self.pending
        current = self.ui.current_screen() if self.ui is not None else None
        if current == screen and self.gap_reported and self._open(screen):
            self.pending = None
        elif current in (screen, previous) and left > 0:
            self.pending[2] = left - 1
        else:
            self.pending = None

    def _open(self, screen):
        if screen == ENGINE_SCREEN:
            lists = self.ui.engine_lists()
            if lists is None:
                return False
            self.session = EngineSession(self, lists)
            logger.info("Browser: engines (%d categories)", len(lists[0]))
            return True
        entries, index = self.ui.add_chain_entries()
        if not entries:
            return False
        cursor = next((p for p, (i, _) in enumerate(entries) if i == index), 0)
        self.session = AddChainSession(self, self.add_chain_backs, True, entries, cursor)
        return True

    def command(self, cmd, page_columns, page_results):
        """``browser …`` while busy; False if not handled."""
        c = cmd.command
        if c == "stop":
            self.stop(cmd.flag)
            return True
        if c in ("preview", "contentType/+", "contentType/-"):
            return True
        if self.session is None:
            return cmd.parts[:1] in (["column"], ["result"])
        return self.session.command(cmd.parts, cmd.reset, page_columns, page_results)

    def collect(self, reporter, page_columns, page_results):
        if self.session is None:
            reporter.put(M.BROWSER_EXISTS, False)
            self.gap_reported = True
            return
        self.session.collect(reporter, page_columns, page_results)
