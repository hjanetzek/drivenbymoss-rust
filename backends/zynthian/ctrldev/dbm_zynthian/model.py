# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: zynthian's chains and mixer as the core's DAW model
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

"""zynthian's chains as the tracks of the DrivenByMoss core (phase 1: the mixer).

The model reads zynthian's objects every time (nothing is copied), so the reporter sees every
change, also those made by the GUI, MIDI learn or a snapshot. It only knows the interfaces it
uses, so the tests can give it stand-ins:

- the chain manager: ``chains`` (dict in display order), ``active_chain``,
  ``set_active_chain_by_id``;
- a chain: ``chain_id``, ``midi_chan``, ``title``/``get_title()``, ``set_title()``,
  ``is_mixbus()``, ``is_midi()``, ``is_audio()``, ``zynmixer_proc``;
- the mixer processor of a chain: ``controllers_dict`` (zctrls ``level``, ``balance``,
  ``mute``, ``solo``, ``record``, ``send_<mixbus chain id>_level``), ``mixer_chan``,
  ``zynmixer`` (``lib_zynmixer.getDpm`` for the meters);
- a zctrl (``zynthian_controller``): ``value``, ``value_min``, ``value_max``,
  ``value_default``, ``set_value()``, ``toggle()``, ``reset_value()``.

The mapping (see doc/ZYNTHIAN_PLAN.md §3 of the DrivenByMoss port):

| Core | zynthian |
| --- | --- |
| tracks | the chains without the main chain and the mixbus chains, in display order |
| effect tracks | the mixbus chains (aux buses), without the main chain |
| master | the main chain (chain 0, the main mixbus) |
| volume | zctrl ``level`` (0..1, linear gain: 1.0 = 0 dB) |
| pan | zctrl ``balance`` (-1..1) |
| mute, solo, recarm | zctrls ``mute``, ``solo``, ``record`` (the audio recorder's arm) |
| send ``s`` | zctrl ``send_<id>_level`` of the s-th mixbus chain |
| select | the active chain |
| color | ``LAUNCHER_COLOUR`` of the chain's MIDI channel (given as palette), else gray |
| VU | the peak meter of the strip (dBFS, -60..0 dB as 0..1) |

A chain without a mixer strip (a MIDI chain) is a track without volume, pan, mute and solo:
its values are reported as 0 and its commands are ignored.
"""

import math
import logging

import dbm_host
from dbm_host.parameter import format_pan, limit_text

logger = logging.getLogger("dbm_zynthian")

MAIN_CHAIN_ID = 0

# The meters: dBFS from VU_FLOOR_DB to 0 dB as 0..1, in steps of 1/VU_STEPS (fewer reports)
VU_FLOOR_DB = -60.0
VU_STEPS = 128

GRAY = (128, 128, 128)


def format_level(level):
    """The text of a zynmixer level (linear gain, 1.0 = 0 dB), as the mixer GUI computes it."""
    if level is None or level <= 0:
        return "-Inf dB"
    return f"{20.0 * math.log10(level):.1f} dB"


def dbfs_to_vu(dbfs):
    """A peak meter value in dBFS as the core's VU (0..1)."""
    if dbfs is None or dbfs <= VU_FLOOR_DB:
        return 0.0
    value = min(1.0, (dbfs - VU_FLOOR_DB) / -VU_FLOOR_DB)
    return round(value * VU_STEPS) / VU_STEPS


def hex_to_rgb(text):
    """"#RRGGBB" as (r, g, b), None if it is no such text."""
    try:
        text = text.lstrip("#")
        return int(text[0:2], 16), int(text[2:4], 16), int(text[4:6], 16)
    except (AttributeError, ValueError, IndexError):
        return None


class ZctrlParameter:
    """A zctrl as a normalized value 0..1 of the core (duck-types ``dbm_host.Parameter``).

    ``get_zctrl`` returns the zctrl or None (no mixer strip, the send was removed); the zctrl is
    looked up every time because zynthian replaces the send zctrls when the mixbuses change."""

    def __init__(self, get_zctrl, text=None):
        self.get_zctrl = get_zctrl
        self.text = text
        """Returns the displayed text of a normalized value; the default is the zctrl's label."""
        self.touched = False
        self.indicated = False

    @property
    def zctrl(self):
        try:
            return self.get_zctrl()
        except (AttributeError, KeyError, TypeError):
            return None

    @property
    def exists(self):
        return self.zctrl is not None

    @staticmethod
    def _normalize(zctrl, value):
        span = zctrl.value_max - zctrl.value_min
        if not span:
            return 0.0
        return min(1.0, max(0.0, (value - zctrl.value_min) / span))

    @property
    def value(self):
        zctrl = self.zctrl
        if zctrl is None:
            return 0.0
        return self._normalize(zctrl, zctrl.value)

    @property
    def default(self):
        zctrl = self.zctrl
        if zctrl is None:
            return 0.0
        return self._normalize(zctrl, zctrl.value_default)

    def set(self, value):
        """Set from a normalized value. zynthian has no takeover mode: always taken."""
        zctrl = self.zctrl
        if zctrl is None:
            return False
        value = min(1.0, max(0.0, float(value)))
        zctrl.set_value(zctrl.value_min + value * (zctrl.value_max - zctrl.value_min))
        return True

    def set_immediately(self, value):
        self.set(value)

    def inc(self, delta):
        """Change by a normalized delta (the host feature valueInc)."""
        if self.zctrl is not None:
            self.set(self.value + float(delta))

    def reset(self):
        zctrl = self.zctrl
        if zctrl is not None:
            zctrl.reset_value()

    def touch(self, is_touched):
        self.touched = bool(is_touched)

    def set_indication(self, is_indicated):
        self.indicated = bool(is_indicated)

    def displayed(self):
        zctrl = self.zctrl
        if zctrl is None:
            return ""
        if self.text is not None:
            return self.text(self.value)
        try:
            return str(zctrl.get_value2label())
        except Exception:
            return str(zctrl.value)

    def displayed_limited(self, limit):
        return limit_text(self.displayed(), limit)


