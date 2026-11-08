# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: the "add chain" placeholder track
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


"""The "add chain" placeholder (doc/ZYNTHIAN_PLAN.md §3.2): one empty track after the last chain,
before the mixbuses and the main chain; selecting it opens zynthian's Add Chain view (CUIA
``ADD_CHAIN``) and selects no chain, its other commands are ignored. The adapter alone, then the
Launch Control XL's Track Focus, the Maschine JAM's group buttons and the Launchpad X's Mix view
with the core."""

import tempfile
import unittest

import zynstubs

zynstubs.install()

from dbm_host import Banks  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.bridge import ZynBridge  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.commands import ZynCommandHandler  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.model import ADD_CHAIN_CUIA, ZynthianModel  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.reporter import ZynReporter  # noqa: E402
from zyngine.ctrldev.zynthian_ctrldev_dbm_launchcontrol_xl import zynthian_ctrldev_dbm_launchcontrol_xl as lcxl  # noqa: E402
from zyngine.ctrldev.zynthian_ctrldev_dbm_maschine_jam import zynthian_ctrldev_dbm_maschine_jam as jam  # noqa: E402

ADD_CHAIN = [(ADD_CHAIN_CUIA, None)]


def project(chains):
    """The main chain, ``chains`` synths (the first active) and a mixbus."""
    state = zynstubs.ZynthianState()
    for n in range(chains):
        state.add_chain(n + 1, f"Synth {n + 1}", midi_chan=n)
    state.add_chain(20, "Reverb", mixbus=True)
    if chains:
        state.chain_manager.active_chain = state.chain_manager.chains[1]
    return state


class Host:

    def __init__(self, state, setup=None):
        self.state = state
        self.model = ZynthianModel(state.chain_manager, zynseq=state.zynseq)
        self.model.send_cuia = state.send_cuia
        self.banks = Banks.from_model_setup(setup or {"numTracks": 8, "numSends": 2})
        self.reporter = ZynReporter(self.model, self.banks)
        self.commands = ZynCommandHandler(self.model, self.banks, self.reporter)
        self.commands.configure()
        self.reporter.features = self.commands.host_features
        self.values = {}

    def report(self):
        # A full pass: the state is changed directly, without zynthian's change signals (the
        # two rates: test_rates.py)
        self.reporter.request_full()
        lines = dict(line.split(" ", 1) for line in self.reporter.collect().splitlines())
        self.values.update(lines)
        return lines

    def command(self, processor, command, value=None):
        self.commands.handle(processor, command, value)


class TrackListTest(unittest.TestCase):

    def names(self, chains, full):
        return [t.name for t in Host(project(chains)).model.track_list(full)]

    def test_no_chain(self):
        self.assertEqual(self.names(0, False), ["+"])
        self.assertEqual(self.names(0, True), ["+", "Reverb", "Main"], "The master stays last")

    def test_one_chain(self):
        self.assertEqual(self.names(1, False), ["Synth 1", "+"])
        self.assertEqual(self.names(1, True), ["Synth 1", "+", "Reverb", "Main"])

    def test_eight_chains_the_placeholder_on_the_second_page(self):
        host = Host(project(8))
        report = host.report()
        self.assertEqual(report["/track/count"], "9")
        self.assertEqual(report["/track/8/name"], "+")
        self.assertEqual(report["/track/8/exists"], "1")
        host.command("bank", "track", 8)
        report = host.report()
        self.assertEqual(report["/track/offset"], "8", "Paged to the placeholder")
        host.command("track", "8/select", 1)
        self.assertEqual(host.state.cuias, ADD_CHAIN)

    def test_how_it_looks(self):
        host = Host(project(1))
        host.state.zynseq = None
        report = host.report()
        prefix = "/track/1/"
        self.assertEqual(report[prefix + "name"], "+")
        self.assertEqual(report[prefix + "color"], "0 0 0", "Black: unlit")
        self.assertEqual(report[prefix + "select"], "0")
        self.assertEqual(report[prefix + "send/count"], "0")
        self.assertEqual((report[prefix + "mute"], report[prefix + "solo"], report[prefix + "recarm"]), ("0", "0", "0"))
        self.assertEqual((report[prefix + "volume"], report[prefix + "volume/str"]), ("0.000000", ""))
        self.assertEqual((report[prefix + "canHoldNotes"], report[prefix + "canHoldAudio"]), ("0", "0"))

    def test_no_slots(self):
        state = project(1)
        state.zynseq = zynstubs.FakeZynseq()
        host = Host(state)
        report = host.report()
        self.assertEqual(report["/track/1/slot/count"], "0")
        self.assertNotIn("/track/1/slot/0/exists", report)

    def test_new_chain_moves_it(self):
        host = Host(project(1))
        host.report()
        host.state.add_chain(2, "Synth 2", midi_chan=1)
        report = host.report()
        self.assertEqual(report["/track/1/name"], "Synth 2")
        self.assertEqual(report["/track/2/name"], "+")
        self.assertEqual(report["/track/count"], "3")


