#!/usr/bin/python3
# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# Zynthian Control Device Driver for "Ableton Push 1", run by DrivenByMoss
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
# Ableton Push 1 (DrivenByMoss)
# ------------------------------------------------------------------------------------------------------------------


class zynthian_ctrldev_dbm_push1(zynthian_ctrldev_base_dbm):

    # The ALSA card "Ableton Push" has two MIDI ports: 0 the Live Port ("Ableton Push Live Port"),
    # 1 the User Port ("Ableton Push User Port"), which the core uses (as the Java version). The
    # Push 1 has no USB display (its display is sysex).
    # The dev_ids: the port's jack name form (as the Launchkey MK4's "DAW In") and the alias form
    # "<card> IN <port>"
    dev_ids = ["Ableton Push User Port", "Ableton Push IN 2"]
    dbm_controller_id = "DBED9610-C474-11E6-9598-0800200C9A66"  # Push1ControllerInstance (dbm_create_controller takes the UUID)
    driver_name = "DrivenByMoss Ableton Push 1"
    driver_description = ("Ableton Push 1, run by DrivenByMoss: the pads play the active chain, the display and the knobs edit the chains, devices and parameters")
    autoload_flag = True  # zynthian has no other driver for this device

# ------------------------------------------------------------------------------------------------------------------
