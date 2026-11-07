# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: tests of the browser on a processor's banks and presets
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

"""The core's browser on the banks and presets of the cursor device (dbm_zynthian/browser.py,
doc/ZYNTHIAN_PLAN.md §3.2): the mapping on zynthian_processor's bank and preset functions with
a stand-in engine, zynthian's preset view (the CUIAs, with a stand-in UI), then the Maschine
Mk2's BROWSE mode with the core."""

import tempfile
import unittest

import zynstubs

zynstubs.install()

from dbm_host import Banks  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.bridge import ZynBridge  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.browser import BrowserUI  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.commands import ZynCommandHandler  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.model import ZynthianModel  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.reporter import ZynReporter  # noqa: E402
from zyngine.ctrldev.zynthian_ctrldev_dbm_maschine_mk2 import zynthian_ctrldev_dbm_maschine_mk2 as mk2  # noqa: E402

BANKS = {
    "Arpeggios": ["Arp 1", "Arp 2"],
    "Pads": ["Warm Pad", "Glass Pad", "Choir", "Strings", "Sweep", "Vox"],
    "Bass": ["Fat Bass", "Sub Bass", "Acid"],
}


def project(favs=()):
    """test_model's project; the Synth chain's processor has the banks above, with "Pads" /
    "Glass Pad" loaded."""
    state = zynstubs.project()
    proc = state.add_preset_processor(1, "ZynAddSubFX", BANKS, favs)
    proc.get_bank_list()
    proc.set_bank_by_name("Pads")
    proc.load_preset_list()
    proc.set_preset_by_name("Glass Pad")
    proc.engine.calls.clear()
    state.proc = proc
    return state


class FakeUI:
    """zynthian's Qt UI as far as the CUIAs of BrowserUI go (zynqt/ui.py): CHAIN_CONTROL makes
    the active chain's current processor the UI's and shows chain control, SHOW_SCREEN shows a
    screen (the history keeps the previous one; the same screen again is filled anew), SELECT
    moves the selection, BACK goes back. Run at once (the state manager's queue in the UI's
    thread)."""

    def __init__(self, chain_manager, screen="mixer"):
        self.chain_manager = chain_manager
        self.current_screen = screen
        self.current_processor = None
        self.history = []
        self.cuias = []
        self.fills = 0
        self.index = None

    # The state manager

    def send_cuia(self, cuia, params=None):
        self.cuias.append((cuia, list(params or [])))
        if cuia == "CHAIN_CONTROL":
            self.current_processor = self.chain_manager.active_chain.current_processor
            self.show("chain_control")
        elif cuia == "SHOW_SCREEN":
            self.show(params[0])
        elif cuia == "SELECT":
            self.index = params[0]
        elif cuia == "BACK" and self.history:
            self.current_screen = self.history.pop()

    def show(self, screen):
        if screen != self.current_screen:
            self.history.append(self.current_screen)
            self.current_screen = screen
        if screen == "preset":
            self.fills += 1
            self.preset_list = list(self.current_processor.preset_list)
            self.index = self.current_processor.get_preset_index()


class Host:

    def __init__(self, state=None, ui=None, setup=None):
        self.state = state or project()
        self.proc = self.state.proc
        self.model = ZynthianModel(self.state.chain_manager)
        self.banks = Banks.from_model_setup(setup or {"numTracks": 8, "numFilterColumnEntries": 4, "numResults": 4})
        self.reporter = ZynReporter(self.model, self.banks)
        self.commands = ZynCommandHandler(self.model, self.banks, self.reporter)
        self.commands.configure()
        self.reporter.features = self.commands.host_features
        self.ui = ui
        if ui is not None:
            self.model.browser.ui = BrowserUI(ui, lambda: ui)

    def report(self):
        return dict(line.split(" ", 1) for line in self.reporter.collect().splitlines())

    def browser(self, command, value=None):
        self.commands.handle("browser", command, value)

    def values(self):
        return self.reporter.last_values

    def results(self):
        lv = self.values()
        return [lv[f"/browser/result/{i}/name"] for i in range(4) if lv[f"/browser/result/{i}/exists"] == "1"]

    def items(self):
        lv = self.values()
        return [lv[f"/browser/column/0/item/{i}/name"] for i in range(4) if lv[f"/browser/column/0/item/{i}/exists"] == "1"]


