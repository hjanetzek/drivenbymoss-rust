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

"""The host's pages (dbm_zynthian/host_page.py, doc/HOST_PAGE_PLAN.md §12) with zynthian-ui's
grid module (the ``electra-grid`` branch: ``$ZYNTHIAN_UI_DIR`` must be such a checkout, else the
tests are skipped): the reports, the commands with their epochs and the shared selection (with a
fake screen as its owner, or none), the handled commands, and end to end with the Maschine JAM in
the core as the reference host's tests (bindings/python/tests/test_host_page.py)."""

import logging
import sys
import tempfile
import unittest

import zynstubs

zynstubs.install()

import drivenbymoss  # noqa: E402
from dbm_host import Banks  # noqa: E402
from drivenbymoss import protocol  # noqa: E402

GRID_FILE = zynstubs.UI_DIR / "zyngui" / "zynthian_control_grid_data.py"
GRID_MODULE = "zyngui.zynthian_control_grid_data"
M = protocol.Msg
JAM = "72F32A83-A697-446B-9016-64AA3F9476E4"


def grid_available():
    from zyngine.zynthian_signal_manager import zynsigman
    return GRID_FILE.exists() and hasattr(zynsigman, "SS_GUI_CONTROL_GRID_PAGE")


class GridModule:
    """zyngui's grid module importable while a test runs (zynstubs' zyngui has no path: the
    other tests see a stock zynthian)"""

    def __enter__(self):
        import zyngui
        self.path = str(GRID_FILE.parent)
        zyngui.__path__.append(self.path)
        sys.modules.pop(GRID_MODULE, None)
        import importlib
        self.gd = importlib.import_module(GRID_MODULE)
        self.gd.shared_selection = self.gd.SharedSelection()
        return self.gd

    def __exit__(self, *args):
        import zyngui
        zyngui.__path__.remove(self.path)
        sys.modules.pop(GRID_MODULE, None)
        if hasattr(zyngui, "zynthian_control_grid_data"):
            del zyngui.zynthian_control_grid_data


def synth_state(screens=14):
    """zynstubs' project with a synth of 14 screens and an effect on the active chain (1)"""
    state = zynstubs.project()
    screens = [(f"S{i}", [f"p{i}0", f"mode{i}", f"toggle{i}", f"float{i}"]) for i in range(screens)]
    state.synth = state.add_processor(1, "Helm", screens)
    state.fx = state.add_processor(1, "Reverb", [("Room", ["steps0", "float0"])], type="Audio Effect")
    return state


def handler(state):
    from zyngine.ctrldev.dbm_zynthian.commands import ZynCommandHandler
    from zyngine.ctrldev.dbm_zynthian.model import ZynthianModel
    from zyngine.ctrldev.dbm_zynthian.reporter import ZynReporter
    model = ZynthianModel(state.chain_manager, palette=["#0000FF"], zynseq=None)
    banks = Banks.from_model_setup({"numTracks": 8, "numSends": 2})
    commands = ZynCommandHandler(model, banks, ZynReporter(model, banks))
    commands.configure()
    return commands


class FakeScreen:
    """Chain control's screen as the shared selection's owner: requests are queued (its GUI
    thread) until apply(), which selects and reports as the Qt screen does"""

    def __init__(self, gd, state):
        self.gd = gd
        self.state = state
        self.queue = []
        self.first_row = 0
        gd.shared_selection.set_owner(self)

    def grid_selection_request(self, *args):
        self.queue.append(args)

    def apply(self):
        gd = self.gd
        for kind, chain_id, page, slot, first_row in self.queue:
            chain = self.state.chain_manager.get_active_chain()
            if chain.chain_id != chain_id:
                continue
            pages, _ = gd.chain_pages(chain, chain.get_processors(), chain.current_processor)
            if kind == "window" or not 0 <= page < len(pages):
                continue
            if kind == "page" or slot < 0:
                slot = gd.first_slot(pages[page])
            # The screen (grid4x6, by rows) scrolls to show the selected page
            if page < self.first_row:
                self.first_row = page
            elif page >= self.first_row + 6:
                self.first_row = page - 5
            gd.shared_selection.report_page(chain_id, page, slot)
            gd.shared_selection.report_window(chain_id, self.first_row, 6, "grid4x6")
        self.queue.clear()


