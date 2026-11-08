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


"""Every command ID is handled by ZynCommandHandler (typed or through its text) or in its
UNHANDLED_COMMANDS, the set passed to the core (dbm_host.coverage); the typed commands of
zynthian (the channel values and flags, the parameters) do what their text does."""

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


def handler():
    """A ZynCommandHandler on zynstubs' project with zynseq (the transport and the launcher)."""
    state = zynstubs.project()
    state.zynseq = zynstubs.FakeZynseq(phrases=4)
    model = ZynthianModel(state.chain_manager, palette=["#0000FF"], zynseq=state.zynseq)
    banks = Banks.from_model_setup({"numTracks": 8, "numSends": 2, "noteClips": [[16, 8]], "deviceIds": ["FIRST_INSTRUMENT"]})
    commands = ZynCommandHandler(model, banks, ZynReporter(model, banks))
    commands.configure()
    return commands


class CommandCoverageTest(unittest.TestCase):

    def test_handled_commands(self):
        logging.disable(logging.CRITICAL)
        self.addCleanup(logging.disable, logging.NOTSET)
        unhandled = unhandled_commands(handler)
        declared = ZynCommandHandler.UNHANDLED_COMMANDS
        self.assertEqual(sorted(protocol.Cmd(i).name for i in unhandled - declared), [], "Not handled, not declared")
        self.assertEqual(sorted(protocol.Cmd(i).name for i in declared - unhandled), [], "Declared unhandled, but handled")

    def test_typed_like_text(self):
        """The typed handlers give the model and the log of the text form."""
        lines = ["track\t1/volume\t0.25", "track\t1/mute\t1", "track\t1/send/0/volume/inc\t0.125", "track\t1/select\t1",
                 "device\tparam/0/value\t0.75", "track\t2/volume/reset"]
        text, typed = handler(), handler()
        legacy = [("double\t" if "." in line else "int\t" if line.count("\t") == 2 else "noArg\t") + line for line in lines]
        for line in legacy:
            _kind, processor, command, value = drivenbymoss.legacy_command(line)
            text.handle(processor, command, value)
        records = drivenbymoss.messages_from_text("\n".join(legacy))
        typed.process_commands(records)
        self.assertEqual([type(c).__name__ for c in typed.log], ["ViewCommand"] * len(lines))
        self.assertEqual([(c.processor, c.full_command, c.number) for c in text.log], [(c.processor, c.full_command, c.number) for c in typed.log])
        self.assertEqual(text.unhandled, typed.unhandled)
        state = [[(t.name, t.selected, t.mute, t.volume.value) for t in h.daw.track_list(True) if t.has_mixer] for h in (text, typed)]
        self.assertEqual(state[0], state[1])


if __name__ == "__main__":
    unittest.main()
