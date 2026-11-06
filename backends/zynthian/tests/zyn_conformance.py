# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: the conformance scenarios against the model adapter
#
# Copyright (C) 2026 Hannes Janetzek
#
# ******************************************************************************
#
# This program is free software; you can redistribute it and/or
# modify it under the terms of the GNU General Public License as
# published by the Free Software Foundation; either version 2 of
# the License, or any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
# GNU General Public License for more details.
#
# For a full copy of the GNU General Public License see the LICENSE.txt file.
#
# ******************************************************************************

"""The conformance scenarios of DrivenByMoss (``tests/conformance/<setup>/*.toml``) against the
zynthian model adapter, compared with the results of the Java DrivenByMoss on Bitwig, for the
scenarios of the current phase (doc/ZYNTHIAN_PLAN.md §6 b).

Reused from this repository: the scenario files and their device stubs,
``conformance_runner`` (``led_state``, ``check_expectations``, ``matches``, ``parse_hex``), the
in-process runner's timing (``conformance.runner``), the setups and the profile settings
(``conformance.fixture``), the references (``conformance.reference``).

zynthian's own (zynthian's model differs from Bitwig's):

- The fixture: ``ZynthianState`` with the fixture's tracks (names, instrument tracks with a MIDI
  channel and a synth engine of 6 controller screens = 3 pages, audio tracks without), the
  fixture volume, its two effect tracks as mixbuses and zynseq (stand-in, 8 phrases = the
  fixture's 8 scenes, an empty pattern of 4 beats where the fixture has its two empty clips); the fixture's devices
  as processors with the same names and pages (two controller screens of 4 per page of 8, the
  first named as the page; the EQ+ as an audio effect with the reference host's pages); no clips
  or scenes, no drum pads or layers. The tracks have the fixture's colors (zynthian's own are by
  MIDI channel), so the LEDs which show track colors compare.
- The OSC steps: ``/click``, ``/tempo/raw``, ``/device|primary/page/N/selected`` and the track addresses (``/track/N/select|mute|solo|recarm|volume|pan``,
  ``/track/N/send/M/volume``, ``/track/bank/page/±``) with the observer's bank of 8 tracks that
  follows the selected track. Other addresses are skipped (``unknown_osc``).
- The state view: ``osc:/track/N/exists|name|selected|mute|solo|recarm|volume|pan`` and
  ``send/M/volume`` (0..127) of that bank, ``osc:/track/selected/name``; for ``/device`` and
  ``/primary``: ``exists``, ``name``, ``bypass``, ``page/N/exists|name|selected``,
  ``page/selected/name``.
- The comparison: per step of a judged scenario, the complete LED state against the reference's
  (the file's ``known_difference`` patterns apply) and the reference's state changes of the keys
  the view models; the expectations ``expect.bitwig``.

All scenarios of a file run in order (they build on each other); only those of the phase are
judged.
"""

import fnmatch
import os
import sys
import tempfile
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

import zynstubs

zynstubs.install()


def dbm_repository():
    return zynstubs.REPOSITORY


sys.path.insert(0, str(dbm_repository() / "bindings" / "python"))
import conformance  # noqa: E402,F401  (puts conformance_runner on the path)
from conformance import reference  # noqa: E402
from conformance.fixture import DEVICES, EFFECT_TRACKS, INSTRUMENT, REMOTE_CONTROLS, SETUPS, TRACK_COLORS, TRACK_VOLUME, TRACKS, copy_profile_config  # noqa: E402
from conformance.runner import FRESH_PROJECT_MS, MARKER_MS, OSC_PAUSE_MS, RESTART_REST_MS, RESTART_TIMEOUT_MS, RESTART_WAIT_MS  # noqa: E402
from conformance_runner import DEFAULT_SETTLE_MS, check_expectations, led_state, matches, parse_hex, run_name, to_hex  # noqa: E402
from dbm_host import NoteGrid  # noqa: E402
from dbm_host.bridge import TICK_MS  # noqa: E402

from zyngine.ctrldev.dbm_zynthian.bridge import ZynBridge  # noqa: E402
from zyngine.ctrldev.dbm_zynthian.model import ZynthianModel  # noqa: E402

