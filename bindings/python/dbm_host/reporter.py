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

import os
import struct
from collections.abc import Mapping
from typing import Callable

import drivenbymoss
from drivenbymoss import protocol
from drivenbymoss.protocol import ArrangerFlag, ChKind, Dev, MixerFlag, PadKind, ParamOwner

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


class M:
    """The IDs of protocol.Msg as plain ints: the keys of the reports (``M.CHANNEL_NAME |
    channel_key(ChKind.TRACK, 3)``). A plain int is a faster dict key than the IntEnum member."""


for _member in protocol.Msg:
    setattr(M, _member.name, int(_member))


def channel_key(kind: int, position: int) -> int:
    """The key base of a channel (the slots ch_kind, ch_pos): OR'ed with a channel, send or
    slot ID; a send or slot index goes into the third slot (``<< 48``)."""
    return kind << 16 | position << 32


def grid_key(grid) -> int:
    """The key base of a step grid (``clip.NoteGrid``: the slots grid_steps, grid_rows); a step
    adds ``(channel << 7 | row) << 48 | step << 64``."""
    return grid.num_steps << 16 | grid.num_rows << 32


# The flags of a slot (collect_slot): ID, attribute
SLOT_FLAGS = ((M.SLOT_SELECTED, "selected"), (M.SLOT_PLAYING, "playing"), (M.SLOT_RECORDING, "recording"),
              (M.SLOT_PLAY_QUEUED, "play_queued"), (M.SLOT_REC_QUEUED, "rec_queued"), (M.SLOT_STOP_QUEUED, "stop_queued"))

# The drum machine of collect_specific (/drum: its own IDs, no index), next to Dev.DEVICE and
# Dev.PRIMARY (the IDs of /device and /primary with the Dev index)
DRUM = -1
DEVICE_IDS = (M.DEVICE_EXISTS, M.DEVICE_NAME, M.DEVICE_BYPASS, M.DEVICE_WINDOW, M.DEVICE_EXPAND, M.DEVICE_PARAMETERS,
              M.DEVICE_HAS_DRUM_PADS, M.DEVICE_HAS_LAYERS, M.DEVICE_HAS_MUTED_PADS, M.DEVICE_HAS_SOLOED_PADS)
DRUM_IDS = (M.DRUM_EXISTS, M.DRUM_NAME, M.DRUM_BYPASS, M.DRUM_WINDOW, M.DRUM_EXPAND, M.DRUM_PARAMETERS,
            M.DRUM_HAS_DRUM_PADS, M.DRUM_HAS_LAYERS, M.DRUM_HAS_MUTED_PADS, M.DRUM_HAS_SOLOED_PADS)


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


# No last value (None is a value)
_NONE = object()

# The values put keeps as they are: they cannot change after the call (a tuple holds numbers);
# others are kept as their text
_IMMUTABLE = frozenset((bool, int, float, str, tuple, type(None)))

# The protocols of the reports (Reporter.protocol, HostBridge's DBM_HOST_PROTOCOL)
TEXT = "text"
BINARY = "binary"
CHECK = "check"
"""Binary, and every record is checked against the text line converted by the core."""
PROTOCOLS = (TEXT, BINARY, CHECK)


def default_protocol() -> str:
    """The protocol of the reports: the environment variable DBM_HOST_PROTOCOL (text, binary or
    check), binary by default."""
    value = os.environ.get("DBM_HOST_PROTOCOL", BINARY)
    return value if value in PROTOCOLS else BINARY


# The keys and addresses of the binary form, shared by all reporters (resolved by the core once)
_ADDRESS_KEYS: dict[str, int | None] = {}
_KEY_ADDRESSES: dict[int, str] = {}
UNKNOWN_ADDRESSES: set[str] = set()
"""The addresses which are not in the table (reported as text lines)."""
_DUMMY_VALUES = ("0", "", "0 0 0", "AUDIO")
_HEAD = struct.Struct("<HBB4HI")


def address_key(address: str) -> int | None:
    """The key (protocol.key: ID and indices) of a report address, None if it is not in the
    table. Resolved once by the core (dbm_messages_from_text)."""
    k = _ADDRESS_KEYS.get(address, _NONE)
    if k is _NONE:
        k = None
        for dummy in _DUMMY_VALUES:
            buffer = drivenbymoss.messages_from_text(f"{address} {dummy}")
            if len(buffer) > protocol.HEADER_SIZE:
                k = protocol.record_key(_HEAD.unpack_from(buffer, protocol.HEADER_SIZE))
                _KEY_ADDRESSES.setdefault(k, address)
                break
        _ADDRESS_KEYS[address] = k
        if k is None:
            UNKNOWN_ADDRESSES.add(address)
    return k


def key_address(k) -> str:
    """The address of a key (by the core's text form of a record); an address stays itself."""
    if type(k) is str:
        return k
    address = _KEY_ADDRESSES.get(k)
    if address is None:
        w = protocol.Writer(64)
        kind = protocol.KINDS[k & 0xFFFF]
        dummy = {"none": None, "bool": False, "i32": 0, "f64": 0.0, "text": "", "color": None, "notes": [], "step": None, "text_list": []}[kind]
        w.put_key(k, dummy)
        address = drivenbymoss.messages_to_text(w).split(" ", 1)[0]
        _KEY_ADDRESSES[k] = address
        _ADDRESS_KEYS.setdefault(address, k)
    return address


