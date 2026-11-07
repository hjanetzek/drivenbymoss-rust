#!/usr/bin/python3
# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# Base class of the DrivenByMoss drivers: a controller run by the DrivenByMoss core
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

# The ctrldev manager skips the modules whose names start with "zynthian_ctrldev_base",
# so this module is not a driver itself. Each controller has a thin module with a subclass
# which sets dev_ids, dbm_controller_id and the names (zynthian_ctrldev_dbm_*.py).
#
# Threads: all calls into the core (dbm_*) run on the driver's tick thread. midi_event (the
# state manager's fast thread) only queues the MIDI; get_state, set_state, sleep_on/off and the
# signal callbacks (refresh) only set flags or cached values which the tick thread uses. The
# core's commands therefore change zynthian's mixer from the tick thread, as the stock drivers
# do from the fast thread.

import os
import logging
from time import monotonic
from queue import SimpleQueue, Empty
from threading import Thread, Event

# Zynthian specific modules
import zynautoconnect
from zyncoder.zyncore import lib_zyncore
from zyngine.zynthian_signal_manager import zynsigman
from zyngine.ctrldev.zynthian_ctrldev_base import zynthian_ctrldev_base
from zyngine.ctrldev.dbm_zynthian.bridge import ZynBridge
from zyngine.ctrldev.dbm_zynthian.browser import BrowserUI
from zyngine.ctrldev.dbm_zynthian.model import ZynthianModel

# ------------------------------------------------------------------------------------------------------------------
# DrivenByMoss control device base class
# ------------------------------------------------------------------------------------------------------------------

# The tick of the core (doc/HOST_BRIDGE_GUIDE.md §1: every 30 ms)
TICK_SECONDS = 0.03

# How often the document settings are read for get_state
DOCUMENT_CACHE_SECONDS = 1.0

# The largest message dev_send_midi_event takes: the ring buffer of a zmop has 16384 bytes
# (JACK_MIDI_BUFFER_SIZE in zynmidirouter.h), of which 16383 can be used
MAX_MIDI_OUT_BYTES = 16383


def launcher_palette():
    """The colors of the MIDI channels (zynthian_gui_config.LAUNCHER_COLOUR) as "#RRGGBB"."""
    try:
        from zyngui import zynthian_gui_config
        return [colour["rgb"] for colour in zynthian_gui_config.LAUNCHER_COLOUR[:16]]
    except Exception:
        return []