class BrowserMappingTest(unittest.TestCase):

    def test_closed(self):
        host = Host()
        report = host.report()
        self.assertEqual(report["/browser/exists"], "0")
        self.assertFalse([key for key in report if key.startswith("/browser/") and key != "/browser/exists"])

    def test_open_shows_the_bank_and_the_loaded_preset(self):
        host = Host()
        host.report()
        host.browser("replace/device")
        report = host.report()
        lv = host.values()
        self.assertEqual(report["/browser/exists"], "1")
        self.assertEqual(lv["/browser/contentType/count"], "1")
        self.assertEqual(lv["/browser/contentType/selected/name"], "Presets")
        self.assertEqual(lv["/browser/column/count"], "7")
        self.assertEqual((lv["/browser/column/0/exists"], lv["/browser/column/0/name"]), ("1", "Bank"))
        self.assertEqual(lv["/browser/column/0/cursor/name"], "Pads")
        # The section title is not a bank
        self.assertEqual(host.items(), ["Arpeggios", "Pads", "Bass"])
        self.assertEqual(lv["/browser/column/0/cursorIndex"], "1")
        self.assertEqual(lv["/browser/column/0/item/1/select"], "1")
        for column in range(1, 7):
            self.assertEqual(lv[f"/browser/column/{column}/exists"], "0")
        self.assertEqual(host.results(), ["Warm Pad", "Glass Pad", "Choir", "Strings"])
        self.assertEqual(lv["/browser/result/1/select"], "1")
        self.assertEqual(lv["/browser/result/cursor/name"], "Glass Pad")
        self.assertEqual(host.proc.engine.calls, [], "Opening loads nothing")

    def test_results_and_pages(self):
        host = Host()
        host.browser("replace/device")
        host.browser("result/+")
        host.report()
        self.assertEqual(host.values()["/browser/result/cursor/name"], "Choir")
        host.browser("result/page/+")  # Glass Pad + 1 + 4: Vox (the last), on the second page
        host.report()
        lv = host.values()
        self.assertEqual(lv["/browser/result/cursor/name"], "Vox")
        self.assertEqual(host.results(), ["Sweep", "Vox"])
        self.assertEqual(lv["/browser/result/1/select"], "1")
        host.browser("result/page/-")
        host.browser("result/-")
        host.report()
        self.assertEqual(host.values()["/browser/result/cursor/name"], "Warm Pad")
        host.browser("result/-")  # Stays at the first
        host.report()
        self.assertEqual(host.values()["/browser/result/cursor/name"], "Warm Pad")
        self.assertEqual(host.proc.engine.calls, [])

    def test_bank_column(self):
        host = Host()
        host.browser("replace/device")
        host.browser("column/0/+")
        host.report()
        lv = host.values()
        self.assertEqual(lv["/browser/column/0/cursor/name"], "Bass")
        self.assertEqual(host.results(), ["Fat Bass", "Sub Bass", "Acid"])
        self.assertEqual(lv["/browser/result/cursor/name"], "Fat Bass", "Another bank: its first preset")
        self.assertEqual(host.proc.bank_name, "Bass", "The processor lists the browsed bank (for the UI)")
        self.assertEqual(host.proc.get_preset_name(), "Glass Pad", "Still the loaded preset")
        self.assertEqual(host.proc.engine.calls, [], "The bank is not set in the engine")
        host.browser("column/0/reset")
        host.report()
        self.assertEqual(host.values()["/browser/column/0/cursor/name"], "Arpeggios")
        host.browser("column/0/page/+")
        host.report()
        self.assertEqual(host.values()["/browser/column/0/cursor/name"], "Bass")
        host.browser("column/0/-")
        host.report()
        self.assertEqual(host.values()["/browser/result/cursor/name"], "Glass Pad", "Back in the loaded preset's bank")

    def test_commit_loads_bank_and_preset(self):
        host = Host()
        host.browser("replace/device")
        host.browser("column/0/+")
        host.browser("result/+")
        host.browser("stop", 1)
        report = host.report()
        self.assertEqual(report["/browser/exists"], "0")
        self.assertEqual(host.proc.engine.calls, [("set_bank", "Bass"), ("set_preset", "Sub Bass")])
        self.assertEqual((host.proc.get_bank_name(), host.proc.get_preset_name()), ("Bass", "Sub Bass"))
        self.assertEqual(host.proc.get_preset_index(), 1)
        # A second stop (the core's browse mode commits when it is left) does nothing
        host.browser("stop", 1)
        self.assertEqual(len(host.proc.engine.calls), 2)

    def test_commit_in_the_same_bank(self):
        host = Host()
        host.browser("replace/device")
        host.browser("result/+")
        host.browser("stop", 1)
        self.assertEqual(host.proc.engine.calls, [("set_preset", "Choir")])

    def test_cancel_keeps_bank_and_preset(self):
        host = Host()
        host.browser("replace/device")
        host.browser("column/0/+")
        host.browser("result/+")
        host.browser("stop", 0)
        self.assertEqual(host.report()["/browser/exists"], "0")
        self.assertEqual(host.proc.engine.calls, [])
        self.assertEqual((host.proc.bank_name, host.proc.get_preset_name()), ("Pads", "Glass Pad"))
        self.assertEqual([row[2] for row in host.proc.preset_list][:2], ["Warm Pad", "Glass Pad"])

    def test_preview_preloads_and_cancel_restores(self):
        host = Host()
        host.browser("replace/device")
        host.browser("preview", 1)
        host.report()
        self.assertEqual(host.values()["/browser/preview"], "1")
        host.proc.engine.calls.clear()
        host.browser("result/+")
        host.browser("result/+")
        self.assertEqual(host.proc.engine.calls, [("preload", "Choir"), ("preload", "Strings")])
        host.browser("stop", 0)
        # restore_preset: the state before the first preload (bank and preset into the engine)
        self.assertEqual(host.proc.engine.calls[2:], [("set_bank", "Pads"), ("set_preset", "Glass Pad")])
        self.assertEqual((host.proc.bank_name, host.proc.get_preset_name()), ("Pads", "Glass Pad"))
        self.assertIsNone(host.proc.preload_info)

    def test_preview_commit_of_the_preloaded_preset(self):
        host = Host()
        host.browser("preview", 1)
        host.browser("replace/device")
        host.proc.engine.calls.clear()
        host.browser("result/+")
        host.browser("stop", 1)
        self.assertEqual(host.proc.engine.calls, [("preload", "Choir")], "Already preloaded: not loaded again")
        self.assertEqual(host.proc.get_preset_name(), "Choir")
        self.assertIsNone(host.proc.preload_info)

    def test_favourites_bank(self):
        host = Host(project(favs=[("Bass", "Acid")]))
        host.browser("replace/device")
        host.report()
        self.assertEqual(host.items()[0], "*** Favorites ***")
        host.browser("column/0/reset")
        host.report()
        self.assertEqual(host.results(), ["Acid"])
        host.browser("stop", 1)
        # zynthian's preset list marks the favourites in the titles (load_preset_list)
        self.assertEqual(host.proc.engine.calls, [("set_bank", "Bass"), ("set_preset", "❤ Acid")])
        self.assertEqual((host.proc.get_bank_name(), host.proc.get_preset_name()), ("Bass", "Acid"))
        self.assertFalse(host.proc.show_fav_presets)

    def test_preset_names_without_the_favourite_mark(self):
        host = Host(project(favs=[("Pads", "Choir")]))
        host.browser("replace/device")
        host.report()
        self.assertEqual(host.results()[2], "Choir")

    def test_unsupported(self):
        host = Host()
        for command in ("add/track/0", "insertAfter", "insertBefore", "replace/slot/0/1", "replace/drum/pad/36"):
            host.browser(command)
        host.browser("replace/device")
        host.browser("column/3/+")
        host.browser("contentType/+")
        self.assertEqual(host.commands.unhandled, ["browser add/track/<n>", "browser insertAfter", "browser insertBefore",
                                                   "browser replace/slot/<n>/<n>", "browser replace/drum/pad/<n>",
                                                   "browser column/<n>/+"])

    def test_no_cursor_device(self):
        host = Host()
        host.state.chain_manager.active_chain = host.state.chain_manager.chains[3]  # Audio In
        host.browser("replace/device")
        self.assertEqual(host.report()["/browser/exists"], "0")


