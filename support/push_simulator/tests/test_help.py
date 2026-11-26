# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The help boxes of the Push simulator (doc/CONTROLLER_HELP_PLAN.md §13): the mapping of the
core's entries to the controls, the areas and sides, and the real core's entries of the Push 3 in
the states every button makes (each goes to a control)."""

import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE.parents[1] / "controller_simulator"))

from push_simulator.device import PushDevice  # noqa: E402

REPO = HERE.parents[2]
LIBRARY = os.environ.get("DBM_LIBRARY") or str(REPO / "target" / "debug" / "libdrivenbymoss.so")
PUSH3 = "F8D561F2-722D-4DC7-975A-C562788C63FA"


def entry(entry_id, control=-1, bind="cc", **fields):
    return {"id": entry_id, "label": entry_id, "bind": bind, "channel": 0, "control": control, "pressed": False, "presses": 0,
            "short": "Text", "long": "", **fields}


class MappingTest(unittest.TestCase):

    def setUp(self):
        from PySide6.QtCore import QCoreApplication
        self.app = QCoreApplication.instance() or QCoreApplication([])
        from push_simulator.qt_bridge import DisplayImageProvider, Simulator
        self.device = PushDevice("push3")
        self.sim = Simulator(self.device, DisplayImageProvider())

    def test_bindings(self):
        play = self.sim._help_target(entry("Play", 85))
        self.assertEqual(self.device.controls[play.key].label, "Play")
        jog = self.sim._help_target(entry("PlayPosition", 70))
        self.assertEqual((self.device.controls[jog.key].kind, jog.label), ("encoder", "Jog wheel"))
        strip = self.sim._help_target(entry("Touchstrip", bind=None))
        self.assertEqual(self.device.controls[strip.key].kind, "strip")
        self.assertIsNone(self.sim._help_target(entry("Footswitch2", 999)))

    def test_rows_areas_and_sides(self):
        model = self.sim.help_model
        above = [{"id": f"Row2{i + 1}", "bind": "cc", "channel": 0, "control": 102 + i} for i in range(8)]
        scenes = [{"id": f"Scene{i + 1}", "bind": "cc", "channel": 0, "control": 43 - i} for i in range(8)]
        for item in (entry("grid", bind=None), entry("knobs", bind=None), entry("Row21..Row28", bind=None, members=above),
                     entry("Scene1..Scene8", bind=None, members=scenes), entry("Play", 85), entry("Shift", 49), entry("Device", 110)):
            model.apply(item)
        items = {item["id"]: item for item in model.items()}
        self.assertEqual({key: item["side"] for key, item in items.items()},
                         {"grid": "bottom", "knobs": "top", "Row21..Row28": "top", "Scene1..Scene8": "right", "Play": "left",
                          "Shift": "right", "Device": "top"})
        self.assertEqual(items["Row21..Row28"]["label"], "Above the display 1–8")
        self.assertEqual(items["Scene1..Scene8"]["label"], "Scene 1–8")


@unittest.skipUnless(os.path.exists(LIBRARY), "the core library is not built (cargo build)")
class CoreHelpTest(unittest.TestCase):
    """The real core's entries in the Push 3 simulator: every one goes to a control, also in the
    states which every button makes."""

    def test_every_entry_has_a_control(self):
        from PySide6.QtCore import QCoreApplication
        self.app = QCoreApplication.instance() or QCoreApplication([])
        sys.path.insert(0, str(REPO / "bindings" / "python"))
        os.environ.setdefault("DBM_LIBRARY", LIBRARY)
        from dbm_host import Bridge, SimDaw
        from push_simulator.qt_bridge import DisplayImageProvider, Simulator
        daw = SimDaw().build(tracks=[("Synth", "INSTRUMENT"), ("Drums", "INSTRUMENT")], effect_tracks=["FX 1"], scenes=8)
        daw.tracks[0].select()
        box = []
        device = PushDevice("push3", send=lambda data, port: box[0].receive(data))
        sim = Simulator(device, DisplayImageProvider())
        config = tempfile.TemporaryDirectory()
        self.addCleanup(config.cleanup)
        bridge = Bridge(daw, PUSH3, config.name, "Bitwig Studio 5.3")
        box.append(bridge)
        bridge.on_midi_out = lambda data: device.receive(bytes(data))
        bridge.start()
        self.addCleanup(bridge.close)
        bridge.wait_for_startup()
        bridge.advance(300)
        unmapped = set()

        def check(lines):
            for line in lines:
                item = json.loads(line)
                if item["short"] and not item["id"].startswith("@") and sim.help_model.target(item)[0] is None:
                    unmapped.add(item["id"])

        lines = bridge.app.take_control_help_lines()
        self.assertGreater(len(lines), 40)
        check(lines)
        for control in device.controls:
            if control.kind != "button":
                continue
            device.press_button(control.cc)
            bridge.advance(30)
            check(bridge.app.take_control_help_lines())
            device.release_button(control.cc)
            bridge.advance(30)
            check(bridge.app.take_control_help_lines())
        self.assertEqual(unmapped, set())
        self.assertEqual(bridge.core_errors(), [])


if __name__ == "__main__":
    unittest.main()