# The scenarios each phase judges, per setup and file (doc/ZYNTHIAN_PLAN.md §6 b)
PHASES = {
    1: {
        ("launchcontrol_xl", "track_mix"): [
            "lcxl.mix.track-focus", "lcxl.mix.mute", "lcxl.mix.solo", "lcxl.mix.recarm", "lcxl.mix.knobs",
            "lcxl.mix.track-page",
        ],
    },
}
# Phase 2 judges the scenarios of phase 1 and the device and first-instrument scenarios
PHASES[2] = {key: list(ids) for key, ids in PHASES[1].items()}
PHASES[2][("launchcontrol_xl", "track_mix")] += ["lcxl.mix.device-page", "lcxl.mix.transport-click",
                                                  "lcxl.mix.transport-tempo", "lcxl.mix.transport-defeat"]
PHASES[2][("launchcontrol_xl", "note_sequencer")] = ["lcxl.noteseq.device-page"]
# Phase 3 (the step grid, doc/ZYNTHIAN_PLAN.md §3.5): the step sequencers on zynseq patterns
PHASES[3] = {key: list(ids) for key, ids in PHASES[2].items()}
PHASES[3][("launchcontrol_xl", "note_sequencer")] = [
    "lcxl.noteseq.device-page", "lcxl.noteseq.create-note", "lcxl.noteseq.repeat", "lcxl.noteseq.panning",
    "lcxl.noteseq.delete-note",
]
PHASES[3][("launchcontrol_xl", "drum_sequencer")] = [
    "lcxl.drum.select-pad", "lcxl.drum.step-on", "lcxl.drum.knobs", "lcxl.drum.step-off", "lcxl.drum.pad-mute",
    "lcxl.drum.pad-page",
]

# The Maschine Mk2 (doc/ZYNTHIAN_PLAN.md §7): not judged are automation (auto, shift-auto), the
# arranger overdub (lock), note repeat, the loop and the step sequencer (phase 3)
PHASES[2][("maschine_mk2", "global")] = [
    "mk2.global.shift-lock", "mk2.transport.metronome", "mk2.group.select", "mk2.group.mute", "mk2.group.solo",
    "mk2.group.recarm", "mk2.global.clear-mute", "mk2.global.clear-solo", "mk2.mode.panorama", "mk2.mode.send",
    "mk2.mode.parameters", "mk2.mode.volume", "mk2.encoder.tempo",
]
PHASES[2][("maschine_mk2", "pad_modes")] = [
    "mk2.chords.play", "mk2.drum.play", "mk2.keyboard.play", "mk2.pattern.select", "mk2.scene.select",
    "mk2.variation.parameter",
]
PHASES[3][("maschine_mk2", "global")] = list(PHASES[2][("maschine_mk2", "global")])
PHASES[3][("maschine_mk2", "pad_modes")] = PHASES[2][("maschine_mk2", "pad_modes")] + [
    "mk2.drum.step-on", "mk2.drum.mute-step", "mk2.drum.step-off",
]

