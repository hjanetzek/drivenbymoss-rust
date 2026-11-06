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
        result = conformance.run_file("launchcontrol_xl", "track_mix", phase=3)
        report = result.report(verbose=True)
        self.assertEqual(len(result.judged), 10, "All scenarios of the file", )
        self.assertTrue(result.ok, report)
        self.assertEqual(result.core_errors, [], report)
        # The OSC steps and commands of the scenarios not judged in phase 1
        self.assertEqual(result.unknown_osc, [], report)
        self.assertEqual(result.unhandled, [], report)

    def test_note_sequencer(self):
        """The first instrument's pages and the note sequencer on the zynseq pattern (phase 3)."""
        result = conformance.run_file("launchcontrol_xl", "note_sequencer", phase=3)
        report = result.report(verbose=True)
        self.assertEqual(len(result.judged), 5, report)
        self.assertTrue(result.ok, report)
        known = {s.id: s.known for s in result.judged}
        for scenario in ("lcxl.noteseq.device-page", "lcxl.noteseq.create-note", "lcxl.noteseq.delete-note"):
            self.assertEqual(known[scenario], [], f"{scenario}: the same as Java on Bitwig")
        self.assertEqual(result.core_errors, [], report)
        self.assertEqual(result.unhandled, [], report)

    def test_drum_sequencer(self):
        result = conformance.run_file("launchcontrol_xl", "drum_sequencer", phase=3)
        report = result.report(verbose=True)
        self.assertEqual(len(result.judged), 6, report)
        self.assertTrue(result.ok, report)
        self.assertEqual(result.core_errors, [], report)
        # The drum pads' mute: zynthian has no drum machine
        self.assertEqual(result.unhandled, ["primary pad/<n>/mute"], report)


@unittest.skipUnless(AVAILABLE and SETUPS["maschine_mk2"].profile_dir.is_dir(), "needs the profile of the maschine_mk2 setup")
class MaschineMk2ConformanceTest(unittest.TestCase):
    """The Maschine Mk2 scenarios of phase 2 (zyn_conformance.PHASES): the group buttons, the
    display modes (volume, panorama, send, parameters, tempo), the pad modes which play notes
    (keyboard, chords, drum), the clip and scene pads, the parameter selection."""

    def run_file(self, name, judged):
        result = conformance.run_file("maschine_mk2", name, phase=3)
        report = result.report(verbose=True)
        self.assertEqual(len(result.judged), judged, report)
        self.assertTrue(result.ok, report)
        self.assertEqual(result.core_errors, [], report)
        # Only note repeat, the automation and the loop are not handled (no such thing in zynthian)
        self.assertTrue(all(key.split(" ")[0] in ("noterepeat", "automation", "repeat") for key in result.unhandled), report)
        return result

    def test_global(self):
        self.run_file("global", 13)

    def test_pad_modes(self):
        result = self.run_file("pad_modes", 9)
        known = {s.id: s.known for s in result.judged}
        for scenario in ("mk2.keyboard.play", "mk2.chords.play", "mk2.variation.parameter",
                         "mk2.drum.step-on", "mk2.drum.mute-step", "mk2.drum.step-off"):
            self.assertEqual(known[scenario], [], f"{scenario}: the same as Java on Bitwig")


if __name__ == "__main__":
    unittest.main()
