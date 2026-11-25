# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: chain control's grid as the host's pages
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

"""zynthian's chain control grid as the host's parameter pages (doc/HOST_PAGE_PLAN.md §12): a
controller with a grid and encoders (the Maschine JAM) shows the active chain's controller pages
as zynthian-ui's grid layouts do, with the selection shared with zynthian's screen.

It needs zynthian-ui's grid module (``zyngui.zynthian_control_grid_data`` with ``chain_pages``
and ``shared_selection``, the ``electra-grid`` branch): ``grid_module()``. Without it (a stock
zynthian) the host-page commands stay undeclared (commands.py) and the controller has no host
source.

- **The grid**: a ``ControlGrid`` of its own (``grid8x6`` for an 8-column surface, else
  ``grid4x6``; paged: windows of whole 6 rows), on the driver's tick thread (the grid model is
  not thread safe). Its pages are ``chain_pages`` of the active chain, the same as the screen's;
  they are read again with every full pass while shown and set when they differ.
- **The selection** is zynthian's (``shared_selection``, owned by chain control's screen): the
  grid shows the shared page and cell of the active chain (or, if the screen's selection is
  another chain's, chain_pages' default: the current processor's current screen). Selecting
  commands become requests to the screen (applied in its GUI thread); the result comes back as a
  zynsigman signal (bridge.py: a full pass). The core never shows a selection that was not
  reported.
- **Reports** (``collect_full``: structure, values and selection; ``collect_fast``: the values
  and texts of the shown cells that changed, by ``ValueWatcher``), only while a surface shows
  them (``HostPageShow`` on). The epoch (first) counts the changes of which controller a cell
  holds (chain, pages, window, layout).
- **Commands** (``show``, ``turn``, ``press``, ``reset``, ``assign``, ``select_row``/``page``/
  ``set``/``section``): cell commands of another epoch are dropped. The value math is the grid's:
  a detent ``turn`` (zctrl.nudge), a strip's travel ``turn_travel``. A press on a page without
  controllers (a processor without pages) is the screen's tap on its header
  (``shared_selection.request_open``: selects it, the selected one opens its banks / presets).
- **zynthian's screen follows the surface**: ``show`` on opens chain control of the active chain
  (``shared_selection.request_show``; not over a menu or dialog, the screen decides), on the
  surface's selected page when it wasn't shown; the navigation commands and a press bring it to
  the front again if the user went elsewhere; turns and ``show`` off don't change the screen.
- **Chain controllers** (zynthian-ui docs/chain-controller-pages.md): the chain controllers'
  pages (named pages of 4 slots) are the first section of ``chain_pages`` ("Chain"), so they are
  reported as any other pages, in a colour of their own (zynthian-ui's ``page_color``, through
  the existing colour fields); ``assign`` (``HostPageCellAssign``, the JAM's hold-a-row +
  touch-a-strip, slots 0-7) puts a cell's controller into slot n % 4 of page current + n // 4
  (strips 5-8 the next page, created as needed).
- **Errors**: a chain whose pages fail to read is reported as none (``exists`` false, logged
  once); a failing command is logged (commands.py), the driver goes on.

Cell kinds: empty 0, float/integer 1 (fader), list 2, toggle 3, trigger 4, path 6 (action).
"""

import importlib
import logging

from dbm_host.reporter import M

logger = logging.getLogger("dbm_zynthian")

KINDS = {"empty": 0, "float": 1, "integer": 1, "list": 2, "toggle": 3, "trigger": 4, "path": 6}
EMPTY_COLOR = (0, 0, 0)

# The keys of a cell's values (cell << 16), built once
CELL_VALUES = (M.HOST_CELL_KIND, M.HOST_CELL_NAME, M.HOST_CELL_COLOR, M.HOST_CELL_VALUE, M.HOST_CELL_TEXT,
               M.HOST_CELL_STEPS, M.HOST_CELL_ON, M.HOST_CELL_ENABLED, M.HOST_CELL_SET)


def grid_module():
    """zynthian-ui's grid module with the shared page list and selection, None without it"""
    try:
        gd = importlib.import_module("zyngui.zynthian_control_grid_data")
    except Exception:
        return None
    needed = ("chain_pages", "shared_selection", "section_color", "first_slot", "ControlGrid", "ValueWatcher")
    if not all(hasattr(gd, name) for name in needed) or not hasattr(gd.ControlGrid, "turn_travel"):
        return None
    return gd