# The differences of zynthian's model from Bitwig's which the judged scenarios show:
# (scenario, fnmatch pattern of the problem, reason)
KNOWN_DIFFERENCES = [
    ("*", "step *: STATE osc:/track/*/recarm: zynthian 0 java 1",
     "Bitwig's auto-arm (setup preference): selecting an instrument track arms it; zynthian has no auto-arm"),
    ("*", "step *: STATE osc:/track/*/clip/*/isSelected: zynthian 0 java 1",
     "Bitwig keeps a selected slot per track (the fixture's Drums slot 1); zynthian has one selected phrase, shown on the active chain"),
    ("lcxl.mix.*", "step *: LED 98 4[9ABC]: zynthian 0D java 0F",
     "Bitwig's auto-arm: the armed instrument track lights red in the Record Arm row"),
    ("lcxl.mix.*", "step *: LED 98 49: zynthian 0D java 0F", "Bitwig's auto-arm (see above)"),
    ("lcxl.mix.[rt]*", "step *: LED 98 39: zynthian 0C java 1D",
     "Record Arm held: Track Focus 5 is undo; Bitwig's fixture had an undo history, zynthian has none (/project/canUndo 0)"),
    ("lcxl.mix.solo", "step 1: LED 98 49: zynthian 3E java 1D",
     "Solo held: the Track Control row shows the scenes; zynthian always has a selected phrase (the first), Bitwig's fixture had no selected scene"),
]
# The Maschine Mk2
KNOWN_DIFFERENCES += [
    ("mk2.*", "step *: LED B0 23: *", "AUTO: zynthian has no automation (Bitwig's state of the fixture and of mk2.global.auto)"),
    ("mk2.*", "step *: LED B0 24: *", "LOCK: zynthian has no arranger overdub (on in Bitwig's fixture)"),
    ("mk2.*", "step *: LED B0 35: *", "RESTART: zynthian has no loop (/repeat 0)"),
    ("mk2.*", "step *: LED B0 3A: zynthian 7F java None",
     "REC: in zynthian record and launcher overdub are the same MIDI record, on since mk2.global.shift-lock"),
    ("mk2.global.shift-*", "step *: LED 90 *: zynthian 00 java *",
     "Shift held: the pads are the clip functions, lit only with a cursor clip; zynthian reports no clip yet (phase 3)"),
    ("mk2.global.clear-*", "step *: LED 90 *: zynthian 00 java *", "Shift held: the clip functions (see shift-*)"),
    ("mk2.transport.metronome", "step *: LED 90 *: zynthian 00 java *", "Shift held: the clip functions (see shift-*)"),
    ("mk2.group.recarm", "step *: LED B0 64: zynthian 4C java 06", "Bitwig's auto-arm: Synth is armed (red with REC held)"),
    ("mk2.group.recarm", "bitwig /record: expected 0, got 1",
     "In zynthian record and launcher overdub are the same MIDI record, on since mk2.global.shift-lock"),
    ("mk2.mode.volume", "step *: LED F0 00 00 66 14 12 38: *",
     "The volume texts: zynthian's level is linear gain (20 log10: -5.3 dB), Bitwig has its own curve (-10.0 dB)"),
    ("mk2.mode.parameters", "step *: LED F0 00 00 66 14 12 38: *",
     "The parameter values: zynthian's labels of the zctrls, Bitwig's displayed values (the names compare)"),
    ("mk2.encoder.tempo", "step *: LED F0 00 00 66 14 12 *: *",
     "Time and Position stay empty: zynthian has no song position (zynseq knows only the beat in the bar)"),
    ("mk2.drum.play", "step *: LED 90 24: zynthian 32 java 2A",
     "The first pad: Bitwig's Drum Machine reports its pads (colors), zynthian has no drum machine (the core's own pads)"),
    ("*", "step *: STATE osc:/track/*/clip/1/hasContent: zynthian 0 java 1",
     "Bitwig's fixture has an empty clip in slot 1 of Synth and Drums, which counts as content; an empty zynseq pattern does not"),
    ("mk2.pattern.select", "step *: LED 90 24: zynthian 4C java 2D",
     "Slot 1: Bitwig's fixture has an empty clip there, which counts as content; an empty zynseq pattern does not"),
    ("mk2.scene.select", "step *: LED 90 24: zynthian 4E java 4C",
     "Scene 1: zynthian always has a selected phrase (the first), Bitwig's fixture had no selected scene"),
]

# The step sequencers (clip.py)
KNOWN_DIFFERENCES += [
    ("lcxl.noteseq.repeat", "bitwig /clip/step/0/1/*: expected 15, got 0", "zynseq has no note repeat (the step's repeat count)"),
    ("lcxl.noteseq.panning", "bitwig /clip/step/0/1/*: expected -1.0, got 0.0", "zynseq notes have no panorama"),
    ("lcxl.drum.*", "step *: LED 9B 0F: zynthian 2E java 0C",
     "The knob of the step's panorama: zynseq notes have none (Bitwig's step was set to -1.0 in lcxl.drum.knobs)"),
    ("lcxl.drum.pad-*", "step *: LED 9B *: *",
     "The drum pads (Track Control row, Mute): zynthian has no drum machine (the first instrument has no pads)"),
]
# The fields of a step line (StepInfo::to_wire) zynseq has: state, selected, muted, duration,
# velocity, chance enabled, chance; velocity and chance are compared within one 7-bit step
STEP_FIELDS = {0: 0.0, 1: 0.0, 2: 0.0, 3: 1e-6, 4: 1.0 / 127, 12: 0.0, 13: 1.0 / 127}