def volume_parameter(get_zctrl):
    def text(value):
        zctrl = get_zctrl()
        return format_level(zctrl.value_min + value * (zctrl.value_max - zctrl.value_min))
    return ZctrlParameter(get_zctrl, text)


class ChainSend:
    """The send of a track to an effect track (a mixbus chain)."""

    def __init__(self, track, target):
        self.track = track
        self.target = target
        symbol = f"send_{target.chain_id}_level"
        self.volume = volume_parameter(lambda: track.controller(symbol))

    @property
    def name(self):
        return self.target.name

    @property
    def color(self):
        return self.target.color

    @property
    def enabled(self):
        """zynthian's sends have no switch (only pre/post fader): always enabled."""
        return True


class ChainTrack:
    """A chain as a track, an effect track or the master of the core."""

    def __init__(self, model, chain):
        self.model = model
        self.chain = chain
        self.volume = volume_parameter(lambda: self.controller("level"))
        self.pan = ZctrlParameter(lambda: self.controller("balance"), format_pan)

    # zynthian's objects

    @property
    def chain_id(self):
        return self.chain.chain_id

    def controller(self, symbol):
        """The zctrl of the chain's mixer strip, None without strip or zctrl."""
        proc = self.chain.zynmixer_proc
        if proc is None:
            return None
        return proc.controllers_dict.get(symbol)

    def _flag(self, symbol):
        zctrl = self.controller(symbol)
        return zctrl is not None and zctrl.value > zctrl.value_min

    def _set_flag(self, symbol, flag):
        zctrl = self.controller(symbol)
        if zctrl is not None:
            zctrl.set_value(zctrl.value_max if flag else zctrl.value_min)

    # The values the reporter reads

    @property
    def is_master(self):
        return self.chain.chain_id == MAIN_CHAIN_ID

    @property
    def is_effect(self):
        return not self.is_master and bool(self.chain.is_mixbus())

    @property
    def has_mixer(self):
        return self.chain.zynmixer_proc is not None

    @property
    def name(self):
        try:
            return self.chain.get_title() or ""
        except Exception:
            return self.chain.title or ""

    @property
    def type_name(self):
        if self.is_master:
            return "MASTER"
        if self.is_effect:
            return "EFFECT"
        if self.chain.is_midi():
            return "INSTRUMENT"
        return "AUDIO"

    @property
    def color(self):
        return self.model.color_of(self.chain)

    @property
    def selected(self):
        active = self.model.chain_manager.active_chain
        return active is not None and active.chain_id == self.chain.chain_id

    @property
    def mute(self):
        return self._flag("mute")

    @property
    def solo(self):
        return self._flag("solo")

    @property
    def arm(self):
        return self._flag("record")

    @property
    def can_hold_notes(self):
        return bool(self.chain.is_midi())

    @property
    def can_hold_audio(self):
        return bool(self.chain.is_audio())

    @property
    def sends(self):
        if self.is_master or self.is_effect:
            return []
        return [ChainSend(self, target) for target in self.model.effect_tracks()]

    def vu(self):
        """(left, right) of the peak meter, 0..1."""
        proc = self.chain.zynmixer_proc
        if proc is None or not self.model.with_vu:
            return 0.0, 0.0
        try:
            lib = proc.zynmixer.lib_zynmixer
            return dbfs_to_vu(lib.getDpm(proc.mixer_chan, 0)), dbfs_to_vu(lib.getDpm(proc.mixer_chan, 1))
        except Exception:
            return 0.0, 0.0

    # The operations of the commands

    def select(self):
        self.model.chain_manager.set_active_chain_by_id(self.chain.chain_id)

    def set_mute(self, flag):
        self._set_flag("mute", flag)

    def set_solo(self, flag):
        self._set_flag("solo", flag)

    def toggle_solo(self):
        zctrl = self.controller("solo")
        if zctrl is not None:
            zctrl.toggle()

    def set_arm(self, flag):
        self._set_flag("record", flag)

    def rename(self, name):
        self.chain.set_title(name)