class BrowserUITest(unittest.TestCase):
    """zynthian's preset view through CUIAs (BrowserUI) with a stand-in of the Qt UI."""

    def setUp(self):
        self.state = project()
        self.ui = FakeUI(self.state.chain_manager)
        self.host = Host(self.state, self.ui)

    def test_open_select_and_close(self):
        ui, host = self.ui, self.host
        host.browser("replace/device")
        self.assertEqual(ui.cuias, [("CHAIN_CONTROL", []), ("SHOW_SCREEN", ["preset"]), ("SELECT", [1])])
        self.assertEqual((ui.current_screen, ui.current_processor), ("preset", host.proc))
        host.report()
        host.browser("result/+")
        self.assertEqual(ui.cuias[-1], ("SELECT", [2]))
        # Another bank: the preset view reads the browsed bank's list
        host.browser("column/0/+")
        self.assertEqual(ui.cuias[-2:], [("SHOW_SCREEN", ["preset"]), ("SELECT", [0])])
        self.assertEqual([row[2] for row in ui.preset_list], ["Fat Bass", "Sub Bass", "Acid"])
        host.browser("stop", 1)
        self.assertEqual(ui.cuias[-2:], [("BACK", []), ("BACK", [])])
        self.assertEqual(ui.current_screen, "mixer", "Back where the UI was")
        host.report()

    def test_the_ui_on_chain_control_of_the_processor(self):
        ui, host = self.ui, self.host
        ui.current_screen, ui.current_processor = "chain_control", host.proc
        host.browser("replace/device")
        self.assertEqual(ui.cuias, [("SHOW_SCREEN", ["preset"]), ("SELECT", [1])])
        host.browser("stop", 0)
        self.assertEqual(ui.cuias[-1], ("BACK", []))
        self.assertEqual(ui.current_screen, "chain_control")

    def test_back_in_the_ui_cancels(self):
        ui, host = self.ui, self.host
        host.browser("replace/device")
        host.report()
        host.browser("column/0/+")
        ui.send_cuia("BACK")
        report = host.report()
        self.assertEqual(report["/browser/exists"], "0", "The browser closes with the preset view")
        self.assertEqual((host.proc.bank_name, host.proc.get_preset_name()), ("Pads", "Glass Pad"))
        sent = len(ui.cuias)
        host.browser("stop", 1)  # The core's browse mode commits when it is left: nothing left
        self.assertEqual(len(ui.cuias), sent)
        self.assertEqual(host.proc.engine.calls, [])

    def test_preset_chosen_in_the_ui(self):
        ui, host = self.ui, self.host
        host.browser("replace/device")
        host.report()
        host.browser("column/0/+")
        # The preset view's select_action: set_preset of the list it shows, then chain control
        host.proc.set_preset(2)
        ui.show("chain_control")
        report = host.report()
        self.assertEqual(report["/browser/exists"], "0")
        self.assertEqual((host.proc.bank_name, host.proc.get_preset_name()), ("Bass", "Acid"), "The UI's choice stays")
        self.assertEqual(host.proc.engine.calls, [("set_preset", "Acid"), ("set_bank", "Bass")])

    def test_no_ui(self):
        host = Host(project(), None)
        host.model.browser.ui = BrowserUI(self.ui, lambda: None)
        host.browser("replace/device")
        host.browser("stop", 1)
        self.assertEqual(self.ui.cuias, [])


