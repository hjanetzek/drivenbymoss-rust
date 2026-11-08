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
#
# In-process devices (hid_ids, the Maschines): zynthian's ctrldev manager loads the driver
# without a MIDI port (idev_in None) when the HID device is there; the driver drives it itself
# with libdbm_nihost (dbm_zynthian/nihost.py), with the same MIDI as dbm-nihost's ports. A
# thread of the driver reads the library's MIDI and passes it to midi_event (in place of the
# fast thread), send_midi goes to the library, and the tick thread writes the LEDs and displays
# (flush_device).

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
from zyngine.ctrldev.dbm_zynthian.add_chain import AddChainUI
from zyngine.ctrldev.dbm_zynthian.browser import BrowserUI
from zyngine.ctrldev.dbm_zynthian.model import ZynthianModel
from zyngine.ctrldev.dbm_zynthian import nihost

# ------------------------------------------------------------------------------------------------------------------
# DrivenByMoss control device base class
# ------------------------------------------------------------------------------------------------------------------

# The tick of the core (doc/HOST_BRIDGE_GUIDE.md §1: every 30 ms)
TICK_SECONDS = 0.03

# How often the document settings are read for get_state
DOCUMENT_CACHE_SECONDS = 1.0

# The messages of dbm-nihost (support/linux/ni_host/src/midi.h): the query whether the device is
# there, and the message that it is gone (unplugged, dbm-nihost stops)
NIHOST_QUERY = bytes.fromhex("F07D44424D11F7")
NIHOST_GONE = bytes.fromhex("F07D44424D13F7")

# The largest message dev_send_midi_event takes: the ring buffer of a zmop has 16384 bytes
# (JACK_MIDI_BUFFER_SIZE in zynmidirouter.h), of which 16383 can be used
MAX_MIDI_OUT_BYTES = 16383

