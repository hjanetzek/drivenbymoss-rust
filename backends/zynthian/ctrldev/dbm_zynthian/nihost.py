# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: the NI host (dbm-nihost) in the driver's process
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

"""libdbm_nihost (support/linux/ni_host/include/dbm_nihost.h) through ctypes: the Maschine Mk2
and JAM over hidraw, with the MIDI of dbm-nihost's ports (template MIDI, hello, gone, query),
but in the driver's process instead of through dbm-nihost, ALSA, PipeWire's MIDI bridge and JACK.

The library is ``$DBM_NIHOST_LIBRARY``, else ``/usr/lib/drivenbymoss/libdbm_nihost.so`` (the
drivenbymoss package, next to the core), else ``/usr/lib/dbm-nihost/libdbm_nihost.so`` (dbm-nihost
0.2.0 shipped it), else a build in the repository (``build-nihost`` at the top or
``support/linux/ni_host/build``).

The sysfs check ``hid_present`` needs no library.
"""

import ctypes
import glob
import os
import threading
from pathlib import Path

KIND_MK2 = 1
KIND_JAM = 2

API_VERSION = 1

DEFAULT_LIBRARY = "/usr/lib/drivenbymoss/libdbm_nihost.so"
OLD_LIBRARY = "/usr/lib/dbm-nihost/libdbm_nihost.so"
# backends/zynthian/ctrldev/dbm_zynthian -> the repository (development)
_PARENTS = Path(__file__).resolve().parents
REPOSITORY_LIBRARIES = [] if len(_PARENTS) < 5 else [
    str(_PARENTS[4] / "build-nihost" / "libdbm_nihost.so"),
    str(_PARENTS[4] / "support" / "linux" / "ni_host" / "build" / "libdbm_nihost.so")]

# The read buffer: many messages per read (the longest is a sysex of a few dozen bytes)
READ_BUFFER_BYTES = 4096

_library = None
_library_lock = threading.Lock()


def library_path():
    path = os.environ.get("DBM_NIHOST_LIBRARY")
    if path:
        return path
    for candidate in [DEFAULT_LIBRARY, OLD_LIBRARY] + REPOSITORY_LIBRARIES:
        if os.path.isfile(candidate):
            return candidate
    return DEFAULT_LIBRARY


def load_library():
    """The library (loaded once); OSError if it isn't there or too old."""
    global _library
    with _library_lock:
        if _library is not None:
            return _library
        lib = ctypes.CDLL(library_path())
        lib.dbm_nihost_api_version.restype = ctypes.c_int
        lib.dbm_nihost_api_version.argtypes = []
        if lib.dbm_nihost_api_version() != API_VERSION:
            raise OSError(f"{library_path()}: API version {lib.dbm_nihost_api_version()}, expected {API_VERSION}")
        handle = ctypes.c_void_p
        buffer = ctypes.POINTER(ctypes.c_uint8)
        lib.dbm_nihost_open.restype = handle
        lib.dbm_nihost_open.argtypes = [ctypes.c_int]
        lib.dbm_nihost_close.restype = None
        lib.dbm_nihost_close.argtypes = [handle]
        lib.dbm_nihost_is_attached.restype = ctypes.c_int
        lib.dbm_nihost_is_attached.argtypes = [handle]
        lib.dbm_nihost_read.restype = ctypes.c_int
        lib.dbm_nihost_read.argtypes = [handle, buffer, ctypes.c_size_t, ctypes.c_int]
        lib.dbm_nihost_send.restype = ctypes.c_int
        lib.dbm_nihost_send.argtypes = [handle, ctypes.c_char_p, ctypes.c_size_t]
        lib.dbm_nihost_flush.restype = ctypes.c_int
        lib.dbm_nihost_flush.argtypes = [handle]
        _library = lib
        return lib


def hid_present(vendor_id, product_id, sysfs_root="/sys/class/hidraw"):
    """True if a hidraw device with the ids is there (HID_ID in its uevent, as the library finds
    it); cheap: a few small sysfs files."""
    for uevent in glob.glob(f"{sysfs_root}/hidraw*/device/uevent"):
        try:
            with open(uevent) as f:
                for line in f:
                    if line.startswith("HID_ID="):
                        _bus, vendor, product = line.strip()[7:].split(":")
                        if int(vendor, 16) == vendor_id and int(product, 16) == product_id:
                            return True
                        break
        except (OSError, ValueError):
            continue
    return False


def split_messages(data):
    """The messages of a read: each with 2 bytes length (little endian) before it."""
    messages = []
    i = 0
    while i + 2 <= len(data):
        length = data[i] | (data[i + 1] << 8)
        messages.append(bytes(data[i + 2:i + 2 + length]))
        i += 2 + length
    return messages


class NihostDevice:
    """A Maschine driven by the library: ``read`` (on a thread of its own, it blocks up to the
    timeout), ``send`` (the core's MIDI, one message), ``flush`` (the changed LEDs and displays).
    ``close`` last, after the reading thread has stopped."""

    def __init__(self, kind, lib=None):
        self.lib = lib if lib is not None else load_library()
        self.handle = self.lib.dbm_nihost_open(kind)
        if not self.handle:
            raise ValueError(f"libdbm_nihost: no device of kind {kind}")
        self.buffer = (ctypes.c_uint8 * READ_BUFFER_BYTES)()

    def read(self, timeout_ms):
        """The messages to the DAW which came within the timeout ([] if none)."""
        if not self.handle:
            return []
        length = self.lib.dbm_nihost_read(self.handle, self.buffer, READ_BUFFER_BYTES, int(timeout_ms))
        if length <= 0:
            return []
        return split_messages(bytes(self.buffer[:length]))

    def send(self, data):
        if self.handle:
            data = bytes(data)
            self.lib.dbm_nihost_send(self.handle, data, len(data))

    def flush(self):
        if self.handle:
            return self.lib.dbm_nihost_flush(self.handle)
        return 0

    def is_attached(self):
        return bool(self.handle) and self.lib.dbm_nihost_is_attached(self.handle) == 1

    def close(self):
        handle, self.handle = self.handle, None
        if handle:
            self.lib.dbm_nihost_close(handle)