class CommandTest(unittest.TestCase):

    def test_select_opens_add_chain(self):
        host = Host(project(2))
        host.report()
        host.command("track", "2/select", 1)
        self.assertEqual(host.state.cuias, ADD_CHAIN)
        self.assertEqual(host.state.chain_manager.active_chain.chain_id, 1, "No chain selected")
        report = host.report()
        self.assertEqual((report.get("/track/0/select"), report.get("/track/2/select")), ("1", "0"),
                         "The selection is reported again: the core keeps the active chain")
        self.assertEqual(host.commands.unhandled, [])

    def test_select_without_ui(self):
        host = Host(project(1))
        host.model.send_cuia = None
        host.command("track", "1/select", 1)
        self.assertEqual(host.state.chain_manager.active_chain.chain_id, 1)

    def test_other_commands_ignored(self):
        state = project(1)
        state.zynseq = zynstubs.FakeZynseq()
        host = Host(state)
        before = host.state.zctrl(1, "level").value
        for command, value in (("1/mute", 1), ("1/solo", 1), ("1/solo/toggle", None), ("1/volume", 0.5),
                               ("1/volume/inc", 0.1), ("1/volume/reset", None), ("1/pan", 0.0),
                               ("1/send/0/volume", 1.0), ("1/send/0/active", 1.0), ("1/recarm", 1),
                               ("1/name", "x"), ("1/clip/0/launch", 1), ("1/clip/0/record", 1),
                               ("1/clip/0/select", 1), ("1/createClip/0", 4), ("1/stop", None), ("1/select", 0)):
            host.command("track", command, value)
        self.assertEqual(host.state.cuias, [], "Only a select opens the Add Chain view")
        self.assertEqual(host.commands.unhandled, [], "Ignored, not logged as unhandled")
        self.assertEqual(host.state.zctrl(1, "level").value, before)
        self.assertEqual(host.state.zctrl(1, "mute").value, 0)
        self.assertEqual(host.state.chain_manager.chains[1].title, "Synth 1")
        self.assertEqual(getattr(state.zynseq.libseq, "play_calls", []), [], "Nothing launched")
        self.assertFalse(getattr(state.zynseq.libseq, "midi_record", False), "Nothing recorded")
        self.assertEqual(getattr(state.zynseq.libseq, "patterns", {}), {}, "No clip created")

    def test_select_next_skips_it(self):
        host = Host(project(2), {"numTracks": 8, "hasFullFlatTrackList": True})
        host.state.chain_manager.set_active_chain_by_id(2)
        host.command("track", "selectNext")
        self.assertEqual(host.state.chain_manager.active_chain.chain_id, 20, "On to the mixbus")
        host.command("track", "selectPrevious")
        self.assertEqual(host.state.chain_manager.active_chain.chain_id, 2)
        self.assertEqual(host.state.cuias, [])


TEMPLATE_HEADER = bytes.fromhex("F0002029021177")
TRACK_FOCUS = [0x29, 0x2A, 0x2B, 0x2C, 0x39, 0x3A, 0x3B, 0x3C]


