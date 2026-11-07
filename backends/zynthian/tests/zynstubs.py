# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: stand-ins for the tests (no UI, no JACK, no zyncore)
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

"""Stand-ins for the parts of zynthian the dbm drivers use, so the tests run with a plain
``python3`` (standard library only) and without the UI, JACK or the native libraries.

``install()`` must run before the first import of a zynthian module:

- ``zyngine`` becomes a bare package with the real path: its ``__init__`` (all engines) is
  not run, its modules import normally.
- ``zyncoder.zyncore.lib_zyncore`` is a ``FakeZyncore`` that records what the drivers send.
- ``zynautoconnect``, ``zynlibs.zynseq.zynseq``, ``pyliblo3`` and ``zyngui.zynthian_gui_config``
  are small modules with what the imports need.

The zynthian objects themselves are real where it is cheap: ``zynthian_controller`` (the zctrls
of the mixer, configured as ``zynthian_engine_audio_mixer`` does) and the signal manager.
``ZynthianState`` builds chains, mixer strips and mixbus sends with them.

The tests need a zynthian-ui checkout (the ``nura``/``dbm`` branch): ``$ZYNTHIAN_UI_DIR``, else
``zynthian-dbm`` or ``zynthian-ui`` next to this repository. The drivers come from
``backends/zynthian/ctrldev`` of this repository, added to the package ``zyngine.ctrldev`` as
zynthian's ``$ZYNTHIAN_CTRLDEV_PATH`` does. The DrivenByMoss packages come from
``bindings/python`` (or ``$DBM_PYTHON_DIR``), the native library from ``$DBM_LIBRARY`` or
``target/debug``.
"""

import copy
import os
import sys
import types
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[3]
CTRLDEV_DIR = REPOSITORY / "backends" / "zynthian" / "ctrldev"


def find_ui_dir():
    path = os.environ.get("ZYNTHIAN_UI_DIR")
    if path:
        return Path(path)
    for name in ("zynthian-dbm", "zynthian-ui"):
        candidate = REPOSITORY.parent / name
        if (candidate / "zyngine" / "zynthian_ctrldev_manager.py").exists():
            return candidate
    return REPOSITORY.parent / "zynthian-ui"


UI_DIR = find_ui_dir()


def available():
    """True if the zynthian-ui checkout is there (the tests are skipped without it)."""
    return (UI_DIR / "zyngine" / "ctrldev" / "zynthian_ctrldev_base.py").exists()


def dbm_python_dir():
    path = os.environ.get("DBM_PYTHON_DIR")
    if path:
        return Path(path)
    return REPOSITORY / "bindings" / "python"


class FakeZyncore:
    """lib_zyncore: records the MIDI sent to the devices."""

    def __init__(self):
        self.sent = []          # (idev, bytes)
        self.ui_chans = {}
        self.notes = []         # ("on"/"off", chan, note, velocity) into the chains

    def dev_send_midi_event(self, idev, data, size):
        self.sent.append((idev, bytes(data[:size])))
        return 1

    def zmip_set_ui_midi_chans(self, izmip, chans):
        self.ui_chans[izmip] = chans

    def ui_send_note_on(self, chan, note, velocity):
        self.notes.append(("on", chan, note, velocity))

    def ui_send_note_off(self, chan, note, velocity):
        self.notes.append(("off", chan, note, velocity))

    def __getattr__(self, name):
        # Any other function of the library: does nothing
        return lambda *args, **kwargs: 0


def _module(name, **attributes):
    module = types.ModuleType(name)
    module.__dict__.update(attributes)
    sys.modules[name] = module
    return module


