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

Modelled (step 3 of PYTHON_HOST_PLAN.md §5): the transport switches and the tempo, the tracks of
the bank with sends and slots, the cursor track (``/track/selected``), the master, the scenes,
the cursor device and the first instrument with their siblings and pages. Not modelled yet:
device parameters, layers, drum pads, the EQ, the clip and its steps, markers, the project and
track remote controls, the browser, the layout and panels, the crossfader, the playing notes,
the ``/conformance`` values, ``/vkb_midi``, VU meters (ignored by the comparison anyway).

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
from dbm_host.model import Channel, MasterTrack, Track

PAGE_SIZE = 8
"""The bank size of the OSC controller (OSCConfiguration "Bank page size", default 8)."""

# The delay of the selection after a page scroll (AbstractItemBankImpl.selectPreviousPage)
SELECT_AFTER_SCROLL_MS = 75

# The modelled keys (without the "osc:" prefix). N is a 1-based index.
_N = r"[1-9]\d*"
_CHANNEL = r"(exists|name|color|activated|selected|mute|solo|volume|volumeStr|pan|panStr)"
_TRACK = (r"(type|isGroup|recarm|monitor|autoMonitor|canHoldNotes|canHoldAudioData|position"
          rf"|send/{_N}/(exists|name|volume|volumeStr|activated)"
          rf"|clip/{_N}/(name|isSelected|hasContent|isPlaying|isRecording|isPlayingQueued|isRecordingQueued|isStopQueued|color))")
_DEVICE = rf"(exists|name|bypass|page/{_N}/(exists|name|selected)?|page/selected/name)"
MODELLED = [
    r"/(play|record|repeat|punchIn|punchOut|overdub|overdub/launcher|autowrite|autowrite/launcher)",
    r"/(click|click/ticks|click/volume|click/preroll|preroll|tempo/raw|time/signature)",
    rf"/track/{_N}/({_CHANNEL}|{_TRACK})",
    rf"/track/selected/((exists|name|color|activated|mute|solo|volume|volumeStr|pan|panStr)|{_TRACK}|pinned)",
    rf"/master/{_CHANNEL}",
    rf"/scene/{_N}/(exists|name|color|selected)",
    rf"/(device|primary)/{_DEVICE}",
    rf"/device/sibling/{_N}/(exists|name|selected|bypass)",
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
        self._cursor: Channel | None = daw.cursor_channel
        self._tasks: list[tuple[float, Callable[[], None]]] = []
        self.follow()

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
        cursor = self.daw.cursor_channel
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
            self._track(put, f"/track/{number}/", self.track(number), self.track_offset + number - 1)
        cursor = self.daw.cursor_channel
        self._track(put, "/track/selected/", cursor, self._position(cursor), cursor=True)
        self._master(put)
        self._scenes(put)
        self._device(put, "/device/", self.daw.cursor_device, siblings=True)
        self._device(put, "/primary/", self.daw.primary_device, siblings=False)
        return state

    def _position(self, channel: Channel | None) -> int:
        """TrackImpl.getPosition: the position in the list of its bank."""
        if channel is None:
            return -1
        for tracks in (self.daw.tracks, self.daw.effect_tracks):
            if channel in tracks:
                return tracks.index(channel)
        return 0

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
        put("/time/signature", f"{t.numerator} / {t.denominator}")

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
        put(prefix + "autoMonitor", flag(track.monitor_mode == "AUTO" if exists else True))
        put(prefix + "canHoldNotes", flag(exists and track.can_hold_notes))
        put(prefix + "canHoldAudioData", flag(exists and track.can_hold_audio))
        put(prefix + "position", str(position))
        if cursor:
            put(prefix + "pinned", flag(self.daw.cursor.pinned))
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
        if siblings:
            cursor = self.daw.cursor_channel
            chain = cursor.devices.devices if cursor is not None else []
            position = chain.index(device) if device in chain else -1
            for number in range(1, PAGE_SIZE + 1):
                sibling = chain[number - 1] if number <= len(chain) else None
                sibling_prefix = f"{prefix}sibling/{number}/"
                put(sibling_prefix + "exists", flag(sibling is not None))
                put(sibling_prefix + "name", sibling.name if sibling is not None else "")
                put(sibling_prefix + "bypass", flag(not (sibling is not None and sibling.enabled)))
                put(sibling_prefix + "selected", flag(sibling is not None and number - 1 == position))
        pages = device.pages if exists else []
        selected = device.selected_page if exists else -1
        for number in range(1, PAGE_SIZE + 1):
            name = pages[number - 1] if number <= len(pages) else ""
            page_prefix = f"{prefix}page/{number}/"
            put(page_prefix + "exists", flag(name.strip() != ""))
            put(page_prefix, name)
            put(page_prefix + "name", name)
            put(page_prefix + "selected", flag(selected == number - 1))
        put(prefix + "page/selected/name", device.selected_page_name if exists else "")


__all__ = ["MODELLED", "OscController", "PAGE_SIZE", "is_modelled", "number", "rgb", "value127"]