def step_known(ours, theirs):
    """True if two step lines differ only in what zynseq does not have (the reason of the known
    difference: STEP_REASON)."""
    a, b = (ours or "0").split(), (theirs or "0").split()
    if len(a) != 24 or len(b) != 24:
        return False
    return all(abs(float(a[i]) - float(b[i])) <= tolerance + 1e-9 for i, tolerance in STEP_FIELDS.items())


STEP_REASON = ("The step's attributes zynseq has (velocity, duration, chance, muted) compare; the others (pan, repeat, "
               "gain, …) zynseq notes do not have, velocity and chance are zynseq's 7-bit / 32-bit values")

for _scenario in ("lcxl.mix.knobs", "lcxl.mix.device-page", "lcxl.mix.track-page", "lcxl.mix.transport-*"):
    # the later scenarios show the first page again, with the values of lcxl.mix.knobs
    KNOWN_DIFFERENCES += [
        (_scenario, "step *: LED 98 [012]?: *",
         "Bitwig's takeover mode catch (setup preference): a knob that jumps is not taken, its LED shows the old value; zynthian takes every value"),
        (_scenario, "step *: STATE osc:/track/*/[pv]*: *",
         "Bitwig's takeover mode catch: the jumped pan and volume are not taken in Bitwig"),
    ]

# The pages of the EQ+ in the reference host (dbm_host.device.eq_plus)
EQ_PLUS_PAGES = ["Gains", "Freqs", "Qs"]

PAGE_SIZE = 8
MODELLED_FIELDS = ("exists", "name", "selected", "mute", "solo", "recarm", "volume", "pan")


def fixture():
    """The fixture's tracks and effect tracks as zynthian chains, the first track selected."""
    state = zynstubs.ZynthianState()
    state.zynseq = zynstubs.FakeZynseq(phrases=8)  # the fixture's 8 scenes
    midi_chan = 0
    for chain_id, (name, track_type) in enumerate(TRACKS, start=1):
        chan = None
        if track_type == "INSTRUMENT":
            chan, midi_chan = midi_chan, midi_chan + 1
        state.add_chain(chain_id, name, midi_chan=chan, level=TRACK_VOLUME)
    for index, name in enumerate(EFFECT_TRACKS):
        state.add_chain(len(TRACKS) + 1 + index, name, mixbus=True)
    # The fixture's devices as processors with the same names and pages: each page of 8 is two
    # controller screens of 4, the first named as the page (an instrument is a "MIDI Synth")
    for chain_id, (name, track_type) in enumerate(TRACKS, start=1):
        for device_name, kind, pages in DEVICES.get(name, []):
            pages = pages or EQ_PLUS_PAGES
            screens = []
            for page in pages:
                symbols = [f"{page.lower()}{i}" for i in range(8)]
                screens += [(page, symbols[:4]), (f"{page} 2", symbols[4:])]
            proc = state.add_processor(chain_id, device_name, screens,
                                       type="MIDI Synth" if kind == INSTRUMENT else "Audio Effect")
            # The parameters get the names (and normalized values) of the fixture's remote
            # controls; their texts stay zynthian's labels
            controls = REMOTE_CONTROLS.get(device_name, [])
            zctrls = [zctrl for _, screen in proc.ctrl_screens_dict.items() for zctrl in screen]
            for zctrl, control in zip(zctrls, controls):
                if control is not None:
                    name, value, _ = control
                    zctrl.name = zctrl.short_name = name
                    zctrl.set_value(round(zctrl.value_min + value * (zctrl.value_max - zctrl.value_min)))
    # The fixture's clips: slot 1 of Synth and Drums, an empty clip of a bar (4 beats): an empty
    # pattern of 4 beats (Bitwig's empty clip counts as content, a zynseq pattern only with notes)
    libseq = state.zynseq.libseq
    for chain_id in (1, 2):
        chan = state.chain_manager.chains[chain_id].midi_chan
        pattern = libseq.createPattern()
        libseq.addPattern(0, 0, chan, 0, 0, pattern, False)
    state.chain_manager.active_chain = state.chain_manager.chains[1]
    return state