class LaunchControlXLTest(unittest.TestCase):
    """Track Focus 5 on the placeholder (zynstubs' project: four chains, then "+", the two
    mixbuses, the main chain in the full flat list)."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="dbm-zynthian-add-chain-")
        self.addCleanup(self.temp.cleanup)
        self.state = zynstubs.project()
        self.model = ZynthianModel(self.state.chain_manager, palette=["#0000FF", "#BBBB00", "#FF00FF"])
        self.model.send_cuia = self.state.send_cuia
        self.midi_out = []
        self.bridge = ZynBridge(self.model, lcxl.dbm_controller_id, self.temp.name, "zynthian", midi_sink=self.device)
        self.addCleanup(self.bridge.close)
        self.bridge.start()
        self.bridge.wait_for_startup()
        self.bridge.advance(100)

    def device(self, data):
        self.midi_out.append(bytes(data))
        if data[: len(TEMPLATE_HEADER)] == TEMPLATE_HEADER:
            self.bridge.receive(data)

    def led(self, note):
        for data in reversed(self.midi_out):
            if len(data) == 3 and data[0] == 0x98 and data[1] == note:
                return data[2]
        return None

    def press(self, note):
        self.bridge.send([0x98, note, 127])
        self.bridge.send([0x88, note, 0])

    def test_track_focus_opens_add_chain(self):
        self.assertEqual(self.bridge.reporter.last_values["/track/4/name"], "+")
        selected, other, placeholder = self.led(TRACK_FOCUS[0]), self.led(TRACK_FOCUS[1]), self.led(TRACK_FOCUS[4])
        self.assertNotEqual(selected, other)
        # The LCXL lights Track Focus of every existing track alike (no track colors)
        self.assertEqual(placeholder, other)
        self.press(TRACK_FOCUS[4])
        self.bridge.advance(300)
        self.assertEqual(self.state.cuias, ADD_CHAIN)
        self.assertEqual(self.state.chain_manager.active_chain.chain_id, 1)
        self.assertEqual(self.led(TRACK_FOCUS[0]), selected, "Track Focus 1 still shows the selection")
        self.assertEqual(self.led(TRACK_FOCUS[4]), placeholder, "The placeholder's LED unchanged")
        # The core did not move its cursor: Track Focus 2 still selects Piano
        self.press(TRACK_FOCUS[1])
        self.bridge.advance(300)
        self.assertEqual(self.state.chain_manager.active_chain.chain_id, 2)
        self.assertEqual(self.state.cuias, ADD_CHAIN)
        self.assertEqual(list(self.bridge.core_errors()), [])


GROUP_A = 70


class MaschineJamTest(unittest.TestCase):
    """Group button E on the placeholder (four chains, then "+")."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="dbm-zynthian-add-chain-jam-")
        self.addCleanup(self.temp.cleanup)
        self.state = zynstubs.project()
        self.model = ZynthianModel(self.state.chain_manager, palette=["#0000FF", "#BBBB00", "#FF00FF"])
        self.model.send_cuia = self.state.send_cuia
        self.midi_out = []
        self.bridge = ZynBridge(self.model, jam.dbm_controller_id, self.temp.name, "zynthian", midi_sink=self.midi_out.append)
        self.addCleanup(self.bridge.close)
        self.bridge.start()
        self.bridge.wait_for_startup()
        self.bridge.advance(500)

    def cc(self, cc):
        return next((d[2] for d in reversed(self.midi_out) if len(d) == 3 and d[0] == 0xB0 and d[1] == cc), None)

    def button(self, cc):
        self.bridge.send([0xB0, cc, 127])
        self.bridge.send([0xB0, cc, 0], 300)

    def test_group_button_opens_add_chain(self):
        selected, placeholder, missing = self.cc(GROUP_A), self.cc(GROUP_A + 4), self.cc(GROUP_A + 5)
        # The core lights every existing track (dim_or_highlight_color): black is the dim white
        self.assertEqual(missing, 0, "Group F: no track")
        self.assertNotIn(placeholder, (0, selected), "Group E: dim")
        self.button(GROUP_A + 4)
        self.assertEqual(self.state.cuias, ADD_CHAIN)
        self.assertEqual(self.state.chain_manager.active_chain.chain_id, 1)
        self.assertEqual(self.cc(GROUP_A), selected, "Group A still shows the selection")
        self.assertEqual(self.cc(GROUP_A + 4), placeholder)
        self.button(GROUP_A + 1)
        self.assertEqual(self.state.chain_manager.active_chain.chain_id, 2)
        self.assertEqual(list(self.bridge.core_errors()), [])


class LaunchpadXMixViewTest(unittest.TestCase):
    """The Launchpad X's Mix view (Shift + Session): the top row selects the track of its column
    (column 5: the placeholder)."""

    def test_top_row_opens_add_chain(self):
        from test_launchpad import SESSION, SHIFT, Device
        lp = Device(self)
        lp.model.send_cuia = lp.state.send_cuia
        lp.bridge.send([0xB0, SHIFT, 127])
        lp.button(SESSION)
        lp.bridge.send([0xB0, SHIFT, 0], 300)
        self.assertIn("Mix", lp.notifications())
        self.assertEqual(lp.led(85)[0], 1, "The volume row shows the track color: black is the darkest gray")
        self.assertEqual((lp.led(55)[0], lp.led(65)[0]), (0, 0), "No sends: the send rows unlit")
        lp.pad(85)
        self.assertEqual(lp.state.cuias, ADD_CHAIN)
        self.assertEqual(lp.state.chain_manager.active_chain.chain_id, 1)
        lp.pad(82)
        self.assertEqual(lp.state.chain_manager.active_chain.chain_id, 2, "The core's selection is not stuck")
        self.assertEqual(lp.errors(), [])



# The browser session on the Add Chain view (add_chain.py)

QT_ENTRIES = ["Instrument", "Audio Input", "Clip Launcher", "MIDI", "MIDI\n+\nAudio", "Audio Generator", "Special", "Mixbus"]
NAMES = [" ".join(t.split()) for t in QT_ENTRIES]


class FakeAddChainScreen:
    """zynqt's AddChain screen: its entries (a 4x4 grid: placeholders after the chain types)."""

    def __init__(self, ui):
        self.ui = ui
        self.entries = []
        self.index = 0

    def fill(self, titles):
        self.entries = [{"title": t, "separator": False, "enabled": True} for t in titles]
        self.entries += [{"title": "", "separator": True, "enabled": True}] * (16 - len(titles))
        self.index = 0


ENGINES = {
    "MIDI Synth": {"Synth": {"ZY": "ZynAddSubFX", "OB": "OBXD"}, "Sampler": {"SF": "FluidSynth", "LS": "LinuxSampler"}},
    "Audio Effect": {"Reverb": {"DR": "Dragonfly"}, "Other": {"MI": "Mixer", "EQ": "EQ"}},
}