class ZynthianModel:
    """zynthian as the DAW of a ``dbm_host.Bridge`` (``ZynBridge``)."""

    def __init__(self, chain_manager, palette=None, with_vu=True, zynseq=None):
        """``palette``: the colors of the MIDI channels 0..15 as "#RRGGBB" (zynthian_gui_config's
        LAUNCHER_COLOUR); ``with_vu``: report the peak meters; ``zynseq``: the sequencer, for the
        transport (phase 2; None: no transport)."""
        # Phase 2: the devices and the transport (imported here: they import this module)
        from zyngine.ctrldev.dbm_zynthian.device import DeviceChain
        from zyngine.ctrldev.dbm_zynthian.launcher import ZynLauncher
        from zyngine.ctrldev.dbm_zynthian.transport import ZynTransport
        self.chain_manager = chain_manager
        self.devices = DeviceChain(self)
        self.transport = ZynTransport(zynseq) if zynseq is not None else None
        self.launcher = ZynLauncher(self, zynseq) if zynseq is not None else None
        self.selected_notes = {}
        """The notes the core selected, per pattern: {(position, key)} (zynseq has no selection)."""
        self.clip_channel = 0
        """The MIDI channel the core addresses the cursor clip's notes with (zynseq has none)."""
        self.clip_start_request = None
        """A clip start other than 0 the core asked for since the last report (``clip loopStart``,
        ``start``): the loop length and end of that range are not applied (commands.py)."""
        self._clip = None
        self.palette = [hex_to_rgb(color) or GRAY for color in palette or []]
        self.with_vu = with_vu
        self.note_input = None
        """The default note input of the controller (``dbm_host.NoteInput``)."""
        self.note_sink = None
        """Called with (status, key, velocity) for the notes of the note inputs and the core's
        played notes (process_midi_arg): the driver sends them to the active chain."""
        self.playing_notes = {}
        """The notes played through the note inputs which sound now (key -> velocity), reported as
        the cursor track's playing notes (the pads light while they are held)."""
        self._tracks = {}

    # The chains

    def _track(self, chain):
        track = self._tracks.get(chain.chain_id)
        if track is None or track.chain is not chain:
            track = ChainTrack(self, chain)
            self._tracks[chain.chain_id] = track
        return track

    def _chains(self):
        # A copy: the GUI thread may add or remove chains meanwhile
        return list(self.chain_manager.chains.values())

    def tracks(self):
        """The chains without the main chain and the mixbuses, in display order."""
        return [self._track(chain) for chain in self._chains() if chain.chain_id != MAIN_CHAIN_ID and not chain.is_mixbus()]

    def effect_tracks(self):
        """The mixbus chains without the main chain, in display order."""
        return [self._track(chain) for chain in self._chains() if chain.chain_id != MAIN_CHAIN_ID and chain.is_mixbus()]

    def master(self):
        chain = self.chain_manager.chains.get(MAIN_CHAIN_ID)
        return self._track(chain) if chain is not None else None

    def track_list(self, with_effects_and_master):
        """The core's track list; the full flat list (model setup ``hasFullFlatTrackList``) has the
        effect tracks and the master after the tracks."""
        tracks = self.tracks()
        if with_effects_and_master:
            tracks += self.effect_tracks()
            master = self.master()
            if master is not None:
                tracks.append(master)
        return tracks

    def cursor_clip(self):
        """The cursor clip: the pattern of the cursor slot (clip.py), read once per collection or
        command (``invalidate_clip``); None without a MIDI chain or pattern."""
        if self._clip is not None:
            return self._clip
        launcher = self.launcher
        if launcher is None or not launcher.available:
            return None
        selected = launcher.selected_sequence()
        if selected is None:
            return None
        phrase, chan = selected
        pattern = launcher.pattern(phrase, chan)
        if pattern is None:
            return None
        from zyngine.ctrldev.dbm_zynthian.clip import ZynseqClip
        clip = ZynseqClip(self, pattern, channel=self.clip_channel)
        clip.phrase, clip.chan = phrase, chan
        self._clip = clip
        return clip

    def invalidate_clip(self):
        self._clip = None

    def color_of(self, chain):
        chan = chain.midi_chan
        if isinstance(chan, int) and 0 <= chan < len(self.palette):
            return self.palette[chan]
        return GRAY

    # What the Bridge needs (dbm_host.SimDaw)

    def create_note_input(self, name="", filters=None, default=True):
        note_input = dbm_host.NoteInput(name, filters)
        if default:
            self.note_input = note_input
        return note_input

    def play_live_note(self, status, key, velocity):
        if not 0 <= key < 128:
            return
        self.note_played(status, key, velocity)
        if self.note_sink is not None:
            self.note_sink(status, key, velocity)

    def note_played(self, status, key, velocity):
        """Keep the sounding notes (also called by the driver, which sends the notes itself)."""
        if status & 0xF0 == 0x90 and velocity > 0:
            self.playing_notes[key] = velocity
        else:
            self.playing_notes.pop(key, None)

    def advance(self, ms):
        """The simulated time of the reference host; zynthian runs in real time."""
