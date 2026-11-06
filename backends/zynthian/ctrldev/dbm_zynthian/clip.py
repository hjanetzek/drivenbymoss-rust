# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: zynseq patterns as the core's cursor clip and step grids
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

"""The pattern of the selected zynseq sequence as the core's cursor clip (doc/HOST_BRIDGE_GUIDE.md
§3.7, §4.7; doc/ZYNTHIAN_PLAN.md §3.5).

The cursor clip is the pattern (track 0, position 0) of the cursor slot (launcher.py: the active
chain's sequence in the selected phrase), the clip of zynthian's pattern editor. ``ZynseqClip``
is a ``dbm_host.clip.Clip`` whose notes are read from that pattern and whose changes are written
to it, so the reference host's step grids (``dbm_host.clip.NoteGrid``: the steps in the core's
format, toggle, set, clear, rows, columns, moving, the attributes, step size, scrolling) work on
it unchanged.

Time. A zynseq pattern has ``S`` steps per beat (``getStepsPerBeat``) and ``beats`` beats; an
event has an integer ``position`` in steps, a fractional ``offset`` (0..1 of a step) and a
``duration`` in steps. The core's grid has its own step size in beats (``stepSize``, set by the
sequencer views). A note at ``start`` beats is the event at ``position = floor(start * S)``,
``offset = start * S - position``; its duration is ``duration * S`` steps. So every core
resolution maps, also those that don't divide zynseq's (a 1/3 beat on a 4-step pattern is
position 1, offset 0.333). zynseq's resolution is the pattern's (the user's choice in the
pattern editor); the adapter never changes it (``setStepsPerBeat`` would not move the events
either, analysis §9 Z13). Limit: zynseq has one note per key and step (a new note deletes the
overlapping notes of its key, notes are addressed by step and key), so with a core step finer
than zynseq's step a key holds one note per zynseq step; a second one replaces the first.

The note attributes zynseq has: velocity (0..127 as 0..1), duration, chance (``play_chance``,
always enabled), muted (``play_freq`` 0 = never plays, 1 = every loop). The rest of the core's 24
values (velocity spread, release velocity, pressure, timbre, pan, transpose, gain, occurrence,
recurrence, repeat) do not exist: they are reported with the values of a new Bitwig note, and
setting them changes nothing, so the core shows those values again. zynseq has no per-note MIDI
channel: the notes are reported on the channel the core last used (0 at first). The selection of
notes (the core's step editing) is kept by the adapter.

zynseq's pattern functions work on the globally selected pattern (``selectPattern``), which the
pattern editor uses too. The adapter selects the cursor pattern for each read or change and
selects the previous one again (``selected_pattern``). Usually they are the same: the pattern
editor and the launcher edit the selected sequence's pattern. The C functions mark the pattern as
modified, which the pattern editor polls (``isPatternModified``, read and cleared by it; the
adapter never calls it) and redraws.
"""

import ctypes
import logging
from contextlib import contextmanager
from dataclasses import dataclass

from dbm_host.clip import EPSILON, Clip, Note

logger = logging.getLogger("dbm_zynthian")

MIDI_NOTE_ON = 0x90
NO_PATTERN = 0xFFFFFFFF


class StepEvent(ctypes.Structure):
    """zynseq's StepEvent (pattern.h, packed to 1), as zynseq.py's event_data."""
    _pack_ = 1
    _fields_ = [
        ("position", ctypes.c_uint32), ("offset", ctypes.c_float), ("duration", ctypes.c_float),
        ("command", ctypes.c_uint8), ("val1_start", ctypes.c_uint8), ("val2_start", ctypes.c_uint8),
        ("val1_end", ctypes.c_uint8), ("val2_end", ctypes.c_uint8), ("stut_speed", ctypes.c_uint8),
        ("stut_velfx", ctypes.c_uint8), ("stut_ramp", ctypes.c_uint8), ("play_freq", ctypes.c_uint8),
        ("stut_freq", ctypes.c_uint8), ("play_chance", ctypes.c_float), ("stut_chance", ctypes.c_float),
    ]


def event_class(libseq):
    """The structure getEventDataAt takes: zynseq.py declares its own class in argtypes."""
    try:
        return libseq.getEventDataAt.argtypes[1]._type_
    except (AttributeError, TypeError, IndexError):
        return StepEvent