class FakeEngineScreen:
    """zynqt's Engine screen: the categories of the chain type, the category's engines ("None"
    first for a MIDI or audio effect chain), its first one selected."""

    def __init__(self, ui):
        self.ui = ui
        self.show_all = False
        self.categories = []
        self.categoryIndex = 0
        self.engines_by_cat = {}
        self.entries = []
        self.index = 0

    def fill(self):
        proc_type = self.ui.chain_edit.status["type"]
        self.engines_by_cat = {cat: {code: {"TITLE": title} for code, title in engines.items()}
                               for cat, engines in ENGINES.get(proc_type, {}).items()}
        self.categories = list(self.engines_by_cat)
        self.categoryIndex = 1 if len(self.categories) > 1 else 0  # The last one used for the type
        self.fill_category()

    def fill_category(self):
        proc_type = self.ui.chain_edit.status["type"]
        entries = [("None", "None")] if proc_type in ("MIDI Tool", "Audio Effect") else []
        cat = self.categories[self.categoryIndex] if self.categories else None
        entries += [(info["TITLE"], code) for code, info in self.engines_by_cat.get(cat, {}).items() if code != "MI"]
        self.entries = [{"title": t, "data": c, "separator": False, "enabled": True} for t, c in entries]
        self.index = 0


class FakeChainEdit:
    def __init__(self):
        self.status = {}


TYPES = {"Instrument": {"type": "MIDI Synth"}, "Audio Input": {"type": "Audio Effect", "midi_chan": None},
         "Clip Launcher": {"type": "Audio Generator", "engine": "CL", "midi_chan": None},
         "MIDI": {"type": "MIDI Tool"}, "MIDI + Audio": {"type": "Audio Effect"},
         "Audio Generator": {"type": "Audio Generator", "midi_chan": None}, "Special": {"type": "Special"},
         "Mixbus": {"type": "Audio Effect", "midi_chan": None}}


class FakeQtUI:
    """zynthian's Qt UI as far as the chain sessions' CUIAs go (zynqt/ui.py): ADD_CHAIN shows the
    Add Chain view (filled anew, the selection on the first entry), SELECT moves the selection
    of the screen shown, ARROW_LEFT/RIGHT the engine screen's category, SELECT_ACTION runs the
    selected entry's action (zynthian's modify_chain: the engine screen, then the MIDI channel
    screen or the new chain; a Clip Launcher chain is added at once), BACK goes back. Run at
    once."""

    def __init__(self, state, screen="mixer", titles=QT_ENTRIES):
        self.state = state
        self.current_screen = screen
        self.history = []
        self.cuias = []
        self.titles = list(titles)
        self.chain_edit = FakeChainEdit()
        self.list_screens = {"add_chain": FakeAddChainScreen(self), "engine": FakeEngineScreen(self)}
        self.chosen = None
        self.engine = None

    @property
    def screen(self):
        return self.list_screens["add_chain"]

    @property
    def engine_screen(self):
        return self.list_screens["engine"]

    def send_cuia(self, cuia, params=None):
        self.cuias.append((cuia, list(params or [])))
        shown = self.list_screens.get(self.current_screen)
        if cuia == "ADD_CHAIN":
            self.screen.fill(self.titles)
            self.show("add_chain")
        elif cuia == "SELECT" and shown is not None:
            shown.index = params[0]
        elif cuia in ("ARROW_LEFT", "ARROW_RIGHT") and shown is self.engine_screen:
            index = shown.categoryIndex + (1 if cuia == "ARROW_RIGHT" else -1)
            if 0 <= index < len(shown.categories):
                shown.categoryIndex = index
                shown.fill_category()
        elif cuia == "SELECT_ACTION" and shown is self.screen:
            self.activate(self.screen.index)
        elif cuia == "SELECT_ACTION" and shown is self.engine_screen:
            self.pick_engine(shown.index)
        elif cuia == "BACK" and self.history:
            self.current_screen = self.history.pop()

    def activate(self, index):
        self.chosen = " ".join(self.screen.entries[index]["title"].split())
        self.chain_edit.status = dict(TYPES.get(self.chosen, {"type": "?"}))
        self.modify_chain()

    def pick_engine(self, index):
        self.engine = self.engine_screen.entries[index]["title"]
        self.chain_edit.status["engine"] = self.engine_screen.entries[index]["data"]
        self.modify_chain()

    def modify_chain(self):
        status = self.chain_edit.status
        if "engine" not in status:
            self.show("engine")
            self.engine_screen.fill()
        elif "midi_chan" not in status:
            self.show("midi_chan")
        else:
            # ChainEdit._add_chain: the chain is added and active
            chain = self.state.add_chain(10, self.engine or self.chosen)
            self.state.chain_manager.active_chain = chain
            self.history = []
            self.current_screen = "launcher" if status["engine"] == "CL" else "chain_control"

    def show(self, screen):
        if screen != self.current_screen:
            self.history.append(self.current_screen)
            self.current_screen = screen


class BrowsingHost(Host):

    def __init__(self, state=None, screen="mixer", titles=QT_ENTRIES):
        from zyngine.ctrldev.dbm_zynthian.add_chain import AddChainUI
        state = state or project(2)
        super().__init__(state, {"numTracks": 8, "numSends": 2, "numFilterColumnEntries": 4, "numResults": 4})
        self.ui = FakeQtUI(state, screen, titles)
        self.model.browse_add_chain = True
        self.model.browser.add_chain.ui = AddChainUI(self.ui, lambda: self.ui)

    def browser(self, command, value=None):
        self.commands.handle("browser", command, value)

    def items(self, prefix):
        lv = self.reporter.last_values
        return [lv[f"{prefix}{i}/name"] for i in range(4) if lv[f"{prefix}{i}/exists"] == "1"]


