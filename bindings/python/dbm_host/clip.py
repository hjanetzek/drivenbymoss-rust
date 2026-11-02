# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The clips of the simulated DAW and their step grids: the notes of a launcher clip, the clip
properties, and the step grids of the cursor clip in the sizes the controller asks for.

Implements the DAW side of HOST_BRIDGE_GUIDE.md §3.6 "Cursor clip and step grid" (``/clip/…``,
``/clip/grid/<steps>x<rows>/playingStep``, ``/clip/grid/<id>/step/<channel>/<x>/<y>``) for the
Reporter and the commands of §4.7 for the CommandHandler.

Simulates the Bitwig extension (``BitwigNoteClip``: a launcher cursor clip per size of the model
setup's ``noteClips``, the step format of ``StepInfo::to_wire``) and the Java DrivenByMoss for
Bitwig (``CursorClipImpl``, ``StepInfoImpl``), with the Bitwig API behind them
(``PinnableCursorClip``: ``toggleStep``, ``setStep``, ``clearStep``, ``clearStepsAtX|Y``,
``clearSteps``, ``moveStep``, ``scrollToStep``, ``scrollStepsPageForward|Backwards``,
``setStepSize``; ``NoteStep`` and its setters).

The model:

- A clip has notes (``Note``: channel, key, start in beats, all attributes of a Bitwig
  ``NoteStep``). A step of a grid is the range ``[(scroll + x) * step_size, … + step_size)`` of
  the key ``y``: ``NOTE_ON`` if a note starts in it, ``NOTE_SUSTAIN`` if a note plays through
  it (with the attributes of that note), else empty.
- The gain is kept as the step reports it: Bitwig's ``NoteStep.gain ()`` is twice the scale of
  ``setGain`` and the extension reports ``gain () / 2`` (doc/STATUS.md, checked on Bitwig
  2026-10-31), so ``setGain (v)`` is reported as ``v``.

Known only from recordings (``osc:/clip/step/…`` of target/conformance/*-java.json, the
``/clip/grid/…/step`` model lines of tests/conformance/*/replay/*-rust.rec), Bitwig does not
document it:

- A note created by ``toggleStep`` (velocity v) has: velocity v / 127, duration = step size,
  release velocity 1.0, gain 0.0 (as reported), chance 1.0, and the chance, occurrence (0),
  recurrence (length 1, mask 1) and repeat (count 0) switches on; pressure, timbre, panorama,
  transpose, velocity spread and the repeat curves 0.
- The new note is selected, the other notes of the clip are deselected; removing a note with
  ``toggleStep`` deselects all notes (jam.noteseq.second-note, jam.noteseq.delete-note of
  maschine_jam note_sequencer).
- A new launcher clip plays from 0 to its length, loops over it, has shuffle on and an accent of
  0.5 (``/clip/shuffle 1``, ``/clip/accent 0.5`` for the fixture's clips). A clip which does not
  exist reports 0 / off for everything except start and loop start.

Simplifications:

- ``setFrom`` (``setStep``, the other values 100 ms later) sets all values at once.
- The recurrence, occurrence and repeats do not change what plays (nothing plays).
- ``playingStep``: the step of the play position of the playing cursor clip (from the start of
  the slot's playback, wrapped in its loop), relative to the scroll position of the grid; -1
  if the clip does not play. Bitwig counted the steps of the 0x0 grid as well.
"""

from copy import deepcopy
from dataclasses import dataclass

from .parameter import Parameter, Preferences

NOTE_EMPTY = 0
NOTE_ON = 1
NOTE_SUSTAIN = 2

# Positions closer than this are the same (beats)
EPSILON = 1e-6

# The occurrence types in the order of the core (NoteOccurrenceType::ALL, BitwigNoteClip.OCCURRENCES)
NUM_OCCURRENCES = 11


@dataclass
class Note:
    """A note of a clip with the attributes of a Bitwig ``NoteStep`` (see the module docstring
    for the defaults of a new note)."""

    channel: int
    key: int
    start: float
    duration: float = 0.25
    velocity: float = 1.0
    selected: bool = False
    muted: bool = False
    velocity_spread: float = 0.0
    release_velocity: float = 1.0
    pressure: float = 0.0
    timbre: float = 0.0
    pan: float = 0.0
    transpose: float = 0.0
    gain: float = 0.0
    """As reported (``gain () / 2``), the scale of ``setGain``."""
    chance_enabled: bool = True
    chance: float = 1.0
    occurrence_enabled: bool = True
    occurrence: int = 0
    recurrence_enabled: bool = True
    recurrence_length: int = 1
    recurrence_mask: int = 1
    repeat_enabled: bool = True
    repeat_count: int = 0
    repeat_curve: float = 0.0
    repeat_velocity_curve: float = 0.0
    repeat_velocity_end: float = 0.0

    @property
    def end(self) -> float:
        return self.start + self.duration

    def wire(self, state: int) -> str:
        """The step in the format of ``StepInfo::to_wire`` (BitwigNoteClip.format: 24 values,
        flags 0/1, doubles as Java's ``Double.toString``)."""
        values = [state, int(self.selected), int(self.muted), self.duration, self.velocity, self.velocity_spread, self.release_velocity,
                  self.pressure, self.timbre, self.pan, self.transpose, self.gain, int(self.chance_enabled), self.chance,
                  int(self.occurrence_enabled), self.occurrence, int(self.recurrence_enabled), self.recurrence_length, self.recurrence_mask,
                  int(self.repeat_enabled), self.repeat_count, self.repeat_curve, self.repeat_velocity_curve, self.repeat_velocity_end]
        return " ".join(java_double(value) if isinstance(value, float) else str(int(value)) for value in values)


def java_double(value: float) -> str:
    """Java's ``Double.toString`` for the values of the steps (``0.0``, ``1.0``, ``-1.0``,
    ``0.5039370078740157``): Python's shortest repr, with ``.0`` for integral values; the
    exponent forms of very small and large values are not needed here."""
    text = repr(float(value))
    return "0.0" if text == "-0.0" else text


def _clamp(value: float, low: float, high: float) -> float:
    return max(low, min(high, float(value)))


# The setters of the NoteStep attributes: the command name, the Note field, a converter
_ATTRIBUTES = {
    "muted": ("muted", lambda v: v > 0),
    "duration": ("duration", lambda v: max(EPSILON, float(v))),
    "velocity": ("velocity", lambda v: _clamp(v, 0.0, 1.0)),
    "velocitySpread": ("velocity_spread", lambda v: _clamp(v, 0.0, 1.0)),
    "releaseVelocity": ("release_velocity", lambda v: _clamp(v, 0.0, 1.0)),
    "pressure": ("pressure", lambda v: _clamp(v, 0.0, 1.0)),
    "timbre": ("timbre", lambda v: _clamp(v, -1.0, 1.0)),
    "pan": ("pan", lambda v: _clamp(v, -1.0, 1.0)),
    "transpose": ("transpose", float),
    "gain": ("gain", lambda v: _clamp(v, 0.0, 1.0)),
    "chanceEnabled": ("chance_enabled", lambda v: v > 0),
    "chance": ("chance", lambda v: _clamp(v, 0.0, 1.0)),
    "occurrenceEnabled": ("occurrence_enabled", lambda v: v > 0),
    "occurrence": ("occurrence", lambda v: max(0, min(NUM_OCCURRENCES - 1, int(v)))),
    "recurrenceEnabled": ("recurrence_enabled", lambda v: v > 0),
    "recurrenceLength": ("recurrence_length", int),
    "recurrenceMask": ("recurrence_mask", int),
    "repeatEnabled": ("repeat_enabled", lambda v: v > 0),
    "repeatCount": ("repeat_count", int),
    "repeatCurve": ("repeat_curve", float),
    "repeatVelocityCurve": ("repeat_velocity_curve", float),
    "repeatVelocityEnd": ("repeat_velocity_end", float),
}
ATTRIBUTES = tuple(_ATTRIBUTES)


def set_attribute(note: Note, attribute: str, value: float) -> bool:
    """A ``NoteStep`` setter by its command name (BitwigNoteClip.updateAttribute); False if the
    attribute is unknown."""
    if attribute not in _ATTRIBUTES:
        return False
    name, convert = _ATTRIBUTES[attribute]
    setattr(note, name, convert(value))
    return True


def update_from_wire(note: Note, values: list[str]) -> None:
    """All attributes of a step from the 24 values of the core (BitwigNoteClip.update,
    ``sendClipData``), except the state and the selection."""
    def number(index: int) -> float:
        try:
            return float(values[index])
        except (IndexError, ValueError):
            return 0.0
    for attribute, index in (("muted", 2), ("duration", 3), ("velocity", 4), ("velocitySpread", 5), ("releaseVelocity", 6), ("pressure", 7),
                             ("timbre", 8), ("pan", 9), ("transpose", 10), ("gain", 11), ("chanceEnabled", 12), ("chance", 13),
                             ("occurrenceEnabled", 14), ("occurrence", 15), ("recurrenceEnabled", 16), ("recurrenceMask", 18),
                             ("repeatEnabled", 19), ("repeatCount", 20), ("repeatCurve", 21), ("repeatVelocityCurve", 22), ("repeatVelocityEnd", 23)):
        set_attribute(note, attribute, number(index))
    note.recurrence_length = max(1, int(number(17)))


class Clip:
    """A launcher clip (Bitwig ``Clip``): notes and the clip properties."""

    def __init__(self, preferences: Preferences, length: float = 4.0):
        self.notes: list[Note] = []
        self.play_start = 0.0
        self.play_stop = float(length)
        self.loop = True
        self.loop_start = 0.0
        self.loop_length = float(length)
        self.shuffle = True
        self.accent = Parameter(preferences, 0.5, name="Accent")
        self.name = ""

    def copy(self) -> "Clip":
        return deepcopy(self)

    # Notes

    def notes_at(self, channel: int, key: int, start: float, end: float) -> list[Note]:
        """The notes of the channel and key which start in [start, end)."""
        return [note for note in self.notes if note.channel == channel and note.key == key and start - EPSILON <= note.start < end - EPSILON]

    def state(self, channel: int, key: int, start: float, end: float) -> tuple[int, Note | None]:
        """The state of the step [start, end) of the key: NOTE_ON with the note which starts in it,
        NOTE_SUSTAIN with the note which plays through it, else NOTE_EMPTY."""
        starting = self.notes_at(channel, key, start, end)
        if starting:
            return NOTE_ON, starting[0]
        for note in self.notes:
            if note.channel == channel and note.key == key and note.start < start - EPSILON and note.end > start + EPSILON:
                return NOTE_SUSTAIN, note
        return NOTE_EMPTY, None

    def select_only(self, selected: Note | None) -> None:
        for note in self.notes:
            note.selected = note is selected

    def add_note(self, note: Note) -> Note:
        """A new note replaces the notes which start in its step; it is the only selected one."""
        for other in self.notes_at(note.channel, note.key, note.start, note.start + max(note.duration, EPSILON)):
            self.notes.remove(other)
        self.notes.append(note)
        self.select_only(note)
        return note

    def remove(self, notes: list[Note]) -> None:
        for note in notes:
            if note in self.notes:
                self.notes.remove(note)

    def transpose(self, semitones: int) -> None:
        """``transpose``: move all notes; notes beyond 0..127 are dropped (not observed)."""
        for note in self.notes:
            note.key += int(semitones)
        self.notes = [note for note in self.notes if 0 <= note.key < 128]

    def duplicate_content(self) -> None:
        """``duplicateContent``: the notes of the loop are repeated after it, the loop doubles."""
        length = self.loop_length
        if length <= 0:
            return
        copies = []
        for note in self.notes:
            if self.loop_start - EPSILON <= note.start < self.loop_start + length - EPSILON:
                copy = deepcopy(note)
                copy.start += length
                copy.selected = False
                copies.append(copy)
        self.notes.extend(copies)
        self.loop_length = length * 2
        if self.play_stop < self.loop_start + self.loop_length:
            self.play_stop = self.loop_start + self.loop_length


class NoteGrid:
    """A step grid of the cursor clip (a launcher cursor clip of ``num_steps`` x ``num_rows``,
    BitwigNoteClip): its own step size and scroll position, on the clip the cursor clip shows
    (``clip`` is a callable which returns it, None without one)."""

    def __init__(self, num_steps: int, num_rows: int, clip):
        self.num_steps = int(num_steps)
        self.num_rows = int(num_rows)
        self.clip = clip
        self.step_size = 0.25
        self.scroll = 0
        """The first step of the window."""
        self.key_offset = 0
        """The first key of the window (``scrollToKey``; the core does not move it)."""

    @property
    def id(self) -> str:
        return f"{self.num_steps}x{self.num_rows}"

    def range(self, x: int) -> tuple[float, float]:
        start = (self.scroll + x) * self.step_size
        return start, start + self.step_size

    def key(self, y: int) -> int:
        return self.key_offset + y

    def step(self, channel: int, x: int, y: int) -> tuple[int, Note | None]:
        clip = self.clip()
        if clip is None or not (0 <= x < self.num_steps and 0 <= y < self.num_rows):
            return NOTE_EMPTY, None
        return clip.state(channel, self.key(y), *self.range(x))

    def steps(self, channels: int = 16) -> dict[tuple[int, int, int], str]:
        """All steps which are not empty: (channel, x, y) -> the reported line."""
        clip = self.clip()
        result = {}
        if clip is None:
            return result
        for note in clip.notes:
            y = note.key - self.key_offset
            if not 0 <= y < self.num_rows or not 0 <= note.channel < channels:
                continue
            first = int((note.start + EPSILON) // self.step_size) - self.scroll
            last = int((note.end - EPSILON) // self.step_size) - self.scroll
            for x in range(max(0, first), min(self.num_steps - 1, last) + 1):
                state, owner = clip.state(note.channel, note.key, *self.range(x))
                if owner is not None:
                    result[(note.channel, x, y)] = owner.wire(state)
        return result

    # Commands (BitwigNoteClip.handle)

    def set_step_size(self, size: float) -> None:
        if size > 0:
            self.step_size = float(size)

    def scroll_to(self, step: int) -> None:
        self.scroll = max(0, int(step))

    def scroll_page(self, direction: int) -> None:
        """``scrollStepsPageForward / Backwards``: by the number of steps of the window."""
        self.scroll = max(0, self.scroll + direction * self.num_steps)

    def toggle(self, channel: int, x: int, y: int, velocity: int) -> None:
        """``toggleStep``: remove the notes which start in the step (deselects all notes), else
        create a note of the step size (see the module docstring)."""
        clip = self.clip()
        if clip is None:
            return
        start, end = self.range(x)
        starting = clip.notes_at(channel, self.key(y), start, end)
        if starting:
            clip.remove(starting)
            clip.select_only(None)
        else:
            clip.add_note(Note(channel, self.key(y), start, self.step_size, _clamp(velocity / 127.0, 0.0, 1.0)))

    def set(self, channel: int, x: int, y: int, velocity: int, duration: float) -> Note | None:
        """``setStep``: a new note with the velocity (0..127) and duration."""
        clip = self.clip()
        if clip is None:
            return None
        start, _ = self.range(x)
        return clip.add_note(Note(channel, self.key(y), start, max(EPSILON, float(duration)), _clamp(velocity / 127.0, 0.0, 1.0)))

    def clear(self, channel: int, x: int, y: int) -> None:
        clip = self.clip()
        if clip is not None:
            clip.remove(clip.notes_at(channel, self.key(y), *self.range(x)))

    def clear_row(self, channel: int, y: int) -> None:
        """``clearStepsAtY``: all notes of the key (in the whole clip)."""
        clip = self.clip()
        if clip is not None:
            clip.remove([note for note in clip.notes if note.channel == channel and note.key == self.key(y)])

    def clear_column(self, channel: int, x: int) -> None:
        """``clearStepsAtX``: all notes which start in the step."""
        clip = self.clip()
        if clip is not None:
            start, end = self.range(x)
            clip.remove([note for note in clip.notes if note.channel == channel and start - EPSILON <= note.start < end - EPSILON])

    def clear_all(self) -> None:
        clip = self.clip()
        if clip is not None:
            clip.notes.clear()

    def move_y(self, channel: int, x: int, y: int, row: int) -> None:
        """``moveStep (channel, x, y, 0, row - y)``: the notes of the step to another key."""
        clip = self.clip()
        if clip is None:
            return
        for note in clip.notes_at(channel, self.key(y), *self.range(x)):
            note.key = self.key(row)

    def note(self, channel: int, x: int, y: int) -> Note | None:
        """``getStep``: the note which starts in the step, None for a step without one (its
        setters do nothing)."""
        state, note = self.step(channel, x, y)
        return note if state == NOTE_ON else None


__all__ = ["ATTRIBUTES", "Clip", "NOTE_EMPTY", "NOTE_ON", "NOTE_SUSTAIN", "Note", "NoteGrid", "java_double", "set_attribute", "update_from_wire"]
