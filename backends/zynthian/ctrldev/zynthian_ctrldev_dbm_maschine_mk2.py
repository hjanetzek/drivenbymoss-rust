#!/usr/bin/python3
# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# Zynthian Control Device Driver for "Native Instruments Maschine Mk2", run by DrivenByMoss
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

from zyngine.ctrldev.zynthian_ctrldev_base_dbm import zynthian_ctrldev_base_dbm
from zyngine.ctrldev.dbm_zynthian import nihost

# ------------------------------------------------------------------------------------------------------------------
# Native Instruments Maschine Mk2 (DrivenByMoss), in-process or through dbm-nihost
# ------------------------------------------------------------------------------------------------------------------


class zynthian_ctrldev_dbm_maschine_mk2(zynthian_ctrldev_base_dbm):

    # The Mk2 has no MIDI driver on Linux: dbm-nihost (support/linux/ni_host of DrivenByMoss)
    # reads it over hidraw and creates the ALSA sequencer client "Maschine MK2" with NI's port
    # names "Maschine MK2 In" / "Maschine MK2 Out" (the core's Linux port names); PipeWire shows
    # them as "Midi-Bridge:Maschine MK2: In (capture)" / "Out (playback)", the device ID is
    # "Maschine MK2 IN 1". Not the device's own USB MIDI port ("Maschine Controller MK2 MIDI 1",
    # its MIDI mode without displays), nor the Maschine JAM's ("Maschine Jam - 1 IN 1").
    dev_ids = ["Maschine MK2 IN 1"]
    dbm_controller_id = "015E4908-C049-4808-A645-A7A44BEFE36B"
    driver_name = "DrivenByMoss Maschine Mk2"
    driver_description = ("Native Instruments Maschine Mk2 (in-process or through dbm-nihost), run by DrivenByMoss: the displays show "
                          "the chains, devices and parameters; the pads play the active chain")
    autoload_flag = True  # zynthian has no other driver for this device
    # In-process (zynthian's ctrldev manager with HID drivers): the driver drives the Mk2 itself
    # over hidraw with libdbm_nihost, the same host as dbm-nihost's, without its ports; dbm-nihost
    # must not run then (only one process can drive the device)
    hid_ids = [(0x17CC, 0x1140)]
    nihost_kind = nihost.KIND_MK2
    # dbm-nihost creates the ports also without the device: the controller runs only while the Mk2
    # is there (its hello, which also says that dbm-nihost sends the Shift button)
    nihost_hello = bytes.fromhex("F07D44424D1101F7")
    # The "+" group button enters the browse mode on zynthian's Add Chain view (the core's browse
    # mode follows the browser opened by the host)
    browse_add_chain = True

# ------------------------------------------------------------------------------------------------------------------
