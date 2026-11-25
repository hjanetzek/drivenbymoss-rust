# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The mirror of a real device (doc/CONTROLLER_HELP_PLAN.md §4): the core's MIDI to the device
("out") lights the simulator as if it came from its ALSA port, the hardware's input ("in") shows
as pressed and is never sent back, the simulator's own input goes to the host, and a (re)connect
starts the mirror again (the core sends the complete state then)."""

import importlib.util
import json
import os
import sys
import tempfile
import time
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

from maschine_simulator import ni  # noqa: E402
from maschine_simulator.device import MaschineDevice  # noqa: E402
from sim_common import help as sim_help  # noqa: E402

REPO = HERE.parents[2]
LIBRARY = os.environ.get("DBM_LIBRARY") or str(REPO / "target" / "debug" / "libdrivenbymoss.so")
JAM = "72F32A83-A697-446B-9016-64AA3F9476E4"
MK2 = "015E4908-C049-4808-A645-A7A44BEFE36B"
MIRROR_LINE = json.dumps({"mirror": "Maschine JAM", "source": "hardware", "input": True})


def out_line(data: bytes) -> str:
    return json.dumps({"midi": "out", "data": bytes(data).hex()})


def in_line(data: bytes) -> str:
    return json.dumps({"midi": "in", "data": bytes(data).hex()})


def host_module():
    spec = importlib.util.spec_from_file_location("push_dbm_host_alsa", HERE.parents[1] / "push_simulator" / "tools" / "dbm_host_alsa.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class QtTest(unittest.TestCase):

    def setUp(self):
        from PySide6.QtCore import QCoreApplication
        self.app = QCoreApplication.instance() or QCoreApplication([])

    def simulator(self, model: str = "jam"):
        """A simulator (without a window) whose own MIDI is recorded (it must not reach ALSA
        while mirroring)."""
        from maschine_simulator.qt_bridge import MaschineSimulator
        sent = []
        box = []

        def send(data, port):
            if box and box[0].mirror_output(data):
                return
            sent.append(bytes(data))

        device = MaschineDevice(model, send=send)
        sim = MaschineSimulator(device)
        box.append(sim)
        return device, sim, sent


class MirrorStreamTest(QtTest):
    """The real core's MIDI to the device, as the mirror's "out" lines: the mirroring simulator
    shows what a simulator driven over its port shows."""

    def run_core(self, uuid: str, model: str):
        sys.path.insert(0, str(REPO / "bindings" / "python"))
        os.environ.setdefault("DBM_LIBRARY", LIBRARY)
        from dbm_host import Bridge, SimDaw
        daw = SimDaw().build(tracks=[("Synth", "INSTRUMENT"), ("Drums", "INSTRUMENT")], effect_tracks=["FX 1"], scenes=8)
        daw.tracks[0].select()
        daw.tracks[0].slots[0].create_clip(4.0)
        box = []
        # The reference: a simulator on the core's port
        direct = MaschineDevice(model, send=lambda data, port: box[0].receive(data))
        mirror_device, sim, sent = self.simulator(model)
        sim._on_help_line(json.dumps({"mirror": "Maschine", "source": "hardware"}))
        config = tempfile.TemporaryDirectory()
        self.addCleanup(config.cleanup)
        bridge = Bridge(daw, uuid, config.name, "Bitwig Studio 5.3")
        box.append(bridge)

        def to_device(data):
            direct.receive(bytes(data))
            sim._on_help_line(out_line(bytes(data)))

        bridge.on_midi_out = to_device
        bridge.start()
        self.addCleanup(bridge.close)
        direct.attach()
        bridge.wait_for_startup()
        bridge.advance(300)
        return direct, mirror_device, sim, sent

    def test_jam(self):
        direct, mirrored, sim, sent = self.run_core(JAM, "jam")
        self.assertEqual(mirrored.leds, direct.leds)
        self.assertTrue(any(mirrored.leds[c.key] for c in mirrored.controls if c.kind == "pad"), "the pads are lit")
        self.assertEqual([vars(s) for s in mirrored.strips], [vars(s) for s in direct.strips])
        self.assertTrue(any(s.colour for s in mirrored.strips), "the strips are set up")
        self.assertEqual(mirrored.meters, direct.meters)
        # The drawn colours too
        for control in mirrored.controls:
            self.assertEqual(mirrored.led_rgb(control.key), direct.led_rgb(control.key), control.label)
        self.assertEqual(sent, [], "a mirror sends nothing to ALSA")
        self.assertIn("Mirror: Maschine (hardware)", sim.statusText)

    def test_mk2_displays(self):
        direct, mirrored, sim, sent = self.run_core(MK2, "mk2")
        self.assertEqual(mirrored.leds, direct.leds)
        self.assertEqual(mirrored.display_lines(0), direct.display_lines(0))
        self.assertEqual(mirrored.display_lines(1), direct.display_lines(1))
        self.assertTrue("".join(mirrored.display_lines(0)).strip(), "the displays show text")
        # The core's query is answered by the hardware, not by the mirror
        self.assertEqual(sent, [])


class MirrorInputTest(QtTest):
    """The hardware's input ("in" lines): pressed in the simulator, the help box expanded, no LED
    changed and nothing sent back."""

    def button(self, device, label):
        control = next(c for c in device.controls if c.kind == "button" and c.label == label and c.template)
        return control, device.inputs[control.template]

    def test_the_jam_device_model(self):
        device = MaschineDevice("jam")
        play, t = self.button(device, "Play")
        self.assertEqual(device.mirror_input(bytes([0xB0 | t.channel, t.number, t.on])), [(play.key, True)])
        self.assertEqual(device.mirror_input(bytes([0xB0 | t.channel, t.number, t.off])), [(play.key, False)])
        pad = next(c for c in device.controls if c.kind == "pad")
        pt = device.inputs[pad.template]
        self.assertEqual(device.mirror_input(bytes([0x90 | pt.channel, pt.number, 100])), [(pad.key, True)])
        self.assertEqual(device.mirror_input(bytes([0x80 | pt.channel, pt.number, 0])), [(pad.key, False)])
        strip = next(c for c in device.controls if c.kind == "strip" and c.index == 2)
        touch, move = device.inputs[strip.touch], device.inputs[strip.template]
        self.assertEqual(device.mirror_input(bytes([0xB0, touch.number, 127])), [(strip.key, True)])
        self.assertEqual(device.mirror_input(bytes([0xB0, move.number, 64])), [(strip.key, None)])
        encoder = next(c for c in device.controls if c.kind == "encoder")
        et = device.inputs[encoder.template]
        self.assertEqual(device.mirror_input(bytes([0xB0, et.number, 1])), [(encoder.key, None)])
        self.assertEqual(device.mirror_input(ni.ni_shift("jam", True)), [(device.shift_key, True)])
        self.assertTrue(device.shift_down, "the Shift LED is lit as the host does")
        self.assertEqual(device.mirror_input(bytes([0xB0, 127, 1])), [])
        # No LED changed: the "in" messages are not the core's
        self.assertFalse(any(device.leds.values()))

    def test_in_lines_highlight_without_sending(self):
        device, sim, sent = self.simulator()
        server_sent = []
        sim._help_server = type("Server", (), {"send": lambda self, line: server_sent.append(line), "stop": lambda self: None,
                                               "path": ""})()
        play, t = self.button(device, "Play")
        sim._on_help_line(json.dumps({"id": "Play", "label": "PLAY", "bind": "cc", "channel": t.channel, "control": t.number,
                                      "short": "Start playback"}))
        # Without a mirror an "in" line does nothing
        sim._on_help_line(in_line(bytes([0xB0, t.number, 127])))
        self.assertFalse(sim.controlPressed(play.key))
        sim._on_help_line(MIRROR_LINE)
        self.assertIn("Mirror: Maschine JAM (hardware), clicks act on the core", sim.statusText)
        sim._on_help_line(in_line(bytes([0xB0 | t.channel, t.number, t.on])))
        self.assertTrue(sim.controlPressed(play.key))
        self.assertEqual(sim.help_model.expanded, "Play")
        self.assertEqual(device.leds[play.key], 0, "the press lights no LED")
        sim._on_help_line(in_line(bytes([0xB0 | t.channel, t.number, t.off])))
        self.assertFalse(sim.controlPressed(play.key))
        # A pad of the hardware expands the pads' box
        sim._on_help_line(json.dumps({"id": "grid", "label": "Pads", "bind": None, "short": "Play notes"}))
        pad = next(c for c in device.controls if c.kind == "pad")
        sim._on_help_line(in_line(bytes([0x90, device.inputs[pad.template].number, 100])))
        self.assertEqual(sim.help_model.expanded, "grid")
        # A turn flashes
        encoder = next(c for c in device.controls if c.kind == "encoder")
        sim._on_help_line(in_line(bytes([0xB0, device.inputs[encoder.template].number, 1])))
        self.assertTrue(sim.controlPressed(encoder.key))
        sim._mirror_pressed[encoder.key] = time.monotonic() - 1
        sim._tick()
        self.assertFalse(sim.controlPressed(encoder.key))
        # Nothing went back to the host or to ALSA
        self.assertEqual(server_sent, [])
        self.assertEqual(sent, [])
        # The simulator's own click goes to the host as the device's input, not to ALSA
        device.press_button(play.key)
        self.assertEqual([json.loads(line) for line in server_sent], [{"midi": "in", "data": bytes([0xB0 | t.channel, t.number, t.on]).hex()}])
        self.assertEqual(sent, [])
        # Clicks ignored: dropped
        sim.mirror_input_enabled = False
        device.release_button(play.key)
        self.assertEqual(len(server_sent), 1)
        self.assertEqual(sent, [])
        self.assertIn("clicks ignored", sim.statusText)
        # The host went away: no mirror, the simulator's MIDI goes to its port again
        sim._on_help_connect(False)
        self.assertFalse(sim.mirroring)
        device.press_button(play.key)
        self.assertEqual(len(sent), 1)

    def test_a_new_mirror_forgets_the_old_state(self):
        device, sim, _ = self.simulator()
        sim._on_help_line(MIRROR_LINE)
        play, t = self.button(device, "Play")
        sim._on_help_line(out_line(bytes([0xB0, t.number, 127])))
        self.assertEqual(device.leds[play.key], 127)
        sim._on_help_line(in_line(bytes([0xB0, t.number, 127])))
        sim._on_help_line(MIRROR_LINE)
        self.assertEqual(device.leds[play.key], 0)
        self.assertFalse(sim.controlPressed(play.key))


class FakeApp:
    """The core's mirror functions: a mirror start "refreshes" (queues the complete state)."""

    def __init__(self, state):
        self.state = state
        self.queue = []
        self.injected = []
        self.active = False
        self.starts = 0

    def mirror_device_midi(self, on):
        self.active = on
        self.queue = [out_line(m) for m in self.state] if on else []
        self.starts += int(on)
        return True

    def take_device_midi_lines(self):
        lines, self.queue = self.queue, []
        return lines

    def inject_device_midi(self, data):
        self.injected.append(bytes(data))
        return True