@unittest.skipUnless(grid_available(), f"no grid module in {zynstubs.UI_DIR} (zynthian-ui electra-grid)")
class HostPagesTest(unittest.TestCase):

    def setUp(self):
        logging.disable(logging.CRITICAL)
        self.addCleanup(logging.disable, logging.NOTSET)
        self.grid = GridModule()
        self.gd = self.grid.__enter__()
        self.addCleanup(self.grid.__exit__)
        self.state = synth_state()
        self.commands = handler(self.state)

    def run_commands(self, *lines):
        if any("show/" in line for line in lines):
            self.seen = {}      # the reporter forgets on show: a dump
        self.commands.process_commands(drivenbymoss.messages_from_text("\n".join(lines)))

    def report(self, full=True):
        reporter = self.commands.reporter
        out = {}
        seen = self.__dict__.setdefault("seen", {})

        def put(k, v):
            """change-only, as the reporter's put"""
            if k not in seen or seen[k] != v or type(seen[k]) is not type(v):
                seen[k] = out[k] = v
        pages = reporter.host_pages
        pages.collect_full(put) if full else pages.collect_fast(put)
        return out

    def test_declared_only_with_the_grid_module(self):
        from zyngine.ctrldev.dbm_zynthian.commands import HOST_PAGE_COMMANDS
        self.assertTrue(set(HOST_PAGE_COMMANDS) <= self.commands.handled_commands())
        self.grid.__exit__()
        self.addCleanup(self.grid.__enter__)       # (the cleanup leaves it again)
        stock = handler(self.state)
        self.assertFalse(set(HOST_PAGE_COMMANDS) & stock.handled_commands())
        self.assertIsNone(stock.host_pages)

    def test_reports(self):
        self.assertEqual(self.report(), {}, "nothing while not shown")
        self.run_commands("int\thostpage\tshow/8/6/8/1\t1")
        out = self.report()
        # Pages: Helm's 14, the Reverb's, the mixer strip's: 16 pages, 8 rows of two, 2 windows
        self.assertEqual((out[M.HOST_PAGE_COLUMNS], out[M.HOST_PAGE_ROWS], out[M.HOST_PAGE_COUNT]), (8, 6, 2))
        self.assertEqual(out[M.HOST_PAGE_TITLE], "zynthian · Synth")
        self.assertEqual(out[M.HOST_PAGE_NAME | 1 << 16], "Helm S12")
        self.assertEqual([out[M.HOST_SECTION_NAME | s << 16] for s in range(out[M.HOST_SECTION_COUNT])], ["Helm", "Reverb", "Mixer"])
        self.assertEqual(out[M.HOST_SECTION_COLOR | 1 << 16], self.gd.section_color(1))
        names = [out[M.HOST_CELL_NAME | c << 16] for c in range(8)]
        self.assertEqual(names, ["p00", "mode0", "toggle0", "float0", "p10", "mode1", "toggle1", "float1"])
        kinds = [out[M.HOST_CELL_KIND | c << 16] for c in range(4)]
        self.assertEqual(kinds, [1, 2, 3, 1])
        self.assertEqual([out[M.HOST_CELL_STEPS | c << 16] for c in range(4)], [128, 3, 2, 0])
        self.assertEqual((out[M.HOST_CELL_TEXT | 1 << 16], out[M.HOST_CELL_SET | 5 << 16], out[M.HOST_SET_COUNT]), ("saw", 1, 12))
        self.assertEqual((self.seen[M.HOST_SET_SELECTED], self.seen[M.HOST_PAGE_ROW], self.seen[M.HOST_CELL_SELECTED], self.seen[M.HOST_SECTION_SELECTED]), (0, 0, 0, 0))
        self.assertEqual(self.report(), {}, "change-only")
        # The fast pass: a changed value and its text, nothing else
        self.state.synth.controllers_dict["p10"].set_value(100)
        out = self.report(full=False)
        self.assertEqual(sorted(out), sorted([M.HOST_CELL_VALUE | 4 << 16, M.HOST_CELL_TEXT | 4 << 16]))
        self.assertEqual(out[M.HOST_CELL_TEXT | 4 << 16], "100")
        # Off: nothing more
        self.run_commands("int\thostpage\tshow/8/6/8/1\t0")
        self.state.synth.controllers_dict["p10"].set_value(10)
        self.assertEqual(self.report(full=False), {})

    def test_commands_without_a_screen(self):
        """No owner of the shared selection (the Tk UI): requests apply at once"""
        self.run_commands("int\thostpage\tshow/8/6/8/1\t1")
        epoch = self.report()[M.HOST_PAGE_EPOCH]
        synth = self.state.synth.controllers_dict
        # A detent and a strip's travel; a fine travel
        self.run_commands(f"int\thostpage\tcell/0/turn/detent/normal/{epoch}\t3")
        self.assertEqual(synth["p00"].value, 67)
        self.run_commands(f"int\thostpage\tcell/1/turn/travel/normal/{epoch}\t70")
        self.assertEqual(synth["mode0"].get_value2label(), "square")
        self.run_commands(f"int\thostpage\tcell/3/turn/travel/fine/{epoch}\t127")
        self.assertAlmostEqual(synth["float0"].value, 0.6)
        self.run_commands(f"noArg\thostpage\tcell/0/reset/{epoch}")
        self.assertEqual(synth["p00"].value, 64)
        # A press selects its page and cell (the screen's), then acts: a toggle switches
        self.run_commands(f"int\thostpage\tcell/6/press/{epoch}\t1", f"int\thostpage\tcell/6/press/{epoch}\t0")
        self.assertEqual(synth["toggle1"].get_value2label(), "on")
        self.assertEqual(self.gd.shared_selection.get(1)[1:3], (1, 2))
        out = self.report()
        self.assertEqual((self.seen[M.HOST_SET_SELECTED], self.seen[M.HOST_PAGE_ROW], self.seen[M.HOST_CELL_SELECTED]), (1, 0, 6))
        # Rows (the same half), sets, sections, pages (windows)
        self.run_commands("int\thostpage\trow\t3")
        self.assertEqual(self.gd.shared_selection.get(1).page, 7)
        self.run_commands("int\thostpage\tset\t4")
        self.assertEqual(self.gd.shared_selection.get(1).page, 4)
        self.run_commands("int\thostpage\tsection\t2")
        self.assertEqual(self.gd.shared_selection.get(1).page, 15)
        out = self.report()
        self.assertEqual((self.seen[M.HOST_PAGE_SELECTED], self.seen[M.HOST_SET_SELECTED], self.seen[M.HOST_SECTION_SELECTED]), (1, 3, 2))
        self.assertNotEqual(out[M.HOST_PAGE_EPOCH], epoch, "another window: a new epoch")
        self.assertEqual(out[M.HOST_CELL_SET | 16 << 16], -1, "the last window is partly empty")
        self.run_commands("int\thostpage\trow\t2")
        self.assertEqual(self.gd.shared_selection.get(1).page, 15, "no page in that row: nothing")
        self.run_commands("int\thostpage\tpage\t0")
        self.assertEqual(self.gd.shared_selection.get(1).page, 0)
        # A cell command of another epoch is dropped
        new_epoch = self.report()[M.HOST_PAGE_EPOCH]
        self.run_commands(f"int\thostpage\tcell/0/turn/detent/normal/{epoch}\t3")
        self.assertEqual((synth["p00"].value, self.commands.host_pages.dropped), (64, 1))
        self.run_commands(f"int\thostpage\tcell/0/turn/detent/normal/{new_epoch}\t3")
        self.assertEqual(synth["p00"].value, 67)

    def test_row_without_the_right_page(self):
        self.state = synth_state(13)
        self.commands = handler(self.state)
        self.run_commands("int\thostpage\tshow/8/6/8/1\t1", "int\thostpage\tset\t1")
        self.report()
        self.run_commands("int\thostpage\tpage\t1")
        self.report()
        self.run_commands("int\thostpage\tset\t1", "int\thostpage\trow\t1")
        self.assertEqual(self.gd.shared_selection.get(1).page, 14, "no right page in that row: the left one")

    def test_selection_through_the_screen(self):
        """With the screen as the owner: requests reach it, only its reports move the JAM; a
        change on the screen (touch, encoders) is reported; the screen shows the selected page"""
        screen = FakeScreen(self.gd, self.state)
        self.run_commands("int\thostpage\tshow/8/6/8/1\t1")
        out = self.report()
        epoch = out[M.HOST_PAGE_EPOCH]
        self.run_commands(f"int\thostpage\tcell/21/press/{epoch}\t1")     # row 2, right: page 5, slot 1
        self.assertEqual(screen.queue, [("cell", 1, 5, 1, 0)])
        self.assertNotIn(M.HOST_CELL_SELECTED, self.report(), "not before the screen applied it")
        screen.apply()
        out = self.report()
        self.assertEqual((self.seen[M.HOST_SET_SELECTED], self.seen[M.HOST_PAGE_ROW], self.seen[M.HOST_CELL_SELECTED]), (5, 2, 21))
        # From the screen: another page and cell
        self.gd.shared_selection.report_page(1, 13, 3)
        out = self.report()
        self.assertEqual((self.seen[M.HOST_PAGE_SELECTED], self.seen[M.HOST_SET_SELECTED], self.seen[M.HOST_CELL_SELECTED]), (1, 1, 7))
        # The JAM's selections: the screen's window always shows the selected page
        for line in ("int\thostpage\tpage\t0", "int\thostpage\trow\t5", "int\thostpage\tsection\t2", "int\thostpage\tset\t0"):
            self.run_commands(line)
            screen.apply()
            selection = self.gd.shared_selection.get(1)
            self.assertTrue(selection.first_row <= selection.page < selection.first_row + selection.rows, (line, selection))
        # Another chain: the screen's selection is the old one's, the default (current screen)
        self.state.synth.current_screen_index = 3
        piano = self.state.chain_manager.chains[2]
        self.state.chain_manager.active_chain = piano
        out = self.report()
        self.assertEqual(out[M.HOST_PAGE_TITLE], "zynthian · Piano")
        self.assertEqual(out[M.HOST_CELL_NAME | 0], "level")

    def test_press_on_a_processor_without_pages_opens_it(self):
        """A processor without pages (no soundfont, no file) is an empty page: a press on its
        cells is the screen's tap on its header (request_open); a controller's cell selects it"""
        self.state = synth_state(2)
        self.state.add_processor(1, "Player", [], type="Audio Effect")
        self.commands = handler(self.state)
        screen = FakeScreen(self.gd, self.state)
        self.run_commands("int\thostpage\tshow/8/6/8/1\t1")
        epoch = self.report()[M.HOST_PAGE_EPOCH]
        pages = self.commands.host_pages.grid.pages
        page = next(i for i, p in enumerate(pages) if p.empty)
        self.assertEqual(pages[page].section, "Player")
        cell = page // 2 * 8 + page % 2 * 4 + 1      # its second cell (no controller)
        self.run_commands(f"int\thostpage\tcell/{cell}/press/{epoch}\t1", f"int\thostpage\tcell/1/press/{epoch}\t1")
        self.assertEqual(screen.queue, [("open", 1, page, -1, 0), ("cell", 1, 0, 1, 0)])

    def remote_chain(self):
        """The active chain with remote-control pages (zynthian-ui's zyngine.zynthian_remote_pages)"""
        from zyngine.zynthian_remote_pages import RemotePages
        chain = self.state.chain_manager.get_active_chain()
        for i, proc in enumerate(chain.get_processors()):
            proc.id = i + 1
        chain.remote_pages = RemotePages(chain)
        return chain

    def test_remote_pages_and_assign(self):
        """The chain's remote pages are the first section; assign (HostPageCellAssign, the JAM's
        hold-a-row + touch-a-strip) puts a cell's controller into the current remote page"""
        chain = self.remote_chain()
        self.run_commands("int\thostpage\tshow/8/6/8/1\t1")
        out = self.report()
        epoch = out[M.HOST_PAGE_EPOCH]
        self.assertEqual(out[M.HOST_SECTION_NAME], "Helm", "no remote pages: no section")
        pages = self.commands.host_pages
        # No screen (the Tk UI): applied at once; the first page is created
        self.assertTrue(pages.assign(5, epoch, 2))                  # mode1 to slot 3
        self.assertEqual(chain.remote_pages.pages[0].slots[2], (1, "mode1"))
        out = self.report()
        new_epoch = out[M.HOST_PAGE_EPOCH]
        self.assertNotEqual(new_epoch, epoch, "the cells hold other controllers")
        self.assertEqual([self.seen[M.HOST_SECTION_NAME | s << 16] for s in range(self.seen[M.HOST_SECTION_COUNT])],
                         ["Remote", "Helm", "Reverb", "Mixer"])
        self.assertEqual((self.seen[M.HOST_SET_NAME], self.seen[M.HOST_SET_NAME | 1 << 16]), ("Page 1", "Page 1 5-8"))
        self.assertEqual((self.seen[M.HOST_CELL_NAME | 2 << 16], self.seen[M.HOST_CELL_KIND | 3 << 16]), ("mode1", 0))
        self.assertEqual(self.seen[M.HOST_CELL_NAME | 8 << 16], "p00", "the processor's pages after it")
        # Another epoch: dropped; an empty cell clears its slot; -1 removes
        self.assertFalse(pages.assign(8, epoch, 0))
        self.assertTrue(pages.assign(9, new_epoch, 7))              # mode0 to slot 8
        self.assertTrue(pages.assign(3, new_epoch, 2))              # the empty cell 3: clears slot 3
        self.assertEqual(chain.remote_pages.pages[0].slots[2], None)
        self.assertEqual(chain.remote_pages.pages[0].slots[7], (1, "mode0"))
        self.assertTrue(pages.assign(9, new_epoch, -1))
        self.assertEqual(chain.remote_pages.find(self.state.synth.controllers_dict["mode0"]), [])
        self.assertFalse(pages.assign(3, new_epoch, -1), "nothing to remove")
        self.assertFalse(pages.assign(9, new_epoch, 8), "no such slot")

    def test_remote_assign_goes_through_the_screen(self):
        """With the screen as the owner, an assignment is applied in its thread"""
        chain = self.remote_chain()
        calls = []
        screen = FakeScreen(self.gd, self.state)
        screen.grid_remote_request = lambda chain, call: calls.append((chain, call))
        self.run_commands("int\thostpage\tshow/8/6/8/1\t1")
        epoch = self.report()[M.HOST_PAGE_EPOCH]
        self.commands.host_pages.assign(0, epoch, 0)
        self.assertEqual(chain.remote_pages.pages, [], "not before the screen applies it")
        self.assertIs(calls[0][0], chain)
        calls[0][1]()
        self.assertEqual(chain.remote_pages.pages[0].slots[0], (1, "p00"))

    def test_a_failing_chain_is_none(self):
        """An exception reading a chain's pages: reported as no page (exists false), logged once,
        the pass goes on; the chain's pages come back when they can be read"""
        self.run_commands("int\thostpage\tshow/8/6/8/1\t1")
        out = self.report()
        epoch = out[M.HOST_PAGE_EPOCH]
        chain = self.state.chain_manager.get_active_chain()
        processors = chain.get_processors

        def broken(*args, **kwargs):
            raise KeyError("zoom")
        chain.get_processors = broken
        gd_chain_pages = self.gd.chain_pages
        self.gd.chain_pages = lambda *args: broken()
        try:
            out = self.report()
            self.assertEqual(out[M.HOST_PAGE_EXISTS], False)
            self.assertNotEqual(out[M.HOST_PAGE_EPOCH], epoch, "no cell of the old pages stays addressable")
            self.assertEqual(self.report(), {}, "the same, once")
            self.run_commands(f"int\thostpage\tcell/0/turn/detent/normal/{epoch}\t3")
            self.run_commands("int\thostpage\tset\t1")       # no pages: nothing, no exception
        finally:
            chain.get_processors = processors
            self.gd.chain_pages = gd_chain_pages
        out = self.report()
        self.assertEqual(out[M.HOST_PAGE_EXISTS], True)
        self.assertEqual(self.seen[M.HOST_CELL_NAME | 0], "p00")

    def test_a_failing_command_is_logged(self):
        self.run_commands("int\thostpage\tshow/8/6/8/1\t1")
        epoch = self.report()[M.HOST_PAGE_EPOCH]
        zctrl = self.state.synth.controllers_dict["p00"]

        def broken(*args, **kwargs):
            raise RuntimeError("zynmixer")
        zctrl.nudge = broken
        self.run_commands(f"int\thostpage\tcell/0/turn/detent/normal/{epoch}\t3")   # no exception
        self.run_commands("int\thostpage\tset\t1")
        self.assertEqual(self.gd.shared_selection.get(1).page, 1, "the next commands still work")


