# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Tests of the Python binding: drive a Launchpad X instance (and a single Launch Control XL as
for Bitwig) through drivenbymoss.App with a recording backend, as bindings/cpp/tests does.

Run from the repository root (after 'cargo build')::

    python3 -m unittest discover -s bindings/python/tests
"""

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import drivenbymoss  # noqa: E402

LAUNCHPAD_X = "de.mossgrabers.reaper.controller.novation.launchpad.LaunchpadXControllerInstance"
LAUNCH_CONTROL_XL = "5AFA5045-FF92-4737-A091-5B371CD5E529"

# The device IDs of the host; the names are the ones of the automatic detection
INPUT_ID = 3
OUTPUT_ID = 5


class RecordingBackend(drivenbymoss.Backend):
    """Records all calls; offers the MIDI ports of a Launchpad X."""

    def __init__(self):
        self.commands = []
        self.midi_out = []
        self.opened = []
        self.closed = []
        self.logs = []
        self.raise_in = set()
        self.app = None

    def _maybe_raise(self, name):
        if name in self.raise_in:
            raise RuntimeError(f"Must not escape into the library ({name})")

    def log(self, message):
        self.logs.append(message)

    def process(self, processor, command, value):
        self._maybe_raise("process")
        self.commands.append((processor, command, value))

    def get_midi_inputs(self):
        return [drivenbymoss.MidiDevice(INPUT_ID, "Launchpad X LPX MIDI Out")]

    def get_midi_outputs(self):
        return [(OUTPUT_ID, "Launchpad X LPX MIDI In")]

    def open_midi_input(self, device_id):
        self.opened.append(device_id)
        return True

    def open_midi_output(self, device_id):
        self.opened.append(device_id)
        return True

    def close_midi_input(self, device_id):
        self.closed.append(device_id)

    def close_midi_output(self, device_id):
        self.closed.append(device_id)

    def send_midi_data(self, device_id, data):
        self._maybe_raise("send_midi_data")
        if "reenter" in self.raise_in:
            self.app.tick()
        self.midi_out.append((device_id, data))

    def pad_updates(self):
        return [data for device_id, data in self.midi_out if device_id == OUTPUT_ID and data[:1] == b"\x90"]


class BindingTest(unittest.TestCase):

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="dbm-python-test-")
        self.dir = self.temp.name
        Path(self.dir, "DrivenByMoss4Reaper.config").write_text(f"CONTROLLER_INSTANCE0={LAUNCHPAD_X}\n")

    def tearDown(self):
        self.temp.cleanup()

    def start(self, backend=None):
        backend = backend or RecordingBackend()
        app = drivenbymoss.App(backend, self.dir, "7.22/linux-x86_64")
        backend.app = app
        app.start_infrastructure()
        app.update_model("track/count 1\ntrack/0/exists 1\ntrack/0/select 1\n")
        app.tick()
        return app, backend

    def test_version_and_controllers(self):
        self.assertRegex(drivenbymoss.version(), r"^\d+\.\d+")
        models = [c["model"] for c in drivenbymoss.list_controllers()]
        self.assertIn("Launch Control XL", models)

    def test_create_and_destroy(self):
        backend = RecordingBackend()
        app = drivenbymoss.App(backend, self.dir, "7.22/linux-x86_64")
        self.assertFalse(app.closed)
        app.start_infrastructure()
        self.assertEqual(backend.opened, [INPUT_ID, OUTPUT_ID])
        app.close()
        self.assertTrue(app.closed)
        self.assertEqual(sorted(backend.closed), [INPUT_ID, OUTPUT_ID])
        app.close()  # a second close does nothing
        with self.assertRaises(RuntimeError):
            app.tick()

        # The context manager, and a backend with all the default (no-op) methods
        with drivenbymoss.App(drivenbymoss.Backend(), str(Path(self.dir, "does-not-exist"))) as app:
            app.start_infrastructure()
            app.tick()
            self.assertIsNone(app.get_model_setup())
        self.assertTrue(app.closed)

    def test_create_controller(self):
        backend = RecordingBackend()
        with self.assertRaises(ValueError):
            drivenbymoss.App.create_controller(backend, "unknown", self.dir)
        with drivenbymoss.App.create_controller(backend, LAUNCH_CONTROL_XL, self.dir, "Bitwig Studio 5.3") as app:
            self.assertIsNone(app.get_model_setup())
            app.start_infrastructure()
            self.assertEqual(backend.opened, [0, 0])
            setup = app.get_model_setup()
            self.assertEqual(setup["numSends"], 2)
            self.assertGreater(setup["numTracks"], 0)
            self.assertTrue(backend.logs)

    def test_tick(self):
        app, backend = self.start()
        with app:
            self.assertIn(("refresh", "", None), backend.commands)
            backend.midi_out.clear()
            app.update_model("track/0/name Bass\n")
            app.tick()
            app.tick()
            # The pads are flushed on the first tick; afterwards only changes are sent
            self.assertLess(len(backend.pad_updates()), 64)
            backend.midi_out.clear()
            app.restart_controllers()
            app.update_model("track/count 1\ntrack/0/exists 1\ntrack/0/select 1\n")
            app.tick()
            self.assertGreaterEqual(len(backend.pad_updates()), 64)

    def test_midi_in_and_out(self):
        app, backend = self.start()
        with app:
            self.assertGreaterEqual(len(backend.pad_updates()), 64)
            backend.midi_out.clear()
            # Press the lower left pad: its LED changes
            app.on_midi_event(INPUT_ID, bytes([0x90, 11, 127]))
            app.tick()
            pressed = [data for data in backend.pad_updates() if data[1] == 11]
            self.assertEqual(len(pressed), 1, backend.midi_out)
            backend.midi_out.clear()
            # Release it (a list of ints works as well): the LED changes back
            app.on_midi_event(INPUT_ID, [0x90, 11, 0])
            app.tick()
            released = [data for data in backend.pad_updates() if data[1] == 11]
            self.assertEqual(len(released), 1, backend.midi_out)
            self.assertNotEqual(released[0], pressed[0])

    def test_settings_round_trip(self):
        app, backend = self.start()
        with app:
            settings = {s.label: s for s in app.get_settings()}
            base = settings["Base"]
            self.assertEqual((base.scope, base.kind, base.value), ("document", "enum", "C"))
            self.assertIn("G", base.extra)
            app.take_setting_changes()
            self.assertIsInstance(app.take_setting_states(), list)
            app.take_notifications()

            app.set_setting(base.id, "G")
            self.assertEqual({s.id: s.value for s in app.get_settings()}[base.id], "G")
            # A change of the host is not reported back
            self.assertNotIn(base.id, [setting_id for setting_id, _ in app.take_setting_changes()])
            self.assertEqual(app.take_setting_changes(), [])
            self.assertEqual(app.take_notifications(), [])

    def test_document_settings(self):
        app, backend = self.start()
        with app:
            base = next(s for s in app.get_settings() if s.label == "Base")
            app.set_setting(base.id, "D")
            document = app.get_formatted_document_settings()
            self.assertTrue(document.startswith('LAUNCHPAD_X="'), document)
            app.set_default_document_settings()
            self.assertEqual({s.id: s.value for s in app.get_settings()}[base.id], "C")

        # Restore the stored settings in a new instance
        app, backend = self.start()
        with app:
            app.set_formatted_document_settings(document)
            self.assertEqual({s.id: s.value for s in app.get_settings()}[base.id], "D")

    def test_exception_in_callback(self):
        app, backend = self.start()
        with app:
            backend.raise_in = {"process", "send_midi_data"}
            with self.assertLogs("drivenbymoss", "ERROR") as logs:
                app.restart_controllers()
                app.update_model("track/count 1\ntrack/0/exists 1\n")
                app.tick()
            self.assertTrue(any("process_no_arg" in line for line in logs.output), logs.output)
            self.assertTrue(any("send_midi_data" in line and "Must not escape" in line for line in logs.output))
            backend.raise_in = set()

            # Still working
            backend.midi_out.clear()
            app.restart_controllers()
            app.update_model("track/count 1\ntrack/0/exists 1\n")
            app.tick()
            self.assertGreaterEqual(len(backend.pad_updates()), 64)

    def test_reentrant_call_is_refused(self):
        app, backend = self.start()
        with app:
            backend.raise_in = {"reenter"}
            backend.midi_out.clear()
            with self.assertLogs("drivenbymoss", "ERROR") as logs:
                app.on_midi_event(INPUT_ID, [0x90, 11, 127])
                app.tick()
            self.assertTrue(any("must not be called from within its callbacks" in line for line in logs.output))
            backend.raise_in = set()
            app.tick()

    def test_failing_open_and_device_list(self):
        class Failing(RecordingBackend):
            def get_midi_inputs(self):
                raise RuntimeError("no devices")

            def open_midi_output(self, device_id):
                raise RuntimeError("cannot open")

        backend = Failing()
        with self.assertLogs("drivenbymoss", "ERROR"):
            with drivenbymoss.App(backend, self.dir) as app:
                app.start_infrastructure()
                app.tick()
        self.assertEqual(backend.opened, [])


if __name__ == "__main__":
    unittest.main()
