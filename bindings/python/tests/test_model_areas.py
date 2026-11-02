# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Tests of the model areas of the reference host which the device, layer, clip and marker
scenarios need: layers and drum pads (dbm_host.layer), the cursor device commands, the remote
controls, the clip and its step grids (dbm_host.clip), the markers (dbm_host.marker), the note
inputs and the playing notes, the panel layout.

Run from the repository root (after 'cargo build')::

    python3 -m unittest discover -s bindings/python/tests
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from dbm_host import Device, Preferences  # noqa: E402
from dbm_host.clip import NOTE_ON, NOTE_SUSTAIN, Clip, Note, NoteGrid, java_double, set_attribute  # noqa: E402
from dbm_host.device import RemoteControl, eq_plus  # noqa: E402
from dbm_host.layer import Layer  # noqa: E402
from dbm_host.model import NoteInput  # noqa: E402
from dbm_host.parameter import Parameter, format_semitones, recorded_formatter  # noqa: E402

from test_host import Host, project  # noqa: E402


def drum_machine(daw) -> Device:
    device = Device("Drum Machine", pages=["Perform", "Default"], cursor_pages=["Perform"])
    pads = {36 + n: Layer(daw.preferences, name, (1, 2, 3), edit=daw.application.edit) for n, name in enumerate(["Kick", "Snare", "HH"])}
    device.make_drum_machine(daw.preferences, pads)
    return device


def layered(daw) -> Device:
    device = Device("Instrument Layer", pages=["Gain"])
    for name in ("Polysynth", "Acido"):
        device.add_layer(Layer(daw.preferences, name, (4, 5, 6)))
    return device


class LayerTest(unittest.TestCase):

    def test_drum_machine_layers_are_its_pads(self):
        daw = project()
        device = drum_machine(daw)
        self.assertTrue(device.has_drum_pads and device.has_layers)
        self.assertEqual([layer.name for layer in device.layer_list()], ["Kick", "Snare", "HH"])
        self.assertIs(device.layer_list()[1], device.drum_pads[37])
        self.assertFalse(device.drum_pads[35].exists)
        device.drum_pads[38].select()
        self.assertTrue(device.layer_list()[2].selected)
        device.layer_list()[0].select()
        self.assertEqual([pad.selected for pad in device.drum_pads[36:39]], [True, False, False], "One selected layer per device")
        device.drum_pads[37].set_mute(True)
        self.assertTrue(device.has_muted_pads and daw.application.can_undo)
        device.clear_muted_pads()
        self.assertFalse(device.has_muted_pads)
        device.drum_pads[10].set_mute(True)
        self.assertFalse(device.drum_pads[10].mute, "A pad without a device ignores the commands")

    def test_reports_and_commands(self):
        daw = project()
        daw.tracks[1].devices.add(drum_machine(daw))
        daw.tracks[2].devices.add(layered(daw))
        host = Host(daw, {"numTracks": 8, "numParamPages": 2, "numDeviceLayers": 8, "deviceIds": ["FIRST_INSTRUMENT"]})
        daw.tracks[1].select()
        report = host.report()
        self.assertEqual((report["/primary/hasDrumPads"], report["/primary/pad/36/name"], report["/primary/pad/36/color"]), ("1", "Kick", "1 2 3"))
        self.assertEqual((report["/primary/pad/35/exists"], report["/primary/pad/35/volume"], report["/primary/pad/35/active"]), ("0", "0.000000", "1"))
        self.assertEqual((report["/primary/layer/count"], report["/primary/layer/2/name"]), ("3", "HH"))
        self.assertEqual((report["/device/layer/count"], report["/device/layer/0/name"]), ("3", "Kick"))
        self.assertEqual((report["/drum/exists"], report["/drum/pad/37/name"]), ("1", "Snare"))
        self.assertEqual((report["/device/page/count"], report["/primary/page/count"]), ("1", "2"), "The cursor device shows only Perform")
        host.commands.handle("primary", "pad/37/mute", 1)
        host.commands.handle("primary", "pad/38/select", 1)
        host.commands.handle("primary", "pad/38/volume", 0.25)
        report = host.report()
        self.assertEqual((report["/primary/pad/37/mute"], report["/primary/hasMutedPads"], report["/primary/pad/38/select"]), ("1", "1", "1"))
        self.assertEqual(report["/device/layer/2/volume"], "0.250000", "The pad is the layer")
        host.commands.handle("drum", "clearMute", None)
        self.assertFalse(daw.tracks[1].devices.devices[0].drum_pads[37].mute)
        daw.tracks[2].select()
        host.commands.handle("device", "layer/1/select", 1)
        host.commands.handle("primary", "layer/0/solo/toggle", None)
        layers = daw.tracks[2].devices.devices[0].layers
        self.assertEqual(([layer.selected for layer in layers], layers[0].solo), ([False, True], True))
        self.assertEqual(host.report()["/primary/hasDrumPads"], "0")
        self.assertEqual(host.commands.unhandled, [])
        host.commands.handle("device", "layer/9/select", 1)
        self.assertEqual(host.commands.unhandled, ["device layer/<n>/select"], "Beyond numDeviceLayers")


