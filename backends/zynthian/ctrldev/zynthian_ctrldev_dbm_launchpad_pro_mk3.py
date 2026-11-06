#!/usr/bin/python3
# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# Zynthian Control Device Driver for "Novation Launchpad Pro Mk3", run by DrivenByMoss
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
# Novation Launchpad Pro Mk3 (DrivenByMoss)
# ------------------------------------------------------------------------------------------------------------------


class zynthian_ctrldev_dbm_launchpad_pro_mk3(zynthian_ctrldev_base_dbm):

    # The card "Launchpad Pro MK3" has three MIDI ports: 0 "LPProMK3 MIDI", which the core uses
    # ("Launchpad Pro MK3 MIDI 1", "… LPProMK3 MIDI"): IN 1; 1 "LPProMK3 DIN"; 2 "LPProMK3 DAW"
    # (zynthian's driver, IN 3).
    dev_ids = ["Launchpad Pro MK3 IN 1"]
    dbm_controller_id = "4EDED44C-7817-4C66-A334-66A9E342AAA0"
    driver_name = "DrivenByMoss Launchpad Pro Mk3"
    driver_description = ("Novation Launchpad Pro Mk3, run by DrivenByMoss: session (zynseq's launcher), note, drum and "
                          "sequencer views on the active chain and its pattern, mixer and device views")
    # zynthian's own driver (zynthian_ctrldev_launchpad_pro_mk3) runs the device from its DAW port: it autoloads, this one
    # is chosen in the device's MIDI options and then unloads it (doc/ZYNTHIAN_PLAN.md §7b)
    replaces_dev_ids = ["Launchpad Pro MK3 IN 3"]
    autoload_flag = False

# ------------------------------------------------------------------------------------------------------------------