@unittest.skipUnless(grid_available(), f"no grid module in {zynstubs.UI_DIR} (zynthian-ui electra-grid)")
class JamEndToEndTest(unittest.TestCase):
    """The Maschine JAM in the core with zynthian's pages (as the reference host's EndToEndTest)"""

    def setUp(self):
        from zyngine.ctrldev.dbm_zynthian.bridge import ZynBridge
        from zyngine.ctrldev.dbm_zynthian.model import ZynthianModel
        self.grid = GridModule()
        self.gd = self.grid.__enter__()
        self.addCleanup(self.grid.__exit__)
        self.state = synth_state()
        self.screen = FakeScreen(self.gd, self.state)
        self.temp = tempfile.TemporaryDirectory(prefix="dbm-zynthian-hostpage-")
        self.addCleanup(self.temp.cleanup)
        self.midi_out = []
        model = ZynthianModel(self.state.chain_manager, palette=["#0000FF"], zynseq=None)
        self.bridge = ZynBridge(model, JAM, self.temp.name, "zynthian", midi_sink=self.midi_out.append)
        self.bridge.start()
        self.addCleanup(self.bridge.close)
        self.bridge.wait_for_startup()
        self.advance(1100)

    def advance(self, ms):
        for _ in range(0, ms, 10):
            self.screen.apply()         # the screen's GUI thread
            self.bridge.advance(10)

    def cc(self, control, value):
        self.bridge.receive(bytes([0xB0, control, value]))

    def pad(self, x, y):
        note = 36 + (7 - y) * 8 + x
        self.bridge.receive(bytes([0x90, note, 127]))
        self.bridge.receive(bytes([0x80, note, 0]))

    def led(self, x, y):
        note = 36 + (7 - y) * 8 + x
        return next((m[2] for m in reversed(self.midi_out) if len(m) == 3 and m[0] & 0xF0 == 0x90 and m[1] == note), None)

    def test_shared_selection(self):
        # Select + SONG: the host's pages
        for control, value in ((50, 127), (30, 127), (30, 0), (50, 0)):
            self.cc(control, value)
        self.advance(200)
        notifications = self.bridge.take_notifications()
        self.assertTrue(any(n.startswith("Surface: ") for n in notifications), notifications)
        mixer, osc = self.led(1, 1), self.led(6, 1)
        self.assertEqual(self.led(0, 1), mixer + 1, "the selected cell (page 0, its first)")
        # A press on mode1 (cell 5): the screen selects page 1 and the cell; the JAM follows
        self.pad(5, 1)
        self.advance(100)
        self.assertEqual(self.gd.shared_selection.get(1)[1:3], (1, 1))
        self.assertEqual(self.led(5, 1), osc + 2, "mode1 is the selected cell")
        self.assertEqual(self.state.synth.controllers_dict["mode1"].get_value2label(), "square", "a list steps")
        # The screen changes the selection: the JAM follows
        self.gd.shared_selection.report_page(1, 2, 0)
        self.advance(100)
        self.assertEqual(self.led(0, 2), osc + 2)
        # Strip 1 turns row 2's first cell (p20) by its travel
        p20 = self.state.synth.controllers_dict["p20"]
        self.cc(20, 127)
        for position in (30, 50, 70, 90):
            self.cc(8, position)
            self.advance(30)
        self.cc(20, 0)
        self.advance(100)
        self.assertGreater(p20.value, 64)
        # PAGE RIGHT: the second window, its first page on the screen
        self.cc(93, 127)
        self.cc(93, 0)
        self.advance(100)
        self.assertEqual(self.gd.shared_selection.get(1).page, 12)
        self.assertEqual(self.bridge.core_errors(), [])


if __name__ == "__main__":
    unittest.main()
