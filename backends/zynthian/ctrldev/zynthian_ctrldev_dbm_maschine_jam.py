#!/usr/bin/python3
# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# Zynthian Control Device Driver for "Native Instruments Maschine JAM", run by DrivenByMoss
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
# Native Instruments Maschine JAM (DrivenByMoss), in-process or through dbm-nihost
# ------------------------------------------------------------------------------------------------------------------


class zynthian_ctrldev_dbm_maschine_jam(zynthian_ctrldev_base_dbm):

    # The JAM has no MIDI driver on Linux: dbm-nihost (support/linux/ni_host of DrivenByMoss)
    # reads it over hidraw and creates the ALSA sequencer client "Maschine Jam - 1" with NI's port
    # names "Maschine Jam - 1 Input" / "Maschine Jam - 1 Output" (the core's Linux port names);
    # PipeWire shows them as "Midi-Bridge:Maschine Jam - 1: Input (capture)" / "Output (playback)",
    # the device ID is "Maschine Jam - 1 IN 1". Not the Maschine Mk2's ("Maschine MK2 IN 1").
    # dbm-nihost bridges one JAM ("- 1").
    dev_ids = ["Maschine Jam - 1 IN 1"]
    dbm_controller_id = "72F32A83-A697-446B-9016-64AA3F9476E4"
    driver_name = "DrivenByMoss Maschine JAM"
    driver_description = ("Native Instruments Maschine JAM (in-process or through dbm-nihost), run by DrivenByMoss: the clip launcher "
                          "on zynseq, the touch strips on the mixer and the parameters, the pads play the active chain")
    autoload_flag = True  # zynthian has no other driver for this device
    # In-process (zynthian's ctrldev manager with HID drivers): the driver drives the JAM itself
    # over hidraw with libdbm_nihost, the same host as dbm-nihost's, without its ports; dbm-nihost
    # must not run then (only one process can drive the device)
    hid_ids = [(0x17CC, 0x1500)]
    nihost_kind = nihost.KIND_JAM
    # dbm-nihost creates the ports also without the device: the controller runs only while the JAM
    # is there (its hello: NI's "return from host")
    nihost_hello = bytes.fromhex("F000210915004D5000014601F7")

# ------------------------------------------------------------------------------------------------------------------
