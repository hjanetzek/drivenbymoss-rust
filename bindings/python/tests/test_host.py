# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Tests of the reference host (dbm_host): the operations of the simulated DAW, the change-only
reporting, the command handling, and a Launch Control XL running against the simulation.

Run from the repository root (after 'cargo build')::

    python3 -m unittest discover -s bindings/python/tests
"""

import logging
import re
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from dbm_host import TAKEOVER_CATCH, Banks, Bridge, CommandHandler, Device, Parameter, Preferences, Reporter, SimDaw  # noqa: E402
from dbm_host.device import EQ_PLUS, Equalizer, eq_plus, format_eq_frequency, format_eq_gain, format_eq_q  # noqa: E402
from dbm_host.parameter import VOLUME_0DB, format_pan, format_three_digits, format_volume, limit_text  # noqa: E402
from dbm_host.reporter import format_beats, format_time  # noqa: E402
from dbm_host import reporter as reporter_module  # noqa: E402
from dbm_host.coverage import unhandled_commands  # noqa: E402
import drivenbymoss  # noqa: E402
from drivenbymoss import protocol  # noqa: E402

LAUNCH_CONTROL_XL = "5AFA5045-FF92-4737-A091-5B371CD5E529"
TEMPLATE_HEADER = bytes.fromhex("F0002029021177")


def project(**kwargs) -> SimDaw:
    """Five tracks, two effect tracks, eight scenes, the first track selected (as the conformance
    fixture, shorter)."""
    daw = SimDaw(**kwargs).build(
        tracks=[("Synth", "INSTRUMENT"), ("Drums", "INSTRUMENT"), ("Layers", "INSTRUMENT"), ("EQ", "AUDIO"), ("Audio 5", "AUDIO")],
        effect_tracks=["FX 1", "FX 2"],
        scenes=8,
    )
    daw.tracks[0].select()
    return daw


class Host:
    """A SimDaw with a Reporter and a CommandHandler, without the core."""

    def __init__(self, daw: SimDaw | None = None, setup: dict | None = None):
        self.daw = daw or project()
        self.clock = 0.0
        self.banks = Banks.from_model_setup(setup or {"numTracks": 8, "numSends": 2, "numScenes": 8, "hasFullFlatTrackList": True})
        self.reporter = Reporter(self.daw, self.banks, clock=lambda: self.clock)
        self.commands = CommandHandler(self.daw, self.banks, self.reporter, clock=lambda: self.clock)
        self.commands.configure()
        self.reporter.features = self.commands.host_features

    def report(self) -> dict[str, str]:
        lines = self.reporter.collect()
        return dict(line.split(" ", 1) for line in lines.splitlines())

    def lines(self) -> list[str]:
        return self.reporter.collect().splitlines()


class ParameterTest(unittest.TestCase):

    def test_set_inc_reset(self):
        p = Parameter(Preferences(), VOLUME_0DB, formatter=format_volume)
        self.assertTrue(p.set(0.25))
        self.assertEqual(p.value, 0.25)
        p.inc(0.1)
        self.assertAlmostEqual(p.value, 0.35)
        p.inc(5)
        self.assertEqual(p.value, 1.0)
        p.reset()
        self.assertEqual(p.value, VOLUME_0DB)
        p.set_immediately(-1)
        self.assertEqual(p.value, 0.0)
        p.touch(True)
        p.set_indication(True)
        self.assertTrue(p.touched and p.indicated)

    def test_takeover_catch(self):
        p = Parameter(Preferences(takeover_mode=TAKEOVER_CATCH), 0.5)
        self.assertFalse(p.set(0.9), "A jump is not taken")
        self.assertEqual(p.value, 0.5)
        self.assertFalse(p.set(0.7))
        self.assertTrue(p.set(0.45), "The control crossed the value")
        self.assertEqual(p.value, 0.45)
        self.assertTrue(p.set(0.44), "Close to the value")
        p.set_immediately(0.9)
        self.assertEqual(p.value, 0.9)

    def test_texts(self):
        # As Bitwig reported them (tests/conformance/*/replay/*.rec)
        self.assertEqual(format_volume(0.540742), "-10.0 dB")
        self.assertEqual(format_volume(0.793701), "0.0 dB")
        self.assertEqual(format_volume(0), "-Inf dB")
        self.assertEqual(format_pan(0.5), "0.00 %")
        self.assertEqual(format_pan(0.0), "-100 %")
        self.assertEqual(format_pan(0.503937), "0.79 %")
        self.assertEqual(format_pan(0.781298), "56.3 %")
        self.assertEqual(format_pan(0.4999999), "0.00 %", "Never -0.00 %")
        self.assertEqual(limit_text("-100 %", 6), "-100")
        self.assertEqual(limit_text("56.3 %", 8), "56.3 %")
        self.assertEqual([format_three_digits(n) for n in (0.789, 9.996, 56.26, 99.96, 159.68, -100.0)],
                         ["0.79", "10.0", "56.3", "100", "160", "-100"], "The decimals of the rounded number")
        self.assertEqual(limit_text("-10.0 dB", 6), "-10.0")
        self.assertEqual(limit_text("-10.0 dB", 8), "-10.0 dB")
        self.assertEqual(limit_text("0.0 dB", 6), "0.0")
        self.assertEqual(limit_text("-Inf dB", 6), "-Inf d")
        self.assertEqual(limit_text("0.00 %", 6), "0.00")
        self.assertEqual(limit_text("100 %", 8), "100 %")
        self.assertEqual(format_beats(0, 4, 1), "1.01.01:00")
        self.assertEqual(format_beats(8, 4, 0), "2.00.00:00")
        self.assertEqual(format_beats(5.375, 4, 1), "2.02.02:50")
        self.assertEqual(format_time(120, 0), "0.00.00:000")
        self.assertEqual(format_time(120, 150), "0.01.15:000")


class TransportTest(unittest.TestCase):

    def test_play_stop_position(self):
        t = SimDaw().transport
        t.set_position(4)
        self.assertEqual((t.play_start, t.position), (4, 0), "Only the play start while stopped")
        t.play()
        self.assertTrue(t.playing)
        self.assertEqual(t.position, 4)
        t.advance(1000)  # 2 beats at 120 BPM
        self.assertAlmostEqual(t.position, 6)
        t.set_position(1)
        self.assertEqual(t.position, 1, "Jumps while playing")
        t.stop()
        self.assertFalse(t.playing)
        self.assertEqual(t.position, 1)
        t.restart()
        self.assertTrue(t.playing)
        t.stop_and_rewind()
        self.assertEqual((t.playing, t.position, t.play_start), (False, 0, 0))
        t.toggle_play()
        self.assertTrue(t.playing)
        t.toggle_play()
        self.assertFalse(t.playing)

    def test_loop_and_bars(self):
        t = SimDaw().transport
        t.loop = True
        t.loop_start, t.loop_length = 0.0, 8.0
        t.play()
        self.assertEqual(t.advance(1999), [])
        self.assertEqual(t.advance(2), [4.0])
        t.advance(2000)
        self.assertAlmostEqual(t.position, 0.002, places=5, msg="Wrapped at the loop end")

    def test_tempo_signature_automation(self):
        t = SimDaw().transport
        t.change_tempo(1)
        self.assertEqual(t.tempo, 121)
        t.set_tempo(1000)
        self.assertEqual(t.tempo, 666)
        t.set_time_signature(3, 4)
        self.assertEqual(t.quarters_per_measure, 3)
        t.set_time_signature(0, 4)
        self.assertEqual(t.numerator, 3)
        t.set_automation_write_mode(2)
        self.assertEqual((t.automation_arranger, t.automation_launcher, t.automation_mode), (True, True, "touch"))
        t.set_automation_write_mode(1)
        self.assertFalse(t.automation_arranger or t.automation_launcher)

    def test_tap_tempo(self):
        t = SimDaw().transport
        for ms in (0, 500, 1000, 1500):
            t.tap_tempo(ms)
        self.assertAlmostEqual(t.tempo, 120)


class TrackTest(unittest.TestCase):

    def test_sends_and_lists(self):
        daw = project()
        self.assertEqual([s.name for s in daw.tracks[0].sends], ["FX 1", "FX 2"])
        self.assertEqual([s.name for s in daw.effect_tracks[0].sends], ["FX 1", "FX 2"])
        self.assertEqual(daw.master.sends, [])
        fx = daw.add_effect_track("FX 3")
        self.assertEqual(daw.tracks[4].sends[2].target, fx)
        self.assertEqual(len(daw.track_list(False)), 5)
        self.assertEqual([c.type for c in daw.track_list(True)][5:], ["EFFECT"] * 3 + ["MASTER"])
        late = daw.add_track("AUDIO", "Late")
        self.assertEqual(len(late.sends), 3)
        self.assertEqual(len(late.slots), 8)

    def test_selection_and_solo(self):
        daw = project()
        self.assertTrue(daw.tracks[0].selected)
        daw.tracks[2].select()
        self.assertFalse(daw.tracks[0].selected)
        daw.master.select()
        self.assertTrue(daw.master.selected)
        self.assertFalse(daw.tracks[2].selected)

        daw.tracks[0].toggle_solo()
        daw.tracks[1].toggle_solo()
        self.assertTrue(daw.tracks[0].solo and daw.tracks[1].solo, "Not exclusive by default")
        daw.preferences.exclusive_solo = True
        daw.tracks[2].toggle_solo()
        self.assertEqual([t.solo for t in daw.tracks[:3]], [False, False, True])
        daw.tracks[2].toggle_solo()
        self.assertFalse(daw.tracks[2].solo)

    def test_monitor_and_capabilities(self):
        daw = project()
        synth, eq = daw.tracks[0], daw.tracks[3]
        self.assertFalse(synth.monitoring)
        synth.arm = True
        self.assertTrue(synth.monitoring, "AUTO monitors while armed")
        synth.monitor_mode = "OFF"
        self.assertFalse(synth.monitoring)
        self.assertEqual((synth.can_hold_notes, synth.can_hold_audio), (True, False))
        self.assertEqual((eq.can_hold_notes, eq.can_hold_audio), (False, True))

    def test_remove_duplicate_move(self):
        daw = project()
        drums = daw.tracks[1]
        drums.mute = True
        drums.volume.value = 0.3
        copy = daw.duplicate_track(drums)
        self.assertIs(daw.tracks[2], copy)
        self.assertEqual((copy.name, copy.mute, copy.volume.value), ("Drums", True, 0.3))
        daw.move_track(copy, -1)
        self.assertIs(daw.tracks[1], copy)
        self.assertTrue(copy.selected)
        daw.remove_track(copy)
        self.assertIsNone(daw.selected_channel)
        self.assertEqual(len(daw.tracks), 5)
        fx_copy = daw.duplicate_track(daw.effect_tracks[0])
        self.assertEqual([s.target for s in daw.tracks[0].sends], daw.effect_tracks)
        daw.remove_track(fx_copy)
        self.assertEqual([s.name for s in daw.tracks[0].sends], ["FX 1", "FX 2"])

    def test_cursor_pinning(self):
        daw = project()
        daw.set_cursor_pinned(True)
        daw.tracks[3].select()
        self.assertIs(daw.cursor_channel, daw.tracks[0])
        daw.select_neighbour(daw.tracks, 1)
        self.assertIs(daw.cursor_channel, daw.tracks[1], "Moves the pinned cursor")
        self.assertTrue(daw.tracks[3].selected)
        daw.set_cursor_pinned(False)
        self.assertIs(daw.cursor_channel, daw.tracks[3])
        daw.select_neighbour(daw.tracks, -1)
        self.assertTrue(daw.tracks[2].selected)


class ClipLauncherTest(unittest.TestCase):

    def test_launch_while_stopped_and_playing(self):
        daw = project()
        synth = daw.tracks[0]
        synth.slots[0].create_clip()
        synth.slots[1].create_clip()
        self.assertEqual(synth.slots[0].color, synth.color)
        synth.slots[0].launch()
        self.assertTrue(daw.transport.playing, "Launching starts the transport")
        self.assertTrue(synth.slots[0].playing)
        self.assertFalse(synth.is_stopped)
        synth.slots[1].launch()
        self.assertTrue(synth.slots[1].play_queued)
        daw.advance(1000)
        self.assertTrue(synth.slots[0].playing, "Quantized to the next bar")
        daw.advance(1000)
        self.assertEqual((synth.slots[0].playing, synth.slots[1].playing, synth.slots[1].play_queued), (False, True, False))
        synth.stop()
        self.assertTrue(synth.slots[1].stop_queued)
        daw.advance(2000)
        self.assertTrue(synth.is_stopped)

    def test_empty_slots_and_scenes(self):
        daw = project()
        drums = daw.tracks[1]
        drums.arm = True
        drums.slots[2].launch()
        self.assertTrue(drums.slots[2].recording and drums.slots[2].has_content, "Records into an empty slot")
        daw.transport.stop()
        daw.tracks[0].slots[3].create_clip()
        daw.scenes[3].launch()
        self.assertTrue(daw.transport.playing)
        self.assertTrue(daw.tracks[0].slots[3].playing)
        self.assertTrue(drums.slots[3].recording, "The armed track records")
        daw.stop_all_clips()
        daw.advance(2000)
        self.assertTrue(all(track.is_stopped for track in daw.tracks))
        copy = daw.duplicate_scene(daw.scenes[3])
        self.assertTrue(daw.tracks[0].slots[copy.index].has_content)
        daw.remove_scene(copy)
        self.assertEqual(len(daw.scenes), 8)
        self.assertEqual(len(daw.tracks[0].slots), 8)
        daw.tracks[0].slots[3].select()
        self.assertTrue(daw.tracks[0].slots[3].selected)
        daw.tracks[1].slots[0].select()
        self.assertFalse(daw.tracks[0].slots[3].selected)


COVERAGE_SETUP = {"numTracks": 8, "numSends": 2, "numScenes": 8, "hasFullFlatTrackList": True, "noteClips": [[16, 8]], "numMarkers": 8,
                  "deviceIds": ["FIRST_INSTRUMENT", "EQ"], "wantsClipLauncherNavigator": True}


def coverage_host() -> "Host":
    """The host of the coverage test: COVERAGE_SETUP, a device on the cursor track."""
    daw = project()
    daw.tracks[0].devices.add(Device("Polysynth", pages=["OSC1", "OSC2"]))
    return Host(daw, COVERAGE_SETUP)


class CommandCoverageTest(unittest.TestCase):

    def test_handled_commands(self):
        """Every command ID is handled, typed or through its text, or in UNHANDLED_COMMANDS (the
        set passed to the core, App.use_binary_commands)."""
        logging.disable(logging.CRITICAL)
        self.addCleanup(logging.disable, logging.NOTSET)
        unhandled = unhandled_commands(lambda: coverage_host().commands)
        declared = CommandHandler.UNHANDLED_COMMANDS
        self.assertEqual(sorted(protocol.Cmd(i).name for i in unhandled - declared), [], "Not handled, not declared")
        self.assertEqual(sorted(protocol.Cmd(i).name for i in declared - unhandled), [], "Declared unhandled, but handled")
        self.assertEqual(Host().commands.handled_commands(), set(protocol.CMD_IDS) - declared)

    def test_typed_and_text_dispatch(self):
        """The typed handlers (channel values, mute, send volume, parameters) and the text
        fallback give the same model and the same log as the text form."""
        hosts = [Host(), Host()]
        lines = ["track\t0/volume\t0.25", "track\t1/mute\t1", "track\t0/send/1/volume/inc\t0.125", "master\tvolume/reset",
                 "fxtrack\t1/solo\t1", "device\tparam/2/value\t0.75", "track\t2/volume/touch\t1", "transport\tplay"]
        for line in lines:
            processor, command, *value = line.split("\t")
            parsed = (float(value[0]) if "." in value[0] else int(value[0])) if value else None
            hosts[0].commands.handle(processor, command, parsed)
        kinds = {int: "int", float: "double", type(None): "noArg"}
        text = "\n".join(kinds[type(c.value)] + "\t" + c.processor + "\t" + c.full_command + ("" if c.value is None else "\t" + str(c.value))
                         for c in hosts[0].commands.log)
        buffer = drivenbymoss.messages_from_text(text)
        hosts[1].commands.process_commands(buffer)
        typed = [type(c).__name__ for c in hosts[1].commands.log]
        self.assertEqual(typed, ["ViewCommand"] * 7 + ["Command"], "The transport through its text")
        for a, b in zip(hosts[0].commands.log, hosts[1].commands.log):
            self.assertEqual((a.processor, a.full_command, a.command, a.number, a.reset, a.increment), (b.processor, b.full_command, b.command, b.number, b.reset, b.increment))
        self.assertEqual(hosts[1].daw.tracks[0].volume.value, 0.25)
        self.assertEqual(hosts[0].reporter.collect(), hosts[1].reporter.collect())
        self.assertEqual(hosts[0].reporter.pending, hosts[1].reporter.pending, "Reported again")


class ReporterTest(unittest.TestCase):

    def test_binary_reports_equal_the_text(self):
        """The dual run (TYPED_PROTOCOL_PLAN §3): the records of the binary protocol equal the
        core's conversion of the text lines (the check protocol raises otherwise), and both give
        the same text."""
        hosts = {}
        for name in (reporter_module.TEXT, reporter_module.CHECK):
            host = Host()
            host.reporter = Reporter(host.daw, host.banks, clock=lambda: 0.0, features=host.commands.host_features, protocol_=name)
            host.commands.reporter = host.reporter
            hosts[name] = host
        for step in range(3):
            texts = {}
            for name, host in hosts.items():
                if step == 1:
                    host.daw.tracks[2].volume.set(0.123456789)
                    host.daw.tracks[1].name = "Line\nbreak"
                if step == 2:
                    host.reporter.request_dump()
                texts[name] = host.lines()
            self.assertEqual(texts[reporter_module.CHECK], texts[reporter_module.TEXT])
        check = hosts[reporter_module.CHECK].reporter
        self.assertEqual([a for a in check.last_values if reporter_module.address_key(a) is None], [], "Every address is in the table")
        self.assertIsNone(check.take_buffer()[0], "Taken by lines()")
        self.assertEqual(check.last_values["/track/2/volume"], "0.123457")
        k = protocol.key(protocol.Msg.CHANNEL_VOLUME, protocol.ChKind.TRACK, 2)
        self.assertEqual(reporter_module.address_key("/track/2/volume"), k)
        self.assertEqual(reporter_module.key_address(k), "/track/2/volume")

    def test_dump_then_only_changes(self):
        host = Host()
        first = host.lines()
        self.assertEqual(first[0], "/host/features valueInc valueReset immediately indication soloToggle transportCommands deviceCommands")
        values = dict(line.split(" ", 1) for line in first)
        addresses = [line.split(" ", 1)[0] for line in first]
        twice = sorted({address for address in addresses if addresses.count(address) > 1})
        self.assertEqual(twice, ["/device/bypass", "/device/exists", "/device/expand", "/device/name", "/device/window"],
                         "Every address once, except the cursor device's (collectDevice and its layers, as BitwigModel)")
        self.assertEqual(values["/track/count"], "8")
        self.assertEqual(values["/track/7/type"], "MASTER")
        self.assertEqual(values["/track/5/type"], "EFFECT")
        self.assertEqual(values["/track/0/volume"], "0.793701")
        self.assertEqual(values["/track/0/send/1/name"], "FX 2")
        self.assertEqual(values["/fxtrack/count"], "2")
        self.assertEqual(values["/scene/count"], "8")
        self.assertEqual(values["/track/4/slot/count"], "8")
        self.assertEqual(values["/master/name"], "Master")
        self.assertEqual(values["/beats/text"], "1.01.01:00")
        self.assertEqual(values["/track/0/select"], "1")
        self.assertLess(first.index("/track/count 8"), first.index("/track/0/select 1"), "The selection after the count")

        self.assertEqual(host.lines(), [], "Nothing changed")

        host.daw.tracks[2].volume.set(0.540742)
        self.assertEqual(host.lines(), ["/track/2/volume 0.540742", "/track/2/volume/str -10.0 dB", "/track/2/volume/str/6 -10.0",
                                        "/track/2/volume/str/8 -10.0 dB"])

        host.reporter.request_dump()
        dump = host.lines()
        self.assertEqual(len(dump), len(first), "A dump is complete")
        self.assertEqual(set(line.split(" ", 1)[0] for line in dump), set(values))
        self.assertEqual(host.lines(), [])

    def test_selection_change(self):
        host = Host()
        host.lines()
        host.daw.tracks[3].select()
        self.assertEqual(host.lines(), ["/track/hasPrevious 1", "/track/0/select 0", "/track/3/select 1", "/track/3/playingnotes "],
                         "The notes of the new cursor track are reported again")
        host.daw.master.select()
        lines = host.lines()
        self.assertEqual(lines, ["/track/hasNext 0", "/track/3/select 0", "/track/7/select 1", "/track/7/playingnotes ", "/master/select 1"])

    def test_removed_selected_track_is_deselected(self):
        host = Host(setup={"numSends": 2})
        host.lines()
        host.daw.tracks[4].select()
        host.lines()
        host.daw.remove_track(host.daw.tracks[4])
        report = host.report()
        self.assertEqual(report["/track/4/select"], "0")
        self.assertEqual(report["/track/count"], "4")

    def test_report_again_after_a_value_command(self):
        host = Host()
        host.lines()
        host.commands.handle("track", "1/volume", 0.5)
        self.assertEqual(host.report()["/track/1/volume"], "0.500000")
        host.clock = 299
        self.assertEqual(host.lines(), [])
        host.clock = 300
        report = host.report()
        self.assertEqual(report["/track/1/volume"], "0.500000")
        self.assertIn("/track/1/name", report)
        self.assertNotIn("/track/2/name", report)
        self.assertEqual(host.lines(), [])

    def test_sends_limited_by_the_model_setup(self):
        host = Host(setup={"numSends": 1, "hasFullFlatTrackList": False})
        report = host.report()
        self.assertEqual(report["/track/0/send/count"], "1")
        self.assertNotIn("/track/0/send/1/name", report)
        self.assertEqual(report["/track/count"], "5")


class CommandTest(unittest.TestCase):

    def test_value_commands(self):
        host = Host()
        daw, handle = host.daw, host.commands.handle
        handle("track", "0/volume", 0.25)
        self.assertEqual(daw.tracks[0].volume.value, 0.25)
        handle("track", "0/volume/inc", 0.1)
        self.assertAlmostEqual(daw.tracks[0].volume.value, 0.35)
        handle("track", "0/volume/reset", None)
        self.assertEqual(daw.tracks[0].volume.value, VOLUME_0DB)
        handle("track", "1/pan/immediately", 0.0)
        self.assertEqual(daw.tracks[1].pan.value, 0.0)
        handle("track", "1/pan/touch", 1)
        handle("track", "1/volume/indicate", 1)
        self.assertTrue(daw.tracks[1].pan.touched and daw.tracks[1].volume.indicated)
        handle("track", "2/send/1/volume", 0.75)
        self.assertEqual(daw.tracks[2].sends[1].volume.value, 0.75)
        handle("track", "2/send/1/volume/indicate", 1)
        self.assertTrue(daw.tracks[2].sends[1].volume.indicated)
        handle("track", "2/send/0/active", 0.4)
        handle("track", "2/send/0/active", 0.4)
        handle("track", "2/send/0/active", 0.4)
        self.assertTrue(daw.tracks[2].sends[0].enabled, "A toggle")
        handle("master", "volume", 0.1)
        self.assertEqual(daw.master.volume.value, 0.1)
        handle("track", "7/volume", 0.2)
        self.assertEqual(daw.master.volume.value, 0.2, "The master in the full flat list")
        handle("fxtrack", "1/volume", 0.3)
        self.assertEqual(daw.effect_tracks[1].volume.value, 0.3)
        handle("track", "6/send/0/volume", 0.6)
        self.assertEqual(daw.effect_tracks[1].sends[0].volume.value, 0.6)
        self.assertEqual(host.commands.unhandled, [])

    def test_channel_commands(self):
        host = Host()
        daw, handle = host.daw, host.commands.handle
        handle("track", "1/mute", 1)
        handle("track", "2/solo", 1)
        handle("track", "3/recarm", 1)
        handle("track", "4/active", 0)
        handle("track", "0/monitor", 1)
        handle("track", "1/autoMonitor", 0)
        handle("track", "2/select", 1)
        handle("track", "3/color", "RGB(10,20,30)")
        handle("track", "3/name", "Renamed")
        self.assertTrue(daw.tracks[1].mute and daw.tracks[2].solo and daw.tracks[3].arm)
        self.assertFalse(daw.tracks[4].activated)
        self.assertEqual((daw.tracks[0].monitor_mode, daw.tracks[1].monitor_mode), ("ON", "OFF"))
        self.assertTrue(daw.tracks[2].selected)
        self.assertEqual((daw.tracks[3].color, daw.tracks[3].name), ((10, 20, 30), "Renamed"))
        daw.preferences.exclusive_solo = True
        handle("track", "0/solo/toggle", None)
        self.assertEqual([t.solo for t in daw.tracks[:3]], [True, False, False])
        handle("master", "solo/toggle", None)
        self.assertTrue(daw.master.solo)
        self.assertFalse(daw.tracks[0].solo)
        handle("fxtrack", "0/select", 1)
        self.assertTrue(daw.effect_tracks[0].selected)
        handle("track", "pinned", 1)
        self.assertTrue(daw.cursor.pinned)
        handle("track", "pinned/toggle", None)
        self.assertFalse(daw.cursor.pinned)
        handle("track", "0/select", 1)
        handle("track", "selectNext", None)
        self.assertTrue(daw.tracks[1].selected)
        handle("track", "addTrack", ["INSTRUMENT", "New", "RGB(1,2,3)"])
        self.assertEqual((daw.tracks[-1].name, daw.tracks[-1].type, daw.tracks[-1].color), ("New", "INSTRUMENT", (1, 2, 3)))
        self.assertTrue(daw.tracks[-1].selected)
        handle("track", "0/movePrev", None)
        handle("track", "0/moveNext", None)
        self.assertEqual(daw.tracks[1].name, "Synth")
        handle("track", "indicate", 1)
        self.assertTrue(host.banks.show_track_clip_launcher_feedback)
        self.assertEqual(host.commands.unhandled, [])

    def test_slot_and_scene_commands(self):
        host = Host()
        daw, handle = host.daw, host.commands.handle
        handle("track", "0/createClip/2", 4)
        self.assertTrue(daw.tracks[0].slots[2].has_content)
        handle("track", "0/clip/2/select", None)
        self.assertTrue(daw.tracks[0].slots[2].selected)
        handle("track", "0/clip/2/launch", None)
        self.assertTrue(daw.tracks[0].slots[2].playing)
        handle("track", "0/clip/2/launchRelease", None)
        handle("track", "0/clip/2/color", "RGB(255,0,0)")
        self.assertEqual(daw.tracks[0].slots[2].color, (255, 0, 0))
        handle("track", "1/clip/2/paste", "0/2")
        self.assertTrue(daw.tracks[1].slots[2].has_content)
        handle("track", "0/stop", None)
        self.assertTrue(daw.tracks[0].slots[2].stop_queued)
        handle("track", "7/createClip/0", 4)
        handle("scene", "2/select", None)
        self.assertTrue(daw.scenes[2].selected)
        handle("scene", "2/name", "Verse")
        handle("scene", "2/color", "RGB(1,1,1)")
        self.assertEqual((daw.scenes[2].name, daw.scenes[2].color), ("Verse", (1, 1, 1)))
        handle("scene", "2/duplicate", None)
        self.assertEqual(daw.scenes[3].name, "Verse")
        handle("scene", "3/remove", None)
        handle("scene", "stop", None)
        handle("track", "0/clip/2/remove", None)
        self.assertFalse(daw.tracks[0].slots[2].has_content)
        self.assertEqual(host.commands.unhandled, [])

    def test_transport_commands(self):
        host = Host()
        t, handle = host.daw.transport, host.commands.handle
        handle("transport", "play", None)
        self.assertTrue(t.playing)
        handle("transport", "stop", None)
        handle("play", "", None)
        self.assertTrue(t.playing)
        handle("stop", "", None)
        self.assertFalse(t.playing)
        handle("record", "", None)
        handle("repeat", "", 1)
        self.assertTrue(t.recording and t.loop)
        handle("tempo", "", 98.5)
        handle("tempo", "+", None)
        self.assertAlmostEqual(t.tempo, 98.51)
        handle("tempo", "--", None)
        self.assertAlmostEqual(t.tempo, 97.51)
        handle("time", "signature", 403)
        self.assertEqual((t.numerator, t.denominator), (3, 4))
        handle("beats", "position", 6.0)
        self.assertEqual(t.play_start, 6)
        handle("time", "", 0)
        self.assertEqual(t.play_start, 0)
        handle("beats", "loop/start", 4.0)
        handle("beats", "loop/length", 12.0)
        self.assertEqual((t.loop_start, t.loop_length), (4, 12))
        for command, attribute in (("toggleMetronome", "metronome"), ("togglePunchIn", "punch_in"), ("togglePunchOut", "punch_out"),
                                   ("toggleFill", "fill"), ("toggleMetronomeTicks", "metronome_ticks")):
            handle("transport", command, None)
            self.assertTrue(getattr(t, attribute), command)
        handle("transport", "overdub/launcher", 1)
        handle("transport", "metronomeVolume/inc", -0.25)
        self.assertTrue(t.overdub_launcher)
        self.assertAlmostEqual(t.metronome_volume.value, 0.5)
        handle("automation", "", 3)
        self.assertEqual((t.automation_arranger, t.automation_mode), (True, "write"))
        handle("automation", "toggleLauncher", None)
        self.assertFalse(t.automation_launcher)
        handle("inifile", "reaper/prerollmeas", 2)
        self.assertEqual(t.preroll, "two_bars")
        handle("undo", "", None)
        handle("redo", "", None)
        self.assertEqual((host.daw.application.undos, host.daw.application.redos), (1, 1))
        handle("transport", "restart", None)
        handle("transport", "stopAndRewind", None)
        self.assertEqual((t.playing, t.position), (False, 0))
        self.assertEqual(host.commands.unhandled, [])

    def test_refresh_bank_and_unhandled(self):
        host = Host()
        host.lines()
        host.commands.handle("refresh", "", None)
        self.assertGreater(len(host.lines()), 100)
        host.commands.handle("bank", "track", 8)
        host.commands.handle("bank", "scene", 4)
        self.assertEqual((host.banks.track_offset, host.banks.scene_offset), (8, 4))
        self.assertEqual(host.report()["/track/offset"], "8")

        with self.assertLogs("dbm_host", "INFO") as logs:
            host.commands.handle("device", "sibling/3/frobnicate", 1)
            host.commands.handle("device", "sibling/4/frobnicate", 1)
            host.commands.handle("track", "99/mute", 1)
            host.commands.handle("frobnicate", "", None)
        self.assertEqual(host.commands.unhandled, ["device sibling/<n>/frobnicate", "track <n>/mute", "frobnicate"])
        self.assertEqual(len(logs.output), 3, "Each once")
        self.assertIn("Not handled yet: device sibling/<n>/frobnicate", logs.output[0])


class DeviceAndPreferencesTest(unittest.TestCase):

    def test_auto_arm_follows_the_selection(self):
        daw = project(preferences=Preferences(auto_arm=True))
        synth, drums, eq = daw.tracks[0], daw.tracks[1], daw.tracks[3]
        drums.select()
        self.assertEqual((synth.arm, drums.arm), (False, True), "Only instrument tracks, the previous one is disarmed")
        eq.select()
        self.assertEqual((drums.arm, eq.arm), (True, False), "An audio track: the instrument track stays armed")
        daw.effect_tracks[0].select()
        daw.master.select()
        self.assertTrue(drums.arm, "Also for an effect track and the master")
        eq.set_arm(True)
        synth.select()
        self.assertEqual((synth.arm, drums.arm), (True, False), "The next instrument track takes the arm")
        self.assertTrue(eq.arm, "Armed by a command: stays armed")

    def test_edits_make_undo_possible(self):
        host = Host()
        app = host.daw.application
        host.commands.handle("track", "0/select", 1.0)
        self.assertFalse(app.can_undo, "A selection is no edit")
        host.commands.handle("track", "0/mute", 0.0)
        self.assertFalse(app.can_undo, "No change")
        host.commands.handle("track", "0/mute", 1.0)
        self.assertTrue(app.can_undo)
        self.assertIn("/project/canUndo 1", host.lines())
        host.commands.handle("track", "1/volume", 0.25)
        host.commands.handle("track", "1/volume", 0.3)
        self.assertEqual(len(app.undo_stack), 2, "The changes of one value are one edit")
        track0, track1 = host.daw.tracks[0], host.daw.tracks[1]
        host.commands.handle("undo", "", None)
        self.assertEqual(track1.volume.value, track1.volume.default)
        self.assertTrue(track0.mute)
        host.commands.handle("undo", "", None)
        self.assertFalse(track0.mute)
        self.assertEqual((app.can_undo, app.can_redo), (False, True))
        self.assertIn("/project/canUndo 0", host.lines())
        host.commands.handle("redo", "", None)
        self.assertTrue(track0.mute)
        host.commands.handle("track", "0/solo", 1.0)
        self.assertFalse(app.can_redo, "An edit clears the redo steps")

    def test_device_reports_and_parameter_page(self):
        daw = project()
        daw.tracks[0].devices.add(Device("Polysynth", pages=["OSC1", "OSC2", "MIX"]))
        daw.tracks[0].devices.add(Device("Delay", kind="audio_fx"))
        host = Host(daw, {"numTracks": 8, "numParamPages": 2, "deviceIds": ["FIRST_INSTRUMENT"]})
        report = host.report()
        self.assertEqual((report["/device/exists"], report["/device/name"], report["/device/count"]), ("1", "Polysynth", "2"))
        self.assertEqual((report["/device/sibling/2/name"], report["/device/sibling/1/selected"]), ("Delay", "1"))
        self.assertEqual((report["/device/page/count"], report["/device/page/1/name"], report["/device/param/count"]), ("2", "OSC2", "16"))
        self.assertEqual(report["/primary/name"], "Polysynth")
        self.assertNotIn("/device/page/2/name", report, "Only numParamPages pages")
        host.commands.handle("device", "parameterPage", 7.0)
        self.assertEqual(daw.cursor_device.selected_page, 2, "Clamped")
        self.assertEqual(host.report()["/primary/page/selected"], "2", "The same device")
        host.commands.handle("primary", "parameterPage", 0.0)
        self.assertEqual(host.report()["/device/page/selected"], "0")
        daw.tracks[3].select()
        report = host.report()
        self.assertEqual((report["/device/exists"], report["/device/position"], report["/primary/exists"]), ("0", "-1", "0"))
        self.assertEqual(host.commands.unhandled, [])


class NoteRepeatTest(unittest.TestCase):

    def test_the_arpeggiator_of_the_note_input(self):
        host = Host()
        self.assertNotIn("/noterepeat/active", host.report(), "No note input, nothing reported")
        host.commands.handle("noterepeat", "active", 1)
        self.assertEqual(host.commands.unhandled, [], "Ignored without a note input, as the Bitwig extension")

        host.daw.create_note_input()
        report = host.report()
        self.assertEqual({k: v for k, v in report.items() if k.startswith("/noterepeat/")},
                         {"/noterepeat/active": "0", "/noterepeat/period": "4.000000", "/noterepeat/notelength": "0.500000",
                          "/noterepeat/mode": "1", "/noterepeat/velocity": "1"}, "Bitwig's defaults of the recordings")
        host.commands.handle("noterepeat", "rate", 8.0)
        host.commands.handle("noterepeat", "notelength", 0.25)
        host.commands.handle("noterepeat", "mode", 3)
        host.commands.handle("noterepeat", "mode", 9)
        host.commands.handle("noterepeat", "velocity", 0)
        host.commands.handle("noterepeat", "active", 1)
        arpeggiator = host.daw.note_input.arpeggiator
        self.assertEqual((arpeggiator.rate, arpeggiator.mode, arpeggiator.use_pressure_to_velocity), (0.125, "up-down", False), "Mode 9 is ignored")
        report = host.report()
        self.assertEqual((report["/noterepeat/period"], report["/noterepeat/notelength"], report["/noterepeat/mode"]), ("8.000000", "0.250000", "3"))
        self.assertEqual((report["/noterepeat/active"], report["/noterepeat/velocity"]), ("1", "0"))
        host.commands.handle("noterepeat", "rate", 0.0)
        self.assertEqual(arpeggiator.rate, 0.125, "A rate of 0 is ignored")
        host.commands.handle("noterepeat", "shuffle", 1)
        self.assertEqual(host.commands.unhandled, ["noterepeat shuffle"])

    def test_a_new_note_input_has_the_defaults(self):
        daw = project()
        daw.create_note_input().arpeggiator.enabled = True
        self.assertFalse(daw.create_note_input().arpeggiator.enabled)


class DeviceParameterTest(unittest.TestCase):

    def test_parameter_commands(self):
        daw = project()
        daw.tracks[0].devices.add(Device("Polysynth", pages=["OSC1", "OSC2"]))
        host = Host(daw, {"numTracks": 8, "numParamPages": 4})
        host.report()
        host.commands.handle("device", "param/9/indicate", 1)
        host.commands.handle("device", "param/9/touch", 1)
        host.commands.handle("device", "param/9/value", 0.25)
        parameter = daw.cursor_device.parameters[9]
        self.assertEqual((parameter.indicated, parameter.touched, parameter.value), (True, True, 0.25))
        self.assertEqual(host.report()["/device/param/9/value"], "0.250000")
        host.commands.handle("device", "param/20/indicate", 1)
        host.commands.handle("device", "param/40/indicate", 1)
        self.assertNotIn(20, daw.cursor_device.parameters, "No page 3: ignored")
        self.assertEqual(host.commands.unhandled, [], "Beyond the page cursors: ignored, as the Bitwig extension")
        host.commands.handle("device", "param/1/frobnicate", 1)
        self.assertEqual(host.commands.unhandled, ["device param/<n>/frobnicate"])


class NavigatorTest(unittest.TestCase):

    SETUP = {"numTracks": 8, "numScenes": 4, "wantsClipLauncherNavigator": True}

    def test_only_if_the_controller_wants_it(self):
        host = Host()
        self.assertNotIn("clipLauncherNavigator", host.commands.host_features())
        host.commands.handle("navigator", "tracks/+", None)
        self.assertEqual(host.commands.unhandled, ["navigator tracks/+"])
        host = Host(setup=self.SETUP)
        self.assertIn("clipLauncherNavigator", host.commands.host_features())

    def test_select_track_keeps_the_selected_slot(self):
        daw = project()
        daw.tracks[1].slots[0].select()
        host = Host(daw, self.SETUP)
        host.commands.handle("navigator", "selectTrack", 2)
        host.commands.handle("track", "2/select", 1)
        self.assertTrue(daw.tracks[2].selected)
        self.assertTrue(daw.tracks[1].slots[0].selected, "The slot of the other track stays selected")
        self.assertEqual(host.commands.navigator.track_position, 2)

    def test_tracks_move_the_slot(self):
        daw = project()
        daw.tracks[1].select()
        daw.tracks[1].slots[0].select()
        host = Host(daw, self.SETUP)
        host.commands.handle("navigator", "tracks/+", None)
        self.assertTrue(daw.tracks[2].selected)
        self.assertTrue(daw.tracks[2].slots[0].selected)
        host.commands.handle("navigator", "tracks/-", None)
        host.commands.handle("navigator", "tracks/-", None)
        host.commands.handle("navigator", "tracks/-", None)
        self.assertTrue(daw.tracks[0].selected and daw.tracks[0].slots[0].selected, "Stops at the first track")
        daw.tracks[3].select()
        host.commands.handle("navigator", "tracks/+", None)
        self.assertTrue(daw.tracks[4].selected, "The bank follows the selection")

    def test_clips_and_scenes(self):
        daw = project()
        host = Host(daw, self.SETUP)
        for _ in range(5):
            host.commands.handle("navigator", "clips/+", None)
        self.assertTrue(daw.tracks[0].slots[5].selected)
        self.assertTrue(daw.scenes[5].selected, "The sceneIndex observer selects the scene")
        self.assertEqual(host.banks.scene_offset, 4, "and scrolls the scene window to its page")
        daw.scenes[0].select()
        host.commands.handle("navigator", "scenes/-", None)
        self.assertTrue(daw.tracks[0].slots[4].selected)
        self.assertEqual(host.banks.scene_offset, 4)
        daw.scenes[0].select()
        daw.advance(90)
        self.assertTrue(daw.scenes[0].selected)
        daw.advance(20)
        self.assertTrue(daw.scenes[4].selected, "Selected 100 ms later")
        self.assertEqual(host.commands.unhandled, [])


class EqualizerTest(unittest.TestCase):

    SETUP = {"numTracks": 8, "deviceIds": ["EQ"]}

    def host(self) -> Host:
        daw = project()
        daw.tracks[3].devices.add(eq_plus(daw.preferences))
        daw.tracks[3].select()
        return Host(daw, self.SETUP)

    def test_texts(self):
        # As Bitwig showed them for the fixture's EQ+ (target/conformance/*-java.json)
        self.assertEqual([format_eq_frequency(v) for v in (0.200630, 0.300738, 0.503937, 0.602061, 0.899655)],
                         ["80.0 Hz", "160 Hz", "650 Hz", "1.28 kHz", "10.0 kHz"])
        self.assertEqual((format_eq_gain(0.0), format_eq_gain(0.5), format_eq_q(0.5)), ("-30.0 dB", "0.0 dB", "1.00"))
        eq = Equalizer(Preferences())
        self.assertEqual((eq.parameter(0).name, eq.parameter(4).name, eq.parameter(23).name, eq.parameter(24)), ("Band 1 Frequency", "Band 2 Gain", "Band 8 Q", None))

    def test_report(self):
        host = self.host()
        report = host.report()
        self.assertEqual((report["/eq/exists"], report["/eq/name"], report["/eq/param/count"]), ("1", EQ_PLUS, "24"))
        self.assertEqual((report["/eq/param/0/name"], report["/eq/param/0/value/str"], report["/eq/param/0/value/str/6"]), ("Band 1 Frequency", "80.0 Hz", "80.0"))
        self.assertEqual((report["/eq/param/1/value"], report["/eq/param/1/value/str"], report["/eq/param/2/value/str"]), ("0.500000", "0.0 dB", "1.00"))
        self.assertEqual(report["/eq/band/0/value"], "-1", "Off")
        host.daw.tracks[0].select()
        host.report()
        report = host.reporter.snapshot()
        self.assertEqual((report["/eq/exists"], report["/eq/param/count"], report["/eq/band/0/value"], report["/eq/param/0/name"]), ("0", "0", "-1", ""))
        self.assertNotIn("/eq/exists", Host(setup={"numTracks": 8}).report(), "Only if the controller asks for it")

    def test_commands(self):
        host = self.host()
        eq = host.daw.eq_device.equalizer
        host.commands.handle("eq", "band/0", "8")
        host.commands.handle("eq", "band/7", "4")
        self.assertEqual((eq.types[0], eq.types[7], eq.type_text(0)), ("bell", "lowcut", "Bell"))
        host.commands.handle("eq", "param/1/value", 0.0)
        host.commands.handle("eq", "param/3/indicate", 1)
        self.assertEqual((eq.gains[0].value, eq.frequencies[1].indicated), (0.0, True))
        report = host.report()
        self.assertEqual((report["/eq/band/0/value"], report["/eq/band/7/value"], report["/eq/param/1/value/str"]), ("8", "4", "-30.0 dB"))
        host.commands.handle("eq", "window", 1)
        host.commands.handle("eq", "bypass", None)
        self.assertTrue(host.daw.eq_device.window_open and not host.daw.eq_device.enabled)
        self.assertEqual(host.commands.unhandled, [])
        host.commands.handle("eq", "band/0", "5")
        host.commands.handle("eq", "param/24/value", 0.5)
        self.assertEqual(host.commands.unhandled, ["eq band/<n>", "eq param/<n>/value"], "Unknown type and index")

    def test_add(self):
        host = self.host()
        host.daw.tracks[0].select()
        self.assertIsNone(host.daw.eq_device)
        host.commands.handle("eq", "param/1/value", 0.0)
        self.assertEqual(host.commands.unhandled, [], "Ignored without an EQ+")
        host.commands.handle("eq", "add", None)
        self.assertEqual(host.daw.eq_device.name, EQ_PLUS)
        self.assertEqual(host.report()["/eq/exists"], "1")
        host.commands.handle("eq", "remove", None)
        self.assertIsNone(host.daw.eq_device)
        unwanted = Host(setup={"numTracks": 8})
        unwanted.commands.handle("eq", "add", None)
        self.assertEqual(unwanted.commands.unhandled, ["eq add"])


class LaunchControlXLTest(unittest.TestCase):
    """The Novation Launch Control XL against the simulation, as dbm_create_controller (Bitwig).
    The device answers the template selection (as the stub of tests/conformance/launchcontrol_xl);
    the startup selects the factory template 1 (Track Mix, MIDI channel 9, CC B8, notes 98/88)."""

    # The processors which the later steps cover (PYTHON_HOST_PLAN.md §5)
    LATER_STEPS = {"device", "primary", "drum", "clip", "browser", "marker", "application", "project", "groove"}

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="dbm-host-test-")
        self.daw = project()
        self.bridge = Bridge(self.daw, LAUNCH_CONTROL_XL, self.temp.name, "Bitwig Studio 5.3").start()
        self.bridge.on_midi_out = self.device_stub
        self.bridge.wait_for_startup()
        self.bridge.advance(100)

    def tearDown(self):
        self.bridge.close()
        self.temp.cleanup()

    def device_stub(self, data: bytes):
        """The device echoes the template selection."""
        if data[: len(TEMPLATE_HEADER)] == TEMPLATE_HEADER:
            self.bridge.receive(data)

    def led(self, status: int, data1: int) -> int | None:
        for data in reversed(self.bridge.midi_out):
            if len(data) == 3 and data[0] == status and data[1] == data1:
                return data[2]
        return None

    def press(self, note: int, channel: int = 8) -> None:
        self.bridge.send([0x90 + channel, note, 127])
        self.bridge.send([0x80 + channel, note, 0])

    def test_fader_and_mute(self):
        bridge, daw = self.bridge, self.daw
        self.assertIn(bytes.fromhex("F000202902117708F7"), bridge.midi_out, "Template selection at startup")
        self.assertEqual(self.led(0x98, 0x6A), 0x7F, "The Mute mode of the Track Mix is on")
        setup = bridge.app.get_model_setup()
        self.assertEqual((bridge.banks.num_tracks, bridge.banks.num_sends), (setup["numTracks"], setup["numSends"]))
        self.assertIsNotNone(daw.note_input, "The core created the note input at the start")
        self.assertEqual(bridge.reporter.last_values["/noterepeat/period"], "4.000000")

        # Fader 1 (CC 0x4D) sets the volume of the first track
        bridge.send([0xB8, 0x4D, 100])
        bridge.advance(400)
        self.assertAlmostEqual(daw.tracks[0].volume.value, 100 / 127, places=6)
        bridge.send([0xB8, 0x50, 0])
        bridge.advance(400)
        self.assertEqual(daw.tracks[3].volume.value, 0.0)

        # Track Control 2 (note 0x4A) toggles the mute of the second track; the LED follows
        unmuted = self.led(0x98, 0x4A)
        self.press(0x4A)
        bridge.advance(100)
        self.assertTrue(daw.tracks[1].mute)
        muted = self.led(0x98, 0x4A)
        self.assertNotEqual(muted, unmuted)
        self.press(0x4A)
        bridge.advance(100)
        self.assertFalse(daw.tracks[1].mute)
        self.assertEqual(self.led(0x98, 0x4A), unmuted)

        # A mute in the DAW is shown as well
        daw.tracks[2].mute = True
        bridge.advance(100)
        self.assertEqual(self.led(0x98, 0x4B), muted)

        self.assertEqual(bridge.core_errors(), [], "The core knows all reported addresses")
        self.assertEqual(bridge.commands.unhandled, [])

    def test_all_controls_of_the_track_mix(self):
        """Every button, knob and fader of the Track Mix template, also with the mode buttons
        held: no unhandled command of this step's scope."""
        bridge = self.bridge
        track_buttons = [0x29, 0x2A, 0x2B, 0x2C, 0x39, 0x3A, 0x3B, 0x3C, 0x49, 0x4A, 0x4B, 0x4C, 0x59, 0x5A, 0x5B, 0x5C]
        mode_buttons = [0x69, 0x6A, 0x6B, 0x6C]
        arrows = [0x68, 0x69, 0x6A, 0x6B]
        knobs = [*range(0x0D, 0x15), *range(0x1D, 0x25), *range(0x31, 0x39), *range(0x4D, 0x55)]
        for note in track_buttons + mode_buttons:
            self.press(note)
        for cc in arrows:
            bridge.send([0xB8, cc, 127])
            bridge.send([0xB8, cc, 0])
        for knob in knobs:
            bridge.send([0xB8, knob, 100])
            bridge.send([0xB8, knob, 20])
        for hold in mode_buttons:
            bridge.send([0x98, hold, 127])
            for note in track_buttons:
                self.press(note)
            for cc in arrows:
                bridge.send([0xB8, cc, 127])
                bridge.send([0xB8, cc, 0])
            bridge.send([0x88, hold, 0])
        bridge.advance(500)
        print("\nUnhandled commands of the Launch Control XL (later steps):", ", ".join(bridge.commands.unhandled), file=sys.stderr)

        processors = {c.processor for c in bridge.commands.log}
        self.assertTrue({"track", "transport", "scene", "bank"} <= processors, processors)
        in_scope = [key for key in bridge.commands.unhandled if key.split(" ")[0] not in self.LATER_STEPS]
        self.assertEqual(in_scope, [], bridge.commands.unhandled)
        self.assertEqual(bridge.core_errors(), [])
        self.assertTrue(all(re.fullmatch(r"[a-z]+ .*", key) for key in bridge.commands.unhandled))


if __name__ == "__main__":
    unittest.main()