class AddChainBrowserTest(unittest.TestCase):

    def test_select_opens_the_session(self):
        host = BrowsingHost()
        host.report()
        host.command("track", "2/select", 1)
        self.assertEqual(host.ui.cuias, [("ADD_CHAIN", [])])
        self.assertEqual(host.state.cuias, [], "The CUIA goes through the session's UI")
        self.assertEqual(host.ui.current_screen, "add_chain")
        report = host.report()
        self.assertEqual(report["/browser/exists"], "1")
        self.assertEqual(report["/browser/contentType/selected/name"], "Add chain")
        self.assertEqual((report["/browser/column/0/exists"], report["/browser/column/0/name"]), ("1", "Chain"))
        self.assertEqual(report["/browser/column/1/exists"], "0")
        self.assertEqual(host.items("/browser/column/0/item/"), NAMES[:4])
        self.assertEqual(host.items("/browser/result/"), NAMES[:4])
        self.assertEqual(report["/browser/result/cursor/name"], "Instrument")
        self.assertEqual(report["/browser/column/0/cursor/name"], "Instrument")
        self.assertEqual(host.state.chain_manager.active_chain.chain_id, 1, "No chain selected")
        self.assertEqual(host.commands.unhandled, [])

    def test_moving_follows_in_the_ui(self):
        host = BrowsingHost()
        host.command("track", "2/select", 1)
        host.report()
        host.browser("result/+")
        self.assertEqual(host.ui.cuias[-1], ("SELECT", [1]))
        host.browser("column/0/+")
        self.assertEqual(host.ui.cuias[-1], ("SELECT", [2]))
        host.browser("result/page/+")
        self.assertEqual(host.ui.cuias[-1], ("SELECT", [6]))
        report = host.report()
        self.assertEqual(report["/browser/result/cursor/name"], "Special")
        self.assertEqual(host.items("/browser/result/"), NAMES[4:8], "The page with the cursor")
        self.assertEqual(report["/browser/column/0/cursorIndex"], "2")
        host.browser("result/page/+")
        host.browser("result/+")
        self.assertEqual(host.ui.cuias[-1], ("SELECT", [7]), "Stops at the last entry")
        sent = len(host.ui.cuias)
        host.browser("result/+")
        self.assertEqual(len(host.ui.cuias), sent, "No CUIA without a move")
        host.browser("column/0/reset")
        self.assertEqual(host.ui.cuias[-1], ("SELECT", [0]))
        host.browser("result/-")
        self.assertEqual(host.ui.screen.index, 0)
        self.assertEqual(host.commands.unhandled, [])

    def test_commit_runs_the_entry(self):
        host = BrowsingHost()
        host.command("track", "2/select", 1)
        host.report()
        host.browser("stop", 1)
        self.assertEqual(host.ui.cuias[-2:], [("SELECT", [0]), ("SELECT_ACTION", [])])
        self.assertEqual((host.ui.chosen, host.ui.current_screen), ("Instrument", "engine"),
                         "zynthian goes on with the engine of the new chain")
        sent = len(host.ui.cuias)
        host.browser("stop", 1)  # The Mk2's browse mode commits again when it is left
        self.assertEqual(len(host.ui.cuias), sent)

    def test_the_engines_follow(self):
        host = BrowsingHost()
        lv = host.reporter.last_values
        host.command("track", "2/select", 1)
        host.report()
        host.browser("stop", 1)
        host.report()
        self.assertEqual(lv["/browser/exists"], "0", "A gap: the core enters its browse mode again")
        self.assertFalse(host.model.browser.add_chain.is_open)
        host.report()
        self.assertEqual(lv["/browser/exists"], "1")
        self.assertEqual(lv["/browser/contentType/selected/name"], "Engine")
        self.assertEqual(lv["/browser/column/0/name"], "Category")
        self.assertEqual(host.items("/browser/column/0/item/"), ["Synth", "Sampler"])
        self.assertEqual(lv["/browser/column/0/cursor/name"], "Sampler", "The category the screen shows")
        self.assertEqual(host.items("/browser/result/"), ["FluidSynth", "LinuxSampler"])
        self.assertEqual(lv["/browser/result/cursor/name"], "FluidSynth")
        host.browser("result/+")
        self.assertEqual(host.ui.cuias[-1], ("SELECT", [1]))
        host.browser("column/0/-")
        self.assertEqual(host.ui.cuias[-1], ("ARROW_LEFT", []))
        self.assertEqual(host.ui.engine_screen.entries[0]["title"], "ZynAddSubFX")
        host.report()
        self.assertEqual(host.items("/browser/result/"), ["ZynAddSubFX", "OBXD"])
        self.assertEqual(lv["/browser/result/cursor/name"], "ZynAddSubFX")
        host.browser("column/0/-")
        self.assertEqual(host.ui.cuias[-1], ("ARROW_LEFT", []), "No move at the first category")
        host.browser("result/+")
        host.browser("stop", 1)
        self.assertEqual(host.ui.cuias[-2:], [("SELECT", [1]), ("SELECT_ACTION", [])])
        self.assertEqual((host.ui.engine, host.ui.current_screen), ("OBXD", "midi_chan"), "zynthian asks for the MIDI channel")
        host.report()
        host.report()
        self.assertEqual(lv["/browser/exists"], "0", "The browser closes")
        self.assertEqual(host.commands.unhandled, [])

    def test_engine_adds_an_audio_chain(self):
        host = BrowsingHost()
        lv = host.reporter.last_values
        host.report()
        host.command("track", "2/select", 1)
        host.browser("result/+")
        host.browser("stop", 1)
        host.report()
        host.report()
        self.assertEqual(host.items("/browser/result/"), ["None", "EQ"], "None first, the mixer left out")
        self.assertEqual(lv["/browser/column/0/cursor/name"], "Other")
        host.browser("result/+")
        host.browser("stop", 1)
        host.report()
        self.assertEqual(lv["/browser/exists"], "0")
        self.assertEqual((lv["/track/2/name"], lv["/track/2/select"]), ("EQ", "1"), "The new chain, selected")
        self.assertEqual(lv["/track/3/name"], "+")

    def test_engine_cancel_goes_back_to_the_add_chain_view(self):
        host = BrowsingHost()
        lv = host.reporter.last_values
        host.command("track", "2/select", 1)
        host.browser("result/+")
        host.browser("result/+")
        host.browser("result/+")
        host.browser("stop", 1)
        host.report()
        host.report()
        self.assertEqual(lv["/browser/contentType/selected/name"], "Engine")
        chains = len(host.state.chain_manager.chains)
        host.browser("stop", 0)
        self.assertEqual(host.ui.cuias[-1], ("BACK", []))
        self.assertEqual(host.ui.current_screen, "add_chain", "zynthian's BACK: the Add Chain view")
        host.report()
        self.assertEqual(lv["/browser/exists"], "0")
        host.report()
        self.assertEqual(lv["/browser/exists"], "1")
        self.assertEqual(lv["/browser/contentType/selected/name"], "Add chain")
        self.assertEqual(lv["/browser/result/cursor/name"], "MIDI", "The entry chosen before")
        self.assertEqual(len(host.state.chain_manager.chains), chains, "No chain was created")
        host.browser("stop", 0)
        self.assertEqual(host.ui.cuias[-1], ("BACK", []))
        self.assertEqual(host.ui.current_screen, "mixer")
        host.report()
        host.report()
        self.assertEqual(lv["/browser/exists"], "0")

    def test_back_on_the_engine_screen_in_the_ui(self):
        host = BrowsingHost()
        lv = host.reporter.last_values
        host.command("track", "2/select", 1)
        host.browser("stop", 1)
        host.report()
        host.report()
        host.ui.send_cuia("BACK")
        host.report()
        host.report()
        self.assertEqual((lv["/browser/exists"], lv["/browser/contentType/selected/name"]), ("1", "Add chain"))

    def test_commit_adds_a_clip_launcher_and_selects_it(self):
        host = BrowsingHost()
        lv = host.reporter.last_values
        host.report()
        host.command("track", "2/select", 1)
        host.browser("result/+")
        host.browser("result/+")
        host.browser("stop", 1)
        self.assertEqual(host.ui.chosen, "Clip Launcher")
        host.report()
        host.report()
        self.assertEqual(lv["/browser/exists"], "0", "No engine screen: the browser closes")
        self.assertFalse(host.model.browser.add_chain.busy)
        self.assertEqual((lv["/track/2/name"], lv["/track/2/select"]), ("Clip Launcher", "1"))
        self.assertEqual(lv["/track/3/name"], "+")

    def test_cancel_adds_none(self):
        host = BrowsingHost()
        host.command("track", "2/select", 1)
        host.report()
        host.browser("result/+")
        chains = len(host.state.chain_manager.chains)
        host.browser("stop", 0)
        self.assertEqual(host.ui.cuias[-1], ("BACK", []))
        self.assertEqual(host.ui.current_screen, "mixer")
        self.assertEqual((host.ui.chosen, len(host.state.chain_manager.chains)), (None, chains))
        host.report()
        self.assertEqual(host.reporter.last_values["/browser/exists"], "0")

    def test_cancel_with_the_view_shown_before(self):
        host = BrowsingHost(screen="add_chain")
        host.command("track", "2/select", 1)
        host.browser("stop", 0)
        self.assertEqual(host.ui.cuias, [("ADD_CHAIN", [])], "No BACK: the view was shown before")

    def test_back_in_the_ui_closes(self):
        host = BrowsingHost()
        host.command("track", "2/select", 1)
        host.report()
        host.ui.send_cuia("BACK")
        host.report()
        self.assertEqual(host.reporter.last_values["/browser/exists"], "0")
        sent = len(host.ui.cuias)
        host.browser("stop", 1)
        self.assertEqual(len(host.ui.cuias), sent)

    def test_entry_tapped_in_the_ui_closes(self):
        host = BrowsingHost()
        host.command("track", "2/select", 1)
        host.report()
        host.ui.activate(3)  # MIDI: the engine screen
        host.report()
        self.assertEqual(host.reporter.last_values["/browser/exists"], "0")
        host.browser("stop", 1)
        self.assertEqual(host.ui.cuias[-1], ("ADD_CHAIN", []), "Nothing to commit while the engines follow")
        host.report()
        self.assertEqual(host.reporter.last_values["/browser/contentType/selected/name"], "Engine", "They follow")

    def test_the_view_s_own_entries(self):
        titles = list(QT_ENTRIES)
        titles[2] = "Clip Launcher\nMax. 16 reached!"
        host = BrowsingHost(titles=titles)
        host.command("track", "2/select", 1)
        host.report()
        host.browser("result/+")
        host.browser("result/+")
        report = host.report()
        self.assertEqual(report["/browser/result/cursor/name"], "Clip Launcher Max. 16 reached!")

    def test_the_view_filled_later(self):
        """Before the view was filled once (the UI runs ADD_CHAIN later): zynthian's entries
        as add_chain.ENTRIES, then the view's."""
        host = BrowsingHost(titles=QT_ENTRIES + ["Favourite"])
        ui = host.ui
        queued = []
        ui_send = ui.send_cuia
        ui.send_cuia = lambda cuia, params=None: queued.append((cuia, params))
        host.command("track", "2/select", 1)
        host.browser("result/+")
        host.report()
        self.assertEqual(host.reporter.last_values["/browser/result/cursor/name"], "Audio Input")
        for cuia, params in queued:
            ui_send(cuia, params)
        ui.send_cuia = ui_send
        host.report()
        self.assertEqual(host.reporter.last_values["/browser/result/cursor/name"], "Audio Input")
        self.assertEqual(ui.screen.index, 1)
        host.browser("result/page/+")
        host.browser("result/page/+")
        host.report()
        self.assertEqual(host.reporter.last_values["/browser/result/cursor/name"], "Favourite")

    def test_browse_closes_the_session(self):
        host = BrowsingHost()
        browser = host.model.browser
        host.browser("stop", 0)  # Nothing open: ignored
        host.command("track", "2/select", 1)
        host.browser("replace/device")
        self.assertFalse(browser.add_chain.is_open, "BROWSE on the Add Chain session: the presets (here: none)")
        self.assertEqual(host.ui.cuias[-1], ("BACK", []))

    def test_without_the_flag_or_ui(self):
        host = BrowsingHost()
        host.model.browse_add_chain = False
        host.command("track", "2/select", 1)
        self.assertEqual((host.ui.cuias, host.state.cuias), ([], ADD_CHAIN))
        self.assertEqual(host.report()["/browser/exists"], "0")
        host = BrowsingHost()
        host.model.browser.add_chain.ui.get_ui = lambda: None
        host.command("track", "2/select", 1)
        self.assertEqual((host.ui.cuias, host.state.cuias), ([], ADD_CHAIN), "Headless: the CUIA only")
        self.assertEqual(host.report()["/browser/exists"], "0")


