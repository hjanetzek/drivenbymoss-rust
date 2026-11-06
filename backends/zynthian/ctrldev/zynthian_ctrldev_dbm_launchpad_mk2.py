#!/usr/bin/python3
# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# Zynthian Control Device Driver for "Novation Launchpad MkII", run by DrivenByMoss
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
# Novation Launchpad MkII (DrivenByMoss)
# ------------------------------------------------------------------------------------------------------------------


class zynthian_ctrldev_dbm_launchpad_mk2(zynthian_ctrldev_base_dbm):

    # The card "Launchpad MK2" has one MIDI port ("Launchpad MK2 MIDI 1"): IN 1.
    dev_ids = ["Launchpad MK2 IN 1"]
    dbm_controller_id = "4E01A0B0-67B1-11E5-A837-0800200C9A66"
    driver_name = "DrivenByMoss Launchpad MkII"
    driver_description = ("Novation Launchpad MkII, run by DrivenByMoss: session (zynseq's launcher), note, drum and "
                          "sequencer views on the active chain and its pattern, mixer and device views")
    autoload_flag = True  # zynthian has no other driver for this device

# ------------------------------------------------------------------------------------------------------------------