def _to_bool(value):
    # The flags of the text: a number above 0 (True is 1)
    return value > 0 if type(value) in (bool, int) else _NONE


def _to_i32(value):
    return int(value) if type(value) in (bool, int) and -0x80000000 <= value < 0x80000000 else _NONE


def _to_f64(value):
    # Rounded as %.6f: the record carries the double the text line gives (transition mode,
    # TYPED_PROTOCOL_PLAN §5.1)
    return round(float(value), 6) if type(value) in (float, int, bool) else _NONE


def _to_text(value):
    return value.replace("\n", " ").replace("\r", " ") if type(value) is str else _NONE


def _to_color(value):
    if type(value) is tuple and len(value) == 3:
        r, g, b = value
        if r < 0:
            return None
        if type(r) is int and type(g) is int and type(b) is int and 0 <= g <= 255 and 0 <= b <= 255 and r <= 255:
            return value
    return _NONE


_CHANNEL_TYPES = {member.name: int(member) for member in protocol.ChannelType}


def _to_channel_type(value):
    """The ChannelType names of the text as their numbers."""
    return _CHANNEL_TYPES.get(value, _NONE) if type(value) is str else _to_i32(value)


_NORMALIZERS = {"none": format_value, "bool": _to_bool, "i32": _to_i32, "f64": _to_f64, "text": _to_text, "color": _to_color}
"""The Python value of a report as the value of its record (by the value kind); _NONE: the value
is converted by the core from its text (notes, steps, unusual types). A message without a value
(e.g. /track/offset, the value is ignored) is reported when the text changes."""


def _converted(value):
    return _NONE


class _Spec:
    """What the reporter needs of an ID: the normalizer and the writer by key."""

    __slots__ = ("normalize", "write", "exact", "limit_shift")

    def __init__(self, id_: int):
        kind = protocol.KINDS[id_]
        # The values compared as numbers, not by their records (_changed)
        self.exact = kind == "f64"
        self.normalize = _to_channel_type if id_ == protocol.Msg.CHANNEL_TYPE else _NORMALIZERS.get(kind, _converted)
        self.write = protocol.KEY_PUTTERS.get(kind) if kind in _NORMALIZERS else None
        # The position of the slot "limit" of the shortened texts (put_displayed)
        self.limit_shift = 0
        for index, slot in enumerate(protocol.MSG_INFO[id_].slots):
            if slot[0][0] == "limit":
                self.limit_shift = 16 * (index + 1)


_SPECS: dict[int, _Spec] = {}


def _spec(id_: int) -> _Spec:
    spec = _SPECS.get(id_)
    if spec is None:
        spec = _SPECS[id_] = _Spec(id_)
    return spec


class _Converted(bytes):
    """A record converted by the core from a text line."""


