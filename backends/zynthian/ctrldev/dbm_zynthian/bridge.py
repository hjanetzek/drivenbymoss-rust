# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: the loop of one controller
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

"""One controller of the core on zynthian (doc/HOST_BRIDGE_GUIDE.md §1, §5, §7).

``ZynBridge`` is the reference host's ``Bridge`` (create the controller, start it, size the
banks from the model setup, then per tick: MIDI in -> time -> tick -> report -> update_model)
with zynthian's model, reporter and command handler, and without the histories the tests of
the reference host keep (MIDI out, reports, logs).

Time: the bridge gives the core its time with ``dbm_set_time``. The driver passes the real time
(``tick(now_ms)``, milliseconds since the start); the tests advance a simulated time
(``advance(ms)``) and run without sleeping.

All calls run on the thread which calls ``start``, ``tick`` and ``close`` (the driver's tick
thread); the core's callbacks (commands, MIDI out) arrive on it during these calls.
"""

import logging
from collections import deque

from dbm_host import Bridge
from dbm_host.bridge import DEVICE

from zyngine.ctrldev.dbm_zynthian.commands import ZynCommandHandler
from zyngine.ctrldev.dbm_zynthian.reporter import ZynReporter

logger = logging.getLogger("dbm_zynthian")


class ZynBridge(Bridge):

    reporter_class = ZynReporter
    command_handler_class = ZynCommandHandler

    def __init__(self, daw, controller_id, config_path, app_version="zynthian", midi_sink=None):
        """``midi_sink(data)`` sends the core's MIDI to the controller."""
        super().__init__(daw, controller_id, config_path, app_version)
        self.midi_sink = midi_sink
        # Only the last errors of the core (``core_errors``)
        self.logs = deque(maxlen=50)

    def tick(self, now_ms):
        """One loop pass at the time now_ms (ms since the start, never going back)."""
        self.clock_ms = max(self.clock_ms, float(now_ms))
        self.step()

    def take_notifications(self):
        """The notifications of the core since the last call."""
        notifications, self.notifications = self.notifications, []
        return notifications

    # No histories (the reference host keeps them for its tests)

    def report(self):
        lines = self.reporter.collect()
        if lines:
            self.app.update_model(lines)
        return lines

    def send_midi_data(self, device_id, data):
        if device_id == DEVICE and self.midi_sink is not None:
            self.midi_sink(data)

    def log(self, message):
        if message.startswith("ERROR"):
            self.logs.append(message)
            logger.error("core: %s", message)
        else:
            logger.debug("core: %s", message)
