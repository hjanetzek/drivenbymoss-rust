# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: the cost of an idle tick (doc/ZYNTHIAN_CPU.md)
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

"""The Maschine Mk2 against the zynthian stand-ins, idle (no input, transport stopped): the
``put`` calls and the time of ``collect()`` per tick, printed (machine dependent, so only loosely
asserted). doc/ZYNTHIAN_CPU.md ("Done") has the numbers before and after."""

import sys
import tempfile
import time
import unittest

import zynstubs

zynstubs.install()

from zyngine.ctrldev.dbm_zynthian.bridge import ZynBridge  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.model import ZynthianModel  # noqa: E402
from zyngine.ctrldev.zynthian_ctrldev_base_dbm import launcher_palette  # noqa: E402
from zyngine.ctrldev.zynthian_ctrldev_dbm_maschine_mk2 import zynthian_ctrldev_dbm_maschine_mk2 as mk2  # noqa: E402

TICKS = 300
TICK_MS = 30


def project(chains=8, phrases=8):
    """More chains and phrases than zynstubs' project: closer to a real setup (the Duet's profile
    had ~920 puts per collect)."""
    state = zynstubs.project()
    for n in range(chains - 4):
        state.add_chain(10 + n, f"Synth {n + 2}", midi_chan=3 + n)
    state.zynseq = zynstubs.FakeZynseq(phrases=phrases)
    state.add_processor(1, "Synth A", [(f"screen {n}", [f"p{n}{i}" for i in range(4)]) for n in range(4)])
    libseq = state.zynseq.libseq
    for chan in range(3):
        libseq.addPattern(0, 0, chan, 0, 0, libseq.createPattern(), False)
    return state


class IdleTickTest(unittest.TestCase):

    def test_idle_tick(self):
        temp = tempfile.TemporaryDirectory(prefix="dbm-zynthian-cpu-")
        self.addCleanup(temp.cleanup)
        state = project()
        model = ZynthianModel(state.chain_manager, palette=launcher_palette(), zynseq=state.zynseq)
        bridge = ZynBridge(model, mk2.dbm_controller_id, temp.name, "zynthian", midi_sink=lambda data: None)
        self.addCleanup(bridge.close)
        bridge.start()
        bridge.wait_for_startup()
        bridge.advance(2000)

        reporter = bridge.reporter
        # The time of the collection (without counting: the counting wrapper costs as much as a
        # put): collect() of the text protocol, collect_pass() of the binary one (its buffer is
        # taken by the bridge)
        name = "collect_pass" if reporter.binary else "collect"
        collect = getattr(reporter, name)
        times = []

        def timed_collect():
            start = time.perf_counter()
            result = collect()
            times.append(time.perf_counter() - start)
            return result

        setattr(reporter, name, timed_collect)
        full = getattr(reporter, "full_passes", None)
        for _ in range(TICKS):
            bridge.advance(TICK_MS)
        delattr(reporter, name)
        if full is not None:
            print(f"\nidle Mk2: {reporter.full_passes - full} full passes of {TICKS}", file=sys.stderr)

        # The put calls
        puts = [0]
        put = reporter.put

        def counting_put(address, value):
            puts[0] += 1
            put(address, value)

        reporter.put = counting_put
        for _ in range(TICKS):
            bridge.advance(TICK_MS)
        del reporter.put

        per_tick = puts[0] / TICKS
        mean_us = sum(times) / len(times) * 1e6
        ordered = sorted(times)
        median_us = ordered[len(ordered) // 2] * 1e6
        max_us = ordered[-1] * 1e6
        print(f"\nidle Mk2 ({reporter.protocol}), {TICKS} ticks: {per_tick:.0f} put per tick, {name}() mean {mean_us:.0f} µs, "
              f"median {median_us:.0f} µs, max {max_us:.0f} µs", file=sys.stderr)
        self.assertGreater(per_tick, 0)
