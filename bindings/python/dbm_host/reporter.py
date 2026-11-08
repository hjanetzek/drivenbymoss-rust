# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Reports the simulated DAW to the core: change-only lines ``<address> <value>``, a full dump on
request, ``/host/features``.

Implements HOST_BRIDGE_GUIDE.md §2 (the reporting protocol: lines and values, change-only
reporting and dumps, the host features) with the addresses of §3.1 (transport, application,
arranger, mixer), §3.3 (tracks, sends, effect tracks, master, the playing notes of the cursor
track), §3.4 (scenes, slots), the markers, §3.5 (the cursor device, its siblings, remote
controls and layers; the drum machine and the first instrument with their pads and layers; the
EQ+), §3.6 (the cursor clip and its step grids), the note repeat.

Simulates the Bitwig extension of this repository: ``ModelReporter`` (the change tracking) and
``BitwigModel.collect`` (collectTransport, collectTracks, collectSlots, collectEffectTracks,
collectMarkers, collectScenes, collectMaster, collectDevice, ``SpecificDevice.collect`` (the layers of the cursor
device, the drum machine, the first instrument), collectNoteRepeat,
``EqualizerDevice.collect``, ``BitwigNoteClip.collectClip``, ``collectGrid``), which follow the Java ``TransportImpl``, ``TrackImpl``,
``ChannelImpl``, ``SendImpl``, ``SlotImpl``, ``SceneImpl`` of DrivenByMoss for Bitwig.

Differences from the Bitwig extension:

- All items are reported with their absolute positions, not a window (PYTHON_HOST_PLAN.md §3
  "Windows"); ``/track/offset`` etc. report the offset the core requested with ``bank``.
- The selection is reported after the tracks: the old track ``select 0`` within the list, the
  selected one ``select 1`` after ``/track/count`` (HOST_BRIDGE_GUIDE.md §2: deselect, then
  select, in one batch).
- Not reported yet (later steps): the application (except undo/redo and the panel layout), the
  project flags, the crossfader, groove, the sends of layers and pads, the browser. They are added as collectors (``Reporter.collectors``).
