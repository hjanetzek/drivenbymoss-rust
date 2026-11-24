# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
"""The simulated host pages (dbm_host/host_page.py, doc/HOST_PAGE_PLAN.md): the value math, the
layout and reports, the commands with their epochs, and end to end with the Maschine JAM in the
core: the JAM's input moves the host's (screen's) selection, a change on the screen moves the
JAM's highlight."""

import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

from dbm_host import Bridge, HostPages, HostPageView, SimDaw  # noqa: E402
from dbm_host.host_page import KIND_FADER, KIND_LIST, KIND_TOGGLE, KIND_TRIGGER, Control  # noqa: E402
from drivenbymoss import protocol  # noqa: E402

M = protocol.Msg
JAM = "72F32A83-A697-446B-9016-64AA3F9476E4"


def daw_with_pages():
    daw = SimDaw().build(tracks=[("Synth", "INSTRUMENT"), ("Drums", "INSTRUMENT"), ("Vocals", "AUDIO")], scenes=4)
    daw.tracks[0].select()
    daw.host_pages = HostPages(daw)
    return daw


class ControlTest(unittest.TestCase):

    def test_detents_and_travel(self):
        cutoff = Control("Cutoff", 2000.0, 20, 20000)
        cutoff.nudge(1)
        self.assertAlmostEqual(cutoff.value, 2000 + 199.8)
        cutoff.turn_travel(127)
        self.assertEqual(cutoff.value, 20000)
        wave = Control("Wave", 2, labels=["Sine", "Triangle", "Saw", "Square", "Noise"])
        self.assertEqual((wave.kind, wave.steps, wave.text), (KIND_LIST, 5, "Saw"))
        wave.nudge(1)
        self.assertEqual(wave.text, "Square")
        # A strip's travel: an item per quarter of the strip (5 items)
        wave.turn_travel(-20)
        self.assertEqual(wave.text, "Square")
        wave.turn_travel(-12)
        self.assertEqual(wave.text, "Saw")
        wave.press()
        wave.press()
        wave.press()
        self.assertEqual(wave.text, "Sine", "a press steps and wraps")
        sync = Control("Sync", 0, 0, 1, toggle=True)
        self.assertEqual(sync.kind, KIND_TOGGLE)
        sync.press()
        self.assertEqual(sync.text, "On")
        panic = Control("Panic", 0, 0, 1, trigger=True)
        panic.press()
        self.assertEqual((panic.kind, panic.fired), (KIND_TRIGGER, 1))
        level = Control("Level", -6.0, -60, 6)
        level.nudge(5)
        level.reset()
        self.assertEqual((level.kind, level.value), (KIND_FADER, -6.0))


class ViewTest(unittest.TestCase):

    def report(self, view):
        out = {}
        view.report(lambda k, v: out.__setitem__(k, v))
        return out

    def test_layout_reports_and_commands(self):
        daw = daw_with_pages()
        view = HostPageView(daw.host_pages)
        self.assertEqual(self.report(view), {}, "nothing while not shown")
        view.show(8, True, lambda: None)
        out = self.report(view)
        self.assertEqual((out[M.HOST_PAGE_COLUMNS], out[M.HOST_PAGE_ROWS], out[M.HOST_PAGE_COUNT]), (8, 6, 2))
        self.assertEqual(out[M.HOST_PAGE_TITLE], "Host · Synth")
        self.assertEqual([out[M.HOST_SECTION_NAME | s << 16] for s in range(out[M.HOST_SECTION_COUNT])], ["Chain", "Helm", "Reverb"])
        # Row 0: the chain's Mixer (cells 0-3), Helm's Osc 1 (cells 4-7)
        self.assertEqual([out[M.HOST_CELL_NAME | c << 16] for c in range(8)], ["Level", "Balance", "Mute", "Solo", "Wave", "Transpose", "Tune", "Volume"])
        self.assertEqual((out[M.HOST_CELL_KIND | 4 << 16], out[M.HOST_CELL_TEXT | 4 << 16], out[M.HOST_CELL_STEPS | 4 << 16]), (KIND_LIST, "Saw", 5))
        self.assertEqual((out[M.HOST_CELL_SET | 4 << 16], out[M.HOST_CELL_COLOR | 4 << 16]), (1, (255, 80, 0)))
        self.assertEqual((out[M.HOST_SET_SELECTED], out[M.HOST_PAGE_ROW], out[M.HOST_CELL_SELECTED], out[M.HOST_SECTION_SELECTED]), (0, 0, 0, 0))
        epoch = out[M.HOST_PAGE_EPOCH]

        # A press selects the page and the controller on the screen, and acts (a toggle switches)
        view.press(2, epoch, True)
        mute = daw.host_pages.pages()[0][1].controls[2]
        self.assertEqual(mute.value, 1)
        view.press(13, epoch, True)
        self.assertEqual((daw.host_pages.screen.page, daw.host_pages.screen.slot), (3, 1))
        out = self.report(view)
        self.assertEqual((out[M.HOST_SET_SELECTED], out[M.HOST_PAGE_ROW], out[M.HOST_CELL_SELECTED]), (3, 1, 13))
        # Rows keep the half; sets, sections, pages
        view.select_row(2)
        self.assertEqual(daw.host_pages.screen.page, 5)
        view.select_section(2)
        self.assertEqual(daw.host_pages.pages()[daw.host_pages.screen.page][0], 2)
        view.select_window(0)
        view.select_set(1)
        self.assertEqual(daw.host_pages.screen.page, 1)
        # Turns: the strip's travel and the encoder's detent
        transpose = daw.host_pages.pages()[1][1].controls[1]
        view.turn(5, epoch, protocol.TurnUnit.DETENT, False, 3)
        self.assertEqual(transpose.text, "3 st")
        view.reset(5, epoch)
        self.assertEqual(transpose.text, "0 st")

        # Another chain (another track selected): a new epoch, the old one's commands are dropped
        daw.tracks[1].select()
        out = self.report(view)
        self.assertEqual(out[M.HOST_PAGE_TITLE], "Host · Drums")
        self.assertNotEqual(out[M.HOST_PAGE_EPOCH], epoch)
        view.turn(5, epoch, protocol.TurnUnit.DETENT, False, 3)
        self.assertEqual(view.dropped, 1)

        # A narrow surface: one page a row (grid4x6)
        view.show(6, True, lambda: None)
        out = self.report(view)
        self.assertEqual(out[M.HOST_PAGE_COLUMNS], 4)


