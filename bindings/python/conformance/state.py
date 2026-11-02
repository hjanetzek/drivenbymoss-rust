# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The state view: the simulated DAW as the observer of the Bitwig runs reports it.

In the Bitwig runs, the state of Bitwig comes from the OSC controller of the Java DrivenByMoss
(branch ``conformance-osc``, ``controller/osc/module/*Module.java``) as ``osc:<address>`` keys
with the values as text (``conformance_runner.StateListener``). ``snapshot`` produces the same
keys from a ``SimDaw``, for the keys the view models (``is_modelled``); every other key of a
Java trace counts as "not modelled" (PYTHON_HOST_PLAN.md §4).

The observer has its own banks, as the OSC controller in Bitwig (``OscController``): a main
track bank of 8 tracks which follows the cursor track (``ModelImpl``:
``createMainTrackBank``, ``followCursorTrack``), 8 sends, 8 slots, 8 scenes, a sibling bank of 8
devices and a page bank of 8 remote-control pages. Its numbering is 1-based.

Value formats (``AbstractOpenSoundControlWriter``, ``osc_monitor.parse_message``):

- Flags are ``1``/``0``, integers as text.
- Doubles are sent as 32-bit floats and read rounded to 4 decimals (``120.0``).
- Values of parameters (volume, pan, send) are ``0..127`` (``TwosComplementValueChanger (128)``:
  ``Math.round (value * 127)``).
- Colors are ``rgb(r,g,b)`` with 0..255 (``sendOSCColor``).

Modelled: the transport switches, the tempo and the automation write mode, the panel layout and
the visibility flags of the arranger and the mixer, the tracks of the bank with sends, slots
and playing notes (``/track/N/playingNotes`` of the conformance module), the cursor track
(``/track/selected``), the master, the scenes, the markers, the cursor device and the first
instrument with their siblings and pages, the layers of the cursor device, the layers and drum
pads of the first instrument (``/conformance/primary``), the EQ+, the cursor clip (exists,
color) and its steps (``/clip/step``). The observer's cursor track follows the selection; it is
never pinned (the controller pins its own one). Not modelled yet: device parameters, the drum
pads of the cursor device, the sends of layers, the project and track remote controls, the
browser, the crossfader, the other ``/conformance`` values, ``/vkb_midi``, VU meters (ignored
by the comparison anyway).

Simplifications:

- The track bank follows the cursor track page by page (the offset is the page of the cursor
  track), as Bitwig scrolled it in the reference recordings; Bitwig's own rule is not
  documented.
- An empty item of the track bank reports ``activated 1`` and ``autoMonitor 1``, as Bitwig did
  in the reference recordings (the values of an empty Bitwig bank item).
"""

import math
import re
import struct
from collections.abc import Callable

from dbm_host import SimDaw
from dbm_host.clip import NoteGrid
from dbm_host.device import EQ_NUM_BANDS
from dbm_host.model import Channel, MasterTrack, Track

PAGE_SIZE = 8
"""The bank size of the OSC controller (OSCConfiguration "Bank page size", default 8)."""

# The grid of the cursor clip of the conformance module (ConformanceModule.NUM_STEPS, NUM_ROWS)
CLIP_STEPS = 64
CLIP_ROWS = 128

# The drum pad bank of the observer's first instrument: 16 pads from C1 (DrumPadBankImpl)
DRUM_PAD_PAGE_SIZE = 16
DRUM_PAD_OFFSET = 36

# The delay of the selection after a page scroll (AbstractItemBankImpl.selectPreviousPage)
SELECT_AFTER_SCROLL_MS = 75

# The modelled keys (without the "osc:" prefix). N is a 1-based index.
_N = r"[1-9]\d*"
_CHANNEL = r"(exists|name|color|activated|selected|mute|solo|volume|volumeStr|pan|panStr)"
_TRACK = (r"(type|isGroup|recarm|monitor|autoMonitor|canHoldNotes|canHoldAudioData|position"
          rf"|send/{_N}/(exists|name|volume|volumeStr|activated)"
          rf"|clip/{_N}/(name|isSelected|hasContent|isPlaying|isRecording|isPlayingQueued|isRecordingQueued|isStopQueued|color))")
_DEVICE = rf"(exists|name|bypass|expand|parameters|window|page/{_N}/(exists|name|selected)?|page/selected/name|param/{_N}/(exists|name|value|valueStr|modulatedValue))"
_LAYER = r"(exists|activated|selected|name|volume|volumeStr|pan|panStr|mute|solo|color)"
_CONFORMANCE_LAYER = r"(exists|name|selected|mute|solo|volume|pan)"
MODELLED = [
    r"/(play|record|repeat|punchIn|punchOut|overdub|overdub/launcher|autowrite|autowrite/launcher|automationWriteMode)",
    r"/(click|click/ticks|click/volume|click/preroll|preroll|tempo/raw|time/signature)",
    r"/layout|/arranger/(cueMarkerVisibility|playbackFollow|trackRowHeight|clipLauncherSectionVisibility|timeLineVisibility|ioSectionVisibility|effectTracksVisibility)",
    r"/mixer/(clipLauncherSectionVisibility|crossFadeSectionVisibility|deviceSectionVisibility|ioSectionVisibility|meterSectionVisibility|sendsSectionVisibility)",
    rf"/track/{_N}/({_CHANNEL}|{_TRACK}|playingNotes)",
    rf"/track/selected/((exists|name|color|activated|mute|solo|volume|volumeStr|pan|panStr)|{_TRACK}|pinned)",
    rf"/master/{_CHANNEL}",
    rf"/scene/{_N}/(exists|name|color|selected)",
    rf"/marker/{_N}/(exists|name|color)",
    rf"/(device|primary)/{_DEVICE}",
    rf"/device/sibling/{_N}/(exists|name|selected|bypass)",
    rf"/device/layer/({_N}|selected)/{_LAYER}",
    rf"/conformance/primary/(layer|drumpad)/({_N}/{_CONFORMANCE_LAYER}|offset)",
    r"/clip/(exists|color|step/\d+/\d+/\d+)",
    rf"/eq/(exists|name|bypass|expand|parameters|window|type/{_N}/value|(gain|freq|q)/{_N}/(exists|name|value|valueStr|modulatedValue))",
]
_MODELLED = re.compile("|".join(f"(?:{pattern})" for pattern in MODELLED))


def is_modelled(key: str) -> bool:
    """True if the state view produces the key (``osc:/…``; probe keys are never modelled)."""
    if not key.startswith("osc:"):
        return False
    return _MODELLED.fullmatch(key[4:]) is not None


# ---------------------------------------------------------------------------------------------
# Values


def flag(value: bool) -> str:
    return "1" if value else "0"


def number(value: float) -> str:
    """A double as the OSC controller sends it (a 32-bit float) and the runner reads it."""
    return str(round(struct.unpack(">f", struct.pack(">f", float(value)))[0], 4))


def value127(value: float) -> str:
    """A parameter value 0..1 as 0..127 (``Math.round``)."""
    return str(int(math.floor(float(value) * 127.0 + 0.5)))


def rgb(color: tuple[int, int, int]) -> str:
    return "rgb({},{},{})".format(*(int(part) for part in color))


TRACK_TYPES = {"AUDIO": "audio", "INSTRUMENT": "instrument", "HYBRID": "hybrid", "EFFECT": "effect", "MASTER": "master", "GROUP": "group"}


# ---------------------------------------------------------------------------------------------
# The observer


class OscController:
    """The banks of the observer (the Java OSC controller in Bitwig) and its pending tasks."""

    def __init__(self, daw: SimDaw, clock: Callable[[], float] = lambda: 0.0):
        self.daw = daw
        self.clock = clock
        self.track_offset = 0
        self._cursor: Channel | None = daw.selected_channel
        self._tasks: list[tuple[float, Callable[[], None]]] = []
        # ConformanceModule: the steps of a fixed grid of the cursor clip (64 steps of 1/16 from
        # the clip start, all 128 notes); the steps reported so far
        self.clip_grid = NoteGrid(CLIP_STEPS, CLIP_ROWS, lambda: self.cursor_clip)
        self._clip_steps: dict[str, str] = {}
        self._cursor_flags = ("1", "0")
        self.follow()

    # The observer's cursor track and its devices and clip: it follows the selection (the
    # observer's cursor track is never pinned, the controller pins its own one)

    @property
    def cursor(self) -> Channel | None:
        return self.daw.selected_channel

    @property
    def cursor_device(self):
        return self.cursor.devices.selected if self.cursor is not None else None

    @property
    def primary_device(self):
        return self.cursor.devices.first_instrument() if self.cursor is not None else None

    @property
    def eq_device(self):
        return self.cursor.devices.first_equalizer() if self.cursor is not None else None

    @property
    def cursor_clip_slot(self):
        return self.daw.clip_slot_of(self.cursor)

    @property
    def cursor_clip(self):
        slot = self.cursor_clip_slot
        return slot.clip if slot is not None else None

    # The track bank

    def tracks(self) -> list[Track]:
        """The main track bank (the tracks of the root, without effect tracks and master)."""
        return self.daw.tracks

    def track(self, number: int) -> Track | None:
        """The track of the 1-based bank position, None for an empty item."""
        index = self.track_offset + number - 1
        tracks = self.tracks()
        return tracks[index] if 1 <= number <= PAGE_SIZE and 0 <= index < len(tracks) else None

    def follow(self) -> None:
        """``followCursorTrack``: when the cursor track changed, scroll to its page."""
        cursor = self.cursor
        if cursor is self._cursor:
            return
        self._cursor = cursor
        tracks = self.tracks()
        if cursor in tracks:
            index = tracks.index(cursor)
            if not self.track_offset <= index < self.track_offset + PAGE_SIZE:
                self.track_offset = index // PAGE_SIZE * PAGE_SIZE

    def scroll_page(self, direction: int) -> None:
        """``selectPreviousPage`` / ``selectNextPage``: scroll a page if possible and select the
        first track of the page 75 ms later."""
        if direction < 0:
            if self.track_offset <= 0:
                return
            self.track_offset = max(0, self.track_offset - PAGE_SIZE)
        else:
            if self.track_offset + PAGE_SIZE >= len(self.tracks()):
                return
            self.track_offset += PAGE_SIZE
        self.later(SELECT_AFTER_SCROLL_MS, self._select_first)

    def _select_first(self) -> None:
        track = self.track(1)
        if track is not None:
            track.select()

    # Tasks (host.scheduleTask)

    def later(self, delay_ms: float, task: Callable[[], None]) -> None:
        self._tasks.append((self.clock() + delay_ms, task))

    def run_tasks(self) -> None:
        """Run the due tasks; then follow the cursor track."""
        now = self.clock()
        due = [task for when, task in self._tasks if when <= now]
        self._tasks = [(when, task) for when, task in self._tasks if when > now]
        for task in due:
            task()
        self.follow()

    # The view

    def snapshot(self) -> dict[str, str]:
        """All modelled keys with their values (``osc:`` prefixed)."""
        self.follow()
        state: dict[str, str] = {}
        put = lambda address, value: state.__setitem__("osc:" + address, value)
        self._transport(put)
        for number in range(1, PAGE_SIZE + 1):
            track = self.track(number)
            self._track(put, f"/track/{number}/", track, self.track_offset + number - 1)
            # ConformanceModule: the notes which play on the track, "note:velocity" by note
            notes = track.playing_notes if track is not None else {}
            put(f"/track/{number}/playingNotes", " ".join(f"{key}:{notes[key]}" for key in sorted(notes)))
        cursor = self.cursor
        self._track(put, "/track/selected/", cursor, self._position(cursor), cursor=True)
        self._master(put)
        self._scenes(put)
        self._markers(put)
        self._device(put, "/device/", self.cursor_device, siblings=True)
        self._device(put, "/primary/", self.primary_device, siblings=False)
        self._device_layers(put)
        self._conformance_primary(put)
        self._equalizer(put)
        self._clip(put)
        return state

    def _clip(self, put) -> None:
        """ClipModule (``/clip/exists``, ``color``) and ConformanceModule.flushSteps: the steps
        of the observer's grid in the format of ``StepInfo::to_wire``; a step which became empty
        stays ``0`` (the observer sends ``0`` once, the trace keeps it)."""
        daw = self.daw
        slot = self.cursor_clip_slot
        put("/clip/exists", flag(slot is not None))
        put("/clip/color", rgb(slot.clip_color if slot is not None else (0, 0, 0)))
        current = {f"/clip/step/{channel}/{x}/{y}": line for (channel, x, y), line in self.clip_grid.steps().items()}
        for address in self._clip_steps:
            if address not in current:
                self._clip_steps[address] = "0"
        self._clip_steps.update(current)
        for address, line in self._clip_steps.items():
            put(address, line)

    def _position(self, channel: Channel | None) -> int:
        """TrackImpl.getPosition: Bitwig's position of the track in the flat list of all tracks,
        the effect tracks and the master (the observer showed 10 for FX 1 and 12 for the master
        of the fixture's 10 tracks and 2 effect tracks: flexi.track.trackbank,
        flexi.master.panning-select)."""
        if channel is None:
            return -1
        flat = self.daw.all_channels()
        return flat.index(channel) if channel in flat else -1

    def _transport(self, put) -> None:
        t = self.daw.transport
        put("/play", flag(t.playing))
        put("/record", flag(t.recording))
        put("/overdub", flag(t.overdub_arranger))
        put("/overdub/launcher", flag(t.overdub_launcher))
        put("/repeat", flag(t.loop))
        put("/punchIn", flag(t.punch_in))
        put("/punchOut", flag(t.punch_out))
        put("/click", flag(t.metronome))
        put("/click/ticks", flag(t.metronome_ticks))
        put("/click/volume", value127(t.metronome_volume.value))
        put("/click/preroll", flag(t.preroll_metronome))
        put("/preroll", str({"none": 0, "one_bar": 1, "two_bars": 2, "four_bars": 4}.get(t.preroll, 0)))
        put("/tempo/raw", number(t.tempo))
        put("/autowrite", flag(t.automation_arranger))
        put("/autowrite/launcher", flag(t.automation_launcher))
        # TransportImpl.getAutomationWriteMode: Read if no writing is switched on
        put("/automationWriteMode", t.automation_mode if t.automation_arranger or t.automation_launcher else "read")
        put("/time/signature", f"{t.numerator} / {t.denominator}")
        # LayoutModule: the panel layout in lower case, the visibility flags
        daw = self.daw
        put("/layout", daw.application.panel_layout.lower())
        arranger, mixer = daw.arranger, daw.mixer
        for name, value in (("cueMarkerVisibility", arranger.cue_markers), ("playbackFollow", arranger.playback_follow),
                            ("trackRowHeight", arranger.double_row_track_height), ("clipLauncherSectionVisibility", arranger.clip_launcher),
                            ("timeLineVisibility", arranger.timeline), ("ioSectionVisibility", arranger.io_section),
                            ("effectTracksVisibility", arranger.effect_tracks)):
            put("/arranger/" + name, flag(value))
        for name, value in (("clipLauncherSectionVisibility", mixer.clip_launcher_section), ("crossFadeSectionVisibility", mixer.cross_fade_section),
                            ("deviceSectionVisibility", mixer.device_section), ("ioSectionVisibility", mixer.io_section),
                            ("meterSectionVisibility", mixer.meter_section), ("sendsSectionVisibility", mixer.send_section)):
            put("/mixer/" + name, flag(value))

    def _channel(self, put, prefix: str, channel: Channel | None, with_selected: bool = True) -> None:
        exists = channel is not None
        put(prefix + "exists", flag(exists))
        put(prefix + "name", channel.name if exists else "")
        put(prefix + "color", rgb(channel.color if exists else (0, 0, 0)))
        put(prefix + "activated", flag(channel.activated if exists else True))
        if with_selected:
            put(prefix + "selected", flag(exists and channel.selected))
        put(prefix + "mute", flag(exists and channel.mute))
        put(prefix + "solo", flag(exists and channel.solo))
        put(prefix + "volume", value127(channel.volume.value) if exists else "0")
        put(prefix + "volumeStr", channel.volume.displayed() if exists else "")
        put(prefix + "pan", value127(channel.pan.value) if exists else "0")
        put(prefix + "panStr", channel.pan.displayed() if exists else "")

    def _track(self, put, prefix: str, track: Channel | None, position: int, cursor: bool = False) -> None:
        """TrackModule.flushTrack (the modelled part)."""
        exists = track is not None
        self._channel(put, prefix, track, with_selected=not cursor)
        put(prefix + "type", TRACK_TYPES.get(track.type, "unknown") if exists else "unknown")
        put(prefix + "isGroup", flag(exists and track.is_group))
        put(prefix + "recarm", flag(exists and track.arm))
        put(prefix + "monitor", flag(exists and track.monitoring))
        auto_monitor = flag(track.monitor_mode == "AUTO" if exists else True)
        can_hold_notes = flag(exists and track.can_hold_notes)
        if cursor:
            # The master as the cursor track: Bitwig's cursor track kept the values of the track
            # selected before (canHoldNotes 1 and autoMonitor 1 after Synth, flexi.master.
            # panning-select of generic_flexi project; known only from that trace)
            if isinstance(track, MasterTrack):
                auto_monitor, can_hold_notes = self._cursor_flags
            else:
                self._cursor_flags = (auto_monitor, can_hold_notes)
        put(prefix + "autoMonitor", auto_monitor)
        put(prefix + "canHoldNotes", can_hold_notes)
        put(prefix + "canHoldAudioData", flag(exists and track.can_hold_audio))
        put(prefix + "position", str(position))
        if cursor:
            put(prefix + "pinned", "0")
        sends = track.sends if exists else []
        for number in range(1, PAGE_SIZE + 1):
            send = sends[number - 1] if number <= len(sends) else None
            send_prefix = f"{prefix}send/{number}/"
            put(send_prefix + "activated", flag(send is not None and send.enabled))
            put(send_prefix + "exists", flag(send is not None))
            put(send_prefix + "name", send.name if send is not None else "")
            put(send_prefix + "volumeStr", send.volume.displayed() if send is not None else "")
            put(send_prefix + "volume", value127(send.volume.value) if send is not None else "0")
        slots = track.slots if exists else []
        for number in range(1, PAGE_SIZE + 1):
            slot = slots[number - 1] if number <= len(slots) else None
            clip = f"{prefix}clip/{number}/"
            put(clip + "name", slot.name if slot is not None else "")
            for key, attribute in (("isSelected", "selected"), ("hasContent", "has_content"), ("isPlaying", "playing"), ("isRecording", "recording"),
                                   ("isPlayingQueued", "play_queued"), ("isRecordingQueued", "rec_queued"), ("isStopQueued", "stop_queued")):
                put(clip + key, flag(slot is not None and getattr(slot, attribute)))
            put(clip + "color", rgb(slot.color if slot is not None else (0, 0, 0)))

    def _master(self, put) -> None:
        self._channel(put, "/master/", self.daw.master)

    def _markers(self, put) -> None:
        """MarkerModule: the page of 8 markers of the observer's marker bank (offset 0)."""
        markers = self.daw.markers.items
        for number in range(1, PAGE_SIZE + 1):
            marker = markers[number - 1] if number <= len(markers) else None
            prefix = f"/marker/{number}/"
            put(prefix + "exists", flag(marker is not None))
            put(prefix + "name", marker.name if marker is not None else "")
            put(prefix + "color", rgb(marker.color if marker is not None else (0, 0, 0)))

    def _scenes(self, put) -> None:
        scenes = self.daw.scenes
        for number in range(1, PAGE_SIZE + 1):
            scene = scenes[number - 1] if number <= len(scenes) else None
            prefix = f"/scene/{number}/"
            put(prefix + "exists", flag(scene is not None))
            put(prefix + "name", scene.name if scene is not None else "")
            put(prefix + "color", rgb(scene.color if scene is not None else (0, 0, 0)))
            put(prefix + "selected", flag(scene is not None and scene.selected))

    def _device(self, put, prefix: str, device, siblings: bool) -> None:
        """DeviceModule.flushDevice (the modelled part)."""
        exists = device is not None
        put(prefix + "exists", flag(exists))
        put(prefix + "name", device.name if exists else "")
        put(prefix + "bypass", flag(not (exists and device.enabled)))
        put(prefix + "expand", flag(exists and device.expanded))
        put(prefix + "parameters", flag(exists and device.parameters_visible))
        put(prefix + "window", flag(exists and device.window_open))
        if siblings:
            chain = self.cursor.devices.devices if self.cursor is not None else []
            position = chain.index(device) if device in chain else -1
            for number in range(1, PAGE_SIZE + 1):
                sibling = chain[number - 1] if number <= len(chain) else None
                sibling_prefix = f"{prefix}sibling/{number}/"
                put(sibling_prefix + "exists", flag(sibling is not None))
                put(sibling_prefix + "name", sibling.name if sibling is not None else "")
                put(sibling_prefix + "bypass", flag(not (sibling is not None and sibling.enabled)))
                put(sibling_prefix + "selected", flag(sibling is not None and number - 1 == position))
        pages = device.page_names(cursor=siblings) if exists else []
        selected = device.selected_page if exists else -1
        for number in range(1, PAGE_SIZE + 1):
            name = pages[number - 1] if number <= len(pages) else ""
            page_prefix = f"{prefix}page/{number}/"
            put(page_prefix + "exists", flag(name.strip() != ""))
            put(page_prefix, name)
            put(page_prefix + "name", name)
            put(page_prefix + "selected", flag(selected == number - 1))
        put(prefix + "page/selected/name", device.selected_page_name(cursor=siblings) if exists else "")
        # The parameter bank: the 8 slots of the selected page (flushParameterData)
        for number in range(1, PAGE_SIZE + 1):
            index = selected * PAGE_SIZE + number - 1
            control = device.remote_control(index) if exists and selected >= 0 and index < len(pages) * PAGE_SIZE else None
            param_prefix = f"{prefix}param/{number}/"
            put(param_prefix + "exists", flag(control is not None))
            put(param_prefix + "name", control.name if control is not None else "")
            put(param_prefix + "valueStr", control.parameter.displayed() if control is not None else "")
            put(param_prefix + "value", value127(control.parameter.value) if control is not None else "0")
            put(param_prefix + "modulatedValue", value127(control.parameter.value) if control is not None else "0")

    @staticmethod
    def _layer(put, prefix: str, layer, empty_activated: bool = True) -> None:
        """DeviceModule.flushDeviceLayer (without the sends and the VU); an empty item of the
        bank is activated, the empty layer of ``selected`` (EmptyLayer) is not, as the Java
        traces show."""
        exists = layer is not None and layer.exists
        put(prefix + "exists", flag(exists))
        put(prefix + "activated", flag(layer.activated if exists else empty_activated))
        put(prefix + "selected", flag(exists and layer.selected))
        put(prefix + "name", layer.name if exists else "")
        put(prefix + "volumeStr", layer.volume.displayed() if exists else "")
        put(prefix + "volume", value127(layer.volume.value) if exists else "0")
        put(prefix + "panStr", layer.pan.displayed() if exists else "")
        put(prefix + "pan", value127(layer.pan.value) if exists else "0")
        put(prefix + "mute", flag(exists and layer.mute))
        put(prefix + "solo", flag(exists and layer.solo))
        put(prefix + "color", rgb(layer.color if exists else (0, 0, 0)))

    def _device_layers(self, put) -> None:
        """The layer bank of the observer's cursor device (a page of 8 at offset 0) and its
        selected layer. The drum pads of the cursor device (``/device/drumpad``) are not
        modelled: the observer sends them only while the cursor device has drum pads, the Java
        traces show the values of an earlier Drum Machine on other devices."""
        device = self.cursor_device
        layers = device.layer_list() if device is not None else []
        for number in range(1, PAGE_SIZE + 1):
            self._layer(put, f"/device/layer/{number}/", layers[number - 1] if number <= len(layers) else None)
        selected = next((layer for layer in layers if layer.selected), None)
        self._layer(put, "/device/layer/selected/", selected, empty_activated=False)

    def _conformance_primary(self, put) -> None:
        """ConformanceModule.flushChannels: the layers (a page of 8 at offset 0) and the drum pads
        (a page of 16 at offset 36, DrumPadBankImpl's start at C1) of the first instrument."""
        device = self.primary_device
        layers = device.layer_list() if device is not None else []
        pads = device.drum_pads if device is not None and device.drum_pads is not None else []
        for prefix, items, offset, size in (("/conformance/primary/layer/", layers, 0, PAGE_SIZE), ("/conformance/primary/drumpad/", pads, DRUM_PAD_OFFSET, DRUM_PAD_PAGE_SIZE)):
            put(prefix + "offset", str(offset))
            for number in range(1, size + 1):
                index = offset + number - 1
                item = items[index] if index < len(items) else None
                exists = item is not None and item.exists
                item_prefix = f"{prefix}{number}/"
                put(item_prefix + "exists", flag(exists))
                put(item_prefix + "name", item.name if exists else "")
                put(item_prefix + "selected", flag(exists and item.selected))
                put(item_prefix + "mute", flag(exists and item.mute))
                put(item_prefix + "solo", flag(exists and item.solo))
                put(item_prefix + "volume", value127(item.volume.value) if exists else "0")
                put(item_prefix + "pan", value127(item.pan.value) if exists else "0")

    def _equalizer(self, put) -> None:
        """DeviceModule.flushDevice for ``/eq/`` (the first EQ+ of the cursor track,
        EqualizerDeviceImpl): the device, the type of each band (EqualizerBandType, lower case)
        and its gain, frequency and Q (flushParameterData). Without an EQ+ the parameters are
        empty and the types ``off``, as the Java traces show."""
        device = self.eq_device
        exists = device is not None
        put("/eq/exists", flag(exists))
        put("/eq/name", device.name if exists else "")
        put("/eq/bypass", flag(not (exists and device.enabled)))
        put("/eq/expand", flag(exists and device.expanded))
        put("/eq/parameters", flag(exists and device.parameters_visible))
        put("/eq/window", flag(exists and device.window_open))
        for band in range(EQ_NUM_BANDS):
            number = band + 1
            put(f"/eq/type/{number}/value", device.equalizer.types[band] if exists else "off")
            for kind, parameters in (("gain", "gains"), ("freq", "frequencies"), ("q", "qs")):
                parameter = getattr(device.equalizer, parameters)[band] if exists else None
                prefix = f"/eq/{kind}/{number}/"
                put(prefix + "exists", flag(parameter is not None))
                put(prefix + "name", parameter.name if parameter is not None else "")
                put(prefix + "value", value127(parameter.value) if parameter is not None else "0")
                put(prefix + "valueStr", parameter.displayed() if parameter is not None else "")
                put(prefix + "modulatedValue", value127(parameter.value) if parameter is not None else "0")


__all__ = ["MODELLED", "OscController", "PAGE_SIZE", "is_modelled", "number", "rgb", "value127"]