"""

from typing import Callable

from .banks import Banks
from .device import EQ_NUM_BANDS
from .model import PREROLLS, Channel, SimDaw
from .parameter import Parameter

# The texts shortened by the DAW (BitwigModel.LIMITS)
LIMITS = (6, 8)

# The delay before the values of a commanded item are reported again (CommandHandler.reportAgain)
REPORT_AGAIN_DELAY_MS = 300

# BitwigModel: the sibling window of the cursor device, the parameters of a page, the pads and
# layers of the specific devices
NUM_SIBLINGS = 8
PARAMS_PER_PAGE = 8
NUM_DRUM_PADS = 128
NUM_LAYERS = 32
NUM_MARKERS = 100


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


# No last raw value (None is a value)
_NONE = object()

# The values put keeps unformatted: they cannot change after the call (a tuple holds numbers)
_IMMUTABLE = frozenset((bool, int, float, str, tuple, type(None)))


class Reporter:
    """Collects the lines for ``dbm_update_model``.

    Remembers the last value of each address and reports only changes, unless a dump was
    requested (the core's ``refresh``); an unchanged raw value is not even formatted. Starts with
    a dump. The model is collected by the ``collectors``, in order; the later steps append theirs.
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
        self.last_raw: dict[str, object] = {}
        """The last value of each address before formatting (``put`` compares it first; only
        immutable values: flags, numbers, texts, colors)."""
        self._limited_addresses: dict[str, tuple[str, ...]] = {}
        self.lines: list[str] = []
        self.dump_all = True
        self.pending: dict[str, float] = {}
        self._last_selected: int = -1
        self._last_cursor: int = -1
        self.note_grids: list = []
        """The step grids of the cursor clip (``clip.NoteGrid``, from the model setup's
        ``noteClips``; set by ``CommandHandler.configure``)."""
        self._grid_steps: dict[str, set[str]] = {}
        self.collectors: list[Callable[["Reporter"], None]] = [
            Reporter.collect_host_features,
            Reporter.collect_transport,
            Reporter.collect_tracks,
            Reporter.collect_effect_tracks,
            Reporter.collect_markers,
            Reporter.collect_scenes,
            Reporter.collect_master,
            Reporter.collect_device,
            Reporter.collect_device_layers,
            Reporter.collect_note_repeat,
            Reporter.collect_drum,
            Reporter.collect_primary,
            Reporter.collect_eq,
            Reporter.collect_clip,
            Reporter.collect_grids,
        ]

    # Change tracking (ModelReporter)

    def request_dump(self) -> None:
        """Report everything with the next collection."""
        self.dump_all = True

    def forget(self, prefix: str) -> None:
        """Report the values under the prefix again with the next collection."""
        for address in [a for a in self.last_values if a.startswith(prefix)]:
            del self.last_values[address]
            self.last_raw.pop(address, None)

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
        # The same raw value as last time (of the same type: True, 1 and 1.0 have other texts)
        # gives the same text: no formatting, nothing to report
        if not self.dump_all:
            last_raw = self.last_raw.get(address, _NONE)
            if last_raw is not _NONE and type(last_raw) is type(value) and last_raw == value:
                return
        text = format_value(value)
        last = self.last_values.get(address)
        self.last_values[address] = text
        if type(value) in _IMMUTABLE:
            self.last_raw[address] = value
        else:
            self.last_raw.pop(address, None)
        if self.dump_all or text != last:
            self.lines.append(f"{address} {text}")

    def put_displayed(self, address: str, text: str, limited: list[str] | None = None) -> None:
        """A displayed value and its shortened texts (``<address>/6``, ``/8``)."""
        self.put(address, text)
        addresses = self._limited_addresses.get(address)
        if addresses is None:
            addresses = self._limited_addresses[address] = tuple(f"{address}/{limit}" for limit in LIMITS)
        for limited_address, short in zip(addresses, limited or [text[:limit] for limit in LIMITS]):
            self.put(limited_address, short)

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
        self.collect_application()

    def collect_application(self) -> None:
        """BitwigModel.collectApplication (the simulated part): the panel layout, the visibility
        flags of the arranger and the mixer."""
        daw = self.daw
        self.put("/layout", daw.application.panel_layout)
        self.put("/followPlayback", daw.arranger.playback_follow)
        arranger, mixer = daw.arranger, daw.mixer
        for name, value in (("cueMarkers", arranger.cue_markers), ("doubleRowTrackHeight", arranger.double_row_track_height),
                            ("clipLauncher", arranger.clip_launcher), ("timeline", arranger.timeline), ("ioSection", arranger.io_section),
                            ("effectTracks", arranger.effect_tracks)):
            self.put("/arranger/" + name, value)
        for name, value in (("clipLauncherSection", mixer.clip_launcher_section), ("crossFadeSection", mixer.cross_fade_section),
                            ("deviceSection", mixer.device_section), ("ioSection", mixer.io_section), ("meterSection", mixer.meter_section),
                            ("sendSection", mixer.send_section)):
            self.put("/mixer/" + name, value)

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
        # The notes which play on the cursor track, in the note format of the core; the core
        # keeps one note cache: the notes of a new cursor track are reported again
        if cursor != self._last_cursor:
            self._last_cursor = cursor
            self.forget(f"/track/{cursor}/playingnotes")
        if cursor >= 0:
            notes = getattr(tracks[cursor], "playing_notes", {})
            self.put(f"/track/{cursor}/playingnotes", ";".join(f"0:0:0:0:0:{key}:{velocity}" for key, velocity in notes.items()))

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

    def collect_markers(self) -> None:
        """BitwigModel.collectMarkers (if the model setup has ``numMarkers`` > 0): the markers of
        the bank of 100 with their absolute positions, the position in beats."""
        if int(self.banks.setup.get("numMarkers", 0)) <= 0:
            return
        markers = self.daw.markers.items
        for index in range(NUM_MARKERS):
            marker = markers[index] if index < len(markers) else None
            prefix = f"/marker/{index}/"
            self.put(prefix + "exists", marker is not None)
            if marker is None:
                continue
            self.put(prefix + "number", index)
            self.put(prefix + "name", marker.name)
            self.put(prefix + "color", marker.color)
            self.put(prefix + "position", float(marker.position))
        self.put("/marker/count", min(len(markers), NUM_MARKERS))

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

    def collect_note_repeat(self) -> None:
        """BitwigModel.collectNoteRepeat: the arpeggiator of the default note input (nothing
        without one); ``period`` is the inverse of the rate in beats, ``mode`` the index of the
        core's modes."""
        note_input = self.daw.note_input
        if note_input is None:
            return
        arpeggiator = note_input.arpeggiator
        self.put("/noterepeat/active", arpeggiator.enabled)
        if arpeggiator.rate > 0:
            self.put("/noterepeat/period", 1.0 / arpeggiator.rate)
        self.put("/noterepeat/notelength", float(arpeggiator.gate_length))
        self.put("/noterepeat/mode", arpeggiator.mode_index)
        self.put("/noterepeat/velocity", arpeggiator.use_pressure_to_velocity)

    # Devices (BitwigModel.collectDevice, SpecificDevice.collect)

    def num_param_pages(self) -> int:
        return max(0, int(self.banks.setup.get("numParamPages", 8)))

    def collect_pages(self, prefix: str, device, cursor: bool = False) -> None:
        """BitwigModel.collectPages and the page names: the parameters of the first
        ``numParamPages`` pages as one flat list, the page names, the selected page. The
        simulated parameters have no names and texts (device.py); the value of a parameter
        which a command set is reported. ``cursor``: the pages as the cursor device shows
        them."""
        num_pages = self.num_param_pages()
        pages = device.page_names(cursor) if device is not None else []
        used = min(len(pages), num_pages)
        for index in range(num_pages * PARAMS_PER_PAGE):
            param = f"{prefix}/param/{index}/"
            in_use = device is not None and index < used * PARAMS_PER_PAGE
            control = device.remote_control(index) if in_use and device.remote_controls else None
            if control is not None:
                parameter = control.parameter
                self.put(param + "name", control.name)
                self.put(param + "value", parameter.value)
                self.put_parameter_text(param + "value/str", parameter)
            else:
                parameter = device.parameters.get(index) if in_use and not device.remote_controls else None
                self.put(param + "name", "")
                self.put(param + "value", parameter.value if parameter is not None else 0)
                self.put_displayed(param + "value/str", "", ["", ""])
            self.put(param + "steps", -1)
            self.put(param + "modulated", parameter.value if parameter is not None else 0)
        self.put(prefix + "/param/count", used * PARAMS_PER_PAGE)
        for index in range(used):
            self.put(f"{prefix}/page/{index}/name", pages[index])
        self.put(prefix + "/page/count", used)
        self.put(prefix + "/page/selected", device.selected_page if device is not None else -1)

    def collect_device(self) -> None:
        """BitwigModel.collectDevice: the cursor device of the cursor track, its siblings (the
        window at ``DeviceChain.sibling_offset``) and pages."""
        chain = self.daw.cursor_device_chain
        devices = chain.devices if chain is not None else []
        device = self.daw.cursor_device
        exists = device is not None
        position = devices.index(device) if exists else -1
        self.put("/device/count", len(devices))
        self.put("/device/exists", exists)
        self.put("/device/position", position)
        self.put("/device/name", device.name if exists else "")
        self.put("/device/bypass", exists and not device.enabled)
        self.put("/device/window", exists and device.window_open)
        self.put("/device/pinned", self.daw.cursor.device_pinned)
        self.put("/device/expand", exists and device.expanded)
        offset = chain.sibling_offset if chain is not None else 0
        for index in range(NUM_SIBLINGS):
            sibling_position = offset + index
            sibling = devices[sibling_position] if sibling_position < len(devices) else None
            prefix = f"/device/sibling/{index + 1}/"
            self.put(prefix + "name", sibling.name if sibling is not None else "")
            self.put(prefix + "bypass", sibling is not None and not sibling.enabled)
            self.put(prefix + "position", sibling_position if sibling is not None else -1)
            self.put(prefix + "selected", sibling is not None and sibling_position == position)
        self.collect_pages("/device", device, cursor=True)

    def num_device_layers(self) -> int:
        return max(0, int(self.banks.setup.get("numDeviceLayers", 8)))

    def collect_specific(self, prefix: str, device, num_pads: int, num_layers: int, pages: bool, cursor: bool = False) -> None:
        """SpecificDevice.collect of the Bitwig extension: the device, its pages (``pages``),
        ``num_pads`` drum pads and ``num_layers`` layers (as the bank sizes of the Bitwig
        extension, all items reported)."""
        exists = device is not None
        self.put(prefix + "/exists", exists)
        self.put(prefix + "/name", device.name if exists else "")
        self.put(prefix + "/bypass", exists and not device.enabled)
        self.put(prefix + "/window", exists and device.window_open)
        self.put(prefix + "/expand", exists and device.expanded)
        self.put(prefix + "/parameters", exists and device.parameters_visible)
        self.put(prefix + "/hasDrumPads", exists and device.has_drum_pads)
        self.put(prefix + "/hasLayers", exists and device.has_layers)
        if pages:
            self.collect_pages(prefix, device, cursor)
        if num_pads > 0:
            self.put(prefix + "/hasMutedPads", exists and device.has_muted_pads)
            self.put(prefix + "/hasSoloedPads", exists and device.has_soloed_pads)
            pads = device.drum_pads if exists and device.drum_pads is not None else []
            for index in range(num_pads):
                self.collect_layer(f"{prefix}/pad/{index}/", pads[index] if index < len(pads) else None)
        if num_layers <= 0:
            return
        layers = device.layer_list() if exists else []
        self.put(prefix + "/layer/count", min(len(layers), num_layers))
        for index in range(num_layers):
            self.collect_layer(f"{prefix}/layer/{index}/", layers[index] if index < len(layers) else None)

    def collect_layer(self, prefix: str, layer) -> None:
        """SpecificDevice.collectItem: an empty item has the values of an empty Bitwig channel
        (layer.py)."""
        exists = layer is not None and layer.exists
        self.put(prefix + "exists", exists)
        self.put(prefix + "select", exists and layer.selected)
        self.put(prefix + "name", layer.name if exists else "")
        self.put(prefix + "color", layer.color if exists else (0, 0, 0))
        self.put(prefix + "mute", exists and layer.mute)
        self.put(prefix + "solo", exists and layer.solo)
        self.put(prefix + "active", layer.activated if exists else True)
        self.put(prefix + "volume", layer.volume.value if exists else 0.0)
        self.put(prefix + "volume/str", layer.volume.displayed() if exists else "")
        self.put(prefix + "pan", layer.pan.value if exists else 0.0)
        self.put(prefix + "pan/str", layer.pan.displayed() if exists else "")

    def collect_device_layers(self) -> None:
        """``cursorDeviceLayers.collect``: the layers of the cursor device (``numDeviceLayers``),
        no pads, no pages."""
        self.collect_specific("/device", self.daw.cursor_device, 0, self.num_device_layers(), pages=False)

    def collect_drum(self) -> None:
        """The first Drum Machine of the cursor track (``drumDevice``, if the model setup does not
        say ``wantsDrumDevice`` false): 128 pads, no layers, no pages."""
        if not bool(self.banks.setup.get("wantsDrumDevice", True)):
            return
        self.collect_specific("/drum", self.daw.drum_device, NUM_DRUM_PADS, 0, pages=False)

    def collect_primary(self) -> None:
        """SpecificDevice.collect for ``/primary`` (the first instrument of the cursor track: its
        pages, 128 pads, 32 layers), if the controller asks for it (``deviceIds``)."""
        if "FIRST_INSTRUMENT" not in self.banks.setup.get("deviceIds", []):
            return
        self.collect_specific("/primary", self.daw.primary_device, NUM_DRUM_PADS, NUM_LAYERS, pages=True)

    # The cursor clip (BitwigNoteClip)

    def collect_clip(self) -> None:
        """BitwigNoteClip.collectClip: the properties of the cursor clip (a clip which does not
        exist: 0 / off, start and loop start unchanged, as Bitwig reported it, clip.py)."""
        daw = self.daw
        slot = daw.cursor_clip_slot
        clip = daw.cursor_clip
        exists = clip is not None
        self.put("/clip/exists", exists)
        if exists:
            self.put("/clip/start", float(clip.play_start))
        self.put("/clip/end", float(clip.play_stop) if exists else 0.0)
        self.put("/clip/loop", exists and clip.loop)
        if exists:
            self.put("/clip/loopStart", float(clip.loop_start))
        self.put("/clip/loopLength", float(clip.loop_length) if exists else 0.0)
        self.put("/clip/shuffle", exists and clip.shuffle)
        self.put("/clip/accent", clip.accent.value if exists else 0.0)
        self.put("/clip/pinned", daw.cursor.clip_pinned)
        self.put("/clip/color", slot.clip_color if exists else (0, 0, 0))

    def playing_step(self, grid) -> int:
        """The step of the play position of the cursor clip in the grid, -1 if it does not play
        (clip.py)."""
        slot = self.daw.cursor_clip_slot
        clip = slot.clip if slot is not None else None
        if clip is None or not slot.playing or grid.step_size <= 0:
            return -1
        position = clip.play_start + max(0.0, self.daw.transport.position - slot.play_start_beat)
        loop_end = clip.loop_start + clip.loop_length
        if clip.loop and clip.loop_length > 0 and position >= loop_end:
            position = clip.loop_start + (position - loop_end) % clip.loop_length
        return int(position // grid.step_size) - grid.scroll

    def collect_grids(self) -> None:
        """BitwigNoteClip.collectGrid: per grid the playing step and the steps which are not
        empty; a step which became empty is reported once as ``0``."""
        for grid in self.note_grids:
            prefix = f"/clip/grid/{grid.id}/"
            self.put(prefix + "playingStep", self.playing_step(grid))
            current = {f"{prefix}step/{channel}/{x}/{y}": line for (channel, x, y), line in grid.steps().items()}
            for address, line in current.items():
                self.put(address, line)
            reported = self._grid_steps.get(grid.id, set())
            for address in sorted(reported - set(current)):
                self.put(address, "0")
            self._grid_steps[grid.id] = set(current)

    def collect_eq(self) -> None:
        """EqualizerDevice.collect of the Bitwig extension: the first EQ+ of the cursor track in
        the layout of the Reaper EQ (``/eq/param/<3 * band + 0|1|2>/…``: frequency, gain, Q;
        ``/eq/band/<band>/value``: the type as the index of the Reaper EQ), if the controller
        asks for it (``deviceIds``)."""
        if "EQ" not in self.banks.setup.get("deviceIds", []):
            return
        device = self.daw.eq_device
        exists = device is not None
        self.put("/eq/exists", exists)
        self.put("/eq/name", device.name if exists else "")
        self.put("/eq/bypass", exists and not device.enabled)
        self.put("/eq/window", exists and device.window_open)
        self.put("/eq/expand", exists and device.expanded)
        for index in range(3 * EQ_NUM_BANDS):
            parameter = device.equalizer.parameter(index) if exists else None
            prefix = f"/eq/param/{index}/"
            self.put(prefix + "name", parameter.name if parameter is not None else "")
            self.put(prefix + "value", parameter.value if parameter is not None else 0)
            if parameter is not None:
                self.put_parameter_text(prefix + "value/str", parameter)
            else:
                self.put_displayed(prefix + "value/str", "", ["", ""])
            self.put(prefix + "steps", -1)
            self.put(prefix + "modulated", parameter.value if parameter is not None else 0)
        self.put("/eq/param/count", 3 * EQ_NUM_BANDS if exists else 0)
        for band in range(EQ_NUM_BANDS):
            self.put(f"/eq/band/{band}/value", device.equalizer.reaper_type(band) if exists else -1)


__all__ = ["LIMITS", "NUM_SIBLINGS", "PARAMS_PER_PAGE", "REPORT_AGAIN_DELAY_MS", "Reporter", "format_beats", "format_time", "format_value"]
