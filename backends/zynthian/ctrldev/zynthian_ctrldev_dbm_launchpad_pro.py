#!/usr/bin/python3
# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# Zynthian Control Device Driver for "Novation Launchpad Pro", run by DrivenByMoss
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
# Novation Launchpad Pro (DrivenByMoss)
# ------------------------------------------------------------------------------------------------------------------


class zynthian_ctrldev_dbm_launchpad_pro(zynthian_ctrldev_base_dbm):

    # The first Launchpad Pro (2015, zynthian's "Launchpad Pro MK2"): the card "Launchpad Pro" has
    # three MIDI ports: 0 the Live port (zynthian's driver, IN 1), 1 the Standalone port, which the
    # core uses ("Launchpad Pro MIDI 2", "Launchpad Pro Standalone Port"): IN 2; 2 the MIDI (DIN) port.
    dev_ids = ["Launchpad Pro IN 2"]
    dbm_controller_id = "80B63970-64F1-11E5-A837-0800200C9A66"
    driver_name = "DrivenByMoss Launchpad Pro"
    driver_description = ("Novation Launchpad Pro, run by DrivenByMoss: session (zynseq's launcher), note, drum and "
                          "sequencer views on the active chain and its pattern, mixer and device views")
    # zynthian's own driver (zynthian_ctrldev_launchpad_pro_mk2) runs the device from its DAW port: it autoloads, this one
    # is chosen in the device's MIDI options and then unloads it (doc/ZYNTHIAN_PLAN.md §7b)
    replaces_dev_ids = ["Launchpad Pro IN 1"]
    autoload_flag = False

# ------------------------------------------------------------------------------------------------------------------
