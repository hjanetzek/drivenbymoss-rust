# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: tests of the in-process Maschine drivers (libdbm_nihost)
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

"""The Maschine drivers in-process: loaded without a MIDI port (idev_in None), they drive the
device with libdbm_nihost. ``FakeNihostLib`` stands in for the library (the C functions of
dbm_nihost.h as ctypes calls them); ``NihostLibraryTest`` runs the real library's test devices
through dbm_zynthian/nihost.py when it is built (else skipped)."""

import ctypes
import os
import tempfile
import threading
import time
import unittest

import zynstubs

zynstubs.install()

from zyngine.ctrldev import zynthian_ctrldev_base_dbm as base_dbm  # noqa: E402
from zyngine.ctrldev.dbm_zynthian import nihost  # noqa: E402
from zyngine.ctrldev.zynthian_ctrldev_base_dbm import NIHOST_GONE, NIHOST_QUERY  # noqa: E402
from zyngine.ctrldev.zynthian_ctrldev_dbm_maschine_jam import zynthian_ctrldev_dbm_maschine_jam  # noqa: E402
from zyngine.ctrldev.zynthian_ctrldev_dbm_maschine_mk2 import zynthian_ctrldev_dbm_maschine_mk2  # noqa: E402

MK2_HELLO = bytes.fromhex("F07D44424D1101F7")
JAM_HELLO = bytes.fromhex("F000210915004D5000014601F7")
HELLOS = {nihost.KIND_MK2: MK2_HELLO, nihost.KIND_JAM: JAM_HELLO}


class FakeNihostLib:
    """libdbm_nihost: one device per open, with the host's presence logic (hello when attached
    and as the answer to the query, gone when unplugged). Records the MIDI it gets and the
    flushes."""

    def __init__(self, present=False):
        self.present = present
        self.kind = None
        self.handle = None
        self.closed = False
        self.queue = []
        self.condition = threading.Condition()
        self.received = []
        self.queries = 0
        self.flushes = 0
        self.reads = 0

    def push(self, data):
        with self.condition:
            self.queue.append(bytes(data))
            self.condition.notify_all()

    # The device
    def attach(self):
        self.present = True
        self.push(HELLOS[self.kind])

    def detach(self):
        self.present = False
        self.push(NIHOST_GONE)

    # The C functions
    def dbm_nihost_open(self, kind):
        self.kind = kind
        self.handle = 1234
        if self.present:
            self.push(HELLOS[kind])
        return self.handle

    def dbm_nihost_close(self, handle):
        assert handle == self.handle
        self.closed = True

    def dbm_nihost_is_attached(self, handle):
        return 1 if self.present else 0

    def dbm_nihost_read(self, handle, buffer, size, timeout_ms):
        assert not self.closed
        self.reads += 1
        with self.condition:
            if not self.queue:
                self.condition.wait(timeout_ms / 1000.0)
            used = 0
            while self.queue and used + 2 + len(self.queue[0]) <= size:
                data = self.queue.pop(0)
                buffer[used] = len(data) & 0xFF
                buffer[used + 1] = len(data) >> 8
                for i, byte in enumerate(data):
                    buffer[used + 2 + i] = byte
                used += 2 + len(data)
            return used

    def dbm_nihost_send(self, handle, data, length):
        assert not self.closed
        data = bytes(data[:length])
        self.received.append(data)
        if data == NIHOST_QUERY:
            self.queries += 1
            if self.present:
                self.push(HELLOS[self.kind])
        return 0

    def dbm_nihost_flush(self, handle):
        assert not self.closed
        self.flushes += 1
        return 0


def wait_for(condition, timeout=5.0):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if condition():
            return True
        time.sleep(0.02)
    return False


