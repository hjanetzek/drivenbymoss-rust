# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Ableton Push 1/2/3 simulator (PySide6 + QML) for testing the DrivenByMoss Push controller."""

import os as _os
import sys as _sys

# The shared code of the simulators (sim_common) lives in support/controller_simulator
_SHARED = _os.path.join(_os.path.dirname(_os.path.abspath(__file__)), "..", "..", "controller_simulator")
if _os.path.isdir(_SHARED) and _os.path.abspath(_SHARED) not in map(_os.path.abspath, _sys.path):
    _sys.path.insert(0, _os.path.abspath(_SHARED))