class EndToEndTest(unittest.TestCase):
    """The JAM in the core with the simulated host pages."""

    def setUp(self):
        self.daw = daw_with_pages()
        self.bridge = Bridge(self.daw, JAM, tempfile.mkdtemp(prefix="dbm-host-page-"), "Bitwig Studio 5.3")
        self.addCleanup(self.bridge.close)
        self.bridge.start()
        self.bridge.wait_for_startup()
        self.bridge.advance(1100)

    def cc(self, control, value):
        self.bridge.receive(bytes([0xB0, control, value]))

    def pad(self, x, y):
        note = 36 + (7 - y) * 8 + x
        self.bridge.receive(bytes([0x90, note, 127]))
        self.bridge.receive(bytes([0x80, note, 0]))

    def led(self, x, y):
        """The last color of a pad of the grid (x, y from the top left)."""
        note = 36 + (7 - y) * 8 + x
        return next((m[2] for m in reversed(self.bridge.midi_out) if len(m) == 3 and m[0] & 0xF0 == 0x90 and m[1] == note), None)

    def host_commands(self):
        log = [(c.full_command, c.value) for c in self.bridge.commands.log if c.processor == "hostpage"]
        self.bridge.commands.log.clear()
        return log

    def test_shared_selection(self):
        pages = self.daw.host_pages
        self.bridge.commands.log.clear()
        # Select + SONG: no Electra, so the host's pages
        for control, value in ((50, 127), (30, 127), (30, 0), (50, 0)):
            self.cc(control, value)
        self.bridge.advance(200)
        self.assertIn("Surface: Host", self.bridge.notifications)
        self.assertEqual(self.host_commands(), [("show/8/6/8/1", 1)])
        # The screen's page 0 (Mixer) is the active set: its cells at the full level, Osc 1's a
        # level lower (NI's palette: the levels of a hue are consecutive); the selected cell
        # (Level) pale
        mixer, osc = self.led(1, 1), self.led(6, 1)
        self.assertEqual(self.led(0, 1), mixer + 1, "the selected cell")

        # A press on Transpose (cell 5): the host's screen selects Osc 1 and its controller; the
        # JAM shows what the host reports
        self.pad(5, 1)
        self.bridge.advance(100)
        self.assertEqual((pages.screen.page, pages.screen.slot), (1, 1))
        self.assertEqual((self.led(1, 1), self.led(6, 1)), (mixer - 1, osc + 1), "the highlight moved to Osc 1")
        self.assertEqual(self.led(5, 1), osc + 2, "Transpose is the selected cell")

        # The screen changes the selection (zynthian's own encoders): the JAM follows
        pages.select_on_screen(2)
        self.bridge.advance(100)
        self.assertEqual(self.led(6, 1), osc, "Osc 1 no longer active")
        self.assertEqual((self.led(1, 2), self.led(5, 2)), (osc + 1, osc), "Osc 2 (row 2, left) active, Filter not")

        # Strip 1 (row 2: Osc 2's Wave) by its travel; the main encoder turns the selected cell
        wave = pages.pages()[2][1].controls[0]
        self.cc(20, 127)
        for position in (30, 50, 70, 90):
            self.cc(8, position)
            self.bridge.advance(30)
        self.cc(20, 0)
        self.bridge.advance(100)
        self.assertEqual(wave.text, "Noise", wave.text)
        self.cc(57, 127)
        self.bridge.advance(100)
        self.assertEqual(wave.text, "Square")
        commands = self.host_commands()
        self.assertIn(("cell/8/touch/1", 1), commands)
        self.assertTrue(any(c.startswith("cell/8/turn/detent/normal/") and v == -1 for c, v in commands), commands)

        # The next page (PAGE RIGHT): the second window
        self.cc(93, 127)
        self.cc(93, 0)
        self.bridge.advance(100)
        self.assertEqual(pages.screen.page, 12)
        self.assertEqual(self.bridge.core_errors(), [])


if __name__ == "__main__":
    unittest.main()