class MaschineMk2AddChainTest(unittest.TestCase):
    """The Mk2's group button E on the placeholder (four chains, then "+"): the browse mode on the
    Add Chain view, the encoder moves, its push adds."""

    def setUp(self):
        from test_browser import display
        from zyngine.ctrldev.dbm_zynthian.add_chain import AddChainUI
        from zyngine.ctrldev.zynthian_ctrldev_dbm_maschine_mk2 import zynthian_ctrldev_dbm_maschine_mk2 as mk2
        self.display_of = display
        self.temp = tempfile.TemporaryDirectory(prefix="dbm-zynthian-add-chain-mk2-")
        self.addCleanup(self.temp.cleanup)
        self.state = zynstubs.project()
        self.model = ZynthianModel(self.state.chain_manager, palette=["#0000FF", "#BBBB00", "#FF00FF"])
        self.model.send_cuia = self.state.send_cuia
        self.ui = FakeQtUI(self.state)
        self.model.browse_add_chain = mk2.browse_add_chain
        self.model.browser.add_chain.ui = AddChainUI(self.ui, lambda: self.ui)
        self.midi_out = []
        self.bridge = ZynBridge(self.model, mk2.dbm_controller_id, self.temp.name, "zynthian", midi_sink=self.midi_out.append)
        self.addCleanup(self.bridge.close)
        self.bridge.start()
        self.bridge.wait_for_startup()
        self.bridge.advance(100)

    def press(self, cc):
        self.bridge.send([0xB0, cc, 127])
        self.bridge.send([0xB0, cc, 0], 200)
        self.bridge.advance(200)

    def turn(self, steps):
        for _ in range(abs(steps)):
            self.bridge.send([0xB0, MK2_ENCODER, 1 if steps > 0 else 127], 100)
        self.bridge.advance(200)

    def cell(self, row, column):
        return self.display_of(self.midi_out)[row][column * 7:column * 7 + 7].strip()

    def test_group_button_browses_the_add_chain_view(self):
        self.press(MK2_GROUP_A + 4)
        self.assertEqual(self.ui.cuias, [("ADD_CHAIN", [])])
        self.assertTrue(self.model.browser.add_chain.is_open)
        self.assertEqual((self.cell(0, 0), self.cell(1, 0)), ("Chain", "Instru"), self.display_of(self.midi_out))
        self.assertEqual((self.cell(0, 7), self.cell(1, 7)), (">Add c", "Instru"), self.display_of(self.midi_out))
        self.turn(2)
        self.assertEqual(self.cell(1, 7), "Clip L", self.display_of(self.midi_out))
        self.assertEqual(self.ui.screen.index, 2, "zynthian's view follows")
        self.turn(-1)
        self.assertEqual(self.cell(1, 0), "Audio", self.display_of(self.midi_out))
        self.turn(1)
        self.press(MK2_ENCODER_PUSH)
        self.assertEqual(self.ui.cuias[-2:], [("SELECT", [2]), ("SELECT_ACTION", [])])
        self.assertFalse(self.model.browser.add_chain.is_open)
        self.assertNotEqual(self.cell(0, 0), "Chain", "The Mk2 left the browse mode")
        self.assertEqual(self.state.chain_manager.active_chain.title, "Clip Launcher")
        self.assertEqual(self.bridge.reporter.last_values["/track/4/name"], "Clip Launcher")
        self.assertEqual(self.bridge.reporter.last_values["/track/4/select"], "1", "The new chain is selected")
        self.assertEqual(list(self.bridge.core_errors()), [])
        self.assertFalse([k for k in self.bridge.commands.unhandled if k.startswith("browser")], self.bridge.commands.unhandled)

    def test_then_the_engines(self):
        self.press(MK2_GROUP_A + 4)
        self.press(MK2_ENCODER_PUSH)  # Instrument
        self.assertEqual(self.ui.current_screen, "engine")
        self.bridge.advance(300)
        shown = self.display_of(self.midi_out)
        self.assertEqual((self.cell(0, 0), self.cell(1, 0)), ("Ctgory", "Sample"), shown)
        self.assertEqual((self.cell(0, 7), self.cell(1, 7)), (">Engin", "FluidS"), shown)
        self.turn(1)
        self.assertEqual(self.cell(1, 7), "LinuxS", self.display_of(self.midi_out))
        self.assertEqual(self.ui.engine_screen.index, 1, "zynthian's engine screen follows")
        # Knob 1: the category
        for _ in range(3):
            self.bridge.send([0xB0, 70, 127], 100)
        self.bridge.advance(200)
        self.assertEqual((self.cell(1, 0), self.cell(1, 7)), ("Synth", "ZynAdd"), self.display_of(self.midi_out))
        self.assertEqual(self.ui.cuias[-1], ("ARROW_LEFT", []))
        self.press(MK2_ENCODER_PUSH)
        self.assertEqual((self.ui.engine, self.ui.current_screen), ("ZynAddSubFX", "midi_chan"))
        self.bridge.advance(300)
        self.assertNotEqual(self.cell(0, 0), "Ctgory", "The Mk2 left the browse mode")
        self.assertFalse(self.model.browser.add_chain.busy)
        self.assertEqual(list(self.bridge.core_errors()), [])
        self.assertFalse([k for k in self.bridge.commands.unhandled if k.startswith("browser")], self.bridge.commands.unhandled)

    def test_engine_cancel_back_to_the_chain_types(self):
        self.press(MK2_GROUP_A + 4)
        self.turn(1)
        self.press(MK2_ENCODER_PUSH)  # Audio Input
        self.bridge.advance(300)
        self.assertEqual(self.cell(0, 7), ">Engin", self.display_of(self.midi_out))
        self.press(MK2_BROWSE)
        self.bridge.advance(300)
        self.assertEqual(self.ui.current_screen, "add_chain")
        self.assertEqual((self.cell(0, 7), self.cell(1, 7)), (">Add c", "Audio"), self.display_of(self.midi_out))
        self.press(MK2_BROWSE)
        self.bridge.advance(300)
        self.assertEqual(self.ui.current_screen, "mixer")
        self.assertNotEqual(self.cell(0, 0), "Chain", "The Mk2 left the browse mode")
        self.assertEqual(list(self.bridge.core_errors()), [])

    def test_browse_cancels(self):
        chains = len(self.state.chain_manager.chains)
        self.press(MK2_GROUP_A + 4)
        self.turn(1)
        self.press(MK2_BROWSE)
        self.assertEqual(self.ui.cuias[-1], ("BACK", []))
        self.assertEqual(self.ui.current_screen, "mixer")
        self.assertFalse(self.model.browser.add_chain.is_open)
        self.assertEqual(len(self.state.chain_manager.chains), chains)
        self.assertEqual(self.state.chain_manager.active_chain.chain_id, 1)
        self.assertNotEqual(self.cell(0, 0), "Chain", "The Mk2 left the browse mode")
        self.assertEqual(list(self.bridge.core_errors()), [])


# The Mk2's controls (src/controller/maschine/mk/surface.rs)
MK2_GROUP_A = 100
MK2_ENCODER = 7
MK2_ENCODER_PUSH = 8
MK2_BROWSE = 40

if __name__ == "__main__":
    unittest.main()
