#!/usr/bin/python3
# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# Zynthian Control Device Driver for "Ableton Push 2", run by DrivenByMoss
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

# ------------------------------------------------------------------------------------------------------------------
# Ableton Push 2 (DrivenByMoss)
# ------------------------------------------------------------------------------------------------------------------


class zynthian_ctrldev_dbm_push2(zynthian_ctrldev_base_dbm):

    # The ALSA card "Ableton Push 2" has two MIDI ports: 0 the Live Port ("Ableton Push 2 Live
    # Port", the core's), 1 the User Port. The pads and buttons are class compliant USB MIDI; the
    # display is USB interface 0, bulk endpoint 0x01 (2982:1967): the core's frames go there
    # through usbdevfs (drivenbymoss/usb.py, Bridge.usb_bulk_out), which needs write access to
    # /dev/bus/usb (zynthian runs as root; else the udev rule 70-dbm-nihost.rules).
    # The dev_ids: the port's jack name form (as the Launchkey MK4's "DAW In") and the alias form
    # "<card> IN <port>"
    dev_ids = ["Ableton Push 2 Live Port", "Ableton Push 2 IN 1"]
    dbm_controller_id = "15176AA0-C476-11E6-9598-0800200C9A66"  # Push2ControllerInstance (dbm_create_controller takes the UUID)
    driver_name = "DrivenByMoss Ableton Push 2"
    driver_description = ("Ableton Push 2, run by DrivenByMoss: the pads play the active chain, the display (over USB) and the knobs edit the chains, devices and parameters")
    autoload_flag = True  # zynthian has no other driver for this device

# ------------------------------------------------------------------------------------------------------------------
