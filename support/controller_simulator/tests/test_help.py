# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The help of the controls (doc/CONTROLLER_HELP_PLAN.md): the box layout, the model, the socket,
the Maschine JAM's mapping of the core's entries, and the host's writer."""

import json
import os
import socket
import sys
import tempfile
import time
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

from maschine_simulator.device import MaschineDevice  # noqa: E402
from sim_common import help as sim_help  # noqa: E402


def entry(entry_id, short="Text", **fields):
    return {"id": entry_id, "label": entry_id.upper(), "bind": "cc", "channel": 0, "control": 90, "pressed": False, "presses": 0,
            "short": short, "long": "", **fields}


class SpreadTest(unittest.TestCase):

    def check(self, positions, size, gap):
        for a, b in zip(positions, positions[1:]):
            self.assertGreaterEqual(b - a, size + gap - 1e-9)

    def test_free_boxes_stay(self):
        self.assertEqual(sim_help.spread([10, 100, 200], 30, 5, 0, 1000), [10, 100, 200])

    def test_crowded_boxes_move_both_ways(self):
        # Three boxes wished at the same place: the middle one stays, the others move evenly
        positions = sim_help.spread([100, 100, 100], 30, 10, 0, 1000)
        self.assertEqual(positions, [60, 100, 140])
        self.check(positions, 30, 10)

    def test_bounds(self):
        positions = sim_help.spread([0, 0, 990], 30, 10, 20, 1000)
        self.assertEqual(positions[0], 20)
        self.assertLessEqual(positions[-1] + 30, 1000)
        self.check(positions, 30, 10)
        # Too many to fit: from the start on
        self.assertEqual(sim_help.spread([5, 5, 5], 50, 10, 0, 100), [0, 60, 120])


class ModelTest(unittest.TestCase):

    def model(self):
        targets = {"Play": sim_help.HelpTarget(1, 0, 100, 20, 10), "Song": sim_help.HelpTarget(2, 0, 0, 20, 10, side="top"),
                   "grid": sim_help.HelpTarget(-1, 40, 20, 100, 100, side="top")}
        return sim_help.HelpModel(lambda e: targets.get(e["id"]), (200, 200), (40, 20, 140, 120))

    def test_entries_and_sides(self):
        model = self.model()
        model.apply_line(json.dumps(entry("Play")))
        model.apply_line(json.dumps(entry("Song")))
        model.apply_line(json.dumps(entry("grid")))
        model.apply_line(json.dumps(entry("Footswitch1")))
        self.assertFalse(model.apply_line("not json"))
        items = {item["id"]: item for item in model.items()}
        self.assertEqual(set(items), {"Play", "Song", "grid"}, "an entry without a control has no box")
        self.assertEqual(items["Play"]["side"], "left")
        self.assertEqual(items["Song"]["side"], "top")
        # An area: the line goes to the edge facing the box
        self.assertEqual((items["grid"]["ax"], items["grid"]["ay"]), (90, 20))
        # An empty text removes the box
        model.apply(entry("Play", short=""))
        self.assertNotIn("Play", {item["id"] for item in model.items()})

    def test_a_press_expands(self):
        model = self.model()
        model.apply(entry("Play"))
        model.apply(entry("Song"))
        self.assertIsNone(model.expanded)
        model.apply(entry("Play", pressed=True, presses=1))
        self.assertEqual(model.expanded, "Play")
        # A press which was released before the host collected it counts too
        model.apply(entry("Song", presses=1))
        self.assertEqual(model.expanded, "Song")
        model.toggle_expanded("Song")
        self.assertIsNone(model.expanded)
        model.toggle_expanded("Play")
        self.assertEqual([item["id"] for item in model.items() if item["expanded"]], ["Play"])