def value127(value):
    return str(round(value * 127))


@dataclass
class ScenarioResult:
    id: str
    judged: bool
    problems: list = field(default_factory=list)
    known: list = field(default_factory=list)


@dataclass
class FileResult:
    setup: str
    file: str
    scenarios: list = field(default_factory=list)
    unknown_osc: list = field(default_factory=list)
    core_errors: list = field(default_factory=list)
    unhandled: list = field(default_factory=list)

    @property
    def judged(self):
        return [s for s in self.scenarios if s.judged]

    @property
    def ok(self):
        return not self.core_errors and all(not s.problems for s in self.judged)

    def report(self, verbose=False):
        lines = []
        for s in self.scenarios:
            lines.append(f"{'ok  ' if not s.problems else 'FAIL'} {s.id}" + ("" if s.judged else " (not judged)")
                         + (f", {len(s.known)} known differences" if s.known else ""))
            lines += [f"       {p}" for p in s.problems]
            if verbose:
                lines += [f"       known: {k}" for k in s.known]
        lines += [f"UNKNOWN OSC {k}" for k in self.unknown_osc]
        lines += [f"UNHANDLED {k}" for k in self.unhandled]
        lines += [f"CORE {e}" for e in self.core_errors]
        return "\n".join(lines)


class Session:
    """The controller on the zynthian fixture, with the device stubs and the observer."""

    def __init__(self, setup, config_dir):
        self.setup = setup
        self.state = fixture()
        self.model = ZynthianModel(self.state.chain_manager, zynseq=self.state.zynseq)
        # zynthian colors a chain by its MIDI channel (gray without): the fixture takes Bitwig's
        # track colors, so the LEDs which show them compare
        colors = {chain_id: color for chain_id, color in enumerate(TRACK_COLORS, start=1)}
        default_color = self.model.color_of
        self.model.color_of = lambda chain: colors.get(chain.chain_id) or default_color(chain)
        self.bridge = ZynBridge(self.model, setup.controller_id, str(config_dir), setup.app_version, midi_sink=self.on_midi_out)
        self.stubs = []
        self.received = []
        self.unknown_osc = []
        self.observer_page = 0
        # The cursor clip as the conformance module observes it (ConformanceModule: 64 x 128 at
        # the default step size, the reference host's state.py)
        self.clip_grid = NoteGrid(64, 128, self.model.cursor_clip)
        self.clip_steps = {}

    def on_midi_out(self, data):
        message = list(data)
        self.received.append(message)
        for stub in self.stubs:
            if matches(stub["match"], message):
                self.bridge.receive(bytes(message if stub["reply"] == "echo" else parse_hex(stub["reply"])))

    def take(self):
        result, self.received = self.received, []
        return result

    def advance(self, ms):
        self.bridge.advance(ms)
        self.follow_selection()

    # The observer (the OSC controller of the Bitwig runs): a bank of 8 tracks

    def follow_selection(self):
        tracks = self.model.tracks()
        selected = next((i for i, t in enumerate(tracks) if t.selected), -1)
        if selected >= 0:
            self.observer_page = selected // PAGE_SIZE

    def bank_track(self, number):
        position = self.observer_page * PAGE_SIZE + number - 1
        tracks = self.model.tracks()
        return tracks[position] if 0 <= position < len(tracks) and number <= PAGE_SIZE else None

    def send_osc(self, text):
        address, _, value = text.partition(" ")
        value = value.strip()
        parts = address.strip("/").split("/")
        handled = False
        if parts[:3] == ["track", "bank", "page"] and len(parts) == 4:
            self.observer_page = max(0, self.observer_page + (1 if parts[3] == "+" else -1))
            handled = True
        elif len(parts) >= 3 and parts[0] == "track" and parts[1].isdigit():
            track = self.bank_track(int(parts[1]))
            handled = self.track_osc(track, parts[2:], value)
        elif parts == ["click"] and self.model.transport is not None:
            self.model.transport.set_metronome((value or "1") not in ("0", "0.0") if value else not self.model.transport.metronome)
            handled = True
        elif parts == ["tempo", "raw"] and self.model.transport is not None:
            self.model.transport.set_tempo(float(value))
            handled = True
        elif len(parts) == 4 and parts[0] in ("device", "primary") and parts[1] == "page" and parts[2].isdigit() and parts[3] == "selected":
            device = self.device(parts[0])
            if device is not None:
                device.select_page(int(parts[2]) - 1)
            handled = True
        if not handled and address not in self.unknown_osc:
            self.unknown_osc.append(address)

    def track_osc(self, track, sub, value):
        launcher = self.model.launcher
        flag = (value or "1") not in ("0", "0.0")
        if sub in (["select"], ["selected"]):
            if track is not None and flag:
                track.select()
        elif sub == ["mute"]:
            if track is not None:
                track.set_mute(flag if value else not track.mute)
        elif sub == ["solo"]:
            if track is not None:
                track.set_solo(flag if value else not track.solo)
        elif sub == ["recarm"]:
            if track is not None:
                track.set_arm(flag if value else not track.arm)
        elif sub in (["volume"], ["pan"]):
            if track is not None:
                (track.volume if sub[0] == "volume" else track.pan).set(float(value) / 127.0)
        elif len(sub) == 3 and sub[0] == "clip" and sub[2] in ("select", "selected"):
            if track is not None and launcher is not None:
                launcher.select(track, int(sub[1]) - 1)
        elif len(sub) == 3 and sub[0] == "send" and sub[2] == "volume":
            if track is not None and int(sub[1]) <= len(track.sends):
                track.sends[int(sub[1]) - 1].volume.set(float(value) / 127.0)
        else:
            return False
        return True

    def device(self, name):
        return self.model.devices.cursor() if name == "device" else self.model.devices.primary()

    def run_step(self, step, settle_ms):
        for _ in range(step.get("repeat", 1)):
            for command in step.get("osc", "").split("|"):
                if command.strip():
                    self.send_osc(command.strip())
                    self.advance(OSC_PAUSE_MS)
            for message in step.get("midi", "").split("|"):
                if message.strip():
                    self.bridge.receive(bytes(parse_hex(message)))
                    self.advance(step.get("interval_ms", 0) or TICK_MS)
        self.advance(step.get("wait_ms", 0) + settle_ms)

    def snapshot(self):
        """The observer's keys (see the module docstring)."""
        view = {}
        for number in range(1, PAGE_SIZE + 1):
            track = self.bank_track(number)
            prefix = f"osc:/track/{number}/"
            view[prefix + "exists"] = "1" if track is not None else "0"
            if track is None:
                # An empty item of the observer's bank, as Bitwig reported it
                view[prefix + "name"] = ""
                view[prefix + "volume"] = "0"
                view[prefix + "pan"] = "0"
                for phrase in range(PAGE_SIZE):
                    for field in ("isSelected", "hasContent", "isPlaying"):
                        view[f"{prefix}clip/{phrase + 1}/{field}"] = "0"
                continue
            view[prefix + "name"] = track.name
            view[prefix + "selected"] = "1" if track.selected else "0"
            view[prefix + "mute"] = "1" if track.mute else "0"
            view[prefix + "solo"] = "1" if track.solo else "0"
            view[prefix + "recarm"] = "1" if track.arm else "0"
            view[prefix + "volume"] = value127(track.volume.value)
            view[prefix + "pan"] = value127(track.pan.value)
            for index, send in enumerate(track.sends[:PAGE_SIZE], start=1):
                view[f"{prefix}send/{index}/volume"] = value127(send.volume.value)
            launcher = self.model.launcher
            chan = launcher.sequence_of(track) if launcher is not None else None
            for phrase in range(PAGE_SIZE):
                # A chain without sequences (audio) has empty slots, as Bitwig's audio tracks
                slot = f"{prefix}clip/{phrase + 1}/"
                exists = chan is not None and phrase < launcher.num_phrases()
                view[slot + "isSelected"] = "1" if exists and track.selected and phrase == launcher.selected_phrase else "0"
                view[slot + "hasContent"] = "1" if exists and launcher.has_content(phrase, chan) else "0"
                view[slot + "isPlaying"] = "1" if exists and launcher.play_state(phrase, chan) in (1, 3) else "0"
        selected = next((t for t in self.model.tracks() if t.selected), None)
        view["osc:/track/selected/name"] = selected.name if selected else ""
        self.model.invalidate_clip()
        clip = self.model.cursor_clip()
        view["osc:/clip/exists"] = "1" if clip is not None else "0"
        current = {f"osc:/clip/step/{channel}/{x}/{y}": line for (channel, x, y), line in self.clip_grid.steps().items()}
        for address in self.clip_steps:
            if address not in current:
                self.clip_steps[address] = "0"
        self.clip_steps.update(current)
        view.update(self.clip_steps)
        transport = self.model.transport
        if transport is not None:
            view["osc:/click"] = "1" if transport.metronome else "0"
            view["osc:/tempo/raw"] = f"{round(transport.tempo, 4)}"
            view["osc:/play"] = "1" if transport.playing else "0"
        launcher = self.model.launcher
        if launcher is not None:
            recording = "1" if launcher.recording else "0"
            view["osc:/record"] = recording
            view["osc:/overdub/launcher"] = recording
        for name in ("device", "primary"):
            device = self.device(name)
            prefix = f"osc:/{name}/"
            view[prefix + "exists"] = "1" if device is not None else "0"
            view[prefix + "name"] = device.name if device is not None else ""
            # An empty device of the observer is "bypassed" (Bitwig: isEnabled of an empty item)
            view[prefix + "bypass"] = "1" if device is None or not device.enabled else "0"
            pages = device.page_names() if device is not None else []
            selected_page = device.selected_page if device is not None else -1
            for number in range(1, PAGE_SIZE + 1):
                exists = number <= len(pages)
                view[f"{prefix}page/{number}/exists"] = "1" if exists else "0"
                view[f"{prefix}page/{number}/name"] = pages[number - 1] if exists else ""
                view[f"{prefix}page/{number}/selected"] = "1" if exists and selected_page == number - 1 else "0"
            view[prefix + "page/selected/name"] = pages[selected_page] if 0 <= selected_page < len(pages) else ""
        return view