def install():
    # Once per process, also if this module is imported under two names (run_tests.py)
    if getattr(sys.modules.get("zyngine"), "_dbm_stubs", False):
        return
    for path in (str(UI_DIR), str(dbm_python_dir())):
        if path not in sys.path:
            sys.path.insert(0, path)

    # zyngine without its __init__
    zyngine = _module("zyngine")
    zyngine.__path__ = [str(UI_DIR / "zyngine")]
    zyngine._dbm_stubs = True

    _module("pyliblo3", send=lambda *args, **kwargs: None)

    import zyncoder.zyncore
    zyncoder.zyncore.lib_zyncore = FakeZyncore()

    devices = {}
    _module("zynautoconnect",
            devices=devices,
            get_midi_in_devid=lambda idev: devices.get(idev, (None, None))[0],
            get_midi_in_uid=lambda idev: devices.get(idev, (None, None))[1],
            dev_in_2_dev_out=lambda idev: idev,
            request_midi_connect=lambda *args, **kwargs: None)

    zynlibs = _module("zynlibs")
    zynlibs.__path__ = []
    zynseq_pkg = _module("zynlibs.zynseq")
    zynseq_pkg.__path__ = []
    zynseq_pkg.zynseq = _module("zynlibs.zynseq.zynseq")
    zynlibs.zynseq = zynseq_pkg

    zyngui = _module("zyngui")
    zyngui.__path__ = []
    zyngui.zynthian_gui_config = _module("zyngui.zynthian_gui_config", LAUNCHER_COLOUR=[
        {"rgb": "#0000FF"}, {"rgb": "#BBBB00"}, {"rgb": "#FF00FF"}, {"rgb": "#23C497"},
    ])

    # The drivers of this repository as part of zyngine.ctrldev ($ZYNTHIAN_CTRLDEV_PATH)
    import zyngine.ctrldev
    if str(CTRLDEV_DIR) not in zyngine.ctrldev.__path__:
        zyngine.ctrldev.__path__.append(str(CTRLDEV_DIR))


def lib_zyncore():
    import zyncoder.zyncore
    return zyncoder.zyncore.lib_zyncore


# --------------------------------------------------------------------------------------------
# A zynthian state: chains with mixer strips
# --------------------------------------------------------------------------------------------


class FakeEngine:
    """The engine of the zctrls: records the values sent (zynthian_engine_audio_mixer would set
    them in zynmixer)."""

    def __init__(self):
        self.sent = []

    def send_controller_value(self, zctrl):
        self.sent.append((zctrl.processor.chain.chain_id, zctrl.symbol, zctrl.value))


class FakeLib:
    """lib_zynmixer: the peak meters in dBFS."""

    def __init__(self):
        self.dpm = {}

    def getDpm(self, chan, leg):
        return self.dpm.get((chan, leg), -200.0)


class FakeZynmixer:
    def __init__(self):
        self.lib_zynmixer = FakeLib()


def mixer_controllers(engine, processor, level=0.8):
    """The zctrls of a mixer strip, with the options of zynthian_engine_audio_mixer."""
    from zyngine.zynthian_controller import zynthian_controller
    toggle = {"is_toggle": True, "value_max": 1, "value_default": 0, "value": 0, "labels": ["off", "on"], "processor": processor}
    return {
        "level": zynthian_controller(engine, "level", {"is_integer": False, "value_max": 1.0, "value_default": 0.8,
                                                       "value": level, "processor": processor}),
        "balance": zynthian_controller(engine, "balance", {"is_integer": False, "value_min": -1.0, "value_max": 1.0,
                                                           "value_default": 0.0, "value": 0.0, "processor": processor}),
        "mute": zynthian_controller(engine, "mute", dict(toggle)),
        "solo": zynthian_controller(engine, "solo", dict(toggle)),
        "record": zynthian_controller(engine, "record", dict(toggle)),
    }


class FakeProcessor:
    def __init__(self, chain, eng_code, mixer_chan, zynmixer):
        self.chain = chain
        self.eng_code = eng_code
        self.mixer_chan = mixer_chan
        self.zynmixer = zynmixer
        self.controllers_dict = {}