class RowsAndScopesTest(unittest.TestCase):
    """Rows of buttons as one box, the scopes, hiding the global boxes, the view and mode names."""

    def model(self):
        targets = {80 + i: sim_help.HelpTarget(10 + i, 40 + i * 20, 8, 18, 10, label=f"Scene {i + 1}") for i in range(8)}
        targets[90] = sim_help.HelpTarget(1, 4, 150, 30, 9, label="Play")
        return sim_help.HelpModel(lambda e: targets.get(e.get("control")), (262, 252), (40, 24, 198, 182))

    def test_a_row_is_one_area(self):
        model = self.model()
        members = [{"id": f"Scene{i + 1}", "bind": "cc", "channel": 0, "control": 80 + i} for i in range(8)]
        model.apply(entry("Scene1..Scene8", bind=None, control=-1, members=members, scope="global"))
        item = model.items()[0]
        self.assertTrue(item["area"])
        self.assertEqual(item["label"], "Scene 1–8")
        self.assertEqual(len(item["parts"]), 8)
        self.assertEqual((item["x"], item["w"]), (40, 7 * 20 + 18))
        self.assertEqual(item["side"], "top")

    def test_scopes_context_and_hiding(self):
        model = self.model()
        model.apply(entry("Play", scope="global"))
        model.apply(entry("Record", control=90, scope="modifier"))
        model.apply(entry("@view", short="Session", bind=None))
        model.apply(entry("@mode", short="Volume", bind=None))
        self.assertEqual(model.context, {"view": "Session", "mode": "Volume"})
        self.assertEqual({i["id"]: i["scope"] for i in model.items()}, {"Play": "global", "Record": "modifier"})
        model.set_hide_global(True)
        self.assertEqual([i["id"] for i in model.items()], ["Record"])

    def test_row_label(self):
        self.assertEqual(sim_help.row_label(["Group A", "Group H"]), "Group A–H")
        self.assertEqual(sim_help.row_label(["Scene 1", "Scene 12"]), "Scene 1–12")


class ServerTest(unittest.TestCase):

    def test_lines_over_the_socket(self):
        with tempfile.TemporaryDirectory(prefix="dbm-help-") as directory:
            path = os.path.join(directory, "h.sock")
            lines, connects = [], []
            server = sim_help.HelpServer(path, lines.append, connects.append)
            server.start()
            try:
                client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                client.connect(path)
                client.sendall(b'{"id": "a"}\n{"id"')
                client.sendall(b': "b"}\n\n')
                client.close()
                deadline = time.monotonic() + 3
                while (len(lines) < 2 or connects != [True, False]) and time.monotonic() < deadline:
                    time.sleep(0.02)
                self.assertEqual(lines, ['{"id": "a"}', '{"id": "b"}'])
                self.assertEqual(connects, [True, False])
            finally:
                server.stop()
            self.assertFalse(os.path.exists(path))


class JamMappingTest(unittest.TestCase):
    """The JAM simulator maps the core's entries to its controls (without a window)."""

    def setUp(self):
        from PySide6.QtCore import QCoreApplication
        self.app = QCoreApplication.instance() or QCoreApplication([])
        from maschine_simulator.qt_bridge import MaschineSimulator
        self.device = MaschineDevice("jam")
        self.sim = MaschineSimulator(self.device)

    def target(self, **fields):
        return self.sim._help_target(entry(fields.pop("id", "X"), **fields))

    def test_bindings(self):
        play = self.target(id="Play", control=90)
        self.assertEqual(self.device.by_key[play.key].label, "Play")
        encoder = self.target(id="MasterKnob", control=57)
        self.assertEqual(self.device.by_key[encoder.key].kind, "encoder")
        self.assertEqual(encoder.label, "Encoder")
        scene = self.target(id="Scene1", control=80)
        self.assertEqual(scene.label, "Scene 1")
        song = self.target(id="Session", control=30)
        self.assertEqual(song.side, "top")
        # Shift is NI's SysEx: by its id
        shift = self.sim._help_target(entry("Shift", bind=None, control=-1))
        self.assertEqual(shift.key, self.device.shift_key)
        self.assertIsNone(self.target(id="Footswitch1", control=99))

    def test_areas_and_sides(self):
        model = self.sim.help_model
        for item in (entry("grid", bind=None), entry("strips", bind=None), entry("Play", control=90), entry("Select", control=50),
                     entry("RowSelect1", control=70)):
            model.apply(item)
        sides = {item["id"]: item["side"] for item in model.items()}
        self.assertEqual(sides, {"grid": "top", "strips": "bottom", "Play": "bottom", "Select": "right", "RowSelect1": "bottom"})


REPO = HERE.parents[2]
LIBRARY = os.environ.get("DBM_LIBRARY") or str(REPO / "target" / "debug" / "libdrivenbymoss.so")
JAM = "72F32A83-A697-446B-9016-64AA3F9476E4"
# The core's controls the JAM simulator doesn't draw
UNMAPPED = {"Footswitch1", "Footswitch2"}


