# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: the core's browser on the banks and presets of a processor
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

"""The core's browser (doc/HOST_BRIDGE_GUIDE.md §3.8, §4.9) on the banks and presets of the
cursor device (the active chain's current processor, device.py).

| Core | zynthian |
| --- | --- |
| ``browser replace/device`` (BROWSE) | open on the cursor device's processor; zynthian's preset view is shown (``BrowserUI``) |
| content type | one: "Presets" (no device browser) |
| filter column 0 "Bank" | ``processor.get_bank_list()`` without its section titles (rows without id or name); the favourites ("*** Favorites ***") are a bank |
| filter columns 1..6 | do not exist |
| results | the bank's presets (``processor.preset_list`` after ``load_preset_list``) without section titles, "❤ " removed |
| cursor of a column / result | the bank (at the start the processor's) / the preset (at the start the loaded one, else the bank's first) |
| ``+``, ``-``, ``page/+``, ``page/-``, ``reset`` | the cursor ±1, ±a page (``numFilterColumnEntries``, ``numResults``), the first bank; the page shown always holds the cursor |
| ``preview`` | zynthian's preset preload (``preload_preset``) of the preset under the cursor, also when the bank changes (the preset the cursor lands on); at the start on if zynthian's "Preset Preload" is |
| ``stop 1`` (commit) | the bank (``set_bank``) and the preset (``set_preset``) are loaded |
| ``stop 0`` (cancel) | the preset preloaded for the preview is undone (``restore_preset``, to the state with the processor's own bank, ``_preload``), the processor keeps its bank and preset |

While browsing, the processor's bank is the browsed one without loading it into the engine
(``set_bank(index, set_engine=False)``, as zynthian's bank screen before a preset is chosen):
zynthian's preset view then lists the browsed bank. Cancel restores the processor's bank, its
favourites flag and its preset list.

Not mapped (reported absent or logged as not handled): the device browser (``browser
add/…``, ``insertBefore``, ``insertAfter``, ``replace/slot/…``, ``replace/drum|primary/pad/…``),
the filter columns 1..6, the hit counts (0), banks in sub-folders (``bank_subdir_info``: such a
bank lists what its engine lists without the folder level).
"""

import copy
import logging

logger = logging.getLogger("dbm_zynthian")

NUM_COLUMNS = 7
BANK_COLUMN = 0
CONTENT_TYPE = "Presets"
BANK_COLUMN_NAME = "Bank"
FAVS_ID = "*FAVS*"
FAV_MARK = "❤"


def row_name(row):
    """The title of a bank or preset row, without the favourite mark."""
    name = row[2] if len(row) > 2 and row[2] else ""
    if name.startswith(FAV_MARK):
        name = name[len(FAV_MARK):].lstrip()
    return name


def selectable(row):
    """Rows without id (section titles) or without name can't be chosen."""
    return isinstance(row, (list, tuple)) and len(row) > 2 and row[0] is not None and bool(row[2])


def preset_preload_enabled():
    try:
        from zyngui import zynthian_gui_config
        return bool(getattr(zynthian_gui_config, "preset_preload", 0))
    except Exception:
        return False


