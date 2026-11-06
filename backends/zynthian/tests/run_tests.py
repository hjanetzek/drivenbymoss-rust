# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: runs the tests without the UI
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

"""Runs the tests of the zynthian drivers with the stand-ins of zynstubs (the real zyngine
package would import all engines). From the repository root, after ``cargo build``::

    python3 backends/zynthian/tests/run_tests.py [-v]

They need a zynthian-ui checkout of the ``dbm`` branch (``$ZYNTHIAN_UI_DIR``, else
``../zynthian-dbm`` or ``../zynthian-ui``) and exit with 77 ("skipped") without one.
"""

import sys
import unittest
from pathlib import Path

import zynstubs

if __name__ == "__main__":
    if not zynstubs.available():
        print(f"No zynthian-ui checkout at {zynstubs.UI_DIR} (set ZYNTHIAN_UI_DIR): the tests are skipped")
        sys.exit(77)
    zynstubs.install()
    here = str(Path(__file__).resolve().parent)
    tests = unittest.defaultTestLoader.discover(here, top_level_dir=here)
    verbosity = 2 if "-v" in sys.argv else 1
    result = unittest.TextTestRunner(verbosity=verbosity).run(tests)
    sys.exit(0 if result.wasSuccessful() else 1)
