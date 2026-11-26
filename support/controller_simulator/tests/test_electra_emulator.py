# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The real core against the Electra One emulator (the real firmware built for Linux, the
electra-one project's ``build/electra``), headless and without ALSA: dbm_host's Bridge with the
simulated DAW in-process, the emulator's virtual cables over its UDP control port (``vmidi send``
into Port 1 and CTRL, ``vmidi log on`` for what the firmware sends). The emulator loads the
DrivenByMoss preset (``--load DrivenByMoss.eproj``).

Checked: the handshake (online), the names and colors on the screen (pixels of the screenshot
data), a knob turn and a knob touch reaching the DAW, a pad touch selecting a track, and the pages
switched on the device selecting the modes. The surface link: the JAM in the same process shows the
Electra's page and turns its knobs (doc/SURFACE_LINK.md).

Skipped without the emulator (``ELECTRA_ONE_BIN``, default ``../electra-one/build/electra`` next
to this repository), the preset (``ELECTRA_ONE_PRESET``, default the emulator's
``data/DrivenByMoss.eproj``) or the built library (cargo build)."""

import os
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
REPO = os.path.abspath(os.path.join(ROOT, "..", ".."))
ELECTRA_ONE = os.path.join(os.path.dirname(REPO), "electra-one")
EMULATOR = os.environ.get("ELECTRA_ONE_BIN") or os.path.join(ELECTRA_ONE, "build", "electra")
PRESET = os.environ.get("ELECTRA_ONE_PRESET") or os.path.join(ELECTRA_ONE, "data", "DrivenByMoss.eproj")
LIBRARY = os.environ.get("DBM_LIBRARY") or os.path.join(REPO, "target", "debug", "libdrivenbymoss.so")
UUID = "24342204-B891-4F62-BC43-8ABA1DE1D690"
JAM_UUID = "72F32A83-A697-446B-9016-64AA3F9476E4"
# The virtual cables of the core's ports: 0 = Port 1 (the controls), 1 = CTRL (SysEx)
CABLES = ("port1", "ctrl")
TICK_MS = 30


def free_udp_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


class Emulator:
    """The emulator process and its control port; MIDI of the firmware arrives as datagrams
    ``vmidi CABLE HEX`` on the same socket as the replies."""

    def __init__(self, directory: str, model: str):
        self.port = free_udp_port()
        self.log = open(os.path.join(directory, "electra.log"), "w")
        self.process = subprocess.Popen(
            [EMULATOR, "--headless", "--no-alsa", "--port", str(self.port), "--model", model,
             "--card", os.path.join(directory, "card"), "--eeprom", os.path.join(directory, "card.eeprom"),
             "--load", PRESET, "--slot", "1"],
            stdout=self.log, stderr=subprocess.STDOUT, cwd=directory)
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.sock.bind(("127.0.0.1", 0))
        self.midi: list[tuple[int, bytes]] = []
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError(f"the emulator stopped ({self.process.returncode})")
            try:
                if self.command("ping", timeout=0.3).startswith("pong"):
                    break
            except TimeoutError:
                pass
        else:
            raise RuntimeError("the emulator does not answer")

    def command(self, text: str, timeout: float = 2.0) -> str:
        """Send a command and wait for its reply (MIDI datagrams in between are kept)."""
        self.sock.sendto(text.encode(), ("127.0.0.1", self.port))
        deadline = time.monotonic() + timeout
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError(text)
            self.sock.settimeout(remaining)
            try:
                data, _ = self.sock.recvfrom(1 << 20)
            except socket.timeout:
                raise TimeoutError(text) from None
            reply = data.decode(errors="replace")
            if not self.take_midi(reply):
                return reply

    def take_midi(self, datagram: str) -> bool:
        if not datagram.startswith("vmidi "):
            return False
        _, cable, hex_data = datagram.split(" ", 2)
        if cable in CABLES:
            self.midi.append((CABLES.index(cable), bytes.fromhex(hex_data.replace(" ", ""))))
        return True

    def poll(self, timeout: float) -> None:
        """Collect the MIDI which arrives within the timeout."""
        deadline = time.monotonic() + timeout
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                return
            self.sock.settimeout(remaining)
            try:
                data, _ = self.sock.recvfrom(1 << 20)
            except socket.timeout:
                return
            self.take_midi(data.decode(errors="replace"))

    def pixel(self, x: int, y: int) -> tuple[int, int, int]:
        value = int(self.command(f"pixel {x} {y}").split()[-1], 16)
        return (value >> 16, (value >> 8) & 0xFF, value & 0xFF)

    def close(self) -> None:
        try:
            self.command("quit", timeout=1)
        except (TimeoutError, OSError):
            pass
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
        self.sock.close()
        self.log.close()


@unittest.skipUnless(os.path.exists(EMULATOR) and os.path.exists(PRESET), "the Electra One emulator or its DrivenByMoss preset is not there")
@unittest.skipUnless(os.path.exists(LIBRARY), "the core library is not built (cargo build)")
class ElectraEmulatorTest(unittest.TestCase):
    def setUp(self):
        sys.path.insert(0, os.path.join(REPO, "bindings", "python"))
        os.environ.setdefault("DBM_LIBRARY", LIBRARY)
        self.directory = tempfile.mkdtemp(prefix="dbm-electra-")
        self.addCleanup(shutil.rmtree, self.directory, True)

    def run_loop(self, emulator, bridge, seconds: float, until=None) -> bool:
        """Pump MIDI both ways and tick the core in (about) real time."""
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            emulator.poll(TICK_MS / 1000)
            pending, emulator.midi = emulator.midi, []
            for port, data in pending:
                bridge.receive(data, port)
            bridge.advance(TICK_MS)
            if until is not None and until():
                return True
        return until is None

    def check_model(self, model: str):
        from dbm_host import Bridge, SimDaw
        daw = SimDaw().build(tracks=[("Synth", "INSTRUMENT"), ("Drums", "INSTRUMENT"), ("Bass", "AUDIO")], scenes=4)
        daw.tracks[0].select()
        emulator = Emulator(self.directory, model)
        self.addCleanup(emulator.close)
        self.assertEqual(emulator.command("vmidi log on"), "ok")
        bridge = Bridge(daw, UUID, os.path.join(self.directory, "config"), "Bitwig Studio 5.3")
        self.addCleanup(bridge.close)
        bridge.on_port_midi_out = lambda port, data: emulator.command(f"vmidi send {CABLES[port]} {bytes(data).hex(' ')}")
        bridge.start()

        # The handshake: device information, presets, the DrivenByMoss preset is selected
        self.assertTrue(self.run_loop(emulator, bridge, 10, lambda: any("Switching to mode: VOLUME" in log for log in bridge.logs)), bridge.logs)
        self.assertIn(f"Model: {model.upper()} - Firmware: v5.0.6", bridge.logs)
        self.assertEqual(bridge.core_errors(), [])
        self.run_loop(emulator, bridge, 1.0)

        # The screen: the pan bar of track 1 in the track's color, its SELECT pad lit (selected)
        r, g, b = emulator.pixel(30, 150)
        tr, tg, tb = daw.tracks[0].color
        self.assertLess(abs(r - tr) + abs(g - tg) + abs(b - tb), 40, ((r, g, b), (tr, tg, tb)))
        selected, other = emulator.pixel(30, 510), emulator.pixel(197, 510)
        self.assertNotEqual(selected, other)

        # A knob turn and a knob touch reach the DAW (knob 1: the volume of track 1)
        volume = daw.tracks[0].volume.value
        emulator.command("potTouch 1 1")
        self.run_loop(emulator, bridge, 0.3)
        self.assertTrue(daw.tracks[0].volume.touched)
        emulator.command("pot 1 8")
        self.run_loop(emulator, bridge, 0.5)
        self.assertGreater(daw.tracks[0].volume.value, volume)
        emulator.command("potTouch 1 0")
        self.run_loop(emulator, bridge, 0.3)
        self.assertFalse(daw.tracks[0].volume.touched)

        # A touch on the SELECT pad of track 2 selects it; the pads follow
        emulator.command("touch 260 527 200")
        self.assertTrue(self.run_loop(emulator, bridge, 3, lambda: daw.tracks[1].selected))
        self.run_loop(emulator, bridge, 1.0)
        # (the touched row's control set is active now and drawn brighter)
        cyan, gray = emulator.pixel(197, 510), emulator.pixel(30, 510)
        self.assertTrue(cyan[1] > cyan[0] + 10 and cyan[2] > cyan[0] + 10, cyan)
        self.assertLess(max(gray) - min(gray), 8, gray)

        # The pages switched on the device select the modes
        for page, mode in enumerate(("VOLUME", "SEND", "DEVICE_PARAMS", "EQ_DEVICE_PARAMS", "TRANSPORT", "SESSION", "PROJECT")):
            emulator.command(f"vmidi send ctrl F0 00 21 45 09 0A {page:02X} F7")
            self.assertTrue(self.run_loop(emulator, bridge, 3, lambda: bridge.logs[-1:] == [f"Switching to mode: {mode}"]), (mode, bridge.logs[-3:]))
        self.run_loop(emulator, bridge, 0.5)
        self.assertEqual(bridge.core_errors(), [])

    def test_mk2(self):
        self.check_model("mk2")

    def test_surface_link(self):
        """The surface link end to end: the Electra (emulator) and the JAM (driven in-process) as
        two controllers of one library on one DAW. The JAM shows the Electra's mixer page and its
        strips turn the Electra's knobs."""
        from dbm_host import Bridge, SimDaw
        daw = SimDaw().build(tracks=[("Synth", "INSTRUMENT"), ("Drums", "INSTRUMENT"), ("Bass", "AUDIO")], scenes=4)
        daw.tracks[0].select()
        emulator = Emulator(self.directory, "mk2")
        self.addCleanup(emulator.close)
        self.assertEqual(emulator.command("vmidi log on"), "ok")
        electra = Bridge(daw, UUID, os.path.join(self.directory, "electra"), "Bitwig Studio 5.3")
        self.addCleanup(electra.close)
        electra.on_port_midi_out = lambda port, data: emulator.command(f"vmidi send {CABLES[port]} {bytes(data).hex(' ')}")
        electra.start()
        jam = Bridge(daw, JAM_UUID, os.path.join(self.directory, "jam"), "Bitwig Studio 5.3")
        self.addCleanup(jam.close)
        jam.start()

        def run(seconds, until=None):
            deadline = time.monotonic() + seconds
            while time.monotonic() < deadline:
                emulator.poll(TICK_MS / 1000)
                pending, emulator.midi = emulator.midi, []
                for port, data in pending:
                    electra.receive(data, port)
                # The DAW moves once per tick, both controllers step
                daw.advance(TICK_MS)
                for bridge in (electra, jam):
                    bridge.clock_ms += TICK_MS
                    bridge.step()
                if until is not None and until():
                    return True
            return until is None

        self.assertTrue(run(10, lambda: any("Switching to mode: VOLUME" in log for log in electra.logs)), electra.logs)
        self.assertTrue(run(3, lambda: any("Surface link: a surface attached." in log for log in electra.logs)), electra.logs)
        run(0.5)

        # Select + CONTROL: the JAM shows the Electra's mixer page
        for control, value in ((50, 127), (39, 127), (39, 0), (50, 0)):
            jam.receive(bytes([0xB0, control, value]))
        self.assertTrue(run(2, lambda: "Surface: Electra One - Mixer" in jam.notifications), jam.notifications)

        # Strip 1 (touched, swiped up): the volume of track 1, through the Electra's knob 1; the
        # Electra gets the new value (CC 10 on channel 16 of Port 1)
        volume = daw.tracks[0].volume.value
        electra.midi_out_ports.clear()
        jam.receive(bytes([0xB0, 20, 127]))
        for position in range(40, 80, 4):
            jam.receive(bytes([0xB0, 8, position]))
            run(0.03)
        jam.receive(bytes([0xB0, 20, 0]))
        self.assertTrue(run(2, lambda: daw.tracks[0].volume.value > volume), (volume, daw.tracks[0].volume.value))
        run(0.5)
        self.assertTrue(any(port == 0 and data[:2] == bytes([0xBF, 10]) for port, data in electra.midi_out_ports))
        self.assertEqual(electra.core_errors(), [])
        self.assertEqual(jam.core_errors(), [])

    def test_mk1(self):
        self.check_model("mk1")


if __name__ == "__main__":
    unittest.main()