class ZynBrowser:
    """The browser of one controller (the model of its bridge)."""

    def __init__(self, model):
        self.model = model
        self.ui = None
        """The preset view of zynthian's UI (``BrowserUI``), None: none is shown."""
        self.processor = None
        """The processor browsed, None while the browser is closed."""
        self.banks = []
        """The banks: (index in the processor's bank_list, row)."""
        self.bank = -1
        self.presets = []
        """The presets of the bank: (index in the processor's preset_list, row)."""
        self.result = -1
        self.preview = preset_preload_enabled()
        self.saved = None
        """The processor's bank, favourites flag, preset list and preset when the browser opened."""

    @property
    def is_open(self):
        return self.processor is not None

    # Opening and closing

    def open(self, processor):
        """Browse the banks and presets of the processor; False if it has none."""
        if self.is_open:
            self.stop(False)
        try:
            bank_list = list(processor.get_bank_list() or [])
        except Exception as e:
            logger.warning("No banks of %s: %s", getattr(processor, "name", processor), e)
            bank_list = []
        banks = [(index, row) for index, row in enumerate(bank_list) if selectable(row)]
        if not banks and not [row for row in processor.preset_list if selectable(row)]:
            logger.info("Browser: %s has no banks and no presets", getattr(processor, "name", processor))
            return False
        self.processor = processor
        self.saved = {
            "bank_index": processor.bank_index,
            "bank_name": processor.bank_name,
            "bank_info": copy.deepcopy(processor.bank_info),
            "show_fav_presets": processor.show_fav_presets,
            "preset_list": list(processor.preset_list),
            "preset_info": copy.deepcopy(processor.preset_info),
        }
        self.banks = banks
        self.bank = self._current_bank()
        self._apply_bank()
        if self.ui is not None:
            self.ui.open(processor)
            self._show(refill=False)
        return True

    def _current_bank(self):
        if not self.banks:
            return -1
        processor = self.processor
        for position, (index, row) in enumerate(self.banks):
            if processor.show_fav_presets and row[0] == FAVS_ID:
                return position
        current = processor.get_bank_index()
        for position, (index, row) in enumerate(self.banks):
            if index == current and row[0] != FAVS_ID:
                return position
        return 0

    def stop(self, commit):
        """``browser stop``: load the selected preset (commit) or keep the processor's one."""
        if not self.is_open:
            return
        try:
            if commit:
                self._commit()
            else:
                self._cancel()
        finally:
            self._close()

    def _close(self):
        self.processor = None
        self.saved = None
        self.banks, self.bank = [], -1
        self.presets, self.result = [], -1
        if self.ui is not None:
            self.ui.close()

    def _restore_bank(self):
        processor, saved = self.processor, self.saved
        processor.bank_index = saved["bank_index"]
        processor.bank_name = saved["bank_name"]
        processor.bank_info = copy.deepcopy(saved["bank_info"])
        processor.show_fav_presets = saved["show_fav_presets"]
        processor.preset_list = list(saved["preset_list"])

    def _commit(self):
        processor = self.processor
        if not 0 <= self.result < len(self.presets):
            self._cancel()
            return
        _, row = self.presets[self.result]
        preset_id = row[0]
        bank_index = self._real_bank_index(row)
        # The processor's own bank first: set_bank loads the bank into the engine only if it
        # differs from that one
        self._restore_bank()
        if bank_index is not None:
            processor.set_show_fav_presets(False)
            processor.set_bank(bank_index)
            processor.load_preset_list()
        index = processor.find_preset_index_by_id(preset_id)
        if index is None:
            logger.warning("Browser: preset %s not found in %s", row_name(row), processor.get_bank_name())
            return
        logger.info("Browser: loading %s / %s", processor.get_bank_name(), row_name(row))
        processor.set_preset(index)

    def _real_bank_index(self, row):
        """The bank_list index of the preset's bank (a favourite: the bank it is from)."""
        if not 0 <= self.bank < len(self.banks):
            return None
        index, bank_row = self.banks[self.bank]
        if bank_row[0] != FAVS_ID:
            return index
        try:
            fav_bank = self.processor.get_preset_favs()[str(row[0])][0]
        except (KeyError, IndexError, TypeError):
            return None
        for index, bank_row in enumerate(self.processor.bank_list):
            if selectable(bank_row) and bank_row[0] != FAVS_ID and bank_row[2] == fav_bank[2]:
                return index
        return None

    def _cancel(self):
        if self.processor.preload_info is not None:
            # Back to the state before the first preload: the processor's bank and preset
            # (_preload), into the engine
            self.processor.restore_preset()
        self._restore_bank()

    # The UI

    def poll_ui(self):
        """Called before each report: the user left zynthian's preset view (BACK, or chose a
        preset there): the browser closes, with the preset the processor has then."""
        if not self.is_open or self.ui is None or not self.ui.left():
            return
        processor = self.processor
        self.ui.forget()
        if processor.preset_info == self.saved["preset_info"]:
            self.stop(False)
            return
        # A preset of the browsed bank was chosen in the UI: that bank stays, now also the
        # engine's (it was set without the engine while browsing)
        logger.info("Browser: closed by the UI, %s chosen there", processor.get_preset_name())
        if processor.bank_info and processor.bank_info != self.saved["bank_info"]:
            try:
                processor.engine.set_bank(processor, processor.bank_info)
            except Exception as e:
                logger.warning("Browser: can't set the bank: %s", e)
        self._close()

    def _show(self, refill):
        if self.ui is not None and 0 <= self.result < len(self.presets):
            self.ui.show(self.presets[self.result][0], refill)
        elif self.ui is not None and refill:
            self.ui.show(None, refill)

    # The bank and the presets

    def _apply_bank(self):
        """Make the browsed bank the processor's (without the engine) and read its presets."""
        processor = self.processor
        if 0 <= self.bank < len(self.banks):
            index, row = self.banks[self.bank]
            if row[0] == FAVS_ID:
                processor.set_show_fav_presets(True)
            else:
                processor.set_show_fav_presets(False)
                processor.set_bank(index, set_engine=False)
            try:
                processor.load_preset_list()
            except Exception as e:
                logger.warning("Browser: no presets of %s: %s", row_name(row), e)
                processor.preset_list = []
        self.presets = [(index, row) for index, row in enumerate(processor.preset_list) if selectable(row)]
        current = self.saved["preset_info"]
        self.result = 0 if self.presets else -1
        if current:
            for position, (index, row) in enumerate(self.presets):
                if row[0] == current[0]:
                    self.result = position
                    break

    def select_bank(self, position):
        if not self.banks:
            return
        position = max(0, min(position, len(self.banks) - 1))
        if position == self.bank:
            return
        self.bank = position
        self._apply_bank()
        # The preview follows the cursor into the bank (its first preset, or the loaded one):
        # a bank with a single preset can be listened to as well
        if self.preview:
            self._preload()
        self._show(refill=True)

    def select_result(self, position):
        if not self.presets:
            return
        position = max(0, min(position, len(self.presets) - 1))
        if position == self.result:
            return
        self.result = position
        if self.preview:
            self._preload()
        self._show(refill=False)

    def _preload(self):
        if not 0 <= self.result < len(self.presets):
            return
        processor = self.processor
        try:
            if processor.preload_state is None:
                # The state restore_preset goes back to: the processor's, with its own bank
                # instead of the browsed one (set without the engine), so restore_preset sets
                # that bank and its preset in the engine
                state = processor.get_state()
                state["bank_info"] = copy.deepcopy(self.saved["bank_info"])
                state["show_fav_presets"] = self.saved["show_fav_presets"]
                processor.preload_state = state
            processor.preload_preset(self.presets[self.result][0])
        except Exception as e:
            logger.warning("Browser: preload failed: %s", e)
        finally:
            if processor.preload_info is None:
                processor.preload_state = None  # Nothing preloaded

    def set_preview(self, preview):
        self.preview = bool(preview)
        if not self.is_open:
            return
        if self.preview:
            self._preload()
        elif self.processor.preload_info is not None:
            self.processor.restore_preset()
            # restore_preset set the processor's own bank (_preload): browse on in the browsed one
            self._apply_bank()

    # The core's commands

    def command(self, cmd, page_columns, page_results):
        """``browser …`` (§4.9); False if not handled."""
        c = cmd.command
        parts = cmd.parts
        if c == "replace/device":
            device = self.model.devices.cursor()
            if device is None:
                return False
            if device.processor is not getattr(self.model.devices.chain, "current_processor", None):
                self.model.devices.select(device)
            self.open(device.processor)
            return True
        if c == "stop":
            self.stop(cmd.flag)
            return True
        if c == "preview":
            self.set_preview(cmd.flag)
            return True
        if c in ("contentType/+", "contentType/-"):
            return True  # One content type
        if not self.is_open:
            return parts[:1] in (["column"], ["result"])
        if len(parts) >= 2 and parts[0] == "column":
            if parts[1] != str(BANK_COLUMN):
                return False
            # "column/<c>/reset": the reset suffix of the value commands (Command.parse)
            what = "reset" if cmd.reset and len(parts) == 2 else "/".join(parts[2:])
            return self._move(self.select_bank, self.bank, what, page_columns, reset=True)
        if len(parts) >= 2 and parts[0] == "result":
            return self._move(self.select_result, self.result, "/".join(parts[1:]), page_results, reset=False)
        return False

    @staticmethod
    def _move(select, current, what, page, reset):
        steps = {"+": 1, "-": -1, "page/+": page, "page/-": -page}
        if what in steps:
            select(current + steps[what])
        elif what == "reset" and reset:
            select(0)
        else:
            return False
        return True

    # The reports (§3.8)

    def collect(self, reporter, page_columns, page_results):
        self.poll_ui()
        reporter.put("/browser/exists", self.is_open)
        if not self.is_open:
            return
        reporter.put("/browser/preview", self.preview)
        reporter.put("/browser/contentType/count", 1)
        reporter.put("/browser/contentType/0/name", CONTENT_TYPE)
        reporter.put("/browser/contentType/selected", 0)
        reporter.put("/browser/contentType/selected/name", CONTENT_TYPE)
        reporter.put("/browser/column/count", NUM_COLUMNS)
        for column in range(NUM_COLUMNS):
            prefix = f"/browser/column/{column}/"
            exists = column == BANK_COLUMN
            items = [row for _, row in self.banks] if exists else []
            cursor = self.bank if exists else -1
            self._collect_items(reporter, prefix + "item/", items, cursor, page_columns, hits=True)
            reporter.put(prefix + "exists", exists)
            reporter.put(prefix + "name", BANK_COLUMN_NAME if exists else "")
            reporter.put(prefix + "wildcard", "")
            reporter.put(prefix + "cursor/exists", 0 <= cursor < len(items))
            reporter.put(prefix + "cursor/name", row_name(items[cursor]) if 0 <= cursor < len(items) else "")
            reporter.put(prefix + "cursorIndex", cursor % page_columns if 0 <= cursor < len(items) and page_columns else -1)
        rows = [row for _, row in self.presets]
        self._collect_items(reporter, "/browser/result/", rows, self.result, page_results, hits=False)
        reporter.put("/browser/result/cursor/name", row_name(rows[self.result]) if 0 <= self.result < len(rows) else "")

    @staticmethod
    def _collect_items(reporter, prefix, rows, cursor, page, hits):
        """The page of the items which holds the cursor."""
        start = cursor // page * page if page and cursor > 0 else 0
        for i in range(page):
            position = start + i
            exists = position < len(rows)
            item = f"{prefix}{i}/"
            reporter.put(item + "exists", exists)
            reporter.put(item + "name", row_name(rows[position]) if exists else "")
            reporter.put(item + "select", exists and position == cursor)
            if hits:
                reporter.put(item + "hits", 0)