MCU = bytes.fromhex("F000006614 12".replace(" ", ""))

# The Mk2's controls (src/controller/maschine/mk/surface.rs)
BROWSE = 40
KNOB_1 = 70
KNOB_8 = 77
ENCODER = 7
ENCODER_PUSH = 8
CURSOR_RIGHT = 31
CURSOR_LEFT = 33


def display(sent):
    """The two lines of the Mk2's display (MCU sysex, 2 x 56 characters)."""
    lines = [" " * 56, " " * 56]
    for data in sent:
        if data[:6] == MCU and data[-1] == 0xF7:
            pos = data[6]
            text = bytes(data[7:-1]).decode("latin1")
            row = 0 if pos < 56 else 1
            start = pos - 56 * row
            lines[row] = (lines[row][:start] + text + lines[row][start + len(text):])[:56]
    return lines


class MaschineMk2BrowseTest(unittest.TestCase):
    """The Maschine Mk2's BROWSE with the core: the bank on knob 1, the presets on knob 8 and the
    encoder, ENCODER PUSH loads, BROWSE again cancels."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="dbm-zynthian-mk2-browse-")
        self.state = project()
        self.proc = self.state.proc
        self.model = ZynthianModel(self.state.chain_manager)
        self.ui = FakeUI(self.state.chain_manager)
        self.model.browser.ui = BrowserUI(self.ui, lambda: self.ui)
        self.midi_out = []
        self.bridge = ZynBridge(self.model, mk2.dbm_controller_id, self.temp.name, "zynthian", midi_sink=self.midi_out.append)
        self.bridge.start()
        self.bridge.wait_for_startup()
        self.bridge.advance(100)

    def tearDown(self):
        self.bridge.close()
        self.temp.cleanup()

    def press(self, cc):
        self.bridge.send([0xB0, cc, 127])
        self.bridge.send([0xB0, cc, 0], 200)
        self.bridge.advance(200)

    def turn(self, cc, steps):
        for _ in range(abs(steps)):
            self.bridge.send([0xB0, cc, 1 if steps > 0 else 127], 100)
        self.bridge.advance(200)

    def display(self):
        return display(self.midi_out)

    def cell(self, row, column):
        """A cell of the display: 6 characters and a space."""
        return self.display()[row][column * 7:column * 7 + 7].strip()

    def test_browse_select_and_cancel(self):
        self.press(BROWSE)
        self.assertEqual(self.model.browser.processor, self.proc, "BROWSE opens the browser on the cursor device")
        top, bottom = self.display()
        self.assertEqual((self.cell(0, 0), self.cell(1, 0)), ("Bank", "Pads"), (top, bottom))
        self.assertEqual(self.cell(0, 7), ">Prese", "The result column (the content type, shortened)")
        self.assertEqual(self.cell(1, 7), "Glass", (top, bottom))
        self.assertEqual(self.ui.current_screen, "preset")
        # Knob 1: the bank
        self.turn(KNOB_1, 1)
        self.assertEqual(self.cell(1, 0), "Bass", self.display())
        self.assertEqual(self.cell(1, 7), "Fat Ba", self.display())
        # Knob 8: the presets
        self.turn(KNOB_8, 2)
        self.assertEqual(self.cell(1, 7), "Acid", self.display())
        self.assertEqual(self.ui.index, 2, "zynthian's preset view follows")
        self.turn(KNOB_8, -1)
        self.assertEqual(self.cell(1, 7), "Sub Ba", self.display())
        # BROWSE again: cancel
        self.press(BROWSE)
        self.assertFalse(self.model.browser.is_open)
        self.assertEqual(self.proc.engine.calls, [])
        self.assertEqual((self.proc.bank_name, self.proc.get_preset_name()), ("Pads", "Glass Pad"))
        self.assertEqual(self.ui.current_screen, "mixer")
        self.assertEqual(list(self.bridge.core_errors()), [])
        self.assertFalse([k for k in self.bridge.commands.unhandled if k.startswith("browser")], self.bridge.commands.unhandled)

    def test_encoder_push_loads(self):
        self.press(BROWSE)
        self.turn(KNOB_1, -1)
        self.assertEqual(self.cell(1, 0), "Arpegg", self.display())
        self.turn(KNOB_8, 1)
        self.assertEqual(self.cell(1, 7), "Arp 2", self.display())
        # The encoder turns the marked column: LEFT moves the mark to the bank
        for _ in range(7):
            self.press(CURSOR_LEFT)
        self.assertEqual(self.cell(0, 0), ">Bank", self.display())
        self.turn(ENCODER, 1)
        self.assertEqual(self.cell(1, 0), "Pads", self.display())
        self.turn(ENCODER, -1)
        for _ in range(7):
            self.press(CURSOR_RIGHT)
        self.turn(ENCODER, 1)
        self.assertEqual(self.cell(1, 7), "Arp 2", self.display())
        self.press(ENCODER_PUSH)
        self.assertFalse(self.model.browser.is_open)
        self.assertEqual(self.proc.engine.calls, [("set_bank", "Arpeggios"), ("set_preset", "Arp 2")])
        self.assertEqual(self.ui.current_screen, "mixer")
        self.assertNotEqual(self.cell(0, 0), "Bank", "The Mk2 left the browse mode")
        self.assertEqual(list(self.bridge.core_errors()), [])


if __name__ == "__main__":
    unittest.main()
