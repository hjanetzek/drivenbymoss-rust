# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The OSC controller of the core on the Python reference host: a client on localhost sends OSC
commands to the controller's receive port and listens on its send port, the simulated DAW
executes the commands of the core and reports its changes back.

Run from the repository root (after 'cargo build')::

    python3 -m unittest discover -s bindings/python/tests
"""

import socket
import sys
import tempfile
import time
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "backends" / "bitwig" / "tools"))

from dbm_host import Bridge, SimDaw  # noqa: E402
from osc_monitor import build_message, parse_message  # noqa: E402

OSC = "94DD41B0-EFEE-11E3-AC10-0800200C9A66"


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.bind(("0.0.0.0", 0))
        return sock.getsockname()[1]


class OscOnReferenceHostTest(unittest.TestCase):

    def setUp(self):
        self.client = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.client.bind(("127.0.0.1", 0))
        self.client.setblocking(False)
        self.receive_port = free_port()
        self.folder = tempfile.TemporaryDirectory()
        # The ports in the configuration file of the controller (OSCConfiguration)
        Path(self.folder.name, "DrivenByMoss4Reaper-OSC.config").write_text(
            f"SETUP_PORT_TO_RECEIVE_ON={self.receive_port}\n"
            f"SETUP_PORT_TO_SEND_TO_(REQUIRES_RESTART)={self.client.getsockname()[1]}\n")
        self.daw = SimDaw().build(tracks=[("Synth", "INSTRUMENT"), ("Drums", "INSTRUMENT"), ("Audio", "AUDIO")], scenes=8)
        self.daw.tracks[0].select()
        self.bridge = Bridge(self.daw, OSC, self.folder.name).start()
        self.values: dict[str, list] = {}

    def tearDown(self):
        self.bridge.close()
        self.client.close()
        self.folder.cleanup()

    def send(self, address: str, *args) -> None:
        self.client.sendto(build_message(address, [str(a) for a in args]), ("127.0.0.1", self.receive_port))

    def run_core(self, ms: float = 150) -> None:
        """Advance the simulated time; the datagrams arrive in real time, so wait a bit between
        the ticks."""
        steps = max(1, int(ms // 30))
        for _ in range(steps):
            time.sleep(0.002)
            self.bridge.advance(30)

    def receive(self) -> list[tuple[str, list]]:
        """All messages which arrived (ticking the core, which sends a large update in several
        bundles)."""
        messages = []
        idle = 0
        # While playing the position changes at every tick: at most 100 rounds
        for _ in range(100):
            try:
                data, _ = self.client.recvfrom(65536)
            except BlockingIOError:
                idle += 1
                if idle == 10:
                    break
                self.run_core(30)
                continue
            idle = 0
            messages.extend(parse_message(data))
        for address, values in messages:
            self.values[address] = values
        return messages

    def test_update_and_commands(self):
        # The complete update one second after the startup of the controller
        self.bridge.wait_for_startup()
        self.run_core(1100)
        messages = self.receive()
        self.assertGreater(len(messages), 1000)
        self.assertEqual(messages[0], ("/update", [1]))
        self.assertEqual(self.values["/track/1/name"], ["Synth"])
        self.assertEqual(self.values["/track/2/name"], ["Drums"])
        self.assertEqual(self.values["/track/1/selected"], [1])
        self.assertEqual(self.values["/track/3/type"], ["audio"])
        self.assertEqual(self.values["/track/selected/name"], ["Synth"])
        self.assertEqual(self.values["/play"], [0])

        # Commands: the simulated DAW executes them, the changes come back
        self.send("/track/2/select", 1)
        self.send("/track/3/mute")
        self.send("/play")
        self.run_core()
        self.assertTrue(self.daw.tracks[1].selected)
        self.assertTrue(self.daw.tracks[2].mute)
        self.assertTrue(self.daw.transport.playing)
        self.receive()
        self.assertEqual(self.values["/track/2/selected"], [1])
        self.assertEqual(self.values["/track/1/selected"], [0])
        self.assertEqual(self.values["/track/3/mute"], [1])
        self.assertEqual(self.values["/track/selected/name"], ["Drums"])
        self.assertEqual(self.values["/play"], [1])

        self.send("/track/1/volume", 127)
        self.run_core()
        self.assertAlmostEqual(self.daw.tracks[0].volume.value, 1.0)
        self.receive()
        self.assertEqual(self.values["/track/1/volume"], [127])

        # The virtual keyboard plays into the note input of the controller
        self.send("/vkb_midi/1/note/60", 100)
        self.run_core()
        self.assertIn((0x90, 60, 100), self.bridge.played_notes)

        # Nothing is logged as an error
        self.assertEqual([], [log for log in self.bridge.core_errors() if "OSC" in log])


if __name__ == "__main__":
    unittest.main()
