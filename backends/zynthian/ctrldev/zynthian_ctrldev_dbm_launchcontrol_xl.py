#!/usr/bin/python3
# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# Zynthian Control Device Driver for "Novation Launch Control XL", run by DrivenByMoss
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
# Novation Launch Control XL (DrivenByMoss)
# ------------------------------------------------------------------------------------------------------------------


class zynthian_ctrldev_dbm_launchcontrol_xl(zynthian_ctrldev_base_dbm):

    # The ALSA card "Launch Control XL" has two MIDI ports: 0 the controls ("Launch Control XL
    # Launch Contro", the core's Linux port name; PipeWire: "Launch Control XL: Launch Contro"),
    # 1 the HUI port. The driver takes the first one (IN 1), never HUI (IN 2).
    dev_ids = ["Launch Control XL IN 1"]
    dbm_controller_id = "5AFA5045-FF92-4737-A091-5B371CD5E529"
    driver_name = "DrivenByMoss Launch Control XL"
    driver_description = ("Novation Launch Control XL, run by DrivenByMoss: the factory templates mix the chains "
                          "(faders, pan and send knobs, mute, solo, arm and select buttons, track paging)")
    autoload_flag = True  # zynthian has no other driver for this device

# ------------------------------------------------------------------------------------------------------------------