class InProcessDriverTest(unittest.TestCase):

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="dbm-zynthian-inprocess-")
        self.old_config = os.environ.get("ZYNTHIAN_CONFIG_DIR")
        os.environ["ZYNTHIAN_CONFIG_DIR"] = self.temp.name
        self.state = zynstubs.project()
        self.zyncore = zynstubs.lib_zyncore()
        self.zyncore.sent.clear()
        self.zyncore.notes.clear()
        self.old_load = nihost.load_library
        self.lib = None
        self.drivers = []

    def tearDown(self):
        for driver in self.drivers:
            driver.end()
        nihost.load_library = self.old_load
        if self.old_config is None:
            os.environ.pop("ZYNTHIAN_CONFIG_DIR", None)
        else:
            os.environ["ZYNTHIAN_CONFIG_DIR"] = self.old_config
        self.temp.cleanup()

    def start(self, cls, present):
        self.lib = FakeNihostLib(present)
        nihost.load_library = lambda: self.lib
        # As the ctrldev manager loads a HID driver: no MIDI port
        driver = cls(self.state, None, None)
        self.drivers.append(driver)
        driver.init()
        return driver

    def test_classes(self):
        self.assertEqual(zynthian_ctrldev_dbm_maschine_mk2.hid_ids, [(0x17CC, 0x1140)])
        self.assertEqual(zynthian_ctrldev_dbm_maschine_jam.hid_ids, [(0x17CC, 0x1500)])
        # The same classes still run on dbm-nihost's ports
        self.assertFalse(zynthian_ctrldev_dbm_maschine_mk2(self.state, 3, 5).in_process)
        self.assertTrue(zynthian_ctrldev_dbm_maschine_mk2(self.state, None, None).in_process)

    def test_dormant_until_attached(self):
        driver = self.start(zynthian_ctrldev_dbm_maschine_mk2, present=False)
        time.sleep(0.3)
        self.assertEqual(self.lib.queries, 0, "No query: the library says hello when it opens the device")
        self.assertIsNone(driver.bridge, "No controller without the device")
        self.assertEqual(self.zyncore.sent, [], "Nothing over zynthian's MIDI ports")
        self.lib.attach()
        self.assertTrue(wait_for(lambda: driver.bridge is not None and driver.bridge.note_inputs), "The controller starts")

    def test_connected_before(self):
        """The core's MIDI goes to the library, the LEDs are flushed on the tick; the core's own
        query gets its answer without a restart."""
        driver = self.start(zynthian_ctrldev_dbm_maschine_mk2, present=True)
        self.assertTrue(wait_for(lambda: driver.bridge is not None))
        bridge = driver.bridge
        self.assertTrue(wait_for(lambda: self.lib.queries == 1, 3.0), "The core's query")
        self.assertTrue(wait_for(lambda: len(self.lib.received) > 10), "The core's LEDs and displays")
        flushes = self.lib.flushes
        time.sleep(0.2)
        self.assertGreater(self.lib.flushes, flushes + 2, "Flushed on every tick")
        self.assertIs(driver.bridge, bridge, "An answer doesn't restart the controller")
        self.assertEqual(bridge.core_errors(), [])
        self.assertEqual(self.zyncore.sent, [])

    def test_pads_play_and_gone(self):
        driver = self.start(zynthian_ctrldev_dbm_maschine_mk2, present=True)
        self.assertTrue(wait_for(lambda: driver.bridge is not None and driver.bridge.note_inputs))
        self.assertTrue(wait_for(lambda: self.lib.queries == 1, 3.0))
        chan = driver.active_midi_chan()
        self.lib.push(b"\x90\x30\x64")
        self.assertTrue(wait_for(lambda: any(n[0] == "on" for n in self.zyncore.notes)), "The pad plays the active chain")
        self.assertEqual(self.zyncore.notes[-1][1], chan)
        self.lib.detach()
        self.assertTrue(wait_for(lambda: driver.bridge is None), "Dormant after gone")
        self.assertEqual(self.zyncore.notes[-1][0], "off", "No hanging note")
        self.lib.attach()
        self.assertTrue(wait_for(lambda: driver.bridge is not None), "Back with the hello")

    def test_answer_after_running(self):
        """The core's query, answered while the controller runs, keeps it running; a hello which
        answers nothing (a reattach without "gone") restarts it."""
        driver = self.start(zynthian_ctrldev_dbm_maschine_mk2, present=True)
        self.assertTrue(wait_for(lambda: driver.bridge is not None))
        self.assertTrue(wait_for(lambda: self.lib.queries == 1, 3.0))
        time.sleep(0.2)
        first = driver.bridge
        self.lib.push(MK2_HELLO)
        self.assertTrue(wait_for(lambda: driver.bridge is not None and driver.bridge is not first), "Restarted")

    def test_jam(self):
        driver = self.start(zynthian_ctrldev_dbm_maschine_jam, present=False)
        self.assertEqual(self.lib.kind, nihost.KIND_JAM)
        time.sleep(0.2)
        self.assertIsNone(driver.bridge)
        self.lib.attach()
        self.assertTrue(wait_for(lambda: driver.bridge is not None))
        self.assertTrue(wait_for(lambda: len(self.lib.received) > 10), "The JAM's LEDs")
        self.lib.detach()
        self.assertTrue(wait_for(lambda: driver.bridge is None))
        self.assertEqual(self.lib.queries, 0, "The JAM's core doesn't ask")

    def test_end_closes_the_device(self):
        driver = self.start(zynthian_ctrldev_dbm_maschine_mk2, present=True)
        self.assertTrue(wait_for(lambda: driver.bridge is not None))
        reader = driver.nihost_thread
        self.drivers.remove(driver)
        started = time.monotonic()
        driver.end()
        self.assertLess(time.monotonic() - started, 2.0)
        self.assertFalse(reader.is_alive())
        self.assertTrue(self.lib.closed)
        self.assertIsNone(driver.nihost)

    def test_without_the_library(self):
        """No libdbm_nihost: the driver logs it and stays dormant."""
        def missing():
            raise OSError("libdbm_nihost.so: cannot open shared object file")
        nihost.load_library = missing
        driver = zynthian_ctrldev_dbm_maschine_mk2(self.state, None, None)
        self.drivers.append(driver)
        with self.assertLogs(level="ERROR"):
            driver.init()
        time.sleep(0.2)
        self.assertIsNone(driver.nihost)
        self.assertIsNone(driver.bridge)
        self.assertFalse(driver.device_present)