class HostMirrorTest(QtTest):
    """The host's side over a real socket: the start line and the complete state on every
    (re)connect, the simulator's input played in, the hardware's input not sent back."""

    def wait(self, condition, seconds=3.0):
        deadline = time.monotonic() + seconds
        while not condition() and time.monotonic() < deadline:
            self.app.processEvents()
            time.sleep(0.01)
        self.app.processEvents()
        return condition()

    def test_replay_on_connect_and_input(self):
        host = host_module()
        device, sim, sent = self.simulator()
        play, t = MirrorInputTest.button(self, device, "Play")
        pad = next(c for c in device.controls if c.kind == "pad")
        pad_led = device._led_template(pad)
        state = [bytes([0xB0, t.number, 127]), bytes([0x90, pad_led.number, 21])]
        app = FakeApp(state)
        with tempfile.TemporaryDirectory(prefix="dbm-mirror-") as directory:
            path = os.path.join(directory, "h.sock")
            self.assertTrue(sim.start_help_server(path))
            self.addCleanup(sim.stop_help_server)
            writer = host.HelpWriter(path)
            mirror = host.Mirror(app, "Maschine JAM")
            try:
                for _ in range(2):
                    writer.retry_at = 0
                    self.assertTrue(writer.connect())
                    mirror.start(writer)
                    mirror.forward(writer)
                    self.assertTrue(self.wait(lambda: device.leds[play.key] == 127 and device.leds[pad.key] == 21), device.leds[play.key])
                    self.assertTrue(sim.mirroring)
                    # The hardware's press: shown, not sent back
                    sim._on_help_line(in_line(bytes([0xB0, t.number, 127])))
                    self.assertTrue(sim.controlPressed(play.key))
                    mirror.forward(writer)
                    self.assertEqual(app.injected, [])
                    # The simulator's click: played in as the device's
                    device.press_button(play.key)
                    self.assertTrue(self.wait(lambda: (mirror.forward(writer) or True) and app.injected))
                    self.assertEqual(app.injected, [bytes([0xB0, t.number, t.on])])
                    app.injected.clear()
                    self.assertEqual(sent, [])
                    # The simulator goes away and comes back (a reconnect): the state again
                    writer.drop()
                    mirror.forward(writer)
                    self.assertFalse(app.active, "no queueing without the simulator")
                    self.assertTrue(self.wait(lambda: not sim.mirroring))
                    device.leds[play.key] = 0
                    device.leds[pad.key] = 0
                self.assertEqual(app.starts, 2)
            finally:
                writer.close()

    def test_a_simulator_which_does_not_read_is_dropped(self):
        host = host_module()
        writer = host.HelpWriter("/nonexistent")
        import socket
        a, b = socket.socketpair()
        self.addCleanup(b.close)
        a.setblocking(False)
        writer.sock = a
        for _ in range(200):
            writer.write(["x" * 16384])
            if writer.sock is None:
                break
        self.assertIsNone(writer.sock, "the writer never blocks: the connection is dropped")


if __name__ == "__main__":
    unittest.main()
