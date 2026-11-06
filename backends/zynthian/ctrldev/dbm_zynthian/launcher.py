# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: zynseq's phrases and sequences as scenes and slots, recording
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

"""zynseq's launcher as the core's scenes and slots, and recording into zynseq (doc/HOST_BRIDGE_GUIDE.md
§3.4, §4.2, §4.5; doc/ZYNTHIAN_PLAN.md §3.3).

zynseq records MIDI into one pattern: ``enableMidiRecord(true)`` adds the notes of its MIDI input
to the selected pattern (``selectPattern``) of the selected sequence (``selectSequence``), in real
time while the sequence plays, else as step entry. It has no arranger or timeline recording. So
the core's recording is "record into a launcher slot", as Bitwig records into a clip launcher slot:

| Core | zynseq |
| --- | --- |
| scene ``i`` | phrase ``i`` of the current scene (``zynseq.scene``) |
| slot ``s`` of a track | the sequence (phrase ``s``, the chain's MIDI channel); MIDI chains on channels 1..16 only |
| ``hasContent`` | the sequence's pattern (track 0, position 0) exists and has events |
| ``playing``, ``playQueued``, ``stopQueued`` | ``getPlayState``: PLAYING/STOPPING, STARTING, STOPPING |
| ``recording`` | MIDI record is on and the sequence is the selected one |
| selected slot / scene | ``zynseq.phrase`` (and the active chain) |
| ``track <p>/createClip/<s> <beats>`` | create and add the pattern if there is none, set its length in beats |
| ``track <p>/clip/<s>/record`` | select phrase, sequence and pattern, enable MIDI record, start the sequence |
| ``track <p>/clip/<s>/select``, ``launch`` | select it; start it if it is stopped |
| ``record`` (toggle) | MIDI record on/off for the cursor slot (the active chain in the selected phrase) |
| ``transport overdub/launcher <0|1>``, ``/overdub/launcher`` | MIDI record on/off for the selected pattern |
| ``/overdub/arranger`` | 0, ``transport overdub/arranger`` ignored (no arranger) |
| ``/record`` | MIDI record is on |

The adapter calls libseq: ``zynseq.add_pattern`` passes 6 arguments to ``addPattern``, which has 7
(analysis §9 Z12).

Not mapped: the audio recorder and the SMF recorder (doc/ZYNTHIAN_PLAN.md §3.3), scene launch,
clip deletion and duplication, slot colors (the chain's), clippy (audio) launchers.
"""

import ctypes
import logging

from zyngine.ctrldev.dbm_zynthian.transport import STOPPED

logger = logging.getLogger("dbm_zynthian")

# zynseq/constants.h
PLAYING = 1
STARTING = 2
STOPPING = 3
NO_PATTERN = 0xFFFFFFFF
NUM_SEQUENCE_CHANNELS = 16


class ZynLauncher:

    def __init__(self, model, zynseq):
        self.model = model
        self.zynseq = zynseq

    @property
    def libseq(self):
        return getattr(self.zynseq, "libseq", None)

    @property
    def available(self):
        return self.libseq is not None

    @property
    def scene(self):
        return int(getattr(self.zynseq, "scene", 0))

    @property
    def selected_phrase(self):
        return int(getattr(self.zynseq, "phrase", 0))

    def num_phrases(self):
        try:
            return len(self.zynseq.state["scenes"][self.scene]["phrases"])
        except (AttributeError, KeyError, IndexError, TypeError):
            return int(getattr(self.zynseq, "phrases", 0))

    @staticmethod
    def sequence_of(track):
        """The sequence channel of a track (its chain's MIDI channel), None without sequences."""
        if track.is_master or track.is_effect:
            return None
        chan = track.chain.midi_chan
        if isinstance(chan, int) and 0 <= chan < NUM_SEQUENCE_CHANNELS:
            return chan
        return None

    # Slots

    def pattern(self, phrase, chan):
        pattern = self.libseq.getPattern(self.scene, phrase, chan, 0, 0)
        return None if pattern == NO_PATTERN else pattern

    def has_content(self, phrase, chan):
        pattern = self.pattern(phrase, chan)
        return pattern is not None and not self.libseq.isPatternEmpty(pattern)

    def play_state(self, phrase, chan):
        return self.libseq.getPlayState(self.scene, phrase, chan)

    @property
    def recording(self):
        return bool(self.libseq.isMidiRecord())

    def selected_sequence(self):
        """(phrase, channel) of the cursor slot: the selected phrase, the active chain's channel."""
        chain = self.model.chain_manager.active_chain
        chan = getattr(chain, "midi_chan", None)
        if isinstance(chan, int) and 0 <= chan < NUM_SEQUENCE_CHANNELS:
            return self.selected_phrase, chan
        return None

    def slot_name(self, phrase, chan):
        try:
            return self.zynseq.get_sequence_name(self.scene, phrase, chan)
        except Exception:
            return f"{phrase + 1}"

    # Operations

    def select(self, track, phrase):
        chan = self.sequence_of(track)
        if chan is None:
            return False
        self.zynseq.select_phrase(phrase)
        self.libseq.selectSequence(self.scene, phrase, chan)
        if not track.selected:
            track.select()
        return True

    def ensure_pattern(self, phrase, chan, beats=None):
        """The pattern of the sequence, created (and added at track 0, position 0) if there is
        none; with beats > 0 its length."""
        pattern = self.pattern(phrase, chan)
        if pattern is None:
            pattern = self.libseq.createPattern()
            self.libseq.addPattern(self.scene, phrase, chan, 0, 0, pattern, False)
        if beats is not None and beats > 0:
            self.libseq.setBeatsInPattern(pattern, int(round(beats)))
        self.refresh()
        return pattern

    def create_clip(self, track, phrase, beats):
        chan = self.sequence_of(track)
        if chan is None or not 0 <= phrase < self.num_phrases():
            return False
        self.ensure_pattern(phrase, chan, beats)
        return True

    def launch(self, track, phrase):
        chan = self.sequence_of(track)
        if chan is None or not 0 <= phrase < self.num_phrases():
            return False
        if self.play_state(phrase, chan) == STOPPED:
            self.libseq.togglePlayState(self.scene, phrase, chan)
        return True

    def record(self, track, phrase):
        """Record into the slot: select it and its pattern, MIDI record on, start it."""
        chan = self.sequence_of(track)
        if chan is None or not 0 <= phrase < self.num_phrases():
            return False
        self.select(track, phrase)
        self.set_recording(True, phrase, chan)
        self.launch(track, phrase)
        return True

    def set_recording(self, enable, phrase=None, chan=None):
        """MIDI record on/off; on: into the pattern of the given sequence, else of the cursor
        slot."""
        if enable:
            if phrase is None:
                selected = self.selected_sequence()
                if selected is None:
                    return False
                phrase, chan = selected
            pattern = self.ensure_pattern(phrase, chan)
            self.libseq.selectSequence(self.scene, phrase, chan)
            self.libseq.selectPattern(ctypes.c_uint32(pattern))
        self.libseq.enableMidiRecord(bool(enable))
        return True

    def toggle_recording(self):
        return self.set_recording(not self.recording)

    def refresh(self):
        """zynseq's Python state (its GUI and the stock drivers read it) after a change."""
        try:
            self.zynseq.refresh_state()
        except Exception as e:
            logger.debug("zynseq refresh_state: %s", e)