# How long the reading thread of an in-process device waits in the library per call (also how
# long end() may wait for it)
NIHOST_READ_TIMEOUT_MS = 100


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
    browse_add_chain = False    # The "+" track also browses the Add Chain view (dbm_zynthian/add_chain.py)
    # The devices of dbm-nihost (the Maschines), whose ports exist without the device: the
    # driver is dormant (no controller) until dbm-nihost says hello (an answer to NIHOST_QUERY or
    # a device attached), and again after NIHOST_GONE. None: the device is there with its ports.
    nihost_hello = None
    # In-process (doc/ZYNTHIAN_INTEGRATION.md §10): the (vendor, product) USB ids of the HID
    # device which the driver drives itself with libdbm_nihost, and its kind in the library
    # (nihost.KIND_MK2, nihost.KIND_JAM). zynthian's ctrldev manager loads such a driver with
    # idev_in None when the device is there.
    hid_ids = []
    nihost_kind = None

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
        # dbm-nihost's device (nihost_hello): there, a query waits for its answer, a hello which
        # is no answer (attached, dbm-nihost restarted) restarts the controller
        self.device_present = self.nihost_hello is None
        self.query_pending = False
        self.restart_requested = False
        # In-process: the library's device and the thread which reads it
        self.in_process = idev_in is None and self.nihost_kind is not None
        self.nihost = None
        self.nihost_thread = None
        # The notes the driver sends to the chains: (source, key) -> (MIDI channel, key), source
        # "in" for the device's keys (fast thread), "out" for the core's played notes
        self.sounding = {}
        self.model.note_sink = self.send_core_note
        # The core's browser shows zynthian's preset view (CUIAs)
        self.model.browser.ui = BrowserUI(state_manager)
        self.model.browser.add_chain.ui = AddChainUI(state_manager)
        self.model.browse_add_chain = self.browse_add_chain
        # The "add chain" track opens zynthian's Add Chain view (model.py)
        self.model.send_cuia = state_manager.send_cuia

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
        if self.in_process:
            self.open_nihost()
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
        self.close_nihost()
        super().end()

    # ---------------------------------------------------------------------------
    # In-process devices: libdbm_nihost in place of dbm-nihost's MIDI ports
    # ---------------------------------------------------------------------------

    def open_nihost(self):
        """Open the library's device and start the thread which reads its MIDI. Without the
        library the driver stays dormant (logged)."""
        try:
            self.nihost = nihost.NihostDevice(self.nihost_kind)
        except Exception as e:
            logging.error(f"[{self.__class__.__name__}] Can't drive the device in-process (libdbm_nihost: {e})")
            self.nihost = None
            return
        self.nihost_thread = Thread(target=self.read_nihost, args=(self.nihost,))
        self.nihost_thread.name = f"dbm {self.get_driver_name()} HID"
        self.nihost_thread.daemon = True
        self.nihost_thread.start()

    def read_nihost(self, device):
        """The library's MIDI (the device's input, hello, gone, answers) to midi_event, as the
        fast thread passes a MIDI port's."""
        while not self.exit_flag:
            try:
                messages = device.read(NIHOST_READ_TIMEOUT_MS)
            except Exception as e:
                logging.error(f"[{self.__class__.__name__}] libdbm_nihost: {e}")
                return
            for data in messages:
                self.midi_event(data)

    def flush_device(self):
        """Write the LEDs and displays the core (or the library) changed to the device."""
        if self.nihost is not None:
            try:
                self.nihost.flush()
            except Exception as e:
                logging.error(f"[{self.__class__.__name__}] libdbm_nihost: {e}")

    def close_nihost(self):
        if self.nihost_thread is not None:
            self.nihost_thread.join(2 * NIHOST_READ_TIMEOUT_MS / 1000.0 + 1.0)
            self.nihost_thread = None
        if self.nihost is not None:
            self.flush_device()
            self.nihost.close()
            self.nihost = None

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
            self.flush_device()

    def loop(self):
        start = 0.0
        last_document = 0.0
        self.query_device()
        while not self.exit_flag:
            if self.sleeping or self.start_failed or not self.device_present:
                self.stop_bridge()
                self.flush_device()
                # Only dbm-nihost's hello (or gone) counts while dormant
                self.take_midi()
                if self.device_present and not self.sleeping and not self.start_failed:
                    continue
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
            self.flush_device()
            if self.restart_requested or not self.device_present:
                # dbm-nihost's device is gone, or back (a replug or a restart of dbm-nihost, whose
                # device has lost its LEDs and displays): a fresh controller sends everything
                self.restart_requested = False
                self.stop_bridge()
                continue
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

    def query_device(self):
        """Ask dbm-nihost whether its device is there (it answers with the hello if it is). An
        in-process driver without its library stays dormant."""
        if self.nihost_hello is None:
            return
        if self.in_process:
            # No query: the library opens the device after the driver's start and says hello
            # then (and after every reattach), into the driver's own queue where it can't get
            # lost (unlike through PipeWire's MIDI bridge, §9 Z16). A query besides would bring
            # a second hello, which restarts the controller when it comes after the first.
            return
        if self.idev_out is None and not self.in_process:
            # No way to ask: as if it was there
            self.device_present = True
            return
        self.send_midi(NIHOST_QUERY)

    def take_midi(self):
        """The queued MIDI for the core. dbm-nihost's messages set the device state: a hello which
        answers a query (the driver's or the core's: the Mk2's core asks whether the Shift button
        is sent) goes to the core; one which answers none (the device attached, dbm-nihost
        restarted) restarts a running controller; "gone" makes the driver dormant. Neither goes
        to the core otherwise (the core doesn't know them)."""
        messages = self.drain_midi()
        if self.nihost_hello is None:
            return messages
        result = []
        for data in messages:
            if data[0] != 0xF0:
                result.append(data)
            elif data == NIHOST_GONE:
                if self.device_present:
                    logging.info(f"[{self.__class__.__name__}] dbm-nihost: the device is gone")
                self.device_present = False
                self.query_pending = False
                result = []
            elif data == self.nihost_hello:
                if not self.device_present:
                    logging.info(f"[{self.__class__.__name__}] dbm-nihost: the device is there")
                self.device_present = True
                if self.query_pending:
                    self.query_pending = False
                    result.append(data)
                elif self.bridge is not None:
                    self.restart_requested = True
            else:
                result.append(data)
        return result

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
        for data in self.take_midi():
            bridge.receive(data)
        bridge.tick(now_ms)
        for message in bridge.take_notifications():
            zynsigman.send_queued(zynsigman.S_GUI, zynsigman.SS_GUI_SHOW_MESSAGE, message=message)

    # ---------------------------------------------------------------------------
    # The core's callbacks (on the tick thread, during a call into the core)
    # ---------------------------------------------------------------------------

    def send_midi(self, data):
        """MIDI from the core to the device."""
        if self.nihost is not None:
            data = bytes(data)
            if data == NIHOST_QUERY and self.nihost_hello is not None:
                # The library answers at once, the answer arrives through read_nihost
                self.query_pending = True
            self.nihost.send(data)
            return
        if self.idev_out is None:
            return
        if len(data) > MAX_MIDI_OUT_BYTES:
            if not self._oversize_logged:
                self._oversize_logged = True
                logging.warning(f"[{self.__class__.__name__}] MIDI message of {len(data)} bytes dropped (max. {MAX_MIDI_OUT_BYTES})")
            return
        data = bytes(data)
        if data == NIHOST_QUERY and self.nihost_hello is not None:
            # dbm-nihost's answer is a hello which goes to the core (take_midi)
            self.query_pending = True
        lib_zyncore.dev_send_midi_event(self.idev_out, data, len(data))

# ------------------------------------------------------------------------------------------------------------------