class DeviceCommandTest(unittest.TestCase):

    def setUp(self):
        self.daw = project()
        self.chain = self.daw.tracks[0].devices
        for name in ("Polysynth", "Delay", "Reverb"):
            self.chain.add(Device(name, pages=["One"]))
        self.host = Host(self.daw, {"numTracks": 8, "numParamPages": 1})

    def test_select_next_previous_and_siblings(self):
        h, chain = self.host, self.chain
        h.commands.handle("device", "-", None)
        self.assertEqual(chain.selected_index, 0, "Stops at the start of the chain")
        h.commands.handle("device", "+", None)
        h.commands.handle("device", "+", None)
        h.commands.handle("device", "+", None)
        self.assertEqual(chain.selected_index, 2, "Stops at the end")
        h.commands.handle("device", "selected", 2)
        self.assertEqual(self.daw.cursor_device.name, "Delay")
        h.commands.handle("device", "3/bypass", 1)
        self.assertFalse(chain.devices[2].enabled, "The value is the new bypass state")
        h.commands.handle("device", "sibling/3/bypass", None)
        self.assertTrue(chain.devices[2].enabled, "A toggle")
        h.commands.handle("device", "page/+", None)
        self.assertEqual(chain.sibling_offset, 0, "No second page of siblings")
        report = h.report()
        self.assertEqual((report["/device/sibling/3/name"], report["/device/sibling/2/selected"]), ("Reverb", "1"))
        h.commands.handle("device", "2/moveNext", None)
        self.assertEqual([d.name for d in chain.devices], ["Polysynth", "Reverb", "Delay"])
        self.assertEqual(self.daw.cursor_device.name, "Reverb", "The neighbour is selected")
        self.assertEqual(h.commands.unhandled, [])

    def test_flags_and_pin(self):
        h = self.host
        h.commands.handle("device", "expand", 0)
        h.commands.handle("device", "window", 1)
        h.commands.handle("device", "parameters/toggle", None)
        device = self.daw.cursor_device
        self.assertEqual((device.expanded, device.window_open, device.parameters_visible), (False, True, True))
        h.commands.handle("device", "pinned", 1)
        self.daw.tracks[3].select()
        self.assertIs(self.daw.cursor_device, device, "The pinned cursor device stays")
        report = h.report()
        self.assertEqual((report["/device/pinned"], report["/device/parameters"], report["/device/name"]), ("1", "1", "Polysynth"))
        h.commands.handle("device", "pinned", 0)
        self.assertIsNone(self.daw.cursor_device, "Follows the selection again (track EQ has no device here)")


