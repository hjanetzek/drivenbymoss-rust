# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: the Launch Control XL conformance scenarios of phase 1
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

"""The Launch Control XL scenarios against the model adapter (zyn_conformance.py): all ten
scenarios of track_mix.toml (the mixer of phase 1; the device page, the transport functions and
the scenes of phase 2) and the device page scenario of note_sequencer.toml (the first instrument)
compare the same as Java on Bitwig, apart from the listed differences of zynthian's model
(auto-arm, takeover catch, always a selected phrase, no undo). Skipped without the Bitwig profile
of the setup."""

import unittest

import zynstubs

zynstubs.install()

try:
    import zyn_conformance as conformance
    from conformance.fixture import SETUPS
    AVAILABLE = SETUPS["launchcontrol_xl"].profile_dir.is_dir()
except ImportError:
    AVAILABLE = False


@unittest.skipUnless(AVAILABLE, "needs the profile of the launchcontrol_xl setup (DBM_PROFILE_DIR)")
class TrackMixConformanceTest(unittest.TestCase):

    def test_track_mix(self):
        result = conformance.run_file("launchcontrol_xl", "track_mix", phase=2)
        report = result.report(verbose=True)
        self.assertEqual(len(result.judged), 10, "All scenarios of the file", )
        self.assertTrue(result.ok, report)
        self.assertEqual(result.core_errors, [], report)
        # The OSC steps and commands of the scenarios not judged in phase 1
        self.assertEqual(result.unknown_osc, [], report)
        self.assertEqual(result.unhandled, [], report)

    def test_note_sequencer_device_page(self):
        result = conformance.run_file("launchcontrol_xl", "note_sequencer", phase=2)
        report = result.report(verbose=True)
        self.assertEqual([s.id for s in result.judged], ["lcxl.noteseq.device-page"], report)
        self.assertTrue(result.ok, report)
        self.assertFalse(result.judged[0].known, "No known differences")


if __name__ == "__main__":
    unittest.main()
