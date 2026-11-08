# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: the coverage of the commands (doc/TYPED_PROTOCOL_PLAN.md §2.5)
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


"""The handled commands of ZynCommandHandler (passed to the core, which derives the command
features from them) are exact (dbm_host.coverage); the typed commands on zynthian's model."""

import logging
import unittest

import zynstubs

zynstubs.install()

import drivenbymoss  # noqa: E402
from dbm_host import Banks  # noqa: E402
from dbm_host.coverage import unhandled_commands  # noqa: E402
from drivenbymoss import protocol  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.commands import ZynCommandHandler  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.model import ZynthianModel  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.reporter import ZynReporter  # noqa: E402


# The commands zynthian handles only with zynseq (the transport, the clip launcher)
ZYNSEQ_COMMANDS = {protocol.Cmd.TRANSPORT_PLAY, protocol.Cmd.TRANSPORT_STOP, protocol.Cmd.TRANSPORT_STOP_AND_REWIND,
                   protocol.Cmd.TRANSPORT_RESTART, protocol.Cmd.TRANSPORT_METRONOME, protocol.Cmd.TRANSPORT_TOGGLE_METRONOME,
                   protocol.Cmd.TRANSPORT_METRONOME_VOLUME, protocol.Cmd.TRANSPORT_TAP_TEMPO, protocol.Cmd.TRANSPORT_OVERDUB_ARRANGER,
                   protocol.Cmd.TRANSPORT_OVERDUB_LAUNCHER, protocol.Cmd.TEMPO_STEP, protocol.Cmd.TEMPO_SET, protocol.Cmd.RECORD_TOGGLE,
                   protocol.Cmd.TRACK_STOP, protocol.Cmd.TRACK_STOP_ALT, protocol.Cmd.TRACK_CREATE_CLIP, protocol.Cmd.SLOT_ACTION,
                   protocol.Cmd.SLOT_PASTE, protocol.Cmd.SCENE_ACTION, protocol.Cmd.SCENE_STOP_ALL, protocol.Cmd.SCENE_STOP_ALL_ALT,
                   protocol.Cmd.PROJECT_CREATE_SCENE, protocol.Cmd.PROJECT_CREATE_SCENE_FROM_PLAYING_LAUNCHER_CLIPS}


def handler(zynseq=True):
    """A ZynCommandHandler on zynstubs' project, with zynseq (the transport and the launcher) or
    without."""
    state = zynstubs.project()
    if zynseq:
        state.zynseq = zynstubs.FakeZynseq(phrases=4)
    else:
        state.zynseq = None
    model = ZynthianModel(state.chain_manager, palette=["#0000FF"], zynseq=state.zynseq)
    banks = Banks.from_model_setup({"numTracks": 8, "numSends": 2, "noteClips": [[16, 8]], "deviceIds": ["FIRST_INSTRUMENT"]})
    commands = ZynCommandHandler(model, banks, ZynReporter(model, banks))
    commands.configure()
    return commands


class CommandCoverageTest(unittest.TestCase):

    def test_handled_commands(self):
        """The handled commands (passed to the core, which derives the command features from
        them) are the IDs with a handler: with zynseq all but the snapshot UNHANDLED_COMMANDS,
        without zynseq also not the transport and the clip launcher. A record of each handled ID
        is handled (except the browser's replace/device and the Reaper form of the bypass: they need
        a device in the stand-ins), a record of every other ID is not."""
        logging.disable(logging.CRITICAL)
        self.addCleanup(logging.disable, logging.NOTSET)
        handled = handler().handled_commands()
        self.assertEqual(sorted(protocol.Cmd(i).name for i in set(protocol.CMD_IDS) - handled),
                         sorted(protocol.Cmd(i).name for i in ZynCommandHandler.UNHANDLED_COMMANDS), "The snapshot")
        for zynseq in (True, False):
            handled = handler(zynseq).handled_commands()
            unhandled = unhandled_commands(lambda: handler(zynseq))
            self.assertEqual(sorted(protocol.Cmd(i).name for i in unhandled & handled), ["BROWSER_REPLACE_DEVICE", "DEVICE_INDEX_BYPASS"],
                             f"Handled, not in the coverage test (zynseq {zynseq})")
            self.assertEqual(sorted(protocol.Cmd(i).name for i in set(protocol.CMD_IDS) - handled - unhandled), [], f"Not handled, but declared (zynseq {zynseq})")
        without = set(protocol.CMD_IDS) - handler(False).handled_commands() - ZynCommandHandler.UNHANDLED_COMMANDS
        self.assertEqual(sorted(protocol.Cmd(i).name for i in without), sorted(protocol.Cmd(i).name for i in ZYNSEQ_COMMANDS))

    def test_log_and_values(self):
        """The typed handlers on zynthian's model; the log shows the core's text."""
        lines = ["double\ttrack\t1/volume\t0.25", "int\ttrack\t1/mute\t1", "double\ttrack\t1/send/0/volume/inc\t0.125",
                 "int\ttrack\t1/select\t1", "double\tdevice\tparam/0/value\t0.75", "noArg\ttrack\t2/volume/reset"]
        commands = handler()
        commands.process_commands(drivenbymoss.messages_from_text("\n".join(lines)))
        self.assertEqual([(c.processor, c.full_command, c.number) for c in commands.log],
                         [("track", "1/volume", 0.25), ("track", "1/mute", 1.0), ("track", "1/send/0/volume/inc", 0.125),
                          ("track", "1/select", 1.0), ("device", "param/0/value", 0.75), ("track", "2/volume/reset", 1.0)])
        self.assertEqual(commands.unhandled, [])
        track = commands.daw.track_list(True)[1]
        self.assertEqual((track.selected, track.mute, track.volume.value), (True, True, 0.25))


if __name__ == "__main__":
    unittest.main()
