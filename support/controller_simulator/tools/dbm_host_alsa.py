#!/usr/bin/env python3
# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
"""The reference host on ALSA for all simulators: runs support/push_simulator/tools/dbm_host_alsa.py
(``--model push1|push2|push3|mk2|jam``, see its --help)."""

import os
import runpy
import sys

TOOL = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "push_simulator", "tools", "dbm_host_alsa.py")
sys.argv[0] = TOOL
runpy.run_path(TOOL, run_name="__main__")