class SysfsTest(unittest.TestCase):

    def test_hid_present(self):
        with tempfile.TemporaryDirectory() as root:
            for name, hid_id in (("hidraw0", "0003:0000046D:0000C52B"), ("hidraw3", "0003:000017CC:00001500")):
                os.makedirs(f"{root}/{name}/device")
                with open(f"{root}/{name}/device/uevent", "w") as f:
                    f.write(f"DRIVER=hid-generic\nHID_ID={hid_id}\nHID_NAME=x\n")
            self.assertTrue(nihost.hid_present(0x17CC, 0x1500, root))
            self.assertFalse(nihost.hid_present(0x17CC, 0x1140, root))

    def test_split_messages(self):
        data = bytes([3, 0, 0x90, 1, 2, 2, 0, 0xF0, 0xF7])
        self.assertEqual(nihost.split_messages(data), [b"\x90\x01\x02", b"\xF0\xF7"])


def built_library():
    path = os.environ.get("DBM_NIHOST_LIBRARY")
    if path and os.path.isfile(path):
        return path
    for path in nihost.REPOSITORY_LIBRARIES:
        if os.path.isfile(path):
            return path
    return None


@unittest.skipIf(built_library() is None, "libdbm_nihost not built (DBM_NIHOST_LIBRARY, build-nihost)")
class NihostLibraryTest(unittest.TestCase):
    """The real library's test devices through NihostDevice (the ctypes signatures)."""

    def setUp(self):
        os.environ.setdefault("DBM_NIHOST_LIBRARY", built_library())
        self.lib = nihost.load_library()
        self.lib.dbm_nihost_open_test.restype = ctypes.c_void_p
        self.lib.dbm_nihost_open_test.argtypes = [ctypes.c_int]
        self.lib.dbm_nihost_test_attach.argtypes = [ctypes.c_void_p, ctypes.c_int]
        self.lib.dbm_nihost_test_report.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_size_t]

    def open_test(self, kind):
        # A NihostDevice on a test device of the library (no hidraw)
        lib = self.lib

        class TestLib:
            def __getattr__(self, name):
                return getattr(lib, name)

            def dbm_nihost_open(self, kind):
                return lib.dbm_nihost_open_test(kind)

        return nihost.NihostDevice(kind, TestLib())

    def test_mk2(self):
        device = self.open_test(nihost.KIND_MK2)
        self.assertFalse(device.is_attached())
        device.send(NIHOST_QUERY)
        self.assertEqual(device.read(0), [], "No answer without the device")
        self.lib.dbm_nihost_test_attach(device.handle, 1)
        self.assertTrue(device.is_attached())
        self.assertEqual(device.read(100), [MK2_HELLO])
        device.send(NIHOST_QUERY)
        self.assertEqual(device.read(100), [MK2_HELLO])
        report = bytes([0x01] + [0] * 24)
        self.lib.dbm_nihost_test_report(device.handle, report, len(report))
        device.read(0)
        pressed = bytearray(report)
        pressed[9] = 11  # encoder 1: one step
        self.lib.dbm_nihost_test_report(device.handle, bytes(pressed), len(pressed))
        self.assertEqual(device.read(100), [b"\xB0\x46\x01"])
        device.flush()
        device.send(b"\xB0\x39\x7F")  # PLAY
        self.assertGreater(device.flush(), 0)
        self.assertEqual(device.flush(), 0)
        self.lib.dbm_nihost_test_attach(device.handle, 0)
        self.assertEqual(device.read(100), [NIHOST_GONE])
        device.close()
        self.assertEqual(device.read(0), [])

    def test_jam(self):
        device = self.open_test(nihost.KIND_JAM)
        self.lib.dbm_nihost_test_attach(device.handle, 1)
        self.assertEqual(device.read(100), [JAM_HELLO])
        device.close()


if __name__ == "__main__":
    unittest.main()