class FakeSynthProcessor:
    """A processor with controller screens of up to 4 zctrls, as zynthian_processor."""

    def __init__(self, engine, chain, name, screens, eng_code="JV", type="MIDI Synth"):
        from zyngine.zynthian_controller import zynthian_controller
        self.chain = chain
        self.eng_code = eng_code
        self.type = type
        self.name = name
        self.bypassed = False
        self.current_screen_index = 0
        self.controllers_dict = {}
        self.ctrl_screens_dict = {}
        for screen, symbols in screens:
            zctrls = []
            for symbol in symbols:
                if symbol.startswith("mode"):
                    options = {"labels": ["saw", "square", "sine"], "value": 0, "processor": self}
                elif symbol.startswith("toggle"):
                    # A switch with labels (LV2 toggled ports, zynthian_engine's "off|on")
                    options = {"labels": ["off", "on"], "value_max": 127, "value": 0, "processor": self}
                elif symbol.startswith("switch"):
                    # A toggle without labels (0 / 1)
                    options = {"is_toggle": True, "value_min": 0, "value_max": 1, "value": 0, "processor": self}
                elif symbol.startswith("steps"):
                    options = {"value_min": 0, "value_max": 3, "value": 0, "is_integer": True, "processor": self}
                elif symbol.startswith("float"):
                    options = {"value_min": 0.0, "value_max": 1.0, "value": 0.5, "is_integer": False, "processor": self}
                else:
                    options = {"value_min": 0, "value_max": 127, "value_default": 64, "value": 64, "is_integer": True, "processor": self}
                zctrl = zynthian_controller(engine, symbol, options)
                self.controllers_dict[symbol] = zctrl
                zctrls.append(zctrl)
            self.ctrl_screens_dict[screen] = zctrls

    def get_name(self):
        return self.name

    def get_ctrl_screens(self):
        return self.ctrl_screens_dict

    def get_current_screen_index(self):
        return self.current_screen_index

    def set_current_screen_index(self, index):
        self.current_screen_index = index

    def is_bypassed(self):
        return self.bypassed

    def set_bypass(self, bypass):
        self.bypassed = bool(bypass)

    def toggle_bypass(self):
        self.bypassed = not self.bypassed


class FakePresetEngine:
    """An engine with banks and presets as ZynAddSubFX's (bank rows [path, msb, name, ...],
    preset rows [path, [msb, lsb, prg], title, ext, file]) and the favourites of zynthian_engine
    (in memory). Records set_bank and set_preset (preload: True for a preload)."""

    def __init__(self, banks, favs=(), nickname="ZY"):
        self.nickname = nickname
        self.banks = banks
        self.show_favs_bank = True
        self.allow_preset_preload = True
        self.calls = []
        self.preset_favs = {}
        for bank, title in favs:
            bank_row = self.bank_list()[[row[2] for row in self.bank_list()].index(bank)]
            preset = next(row for row in self.get_preset_list(bank_row) if row[2] == title)
            self.preset_favs[str(preset[0])] = [bank_row, preset]

    def get_name(self, processor):
        return processor.name

    def bank_list(self):
        rows = [[None, 0, "System banks"]]  # A section title, as zynthian's lists have
        for i, name in enumerate(self.banks):
            rows.append([f"/banks/{name}", i, name, "_"])
        return rows

    def get_bank_list(self, processor=None):
        return self.bank_list()

    def set_bank(self, processor, bank):
        self.calls.append(("set_bank", bank[2]))
        return True

    def get_preset_list(self, bank, processor=None):
        return [[f"{bank[0]}/{i + 1:04}-{title}.xiz", [bank[1], 0, i], title, "xiz", f"{i + 1:04}-{title}.xiz"]
                for i, title in enumerate(self.banks[bank[2]])]

    def set_preset(self, processor, preset, preload=False):
        self.calls.append(("preload" if preload else "set_preset", preset[2]))
        return True

    def cmp_presets(self, preset1, preset2):
        try:
            return preset1[0] == preset2[0]
        except (TypeError, IndexError):
            return False

    def get_preset_favs(self, processor):
        return self.preset_favs

    def is_preset_fav(self, preset):
        return str(preset[0]) in self.preset_favs

    def add_processor(self, processor):
        pass


def preset_processor(chain, name, banks, favs=()):
    """A zynthian_processor (its bank and preset functions) with a FakePresetEngine: banks is
    {bank name: [preset titles]}, favs [(bank, title)]."""
    from zyngine.zynthian_processor import zynthian_processor

    class PresetProcessor(zynthian_processor):
        def get_ctrl_screens(self):
            return {}

        def get_current_screen_index(self):
            return 0

        def set_current_screen_index(self, index):
            pass

        def get_state(self):
            return {"bank_info": self.bank_info, "preset_info": self.preset_info}

        def set_state(self, state):
            # As zynthian_processor.set_state: the bank, then the preset into the engine
            if state["bank_info"]:
                self.set_bank_by_info(state["bank_info"])
            self.load_preset_list()
            if "preset_info" in state:
                self.set_preset(state["preset_info"], force_set_engine=False)

    processor = PresetProcessor("ZY", {"TYPE": "MIDI Synth", "NAME": name})
    processor.set_engine(FakePresetEngine(banks, favs))
    processor.set_chain(chain)
    processor.midi_chan = chain.midi_chan
    return processor


