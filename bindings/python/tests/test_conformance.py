# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Tests of the in-process conformance runner (bindings/python/conformance): the OSC command
table, the state view, the reference results, and the Launch Control XL Track Mix file end to
end against the Java reference.

Run from the repository root (after 'cargo build')::

    python3 -m unittest discover -s bindings/python/tests
"""

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import conformance  # noqa: E402,F401  (sets the path of backends/bitwig/tools)
from conformance import osc, reference  # noqa: E402
from conformance.fixture import SETUPS, build_fixture  # noqa: E402
from conformance.runner import run_file  # noqa: E402
from conformance.state import OscController, is_modelled, number, value127  # noqa: E402
from dbm_host import TAKEOVER_CATCH, Preferences  # noqa: E402


class Observer:
    """The fixture with the observer and a clock."""

    def __init__(self, **preferences):
        self.daw = build_fixture(Preferences(**preferences))
        self.osc = OscController(self.daw, clock=lambda: self.daw.time_ms)

    def run(self, *commands: str) -> None:
        for command in commands:
            osc.execute(self.osc, command)

    def advance(self, ms: float) -> None:
        self.daw.advance(ms)
        self.osc.run_tasks()

    def state(self, address: str) -> str:
        return self.osc.snapshot()["osc:" + address]


class OscTableTest(unittest.TestCase):

    def test_typed_values(self):
        self.assertEqual(osc.parse("/track/1/select 1"), ("/track/1/select", 1))
        self.assertEqual(osc.parse("/tempo/raw 120.5"), ("/tempo/raw", 120.5))
        self.assertEqual(osc.parse("/device/page/1/selected"), ("/device/page/1/selected", None))

    def test_track_commands(self):
        o = Observer(auto_arm=True)
        daw = o.daw
        o.run("/track/2/mute 1", "/track/3/solo 1", "/track/4/recarm 1")
        self.assertTrue(daw.tracks[1].mute and daw.tracks[2].solo and daw.tracks[3].arm)
        o.run("/track/2/mute", "/track/3/solo 0")
        self.assertFalse(daw.tracks[1].mute or daw.tracks[2].solo, "Without a value: toggle")
        self.assertTrue(daw.application.can_undo, "Mute and solo are edits")
        # TrackImpl.setMonitor ignores the value
        o.run("/track/5/monitor 0")
        self.assertEqual(daw.tracks[4].monitor_mode, "ON")
        o.run("/track/5/autoMonitor 1")
        self.assertEqual(daw.tracks[4].monitor_mode, "AUTO")
        o.run("/track/4/select 1")
        self.assertIs(daw.selected_channel, daw.tracks[3])
        o.run("/track/1/volume 127", "/track/1/pan 0")
        self.assertEqual(daw.tracks[0].volume.value, 1.0)
        self.assertEqual(daw.tracks[0].pan.value, 0.0)
        o.run("/track/2/clip/3/select")
        self.assertIs(daw.selected_slot, daw.tracks[1].slots[2])

    def test_takeover_catch_applies_to_osc(self):
        o = Observer(takeover_mode=TAKEOVER_CATCH)
        before = o.daw.tracks[0].volume.value
        o.run("/track/1/volume 127")
        self.assertEqual(o.daw.tracks[0].volume.value, before, "A jump is caught (RangedValueImpl.setValue)")

    def test_transport_and_device(self):
        o = Observer()
        t = o.daw.transport
        o.run("/click 1", "/repeat", "/tempo/raw 120", "/play 1")
        self.assertTrue(t.metronome and t.loop and t.playing)
        self.assertEqual(t.tempo, 120)
        o.run("/stop", "/stop")
        self.assertFalse(t.playing)
        o.run("/device/page/3/selected", "/primary/page/2")
        self.assertEqual(o.daw.cursor_device.selected_page, 1, "Cursor device and first instrument are the same Polysynth")
        o.run("/device/page/select 4")
        self.assertEqual(o.state("/device/page/selected/name"), "FILTER")

    def test_bank_page_selects_the_first_track_later(self):
        o = Observer()
        o.run("/track/bank/page/-")
        self.assertEqual(o.osc.track_offset, 0, "No previous page")
        o.run("/track/bank/page/+")
        self.assertEqual(o.osc.track_offset, 8)
        self.assertIs(o.daw.selected_channel, o.daw.tracks[0])
        o.advance(75)
        self.assertIs(o.daw.selected_channel, o.daw.tracks[8], "75 ms later (AbstractItemBankImpl.selectNextPage)")
        self.assertEqual(o.state("/track/1/name"), "Audio 9")

    def test_unknown(self):
        with self.assertRaises(osc.UnknownOscCommand):
            Observer().run("/browser/tag/1")


class StateViewTest(unittest.TestCase):

    def test_formats(self):
        self.assertEqual(number(110), "110.0")
        self.assertEqual(number(120.1), "120.1")
        self.assertEqual(value127(0.5), "64")
        self.assertEqual(value127(0.540742), "69")
        o = Observer()
        state = o.osc.snapshot()
        self.assertEqual(state["osc:/track/1/volume"], "69")
        self.assertEqual(state["osc:/track/1/volumeStr"], "-10.0 dB")
        self.assertEqual(state["osc:/track/1/color"], "rgb(87,97,198)")
        self.assertEqual(state["osc:/track/1/type"], "instrument")
        self.assertEqual(state["osc:/track/selected/name"], "Synth")
        self.assertEqual(state["osc:/master/volume"], "101")
        self.assertEqual(state["osc:/tempo/raw"], "110.0")
        self.assertEqual(state["osc:/primary/page/2/exists"], "1")
        self.assertEqual(state["osc:/device/page/1/"], "OSC1")
        self.assertTrue(all(is_modelled(key) for key in state))

    def test_is_modelled(self):
        for key in ("osc:/track/3/selected", "osc:/track/selected/recarm", "osc:/device/page/2/selected", "osc:/device/page/1/", "osc:/click", "osc:/track/1/send/8/volumeStr",
                    "osc:/eq/type/8/value", "osc:/eq/freq/1/valueStr", "osc:/eq/parameters", "osc:/device/param/1/value", "osc:/clip/exists",
                    "osc:/clip/step/0/12/41", "osc:/device/layer/2/selected", "osc:/device/layer/selected/volumeStr",
                    "osc:/conformance/primary/drumpad/16/mute", "osc:/track/1/playingNotes", "osc:/marker/2/name", "osc:/layout"):
            self.assertTrue(is_modelled(key), key)
        for key in ("osc:/track/selected/selected", "osc:/device/lastparam/value", "osc:/track/1/vu", "osc:/clip/pinned", "osc:/device/drumpad/1/name",
                    "osc:/device/layer/1/send/1/name", "probe:/track/1/name", "osc:/track/0/name"):
            self.assertFalse(is_modelled(key), key)

    def test_the_bank_follows_the_cursor_track(self):
        o = Observer(auto_arm=True)
        o.daw.tracks[9].select()
        self.assertEqual(o.state("/track/2/name"), "Audio 10")
        self.assertEqual(o.state("/track/3/exists"), "0")
        self.assertEqual(o.state("/track/3/type"), "unknown")
        self.assertEqual(o.state("/track/3/position"), "10")
        self.assertEqual(o.state("/track/1/recarm"), "0", "An audio track is not auto-armed")
        self.assertEqual(o.state("/device/exists"), "0")
        self.assertEqual(o.state("/device/bypass"), "1")
        o.daw.tracks[2].select()
        self.assertEqual(o.state("/track/1/name"), "Synth")
        self.assertEqual(o.state("/track/3/recarm"), "1", "An instrument track is auto-armed")

    def test_the_eq_of_the_fixture(self):
        """The EQ+ of track EQ as the observer of the Java runs reported it."""
        o = Observer()
        self.assertEqual((o.state("/eq/exists"), o.state("/eq/bypass"), o.state("/eq/type/1/value"), o.state("/eq/gain/1/valueStr")), ("0", "1", "off", ""))
        o.run("/track/4/select 1")
        self.assertEqual((o.state("/eq/exists"), o.state("/eq/name"), o.state("/eq/expand"), o.state("/eq/parameters")), ("1", "EQ+", "1", "0"))
        self.assertEqual([o.state(f"/eq/freq/{n}/value") for n in range(1, 9)], ["25", "38", "51", "64", "76", "89", "102", "114"])
        self.assertEqual((o.state("/eq/freq/5/valueStr"), o.state("/eq/gain/2/valueStr"), o.state("/eq/q/3/valueStr")), ("1.28 kHz", "0.0 dB", "1.00"))
        self.assertEqual(o.state("/eq/q/8/name"), "Band 8 Q")


class ReferenceTest(unittest.TestCase):

    def test_reduce_and_expand(self):
        def step(bitwig, leds):
            return {"input": {}, "bitwig": bitwig, "leds": leds}
        trace = {"controller": "X", "label": "java", "scenarios": [
            {"id": "a", "initial": {"leds": {"90 01": "01"}}, "start": {"osc:/click": "0", "osc:/device/lastparam/value": "3"},
             "steps": [step({"osc:/click": "1", "osc:/device/lastparam/value": "4", "osc:/track/1/vu": "9"}, {"90 01": "02"})]},
            {"id": "b", "initial": {"leds": {"90 01": "02", "90 02": "7F"}}, "start": {"osc:/click": "0"},
             "steps": [step({}, {"90 01": "02", "90 02": "00"})]},
        ]}
        ref = reference.reduce_trace(trace)
        self.assertEqual(ref["leds"], {"90 01": "01"})
        first, second = ref["scenarios"]
        self.assertEqual(first["start"], {"osc:/click": "0"})
        self.assertEqual(first["steps"][0], {"leds": {"90 01": "02"}, "bitwig": {"osc:/click": "1"}, "not_modelled": ["osc:/device/lastparam/value"]})
        self.assertEqual(second["leds"], {"90 02": "7F"})
        self.assertEqual(second["start"], {"osc:/click": "0"}, "The change since the end of the previous scenario")
        self.assertEqual(reference.not_modelled(ref), {"a": ["osc:/device/lastparam/value"]})
        oracle = reference.expand(json.loads(reference.dumps(ref)))
        self.assertEqual(oracle["scenarios"][1]["start"], {"osc:/click": "0"})
        self.assertEqual(oracle["scenarios"][1]["steps"][0]["leds"], {"90 01": "02", "90 02": "00"})


class TrackMixTest(unittest.TestCase):
    """tests/conformance/launchcontrol_xl/track_mix.toml against the Java reference."""

    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory(prefix="dbm-conformance-test-")
        setup = SETUPS["launchcontrol_xl"]
        cls.file = setup.scenario_dir / "track_mix.toml"
        cls.result = run_file(setup, cls.file, keep=Path(cls.temp.name))

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def test_same_as_java(self):
        r = self.result
        self.assertTrue(r.has_reference)
        self.assertTrue(r.ok, "\n".join(r.output))
        self.assertEqual((r.same, r.different, r.failed_expectations), (r.scenarios, 0, 0))
        self.assertEqual((r.unhandled, r.unknown_osc, r.core_errors), ([], [], []))

    def test_the_start_is_the_state_of_the_java_run(self):
        """The fixture and the state view give the same keys and values as the observer in the
        Java run, before the first scenario."""
        r = self.result
        java = reference.load(reference.path_for(self.file, r.run))["scenarios"][0]["start"]
        sim = json.loads((Path(self.temp.name) / f"{r.run}-sim.json").read_text())["scenarios"][0]["start"]
        # The observer reports a step which became empty once as "0" and the trace keeps it: the
        # Java run starts with the empty steps of earlier runs
        java = {k: v for k, v in java.items() if not (k.startswith("osc:/clip/step/") and v == "0")}
        self.assertEqual(set(sim), set(java))
        self.assertEqual({k: v for k, v in sim.items() if java[k] != v}, {})


if __name__ == "__main__":
    unittest.main()