def cell_steps(gd, zctrl, kind):
    """The values of a stepped cell (a list's items, a toggle's 2, an integer's range), else 0"""
    if kind not in ("list", "toggle", "integer"):
        return 0
    steps = gd.cell_steps(zctrl) if hasattr(gd, "cell_steps") else None
    return len(steps) if steps is not None else 0


class ZynHostPages:
    """The host's pages of one controller (one per bridge)"""

    def __init__(self, chain_manager, gd):
        self.chain_manager = chain_manager
        self.gd = gd
        self.grid = gd.ControlGrid("grid8x6", paged=True)
        self.watcher = gd.ValueWatcher()
        self.shown = False
        self.epoch = 0
        self.chain = None
        self._signature = None
        self._structure = None
        self._keys = [tuple(k | cell << 16 for k in CELL_VALUES) for cell in range(64)]
        self.dropped = 0
        """Cell commands of another epoch (tests)."""
        self._failed_chain = None

    # The model

    @property
    def chain_id(self):
        return self.chain.chain_id if self.chain is not None else None

    def update(self):
        """Read the active chain's pages and the shared selection into the grid"""
        gd = self.gd
        chain = self.chain_manager.get_active_chain()
        self.chain = chain
        if chain is None:
            if self.grid.pages:
                self.grid.set_pages([])
            self._signature = None
        else:
            try:
                processors = chain.get_processors()
            except Exception:
                processors = []
            pages, default = gd.chain_pages(chain, processors, getattr(chain, "current_processor", None))
            signature = (chain.chain_id, tuple((p.title, p.section, p.section_index, tuple(id(z) for z in p.zctrls)) for p in pages))
            selection = gd.shared_selection.get(chain.chain_id)
            page, slot = (selection.page, selection.slot) if selection is not None and 0 <= selection.page < len(pages) else (default, -1)
            slot = slot if slot >= 0 else None
            if signature != self._signature:
                self._signature = signature
                self.grid.set_pages(pages, page, slot)
            elif page != self.grid.selected:
                self.grid.select(page, slot)
            elif slot is not None:
                self.grid.select_slot(slot)
            elif self.grid.selected >= 0:
                first = gd.first_slot(self.grid.pages[self.grid.selected])
                if first >= 0:
                    self.grid.select_slot(first)
        structure = (self.chain_id, self._signature, self.grid.first_row, self.grid.layout.name)
        if structure != self._structure:
            self._structure = structure
            self.epoch = (self.epoch + 1) & 0xFFFF
            self.watcher.reset()

    # Reports

    def collect_full(self, put):
        """Everything (the full pass): the structure, the values, the selection. A chain whose
        pages can't be read (an exception from zynthian's objects) is reported as none
        (``exists`` false) and logged once per chain, never failing the pass"""
        if not self.shown:
            return
        try:
            self.update()
        except Exception as e:
            chain = self.chain_id
            if self._failed_chain != chain:
                self._failed_chain = chain
                logger.exception("host pages of chain %s: %s", chain, e)
                # No cell of the pages before stays addressable
                self.epoch = (self.epoch + 1) & 0xFFFF
            self.chain = None
            self._signature = None
            if self.grid.pages:
                self.grid.set_pages([])
            self._structure = None
            put(M.HOST_PAGE_EPOCH, self.epoch)
            put(M.HOST_PAGE_EXISTS, False)
            return
        self._failed_chain = None
        gd = self.gd
        grid = self.grid
        chain = self.chain
        put(M.HOST_PAGE_EPOCH, self.epoch)
        put(M.HOST_PAGE_EXISTS, chain is not None)
        title = ""
        if chain is not None:
            try:
                title = chain.get_title()
            except Exception:
                title = ""
        put(M.HOST_PAGE_TITLE, f"zynthian · {title}" if title else "zynthian")
        put(M.HOST_PAGE_COLUMNS, grid.columns)
        put(M.HOST_PAGE_ROWS, grid.rows)
        pages = grid.pages
        windows = grid.window_count if pages else 0
        put(M.HOST_PAGE_COUNT, windows)
        per_window = grid.rows * grid.pages_per_row
        for w in range(windows):
            first = pages[w * per_window]
            put(M.HOST_PAGE_NAME | w << 16, f"{first.section} {first.title}" if first.section != first.title else first.title)
        put(M.HOST_PAGE_SELECTED, grid.window)
        # The sections: the chain controllers, the processors (by section_index); colours as
        # the screen's (page_color: the chain controllers' own, the processors' from 0)
        sections = {}
        colors = {}
        for page in pages:
            sections.setdefault(page.section_index, page.section)
            colors.setdefault(page.section_index, self.page_color(page))
        count = max(sections) + 1 if sections else 0
        put(M.HOST_SECTION_COUNT, count)
        for s in range(count):
            put(M.HOST_SECTION_NAME | s << 16, sections.get(s, ""))
            put(M.HOST_SECTION_COLOR | s << 16, colors.get(s) or gd.section_color(s))
        shown = grid.shown_pages
        put(M.HOST_SET_COUNT, len(shown))
        for i, p in enumerate(shown):
            page = pages[p]
            put(M.HOST_SET_NAME | i << 16, page.title)
            put(M.HOST_SET_SECTION | i << 16, page.section_index)
            put(M.HOST_SET_COLOR | i << 16, self.page_color(page))
        first_page = grid.first_page
        put(M.HOST_SET_SELECTED, grid.selected - first_page if grid.selected in shown else -1)
        put(M.HOST_SECTION_SELECTED, pages[grid.selected].section_index if 0 <= grid.selected < len(pages) else -1)
        for cell in grid.cells:
            self.put_cell(put, cell, first_page)
        self.watcher.changed(grid.cells)
        put(M.HOST_PAGE_ROW, max(0, grid.selected_row))
        put(M.HOST_CELL_SELECTED, grid.selected_cell)

    def page_color(self, page):
        """A page's colour as the screen's: zynthian-ui's page_color (the chain controllers' own
        colour, the processors' from 0), else (an older zynthian-ui) its section's"""
        page_color = getattr(self.gd, "page_color", None)
        return page_color(page) if page_color is not None else self.gd.section_color(page.section_index)

    def put_cell(self, put, cell, first_page):
        kind_key, name, color, value, text, steps, on, enabled, set_key = self._keys[cell.id]
        z = cell.zctrl
        page = cell.page
        if z is None:
            put(kind_key, 0)
            put(name, "")
            put(enabled, False)
            put(set_key, page - first_page if page >= 0 else -1)
            put(color, self.page_color(self.grid.pages[page]) if page >= 0 else EMPTY_COLOR)
            return
        kind = cell.kind
        put(kind_key, KINDS.get(kind, 1))
        put(name, cell.name)
        put(color, self.page_color(self.grid.pages[page]))
        self.put_value(put, cell, kind)
        put(steps, cell_steps(self.gd, z, kind))
        put(enabled, cell.enabled)
        put(set_key, page - first_page)

    def put_value(self, put, cell, kind=None):
        _, _, _, value, text, _, on, _, _ = self._keys[cell.id]
        z = cell.zctrl
        kind = kind or cell.kind
        put(value, float(cell.fraction))
        put(text, cell.value_text)
        try:
            put(on, kind == "toggle" and z.value != z.value_min)
        except Exception:
            put(on, False)

    def collect_fast(self, put):
        """The values and texts of the shown cells that changed (the fast pass)"""
        if not self.shown:
            return
        for cell in self.watcher.changed(self.grid.cells):
            self.put_value(put, cell)

    # Commands (the tick thread)

    def show(self, columns, on):
        """``hostpage show``: the layout for the surface's shape (8 columns: grid8x6, else
        grid4x6); True when the reports start (the caller forgets the old ones: a dump)"""
        self.shown = bool(on)
        if not on:
            return False   # the screen stays where it is
        self.grid.set_layout("grid8x6" if columns >= 8 else "grid4x6")
        self._structure = None
        self.watcher.reset()
        self.open_screen()
        return True

    def _front(self):
        """Chain control of the active chain to the front of zynthian's screen if it isn't shown
        (shared_selection.request_show; an older zynthian-ui or no screen: nothing); True if the
        screen got the request"""
        request_show = getattr(self.gd.shared_selection, "request_show", None)
        if request_show is None or self.chain_id is None:
            return False
        return bool(request_show(self.chain_id))

    def open_screen(self):
        """The surface switched to the host's pages: zynthian's screen opens chain control of the
        active chain, on the page the surface shows (its selection: the shared one, else the
        chain's default) when it wasn't shown"""
        try:
            self.update()
        except Exception as e:
            logger.warning("host pages: the active chain's pages: %s", e)
            return
        selection = self.gd.shared_selection
        owner_shown = getattr(selection, "owner_shown", None)
        was_shown = owner_shown() if owner_shown is not None else True
        if self._front() and not was_shown and 0 <= self.grid.selected < len(self.grid.pages):
            selection.request_page(self.chain_id, self.grid.selected)

    def _cell(self, cell, epoch):
        if epoch != self.epoch:
            self.dropped += 1
            return None
        return self.grid.cell(cell)

    def turn(self, cell, epoch, travel, fine, delta):
        if self._cell(cell, epoch) is None:
            return
        if travel:
            self.grid.turn_travel(cell, delta, fine)
        else:
            self.grid.turn(cell, delta, fine)

    def press(self, cell, epoch, pressed):
        c = self._cell(cell, epoch)
        if c is None:
            return
        if not pressed:
            self.grid.release(cell)
            return
        if c.page < 0:
            return
        # Its page and the cell on the screen, then by its kind (a toggle switches, a trigger
        # fires, a list steps, a file opens the file selector; a continuous one only selects).
        # A page without controllers (a processor without pages) as a tap on its header on the
        # screen: selects it, the selected one opens its banks / presets
        selection = self.gd.shared_selection
        self._front()
        if c.zctrl is not None:
            selection.request_cell(self.chain_id, c.page, c.slot)
        elif self.grid.pages[c.page].empty and hasattr(selection, "request_open"):
            selection.request_open(self.chain_id, c.page)
        else:
            selection.request_page(self.chain_id, c.page)
        self.grid.press(cell)

    def reset(self, cell, epoch):
        if self._cell(cell, epoch) is not None:
            self.grid.set_default(cell)

    def assign(self, cell, epoch, slot):
        """``hostpage cell/{cell}/assign/{epoch}`` (zynthian-ui docs/chain-controller-pages.md §6):
        the cell's controller to chain controller slot ``slot`` (0-7, the JAM's strips 1-8):
        zynthian's pages have 4 slots, so slot n is slot n % 4 of page current + n // 4 (strips
        5-8 the page after the current one; the pages up to it created; an empty cell clears the
        slot; on that page already: moved); slot -1: removed from all pages. Applied in
        zynthian's GUI thread (the screen's pages are rebuilt there); the change signal
        (SS_GUI_CONTROL_GRID_ZCTRL_PAGES) brings the new pages"""
        c = self._cell(cell, epoch)
        selection = self.gd.shared_selection
        if c is None or self.chain is None or not hasattr(selection, "request_zctrl_assign"):
            return False
        if not -1 <= slot < 8 or slot < 0 and c.zctrl is None:
            return False
        selection.request_zctrl_assign(self.chain, c.zctrl, slot)
        return True

    def show_processor_screen(self, processor, screen):
        """The core selected a device page (``DeviceParameterPage``: the JAM's < > in CONTROL):
        while chain control is shown, its page of that processor's controller screen (the
        device page's first screen, device.py) is selected through the shared selection, so the
        screen's selection follows. True if requested"""
        gd = self.gd
        selection = gd.shared_selection
        owner_shown = getattr(selection, "owner_shown", None)
        chain = self.chain_manager.get_active_chain()
        if owner_shown is None or not owner_shown() or chain is None or screen is None:
            return False
        try:
            processors = chain.get_processors()
            items, _ = gd.chain_page_items(chain, processors, getattr(chain, "current_processor", None))
            _, rows = gd.grid_pages(items)
        except Exception as e:
            logger.warning("chain control's page of %s: %s", processor, e)
            return False
        for i, row in enumerate(rows):
            kind, proc, arg = items[row].data
            if kind == "proc" and proc is processor and arg == screen:
                selection.request_page(chain.chain_id, i)
                return True
        return False

    def _select(self, page):
        """A navigation command (row, window, set, section): the page, chain control to the front"""
        if 0 <= page < len(self.grid.pages):
            self._front()
            self.gd.shared_selection.request_page(self.chain_id, page)

    def select_row(self, row):
        """The page in that row of the window (with two a row: the same half as before, the left
        one if there is none)"""
        grid = self.grid
        page = grid.page_at(row, grid.selected_half)
        self._select(page if page >= 0 else grid.page_at(row, 0))

    def select_window(self, window, last=False):
        """Show that window: select its first page, or with ``last`` its last one (``<`` from a
        window's first set: ``HostPagePage`` with ``PageEnd.LAST``)"""
        grid = self.grid
        if 0 <= window < grid.window_count:
            first = window * grid.rows * grid.pages_per_row
            if last:
                first = min(first + grid.rows * grid.pages_per_row, len(grid.pages)) - 1
            self._select(first)

    def select_set(self, index):
        shown = self.grid.shown_pages
        if 0 <= index < len(shown):
            self._select(shown[index])

    def select_section(self, section):
        self._select(next((i for i, p in enumerate(self.grid.pages) if p.section_index == section), -1))