class RemoteControlTest(unittest.TestCase):

    def test_formatters(self):
        semitones = format_semitones(7.0)
        self.assertEqual([semitones(v) for v in (0.0, 0.5, 0.787402, 1.0)], ["-7.00 st", "0.00 st", "+4.02 st", "+7.00 st"])
        uni = recorded_formatter(0.109533, "11 cents")
        self.assertEqual((uni(0.109533), uni(0.5)), ("11 cents", "50.0 %"), "Known only at the recorded value")

    def test_slots_report_and_share_parameters(self):
        daw = project()
        cutoff = Parameter(daw.preferences, 0.7, formatter=recorded_formatter(0.7, "2.59 kHz"), name="Cutoff")
        device = Device("Synth", pages=["A", "B"], remote_controls=[RemoteControl("Cutoff", cutoff), None] + [None] * 6 + [RemoteControl("Freq", cutoff)])
        daw.tracks[0].devices.add(device)
        host = Host(daw, {"numTracks": 8, "numParamPages": 2})
        report = host.report()
        self.assertEqual((report["/device/param/0/name"], report["/device/param/0/value/str"], report["/device/param/0/value/str/6"]), ("Cutoff", "2.59 kHz", "2.59"))
        self.assertEqual((report["/device/param/1/name"], report["/device/param/1/value"]), ("", "0"), "An empty slot")
        host.commands.handle("device", "param/8/value", 0.25)
        self.assertEqual(host.report()["/device/param/0/value"], "0.250000", "The same parameter on page 2")
        host.commands.handle("device", "param/1/value", 0.25)
        self.assertIsNone(device.parameter(1, daw.preferences), "Commands to an empty slot are ignored")

    def test_eq_plus_controls_are_its_bands(self):
        device = eq_plus(Preferences())
        self.assertEqual([device.remote_control(i).name for i in (0, 8, 23)], ["1 Gain", "1 Freq", "8 Q"])
        self.assertIs(device.parameter(9, Preferences()), device.equalizer.frequencies[1])


class ClipGridTest(unittest.TestCase):

    def test_steps(self):
        clip = Clip(Preferences())
        grid = NoteGrid(8, 128, lambda: clip)
        grid.toggle(0, 1, 60, 127)
        self.assertEqual(grid.step(0, 1, 60)[0], NOTE_ON)
        self.assertEqual(grid.steps(), {(0, 1, 60): "1 1 0 0.25 1.0 0.0 1.0 0.0 0.0 0.0 0.0 0.0 1 1.0 1 0 1 1 1 1 0 0.0 0.0 0.0"},
                         "The values of a new note as Bitwig reported them")
        grid.toggle(0, 3, 62, 100)
        self.assertEqual([note.selected for note in clip.notes], [False, True], "Only the new note is selected")
        set_attribute(grid.note(0, 3, 62), "repeatCount", 15)
        set_attribute(grid.note(0, 3, 62), "pan", -2.0)
        self.assertEqual(grid.steps()[(0, 3, 62)].split()[9], "-1.0", "Clamped")
        grid.toggle(0, 1, 60, 127)
        self.assertEqual(list(grid.steps()), [(0, 3, 62)])
        self.assertFalse(clip.notes[0].selected, "Removing a note deselects all")
        grid.set(0, 4, 50, 64, 0.75)
        self.assertEqual((grid.step(0, 5, 50)[0], grid.step(0, 6, 50)[0], grid.step(0, 7, 50)[0]), (NOTE_SUSTAIN, NOTE_SUSTAIN, 0))
        grid.scroll_page(1)
        self.assertEqual(grid.steps(), {}, "The next page is empty")
        grid.scroll_page(-1)
        grid.clear_row(0, 62)
        grid.move_y(0, 4, 50, 52)
        self.assertEqual([(n.key, n.start) for n in clip.notes], [(52, 1.0)])
        self.assertEqual((java_double(0.5039370078740157), java_double(-0.0), java_double(15.0)), ("0.5039370078740157", "0.0", "15.0"))

    def test_cursor_clip_commands_and_reports(self):
        daw = project()
        daw.tracks[0].slots[0].create_clip(4.0)
        host = Host(daw, {"numTracks": 8, "noteClips": [[8, 127], [0, 0]]})
        report = host.report()
        self.assertEqual((report["/clip/exists"], report["/clip/end"], report["/clip/shuffle"], report["/clip/grid/8x127/playingStep"]), ("1", "4.000000", "1", "-1"))
        self.assertEqual(report["/clip/grid/0x0/playingStep"], "-1")
        host.commands.handle("clip", "grid/8x127/step/0/2/36/toggle", 127)
        host.commands.handle("clip", "grid/8x127/step/0/2/36/chance", 0.5)
        self.assertEqual(host.report()["/clip/grid/8x127/step/0/2/36"].split()[13], "0.5")
        host.commands.handle("clip", "grid/8x127/clearAll", None)
        self.assertEqual(host.report(), {"/clip/grid/8x127/step/0/2/36": "0"}, "Reported once as empty")
        host.commands.handle("clip", "loopLength/inc", 4.0)
        self.assertEqual(daw.cursor_clip.loop_length, 8.0)
        host.commands.handle("clip", "note/36/toggle", 127)
        self.assertEqual(host.commands.unhandled, ["clip note/<n>/toggle"], "Reaper's note list, not with a host grid")

    def test_the_cursor_clip_follows_the_track(self):
        daw = project()
        daw.tracks[0].slots[0].create_clip()
        daw.tracks[1].slots[2].create_clip()
        daw.tracks[1].slots[0].select()
        self.assertIs(daw.cursor_clip_slot, daw.tracks[0].slots[0], "The first clip of the cursor track")
        daw.tracks[1].select()
        self.assertIs(daw.cursor_clip_slot, daw.tracks[1].slots[2], "The selected slot has no clip")
        daw.tracks[2].select()
        self.assertIsNone(daw.cursor_clip)