class BrowserUI:
    """zynthian's preset view while the core browses (the Qt UI and the Tk UI), through CUIAs
    only (the state manager's queue, run in the UI's thread):

    - open: ``CHAIN_CONTROL`` (if the UI's current processor is not the browsed one: it becomes
      the active chain's current processor), then ``SHOW_SCREEN preset``;
    - another bank: ``SHOW_SCREEN preset`` again (the screen reads the processor's preset list
      anew), the selection: ``SELECT <index in the preset list>`` (moves the cursor, loads
      nothing);
    - close: ``BACK`` as often as screens were opened, if the preset view is still shown (or not
      shown yet).

    The UI's own selection is not read back: the core's selection only flows to the UI. If the
    user leaves the preset view (BACK, a preset chosen there), ``left`` says so and the browser
    closes (``ZynBrowser.poll_ui``)."""

    PRESET_SCREEN = "preset"

    def __init__(self, state_manager, get_ui=None):
        self.state_manager = state_manager
        self.get_ui = get_ui or self._zyngui
        self.backs = 0
        self.active = False
        self.seen = False

    @staticmethod
    def _zyngui():
        try:
            from zyngui import zynthian_gui_config
            return getattr(zynthian_gui_config, "zyngui", None)
        except Exception:
            return None

    def send(self, cuia, params=None):
        self.state_manager.send_cuia(cuia, params)

    def current_screen(self):
        ui = self.get_ui()
        return getattr(ui, "current_screen", None) if ui is not None else None

    def open(self, processor):
        ui = self.get_ui()
        self.backs = 0
        self.seen = False
        self.active = ui is not None
        if not self.active:
            return
        screen = getattr(ui, "current_screen", None)
        if getattr(ui, "current_processor", None) is not processor:
            self.send("CHAIN_CONTROL")
            if screen != "chain_control":
                self.backs += 1
            screen = "chain_control"
        self.send("SHOW_SCREEN", [self.PRESET_SCREEN])
        if screen != self.PRESET_SCREEN:
            self.backs += 1

    def show(self, index, refill):
        if not self.active:
            return
        if refill:
            self.send("SHOW_SCREEN", [self.PRESET_SCREEN])
        if index is not None:
            self.send("SELECT", [index])

    def close(self):
        if not self.active:
            return
        self.active = False
        if not self.seen or self.current_screen() == self.PRESET_SCREEN:
            for _ in range(self.backs):
                self.send("BACK")
        self.backs = 0

    def forget(self):
        """The user left the preset view: nothing to close."""
        self.active = False
        self.backs = 0

    def left(self):
        if not self.active:
            return False
        if self.current_screen() == self.PRESET_SCREEN:
            self.seen = True
            return False
        return self.seen