DEVICE_KEYS = ("exists", "name", "bypass", "page/selected/name")


def is_modelled(key):
    parts = key.split("/")
    if key in ("osc:/track/selected/name", "osc:/click", "osc:/tempo/raw", "osc:/play", "osc:/record", "osc:/overdub/launcher",
               "osc:/clip/exists"):
        return True
    if len(parts) == 6 and parts[1:3] == ["clip", "step"]:
        return True
    if len(parts) >= 3 and parts[1] in ("device", "primary"):
        rest = "/".join(parts[2:])
        return rest in DEVICE_KEYS or (len(parts) == 5 and parts[2] == "page" and parts[3].isdigit() and parts[4] in ("exists", "selected"))
    if len(parts) >= 4 and parts[1] == "track" and parts[2].isdigit():
        rest = parts[3:]
        return (rest[0] in MODELLED_FIELDS and len(rest) == 1 or (len(rest) == 3 and rest[0] == "send" and rest[2] == "volume")
                or (len(rest) == 3 and rest[0] == "clip" and rest[2] in ("isSelected", "hasContent", "isPlaying")))
    return False


def known_difference(scenario_id, problem):
    for scenario, pattern, reason in KNOWN_DIFFERENCES:
        if fnmatch.fnmatchcase(scenario_id, scenario) and fnmatch.fnmatchcase(problem, pattern):
            return reason
    return None


