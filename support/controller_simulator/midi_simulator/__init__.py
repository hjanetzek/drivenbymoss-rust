# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Simulators of plain MIDI controllers (PySide6 + QML): Akai APC40 mkII, Novation Launchpad Pro Mk3,
Novation Launch Control XL."""

from .apc40mk2 import Apc40Mk2
from .lcxl import LaunchControlXl
from .lppromk3 import LaunchpadProMk3

DEVICES = {"apc40mk2": Apc40Mk2, "lppromk3": LaunchpadProMk3, "lcxl": LaunchControlXl}