@dataclass
class ZynNote(Note):
    """A note of a zynseq pattern; the attributes zynseq has are written to it when they are set
    (``dbm_host.clip.set_attribute``, the NoteGrid's commands)."""

    position: int = 0
    offset: float = 0.0

    def __post_init__(self):
        object.__setattr__(self, "_clip", None)

    def __setattr__(self, name, value):
        clip = getattr(self, "_clip", None)
        old = getattr(self, name, None)
        object.__setattr__(self, name, value)
        if clip is not None and old != value:
            clip.note_changed(self, name, old)


class ZynNoteList(list):
    """The notes of a ZynseqClip: ``clear`` (NoteGrid.clear_all) clears the pattern."""

    def __init__(self, clip, notes):
        super().__init__(notes)
        self.clip = clip

    def clear(self):
        self.clip.clear_notes()
        super().clear()


class ZynseqClip(Clip):
    """The pattern ``pattern`` of a zynseq sequence as a Clip (see the module docstring)."""

    def __init__(self, model, pattern, channel=0):
        self.model = model
        self.libseq = model.launcher.libseq
        self.pattern = pattern
        self.channel = channel
        super().__init__(None, length=0.0)
        self.read()

    # The selected pattern

    @contextmanager
    def selected_pattern(self):
        libseq = self.libseq
        previous = libseq.getPatternIndex() & 0xFFFFFFFF
        if previous != self.pattern:
            libseq.selectPattern(ctypes.c_uint32(self.pattern))
        try:
            yield libseq
        finally:
            if previous != self.pattern and previous != NO_PATTERN:
                libseq.selectPattern(ctypes.c_uint32(previous))

    # Reading

    def read(self):
        with self.selected_pattern() as libseq:
            self.steps_per_beat = max(1, int(libseq.getStepsPerBeat()))
            self.beats = int(libseq.getBeatsInPattern(ctypes.c_uint32(self.pattern)))
            event = event_class(libseq)()
            notes = []
            index = 0
            while libseq.getEventDataAt(ctypes.c_uint32(index), ctypes.byref(event)) >= 0:
                index += 1
                if event.command != MIDI_NOTE_ON:
                    continue
                notes.append(self.make_note(event))
        self.notes = ZynNoteList(self, notes)
        self.play_start = 0.0
        self.play_stop = float(self.beats)
        self.loop = True
        self.loop_start = 0.0
        self.loop_length = float(self.beats)
        self.shuffle = False
        selected = self.model.selected_notes.get(self.pattern, set())
        for note in self.notes:
            object.__setattr__(note, "selected", (note.position, note.key) in selected)
            object.__setattr__(note, "_clip", self)

    def make_note(self, event):
        spb = self.steps_per_beat
        note = ZynNote(channel=self.channel, key=event.val1_start,
                       start=(event.position + event.offset) / spb,
                       duration=max(EPSILON, event.duration / spb),
                       velocity=event.val2_start / 127.0,
                       muted=event.play_freq == 0,
                       chance=min(1.0, max(0.0, float(event.play_chance))),
                       position=int(event.position), offset=float(event.offset))
        return note

    def position_of(self, start):
        """The zynseq position and offset of a start in beats (see the module docstring)."""
        steps = max(0.0, float(start)) * self.steps_per_beat
        position = int(steps + EPSILON)
        offset = steps - position
        if offset < EPSILON:
            offset = 0.0
        return position, offset

    # Changes (dbm_host.clip.Clip, used by NoteGrid)

    def add_note(self, note):
        """A new note at the note's start (it replaces the notes of its key it overlaps, as
        zynseq does); it is the only selected one."""
        position, offset = self.position_of(note.start)
        if position >= self.beats * self.steps_per_beat or not 0 <= note.key < 128:
            return None
        velocity = max(1, min(127, round(note.velocity * 127)))
        with self.selected_pattern() as libseq:
            libseq.addNote(ctypes.c_uint32(position), ctypes.c_uint8(note.key), ctypes.c_uint8(velocity),
                           ctypes.c_float(note.duration * self.steps_per_beat), ctypes.c_float(offset))
        self.model.selected_notes[self.pattern] = {(position, note.key)}
        self.changed()
        self.read()
        for added in self.notes:
            if added.position == position and added.key == note.key:
                return added
        return None

    def remove(self, notes):
        notes = list(notes)
        if not notes:
            return
        with self.selected_pattern() as libseq:
            for note in notes:
                libseq.removeNote(ctypes.c_uint32(note.position), ctypes.c_uint8(note.key))
        selected = self.model.selected_notes.get(self.pattern, set())
        selected -= {(note.position, note.key) for note in notes}
        self.changed()
        self.read()

    def select_only(self, selected):
        self.model.selected_notes[self.pattern] = set() if selected is None else {(selected.position, selected.key)}
        for note in self.notes:
            object.__setattr__(note, "selected", note is selected)

    def clear_notes(self):
        with self.selected_pattern() as libseq:
            libseq.clearNotes()
        self.model.selected_notes[self.pattern] = set()
        self.changed()

    def transpose(self, semitones):
        with self.selected_pattern() as libseq:
            libseq.transpose(ctypes.c_int8(int(semitones)))
        self.read()

    def duplicate_content(self):
        """The notes repeated after the pattern, the pattern twice as long."""
        beats = self.beats
        if beats <= 0:
            return
        notes = list(self.notes)
        self.set_length(2 * beats)
        shift = beats * self.steps_per_beat
        with self.selected_pattern() as libseq:
            for note in notes:
                libseq.addNote(ctypes.c_uint32(note.position + shift), ctypes.c_uint8(note.key),
                               ctypes.c_uint8(max(1, min(127, round(note.velocity * 127)))),
                               ctypes.c_float(note.duration * self.steps_per_beat), ctypes.c_float(note.offset))
        self.changed()
        self.read()

    def set_length(self, beats):
        """The length of the pattern in whole beats (zynseq's unit), at least 1."""
        beats = max(1, int(round(beats)))
        self.libseq.setBeatsInPattern(ctypes.c_uint32(self.pattern), ctypes.c_uint32(beats))
        self.changed()
        self.read()

    def note_changed(self, note, name, old):
        """An attribute of a note was set: write the ones zynseq has."""
        libseq = self.libseq
        step, key = ctypes.c_uint32(note.position), ctypes.c_uint8(note.key)
        if name == "velocity":
            with self.selected_pattern():
                libseq.setNoteVelocity(step, key, ctypes.c_uint8(max(1, min(127, round(note.velocity * 127)))))
        elif name == "chance":
            with self.selected_pattern():
                libseq.setNotePlayChance(step, key, ctypes.c_float(note.chance))
        elif name == "muted":
            with self.selected_pattern():
                libseq.setNotePlayFreq(step, key, ctypes.c_uint8(0 if note.muted else 1))
        elif name == "duration":
            # No setNoteDuration in zynseq: the note again, which replaces it and keeps its
            # chance, frequency and stutter
            with self.selected_pattern():
                libseq.addNote(step, key, ctypes.c_uint8(max(1, min(127, round(note.velocity * 127)))),
                               ctypes.c_float(max(EPSILON, note.duration) * self.steps_per_beat), ctypes.c_float(note.offset))
        elif name == "key":
            # moveY: the note on another key
            with self.selected_pattern():
                libseq.removeNote(step, ctypes.c_uint8(old))
                libseq.addNote(step, key, ctypes.c_uint8(max(1, min(127, round(note.velocity * 127)))),
                               ctypes.c_float(note.duration * self.steps_per_beat), ctypes.c_float(note.offset))
            self.model.selected_notes[self.pattern] = {(note.position, note.key)}
        else:
            return  # Not in zynseq (pan, gain, timbre, …): the core sees the reported value again
        self.changed()

    def changed(self):
        self.model.launcher.content_changed()

    # The play position

    def playing_step(self, grid, phrase, chan):
        """The grid step of the pattern's play position, -1 if the sequence does not play."""
        launcher = self.model.launcher
        if launcher.play_state(phrase, chan) not in (1, 3) or grid.step_size <= 0:
            return -1
        libseq = self.libseq
        libseq.selectSequence(launcher.scene, phrase, chan)
        with self.selected_pattern():
            playhead = int(libseq.getPatternPlayhead()) & 0xFFFFFFFF
        beats = playhead / self.steps_per_beat
        return int(beats // grid.step_size + EPSILON) - grid.scroll
