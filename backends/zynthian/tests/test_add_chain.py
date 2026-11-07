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


if __name__ == "__main__":
    unittest.main()