@unittest.skipUnless(os.path.exists(LIBRARY), "the core library is not built (cargo build)")
class JamCoreHelpTest(unittest.TestCase):
    """The real core's entries in the JAM simulator: every one goes to a control, in the states
    which every button makes; and the boxes are drawn (offscreen)."""

    def setUp(self):
        from PySide6.QtCore import QCoreApplication
        self.app = QCoreApplication.instance() or QCoreApplication([])

    def run_core(self):
        sys.path.insert(0, str(REPO / "bindings" / "python"))
        os.environ.setdefault("DBM_LIBRARY", LIBRARY)
        from dbm_host import Bridge, SimDaw
        from maschine_simulator.qt_bridge import MaschineSimulator
        daw = SimDaw().build(tracks=[("Synth", "INSTRUMENT"), ("Drums", "INSTRUMENT")], effect_tracks=["FX 1"], scenes=8)
        daw.tracks[0].select()
        box = []
        device = MaschineDevice("jam", send=lambda data, port: box[0].receive(data))
        sim = MaschineSimulator(device)
        config = tempfile.TemporaryDirectory()
        self.addCleanup(config.cleanup)
        bridge = Bridge(daw, JAM, config.name, "Bitwig Studio 5.3")
        box.append(bridge)
        bridge.on_midi_out = lambda data: device.receive(bytes(data))
        bridge.start()
        self.addCleanup(bridge.close)
        device.attach()
        bridge.wait_for_startup()
        bridge.advance(300)
        return device, sim, bridge

    def test_every_entry_has_a_control(self):
        device, sim, bridge = self.run_core()
        lines = bridge.app.take_control_help_lines()
        self.assertGreater(len(lines), 40)
        unmapped = set()

        def check(lines):
            for line in lines:
                item = json.loads(line)
                if item["short"] and not item["id"].startswith("@") and sim.help_model.target(item)[0] is None:
                    unmapped.add(item["id"])

        check(lines)
        for control in device.controls:
            if control.kind != "button":
                continue
            device.press_button(control.key)
            bridge.advance(30)
            check(bridge.app.take_control_help_lines())
            device.release_button(control.key)
            bridge.advance(30)
            check(bridge.app.take_control_help_lines())
        self.assertEqual(unmapped - UNMAPPED, set())
        self.assertEqual(bridge.core_errors(), [])

    def test_boxes_are_drawn(self):
        device, sim, bridge = self.run_core()
        lines = bridge.app.take_control_help_lines()
        # Play is pressed: its box is expanded
        lines = [line.replace('"pressed":false', '"pressed":true') if '"id":"Play"' in line else line for line in lines]
        with tempfile.TemporaryDirectory(prefix="dbm-help-") as directory:
            feed = os.path.join(directory, "help.jsonl")
            Path(feed).write_text("\n".join(lines) + "\n")
            shot = os.path.join(directory, "jam.png")
            import subprocess
            env = dict(os.environ, QT_QPA_PLATFORM="offscreen")
            result = subprocess.run([sys.executable, "-m", "maschine_simulator", "--model", "jam", "--no-alsa", "--help-socket",
                                     os.path.join(directory, "h.sock"), "--help-file", feed, "--show-help", "--size", "1600x1000",
                                     "--quit-after", "1.5", "--screenshot", shot], cwd=str(HERE.parent), env=env,
                                    capture_output=True, text=True, timeout=60)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            from PySide6.QtGui import QImage
            image = QImage(shot)
            self.assertFalse(image.isNull())
            colours = {}
            for x in range(0, image.width(), 2):
                for y in range(0, image.height(), 2):
                    c = image.pixelColor(x, y)
                    colours[(c.red(), c.green(), c.blue())] = colours.get((c.red(), c.green(), c.blue()), 0) + 1
            # The colour bars of the scopes: global, view, mode
            for rgb in ((0x9A, 0xA0, 0xAB), (0x56, 0xB4, 0xE9), (0xE6, 0x9F, 0x00)):
                self.assertGreater(colours.get(rgb, 0), 20, f"no boxes of the colour {rgb}")


class HostWriterTest(unittest.TestCase):

    def test_writer_reconnects(self):
        # The runner itself (tools/dbm_host_alsa.py here only runs it)
        import importlib.util
        spec = importlib.util.spec_from_file_location("push_dbm_host_alsa", HERE.parents[1] / "push_simulator" / "tools" / "dbm_host_alsa.py")
        dbm_host_alsa = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(dbm_host_alsa)
        with tempfile.TemporaryDirectory(prefix="dbm-help-") as directory:
            path = os.path.join(directory, "h.sock")
            writer = dbm_host_alsa.HelpWriter(path)
            self.assertFalse(writer.connect(), "no simulator")
            lines = []
            server = sim_help.HelpServer(path, lines.append)
            server.start()
            try:
                writer.retry_at = 0
                self.assertTrue(writer.connect())
                self.assertFalse(writer.connect(), "connected already")
                writer.write(['{"id": "Play"}', '{"id": "Song"}'])
                deadline = time.monotonic() + 3
                while len(lines) < 2 and time.monotonic() < deadline:
                    time.sleep(0.02)
                self.assertEqual(lines, ['{"id": "Play"}', '{"id": "Song"}'])
            finally:
                writer.close()
                server.stop()


if __name__ == "__main__":
    unittest.main()