class MarkerTest(unittest.TestCase):

    def test_launch_and_find(self):
        daw = project()
        daw.markers.add("M2", 16.0)
        daw.markers.add("M1", 8.0)
        host = Host(daw, {"numTracks": 8, "numMarkers": 8})
        report = host.report()
        self.assertEqual((report["/marker/0/name"], report["/marker/1/position"], report["/marker/count"]), ("M1", "16.000000", "2"))
        host.commands.handle("marker", "1/launch", None)
        t = daw.transport
        self.assertEqual((t.playing, t.play_start), (True, 16.0))
        t.stop()
        self.assertEqual(t.position, 16.0, "Back to the marker")
        host.commands.handle("marker", "previous", None)
        self.assertEqual(t.play_start, 8.0)
        host.commands.handle("marker", "0/remove", None)
        self.assertEqual([m.name for m in daw.markers.items], ["M2"])

    def test_without_a_marker_bank(self):
        host = Host()
        host.commands.handle("marker", "0/launch", None)
        self.assertEqual(host.commands.unhandled, ["marker <n>/launch"])
        self.assertNotIn("/marker/count", host.report())


class NoteInputTest(unittest.TestCase):

    def test_filters_and_translation(self):
        note_input = NoteInput("x", ["80", "90", "B0"])
        self.assertTrue(note_input.matches(bytes.fromhex("903C64")))
        self.assertFalse(note_input.matches(bytes.fromhex("913C64")))
        note_input.key_table = [-1] * 36 + list(range(36, 128))
        self.assertIsNone(note_input.translate(bytes.fromhex("901040")), "Blocked")
        self.assertEqual(note_input.translate(bytes.fromhex("903C64")), (0x90, 60, 100))
        self.assertIsNone(note_input.translate(bytes.fromhex("B00140")), "Not a note")

    def test_live_notes_play_on_the_monitoring_note_tracks(self):
        daw = project(preferences=Preferences(auto_arm=True))
        daw.tracks[1].set_arm(True)
        daw.play_live_note(0x90, 36, 127)
        self.assertEqual((daw.tracks[0].playing_notes, daw.tracks[1].playing_notes, daw.tracks[3].playing_notes), ({36: 127}, {36: 127}, {}))
        host = Host(daw)
        self.assertEqual(host.report()["/track/0/playingnotes"], "0:0:0:0:0:36:127")
        daw.play_live_note(0x80, 36, 0)
        self.assertEqual(daw.tracks[1].playing_notes, {})


class ApplicationTest(unittest.TestCase):

    def test_layout_and_panels(self):
        host = Host()
        host.commands.handle("application", "layout", 1)
        host.commands.handle("arranger", "toggleCueMarkers", None)
        host.commands.handle("mixer", "toggleMeterSection", None)
        report = host.report()
        self.assertEqual((report["/layout"], report["/arranger/cueMarkers"], report["/mixer/meterSection"]), ("MIX", "0", "1"))
        host.commands.handle("project", "createScene", 4)
        self.assertEqual((len(host.daw.scenes), host.daw.selected_scene.index), (9, 8))

    def test_stop_ends_the_automation_writing(self):
        t = project().transport
        t.set_automation_write_mode(4)
        t.stop()
        self.assertTrue(t.automation_arranger, "Not playing: unchanged")
        t.play()
        t.stop()
        self.assertFalse(t.automation_arranger or t.automation_launcher)

    def test_a_send_is_enabled_by_its_volume(self):
        send = project().tracks[0].sends[0]
        send.volume.set(0.5)
        send.volume.set(0.0)
        self.assertTrue(send.enabled)


if __name__ == "__main__":
    unittest.main()
