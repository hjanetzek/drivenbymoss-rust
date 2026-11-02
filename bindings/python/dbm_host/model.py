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

- ``Track.devices``: the device chain of a track (``Device``: parameters, remote-control pages,
  layers, drum pads), for ``/device``, ``/primary``, ``/eq``, ``/drum``.
- ``Slot.clip``: the clip of a slot (note grid, loop, play range), for ``/clip``.
- ``SimDaw.markers``, ``SimDaw.browser``, ``SimDaw.project``, ``SimDaw.groove``: not modelled
  yet (None or empty); ``Application`` has only undo and redo.
- ``SimDaw.add_time_listener``: called by ``advance`` with the elapsed time, e.g. for the VU and
  the playing notes.

Simplifications (each a deliberate choice, not a fit to a scenario):

- Clip launching is quantized to the next bar while the transport plays; launching while stopped
  starts the transport and the clip at once. Launch quantization settings are ignored.
- ``Transport.stop`` moves the play position back to the play start position.
- The monitoring of AUTO is on while the track is armed.
- New sends are disabled (``isEnabled`` 0), as Bitwig reported them for the fixture.
- ``track addTrack``: the name and color are set at once (Bitwig: 300 ms later).
"""

from dataclasses import dataclass, field
from typing import Callable

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
        """Stop; the play position returns to the play start position."""
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
    """Undo and redo (ApplicationImpl, Bitwig ``Application``). The simulation has no undo
    history: it counts the calls; a fixture sets ``can_undo`` and ``can_redo``. The panel layout
    and the other application values come with a later step."""

    def __init__(self):
        self.can_undo = False
        self.can_redo = False
        self.undos = 0
        self.redos = 0

    def undo(self) -> None:
        self.undos += 1

    def redo(self) -> None:
        self.redos += 1


# ---------------------------------------------------------------------------------------------
# Channels


class Send:
    """A send of a channel to an effect track (SendImpl, Bitwig ``Send``). The name and color are
    the effect track's."""

    def __init__(self, preferences: Preferences, target: "Track"):
        self.target = target
        self.volume = Parameter(preferences, 0.0, formatter=format_volume, name="Send")
        self.enabled = False

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

    def set_solo(self, solo: bool, exclusive: bool = False) -> None:
        if solo and exclusive:
            for channel in self.daw.all_channels():
                channel.solo = False
        self.solo = bool(solo)


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
        self.devices: list = []
        """The device chain; devices come with step 4 (PYTHON_HOST_PLAN.md §5)."""

    @property
    def is_effect(self) -> bool:
        return self.type == "EFFECT"

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
        self.clip = None
        """The clip of the slot; clips come with step 4 (PYTHON_HOST_PLAN.md §5)."""

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

    def create_clip(self, length_beats: float = 4.0) -> None:
        """``createNewLauncherClip``: an empty clip with the color of the track."""
        self.has_content = True
        self.clip_color = self.track.color
        self.name = ""

    def delete(self) -> None:
        """``deleteObject``."""
        self.has_content = self.playing = self.recording = False
        self.play_queued = self.rec_queued = self.stop_queued = False
        self.name = ""
        self.clip_color = NO_COLOR

    def copy_from(self, other: "Slot") -> None:
        self.has_content = other.has_content
        self.name = other.name
        self.clip_color = other.clip_color

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
        if self.rec_queued or not self.has_content:
            self.rec_queued = False
            self.recording = True
            self.playing = False
            self.has_content = True
            self.clip_color = self.track.color
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
        """``selectInEditor``."""
        self.daw.selected_scene = self

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
    # Not modelled yet (later steps)
    markers: list = field(default_factory=list)
    browser: object = None
    project: object = None
    groove: object = None

    def __post_init__(self):
        self.transport = Transport(self.preferences)
        self.application = Application()
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
        self.selected_channel = channel

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
    "AUTOMATION_MODES",
    "Application",
    "Channel",
    "CursorTrack",
    "MasterTrack",
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