class zynthian_ctrldev_base_dbm(zynthian_ctrldev_base):

    dev_ids = []
    dbm_controller_id = None    # The UUID of the controller in the core (dbm_list_controllers)
    unroute_from_chains = True  # The core owns all MIDI of the device
    autoload_flag = False       # The subclasses decide
    replaces_dev_ids = []       # The device IDs of the device's other ports whose drivers this one replaces
    with_vu = True              # Report the peak meters of the mixer strips

    def __init__(self, state_manager, idev_in, idev_out=None):
        super().__init__(state_manager, idev_in, idev_out)
        self.model = ZynthianModel(self.chain_manager, palette=launcher_palette(), with_vu=self.with_vu,
                                   zynseq=self.zynseq)
        self.bridge = None
        self.thread = None
        self.midi_queue = SimpleQueue()
        self.wake = Event()
        self.exit_flag = False
        self.sleeping = False
        self.dump_requested = False
        self.document = None            # The core's document settings (cached for get_state)
        self.pending_document = None    # Document settings to apply (set_state)
        self.start_failed = False
        self._oversize_logged = False
        # The notes the driver sends to the chains: (source, key) -> (MIDI channel, key), source
        # "in" for the device's keys (fast thread), "out" for the core's played notes
        self.sounding = {}
        self.model.note_sink = self.send_core_note
        # The core's browser shows zynthian's preset view (CUIAs)
        self.model.browser.ui = BrowserUI(state_manager)

    @classmethod
    def get_config_dir(cls):
        """The folder of the core's controller configurations (<model>.config)."""
        return os.environ.get("ZYNTHIAN_CONFIG_DIR", "/zynthian/config") + "/ctrldev/dbm"

    @classmethod
    def get_app_version(cls):
        return "zynthian"

    # ---------------------------------------------------------------------------
    # Driver life cycle (called by the ctrldev manager)
    # ---------------------------------------------------------------------------

    def init(self):
        # Before the core talks to the device: no second driver on another port of it
        self.unload_replaced_drivers()
        # Registers the chain and snapshot signals, which call refresh
        super().init()
        self.exit_flag = False
        self.thread = Thread(target=self.run, args=())
        self.thread.name = f"dbm {self.get_driver_name()}"
        self.thread.daemon = True
        self.thread.start()

    def end(self):
        self.exit_flag = True
        self.wake.set()
        if self.thread and self.thread.is_alive():
            self.thread.join(5)
        self.thread = None
        super().end()

    def unload_replaced_drivers(self):
        """A device with ports for more than one driver (the Launchpads: zynthian's own driver
        on the DAW port, DrivenByMoss on the MIDI port) can only have one of them: loading this
        driver unloads the other one, of the same device (the part of the uid before "/"), as
        "disabled", so that the ZS3 and snapshots keep the choice (doc/ZYNTHIAN_PLAN.md §7b)."""
        if not self.replaces_dev_ids:
            return
        manager = getattr(self.state_manager, "ctrldev_manager", None)
        uid = zynautoconnect.get_midi_in_uid(self.idev)
        if manager is None or not uid or "/" not in uid:
            return
        device = uid.split("/", 1)[0]
        for izmip, driver in list(manager.drivers.items()):
            other = zynautoconnect.get_midi_in_uid(izmip)
            if izmip == self.idev or not other or "/" not in other:
                continue
            other_device, dev_id = other.split("/", 1)
            if other_device == device and dev_id in self.replaces_dev_ids:
                logging.info(f"[{self.__class__.__name__}] Unloading '{driver.get_driver_name()}' on '{dev_id}': "
                             f"DrivenByMoss runs the device")
                manager.unload_driver(izmip, True)

    def refresh(self):
        """Chains added, removed or moved, a snapshot loaded: report everything again."""
        super().refresh()
        self.dump_requested = True
        self.wake.set()

    def midi_event(self, ev):
        """MIDI from the device (fast thread): notes of the note inputs are played at once
        (play_note), everything is queued for the core (tick thread)."""
        # Words of a SysEx which zynmidi_read takes for events have no status byte (§9 Z4)
        if ev and ev[0] >= 0x80:
            data = bytes(ev)
            if len(data) >= 3 and data[0] & 0xE0 == 0x80:
                self.play_note(data)
            self.midi_queue.put(data)
            self.wake.set()
        return True

    # ---------------------------------------------------------------------------
    # The note inputs: the notes into the active chain (doc/ZYNTHIAN_PLAN.md §3.4)
    # ---------------------------------------------------------------------------

    def active_midi_chan(self):
        chain = self.chain_manager.get_active_chain()
        chan = getattr(chain, "midi_chan", None)
        return chan if isinstance(chan, int) and 0 <= chan < 16 else None

    def play_note(self, data):
        """A note of the device (fast thread): through the core's note inputs (filters, key and
        velocity tables) to the active chain, without waiting for the tick. A note-off goes to
        the channel and key of its note-on, also if the active chain or the tables changed."""
        bridge = self.bridge
        if bridge is None:
            return
        key = data[1]
        on = data[0] & 0xF0 == 0x90 and data[2] > 0
        if not on:
            sounding = self.sounding.pop(("in", key), None)
            if sounding is not None:
                self.send_note(False, sounding[0], sounding[1], 0)
            return
        event = bridge.translate_note(data)
        chan = self.active_midi_chan()
        if event is None or chan is None:
            return
        _, out_key, velocity = event
        previous = self.sounding.pop(("in", key), None)
        if previous is not None:
            self.send_note(False, previous[0], previous[1], 0)
        self.sounding[("in", key)] = (chan, out_key)
        self.send_note(True, chan, out_key, velocity)

    def send_core_note(self, status, key, velocity):
        """A note the core plays (process_midi_arg: the further notes of a chord; tick thread)."""
        on = status & 0xF0 == 0x90 and velocity > 0
        if on:
            chan = self.active_midi_chan()
            if chan is None:
                return
            self.sounding[("out", key)] = (chan, key)
            self.send_note(True, chan, key, velocity)
        else:
            sounding = self.sounding.pop(("out", key), None)
            if sounding is not None:
                self.send_note(False, sounding[0], sounding[1], 0)

    def send_note(self, on, chan, key, velocity):
        self.model.note_played(0x90 if on else 0x80, key, velocity)
        if on:
            lib_zyncore.ui_send_note_on(chan, key, velocity)
        else:
            lib_zyncore.ui_send_note_off(chan, key, velocity)

    def light_off(self):
        """The core owns the LEDs; sleep_on stops the controller instead."""
        pass

    def sleep_on(self):
        """Power save: stop the controller (the core switches its LEDs off)."""
        self.sleeping = True
        self.wake.set()

    def sleep_off(self):
        """Wake up: start the controller again, with the document settings it had."""
        self.sleeping = False
        self.wake.set()

    def get_state(self):
        """The core's document settings belong to the snapshot. zynthian stores driver states
        per ZS3, so every ZS3 gets the current settings; only a snapshot load restores them."""
        if self.document:
            return {"dbm_document": self.document}
        return {}

    def set_state(self, state):
        """Restore the document settings when a snapshot is loaded (load_snapshot recalls its
        first ZS3 while it is busy with "load snapshot"); recalling a ZS3 leaves them as they are."""
        document = state.get("dbm_document") if state else None
        if document is None or not self.is_loading_snapshot():
            return
        self.pending_document = document
        self.document = document
        self.wake.set()

    def is_loading_snapshot(self):
        try:
            return self.state_manager.is_busy("load snapshot")
        except AttributeError:
            return True

    def is_zynthian_busy(self):
        """True while zynthian is busy (loading or saving a snapshot, …). The tick thread then
        doesn't call into the core: the core's reports read zynseq, whose scene and phrase lists
        are replaced while a snapshot loads, and reading them meanwhile crashed zynseq
        (SequenceManager::getSequence, seen on the Duet). The controller freezes for that time;
        its MIDI stays queued."""
        try:
            return bool(self.state_manager.is_busy())
        except AttributeError:
            return False

    # ---------------------------------------------------------------------------
    # The tick thread: the only thread which calls into the core
    # ---------------------------------------------------------------------------

    def run(self):
        try:
            self.loop()
        except Exception as e:
            logging.exception(f"[{self.__class__.__name__}] {e}")
        finally:
            self.stop_bridge()

    def loop(self):
        start = 0.0
        last_document = 0.0
        while not self.exit_flag:
            if self.sleeping or self.start_failed:
                self.stop_bridge()
                self.drain_midi()
                self.wake.wait(1.0)
                self.wake.clear()
                continue
            if self.is_zynthian_busy():
                # No call into the core (its reports read zynseq) while zynthian is busy
                self.wake.wait(TICK_SECONDS)
                self.wake.clear()
                continue
            if self.bridge is None:
                if not self.start_bridge():
                    continue
                start = last_document = monotonic()
            self.wake.wait(TICK_SECONDS)
            self.wake.clear()
            if self.is_zynthian_busy():
                continue
            self.step((monotonic() - start) * 1000.0)
            now = monotonic()
            if now - last_document >= DOCUMENT_CACHE_SECONDS:
                last_document = now
                self.document = self.bridge.app.get_formatted_document_settings()

    def start_bridge(self):
        """Create and start the controller; False if the core can't run it."""
        config_dir = self.get_config_dir()
        try:
            os.makedirs(config_dir, exist_ok=True)
            bridge = ZynBridge(self.model, self.dbm_controller_id, config_dir, self.get_app_version(), midi_sink=self.send_midi)
            # The driver plays the notes of the note inputs itself, as they arrive (play_note)
            bridge.play_note_inputs_in_step = False
            bridge.start()
        except Exception as e:
            logging.error(f"[{self.__class__.__name__}] Can't start DrivenByMoss controller {self.dbm_controller_id}: {e}")
            self.start_failed = True
            return False
        self.bridge = bridge
        if self.pending_document is None and self.document:
            # After sleep: the settings the controller had
            self.pending_document = self.document
        logging.info(f"[{self.__class__.__name__}] Started DrivenByMoss controller {self.dbm_controller_id}")
        return True

    def stop_bridge(self):
        # No hanging notes
        for (source, key), (chan, out_key) in list(self.sounding.items()):
            self.send_note(False, chan, out_key, 0)
        self.sounding.clear()
        if self.bridge is not None:
            try:
                self.document = self.bridge.app.get_formatted_document_settings()
            except Exception:
                pass
            try:
                self.bridge.close()
            except Exception as e:
                logging.error(f"[{self.__class__.__name__}] Error while stopping the controller: {e}")
            self.bridge = None

    def drain_midi(self):
        messages = []
        while True:
            try:
                messages.append(self.midi_queue.get_nowait())
            except Empty:
                return messages

    def step(self, now_ms):
        """One pass of the loop at now_ms (ms since the controller started)."""
        bridge = self.bridge
        if self.pending_document is not None:
            document, self.pending_document = self.pending_document, None
            bridge.app.set_formatted_document_settings(document)
        if self.dump_requested:
            self.dump_requested = False
            bridge.reporter.request_dump()
        for data in self.drain_midi():
            bridge.receive(data)
        bridge.tick(now_ms)
        for message in bridge.take_notifications():
            zynsigman.send_queued(zynsigman.S_GUI, zynsigman.SS_GUI_SHOW_MESSAGE, message=message)

    # ---------------------------------------------------------------------------
    # The core's callbacks (on the tick thread, during a call into the core)
    # ---------------------------------------------------------------------------

    def send_midi(self, data):
        """MIDI from the core to the device."""
        if self.idev_out is None:
            return
        if len(data) > MAX_MIDI_OUT_BYTES:
            if not self._oversize_logged:
                self._oversize_logged = True
                logging.warning(f"[{self.__class__.__name__}] MIDI message of {len(data)} bytes dropped (max. {MAX_MIDI_OUT_BYTES})")
            return
        data = bytes(data)
        lib_zyncore.dev_send_midi_event(self.idev_out, data, len(data))

# ------------------------------------------------------------------------------------------------------------------
