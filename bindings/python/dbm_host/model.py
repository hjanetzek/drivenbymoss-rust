# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The simulated DAW (``SimDaw``): plain state and operations, it knows nothing about the core.

Implements the DAW side of the areas of HOST_BRIDGE_GUIDE.md §3: 3.1 transport, 3.3 tracks,
effect tracks, master and sends, 3.4 scenes and slots. The Reporter (§2) reads this state, the
CommandHandler (§4) calls its operations.

Simulates the Java DrivenByMoss for Bitwig (``bitwig/framework/daw``) and the Bitwig API behind
it: ``TransportImpl`` (Transport), ``ChannelImpl``, ``TrackImpl``, ``MasterTrackImpl``,
``SendImpl`` (Channel, Track, MasterTrack, Send), ``CursorTrackImpl`` (the cursor track, pinning),
``EffectTrackBankImpl``, ``SceneImpl``, ``SlotImpl`` (Scene, ClipLauncherSlot), ``ModelImpl``
(the track list with effect tracks and the master).

Extension points for the later steps (PYTHON_HOST_PLAN.md §5):

- ``Channel.devices``: the device chain of a channel (``device.DeviceChain``: names,
  remote-control pages, layers and drum pads, the EQ+), for ``/device``, ``/primary``, ``/drum``
  and ``/eq``.
- ``Slot.clip``: the clip of a slot (``clip.Clip``: notes, loop, play range), for ``/clip``; the
  cursor clip (``SimDaw.cursor_clip``) is shown in the step grids (``clip.NoteGrid``).
- ``SimDaw.markers``: the cue markers (``marker.Markers``).
- ``SimDaw.browser``, ``SimDaw.project``, ``SimDaw.groove``: not modelled yet (None).
- ``SimDaw.application`` has the undo history and the panel layout, ``SimDaw.arranger`` and
  ``SimDaw.mixer`` their visibility flags.
- ``SimDaw.note_input``: the default note input of the controller with its arpeggiator (the note
  repeat).
- ``SimDaw.add_time_listener``: called by ``advance`` with the elapsed time, e.g. for the VU and
  the playing notes.

Simplifications (each a deliberate choice, not a fit to a scenario):

- Clip launching is quantized to the next bar while the transport plays; launching while stopped
  starts the transport and the clip at once. Launch quantization settings are ignored.
- ``Transport.stop`` moves the play position back to the play start position.
- The monitoring of AUTO is on while the track is armed.
- New sends are disabled (``isEnabled`` 0), as Bitwig reported them for the fixture.
- ``track addTrack``: the name and color are set at once (Bitwig: 300 ms later).
- Undo: a change of mute, solo, a mixer value (volume, pan, send) or the tempo is an edit of the
  project (``Application.edit``), which makes undo possible; the history is not simulated.
