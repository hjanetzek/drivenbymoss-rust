#!/usr/bin/python3
# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# Zynthian Control Device Driver for "Novation Launchpad Mini MkIII", run by DrivenByMoss
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
# Novation Launchpad Mini MkIII (DrivenByMoss)
# ------------------------------------------------------------------------------------------------------------------


class zynthian_ctrldev_dbm_launchpad_mini_mk3(zynthian_ctrldev_base_dbm):

    # The card "Launchpad Mini MK3" has two MIDI ports: 0 "LPMiniMK3 DAW" (zynthian's driver, IN 1),
    # 1 "LPMiniMK3 MIDI", which the core uses ("Launchpad Mini MK3 MIDI 2", "… LPMiniMK3 MI"): IN 2.
    dev_ids = ["Launchpad Mini MK3 IN 2"]
    dbm_controller_id = "A17B269D-2641-452F-B5A2-81BBACDA0D17"
    driver_name = "DrivenByMoss Launchpad Mini MkIII"
    driver_description = ("Novation Launchpad Mini MkIII, run by DrivenByMoss: session (zynseq's launcher), note, drum and "
                          "sequencer views on the active chain and its pattern, mixer and device views")
    # zynthian's own driver (zynthian_ctrldev_launchpad_mini_mk3) runs the device from its DAW port: it autoloads, this one
    # is chosen in the device's MIDI options and then unloads it (doc/ZYNTHIAN_PLAN.md §7b)
    replaces_dev_ids = ["Launchpad Mini MK3 IN 1"]
    autoload_flag = False

# ------------------------------------------------------------------------------------------------------------------
