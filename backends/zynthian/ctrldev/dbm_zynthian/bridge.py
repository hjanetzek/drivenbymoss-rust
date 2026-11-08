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

The two rates of the reporter (reporter.py): every command of the core (commands.py) and every
change signal of zynthian (``change_signals``, registered from ``start`` to ``close``) make the
next pass a full one. The signal callbacks run on zynthian's threads and only set the reporter's flag.
"""

import logging
from collections import deque

from dbm_host import Bridge
from dbm_host.bridge import DEVICE

from zyngine.zynthian_signal_manager import zynsigman

from zyngine.ctrldev.dbm_zynthian.commands import ZynCommandHandler
from zyngine.ctrldev.dbm_zynthian.reporter import ALL_SUBTREES, ZynReporter

logger = logging.getLogger("dbm_zynthian")


def change_signals():
    """The signals of zynthian after which the model is read again at once, with the subtrees of
    the reporter they affect: (signal, subsignal, subtrees). For now any of them makes the next
    pass collect the whole slow part (reporter.py). Not signalled by zynthian: the send levels
    and the values of the processors' parameters (also changed in zynthian's UI), names and
    note edits of the sequences, the browser's position in zynthian's preset view; the full pass
    every SLOW_INTERVAL_MS covers them."""
    s = zynsigman
    chains = {"tracks", "mixer", "slots", "device", "clip", "browser"}
    return [
        # zynmixer: level, balance, mute, solo, record arm (also from zynthian's UI)
        (s.S_MIXER, s.SS_ZYNMIXER_SET_VALUE, {"mixer"}),
        (s.S_PROCESSOR, s.SS_PROCESSOR_BYPASS, {"device"}),
        (s.S_PROCESSOR, s.SS_PROCESSOR_CTRL_SCREENS, {"device"}),
        # The active chain is the selected track, the cursor device and clip
        (s.S_CHAIN_MAN, s.SS_SET_ACTIVE_CHAIN, chains),
        (s.S_CHAIN_MAN, s.SS_MOVE_CHAIN, chains),
        (s.S_CHAIN_MAN, s.SS_ADD_CHAIN, chains),
        (s.S_CHAIN_MAN, s.SS_REMOVE_CHAIN, chains),
        (s.S_CHAIN_MAN, s.SS_REMOVE_ALL_CHAINS, chains),
        (s.S_CHAIN_MAN, s.SS_RENAME_CHAIN, {"tracks"}),
        (s.S_CHAIN_MAN, s.SS_ADD_PROCESSOR, {"device", "browser"}),
        (s.S_CHAIN_MAN, s.SS_REMOVE_PROCESSOR, {"device", "browser"}),
        (s.S_STEPSEQ, s.SS_SEQ_PLAY_STATE, {"slots"}),
        (s.S_STEPSEQ, s.SS_SEQ_STATE, {"slots", "clip"}),
        (s.S_STEPSEQ, s.SS_SEQ_SELECT_PHRASE, {"slots", "clip"}),
        (s.S_STEPSEQ, s.SS_SEQ_TEMPO, {"transport"}),
        (s.S_STEPSEQ, s.SS_SEQ_TIMESIG, {"transport", "clip"}),
        (s.S_STEPSEQ, s.SS_SEQ_METRO, {"transport"}),
        (s.S_STATE_MAN, s.SS_LOAD_ZS3, set(ALL_SUBTREES)),
    ]


class ZynBridge(Bridge):

    reporter_class = ZynReporter
    command_handler_class = ZynCommandHandler

    def __init__(self, daw, controller_id, config_path, app_version="zynthian", midi_sink=None):
        """``midi_sink(data)`` sends the core's MIDI to the controller."""
        super().__init__(daw, controller_id, config_path, app_version)
        self.midi_sink = midi_sink
        # Only the last errors of the core (``core_errors``)
        self.logs = deque(maxlen=50)
        self.play_note_inputs_in_step = True
        """False: the driver plays the notes of the note inputs as they arrive (play_note), the
        step only gives them to the core."""

    def start(self):
        # One callback per signal: it knows the subtrees of its signal
        self._signal_callbacks = []
        for signal, subsignal, subtrees in change_signals():
            callback = self.change_callback(frozenset(subtrees))
            self._signal_callbacks.append((signal, subsignal, callback))
            zynsigman.register(signal, subsignal, callback)
        try:
            return super().start()
        except Exception:
            self.unregister_signals()
            raise

    def close(self):
        self.unregister_signals()
        super().close()

    def unregister_signals(self):
        for signal, subsignal, callback in getattr(self, "_signal_callbacks", []):
            zynsigman.unregister(signal, subsignal, callback)
        self._signal_callbacks = []

    def change_callback(self, subtrees):
        def on_change_signal(**kwargs):
            """A change in zynthian (any thread): the next pass reads the subtrees again."""
            self.reporter.request_full(subtrees)
        return on_change_signal

    def tick(self, now_ms):
        """One loop pass at the time now_ms (ms since the start, never going back)."""
        self.clock_ms = max(self.clock_ms, float(now_ms))
        self.step()

    def take_notifications(self):
        """The notifications of the core since the last call."""
        notifications, self.notifications = self.notifications, []
        return notifications

    def to_note_inputs(self, data):
        if self.play_note_inputs_in_step:
            super().to_note_inputs(data)

    def translate_note(self, data):
        """The note a note input lets through (status, key, velocity) after its key and velocity
        tables, None if none does (or the key is blocked). Only Python data: may be called from
        another thread than the core's."""
        for note_input in list(self.note_inputs.values()):
            event = note_input.translate(data)
            if event is not None:
                return event
        return None

    # No histories (the reference host keeps them for its tests)

    def report(self):
        return self.send_report(keep=False)

    def send_midi_data(self, device_id, data):
        if device_id == DEVICE and self.midi_sink is not None:
            self.midi_sink(data)

    def log(self, message):
        if message.startswith("ERROR"):
            self.logs.append(message)
            logger.error("core: %s", message)
        else:
            logger.debug("core: %s", message)
