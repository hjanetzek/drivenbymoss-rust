# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Reports the simulated DAW to the core: change-only lines ``<address> <value>``, a full dump on
request, ``/host/features``.

Implements HOST_BRIDGE_GUIDE.md §2 (the reporting protocol: lines and values, change-only
reporting and dumps, the host features) with the addresses of §3.1 (transport), §3.3 (tracks,
sends, effect tracks, master) and §3.4 (scenes, slots).

Simulates the Bitwig extension of this repository: ``ModelReporter`` (the change tracking) and
``BitwigModel.collect`` (collectTransport, collectTracks, collectSlots, collectEffectTracks,
collectScenes, collectMaster), which follow the Java ``TransportImpl``, ``TrackImpl``,
``ChannelImpl``, ``SendImpl``, ``SlotImpl``, ``SceneImpl`` of DrivenByMoss for Bitwig.

Differences from the Bitwig extension:

- All items are reported with their absolute positions, not a window (PYTHON_HOST_PLAN.md §3
  "Windows"); ``/track/offset`` etc. report the offset the core requested with ``bank``.
- The selection is reported after the tracks: the old track ``select 0`` within the list, the
  selected one ``select 1`` after ``/track/count`` (HOST_BRIDGE_GUIDE.md §2: deselect, then
  select, in one batch).
- Not reported yet (later steps): the application (except undo/redo), arranger, mixer and
  project flags, the crossfader, groove, the cursor device and its parameters, layers, drum
  pads, the clip and its step grids, markers, the browser, note repeat, playing notes. They are
  added as collectors (``Reporter.collectors``).