- The cursor clip (Bitwig's launcher cursor clip of the cursor track) is the clip of the
  selected slot if that is on the cursor track, else of the slot last selected on the track,
  else of the first slot with a clip. Known only from the Java traces: ``osc:/clip/exists`` and
  ``color`` followed the selected track to its clip in slot 1 while slot 1 of another track was
  selected (generic_flexi track, ``flexi.track.select``; launchcontrol_xl track_mix,
  ``lcxl.mix.track-page``).
- Auto-arm (``Preferences.auto_arm``): selecting a track which can hold notes arms it and
  disarms the track the auto-arm armed before; selecting any other channel (an audio or effect
  track, the master) changes no arm, the instrument track stays armed. A track armed otherwise
  is not disarmed by the auto-arm. Bitwig does not document its rules; they are derived from all
  selection changes of the Java traces (target/conformance/*-java.json, ``osc:/track/selected/
  name`` and ``osc:/track/N/recarm``, 2026-11-02):

  | Selection | Armed before | Armed after | Example |
  | --- | --- | --- | --- |
  | instrument -> instrument | old | new | flexi.track.next-previous (Drums -> Layers) |
  | instrument -> audio | old | old | flexi.modes.track-next-previous-item (Layers -> EQ) |
  | instrument -> effect | old | old | flexi.track.trackbank (Synth -> FX 1) |
  | instrument -> master | old | old | flexi.master.panning-select (Synth -> Master) |
  | audio -> audio | Layers | Layers | flexi.modes.track-next-previous-item (EQ -> Audio 5) |
  | audio -> instrument | Layers | Synth | flexi.modes.track-next-page (EQ -> Synth) |
  | instrument -> instrument, both armed | Synth, Drums | Synth | mk2.keyboard.play (Drums -> Synth) |
"""

from dataclasses import dataclass, field
from typing import Callable

from .clip import Clip
from .device import DeviceChain
from .marker import Markers
from .parameter import (
    VOLUME_0DB,
    Parameter,
    Preferences,
    format_metronome_volume,
    format_pan,
    format_volume,
)

Color = tuple[int, int, int]

GRAY: Color = (127, 127, 127)
NO_COLOR: Color = (0, 0, 0)
SCENE_COLOR: Color = (80, 80, 80)

# The colors of new tracks, in turn (Bitwig's palette is not public; these are plain colors)
TRACK_COLORS: list[Color] = [(87, 97, 198), (149, 73, 203), (217, 46, 36), (255, 131, 62), (115, 152, 20), (0, 157, 71), (0, 166, 148), (68, 200, 255)]

TRACK_TYPES = ("AUDIO", "INSTRUMENT", "HYBRID", "EFFECT", "MASTER", "GROUP")

MONITOR_ON = "ON"
MONITOR_OFF = "OFF"
MONITOR_AUTO = "AUTO"

# TransportImpl: the pre-roll lengths of Bitwig by the number of bars
PREROLLS = ("none", "one_bar", "two_bars", None, "four_bars")

# TransportImpl: the automation write modes
AUTOMATION_MODES = ("latch", "touch", "write")

TEMPO_MIN = 20.0
TEMPO_MAX = 666.0


def color_from_command(text: str | None) -> Color | None:
    """Parse the color of a command, ``RGB(r,g,b)``; None if it is not one."""
    if not text:
        return None
    text = text.strip()
    if not (text.startswith("RGB(") and text.endswith(")")):
        return None
    try:
        red, green, blue = (int(part) for part in text[4:-1].split(","))
    except ValueError:
        return None
    return (max(0, min(255, red)), max(0, min(255, green)), max(0, min(255, blue)))


# ---------------------------------------------------------------------------------------------
# Transport


class Transport:
    """The transport, as TransportImpl (Bitwig ``Transport``). Positions are in quarters
    (beats)."""

    def __init__(self, preferences: Preferences):
        self.playing = False
        self.recording = False
        """The arranger record switch (``isArrangerRecordEnabled``)."""
        self.loop = False
        self.loop_start = 0.0
        self.loop_length = 8.0
        self.punch_in = False
        self.punch_out = False
        self.fill = False
        self.metronome = False
        self.metronome_ticks = False
        self.metronome_volume = Parameter(preferences, 0.75, formatter=format_metronome_volume, name="Metronome")
        self.preroll = "none"
        self.preroll_metronome = True
        self.tempo = 120.0
        self.numerator = 4
        self.denominator = 4
        self.position = 0.0
        """The play position (``getPosition``)."""
        self.play_start = 0.0
        """The play start position (``playStartPosition``), where play starts and stop returns to."""
        self.automation_arranger = False
        self.automation_launcher = False
        self.automation_mode = "latch"
        self.overdub_arranger = False
        self.overdub_launcher = False
        self._taps: list[float] = []

    @property
    def quarters_per_measure(self) -> float:
        return 4.0 * self.numerator / self.denominator

    def play(self) -> None:
        """Start playback at the play start position (``play``; nothing if playing)."""
        if not self.playing:
            self.position = self.play_start
            self.playing = True

    def stop(self) -> None:
        """Stop; the play position returns to the play start position. Stopping a playing
        transport switches the automation writing of the arranger and the launcher off (known
        only from the recordings: ``/automode/arranger 0``, ``/automode/launcher 0`` right after
        ``transport stop`` in tests/conformance/generic_flexi/replay/
        generic_flexi-transport-rust.rec, ``osc:/autowrite 0`` in the Java trace)."""
        if self.playing:
            self.automation_arranger = self.automation_launcher = False
        self.playing = False
        self.position = self.play_start

    def toggle_play(self) -> None:
        if self.playing:
            self.stop()
        else:
            self.play()

    def restart(self) -> None:
        """Play from the play start position (``restart``)."""
        self.position = self.play_start
        self.playing = True

    def stop_and_rewind(self) -> None:
        """TransportImpl.stopAndRewind: stop, then the position to 0 (Bitwig: 100 ms later)."""
        self.stop()
        self.set_position_absolute(0.0)

    def record(self) -> None:
        """Toggle the arranger record switch (``record``)."""
        self.recording = not self.recording

    def set_position(self, beats: float) -> None:
        """TransportImpl.setPosition: the play start position; jump to it if playing."""
        self.play_start = max(0.0, beats)
        if self.playing:
            self.position = self.play_start

    def set_position_absolute(self, beats: float) -> None:
        """Bitwig ``Transport.setPosition``: the play position and the play start position."""
        self.play_start = self.position = max(0.0, beats)

    def set_tempo(self, bpm: float) -> None:
        self.tempo = min(TEMPO_MAX, max(TEMPO_MIN, float(bpm)))

    def change_tempo(self, delta: float) -> None:
        self.set_tempo(self.tempo + delta)

    def tap_tempo(self, now_ms: float) -> None:
        """Bitwig ``tapTempo``: the tempo from the average interval of the recent taps (a tap more
        than 2 s after the last one starts again)."""
        if self._taps and now_ms - self._taps[-1] > 2000:
            self._taps.clear()
        self._taps = (self._taps + [now_ms])[-4:]
        if len(self._taps) >= 2:
            interval = (self._taps[-1] - self._taps[0]) / (len(self._taps) - 1)
            if interval > 0:
                self.set_tempo(60000.0 / interval)

    def set_time_signature(self, numerator: int, denominator: int) -> None:
        if numerator > 0 and denominator > 0:
            self.numerator = numerator
            self.denominator = denominator

    def set_automation_write_mode(self, index: int) -> None:
        """TransportImpl.setAutomationWriteMode by the index of the core (0 TrimRead, 1 Read,
        2 Touch, 3 Write, 4 Latch, 5 LatchPreview): Read and below turn both switches off."""
        if index <= 1:
            self.automation_arranger = self.automation_launcher = False
            return
        self.automation_arranger = self.automation_launcher = True
        self.automation_mode = {2: "touch", 3: "write"}.get(index, "latch")

    def seconds(self) -> float:
        return self.position * 60.0 / self.tempo if self.tempo > 0 else 0.0

    def advance(self, ms: float) -> list[float]:
        """Move the play position while playing; wraps at the end of the arranger loop. Returns
        the bar starts (in beats) which were reached."""
        if not self.playing or ms <= 0:
            return []
        bar = self.quarters_per_measure
        old = self.position
        new = old + ms * self.tempo / 60000.0
        bars = []
        first_bar = int(old // bar) + 1
        while first_bar * bar <= new:
            bars.append(first_bar * bar)
            first_bar += 1
        loop_end = self.loop_start + self.loop_length
        if self.loop and self.loop_length > 0 and old < loop_end <= new:
            new = self.loop_start + (new - loop_end)
            bars.append(self.loop_start)
        self.position = new
        return bars


class Application:
    """Undo and redo (ApplicationImpl, Bitwig ``Application``): the undo history of the project.

    An edit (``edit``) is an entry with the functions which undo and redo it; ``undo`` and
    ``redo`` move the last entry between the two stacks, ``canUndo`` and ``canRedo`` say whether
    they are empty. Consecutive edits of the same value (the same ``key``, e.g. the steps of a
    knob turn) are one entry, a simplification: Bitwig's grouping of edits is not documented.
    Edits without functions (e.g. of layers) only make undo possible. The panel layout and the
    other application values are not simulated.
    """

    def __init__(self):
        self.panel_layout = "ARRANGE"
        """The panel layout (``panelLayout``, one of PANEL_LAYOUTS)."""
        self.undo_stack: list[tuple[object, Callable[[], None] | None, Callable[[], None] | None]] = []
        self.redo_stack: list[tuple[object, Callable[[], None] | None, Callable[[], None] | None]] = []
        self.undos = 0
        self.redos = 0
        self.edits = 0

    @property
    def can_undo(self) -> bool:
        return bool(self.undo_stack)

    @property
    def can_redo(self) -> bool:
        return bool(self.redo_stack)

    def edit(self, key: object = None, undo: Callable[[], None] | None = None, redo: Callable[[], None] | None = None) -> None:
        """An undoable change of the project (Bitwig adds it to the undo history and clears the
        redo steps)."""
        self.edits += 1
        self.redo_stack.clear()
        if key is not None and self.undo_stack and self.undo_stack[-1][0] == key:
            previous_key, previous_undo, _ = self.undo_stack[-1]
            self.undo_stack[-1] = (previous_key, previous_undo, redo)
            return
        self.undo_stack.append((key, undo, redo))

    def edit_value(self, key: object, setter: Callable[[object], None], before: object, after: object) -> None:
        """An edit which set a value from ``before`` to ``after`` with the setter."""
        self.edit(key, lambda: setter(before), lambda: setter(after))

    def undo(self) -> None:
        self.undos += 1
        if self.undo_stack:
            entry = self.undo_stack.pop()
            if entry[1] is not None:
                entry[1]()
            self.redo_stack.append(entry)

    def redo(self) -> None:
        self.redos += 1
        if self.redo_stack:
            entry = self.redo_stack.pop()
            if entry[2] is not None:
                entry[2]()
            self.undo_stack.append(entry)


# The panel layouts of Bitwig by the index of the core (CommandHandler.PANEL_LAYOUTS)
PANEL_LAYOUTS = ("ARRANGE", "MIX", "EDIT", "PLAY")


@dataclass
class Arranger:
    """The visibility flags of the arranger (ArrangerImpl, Bitwig ``Arranger``); the defaults
    are Bitwig's of the fixture (``osc:/arranger/…`` of the Java traces)."""

    cue_markers: bool = True
    playback_follow: bool = True
    double_row_track_height: bool = True
    clip_launcher: bool = True
    timeline: bool = True
    io_section: bool = False
    effect_tracks: bool = True


@dataclass
class Mixer:
    """The visibility flags of the mixer (MixerImpl, Bitwig ``Mixer``); the defaults are
    Bitwig's of the fixture (``osc:/mixer/…`` of the Java traces)."""

    clip_launcher_section: bool = True
    cross_fade_section: bool = False
    device_section: bool = True
    io_section: bool = True
    meter_section: bool = False
    send_section: bool = True


# The toggles of the arranger and the mixer (CommandHandler.handleArranger / handleMixer)
ARRANGER_TOGGLES = {"toggleCueMarkers": "cue_markers", "togglePlaybackFollow": "playback_follow", "toggleTrackRowHeight": "double_row_track_height",
                    "toggleClipLauncher": "clip_launcher", "toggleTimeLine": "timeline", "toggleIoSection": "io_section",
                    "toggleEffectTracks": "effect_tracks"}
MIXER_TOGGLES = {"toggleClipLauncherSection": "clip_launcher_section", "toggleCrossFadeSection": "cross_fade_section",
                 "toggleDeviceSection": "device_section", "toggleIoSection": "io_section", "toggleMeterSection": "meter_section",
                 "toggleSendsSection": "send_section"}


# ---------------------------------------------------------------------------------------------
# Note inputs and note repeat


# The arpeggiator modes by the index of the core's mode (BitwigModel.ARPEGGIATOR_MODES: Down, Up,
# Down-Up, Up-Down)
ARPEGGIATOR_MODES = ("down", "up", "down-up", "up-down")


class Arpeggiator:
    """The arpeggiator of a note input, which Bitwig uses as note repeat (Bitwig ``Arpeggiator``,
    NoteRepeatImpl of the Java version, the note repeat of the Bitwig extension).

    The values are Bitwig's: ``rate`` is the period in beats, ``gate_length`` the note length as
    the core sends it, ``mode`` the name of the mode. The arpeggiator only holds the values; the
    simulation plays no notes.

    The defaults are what Bitwig reported for a new note input in all recorded sessions
    (``tests/conformance/*/replay/*-rust.rec``, the first ``/noterepeat/*`` model lines: active 0,
    period 4.0, i.e. a rate of 1/4 beat, notelength 0.5, mode 1 = up, velocity 1); Bitwig does
    not document them. Bitwig's ranges of rate and gate length are not known: the values are
    taken as they are.
    """

    def __init__(self):
        self.enabled = False
        self.rate = 0.25
        self.gate_length = 0.5
        self.mode = "up"
        self.use_pressure_to_velocity = True

    @property
    def mode_index(self) -> int:
        """The index of the core's mode (BitwigModel.collectNoteRepeat: 0 for an unknown mode)."""
        return ARPEGGIATOR_MODES.index(self.mode) if self.mode in ARPEGGIATOR_MODES else 0

    def set_mode_index(self, index: int) -> None:
        """Ignored out of the range (CommandHandler.handleNoteRepeat)."""
        if 0 <= index < len(ARPEGGIATOR_MODES):
            self.mode = ARPEGGIATOR_MODES[index]


class NoteInput:
    """A note input of the controller's MIDI input (Bitwig ``NoteInput``, created by
    ``MidiIn.createNoteInput`` with masks, ``setKeyTranslationTable``,
    ``setVelocityTranslationTable``, ``sendRawMidiEvent``) and its arpeggiator.

    The masks are hex strings with ``?`` for any digit, padded to 6 digits
    (DbmExtension.toBitwigMasks). The notes it lets through play on the tracks which monitor
    their input (``SimDaw.play_live_note``); a raw event is not translated (the further notes of
    a chord arrive with their pitch: ``raw 90 28 7F`` after the pad 90 24 in
    tests/conformance/maschine_mk2/replay/native_instruments_maschine_mk2-pad_modes-rust.rec,
    and the observer showed the pitches 36 40 43)."""

    def __init__(self, name: str = "", filters: list[str] | None = None):
        self.name = name
        self.arpeggiator = Arpeggiator()
        self.filters = [(mask + "??????")[: max(6, len(mask))].upper() for mask in filters or []]
        self.key_table: list[int] | None = None
        self.velocity_table: list[int] | None = None

    def matches(self, data: bytes) -> bool:
        text = data.hex().upper()
        return any(len(text) >= len(mask) and all(m == "?" or m == c for m, c in zip(mask, text)) for mask in self.filters)

    def translate(self, data: bytes) -> tuple[int, int, int] | None:
        """The note event the input lets through (status, key, velocity) after the translation
        tables, None if it is not a note or the key is blocked (-1)."""
        if len(data) < 3 or data[0] & 0xF0 not in (0x80, 0x90) or not self.matches(data):
            return None
        status, key, velocity = data[0], data[1], data[2]
        if self.key_table is not None and key < len(self.key_table):
            key = self.key_table[key]
            if key < 0:
                return None
        if self.velocity_table is not None and velocity < len(self.velocity_table) and status & 0xF0 == 0x90:
            velocity = self.velocity_table[velocity]
        return status, key, velocity


# ---------------------------------------------------------------------------------------------
# Channels


class Send:
    """A send of a channel to an effect track (SendImpl, Bitwig ``Send``). The name and color are
    the effect track's.

    A send is enabled when its volume is raised above 0 and stays enabled (known only from the
    Java traces: ``osc:/track/1/send/1/activated`` 0 -> 1 after ``/track/1/send/1/volume 100``,
    and 1 after the volume went back to 0; flexi.modes.track-send, flexi.track.knobs)."""

    def __init__(self, preferences: Preferences, target: "Track"):
        self.target = target
        self.volume = Parameter(preferences, 0.0, formatter=format_volume, name="Send")
        self.enabled = False
        self.volume.listeners.append(self._on_volume)

    def _on_volume(self, value: float) -> None:
        if value > 0:
            self.enabled = True

    @property
    def name(self) -> str:
        return self.target.name

    @property
    def color(self) -> Color:
        return self.target.color


class Channel:
    """The values all channels have (ChannelImpl, Bitwig ``Channel``)."""

    def __init__(self, daw: "SimDaw", name: str, color: Color):
        self.daw = daw
        self.name = name
        self.color = color
        self.volume = Parameter(daw.preferences, VOLUME_0DB, formatter=format_volume, name="Volume")
        self.pan = Parameter(daw.preferences, 0.5, formatter=format_pan, name="Pan")
        self.mute = False
        self.solo = False
        self.activated = True
        self.vu_left = 0.0
        self.vu_right = 0.0
        self.sends: list[Send] = []
        self.devices = DeviceChain()
        """The device chain (the selected device is the cursor device of the channel)."""

    @property
    def selected(self) -> bool:
        """Selected in the editor (``addIsSelectedInEditorObserver``); one channel at a time."""
        return self.daw.selected_channel is self

    def select(self) -> None:
        """ChannelImpl.select: select in the editor and the mixer."""
        self.daw.select_channel(self)

    def toggle_solo(self) -> None:
        """``solo ().toggleUsingPreferences (false)``: with the exclusive solo preference, soloing
        unsolos all other channels."""
        self.set_solo(not self.solo, exclusive=self.daw.preferences.exclusive_solo)

    def set_mute(self, mute: bool) -> None:
        """``mute.set``; a change is an edit of the project (undoable)."""
        if self.mute != bool(mute):
            self.mute = bool(mute)
            self.daw.application.edit_value(("mute", id(self)), lambda value: setattr(self, "mute", value), not self.mute, self.mute)

    def set_solo(self, solo: bool, exclusive: bool = False) -> None:
        """``solo.set``; a change is an edit of the project (undoable)."""
        channels = self.daw.all_channels()
        before = [channel.solo for channel in channels]
        if solo and exclusive:
            for channel in channels:
                channel.solo = False
        self.solo = bool(solo)
        after = [channel.solo for channel in channels]
        if before != after:
            def restore(values):
                for channel, value in zip(channels, values):
                    channel.solo = value
            self.daw.application.edit_value(("solo", id(self)), restore, before, after)


class Track(Channel):
    """A track of the track list or an effect track (TrackImpl, Bitwig ``Track``)."""

    def __init__(self, daw: "SimDaw", name: str, color: Color, track_type: str = "AUDIO"):
        super().__init__(daw, name, color)
        if track_type not in TRACK_TYPES:
            raise ValueError(f"Unknown track type: {track_type}")
        self.type = track_type
        self.is_group = track_type == "GROUP"
        self.group_expanded = False
        self.arm = False
        self.monitor_mode = MONITOR_AUTO
        self.slots: list[Slot] = []
        self.last_selected_slot: "Slot | None" = None
        """The slot last selected on this track (for the cursor clip)."""
        self.auto_armed = False
        """Armed by the auto-arm of the selection (Preferences.auto_arm)."""
        self.playing_notes: dict[int, int] = {}
        """The notes which play on the track: key -> velocity, in the order they started
        (``playingNotes``; only live notes, the clips play nothing)."""

    @property
    def is_effect(self) -> bool:
        return self.type == "EFFECT"

    def set_arm(self, armed: bool) -> None:
        """Arm or disarm (``arm.set``); the auto-arm no longer owns the arm."""
        self.arm = bool(armed)
        self.auto_armed = False

    @property
    def can_hold_notes(self) -> bool:
        return self.type in ("INSTRUMENT", "HYBRID", "GROUP")

    @property
    def can_hold_audio(self) -> bool:
        return self.type in ("AUDIO", "HYBRID", "EFFECT", "GROUP")

    @property
    def monitoring(self) -> bool:
        """The effective monitoring (``isMonitoring``)."""
        return self.monitor_mode == MONITOR_ON or (self.monitor_mode == MONITOR_AUTO and self.arm)

    @property
    def is_stopped(self) -> bool:
        """No slot plays or records (``isStopped``)."""
        return not any(slot.playing or slot.recording for slot in self.slots)

    def stop(self) -> None:
        """Stop the playing clip at the next bar (``stop``), at once if the transport is
        stopped."""
        for slot in self.slots:
            slot.play_queued = slot.rec_queued = False
            if slot.playing or slot.recording:
                if self.daw.transport.playing:
                    slot.stop_queued = True
                else:
                    slot.playing = slot.recording = False


class MasterTrack(Channel):
    """The master track (MasterTrackImpl, Bitwig ``MasterTrack``): no sends, no slots."""

    type = "MASTER"
    is_group = False
    group_expanded = False
    arm = False
    monitor_mode = MONITOR_OFF
    monitoring = False
    can_hold_notes = False
    can_hold_audio = True
    is_effect = False
    is_stopped = True
    slots: list = []

    def __init__(self, daw: "SimDaw"):
        super().__init__(daw, "Master", GRAY)


# ---------------------------------------------------------------------------------------------
# Clip launcher


class Slot:
    """A clip launcher slot (SlotImpl, Bitwig ``ClipLauncherSlot``)."""

    def __init__(self, track: Track, scene: "Scene"):
        self.track = track
        self.scene = scene
        self.has_content = False
        self.name = ""
        self.clip_color: Color = NO_COLOR
        self.playing = False
        self.recording = False
        self.play_queued = False
        self.rec_queued = False
        self.stop_queued = False
        self.clip: Clip | None = None
        """The clip of the slot (None without content)."""
        self.play_start_beat = 0.0
        """The transport position at which the slot started to play (for the playing step)."""

    @property
    def index(self) -> int:
        return self.track.slots.index(self)

    @property
    def color(self) -> Color:
        """The slots of group tracks have the color of the track (as the Bitwig extension)."""
        if self.track.is_group:
            return self.track.color
        return self.clip_color if self.has_content else NO_COLOR

    @property
    def selected(self) -> bool:
        return self.track.daw.selected_slot is self

    def select(self) -> None:
        self.track.daw.selected_slot = self
        self.track.last_selected_slot = self

    def create_clip(self, length_beats: float = 4.0) -> None:
        """``createNewLauncherClip``: an empty clip of the length with the color of the track."""
        self.has_content = True
        self.clip_color = self.track.color
        self.name = ""
        self.clip = Clip(self.track.daw.preferences, length_beats if length_beats > 0 else 4.0)

    def delete(self) -> None:
        """``deleteObject``."""
        self.has_content = self.playing = self.recording = False
        self.play_queued = self.rec_queued = self.stop_queued = False
        self.name = ""
        self.clip_color = NO_COLOR
        self.clip = None

    def copy_from(self, other: "Slot") -> None:
        self.has_content = other.has_content
        self.name = other.name
        self.clip_color = other.clip_color
        self.clip = other.clip.copy() if other.clip is not None else None

    def duplicate(self) -> None:
        """``duplicateClip``: a copy into the slot below (if there is one)."""
        slots = self.track.slots
        if self.has_content and self.index + 1 < len(slots):
            slots[self.index + 1].copy_from(self)

    def launch(self, quantized: bool | None = None) -> None:
        """``launch``: play the clip, record into an empty slot of an armed track, or stop the
        track; quantized to the next bar while playing (see the module docstring)."""
        daw = self.track.daw
        if not self.has_content and not self.track.arm:
            self.track.stop()
            return
        for slot in self.track.slots:
            if slot is not self:
                slot.play_queued = slot.rec_queued = False
        if daw.transport.playing if quantized is None else quantized:
            if self.has_content:
                self.play_queued = True
            else:
                self.rec_queued = True
            return
        daw.transport.play()
        self._start()

    def record(self) -> None:
        """``record``: record into the slot (replaces its content)."""
        daw = self.track.daw
        if daw.transport.playing:
            self.rec_queued = True
        else:
            daw.transport.play()
            self.has_content = False
            self._start()

    def _start(self) -> None:
        for slot in self.track.slots:
            if slot is not self:
                slot.playing = slot.recording = slot.stop_queued = False
        self.play_queued = self.stop_queued = False
        self.play_start_beat = self.track.daw.transport.position
        if self.rec_queued or not self.has_content:
            self.rec_queued = False
            self.recording = True
            self.playing = False
            self.has_content = True
            self.clip_color = self.track.color
            self.clip = Clip(self.track.daw.preferences, self.track.daw.transport.quarters_per_measure)
        else:
            self.playing = True
            self.recording = False

    def on_bar(self) -> None:
        """A bar starts: the queued state takes effect."""
        if self.play_queued or self.rec_queued:
            self._start()
        elif self.stop_queued:
            self.playing = self.recording = self.stop_queued = False


class Scene:
    """A scene (SceneImpl, Bitwig ``Scene``)."""

    def __init__(self, daw: "SimDaw", name: str):
        self.daw = daw
        self.name = name
        self.color: Color = SCENE_COLOR

    @property
    def index(self) -> int:
        return self.daw.scenes.index(self)

    @property
    def selected(self) -> bool:
        return self.daw.selected_scene is self

    def select(self) -> None:
        """``selectInEditor``: the scene is selected instead of a slot (the observer showed the
        selected slot deselected after a scene selection: mk2.scene.select of maschine_mk2
        pad_modes, flexi.scene.launch of generic_flexi project)."""
        self.daw.selected_scene = self
        self.daw.selected_slot = None

    def slots(self) -> list[Slot]:
        index = self.index
        return [track.slots[index] for track in self.daw.tracks + self.daw.effect_tracks if index < len(track.slots)]

    def launch(self) -> None:
        """Launch the slots of all tracks; starts the transport."""
        quantized = self.daw.transport.playing
        for slot in self.slots():
            slot.launch(quantized)
        self.daw.transport.play()


# ---------------------------------------------------------------------------------------------
# The DAW


@dataclass
class CursorTrack:
    """The cursor track (CursorTrackImpl): follows the selection unless pinned."""

    pinned: bool = False
    pinned_channel: Channel | None = None
    device_pinned: bool = False
    """The cursor device is pinned (``PinnableCursorDevice.isPinned``)."""
    pinned_device: object = None
    clip_pinned: bool = False
    """The cursor clip is pinned (``PinnableCursorClip.isPinned``)."""
    pinned_slot: object = None


@dataclass
class SimDaw:
    """The simulated DAW: transport, tracks, effect tracks, master, scenes and slots."""

    preferences: Preferences = field(default_factory=Preferences)
    tracks: list[Track] = field(default_factory=list)
    effect_tracks: list[Track] = field(default_factory=list)
    scenes: list[Scene] = field(default_factory=list)
    selected_channel: Channel | None = None
    selected_scene: Scene | None = None
    selected_slot: Slot | None = None
    cursor: CursorTrack = field(default_factory=CursorTrack)
    time_ms: float = 0.0
    """The simulated time, moved by advance."""
    note_input: NoteInput | None = None
    """The default note input of the controller (the first one the core creates); its
    arpeggiator is the note repeat. None until the core creates one."""
    # Not modelled yet (later steps)
    browser: object = None
    project: object = None
    groove: object = None

    def __post_init__(self):
        self.transport = Transport(self.preferences)
        self.application = Application()
        self.arranger = Arranger()
        self.mixer = Mixer()
        self.markers = Markers(self.transport)
        """The cue markers of the arranger (marker.py)."""
        self.master = MasterTrack(self)
        self._time_listeners: list[Callable[[float], None]] = []
        self._color_index = 0

    # Tracks

    def _next_color(self) -> Color:
        color = TRACK_COLORS[self._color_index % len(TRACK_COLORS)]
        self._color_index += 1
        return color

    def _new_track(self, name: str | None, track_type: str, color: Color | None, effect: bool) -> Track:
        tracks = self.effect_tracks if effect else self.tracks
        if name is None:
            name = f"FX {len(tracks) + 1}" if effect else f"{track_type.capitalize()} {len(tracks) + 1}"
        track = Track(self, name, color or (GRAY if effect else self._next_color()), track_type)
        for scene in self.scenes:
            track.slots.append(Slot(track, scene))
        for target in self.effect_tracks:
            track.sends.append(Send(self.preferences, target))
        return track

    def add_track(self, track_type: str = "AUDIO", name: str | None = None, color: Color | None = None, position: int = -1) -> Track:
        """Add a track to the track list (``createAudioTrack(position)`` etc.; -1 at the end)."""
        if track_type == "EFFECT":
            return self.add_effect_track(name, color)
        track = self._new_track(name, track_type, color, effect=False)
        if position < 0 or position > len(self.tracks):
            self.tracks.append(track)
        else:
            self.tracks.insert(position, track)
        return track

    def add_effect_track(self, name: str | None = None, color: Color | None = None) -> Track:
        """Add an effect track; all channels except the master get a send to it."""
        track = self._new_track(name, "EFFECT", color, effect=True)
        self.effect_tracks.append(track)
        for channel in self.tracks + self.effect_tracks:
            channel.sends.append(Send(self.preferences, track))
        return track

    def all_channels(self) -> list[Channel]:
        return [*self.tracks, *self.effect_tracks, self.master]

    def track_list(self, with_effects_and_master: bool) -> list[Channel]:
        """The flat track list (ModelImpl): the tracks, and for a full flat list (Bitwig
        ``createTrackBank (…, true)``) the effect tracks and the master after them."""
        if with_effects_and_master:
            return self.all_channels()
        return list(self.tracks)

    def remove_track(self, track: Track) -> None:
        """``deleteObject``."""
        if track in self.tracks:
            self.tracks.remove(track)
        elif track in self.effect_tracks:
            index = self.effect_tracks.index(track)
            self.effect_tracks.remove(track)
            for channel in self.tracks + self.effect_tracks:
                del channel.sends[index]
        else:
            return
        if self.selected_channel is track:
            self.selected_channel = None
        if self.cursor.pinned_channel is track:
            self.cursor.pinned = False
            self.cursor.pinned_channel = None
        if self.selected_slot is not None and self.selected_slot.track is track:
            self.selected_slot = None

    def duplicate_track(self, track: Track) -> Track:
        """``duplicateObject``: a copy after the track."""
        tracks = self.effect_tracks if track.is_effect else self.tracks
        copy = self._new_track(track.name, track.type, track.color, effect=track.is_effect)
        for attribute in ("mute", "solo", "activated", "arm", "monitor_mode", "group_expanded"):
            setattr(copy, attribute, getattr(track, attribute))
        copy.volume.value = track.volume.value
        copy.pan.value = track.pan.value
        for source, target in zip(track.sends, copy.sends):
            target.volume.value = source.volume.value
            target.enabled = source.enabled
        for source, target in zip(track.slots, copy.slots):
            target.copy_from(source)
        tracks.insert(tracks.index(track) + 1, copy)
        if track.is_effect:
            for channel in self.tracks + self.effect_tracks:
                channel.sends.insert(tracks.index(copy), Send(self.preferences, copy))
        return copy

    def move_track(self, track: Track, direction: int) -> None:
        """Move a track one position up (-1) or down (+1) and select it (CursorTrackImpl
        swapWithPrevious / swapWithNext)."""
        tracks = self.effect_tracks if track.is_effect else self.tracks
        index = tracks.index(track)
        other = index + direction
        if 0 <= other < len(tracks):
            tracks[index], tracks[other] = tracks[other], tracks[index]
            track.select()

    # Selection and the cursor track

    def select_channel(self, channel: Channel | None) -> None:
        """Select a channel in the editor; with the auto-arm preference the arm follows the
        selection of the tracks which can hold notes (see the module docstring)."""
        previous = self.selected_channel
        self.selected_channel = channel
        if not self.preferences.auto_arm or previous is channel:
            return
        if not (isinstance(channel, Track) and channel.can_hold_notes):
            return
        for track in self.tracks + self.effect_tracks:
            if track is not channel and track.auto_armed:
                track.arm = False
                track.auto_armed = False
        if not channel.arm:
            channel.arm = True
            channel.auto_armed = True

    # Devices

    @property
    def cursor_device(self):
        """The cursor device: the selected device of the cursor track (CursorDeviceImpl,
        FOLLOW_SELECTION), or the pinned device; None without one."""
        if self.cursor.device_pinned and self.device_chain_of(self.cursor.pinned_device) is not None:
            return self.cursor.pinned_device
        channel = self.cursor_channel
        return channel.devices.selected if channel is not None else None

    @property
    def cursor_device_chain(self):
        """The device chain of the cursor device (its siblings); the chain of the cursor track
        without a cursor device."""
        chain = self.device_chain_of(self.cursor_device)
        if chain is not None:
            return chain
        channel = self.cursor_channel
        return channel.devices if channel is not None else None

    def device_chain_of(self, device):
        """The chain which holds the device, None if it holds no chain."""
        if device is None:
            return None
        return next((channel.devices for channel in self.all_channels() if device in channel.devices.devices), None)

    def set_cursor_device_pinned(self, pinned: bool) -> None:
        """``cursorDevice.isPinned ().set``: pin the current cursor device (see device.py)."""
        if pinned and not self.cursor.device_pinned:
            self.cursor.pinned_device = self.cursor_device
        self.cursor.device_pinned = bool(pinned) and self.cursor.pinned_device is not None
        if not self.cursor.device_pinned:
            self.cursor.pinned_device = None

    # The cursor clip

    @property
    def cursor_clip_slot(self) -> "Slot | None":
        """The slot of the cursor clip (see the module docstring), the pinned one while the
        cursor clip is pinned; None without a clip."""
        if self.cursor.clip_pinned and self.cursor.pinned_slot is not None:
            slot = self.cursor.pinned_slot
            return slot if slot.has_content and slot.track in self.tracks + self.effect_tracks else None
        return self.clip_slot_of(self.cursor_channel)

    def clip_slot_of(self, channel) -> "Slot | None":
        """The slot of the launcher cursor clip of a cursor track on the channel (see the module
        docstring); None without a clip."""
        if not isinstance(channel, Track):
            return None
        for slot in (self.selected_slot, channel.last_selected_slot):
            if slot is not None and slot.track is channel and slot in channel.slots and slot.has_content:
                return slot
        return next((slot for slot in channel.slots if slot.has_content), None)

    @property
    def cursor_clip(self) -> Clip | None:
        slot = self.cursor_clip_slot
        if slot is None:
            return None
        if slot.clip is None:
            slot.clip = Clip(self.preferences)
        return slot.clip

    def set_cursor_clip_pinned(self, pinned: bool) -> None:
        """``isPinned ().set`` of the cursor clips."""
        if pinned and not self.cursor.clip_pinned:
            self.cursor.pinned_slot = self.cursor_clip_slot
        self.cursor.clip_pinned = bool(pinned) and self.cursor.pinned_slot is not None
        if not self.cursor.clip_pinned:
            self.cursor.pinned_slot = None

    @property
    def drum_device(self):
        """The first Drum Machine of the cursor track (BitwigModel.drumDevice, the Bitwig device
        matcher of the Drum Machine); None without one."""
        channel = self.cursor_channel
        return channel.devices.first_drum_machine() if channel is not None else None

    @property
    def primary_device(self):
        """The first instrument of the cursor track (DeviceID.FIRST_INSTRUMENT); None without
        one."""
        channel = self.cursor_channel
        return channel.devices.first_instrument() if channel is not None else None

    @property
    def eq_device(self):
        """The first EQ+ of the cursor track (DeviceID.EQ, EqualizerDeviceImpl); None without
        one."""
        channel = self.cursor_channel
        return channel.devices.first_equalizer() if channel is not None else None

    @property
    def cursor_channel(self) -> Channel | None:
        """The cursor track: the pinned channel, else the selected one."""
        if self.cursor.pinned and self.cursor.pinned_channel is not None:
            return self.cursor.pinned_channel
        return self.selected_channel

    def set_cursor_pinned(self, pinned: bool) -> None:
        """``cursorTrack.isPinned ().set``: pin the current cursor track."""
        if pinned and not self.cursor.pinned:
            self.cursor.pinned_channel = self.selected_channel
        self.cursor.pinned = bool(pinned) and self.cursor.pinned_channel is not None
        if not self.cursor.pinned:
            self.cursor.pinned_channel = None

    def select_neighbour(self, track_list: list[Channel], direction: int) -> None:
        """``cursorTrack.selectPrevious / selectNext`` in the track list: moves the pinned
        cursor, else the selection."""
        current = self.cursor_channel
        if current not in track_list:
            return
        index = track_list.index(current) + direction
        if not 0 <= index < len(track_list):
            return
        if self.cursor.pinned:
            self.cursor.pinned_channel = track_list[index]
        else:
            track_list[index].select()

    # Note inputs

    def create_note_input(self, name: str = "", filters: list[str] | None = None, default: bool = True) -> NoteInput:
        """``MidiIn.createNoteInput``: a new note input with a new arpeggiator (its defaults, see
        Arpeggiator); the first one of the controller is the default note input."""
        note_input = NoteInput(name, filters)
        if default:
            self.note_input = note_input
        return note_input

    def play_live_note(self, status: int, key: int, velocity: int) -> None:
        """A note from a note input: it plays on every note track which monitors its input
        (monitor on, or auto and armed: Bitwig's input monitoring). The observer saw the notes of
        the Maschine Mk2 on Synth and Drums, both armed by the auto-arm, while Drums was
        selected (target/conformance/native_instruments_maschine_mk2-pad_modes-java.json,
        ``osc:/track/1/playingNotes``, ``osc:/track/2/playingNotes``)."""
        if not 0 <= key < 128:
            return
        on = status & 0xF0 == 0x90 and velocity > 0
        for track in self.tracks:
            if on and track.can_hold_notes and track.monitoring:
                track.playing_notes.pop(key, None)
                track.playing_notes[key] = velocity
            elif not on:
                track.playing_notes.pop(key, None)

    # Scenes

    def add_scene(self, name: str | None = None) -> Scene:
        scene = Scene(self, name or f"Scene {len(self.scenes) + 1}")
        self.scenes.append(scene)
        for track in self.tracks + self.effect_tracks:
            track.slots.append(Slot(track, scene))
        return scene

    def remove_scene(self, scene: Scene) -> None:
        index = scene.index
        for track in self.tracks + self.effect_tracks:
            slot = track.slots.pop(index)
            if self.selected_slot is slot:
                self.selected_slot = None
        self.scenes.remove(scene)
        if self.selected_scene is scene:
            self.selected_scene = None

    def duplicate_scene(self, scene: Scene) -> Scene:
        """``nextSceneInsertionPoint.copySlotsOrScenes``: a copy after the scene."""
        index = scene.index
        copy = Scene(self, scene.name)
        copy.color = scene.color
        self.scenes.insert(index + 1, copy)
        for track in self.tracks + self.effect_tracks:
            slot = Slot(track, copy)
            slot.copy_from(track.slots[index])
            track.slots.insert(index + 1, slot)
        return copy

    def stop_all_clips(self) -> None:
        """``sceneBank.stop``."""
        for track in self.tracks + self.effect_tracks:
            track.stop()

    # Time

    def add_time_listener(self, listener: Callable[[float], None]) -> None:
        self._time_listeners.append(listener)

    def advance(self, ms: float) -> None:
        """Move the simulated time: the play position, queued clips start at the next bar."""
        self.time_ms += ms
        if self.transport.advance(ms):
            self.markers.on_bar()
            for track in self.tracks + self.effect_tracks:
                for slot in track.slots:
                    slot.on_bar()
        for listener in self._time_listeners:
            listener(ms)

    # Building a project

    def build(self, tracks: list[tuple[str, str]] = (), effect_tracks: list[str] = (), scenes: int = 0) -> "SimDaw":
        """Add tracks (name, type), effect tracks and scenes; returns self."""
        for name in effect_tracks:
            self.add_effect_track(name)
        for name, track_type in tracks:
            self.add_track(track_type, name)
        for _ in range(scenes):
            self.add_scene()
        return self


__all__ = [
    "ARPEGGIATOR_MODES",
    "AUTOMATION_MODES",
    "Application",
    "Arpeggiator",
    "Arranger",
    "Mixer",
    "PANEL_LAYOUTS",
    "Channel",
    "CursorTrack",
    "MasterTrack",
    "NoteInput",
    "PREROLLS",
    "Scene",
    "Send",
    "SimDaw",
    "Slot",
    "Track",
    "Transport",
    "color_from_command",
    "format_metronome_volume",
]