class FakeLibseq:
    """zynseq's libseq: transport, tempo, metronome (records the calls)."""

    def __init__(self):
        self.calls = []
        self.state = 0
        self.tempo = 120.0
        self.bpb = 4
        self.metronome_mode = 0
        self.metronome_volume = 0.5

    def getTransportState(self):
        return self.state

    def transportStart(self, client):
        self.calls.append(("start", client.value))
        self.state = 1

    def transportStop(self, client):
        self.calls.append(("stop", client.value))
        self.state = 0

    def getTempo(self):
        return self.tempo

    def getBpb(self):
        return self.bpb

    def tapTempo(self):
        self.calls.append(("tap",))

    def getMetronomeMode(self):
        return self.metronome_mode

    def setMetronomeMode(self, mode):
        self.metronome_mode = mode.value

    def getMetronomeVolume(self):
        return self.metronome_volume

    def setMetronomeVolume(self, level):
        self.metronome_volume = level.value

    # The launcher: patterns of sequences (scene, phrase, channel), play states, MIDI record

    NO_PATTERN = 0xFFFFFFFF

    def _launcher(self):
        if not hasattr(self, "patterns"):
            self.patterns = {}          # pattern -> {"spb": n, "beats": n, "events": [event dicts]}
            self.sequence_patterns = {}  # (scene, phrase, chan) -> pattern
            self.play_states = {}       # (scene, phrase, chan) -> state
            self.playheads = {}         # (scene, phrase, chan) -> play position in steps
            self.play_calls = []         # (phrase, chan, state) of setPlayState
            self.midi_record = False
            self.selected_sequence = None
            self.selected_pattern = None
            self.modified = False

    def getPattern(self, scene, phrase, chan, track, pos):
        self._launcher()
        return self.sequence_patterns.get((scene, phrase, chan), self.NO_PATTERN)

    def createPattern(self):
        self._launcher()
        pattern = len(self.patterns) + 1
        self.patterns[pattern] = {"spb": 4, "beats": 4, "events": []}
        return pattern

    def addPattern(self, scene, phrase, chan, track, pos, pattern, force):
        self._launcher()
        self.sequence_patterns[(scene, phrase, chan)] = pattern
        return True

    def setBeatsInPattern(self, pattern, beats):
        self.patterns[_value(pattern)]["beats"] = _value(beats)

    def getBeatsInPattern(self, pattern):
        return self.patterns[_value(pattern)]["beats"]

    def isPatternEmpty(self, pattern):
        return not self.patterns[_value(pattern)]["events"]

    def getPlayState(self, scene, phrase, chan):
        self._launcher()
        return self.play_states.get((scene, phrase, chan), 0)

    def togglePlayState(self, scene, phrase, chan):
        """zynseq.cpp togglePlayState (sequences with repeat)."""
        state = self.getPlayState(scene, phrase, chan)
        toggled = {0: 2, 2: 0, 1: 8 if chan == 32 else 5, 3: 1, 5: 1, 6: 8, 8: 2}
        self.setPlayState(scene, phrase, chan, toggled.get(state, state))

    def setPlayState(self, scene, phrase, chan, state):
        """zynseq.cpp setPlayState: the state (a phrase launcher's CHILD_STOPPING lets its
        sequences stop, STARTING starts them); recorded in ``play_calls``."""
        self._launcher()
        state = _value(state)
        self.play_calls.append((phrase, chan, state))
        if state in (1, 2):
            # transportStart(TRANSPORT_CLIENT_ZYNSEQ): a launch starts the transport
            if not self.state:
                self.clock_beats = None
            self.state = 1
        if state == 8:
            for (s, p, c), child in list(self.play_states.items()):
                if (s, p) == (scene, phrase) and c != 32 and child & 1:
                    self.play_states[(s, p, c)] = 3
        elif chan == 32 and state == 2:
            for c in range(16):
                if (scene, phrase, c) in self.sequence_patterns:
                    self.play_states[(scene, phrase, c)] = 2
        self.play_states[(scene, phrase, chan)] = state

    def clock(self, ms):
        """A minimal clock for the tests which run on simulated time (the conformance): while the
        transport runs, at each bar (the sync point, at once after the transport started) the
        starting sequences play, the ones stopping at the sync point stop (STOPPING, at the end
        of the pattern in zynseq, here at the bar too); the playheads of the playing sequences
        advance by the steps of their pattern."""
        self._launcher()
        if not self.state:
            return
        beats = getattr(self, "clock_beats", None)
        now = 0.0 if beats is None else beats + ms * self.tempo / 60000.0
        self.clock_beats = now
        if beats is None or int(now // self.bpb) != int(beats // self.bpb):
            for key, state in list(self.play_states.items()):
                if state == 2:
                    self.play_states[key] = 1
                    self.playheads[key] = 0
                elif state in (3, 5):
                    self.play_states[key] = 0
                    self.playheads.pop(key, None)
        for key, state in self.play_states.items():
            pattern = self.patterns.get(self.sequence_patterns.get(key))
            if state in (1, 3, 5) and pattern:
                steps = pattern["spb"] * pattern["beats"]
                self.playheads[key] = int((now % pattern["beats"]) * pattern["spb"]) % steps

    def clearPattern(self, pattern):
        self.patterns[_value(pattern)]["events"] = []
        self.modified = True

    def copyPattern(self, source, destination):
        source, destination = _value(source), _value(destination)
        if source != destination:
            self.patterns[destination] = copy.deepcopy(self.patterns[source])

    def selectSequence(self, scene, phrase, chan):
        self._launcher()
        self.selected_sequence = (scene, phrase, chan)
        return True

    def selectPattern(self, pattern):
        self._launcher()
        pattern = _value(pattern)
        if pattern not in self.patterns:
            self.patterns[pattern] = {"spb": 4, "beats": 4, "events": []}  # zynseq creates it
        self.selected_pattern = pattern

    def getPatternIndex(self):
        self._launcher()
        return -1 if self.selected_pattern is None else self.selected_pattern

    def enableMidiRecord(self, enable):
        self._launcher()
        self.midi_record = bool(enable)

    def isMidiRecord(self):
        self._launcher()
        return self.midi_record

    # The functions on the selected pattern (zynseq.cpp, pattern.cpp)

    def _pattern(self):
        self._launcher()
        return self.patterns.get(self.selected_pattern)

    def getStepsPerBeat(self):
        pattern = self._pattern()
        return pattern["spb"] if pattern else 4

    def getEventDataAt(self, index, data):
        pattern = self._pattern()
        index = _value(index)
        if pattern is None or index >= len(pattern["events"]):
            return -1
        event = data._obj
        for name, value in pattern["events"][index].items():
            setattr(event, name, value)
        return index

    def addNote(self, step, note, velocity, duration, offset=0.0):
        pattern = self._pattern()
        step, note, velocity, duration, offset = (_value(v) for v in (step, note, velocity, duration, offset))
        if pattern is None or step >= pattern["beats"] * pattern["spb"] or note > 127 or velocity > 127:
            return False
        kept, inherited = [], {}
        end = step + duration
        for event in pattern["events"]:
            event_end = event["position"] + event["duration"]
            overlap = event["val1_start"] == note and (step <= event["position"] < end or step < event_end <= end)
            if overlap:
                inherited = inherited or {k: event[k] for k in ("play_chance", "play_freq")}
            else:
                kept.append(event)
        event = {"position": step, "offset": offset, "duration": duration, "command": 0x90, "val1_start": note,
                 "val2_start": velocity, "play_chance": 1.0, "play_freq": 1}
        event.update(inherited)
        kept.append(event)
        kept.sort(key=lambda e: (e["position"], e["val1_start"]))
        pattern["events"] = kept
        self.modified = True
        return True

    def _event(self, step, note):
        pattern = self._pattern()
        for event in pattern["events"] if pattern else []:
            if event["position"] == _value(step) and event["val1_start"] == _value(note):
                return event
        return None

    def removeNote(self, step, note):
        event = self._event(step, note)
        if event is not None:
            self._pattern()["events"].remove(event)
        self.modified = True

    def clearNotes(self):
        if self._pattern():
            self._pattern()["events"] = []
        self.modified = True

    def setNoteVelocity(self, step, note, velocity):
        event = self._event(step, note)
        if event is not None:
            event["val2_start"] = _value(velocity)
        self.modified = True

    def setNotePlayChance(self, step, note, chance):
        event = self._event(step, note)
        if event is not None:
            event["play_chance"] = _value(chance)
        self.modified = True

    def setNotePlayFreq(self, step, note, freq):
        event = self._event(step, note)
        if event is not None:
            event["play_freq"] = _value(freq)
        self.modified = True

    def transpose(self, value):
        for event in self._pattern()["events"]:
            event["val1_start"] = min(127, max(0, event["val1_start"] + _value(value)))
        self.modified = True

    def getPatternPlayhead(self):
        self._launcher()
        return self.playheads.get(self.selected_sequence, 0)

    def add_test_note(self, pattern, position, key, velocity=100, duration=1.0, offset=0.0):
        """A note in a pattern, as zynthian's pattern editor would add it."""
        previous = self.selected_pattern
        self.selectPattern(pattern)
        self.addNote(position, key, velocity, duration, offset)
        self.selected_pattern = previous


def _value(value):
    return getattr(value, "value", value)


class FakeZynseq:
    """zynseq: the transport, the phrases of scene 0 and the launcher functions of libseq."""

    def __init__(self, phrases=4):
        self.libseq = FakeLibseq()
        self.scene = 0
        self.phrase = 0
        self.state = {"scenes": [{"phrases": [{} for _ in range(phrases)]}]}
        self.refreshes = 0
        self.calls = []

    @property
    def phrases(self):
        return len(self.state["scenes"][0]["phrases"])

    def set_tempo(self, tempo):
        self.libseq.tempo = tempo

    def select_phrase(self, phrase, force=False):
        self.phrase = max(0, min(phrase, self.phrases - 1))

    def get_sequence_name(self, scene, phrase, sequence):
        return f"{chr(65 + sequence)}{phrase + 1}"

    def refresh_state(self, send=True):
        self.refreshes += 1

    # zynseq.py's phrase management on scene 0 (libseq's insertPhrase, duplicatePhrase, removePhrase)

    def _move_phrases(self, phrase, offset):
        """The sequences of the phrases from ``phrase`` on move by offset."""
        libseq = self.libseq
        libseq._launcher()
        for name in ("sequence_patterns", "play_states"):
            items = getattr(libseq, name)
            moved = {}
            for (scene, p, chan), value in items.items():
                if p >= phrase:
                    p += offset
                if p >= 0:
                    moved[(scene, p, chan)] = value
            setattr(libseq, name, moved)

    def insert_phrase(self, scene, phrase=None):
        phrase = self.phrases if phrase is None else phrase
        self.calls.append(("insert_phrase", phrase))
        self._move_phrases(phrase, 1)
        self.state["scenes"][0]["phrases"].insert(phrase, {})
        self.refresh_state()

    def duplicate_phrase(self, scene, phrase=None):
        """A stopped copy, inserted at the phrase (the original moves one down)."""
        self.calls.append(("duplicate_phrase", phrase))
        self.insert_phrase(scene, phrase)
        libseq = self.libseq
        for (s, p, chan), pattern in list(libseq.sequence_patterns.items()):
            if p == phrase + 1:
                copy_pattern = libseq.createPattern()
                libseq.copyPattern(pattern, copy_pattern)
                libseq.sequence_patterns[(s, phrase, chan)] = copy_pattern

    def remove_phrase(self, scene, phrase):
        if self.phrases < 2:
            return
        self.calls.append(("remove_phrase", phrase))
        self.libseq.play_states.clear()  # libseq stops all sequences
        for key in [key for key in self.libseq.sequence_patterns if key[1] == phrase]:
            del self.libseq.sequence_patterns[key]
        self._move_phrases(phrase + 1, -1)
        del self.state["scenes"][0]["phrases"][phrase]
        if self.phrase == phrase:
            self.phrase = max(0, self.phrase - 1)
        self.refresh_state()


class FakeChain:
    def __init__(self, chain_id, title, midi_chan=None, mixbus=False, audio=True, synth=False):
        self.chain_id = chain_id
        self.title = title
        self.midi_chan = midi_chan
        self.mixbus = mixbus
        self.synth = synth
        self.zynmixer_proc = None
        self.audio = audio
        self.processors = []
        self.current_processor = None

    def get_processors(self):
        return list(self.processors) + ([self.zynmixer_proc] if self.zynmixer_proc else [])

    def set_current_processor(self, processor):
        if processor in self.get_processors():
            self.current_processor = processor

    def get_title(self):
        return self.title

    def set_title(self, title):
        self.title = title

    def is_mixbus(self):
        return self.mixbus and self.zynmixer_proc is not None

    def is_midi(self):
        return self.midi_chan is not None

    def is_audio(self):
        return self.zynmixer_proc is not None

    def is_synth(self):
        return self.synth


class FakeChainManager:
    def __init__(self):
        self.chains = {}
        self.active_chain = None

    def get_active_chain(self):
        return self.active_chain

    def set_active_chain_by_id(self, chain_id=None):
        self.active_chain = self.chains.get(chain_id, self.active_chain)
        return self.active_chain.chain_id

    def get_chain_ids_filtered(self, filter=None):
        return list(self.chains)


class ZynthianState:
    """A state manager with a chain manager, chains, mixer strips and mixbus sends."""

    def __init__(self):
        self.engine = FakeEngine()
        self.chain_manager = FakeChainManager()
        self.zynmixer_chan = FakeZynmixer()
        self.zynmixer_bus = FakeZynmixer()
        self.zynseq = None
        self.busy = set()
        self.next_chan = 0
        self.next_bus = 1
        self.add_chain(0, "Main", mixbus=True)

    def is_busy(self, client=None):
        return client in self.busy if client else bool(self.busy)

    def add_chain(self, chain_id, title, midi_chan=None, mixbus=False, mixer=True, level=0.8):
        chain = FakeChain(chain_id, title, midi_chan, mixbus=mixbus)
        if mixer:
            if mixbus:
                mixer_chan = 0 if chain_id == 0 else self.next_bus
                if chain_id:
                    self.next_bus += 1
                proc = FakeProcessor(chain, "MR", mixer_chan, self.zynmixer_bus)
            else:
                proc = FakeProcessor(chain, "MI", self.next_chan, self.zynmixer_chan)
                self.next_chan += 1
            proc.controllers_dict = mixer_controllers(self.engine, proc, level)
            chain.zynmixer_proc = proc
        self.chain_manager.chains[chain_id] = chain
        if self.chain_manager.active_chain is None or chain_id and self.chain_manager.active_chain.chain_id == 0:
            self.chain_manager.active_chain = chain
        self.refresh_sends()
        return chain

    def remove_chain(self, chain_id):
        chain = self.chain_manager.chains.pop(chain_id)
        if self.chain_manager.active_chain is chain:
            self.chain_manager.active_chain = next(iter(self.chain_manager.chains.values()))
        self.refresh_sends()

    def refresh_sends(self):
        """As chain_manager.refresh_mixbus_sends: send_<mixbus id>_level zctrls on every input strip."""
        from zyngine.zynthian_controller import zynthian_controller
        buses = [c for c in self.chain_manager.chains.values() if c.chain_id and c.is_mixbus()]
        for chain in self.chain_manager.chains.values():
            proc = chain.zynmixer_proc
            if proc is None or proc.eng_code != "MI":
                continue
            for bus in buses:
                symbol = f"send_{bus.chain_id}_level"
                if symbol not in proc.controllers_dict:
                    proc.controllers_dict[symbol] = zynthian_controller(self.engine, symbol, {
                        "value_max": 1.0, "value_default": 0.0, "value": 0.0, "processor": proc})
            for symbol in [s for s in proc.controllers_dict if s.startswith("send_")]:
                if int(symbol.split("_")[1]) not in [b.chain_id for b in buses]:
                    del proc.controllers_dict[symbol]

    def add_processor(self, chain_id, name, screens, type="MIDI Synth"):
        """A processor with controller screens [(name, [symbols])]; the first is current."""
        chain = self.chain_manager.chains[chain_id]
        proc = FakeSynthProcessor(self.engine, chain, name, screens, type=type)
        chain.processors.append(proc)
        if chain.current_processor is None:
            chain.current_processor = proc
        return proc

    def add_preset_processor(self, chain_id, name, banks, favs=()):
        """A processor with banks and presets (preset_processor), the chain's current one."""
        chain = self.chain_manager.chains[chain_id]
        proc = preset_processor(chain, name, banks, favs)
        chain.processors.append(proc)
        chain.current_processor = proc
        return proc

    def zctrl(self, chain_id, symbol):
        return self.chain_manager.chains[chain_id].zynmixer_proc.controllers_dict[symbol]


def project():
    """Four chains (two synths, an audio chain, a MIDI chain without mixer strip), two aux
    mixbuses and the main chain; the first synth is active."""
    state = ZynthianState()
    state.add_chain(1, "Synth", midi_chan=0)
    state.add_chain(2, "Piano", midi_chan=1)
    state.add_chain(3, "Audio In")
    state.add_chain(4, "MIDI Tool", midi_chan=2, mixer=False)
    state.add_chain(5, "Reverb", mixbus=True)
    state.add_chain(6, "Delay", mixbus=True)
    state.chain_manager.active_chain = state.chain_manager.chains[1]
    return state