class Reporter:
    """Collects the reports for the core: records for ``dbm_update_model_bin`` (binary, the
    default; TYPED_PROTOCOL_PLAN §2.4) or lines for ``dbm_update_model`` (``protocol`` text).

    Remembers the last value of each key and reports only changes, unless a dump was requested
    (the core's ``refresh``); an unchanged value costs one dict lookup and a compare. Starts with
    a dump. The model is collected by the ``collectors``, in order; the later steps append theirs.

    ``put`` takes a key (``protocol.key(Msg.CHANNEL_NAME, ChKind.TRACK, 3)``, the fast path) or
    an address (``"/track/3/name"``, resolved to its key by the core once). The binary form
    writes a record per change; the text form a line ``<address> <value>`` (the address of a key
    from the core, once). In the check protocol every record is compared with the core's
    conversion of the text line which the text protocol reports (the dual run of
    TYPED_PROTOCOL_PLAN §3).
    """

    def __init__(self, daw: SimDaw, banks: Banks, clock: Callable[[], float] = lambda: 0.0,
                 features: Callable[[], list[str]] = lambda: [], protocol_: str | None = None):
        """``clock`` returns the simulated time in ms (for ``forget_later``), ``features`` the
        host features of the command handler."""
        self.daw = daw
        self.banks = banks
        self.clock = clock
        self.features = features
        self.protocol = protocol_ or default_protocol()
        self.binary = self.protocol != TEXT
        self.check = self.protocol == CHECK
        self.last: dict[object, object] = {}
        """The last value of each key (an address only if it is not in the table), as put (the
        text of a value which is not immutable)."""
        self.lines: list[str] = []
        self.writer = protocol.Writer()
        self._spare = protocol.Writer()
        self.text_lines: list[str] = []
        """Lines which are not in the table (binary: sent as text)."""
        self.dump_all = True
        self.pending: dict[str, float] = {}
        self._last_selected: int = -1
        self._last_cursor: int = -1
        self.note_grids: list = []
        """The step grids of the cursor clip (``clip.NoteGrid``, from the model setup's
        ``noteClips``; set by ``CommandHandler.configure``)."""
        self._grid_steps: dict[int, set[int]] = {}
        self._converted: dict[tuple, bytes] = {}
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

    def forget(self, where) -> None:
        """Report again with the next collection: a key, or the values under an address prefix
        (by the addresses of the keys)."""
        if type(where) is int:
            self.last.pop(where, None)
            return
        for k in [k for k in self.last if key_address(k).startswith(where)]:
            del self.last[k]

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

    def put(self, k, value) -> None:
        """Report a value: ``k`` is a key (protocol.key) or an address."""
        last = self.last.get(k, _NONE)
        # The same value as last time (of the same type: True, 1 and 1.0 have other texts):
        # nothing to report. An address of the table is never in ``last`` (keys only)
        if last == value and type(last) is type(value) and not self.dump_all:
            return
        self._changed(k, value, last)

    def _changed(self, k, value, last) -> None:
        if type(value) not in _IMMUTABLE:
            value = format_value(value)
        if type(k) is str:
            resolved = address_key(k)
            if resolved is None:
                self._put_unknown(k, value)
                return
            k = resolved
            last = self.last.get(k, _NONE)
            if last == value and type(last) is type(value) and not self.dump_all:
                return
        if not self.binary:
            self._put_line(k, value, last)
            return
        spec = _SPECS.get(k & 0xFFFF) or _spec(k & 0xFFFF)
        v = spec.normalize(value)
        if v is _NONE:
            v = self._convert(k, format_value(value))
            if last is not _NONE and not self.dump_all and self._convert(k, format_value(last)) == v:
                self.last[k] = value
                return
        elif last is not _NONE and not self.dump_all and (last == value if spec.exact else spec.normalize(last) == v):
            # Another value of the same record (1 and True; for a double only the same number,
            # 1 and 1.0: a change of a double is reported even if the rounding of the
            # transition (_to_f64) gives the same record)
            self.last[k] = value
            return
        self.last[k] = value
        w = self.writer
        if self.check:
            start = w.size
            if type(v) is _Converted:
                w.buf[w._reserve(len(v)):w.size] = v
            else:
                spec.write(w, k, v)
            self._check(k, value, bytes(w.buf[start:w.size]))
        elif type(v) is _Converted:
            w.buf[w._reserve(len(v)):w.size] = v
        else:
            spec.write(w, k, v)

    def _put_line(self, k: int, value, last) -> None:
        """The text protocol: a line if the text changed."""
        text = format_value(value)
        self.last[k] = value
        if self.dump_all or last is _NONE or format_value(last) != text:
            self.lines.append(f"{key_address(k)} {text}")

    def _put_unknown(self, address: str, value) -> None:
        """An address which is not in the table: a text line in both protocols."""
        last = self.last.get(address, _NONE)
        self.last[address] = value
        text = format_value(value)
        if self.dump_all or last is _NONE or format_value(last) != text:
            (self.text_lines if self.binary else self.lines).append(f"{address} {text}")

    def _convert(self, k: int, text: str) -> bytes:
        """The record of a value by the core's conversion of its text line."""
        record = self._converted.get((k, text))
        if record is None:
            record = _Converted(drivenbymoss.messages_from_text(f"{key_address(k)} {text}")[protocol.HEADER_SIZE:])
            if len(self._converted) > 4096:
                self._converted.clear()
            self._converted[(k, text)] = record
        return record

    def _check(self, k: int, value, record: bytes) -> None:
        """The dual run: the record equals the core's conversion of the text line."""
        line = f"{key_address(k)} {format_value(value)}"
        expected = drivenbymoss.messages_from_text(line)[protocol.HEADER_SIZE:]
        if expected != record:
            raise AssertionError(f"The record of '{line}' differs from the text: {list(protocol.iter_records(protocol.HEADER + record))} != {list(protocol.iter_records(protocol.HEADER + expected))}")

    def text_value(self, k) -> str:
        """The text of the last value of a key or address."""
        return format_value(self.last[k])

    def put_displayed(self, k, text: str, limited: list[str] | None = None) -> None:
        """A displayed value and its shortened texts (``<address>/6``, ``/8``; by key the slot
        "limit")."""
        self.put(k, text)
        if type(k) is str:
            keys = [f"{k}/{limit}" for limit in LIMITS]
        else:
            shift = (_SPECS.get(k & 0xFFFF) or _spec(k & 0xFFFF)).limit_shift
            keys = [k | limit << shift for limit in LIMITS]
        if limited is None:
            limited = [text[:limit] for limit in LIMITS]
        self.put(keys[0], limited[0])
        self.put(keys[1], limited[1])

    def put_parameter_text(self, k, parameter: Parameter) -> None:
        self.put_displayed(k, parameter.displayed(), [parameter.displayed_limited(limit) for limit in LIMITS])

    def take(self) -> str:
        """The collected lines (empty if nothing changed); starts a new collection. In the binary
        form the text of the records (for tests and logs: the addresses by the keys, the values as
        the text protocol formats them)."""
        if self.binary:
            last = self.last
            lines = []
            if not self.writer.empty():
                for _p, head in protocol._records(self.writer.buf[:self.writer.size]):
                    k = protocol.record_key(head)
                    lines.append(f"{key_address(k)} {format_value(last[k])}")
            lines += self.text_lines
            self.take_buffer()
        else:
            lines = self.lines
            self.lines = []
            self.dump_all = False
        return "".join(line + "\n" for line in lines)

    def take_buffer(self) -> tuple[protocol.Writer | None, str]:
        """The collected records (None if nothing changed) and the lines which are not in the
        table; starts a new collection. The writer is valid until the next collection."""
        w = None if self.writer.empty() else self.writer
        text = "".join(line + "\n" for line in self.text_lines)
        if w is not None:
            self.writer, self._spare = self._spare, w
            self.writer.clear()
        self.text_lines = []
        self.dump_all = False
        return w, text

    def collect_pass(self) -> None:
        """Collect the model (the pending forgets first); the reports stay for take or
        take_buffer."""
        self.apply_pending_forgets()
        for collector in self.collectors:
            collector(self)

    def collect(self) -> str:
        """Collect the model and take the lines."""
        self.collect_pass()
        return self.take()

    def snapshot(self) -> dict[str, str]:
        """The last reported value of every address as text (for tests and the conformance state
        view)."""
        return {key_address(k): format_value(v) for k, v in self.last.items()}

    @property
    def last_values(self) -> "Mapping[str, str]":
        """The last reported value of every address as text (a live read-only view)."""
        return _TextView(self)

    # The model (BitwigModel.collect)

    def collect_host_features(self) -> None:
        self.put(M.HOST_FEATURES, " ".join(self.features()))

    def collect_transport(self) -> None:
        """BitwigModel.collectTransport (without the application and project values)."""
        t = self.daw.transport
        put = self.put
        qpm = max(1, int(4 * t.numerator / t.denominator))
        put(M.PLAY, t.playing)
        put(M.RECORD, t.recording)
        put(M.REPEAT, t.loop)
        put(M.TEMPO, float(t.tempo))
        put(M.NUMERATOR, t.numerator)
        put(M.DENOMINATOR, t.denominator)
        put(M.TIME, t.seconds())
        put(M.BEATS_POSITION, float(t.play_start))
        put(M.BEATS_LOOP_START, float(t.loop_start))
        put(M.BEATS_LOOP_LENGTH, float(t.loop_length))
        put(M.TIME_TEXT, format_time(t.tempo, t.position) if t.tempo > 0 else "")
        put(M.BEATS_TEXT, format_beats(t.position, qpm, 1))
        put(M.BEATS_LOOP_START_TEXT, format_beats(t.loop_start, qpm, 1))
        put(M.BEATS_LOOP_LENGTH_TEXT, format_beats(t.loop_length, qpm, 0))
        put(M.METRONOME, t.metronome)
        put(M.OVERDUB_ARRANGER, t.overdub_arranger)
        put(M.OVERDUB_LAUNCHER, t.overdub_launcher)
        put(M.AUTOMATION_ARRANGER, t.automation_arranger)
        put(M.AUTOMATION_LAUNCHER, t.automation_launcher)
        modes = {"touch": 2, "write": 3, "latch": 4}
        put(M.AUTOMATION_MODE, modes.get(t.automation_mode, 1) if t.automation_arranger or t.automation_launcher else 1)
        put(M.METRONOME_PREROLL, 2 if t.preroll_metronome else 0)
        put(M.METRONOME_PREROLL_MEASURES, max(0, PREROLLS.index(t.preroll)) if t.preroll in PREROLLS else 0)
        put(M.METRONOME_VOLUME, t.metronome_volume.value)
        # As TransportImpl.getMetronomeVolumeStr: the text limited to 6 and the unit
        put(M.METRONOME_VOLUME_STR, t.metronome_volume.displayed_limited(6) + " dB")
        put(M.METRONOME_TICKS, t.metronome_ticks)
        put(M.PUNCH_IN, t.punch_in)
        put(M.PUNCH_OUT, t.punch_out)
        put(M.FILL, t.fill)
        put(M.PROJECT_CAN_UNDO, self.daw.application.can_undo)
        put(M.PROJECT_CAN_REDO, self.daw.application.can_redo)
        self.collect_application()

    def collect_application(self) -> None:
        """BitwigModel.collectApplication (the simulated part): the panel layout, the visibility
        flags of the arranger and the mixer."""
        daw = self.daw
        self.put(M.LAYOUT, daw.application.panel_layout)
        self.put(M.FOLLOW_PLAYBACK, daw.arranger.playback_follow)
        arranger, mixer = daw.arranger, daw.mixer
        for flag, value in ((ArrangerFlag.CUE_MARKERS, arranger.cue_markers), (ArrangerFlag.DOUBLE_ROW_TRACK_HEIGHT, arranger.double_row_track_height),
                            (ArrangerFlag.CLIP_LAUNCHER, arranger.clip_launcher), (ArrangerFlag.TIMELINE, arranger.timeline),
                            (ArrangerFlag.IO_SECTION, arranger.io_section), (ArrangerFlag.EFFECT_TRACKS, arranger.effect_tracks)):
            self.put(M.ARRANGER | flag << 16, value)
        for flag, value in ((MixerFlag.CLIP_LAUNCHER_SECTION, mixer.clip_launcher_section), (MixerFlag.CROSS_FADE_SECTION, mixer.cross_fade_section),
                            (MixerFlag.DEVICE_SECTION, mixer.device_section), (MixerFlag.IO_SECTION, mixer.io_section),
                            (MixerFlag.METER_SECTION, mixer.meter_section), (MixerFlag.SEND_SECTION, mixer.send_section)):
            self.put(M.MIXER | flag << 16, value)

    def track_list(self) -> list[Channel]:
        return self.daw.track_list(self.banks.full_flat_track_list)

    def collect_channel(self, base: int, channel: Channel) -> None:
        """BitwigModel.collectChannel (with the shortened texts); ``base``: channel_key."""
        put = self.put
        put(M.CHANNEL_NAME | base, channel.name)
        put(M.CHANNEL_COLOR | base, channel.color)
        put(M.CHANNEL_MUTE | base, channel.mute)
        put(M.CHANNEL_SOLO | base, channel.solo)
        put(M.CHANNEL_ACTIVE | base, channel.activated)
        put(M.CHANNEL_VOLUME | base, channel.volume.value)
        self.put_parameter_text(M.CHANNEL_VOLUME_STR | base, channel.volume)
        put(M.CHANNEL_PAN | base, channel.pan.value)
        self.put_parameter_text(M.CHANNEL_PAN_STR | base, channel.pan)

    def collect_vu(self, base: int, channel: Channel) -> None:
        self.put(M.CHANNEL_VU_LEFT | base, float(channel.vu_left))
        self.put(M.CHANNEL_VU_RIGHT | base, float(channel.vu_right))
        self.put(M.CHANNEL_VU | base, (channel.vu_left + channel.vu_right) / 2.0)

    def collect_sends(self, base: int, channel: Channel, with_color: bool) -> None:
        count = min(len(channel.sends), self.banks.num_sends)
        put = self.put
        for index, send in enumerate(channel.sends[:count]):
            send_base = base | index << 48
            put(M.SEND_NAME | send_base, send.name)
            put(M.SEND_VOLUME | send_base, send.volume.value)
            self.put_parameter_text(M.SEND_VOLUME_STR | send_base, send.volume)
            put(M.SEND_ACTIVE | send_base, send.enabled)
            if with_color:
                put(M.SEND_COLOR | send_base, send.color)
        put(M.SEND_COUNT | base, count)

    def collect_track_flags(self, base: int, track) -> None:
        put = self.put
        put(M.CHANNEL_REC_ARM | base, track.arm)
        put(M.CHANNEL_MONITOR | base, track.monitoring)
        put(M.CHANNEL_AUTO_MONITOR | base, track.monitor_mode == "AUTO")
        put(M.CHANNEL_CAN_HOLD_NOTES | base, track.can_hold_notes)
        put(M.CHANNEL_CAN_HOLD_AUDIO | base, track.can_hold_audio)

    @staticmethod
    def track_type(track) -> str:
        return "GROUP" if track.is_group else track.type

    def collect_tracks(self) -> None:
        """BitwigModel.collectTracks and collectSlots, for all tracks."""
        tracks = self.track_list()
        daw = self.daw
        put = self.put
        selected = tracks.index(daw.selected_channel) if daw.selected_channel in tracks else -1
        cursor = tracks.index(daw.cursor_channel) if daw.cursor_channel in tracks else -1
        put(M.TRACK_PINNED, cursor if daw.cursor.pinned and cursor >= 0 else -1)
        put(M.TRACK_HAS_PREVIOUS, cursor > 0)
        put(M.TRACK_HAS_NEXT, 0 <= cursor < len(tracks) - 1)
        put(M.TRACK_OFFSET, self.banks.track_offset)
        # A deselection outside of the list (e.g. the track was removed)
        if self._last_selected != selected and self._last_selected >= len(tracks):
            put(M.CHANNEL_SELECTED | channel_key(ChKind.TRACK, self._last_selected), False)
        self._last_selected = selected

        num_scenes = len(daw.scenes)
        for position, track in enumerate(tracks):
            base = channel_key(ChKind.TRACK, position)
            put(M.CHANNEL_EXISTS | base, 1)
            put(M.CHANNEL_NUMBER | base, position)
            put(M.CHANNEL_TYPE | base, self.track_type(track))
            put(M.CHANNEL_IS_GROUP_EXPANDED | base, track.group_expanded)
            if position != selected:
                put(M.CHANNEL_SELECTED | base, False)
            self.collect_track_flags(base, track)
            self.collect_vu(base, track)
            self.collect_channel(base, track)
            self.collect_sends(base, track, with_color=True)
            put(M.CHANNEL_IS_STOPPED | base, track.is_stopped)
            put(M.SLOT_COUNT | base, num_scenes)
            for index in range(num_scenes):
                self.collect_slot(base | index << 48, track.slots[index] if index < len(track.slots) else None)
        put(M.TRACK_COUNT, len(tracks))
        if selected >= 0:
            put(M.CHANNEL_SELECTED | channel_key(ChKind.TRACK, selected), True)
        # The notes which play on the cursor track, in the note format of the core; the core
        # keeps one note cache: the notes of a new cursor track are reported again
        notes_key = M.CHANNEL_PLAYING_NOTES | channel_key(ChKind.TRACK, cursor)
        if cursor != self._last_cursor:
            self._last_cursor = cursor
            self.forget(notes_key)
        if cursor >= 0:
            notes = getattr(tracks[cursor], "playing_notes", {})
            put(notes_key, ";".join(f"0:0:0:0:0:{key}:{velocity}" for key, velocity in notes.items()))

    def collect_slot(self, base: int, slot) -> None:
        """A slot; ``base``: the channel key with the slot index."""
        put = self.put
        put(M.SLOT_EXISTS | base, slot is not None)
        put(M.SLOT_HAS_CONTENT | base, slot is not None and slot.has_content)
        put(M.SLOT_NAME | base, "" if slot is None else slot.name)
        put(M.SLOT_COLOR | base, (0, 0, 0) if slot is None else slot.color)
        for id_, attribute in SLOT_FLAGS:
            put(id_ | base, slot is not None and getattr(slot, attribute))

    def collect_effect_tracks(self) -> None:
        """BitwigModel.collectEffectTracks, for all effect tracks."""
        put = self.put
        put(M.EFFECT_TRACK_OFFSET, self.banks.fx_track_offset)
        for position, track in enumerate(self.daw.effect_tracks):
            base = channel_key(ChKind.EFFECT, position)
            put(M.CHANNEL_EXISTS | base, 1)
            put(M.CHANNEL_NUMBER | base, position)
            put(M.CHANNEL_TYPE | base, self.track_type(track))
            put(M.CHANNEL_SELECTED | base, track.selected)
            self.collect_track_flags(base, track)
            self.collect_channel(base, track)
            self.collect_sends(base, track, with_color=False)
        put(M.EFFECT_TRACK_COUNT, len(self.daw.effect_tracks))

    def collect_markers(self) -> None:
        """BitwigModel.collectMarkers (if the model setup has ``numMarkers`` > 0): the markers of
        the bank of 100 with their absolute positions, the position in beats."""
        if int(self.banks.setup.get("numMarkers", 0)) <= 0:
            return
        markers = self.daw.markers.items
        put = self.put
        for index in range(NUM_MARKERS):
            marker = markers[index] if index < len(markers) else None
            base = index << 16
            put(M.MARKER_EXISTS | base, marker is not None)
            if marker is None:
                continue
            put(M.MARKER_NUMBER | base, index)
            put(M.MARKER_NAME | base, marker.name)
            put(M.MARKER_COLOR | base, marker.color)
            put(M.MARKER_POSITION | base, float(marker.position))
        put(M.MARKER_COUNT, min(len(markers), NUM_MARKERS))

    def collect_scenes(self) -> None:
        """BitwigModel.collectScenes, for all scenes."""
        put = self.put
        put(M.SCENE_OFFSET, self.banks.scene_offset)
        for position, scene in enumerate(self.daw.scenes):
            base = position << 16
            put(M.SCENE_EXISTS | base, True)
            put(M.SCENE_NUMBER | base, position)
            put(M.SCENE_NAME | base, scene.name)
            put(M.SCENE_COLOR | base, scene.color)
            put(M.SCENE_SELECTED | base, scene.selected)
        put(M.SCENE_COUNT, len(self.daw.scenes))

    def collect_master(self) -> None:
        master = self.daw.master
        base = channel_key(ChKind.MASTER, 0)
        self.put(M.CHANNEL_SELECTED | base, master.selected)
        self.collect_vu(base, master)
        self.collect_channel(base, master)

    def collect_note_repeat(self) -> None:
        """BitwigModel.collectNoteRepeat: the arpeggiator of the default note input (nothing
        without one); ``period`` is the inverse of the rate in beats, ``mode`` the index of the
        core's modes."""
        note_input = self.daw.note_input
        if note_input is None:
            return
        arpeggiator = note_input.arpeggiator
        self.put(M.NOTE_REPEAT_ACTIVE, arpeggiator.enabled)
        if arpeggiator.rate > 0:
            self.put(M.NOTE_REPEAT_PERIOD, 1.0 / arpeggiator.rate)
        self.put(M.NOTE_REPEAT_NOTE_LENGTH, float(arpeggiator.gate_length))
        self.put(M.NOTE_REPEAT_MODE, arpeggiator.mode_index)
        self.put(M.NOTE_REPEAT_VELOCITY, arpeggiator.use_pressure_to_velocity)

    # Devices (BitwigModel.collectDevice, SpecificDevice.collect)

    def num_param_pages(self) -> int:
        return max(0, int(self.banks.setup.get("numParamPages", 8)))

    def collect_pages(self, owner: int, device, cursor: bool = False) -> None:
        """BitwigModel.collectPages and the page names: the parameters of the first
        ``numParamPages`` pages as one flat list, the page names, the selected page. The
        simulated parameters have no names and texts (device.py); the value of a parameter
        which a command set is reported. ``owner``: the ParamOwner (= PageOwner); ``cursor``:
        the pages as the cursor device shows them."""
        num_pages = self.num_param_pages()
        pages = device.page_names(cursor) if device is not None else []
        used = min(len(pages), num_pages)
        put = self.put
        owner_base = owner << 16
        for index in range(num_pages * PARAMS_PER_PAGE):
            base = owner_base | index << 32
            in_use = device is not None and index < used * PARAMS_PER_PAGE
            control = device.remote_control(index) if in_use and device.remote_controls else None
            if control is not None:
                parameter = control.parameter
                put(M.PARAM_NAME | base, control.name)
                put(M.PARAM_VALUE | base, parameter.value)
                self.put_parameter_text(M.PARAM_VALUE_STR | base, parameter)
            else:
                parameter = device.parameters.get(index) if in_use and not device.remote_controls else None
                put(M.PARAM_NAME | base, "")
                put(M.PARAM_VALUE | base, parameter.value if parameter is not None else 0)
                self.put_displayed(M.PARAM_VALUE_STR | base, "", ["", ""])
            put(M.PARAM_STEPS | base, -1)
            put(M.PARAM_MODULATED | base, parameter.value if parameter is not None else 0)
        put(M.PARAM_COUNT | owner_base, used * PARAMS_PER_PAGE)
        for index in range(used):
            put(M.PAGE_NAME | owner_base | index << 32, pages[index])
        put(M.PAGE_COUNT | owner_base, used)
        put(M.PAGE_SELECTED | owner_base, device.selected_page if device is not None else -1)

    def collect_device(self) -> None:
        """BitwigModel.collectDevice: the cursor device of the cursor track, its siblings (the
        window at ``DeviceChain.sibling_offset``) and pages."""
        chain = self.daw.cursor_device_chain
        devices = chain.devices if chain is not None else []
        device = self.daw.cursor_device
        exists = device is not None
        position = devices.index(device) if exists else -1
        put = self.put
        put(M.DEVICE_COUNT, len(devices))
        put(M.DEVICE_EXISTS, exists)
        put(M.DEVICE_POSITION, position)
        put(M.DEVICE_NAME, device.name if exists else "")
        put(M.DEVICE_BYPASS, exists and not device.enabled)
        put(M.DEVICE_WINDOW, exists and device.window_open)
        put(M.DEVICE_PINNED, self.daw.cursor.device_pinned)
        put(M.DEVICE_EXPAND, exists and device.expanded)
        offset = chain.sibling_offset if chain is not None else 0
        for index in range(NUM_SIBLINGS):
            sibling_position = offset + index
            sibling = devices[sibling_position] if sibling_position < len(devices) else None
            # /device/sibling/<index + 1>/: the slot holds the index
            base = index << 32
            put(M.DEVICE_SIBLING_NAME | base, sibling.name if sibling is not None else "")
            put(M.DEVICE_SIBLING_BYPASS | base, sibling is not None and not sibling.enabled)
            put(M.DEVICE_SIBLING_POSITION | base, sibling_position if sibling is not None else -1)
            put(M.DEVICE_SIBLING_SELECTED | base, sibling is not None and sibling_position == position)
        self.collect_pages(ParamOwner.DEVICE, device, cursor=True)

    def num_device_layers(self) -> int:
        return max(0, int(self.banks.setup.get("numDeviceLayers", 8)))

    def collect_specific(self, dev: int, device, num_pads: int, num_layers: int, pages: bool, cursor: bool = False) -> None:
        """SpecificDevice.collect of the Bitwig extension: the device, its pages (``pages``),
        ``num_pads`` drum pads and ``num_layers`` layers (as the bank sizes of the Bitwig
        extension, all items reported). ``dev``: Dev.DEVICE, Dev.PRIMARY or DRUM (the drum
        machine, its own IDs)."""
        exists = device is not None
        put = self.put
        ids = DRUM_IDS if dev == DRUM else DEVICE_IDS
        base = 0 if dev == DRUM else dev << 16
        put(ids[0] | base, exists)
        put(ids[1] | base, device.name if exists else "")
        put(ids[2] | base, exists and not device.enabled)
        put(ids[3] | base, exists and device.window_open)
        put(ids[4] | base, exists and device.expanded)
        put(ids[5] | base, exists and device.parameters_visible)
        put(ids[6] | base, exists and device.has_drum_pads)
        put(ids[7] | base, exists and device.has_layers)
        if pages:
            self.collect_pages(dev, device, cursor)
        pad_dev = Dev.DEVICE if dev == DRUM else dev
        if num_pads > 0:
            put(ids[8] | base, exists and device.has_muted_pads)
            put(ids[9] | base, exists and device.has_soloed_pads)
            pads = device.drum_pads if exists and device.drum_pads is not None else []
            pad_base = PadKind.PAD << 16 | pad_dev << 32 if dev != DRUM else PadKind.DRUM_PAD << 16
            for index in range(num_pads):
                self.collect_layer(pad_base | index << 48, pads[index] if index < len(pads) else None)
        if num_layers <= 0:
            return
        layers = device.layer_list() if exists else []
        put(M.DEVICE_LAYER_COUNT | base, min(len(layers), num_layers))
        layer_base = PadKind.LAYER << 16 | pad_dev << 32
        for index in range(num_layers):
            self.collect_layer(layer_base | index << 48, layers[index] if index < len(layers) else None)

    def collect_layer(self, base: int, layer) -> None:
        """SpecificDevice.collectItem: an empty item has the values of an empty Bitwig channel
        (layer.py). ``base``: the pad key (PadKind, Dev, index)."""
        exists = layer is not None and layer.exists
        put = self.put
        put(M.PAD_EXISTS | base, exists)
        put(M.PAD_SELECTED | base, exists and layer.selected)
        put(M.PAD_NAME | base, layer.name if exists else "")
        put(M.PAD_COLOR | base, layer.color if exists else (0, 0, 0))
        put(M.PAD_MUTE | base, exists and layer.mute)
        put(M.PAD_SOLO | base, exists and layer.solo)
        put(M.PAD_ACTIVE | base, layer.activated if exists else True)
        put(M.PAD_VOLUME | base, layer.volume.value if exists else 0.0)
        put(M.PAD_VOLUME_STR | base, layer.volume.displayed() if exists else "")
        put(M.PAD_PAN | base, layer.pan.value if exists else 0.0)
        put(M.PAD_PAN_STR | base, layer.pan.displayed() if exists else "")

    def collect_device_layers(self) -> None:
        """``cursorDeviceLayers.collect``: the layers of the cursor device (``numDeviceLayers``),
        no pads, no pages."""
        self.collect_specific(Dev.DEVICE, self.daw.cursor_device, 0, self.num_device_layers(), pages=False)

    def collect_drum(self) -> None:
        """The first Drum Machine of the cursor track (``drumDevice``, if the model setup does not
        say ``wantsDrumDevice`` false): 128 pads, no layers, no pages."""
        if not bool(self.banks.setup.get("wantsDrumDevice", True)):
            return
        self.collect_specific(DRUM, self.daw.drum_device, NUM_DRUM_PADS, 0, pages=False)

    def collect_primary(self) -> None:
        """SpecificDevice.collect for ``/primary`` (the first instrument of the cursor track: its
        pages, 128 pads, 32 layers), if the controller asks for it (``deviceIds``)."""
        if "FIRST_INSTRUMENT" not in self.banks.setup.get("deviceIds", []):
            return
        self.collect_specific(Dev.PRIMARY, self.daw.primary_device, NUM_DRUM_PADS, NUM_LAYERS, pages=True)

    # The cursor clip (BitwigNoteClip)

    def collect_clip(self) -> None:
        """BitwigNoteClip.collectClip: the properties of the cursor clip (a clip which does not
        exist: 0 / off, start and loop start unchanged, as Bitwig reported it, clip.py)."""
        daw = self.daw
        slot = daw.cursor_clip_slot
        clip = daw.cursor_clip
        exists = clip is not None
        put = self.put
        put(M.CLIP_EXISTS, exists)
        if exists:
            put(M.CLIP_START, float(clip.play_start))
        put(M.CLIP_END, float(clip.play_stop) if exists else 0.0)
        put(M.CLIP_LOOP, exists and clip.loop)
        if exists:
            put(M.CLIP_LOOP_START, float(clip.loop_start))
        put(M.CLIP_LOOP_LENGTH, float(clip.loop_length) if exists else 0.0)
        put(M.CLIP_SHUFFLE, exists and clip.shuffle)
        put(M.CLIP_ACCENT, clip.accent.value if exists else 0.0)
        put(M.CLIP_PINNED, daw.cursor.clip_pinned)
        put(M.CLIP_COLOR, slot.clip_color if exists else (0, 0, 0))

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
            base = grid_key(grid)
            self.put(M.GRID_PLAYING_STEP | base, self.playing_step(grid))
            current = {M.GRID_STEP | base | (channel << 7 | y) << 48 | x << 64: line for (channel, x, y), line in grid.steps().items()}
            for k, line in current.items():
                self.put(k, line)
            reported = self._grid_steps.get(base, set())
            for k in sorted(reported - set(current)):
                self.put(k, "0")
            self._grid_steps[base] = set(current)

    def collect_eq(self) -> None:
        """EqualizerDevice.collect of the Bitwig extension: the first EQ+ of the cursor track in
        the layout of the Reaper EQ (``/eq/param/<3 * band + 0|1|2>/…``: frequency, gain, Q;
        ``/eq/band/<band>/value``: the type as the index of the Reaper EQ), if the controller
        asks for it (``deviceIds``)."""
        if "EQ" not in self.banks.setup.get("deviceIds", []):
            return
        device = self.daw.eq_device
        exists = device is not None
        put = self.put
        eq = Dev.EQ << 16
        put(M.DEVICE_EXISTS | eq, exists)
        put(M.DEVICE_NAME | eq, device.name if exists else "")
        put(M.DEVICE_BYPASS | eq, exists and not device.enabled)
        put(M.DEVICE_WINDOW | eq, exists and device.window_open)
        put(M.DEVICE_EXPAND | eq, exists and device.expanded)
        for index in range(3 * EQ_NUM_BANDS):
            parameter = device.equalizer.parameter(index) if exists else None
            base = eq | index << 32
            put(M.PARAM_NAME | base, parameter.name if parameter is not None else "")
            put(M.PARAM_VALUE | base, parameter.value if parameter is not None else 0)
            if parameter is not None:
                self.put_parameter_text(M.PARAM_VALUE_STR | base, parameter)
            else:
                self.put_displayed(M.PARAM_VALUE_STR | base, "", ["", ""])
            put(M.PARAM_STEPS | base, -1)
            put(M.PARAM_MODULATED | base, parameter.value if parameter is not None else 0)
        put(M.PARAM_COUNT | eq, 3 * EQ_NUM_BANDS if exists else 0)
        for band in range(EQ_NUM_BANDS):
            put(M.EQ_BAND_TYPE | band << 16, device.equalizer.reaper_type(band) if exists else -1)


class _TextView(Mapping):
    """The last values of a reporter by address, as text."""

    def __init__(self, reporter: Reporter):
        self.reporter = reporter

    def __getitem__(self, address: str) -> str:
        last = self.reporter.last
        k = address_key(address)
        if k is None:
            k = address
        if k not in last:
            raise KeyError(address)
        return format_value(last[k])

    def __iter__(self):
        return iter([key_address(k) for k in self.reporter.last])

    def __len__(self) -> int:
        return len(self.reporter.last)


__all__ = ["BINARY", "CHECK", "TEXT", "M", "address_key", "channel_key", "grid_key", "default_protocol", "key_address", "LIMITS", "NUM_SIBLINGS", "PARAMS_PER_PAGE", "REPORT_AGAIN_DELAY_MS", "Reporter", "format_beats", "format_time", "format_value"]