"""

from typing import Callable

from .banks import Banks
from .model import PREROLLS, Channel, SimDaw
from .parameter import Parameter

# The texts shortened by the DAW (BitwigModel.LIMITS)
LIMITS = (6, 8)

# The delay before the values of a commanded item are reported again (CommandHandler.reportAgain)
REPORT_AGAIN_DELAY_MS = 300


def format_beats(beats: float, quarters_per_measure: int, offset: int) -> str:
    """TimeFormat.formatMeasuresLong: ``bars.beats.sixteenths:ticks``."""
    measure = int(beats // quarters_per_measure)
    t = beats - measure * quarters_per_measure
    quarters = int(t // 1)
    t -= quarters
    eights = int(t // 0.25)
    t -= eights * 0.25
    frames = int(t / 0.25 * 100.0 // 1)
    return f"{measure + offset}.{quarters + offset:02d}.{eights + offset:02d}:{frames:02d}"


def format_time(tempo: float, beats: float) -> str:
    """TimeFormat.formatTimeLong: ``hours.minutes.seconds:milliseconds``."""
    time = beats * 60.0 / tempo
    seconds = int((time % 60) // 1)
    t = (time - seconds) / 60.0
    minutes = int((t % 60) // 1)
    t = (t - minutes) / 60.0
    hours = int(t // 1)
    millis = int((time - ((hours * 60 + minutes) * 60 + seconds)) * 1000)
    return f"{hours}.{minutes:02d}.{seconds:02d}:{millis:03d}"


def format_value(value) -> str:
    """The text of a value: flags 1/0, ints, doubles with 6 decimals (``%.6f``), colors ``r g b``,
    texts with line breaks replaced."""
    if isinstance(value, bool):
        return "1" if value else "0"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        return f"{value:.6f}"
    if isinstance(value, tuple):
        return " ".join(str(int(part)) for part in value)
    return ("" if value is None else str(value)).replace("\n", " ").replace("\r", " ")


class Reporter:
    """Collects the lines for ``dbm_update_model``.

    Remembers the last value of each address and reports only changes, unless a dump was
    requested (the core's ``refresh``). Starts with a dump. The model is collected by the
    ``collectors``, in order; the later steps append theirs.
    """

    def __init__(self, daw: SimDaw, banks: Banks, clock: Callable[[], float] = lambda: 0.0,
                 features: Callable[[], list[str]] = lambda: []):
        """``clock`` returns the simulated time in ms (for ``forget_later``), ``features`` the
        host features of the command handler."""
        self.daw = daw
        self.banks = banks
        self.clock = clock
        self.features = features
        self.last_values: dict[str, str] = {}
        self.lines: list[str] = []
        self.dump_all = True
        self.pending: dict[str, float] = {}
        self._last_selected: int = -1
        self.collectors: list[Callable[["Reporter"], None]] = [
            Reporter.collect_host_features,
            Reporter.collect_transport,
            Reporter.collect_tracks,
            Reporter.collect_effect_tracks,
            Reporter.collect_scenes,
            Reporter.collect_master,
        ]

    # Change tracking (ModelReporter)

    def request_dump(self) -> None:
        """Report everything with the next collection."""
        self.dump_all = True

    def forget(self, prefix: str) -> None:
        """Report the values under the prefix again with the next collection."""
        for address in [a for a in self.last_values if a.startswith(prefix)]:
            del self.last_values[address]

    def forget_later(self, prefix: str, delay_ms: float = REPORT_AGAIN_DELAY_MS) -> None:
        """As forget, after the delay counted from the last call for the prefix (HOST_BRIDGE_GUIDE
        §6.1)."""
        self.pending[prefix] = self.clock() + delay_ms

    def apply_pending_forgets(self) -> None:
        now = self.clock()
        for prefix, due in list(self.pending.items()):
            if due <= now:
                self.forget(prefix)
                del self.pending[prefix]

    def put(self, address: str, value) -> None:
        text = format_value(value)
        last = self.last_values.get(address)
        self.last_values[address] = text
        if self.dump_all or text != last:
            self.lines.append(f"{address} {text}")

    def put_displayed(self, address: str, text: str, limited: list[str] | None = None) -> None:
        """A displayed value and its shortened texts (``<address>/6``, ``/8``)."""
        self.put(address, text)
        for limit, short in zip(LIMITS, limited or [text[:limit] for limit in LIMITS]):
            self.put(f"{address}/{limit}", short)

    def put_parameter_text(self, address: str, parameter: Parameter) -> None:
        self.put_displayed(address, parameter.displayed(), [parameter.displayed_limited(limit) for limit in LIMITS])

    def take(self) -> str:
        """The collected lines (empty if nothing changed); starts a new collection."""
        result = "".join(line + "\n" for line in self.lines)
        self.lines = []
        self.dump_all = False
        return result

    def collect(self) -> str:
        """Collect the model and take the lines."""
        self.apply_pending_forgets()
        for collector in self.collectors:
            collector(self)
        return self.take()

    def snapshot(self) -> dict[str, str]:
        """The last reported value of every address (for tests and the conformance state view)."""
        return dict(self.last_values)

    # The model (BitwigModel.collect)

    def collect_host_features(self) -> None:
        self.put("/host/features", " ".join(self.features()))

    def collect_transport(self) -> None:
        """BitwigModel.collectTransport (without the application and project values)."""
        t = self.daw.transport
        qpm = max(1, int(4 * t.numerator / t.denominator))
        self.put("/play", t.playing)
        self.put("/record", t.recording)
        self.put("/repeat", t.loop)
        self.put("/tempo", float(t.tempo))
        self.put("/numerator", t.numerator)
        self.put("/denominator", t.denominator)
        self.put("/time", t.seconds())
        self.put("/beats/position", float(t.play_start))
        self.put("/beats/loop/start", float(t.loop_start))
        self.put("/beats/loop/length", float(t.loop_length))
        self.put("/time/text", format_time(t.tempo, t.position) if t.tempo > 0 else "")
        self.put("/beats/text", format_beats(t.position, qpm, 1))
        self.put("/beats/loop/start/text", format_beats(t.loop_start, qpm, 1))
        self.put("/beats/loop/length/text", format_beats(t.loop_length, qpm, 0))
        self.put("/click", t.metronome)
        self.put("/overdub/arranger", t.overdub_arranger)
        self.put("/overdub/launcher", t.overdub_launcher)
        self.put("/automode/arranger", t.automation_arranger)
        self.put("/automode/launcher", t.automation_launcher)
        modes = {"touch": 2, "write": 3, "latch": 4}
        self.put("/automode", modes.get(t.automation_mode, 1) if t.automation_arranger or t.automation_launcher else 1)
        self.put("/click/preroll", 2 if t.preroll_metronome else 0)
        self.put("/click/prerollMeasures", max(0, PREROLLS.index(t.preroll)) if t.preroll in PREROLLS else 0)
        self.put("/click/volume", t.metronome_volume.value)
        # As TransportImpl.getMetronomeVolumeStr: the text limited to 6 and the unit
        self.put("/click/volumeStr", t.metronome_volume.displayed_limited(6) + " dB")
        self.put("/click/ticks", t.metronome_ticks)
        self.put("/punchIn", t.punch_in)
        self.put("/punchOut", t.punch_out)
        self.put("/fill", t.fill)
        self.put("/project/canUndo", self.daw.application.can_undo)
        self.put("/project/canRedo", self.daw.application.can_redo)

    def track_list(self) -> list[Channel]:
        return self.daw.track_list(self.banks.full_flat_track_list)

    def collect_channel(self, prefix: str, channel: Channel) -> None:
        """BitwigModel.collectChannel (with the shortened texts)."""
        self.put(prefix + "name", channel.name)
        self.put(prefix + "color", channel.color)
        self.put(prefix + "mute", channel.mute)
        self.put(prefix + "solo", channel.solo)
        self.put(prefix + "active", channel.activated)
        self.put(prefix + "volume", channel.volume.value)
        self.put_parameter_text(prefix + "volume/str", channel.volume)
        self.put(prefix + "pan", channel.pan.value)
        self.put_parameter_text(prefix + "pan/str", channel.pan)

    def collect_vu(self, prefix: str, channel: Channel) -> None:
        self.put(prefix + "vuleft", float(channel.vu_left))
        self.put(prefix + "vuright", float(channel.vu_right))
        self.put(prefix + "vu", (channel.vu_left + channel.vu_right) / 2.0)

    def collect_sends(self, prefix: str, channel: Channel, with_color: bool) -> None:
        count = min(len(channel.sends), self.banks.num_sends)
        for index, send in enumerate(channel.sends[:count]):
            send_prefix = f"{prefix}send/{index}/"
            self.put(send_prefix + "name", send.name)
            self.put(send_prefix + "volume", send.volume.value)
            self.put_parameter_text(send_prefix + "volume/str", send.volume)
            self.put(send_prefix + "active", send.enabled)
            if with_color:
                self.put(send_prefix + "color", send.color)
        self.put(prefix + "send/count", count)

    def collect_track_flags(self, prefix: str, track) -> None:
        self.put(prefix + "recarm", track.arm)
        self.put(prefix + "monitor", track.monitoring)
        self.put(prefix + "autoMonitor", track.monitor_mode == "AUTO")
        self.put(prefix + "canHoldNotes", track.can_hold_notes)
        self.put(prefix + "canHoldAudio", track.can_hold_audio)

    @staticmethod
    def track_type(track) -> str:
        return "GROUP" if track.is_group else track.type

    def collect_tracks(self) -> None:
        """BitwigModel.collectTracks and collectSlots, for all tracks."""
        tracks = self.track_list()
        daw = self.daw
        selected = tracks.index(daw.selected_channel) if daw.selected_channel in tracks else -1
        cursor = tracks.index(daw.cursor_channel) if daw.cursor_channel in tracks else -1
        self.put("/track/pinned", cursor if daw.cursor.pinned and cursor >= 0 else -1)
        self.put("/track/hasPrevious", cursor > 0)
        self.put("/track/hasNext", 0 <= cursor < len(tracks) - 1)
        self.put("/track/offset", self.banks.track_offset)
        # A deselection outside of the list (e.g. the track was removed)
        if self._last_selected != selected and self._last_selected >= len(tracks):
            self.put(f"/track/{self._last_selected}/select", False)
        self._last_selected = selected

        num_scenes = len(daw.scenes)
        for position, track in enumerate(tracks):
            prefix = f"/track/{position}/"
            self.put(prefix + "exists", 1)
            self.put(prefix + "number", position)
            self.put(prefix + "type", self.track_type(track))
            self.put(prefix + "isGroupExpanded", track.group_expanded)
            if position != selected:
                self.put(prefix + "select", False)
            self.collect_track_flags(prefix, track)
            self.collect_vu(prefix, track)
            self.collect_channel(prefix, track)
            self.collect_sends(prefix, track, with_color=True)
            self.put(prefix + "isStopped", track.is_stopped)
            self.put(prefix + "slot/count", num_scenes)
            for index in range(num_scenes):
                self.collect_slot(f"{prefix}slot/{index}/", track.slots[index] if index < len(track.slots) else None)
        self.put("/track/count", len(tracks))
        if selected >= 0:
            self.put(f"/track/{selected}/select", True)

    def collect_slot(self, prefix: str, slot) -> None:
        self.put(prefix + "exists", slot is not None)
        self.put(prefix + "hasContent", slot is not None and slot.has_content)
        self.put(prefix + "name", "" if slot is None else slot.name)
        self.put(prefix + "color", (0, 0, 0) if slot is None else slot.color)
        for field, attribute in (("selected", "selected"), ("playing", "playing"), ("recording", "recording"),
                                 ("playQueued", "play_queued"), ("recQueued", "rec_queued"), ("stopQueued", "stop_queued")):
            self.put(prefix + field, slot is not None and getattr(slot, attribute))

    def collect_effect_tracks(self) -> None:
        """BitwigModel.collectEffectTracks, for all effect tracks."""
        self.put("/fxtrack/offset", self.banks.fx_track_offset)
        for position, track in enumerate(self.daw.effect_tracks):
            prefix = f"/fxtrack/{position}/"
            self.put(prefix + "exists", 1)
            self.put(prefix + "number", position)
            self.put(prefix + "type", self.track_type(track))
            self.put(prefix + "select", track.selected)
            self.collect_track_flags(prefix, track)
            self.collect_channel(prefix, track)
            self.collect_sends(prefix, track, with_color=False)
        self.put("/fxtrack/count", len(self.daw.effect_tracks))

    def collect_scenes(self) -> None:
        """BitwigModel.collectScenes, for all scenes."""
        self.put("/scene/offset", self.banks.scene_offset)
        for position, scene in enumerate(self.daw.scenes):
            prefix = f"/scene/{position}/"
            self.put(prefix + "exists", True)
            self.put(prefix + "number", position)
            self.put(prefix + "name", scene.name)
            self.put(prefix + "color", scene.color)
            self.put(prefix + "select", scene.selected)
        self.put("/scene/count", len(self.daw.scenes))

    def collect_master(self) -> None:
        master = self.daw.master
        self.put("/master/select", master.selected)
        self.collect_vu("/master/", master)
        self.collect_channel("/master/", master)


__all__ = ["LIMITS", "REPORT_AGAIN_DELAY_MS", "Reporter", "format_beats", "format_time", "format_value"]