def known(key, value, patterns):
    text = f"leds {key} {value}"
    return any(fnmatch.fnmatchcase(text, p["match"]) or fnmatch.fnmatchcase(f"leds {key}", p["match"]) for p in patterns)


def compare_leds(ours, theirs, patterns):
    problems = []
    for key in sorted(set(ours) | set(theirs)):
        a, b = ours.get(key), theirs.get(key)
        if a != b and not known(key, a or "-", patterns) and not known(key, b or "-", patterns):
            problems.append(f"LED {key}: zynthian {a} java {b}")
    return problems


def run_file(setup_name, file_name, phase=1, judge_all=False):
    setup = SETUPS[setup_name]
    scenario_file = setup.scenario_dir / f"{file_name}.toml"
    config = tomllib.loads(scenario_file.read_text())
    judged = set(PHASES[phase].get((setup_name, file_name), []))
    patterns = config.get("known_difference", [])
    settle = config.get("settle_ms", DEFAULT_SETTLE_MS)
    sysex_key_bytes = config.get("sysex_key_bytes", 0)
    ref = reference.expand(reference.load(reference.path_for(scenario_file, run_name(config, str(scenario_file)))), patterns)
    ref_scenarios = {s["id"]: s for s in ref["scenarios"]}
    result = FileResult(setup_name, file_name)

    with tempfile.TemporaryDirectory(prefix="dbm-zynthian-conformance-") as temp:
        config_dir = Path(temp) / "config"
        config_dir.mkdir()
        copy_profile_config(setup, config_dir)
        session = Session(setup, config_dir)
        session.bridge.start()
        session.advance(RESTART_WAIT_MS)
        waited = RESTART_WAIT_MS
        while not session.received and waited < RESTART_TIMEOUT_MS:
            session.advance(100)
            waited += 100
        session.advance(RESTART_REST_MS)
        session.take()
        session.stubs = config.get("stub", [])
        session.advance(FRESH_PROJECT_MS + MARKER_MS)
        session.take()
        leds = {}
        for step in config.get("setup", []):
            session.run_step(step, settle)
        led_state(session.take(), leds, sysex_key_bytes)
        session.advance(settle)
        for scenario in config["scenario"]:
            session.advance(settle)
            led_state(session.take(), leds, sysex_key_bytes)
            for step in scenario.get("before", []):
                session.run_step(step, settle)
            led_state(session.take(), leds, sysex_key_bytes)
            entry = ScenarioResult(scenario["id"], judge_all or scenario["id"] in judged)
            expected = ref_scenarios.get(scenario["id"])
            for index, step in enumerate(scenario.get("steps", [])):
                session.run_step(step, settle)
                led_state(session.take(), leds, sysex_key_bytes)
                if not entry.judged or expected is None or index >= len(expected["steps"]):
                    continue
                theirs = expected["steps"][index]
                problems = compare_leds(leds, theirs["leds"], patterns)
                state = session.snapshot()
                for key, value in theirs["bitwig"].items():
                    if is_modelled(key) and state.get(key) != value:
                        if "/clip/step/" in key and step_known(state.get(key), value):
                            entry.known.append(f"step {index + 1}: STATE {key} ({STEP_REASON})")
                            continue
                        problems.append(f"STATE {key}: zynthian {state.get(key)} java {value}")
                entry.problems += [f"step {index + 1}: {p}" for p in problems]
            if entry.judged:
                if expected is None:
                    entry.problems.append("no reference")
                entry.problems += check_expectations(scenario, {"bitwig": session.snapshot(), "leds": dict(leds)})
                for problem in list(entry.problems):
                    reason = known_difference(scenario["id"], problem)
                    if reason:
                        entry.problems.remove(problem)
                        entry.known.append(f"{problem} ({reason})")
            result.scenarios.append(entry)
        result.unknown_osc = session.unknown_osc
        result.core_errors = list(session.bridge.core_errors())
        result.unhandled = list(session.bridge.commands.unhandled)
        session.bridge.close()
    return result


if __name__ == "__main__":
    # python3 zyn_conformance.py [setup file [phase|all]]
    args = sys.argv[1:] or ["launchcontrol_xl", "track_mix"]
    mode = args[2] if len(args) > 2 else "1"
    print(run_file(args[0], args[1], phase=1 if mode == "all" else int(mode), judge_all=mode == "all").report(verbose=True))
