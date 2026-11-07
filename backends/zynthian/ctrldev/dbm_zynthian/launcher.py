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
| ``playing`` | ``getPlayState``: PLAYING, STOPPING, STOPPING_SYNC |
| ``playQueued`` | STARTING |
| ``stopQueued`` | STOPPING, STOPPING_SYNC (still ``playing``, as Bitwig's) |
| ``recording`` | MIDI record is on and the sequence is the selected one |
| selected slot / scene | ``zynseq.phrase`` (and the active chain) |
| ``track <p>/createClip/<s> <beats>`` | create and add the pattern if there is none, set its length in beats |
| ``track <p>/clip/<s>/record`` | select phrase, sequence and pattern, enable MIDI record, start the sequence |
| ``track <p>/clip/<s>/select`` | select it |
| ``track <p>/clip/<s>/launch``, ``launchAlt`` | start it if it is stopped, keep it playing if a stop is queued; a playing or starting sequence stays as it is (zynseq's launch is a toggle, Bitwig's relaunch does not stop); ``launchRelease``: nothing (zynseq launches on press) |
| ``track <p>/clip/<s>/remove`` | stop it at once, clear its pattern (the pattern and its length stay, zynseq has one per sequence) |
| ``track <p>/clip/<s>/duplicate`` | copy its pattern into the slot below (the reference host's ``duplicateClip``) |
| ``track <p>/clip/<s>/paste <track>/<slot>`` | copy the pattern of the source slot into it |
| ``track <p>/stop`` | stop the chain's sequences in all phrases at the next sync point (STOPPING_SYNC, as zynseq's toggle; a starting one at once) |
| ``track <p>/stopAlt`` | stop them at once |
| ``scene stop``, ``stopAlt`` | the same for all sequences and the phrase launchers of the scene (stop all clips) |
| ``scene <s>/select``, ``launch``, ``launchAlt`` | select the phrase; start its phrase launcher (channel 32) unless it plays or starts (``launchRelease``: nothing) |
| ``scene <s>/remove``, ``duplicate`` | ``zynseq.remove_phrase`` (zynseq stops all sequences, keeps at least one phrase), ``duplicate_phrase`` (the copy is inserted before the phrase, zynseq's way) |
| ``project createScene`` | ``zynseq.insert_phrase`` at the end, selected |
| ``project createSceneFromPlayingLauncherClips`` | a new phrase at the end with copies of the playing patterns, selected |
| ``record`` (toggle) | MIDI record on/off for the cursor slot (the active chain in the selected phrase) |
| ``transport overdub/launcher <0|1>``, ``/overdub/launcher`` | MIDI record on/off for the selected pattern |
| ``/overdub/arranger`` | 0, ``transport overdub/arranger`` ignored (no arranger) |
| ``/record`` | MIDI record is on |

The adapter calls libseq: ``zynseq.add_pattern`` passes 6 arguments to ``addPattern``, which has 7
(analysis §9 Z12).

Not mapped: an empty slot's launch does not stop the track (Bitwig's stop button of an empty
slot): every zynseq slot is a sequence with a pattern, and the core launches right after
``createClip`` and ``record``, on the still empty pattern. Not mapped either: the audio recorder
and the SMF recorder (doc/ZYNTHIAN_PLAN.md §3.3), the restart of a playing sequence on launch,
the alternative launch and stop quantization (``launchAlt`` is ``launch``), slot and scene names
and colors (the chain's), clippy (audio) launchers.
"""

import ctypes
import logging
from time import monotonic

from zyngine.ctrldev.dbm_zynthian.transport import STOPPED

logger = logging.getLogger("dbm_zynthian")

# zynseq/constants.h: the play states
PLAYING = 1
STARTING = 2
STOPPING = 3
FORCED_STOP = 4
STOPPING_SYNC = 5
CHILD_PLAYING = 6  # a phrase launcher whose sequences play
CHILD_STOPPING = 8  # a phrase launcher whose sequences stop
# The play states which play (a queued stop still plays), which wait to stop, which do not play
PLAYING_STATES = (PLAYING, STOPPING, STOPPING_SYNC, CHILD_PLAYING, CHILD_STOPPING)
STOP_QUEUED_STATES = (STOPPING, STOPPING_SYNC, CHILD_STOPPING)
STOPPED_STATES = (STOPPED, FORCED_STOP)
NO_PATTERN = 0xFFFFFFFF
NUM_SEQUENCE_CHANNELS = 16
# zynseq.py: the sequence channel of a phrase's launcher; the sequences of a phrase (MIDI and
# clippy's audio launchers)
PHRASE_CHANNEL = 32
NUM_PHRASE_SEQUENCES = 32
# How often zynseq's Python state is refreshed at most after the core changed patterns (s)
REFRESH_SECONDS = 0.5


def slot_states(state):
    """(playing, play queued, stop queued) of a zynseq play state."""
    return state in PLAYING_STATES, state == STARTING, state in STOP_QUEUED_STATES


class ZynLauncher:

    def __init__(self, model, zynseq):
        self.model = model
        self.zynseq = zynseq
        self.dirty = False
        self.last_refresh = 0.0

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
        if track.is_master or track.is_effect or track.chain is None:
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
        self.start(phrase, chan)
        return True

    def start(self, phrase, chan):
        """Start a stopped sequence, keep a stopping one playing (zynseq's toggle where it
        starts); a playing or starting one stays as it is."""
        state = self.play_state(phrase, chan)
        if state in STOPPED_STATES:
            self.libseq.setPlayState(self.scene, phrase, chan, STARTING)
        elif state in (STOPPING, STOPPING_SYNC):
            self.libseq.setPlayState(self.scene, phrase, chan, PLAYING)

    def stop_sequence(self, phrase, chan, immediately=False):
        """Stop a sequence at the next sync point (as zynseq's toggle: STOPPING_SYNC, a phrase
        launcher CHILD_STOPPING), a starting one and with immediately at once."""
        state = self.play_state(phrase, chan)
        if state in STOPPED_STATES:
            return
        if immediately or state == STARTING:
            self.libseq.setPlayState(self.scene, phrase, chan, STOPPED)
        elif state == PLAYING:
            self.libseq.setPlayState(self.scene, phrase, chan, CHILD_STOPPING if chan == PHRASE_CHANNEL else STOPPING_SYNC)
        elif state == CHILD_PLAYING:
            self.libseq.setPlayState(self.scene, phrase, chan, CHILD_STOPPING)

    def stop_track(self, track, immediately=False):
        """``track <p>/stop|stopAlt``: the chain's sequences in all phrases."""
        chan = self.sequence_of(track)
        if chan is None:
            return False
        for phrase in range(self.num_phrases()):
            self.stop_sequence(phrase, chan, immediately)
        return True

    def stop_all(self, immediately=False):
        """``scene stop|stopAlt`` (stop all clips): all sequences and phrase launchers of the
        current scene (the phrase launchers first, else they start their sequences again)."""
        for phrase in range(self.num_phrases()):
            self.stop_sequence(phrase, PHRASE_CHANNEL, immediately)
            for chan in range(NUM_PHRASE_SEQUENCES):
                self.stop_sequence(phrase, chan, immediately)
        return True

    def remove(self, track, phrase):
        """``clip/<s>/remove``: stop the sequence at once and clear its pattern (zynseq keeps one
        pattern per sequence: it stays, with its length)."""
        chan = self.sequence_of(track)
        if chan is None or not 0 <= phrase < self.num_phrases():
            return False
        self.stop_sequence(phrase, chan, immediately=True)
        pattern = self.pattern(phrase, chan)
        if pattern is not None:
            self.libseq.clearPattern(ctypes.c_uint32(pattern))
            self.refresh()
        return True

    def copy(self, source_phrase, source_chan, phrase, chan):
        """Copy the pattern of a sequence into another (its notes and length)."""
        source = self.pattern(source_phrase, source_chan)
        if source is None or (source_phrase, source_chan) == (phrase, chan):
            return False
        target = self.ensure_pattern(phrase, chan)
        self.libseq.copyPattern(ctypes.c_uint32(source), ctypes.c_uint32(target))
        self.refresh()
        return True

    def duplicate(self, track, phrase):
        """``clip/<s>/duplicate``: a copy into the slot below, if there is one (the reference
        host's ``duplicateClip``); nothing for an empty slot."""
        chan = self.sequence_of(track)
        if chan is None or not 0 <= phrase < self.num_phrases():
            return False
        if self.has_content(phrase, chan) and phrase + 1 < self.num_phrases():
            self.copy(phrase, chan, phrase + 1, chan)
        return True

    def paste(self, track, phrase, source_track, source_phrase):
        """``clip/<s>/paste <track>/<slot>``: the source slot's pattern into the slot."""
        chan, source_chan = self.sequence_of(track), self.sequence_of(source_track)
        count = self.num_phrases()
        if chan is None or source_chan is None or not 0 <= phrase < count or not 0 <= source_phrase < count:
            return False
        self.copy(source_phrase, source_chan, phrase, chan)
        return True

    def select_scene(self, phrase):
        if not 0 <= phrase < self.num_phrases():
            return False
        self.zynseq.select_phrase(phrase)
        return True

    def launch_scene(self, phrase):
        """Start the phrase (zynseq's phrase launcher) unless it plays or starts, and select it;
        a queued stop is taken back."""
        if not self.select_scene(phrase):
            return False
        state = self.play_state(phrase, PHRASE_CHANNEL)
        if state in (STOPPING, STOPPING_SYNC):
            self.libseq.setPlayState(self.scene, phrase, PHRASE_CHANNEL, PLAYING)
        elif state not in (PLAYING, STARTING):
            # Stopped, or only some of its sequences play (CHILD_PLAYING, CHILD_STOPPING)
            self.libseq.setPlayState(self.scene, phrase, PHRASE_CHANNEL, STARTING)
        return True

    def create_scene(self):
        """``project createScene``: a new phrase at the end, selected (as Bitwig's)."""
        self.zynseq.insert_phrase(self.scene, self.num_phrases())
        self.zynseq.select_phrase(self.num_phrases() - 1)
        return True

    def create_scene_from_playing(self):
        """``project createSceneFromPlayingLauncherClips``: a new phrase at the end with a copy
        of the playing pattern of each channel, selected."""
        playing = {}
        for phrase in range(self.num_phrases()):
            for chan in range(NUM_SEQUENCE_CHANNELS):
                if chan not in playing and self.play_state(phrase, chan) in PLAYING_STATES:
                    playing[chan] = phrase
        self.create_scene()
        new_phrase = self.num_phrases() - 1
        for chan, phrase in playing.items():
            self.copy(phrase, chan, new_phrase, chan)
        return True

    def remove_scene(self, phrase):
        """``scene <s>/remove``: ``zynseq.remove_phrase`` (it keeps the last phrase; libseq stops
        all sequences first)."""
        if not 0 <= phrase < self.num_phrases():
            return False
        self.zynseq.remove_phrase(self.scene, phrase)
        return True

    def duplicate_scene(self, phrase):
        """``scene <s>/duplicate``: ``zynseq.duplicate_phrase`` (a stopped copy, inserted before
        the phrase)."""
        if not 0 <= phrase < self.num_phrases():
            return False
        self.zynseq.duplicate_phrase(self.scene, phrase)
        return True

    def record(self, track, phrase):
        """Record into the slot: select it and its pattern, MIDI record on, start it."""
        chan = self.sequence_of(track)
        if chan is None or not 0 <= phrase < self.num_phrases():
            return False
        self.select(track, phrase)
        self.set_recording(True, phrase, chan)
        self.start(phrase, chan)
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

    def content_changed(self):
        """The core changed a pattern: zynseq's Python state is refreshed with the next report
        (``flush``), at most every REFRESH_SECONDS."""
        self.dirty = True

    def flush(self):
        if self.dirty and monotonic() - self.last_refresh >= REFRESH_SECONDS:
            self.dirty = False
            self.last_refresh = monotonic()
            self.refresh()

    def refresh(self):
        """zynseq's Python state (its GUI and the stock drivers read it) after a change."""
        try:
            self.zynseq.refresh_state()
        except Exception as e:
            logger.debug("zynseq refresh_state: %s", e)
