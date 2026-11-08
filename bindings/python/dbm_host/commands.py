# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Executes the commands of the core on the simulated DAW, in the binary form of the protocol
(TYPED_PROTOCOL_PLAN §2.7): one buffer of records per call, dispatched by ``CmdId``.

Implements HOST_BRIDGE_GUIDE.md §4 (the command reference, core -> host): 4.1 the value
operations, 4.2 transport, ``noterepeat``, ``application layout``, ``arranger``, ``mixer``,
``project createScene``, the ``refresh`` and ``bank`` commands of 4.3, 4.4 tracks and master
(and the effect tracks of §3.3), 4.5 slots and scenes, 4.6 the devices (``device``, ``primary``,
``drum``: the cursor device, its siblings and layers, the pads and layers of the specific
devices, ``param/<n>/…``, ``eq``), 4.7 the cursor clip and its step grids (``clip``,
``quantize``), 4.8 the markers; ``navigator`` (§2, the host feature clipLauncherNavigator,
``navigator.py``); the processors as BACKEND_PROTOCOL.md §2.

Simulates the command handling of the Bitwig extension (``CommandHandler``, ``DbmExtension.
unhandled``), which calls the Bitwig API as the Java DrivenByMoss for Bitwig does
(``TransportImpl``, ``ChannelImpl``, ``TrackImpl``, ``SendImpl``, ``SlotImpl``, ``SceneImpl``,
``CursorTrackImpl``, ``AbstractTrackBankImpl``, ``NoteRepeatImpl``, ``CursorDeviceImpl``,
``SpecificDeviceImpl``, ``DrumPadImpl``, ``LayerImpl``, ``EqualizerDeviceImpl``,
``CursorClipImpl``, ``MarkerImpl``, ``ApplicationImpl``).

Dispatch: one typed handler per ``CmdId`` (``CommandHandler.register_typed``), which gets the
view of the record and a ``ViewCommand`` and returns False if it does not know the command; the
command is then logged once as ``Not handled yet: <processor> <command>`` (the core's text of the
record, digits replaced by ``<n>``, as the Bitwig extension) and kept in ``unhandled``. The IDs
with a handler are the handled commands passed to the core (``handled_commands``): the core
derives the command features of the host from them (TYPED_PROTOCOL_PLAN §6.6); the value
operation features (``OP_FEATURES``) are reported as ``/host/features``. The channel commands
call the hooks ``channel_command``, ``track_command``, ``send_command``, ``value_command`` and
``set_value`` with the sub commands of the channel (``CHANNEL_SUBS``), which a host overrides
(``ZynCommandHandler``).

The buffer of the core is only valid during the call: the views must not escape it. The log
entries (``ViewCommand``) copy their record.
"""

import logging
import re
from dataclasses import dataclass
from typing import Any, Callable, Iterable

import drivenbymoss
from drivenbymoss import protocol
from drivenbymoss.protocol import ChanKind, ChKind, Cmd, Dev, PadKind, ParamOwner, Processor

from .banks import Banks
from .clip import ATTRIBUTES, NoteGrid, set_attribute, update_from_wire
from .device import EQ_NUM_BANDS, PARAMS_PER_PAGE, eq_plus
from .model import ARRANGER_TOGGLES, MIXER_TOGGLES, PANEL_LAYOUTS, PREROLLS, Channel, SimDaw, Track, color_from_command
from .navigator import ClipLauncherNavigator
from .parameter import Parameter
from .reporter import NUM_DRUM_PADS, NUM_LAYERS, NUM_MARKERS, NUM_SIBLINGS, Reporter

logger = logging.getLogger("dbm_host")

# The order of HostFeature::ALL in src/daw/context.rs
FEATURE_ORDER = ("valueInc", "valueReset", "immediately", "indication", "soloToggle", "transportCommands",
                 "deviceCommands", "layoutStep", "clipLauncherNavigator")

# The commands of a device on the device itself (CommandHandler.handleOwnDevice)
OWN_DEVICE_COMMANDS = ("window/toggle", "expand/toggle", "parameters/toggle", "bypass", "remove", "duplicate", "select")
DEVICE_ACTIONS = {protocol.DeviceAction.BYPASS: "bypass", protocol.DeviceAction.REMOVE: "remove",
                  protocol.DeviceAction.DUPLICATE: "duplicate", protocol.DeviceAction.SELECT: "select"}
DEVICE_TOGGLES = {Cmd.DEVICE_TOGGLE_WINDOW: "window/toggle", Cmd.DEVICE_TOGGLE_EXPANDED: "expand/toggle",
                  Cmd.DEVICE_TOGGLE_PARAMETER_PAGE_SECTION: "parameters/toggle"}

# The suffixes of the value operations in the text of a command (the log)
OP_SUFFIXES = {protocol.OP_INC: "/inc", protocol.OP_RESET: "/reset", protocol.OP_IMMEDIATELY: "/immediately"}

# The update hints of the core (delay, enable): a host which reports changes only ignores them
# (not logged)
IGNORED_COMMANDS = frozenset((int(Cmd.DELAY_UPDATES), int(Cmd.ENABLE_UPDATES)))


@dataclass
class Command:
    """A command of the core as the log shows it (the core's text of the record)."""

    processor: str
    full_command: str
    """The command as the core's text has it."""
    command: str
    """The command without ``/immediately``, ``/inc``, ``/reset``."""
    value: Any
    """None, a str, a list of str, an int, a float, a tuple (colors, pairs)."""
    immediately: bool = False
    increment: bool = False
    reset: bool = False

    @property
    def number(self) -> float:
        """The value as a number; a command without a value counts as 1 (as the Bitwig
        extension)."""
        value = self.value
        if value is None:
            return 1.0
        if type(value) in (int, float):
            return float(value)
        try:
            return float(str(value).strip())
        except ValueError:
            return 0.0

    @property
    def flag(self) -> bool:
        return self.number > 0

    @property
    def text(self) -> str | None:
        return self.value if isinstance(self.value, str) else None

    @property
    def color(self) -> tuple[int, int, int] | None:
        """The value of a color command (0..255 each), None if it is none."""
        value = self.value
        if type(value) is tuple and len(value) == 3:
            return tuple(max(0, min(255, int(part))) for part in value)
        return None


def parse_index(text) -> int:
    try:
        return int(text)
    except (TypeError, ValueError):
        return -1


class ViewCommand(Command):
    """A command of the binary form: the operation (``reset``, ``increment``, ``immediately``,
    ``op``) and the value (a flag as 1/0) read from the record during the call; the record is
    copied (the buffer of the core is only valid during the call), so the command can be kept in
    the log. ``processor``, ``full_command`` and ``command`` are the core's text of the record,
    converted only when they are read (logs, the not handled list)."""

    def __init__(self, view):  # noqa: D107 - no dataclass __init__: the fields are properties
        n = view.h[7]
        p = view.p
        self.record = bytes(view.b[p:p + 24 + ((n + 7) & ~7)])
        self.id = view.h[0]
        op = self.op = view.h[2]
        self.immediately = op == protocol.OP_IMMEDIATELY
        self.increment = op == protocol.OP_INC
        self.reset = op == protocol.OP_RESET
        value = view.value
        self.value = int(value) if type(value) is bool else value
        self._legacy = None

    @property
    def legacy(self) -> tuple:
        """(kind, processor, command, value) of the core's text of the record."""
        if self._legacy is None:
            lines = drivenbymoss.legacy_commands(drivenbymoss.messages_to_text(protocol.HEADER + self.record))
            self._legacy = lines[0] if lines else ("", "", "", None)
        return self._legacy

    @property
    def processor(self) -> str:
        return self.legacy[1]

    @property
    def full_command(self) -> str:
        return self.legacy[2]

    @property
    def command(self) -> str:
        full = self.full_command
        suffix = OP_SUFFIXES.get(self.op)
        return full[:-len(suffix)] if suffix is not None and full.endswith(suffix) else full

    def __repr__(self) -> str:
        return f"ViewCommand({self.processor!r}, {self.full_command!r}, {self.value!r})"


TypedHandler = Callable[[Any, ViewCommand], bool]

# The sub commands of the channel commands (the parts after the channel in the text form; None:
# the send or slot index, filled in by channel_sub)
CHANNEL_SUBS = {
    Cmd.CHANNEL_COLOR: ["color"], Cmd.CHANNEL_MUTE: ["mute"], Cmd.CHANNEL_SOLO: ["solo"],
    Cmd.CHANNEL_SOLO_TOGGLE: ["solo", "toggle"], Cmd.CHANNEL_ACTIVE: ["active"], Cmd.CHANNEL_REMOVE: ["remove"],
    Cmd.CHANNEL_DUPLICATE: ["duplicate"], Cmd.CHANNEL_SELECT: ["select"], Cmd.CHANNEL_SELECT_IN_EDITOR: ["select"],
    Cmd.CHANNEL_VOLUME: ["volume"], Cmd.CHANNEL_PAN: ["pan"], Cmd.CHANNEL_SEND_VOLUME: ["send", None, "volume"],
    Cmd.CHANNEL_SEND_ACTIVE: ["send", None, "active"], Cmd.TRACK_TOGGLE_MULTI_SELECT: ["toggleMultiSelect"],
    Cmd.TRACK_NAME: ["name"], Cmd.TRACK_GROUP_EXPANDED: ["isGroupExpanded"], Cmd.TRACK_REC_ARM: ["recarm"],
    Cmd.TRACK_MONITOR: ["monitor"], Cmd.TRACK_AUTO_MONITOR: ["autoMonitor"], Cmd.TRACK_STOP: ["stop"],
    Cmd.TRACK_STOP_ALT: ["stopAlt"], Cmd.TRACK_RETURN_TO_ARRANGEMENT: ["returnToArrangement"],
    Cmd.TRACK_IN_QUANT_RESOLUTION: ["inQuantResolution"], Cmd.TRACK_CREATE_CLIP: ["createClip", None],
    Cmd.TRACK_RECORD_CLIP: ["recordClip"], Cmd.TRACK_MOVE_PREV: ["movePrev"], Cmd.TRACK_MOVE_NEXT: ["moveNext"],
    Cmd.TRACK_PIN: ["pin"], Cmd.TRACK_OVERDUB: ["overdub"], Cmd.TRACK_SCROLL_TO: ["scrollto"],
}
# The suffix of a value command by its op (/touch, /indicate; set, inc, reset, immediately are
# the flags of the command)
OP_SUBS = {protocol.OP_TOUCH: ["touch"], protocol.OP_INDICATE: ["indicate"]}
PARAM_SUBS = {Cmd.PARAM_VALUE: "value", Cmd.PARAM_TOUCH: "touch", Cmd.PARAM_INDICATE: "indicate"}
# The banks of BANK_SCROLL
BANKS = {Processor.TRACK: "track", Processor.SCENE: "scene", Processor.FX_TRACK: "fxtrack"}
TEMPO_STEPS = {protocol.TempoStep.PLUS: 0.01, protocol.TempoStep.MINUS: -0.01, protocol.TempoStep.PLUS_PLUS: 1.0, protocol.TempoStep.MINUS_MINUS: -1.0}
NAVIGATOR_TARGETS = {protocol.NavigatorTarget.TRACKS: "tracks", protocol.NavigatorTarget.CLIPS: "clips", protocol.NavigatorTarget.SCENES: "scenes"}
ARRANGER_COMMANDS = {
    Cmd.ARRANGER_TOGGLE_CUE_MARKERS: "toggleCueMarkers", Cmd.ARRANGER_TOGGLE_PLAYBACK_FOLLOW: "togglePlaybackFollow",
    Cmd.ARRANGER_TOGGLE_TRACK_ROW_HEIGHT: "toggleTrackRowHeight", Cmd.ARRANGER_TOGGLE_CLIP_LAUNCHER: "toggleClipLauncher",
    Cmd.ARRANGER_TOGGLE_TIME_LINE: "toggleTimeLine", Cmd.ARRANGER_TOGGLE_IO_SECTION: "toggleIoSection",
    Cmd.ARRANGER_TOGGLE_EFFECT_TRACKS: "toggleEffectTracks",
}
MIXER_COMMANDS = {
    Cmd.MIXER_TOGGLE_CLIP_LAUNCHER_SECTION: "toggleClipLauncherSection", Cmd.MIXER_TOGGLE_CROSS_FADE_SECTION: "toggleCrossFadeSection",
    Cmd.MIXER_TOGGLE_DEVICE_SECTION: "toggleDeviceSection", Cmd.MIXER_TOGGLE_IO_SECTION: "toggleIoSection",
    Cmd.MIXER_TOGGLE_METER_SECTION: "toggleMeterSection", Cmd.MIXER_TOGGLE_SENDS_SECTION: "toggleSendsSection",
}
# The attributes of the transport: the toggles, the switches (a flag)
TRANSPORT_TOGGLES = {Cmd.TRANSPORT_TOGGLE_METRONOME: "metronome", Cmd.TRANSPORT_TOGGLE_PREROLL_METRONOME: "preroll_metronome",
                     Cmd.TRANSPORT_TOGGLE_PUNCH_IN: "punch_in", Cmd.TRANSPORT_TOGGLE_PUNCH_OUT: "punch_out",
                     Cmd.TRANSPORT_TOGGLE_FILL: "fill", Cmd.TRANSPORT_TOGGLE_METRONOME_TICKS: "metronome_ticks"}
TRANSPORT_SWITCHES = {Cmd.TRANSPORT_METRONOME: "metronome", Cmd.TRANSPORT_PUNCH_IN: "punch_in", Cmd.TRANSPORT_PUNCH_OUT: "punch_out",
                      Cmd.TRANSPORT_FILL: "fill", Cmd.TRANSPORT_METRONOME_TICKS: "metronome_ticks",
                      Cmd.TRANSPORT_OVERDUB_ARRANGER: "overdub_arranger", Cmd.TRANSPORT_OVERDUB_LAUNCHER: "overdub_launcher"}
CLIP_RANGES = {Cmd.CLIP_START: "play_start", Cmd.CLIP_END: "play_stop", Cmd.CLIP_LOOP_START: "loop_start", Cmd.CLIP_LOOP_LENGTH: "loop_length"}
# The channel commands of Reaper which Bitwig does not have
REAPER_CHANNEL_IDS = (Cmd.TRACK_IN_QUANT_RESOLUTION, Cmd.TRACK_RECORD_CLIP, Cmd.TRACK_PIN, Cmd.TRACK_OVERDUB)
CHANNEL_IDS = tuple(i for i in CHANNEL_SUBS if i not in REAPER_CHANNEL_IDS)
GRID_STEP_IDS = (Cmd.GRID_STEP_TOGGLE, Cmd.GRID_STEP_SET, Cmd.GRID_STEP_SET_FROM, Cmd.GRID_STEP_UPDATE, Cmd.GRID_STEP_CLEAR,
                 Cmd.GRID_STEP_MOVE_Y, Cmd.GRID_STEP_ATTRIBUTE)
GRID_IDS = GRID_STEP_IDS + (Cmd.GRID_STEP_SIZE, Cmd.GRID_CLEAR_ALL, Cmd.GRID_CLEAR_ROW, Cmd.GRID_CLEAR_COLUMN, Cmd.GRID_SCROLL, Cmd.GRID_PAGE)
NAVIGATOR_IDS = (Cmd.NAVIGATOR_STEP, Cmd.NAVIGATOR_SELECT_TRACK)
DEVICE_INDEX_IDS = frozenset((Cmd.DEVICE_INDEX_BYPASS, Cmd.DEVICE_INDEX_REMOVE, Cmd.DEVICE_INDEX_DUPLICATE))
# The device commands whose processor is the field dev (the others: device, or eq)
DEV_SLOT_IDS = frozenset(i for i, info in protocol.CMD_INFO.items() if info.slots and info.slots[0][0][0] == "dev")


def _camel(name: str) -> str:
    """CHANCE_ENABLED -> chanceEnabled (the text names of the domains)."""
    first, *rest = name.lower().split("_")
    return first + "".join(part.capitalize() for part in rest)


STEP_ATTRIBUTES = {int(attribute): _camel(attribute.name) for attribute in protocol.StepAttr}


# Report again (HOST_BRIDGE_GUIDE §6.1, TYPED_PROTOCOL_PLAN §2.4): the entity of a command, whose
# values are reported again 300 ms later (Reporter.forget_later). The rules are those of the text
# protocol: every command with a number value (or none) of a channel (the master, a track, an
# effect track, a drum pad, a layer), of a slot of a track, of a parameter of a device or the
# project, and every command to the master.
_NO_REPORT_AGAIN = frozenset((Cmd.CHANNEL_COLOR, Cmd.TRACK_NAME))
_PAD_OWNERS = {Processor.DRUM: (PadKind.DRUM_PAD, None), Processor.PRIMARY: (PadKind.PAD, Dev.PRIMARY),
               Processor.DEVICE: (PadKind.PAD, Dev.DEVICE), Processor.EQ: (PadKind.PAD, Dev.EQ)}
_LAYER_OWNERS = {Processor.PRIMARY: (PadKind.LAYER, Dev.PRIMARY), Processor.DEVICE: (PadKind.LAYER, Dev.DEVICE),
                 Processor.EQ: (PadKind.LAYER, Dev.EQ)}
_PARAM_OWNERS = {Processor.DEVICE: ParamOwner.DEVICE, Processor.PRIMARY: ParamOwner.PRIMARY, Processor.EQ: ParamOwner.EQ,
                 Processor.PROJECT: ParamOwner.PROJECT}
# The commands whose first slot is their processor, with a number value or none
_PROCESSOR_SLOT_IDS = frozenset(i for i, info in protocol.CMD_INFO.items() if info.template.startswith("{") and not info.template.startswith("{ch}")
                                and info.kind not in ("text", "text_list", "color") and i not in IGNORED_COMMANDS)
MASTER = ("ch", ChKind.MASTER, 0)


def report_again_entity(view) -> tuple | None:
    """The entity (Reporter.forget_entity) which a command reports again, None if none."""
    h = view.h
    id_ = h[0]
    if id_ in CHANNEL_SUBS:
        if id_ in _NO_REPORT_AGAIN:
            return None
        kind, processor, index = h[3], h[4], protocol.signed(h[5])
        if kind == ChanKind.MASTER or processor == Processor.MASTER:
            return MASTER
        if index < 0:
            return None
        if kind == ChanKind.TRACK:
            if processor == Processor.TRACK:
                return ("ch", ChKind.TRACK, index)
            return ("ch", ChKind.EFFECT, index) if processor == Processor.FX_TRACK else None
        owner = (_PAD_OWNERS if kind == ChanKind.PAD else _LAYER_OWNERS).get(processor)
        return ("pad", owner[0], owner[1], index) if owner is not None else None
    if id_ in (Cmd.SLOT_ACTION, Cmd.TRACK_PARAM_VALUE):
        track = protocol.signed(h[3])
        return ("ch", ChKind.TRACK, track) if track >= 0 else None
    if id_ == Cmd.MASTER_CROSSFADER:
        return MASTER
    if id_ in _PROCESSOR_SLOT_IDS:
        processor = h[3]
        if processor == Processor.MASTER:
            return MASTER
        if id_ in PARAM_SUBS:
            owner = _PARAM_OWNERS.get(processor)
            param = protocol.signed(h[4])
            return ("param", owner, param) if owner is not None and param >= 0 else None
        if id_ in DEVICE_INDEX_IDS and processor in (Processor.TRACK, Processor.FX_TRACK):
            # '<track|fxtrack> <position>/bypass': the channel at the (1-based) position
            return ("ch", ChKind.TRACK if processor == Processor.TRACK else ChKind.EFFECT, protocol.signed(h[4]) + 1)
    return None


class CommandHandler:
    """Maps the commands of the core to model operations."""

    OP_FEATURES = ("valueInc", "valueReset", "immediately", "indication")
    """The value operations the handler implements (``/host/features``; the command features
    come from the handled commands, TYPED_PROTOCOL_PLAN §6.6)."""

    UNHANDLED_COMMANDS: frozenset[int] = frozenset((
        Cmd.METRO_VOL_SET, Cmd.METRO_VOL_STEP, Cmd.TRANSPORT_END, Cmd.TRANSPORT_CROSSFADE,
        Cmd.PROJECT_ENGINE, Cmd.APPLICATION_LAYOUT_STEP, Cmd.APPLICATION_TOGGLE_NOTE_EDITOR,
        Cmd.APPLICATION_TOGGLE_AUTOMATION_EDITOR, Cmd.APPLICATION_TOGGLE_DEVICES,
        Cmd.APPLICATION_TOGGLE_INSPECTOR, Cmd.APPLICATION_TOGGLE_MIXER,
        Cmd.APPLICATION_TOGGLE_FULL_SCREEN, Cmd.APPLICATION_TOGGLE_BROWSER, Cmd.APPLICATION_DUPLICATE,
        Cmd.APPLICATION_DELETE_SELECTION, Cmd.APPLICATION_ARROW_LEFT, Cmd.APPLICATION_ARROW_UP,
        Cmd.APPLICATION_ARROW_RIGHT, Cmd.APPLICATION_ARROW_DOWN, Cmd.APPLICATION_ZOOM_OUT,
        Cmd.APPLICATION_ZOOM_IN, Cmd.APPLICATION_DEC_TRACK_HEIGHT, Cmd.APPLICATION_INC_TRACK_HEIGHT,
        Cmd.APPLICATION_ENTER, Cmd.APPLICATION_ESCAPE, Cmd.APPLICATION_SLICE_TO_SAMPLER,
        Cmd.APPLICATION_SLICE_TO_DRUM_MACHINE, Cmd.APPLICATION_RECORD_QUANTIZATION,
        Cmd.APPLICATION_TOGGLE_RECORD_QUANTIZATION_NOTE_LENGTH, Cmd.PROJECT_PREVIOUS, Cmd.PROJECT_NEXT,
        Cmd.PROJECT_CREATE_SCENE_FROM_PLAYING_LAUNCHER_CLIPS, Cmd.PROJECT_SAVE, Cmd.PROJECT_LOAD,
        Cmd.PROJECT_UNSOLO_ALL, Cmd.PROJECT_UNMUTE_ALL, Cmd.PROJECT_CUE, Cmd.TRACK_IN_QUANT_RESOLUTION,
        Cmd.TRACK_RECORD_CLIP, Cmd.TRACK_PIN, Cmd.TRACK_OVERDUB, Cmd.SLOT_INSERT_FILE, Cmd.DEVICE_ADD,
        Cmd.TRACK_PARAM_VALUE, Cmd.MASTER_CROSSFADER, Cmd.CLIP_NOTE_TOGGLE, Cmd.CLIP_NOTE_SET,
        Cmd.CLIP_NOTE_UPDATE, Cmd.CLIP_NOTE_CLEAR, Cmd.CLIP_NOTE_MOVE_Y, Cmd.CLIP_NOTE_CLEAR_ROW,
        Cmd.CLIP_NOTE_CLEAR_POSITION, Cmd.BROWSER_CONTENT_TYPE, Cmd.BROWSER_PREVIEW,
        Cmd.BROWSER_COLUMN_STEP, Cmd.BROWSER_COLUMN_PAGE, Cmd.BROWSER_COLUMN_RESET,
        Cmd.BROWSER_REPLACE_DEVICE, Cmd.BROWSER_REPLACE_SLOT, Cmd.BROWSER_REPLACE_PAD,
        Cmd.BROWSER_ADD_TO_TRACK, Cmd.BROWSER_ADD_TO_MASTER, Cmd.BROWSER_INSERT_BEFORE,
        Cmd.BROWSER_INSERT_AFTER, Cmd.BROWSER_STOP, Cmd.BROWSER_RESULT_STEP, Cmd.BROWSER_RESULT_PAGE,
        Cmd.GROOVE_ACTIVE, Cmd.GROOVE_AMOUNT, Cmd.GROOVE_PARAM,
    ))
    """The CmdIds without a handler in the model setup of the coverage test (later steps: the
    application and project commands, the browser, groove, the clip notes, …). A snapshot: the
    coverage test of TYPED_PROTOCOL_PLAN §2.5 (test_host.CommandCoverageTest) keeps it and
    ``handled_commands`` exact."""

    def __init__(self, daw: SimDaw, banks: Banks, reporter: Reporter, clock: Callable[[], float] = lambda: 0.0):
        self.daw = daw
        self.banks = banks
        self.reporter = reporter
        self.clock = clock
        self.unhandled: list[str] = []
        """The keys of the commands which are not handled, in the order of their first use."""
        self.log: list[Command] = []
        """All commands, in order (for tests and the conformance runner)."""
        self._unhandled_keys: set[str] = set()
        self.navigator: ClipLauncherNavigator | None = None
        """The clip launcher navigator, if the controller wants it (``configure``)."""
        self.note_grids: list[NoteGrid] = []
        """The step grids of the note clips (``configure``)."""
        self.typed: dict[int, TypedHandler] = {}
        """The handlers by CmdId (process_commands)."""
        self.register_commands()

    def register_commands(self) -> None:
        """Register the handlers of the commands (a host replaces them)."""
        r = self.register_typed
        r((Cmd.REFRESH,), lambda view, cmd: self.refresh())
        r((Cmd.BANK_SCROLL,), self._bank_scroll)
        r(CHANNEL_IDS, self._channel)
        r((Cmd.BANK_INDICATE,), self._bank_indicate)
        r((Cmd.TRACK_PINNED, Cmd.TRACK_PINNED_TOGGLE, Cmd.TRACK_SELECT_PREVIOUS, Cmd.TRACK_SELECT_NEXT, Cmd.TRACK_ADD), self._track_bank)
        r((Cmd.SLOT_ACTION, Cmd.SLOT_COLOR, Cmd.SLOT_PASTE), self._slot)
        r((Cmd.SCENE_ACTION, Cmd.SCENE_NAME, Cmd.SCENE_COLOR, Cmd.SCENE_STOP_ALL, Cmd.SCENE_STOP_ALL_ALT), self._scene)
        r((Cmd.UNDO, Cmd.REDO, Cmd.PLAY_TOGGLE, Cmd.STOP_TOGGLE, Cmd.RECORD_TOGGLE, Cmd.REPEAT_TOGGLE, Cmd.TEMPO_SET, Cmd.TEMPO_STEP,
           Cmd.TIME_SIGNATURE, Cmd.TIME_SET, Cmd.TIME_SET_INT, Cmd.TIME_SET_NO_SNAP, Cmd.TIME_LOOP_START, Cmd.TIME_LOOP_LENGTH,
           Cmd.BEATS_POSITION, Cmd.BEATS_POSITION_NO_SNAP, Cmd.BEATS_LOOP_START, Cmd.BEATS_LOOP_LENGTH, Cmd.AUTOMATION_MODE,
           Cmd.AUTOMATION_TOGGLE_ARRANGER, Cmd.AUTOMATION_TOGGLE_LAUNCHER, Cmd.AUTOMATION_RESET_OVERRIDES,
           Cmd.INI_FILE_PREROLL_MEASURES), self._transport_basic)
        r((*TRANSPORT_TOGGLES, *TRANSPORT_SWITCHES, Cmd.TRANSPORT_PLAY, Cmd.TRANSPORT_STOP, Cmd.TRANSPORT_RESTART, Cmd.TRANSPORT_STOP_AND_REWIND,
           Cmd.TRANSPORT_TAP_TEMPO, Cmd.TRANSPORT_METRONOME_VOLUME), self._transport)
        r((Cmd.ACTION,), lambda view, cmd: True)  # No actions in the simulation; unknown IDs are ignored, as the Java version
        r((Cmd.APPLICATION_LAYOUT,), self._application_layout)
        r((*ARRANGER_COMMANDS, *MIXER_COMMANDS), self._panel_toggle)
        r((Cmd.MARKER_ADD, Cmd.MARKER_PREVIOUS, Cmd.MARKER_NEXT, Cmd.MARKER_ACTION), self._marker)
        r((Cmd.PROJECT_CREATE_SCENE,), self._project_create_scene)
        r((Cmd.CLIP_NAME, Cmd.CLIP_COLOR, *CLIP_RANGES, Cmd.CLIP_LOOP, Cmd.CLIP_SHUFFLE, Cmd.CLIP_ACCENT, Cmd.CLIP_DUPLICATE,
           Cmd.CLIP_DUPLICATE_CONTENT, Cmd.CLIP_CLEAR, Cmd.CLIP_PINNED, Cmd.CLIP_TRANSPOSE), self._clip)
        r(GRID_IDS, self._grid)
        r((Cmd.QUANTIZE,), self._quantize)
        r((Cmd.NOTE_REPEAT_ACTIVE, Cmd.NOTE_REPEAT_RATE, Cmd.NOTE_REPEAT_NOTE_LENGTH, Cmd.NOTE_REPEAT_MODE, Cmd.NOTE_REPEAT_VELOCITY), self._note_repeat)
        r((Cmd.DEVICE_STEP, Cmd.DEVICE_PAGE_STEP, Cmd.DEVICE_SELECTED, Cmd.DEVICE_PARAMETER_PAGE, Cmd.DEVICE_WINDOW, Cmd.DEVICE_EXPAND,
           Cmd.DEVICE_PINNED, Cmd.DEVICE_PRESET, Cmd.DEVICE_ACTION, *DEVICE_TOGGLES, Cmd.DEVICE_SIBLING, Cmd.DEVICE_INDEX_MOVE_PREV,
           Cmd.DEVICE_INDEX_MOVE_NEXT, *DEVICE_INDEX_IDS, Cmd.DRUM_CLEAR_MUTE, Cmd.DRUM_CLEAR_SOLO, Cmd.EQ_ADD, Cmd.EQ_BAND_TYPE,
           *PARAM_SUBS), self._device_command)

    def configure(self) -> None:
        """Create the objects which depend on the model setup of the controller (after
        ``Banks.configure``): the clip launcher navigator (BitwigModel: only if the model setup
        has ``wantsClipLauncherNavigator``; its commands are handled only then), the step grids
        of the note clips (``noteClips``, one per size, as BitwigModel)."""
        self.navigator = ClipLauncherNavigator(self.daw, self.banks) if self.banks.wants_clip_launcher_navigator else None
        for command_id in NAVIGATOR_IDS:
            if self.navigator is None:
                self.typed.pop(int(command_id), None)
            else:
                self.typed[int(command_id)] = self._navigator
        self.note_grids = [NoteGrid(int(steps), int(rows), lambda: self.daw.cursor_clip) for steps, rows in self.banks.setup.get("noteClips", [])]
        self.reporter.note_grids = self.note_grids

    def host_features(self) -> list[str]:
        """The value operation features for ``/host/features`` (the command features come from
        ``handled_commands``), in the order of the core."""
        return [name for name in FEATURE_ORDER if name in self.OP_FEATURES]

    def handled_commands(self) -> set[int]:
        """The CmdIds the handler executes (those with a handler, and the ignored update hints),
        for App.use_binary_commands: the core derives the command features of the host from them
        and logs a command of another ID once. Call it again after ``configure`` (the
        navigator)."""
        return set(self.typed) | IGNORED_COMMANDS

    def register_typed(self, command_ids: Iterable[int], handler: TypedHandler) -> None:
        """Handle the commands of the IDs with a typed handler."""
        for command_id in command_ids:
            self.typed[int(command_id)] = handler

    # Entry point (Backend.process_commands)

    def before_command(self) -> None:
        """Called before every command."""

    def process_commands(self, buffer) -> None:
        """The commands of one call (Backend.process_commands): by CmdId through the typed
        handlers. ``buffer`` is only valid during the call."""
        typed = self.typed
        for view in protocol.iter_commands(buffer):
            id_ = view.h[0]
            if id_ in IGNORED_COMMANDS:
                continue
            cmd = ViewCommand(view)
            self.before_command()
            self.log.append(cmd)
            entity = report_again_entity(view)
            if entity is not None:
                self.reporter.forget_later(entity)
            handler = typed.get(id_)
            try:
                handled = handler is not None and handler(view, cmd)
            except (IndexError, ValueError, TypeError) as error:
                logger.warning("Command %s %s %r failed: %s", cmd.processor, cmd.full_command, cmd.value, error)
                handled = False
            if not handled:
                self.not_handled(cmd)

    def not_handled(self, cmd: Command) -> None:
        """Log a command which is not handled, once (DbmExtension.unhandled)."""
        self.not_handled_key(f"{cmd.processor} {cmd.full_command}")

    def not_handled_key(self, text: str) -> None:
        key = re.sub(r"\d+", "<n>", text.strip())
        if key not in self._unhandled_keys:
            self._unhandled_keys.add(key)
            self.unhandled.append(key)
            logger.info("Not handled yet: %s", key)

    # Values

    def set_value(self, parameter: Parameter, cmd: Command) -> None:
        """Set, reset, change (``/inc``) or set immediately, as the command says; a change is an
        edit of the project."""
        before = parameter.value
        self._apply_value(parameter, cmd)
        if parameter.value != before:
            self.daw.application.edit_value(("value", id(parameter)), lambda value: setattr(parameter, "value", value), before, parameter.value)

    @staticmethod
    def _apply_value(parameter: Parameter, cmd: Command) -> None:
        if cmd.reset:
            parameter.reset()
        elif cmd.increment:
            parameter.inc(cmd.number)
        elif cmd.immediately:
            parameter.set_immediately(cmd.number)
        else:
            parameter.set(cmd.number)

    def value_command(self, parameter: Parameter, sub: list, cmd: Command) -> bool:
        """``<value>``, ``<value>/touch``, ``<value>/indicate`` of a parameter."""
        if not sub:
            self.set_value(parameter, cmd)
        elif sub == ["touch"]:
            parameter.touch(cmd.flag)
        elif sub == ["indicate"]:
            parameter.set_indication(cmd.flag)
        else:
            return False
        return True

    @staticmethod
    def is_touch_or_indicate(cmd: ViewCommand) -> bool:
        """A value command with touch or indication (unknown for the metronome volume, the clip
        range and the accent)."""
        return cmd.op in OP_SUBS

    # Refresh, banks

    def refresh(self) -> bool:
        self.reporter.request_dump()
        return True

    def _bank_scroll(self, view, cmd: ViewCommand) -> bool:
        bank = BANKS.get(view.bank)
        return bank is not None and self.banks.scroll(bank, int(cmd.number))

    # Transport

    def _transport_basic(self, view, cmd: ViewCommand) -> bool:
        """The Reaper-style transport (HOST_BRIDGE_GUIDE §4.2): play, stop, record, repeat,
        tempo, time, beats, automation, inifile, undo, redo."""
        t = self.daw.transport
        id_ = view.h[0]
        if id_ == Cmd.UNDO:
            self.daw.application.undo()
        elif id_ == Cmd.REDO:
            self.daw.application.redo()
        elif id_ == Cmd.PLAY_TOGGLE:
            t.toggle_play()
        elif id_ == Cmd.STOP_TOGGLE:
            t.stop()
        elif id_ == Cmd.RECORD_TOGGLE:
            t.record()
        elif id_ == Cmd.REPEAT_TOGGLE:
            t.loop = not t.loop
        elif id_ in (Cmd.TEMPO_SET, Cmd.TEMPO_STEP):
            before = t.tempo
            if id_ == Cmd.TEMPO_SET:
                t.set_tempo(cmd.number)
            else:
                t.change_tempo(TEMPO_STEPS[view.step])
            if t.tempo != before:
                self.daw.application.edit_value("tempo", t.set_tempo, before, t.tempo)
        elif id_ == Cmd.TIME_SIGNATURE:
            numerator, denominator = cmd.value
            t.set_time_signature(numerator, denominator)
        elif id_ in (Cmd.TIME_SET, Cmd.TIME_SET_INT, Cmd.TIME_SET_NO_SNAP, Cmd.TIME_LOOP_START, Cmd.TIME_LOOP_LENGTH):
            beats = cmd.number * t.tempo / 60.0
            if id_ == Cmd.TIME_LOOP_START:
                t.loop_start = max(0.0, beats)
            elif id_ == Cmd.TIME_LOOP_LENGTH:
                t.loop_length = max(0.0, beats)
            else:
                t.set_position(beats)
        elif id_ in (Cmd.BEATS_POSITION, Cmd.BEATS_POSITION_NO_SNAP):
            t.set_position(cmd.number)
        elif id_ == Cmd.BEATS_LOOP_START:
            t.loop_start = max(0.0, cmd.number)
        elif id_ == Cmd.BEATS_LOOP_LENGTH:
            t.loop_length = max(0.0, cmd.number)
        elif id_ == Cmd.AUTOMATION_TOGGLE_ARRANGER:
            t.automation_arranger = not t.automation_arranger
        elif id_ == Cmd.AUTOMATION_TOGGLE_LAUNCHER:
            t.automation_launcher = not t.automation_launcher
        elif id_ == Cmd.AUTOMATION_RESET_OVERRIDES:
            pass  # No automation overrides in the simulation
        elif id_ == Cmd.AUTOMATION_MODE:
            t.set_automation_write_mode(int(cmd.number))
        elif id_ == Cmd.INI_FILE_PREROLL_MEASURES:
            # Only the pre-roll length (TransportImpl.setPrerollMeasures)
            bars = int(cmd.number)
            if not 0 <= bars < len(PREROLLS) or PREROLLS[bars] is None:
                return False
            t.preroll = PREROLLS[bars]
        else:
            return False
        return True

    def _transport(self, view, cmd: ViewCommand) -> bool:
        """``transport <command>`` (host feature transportCommands, TransportImpl)."""
        t = self.daw.transport
        id_ = view.h[0]
        if id_ in TRANSPORT_TOGGLES:
            attribute = TRANSPORT_TOGGLES[id_]
            setattr(t, attribute, not getattr(t, attribute))
        elif id_ in TRANSPORT_SWITCHES:
            setattr(t, TRANSPORT_SWITCHES[id_], cmd.flag)
        elif id_ == Cmd.TRANSPORT_PLAY:
            t.play()
        elif id_ == Cmd.TRANSPORT_STOP:
            t.stop()
        elif id_ == Cmd.TRANSPORT_RESTART:
            t.restart()
        elif id_ == Cmd.TRANSPORT_STOP_AND_REWIND:
            t.stop_and_rewind()
        elif id_ == Cmd.TRANSPORT_TAP_TEMPO:
            t.tap_tempo(self.clock())
        elif id_ == Cmd.TRANSPORT_METRONOME_VOLUME:
            if self.is_touch_or_indicate(cmd):
                return False
            self.set_value(t.metronome_volume, cmd)
        else:
            return False
        return True

    # Application, arranger, mixer

    def _application_layout(self, view, cmd: ViewCommand) -> bool:
        """``application layout <index>`` (ApplicationImpl.setPanelLayout; the other application
        commands act on Bitwig's UI, which is not simulated)."""
        index = int(cmd.number)
        if 0 <= index < len(PANEL_LAYOUTS):
            self.daw.application.panel_layout = PANEL_LAYOUTS[index]
        return True

    def _panel_toggle(self, view, cmd: ViewCommand) -> bool:
        """``arranger|mixer toggle…``: the visibility flags (CommandHandler.handleArranger,
        handleMixer)."""
        id_ = view.h[0]
        if id_ in ARRANGER_COMMANDS:
            target, attribute = self.daw.arranger, ARRANGER_TOGGLES[ARRANGER_COMMANDS[id_]]
        else:
            target, attribute = self.daw.mixer, MIXER_TOGGLES[MIXER_COMMANDS[id_]]
        setattr(target, attribute, not getattr(target, attribute))
        return True

    # Markers

    def _marker(self, view, cmd: ViewCommand) -> bool:
        """``marker add|previous|next``, ``marker <i>/launch|select|remove`` (CommandHandler.
        handleMarker; ``select`` and ``previous|next`` set the play start position as
        TransportImpl.setPosition). Unknown without a marker bank (``numMarkers``)."""
        daw = self.daw
        markers = daw.markers
        id_ = view.h[0]
        if id_ == Cmd.MARKER_ADD:
            markers.add_at_playback_position()
        elif id_ in (Cmd.MARKER_PREVIOUS, Cmd.MARKER_NEXT):
            marker = markers.find(id_ == Cmd.MARKER_NEXT)
            if marker is not None:
                daw.transport.set_position(marker.position)
        else:
            index = view.marker
            if int(self.banks.setup.get("numMarkers", 0)) <= 0 or not 0 <= index < NUM_MARKERS:
                return False
            if index >= len(markers.items):
                return True
            marker = markers.items[index]
            action = view.action
            if action == protocol.MarkerAction.LAUNCH:
                markers.launch(marker)
            elif action == protocol.MarkerAction.SELECT:
                daw.transport.set_position(marker.position)
            else:
                markers.remove(marker)
        return True

    # The project

    def _project_create_scene(self, view, cmd: ViewCommand) -> bool:
        """``project createScene``: a new empty scene at the end, selected
        (CommandHandler.handleProject, ``Project.createScene``; that Bitwig selects it is known
        only from the observer, which showed the selected slot deselected, flexi.scene.create of
        generic_flexi project). The other project commands (cue, remote controls, …) are not
        simulated."""
        self.daw.add_scene().select()
        return True

    # The cursor clip and its step grids

    def _clip(self, view, cmd: ViewCommand) -> bool:
        """``clip …``: the cursor clip (CommandHandler.handleClip of the Bitwig extension). The
        commands of the Reaper note list (``note/<pitch>/…``, sent only without a host grid) are
        unknown, as there."""
        daw = self.daw
        clip = daw.cursor_clip
        slot = daw.cursor_clip_slot
        id_ = view.h[0]
        if id_ == Cmd.CLIP_NAME:
            if clip is not None:
                clip.name = slot.name = cmd.text or ""
        elif id_ == Cmd.CLIP_COLOR:
            color = cmd.color
            if color is not None and slot is not None:
                slot.clip_color = color
        elif id_ in CLIP_RANGES:
            if self.is_touch_or_indicate(cmd):
                return False
            if clip is not None:
                attribute = CLIP_RANGES[id_]
                value = getattr(clip, attribute) + cmd.number if cmd.increment else cmd.number
                setattr(clip, attribute, max(0.0, value))
        elif id_ in (Cmd.CLIP_LOOP, Cmd.CLIP_SHUFFLE):
            if clip is not None:
                setattr(clip, "loop" if id_ == Cmd.CLIP_LOOP else "shuffle", cmd.flag)
        elif id_ == Cmd.CLIP_ACCENT:
            if self.is_touch_or_indicate(cmd):
                return False
            if clip is not None:
                self.set_value(clip.accent, cmd)
        elif id_ == Cmd.CLIP_DUPLICATE:
            if slot is not None:
                slot.duplicate()
        elif id_ == Cmd.CLIP_DUPLICATE_CONTENT:
            if clip is not None:
                clip.duplicate_content()
        elif id_ == Cmd.CLIP_CLEAR:
            if clip is not None:
                clip.notes.clear()
        elif id_ == Cmd.CLIP_PINNED:
            daw.set_cursor_clip_pinned(cmd.flag)
        elif id_ == Cmd.CLIP_TRANSPOSE:
            # CursorClipImpl: only on tracks with notes
            if clip is not None and slot.track.can_hold_notes:
                clip.transpose(int(cmd.number))
        else:
            return False
        return True

    def find_grid(self, view) -> NoteGrid | None:
        """The step grid of a grid command (by its size), None if there is none."""
        steps, rows = view.h[3], view.h[4]
        return next((grid for grid in self.note_grids if grid.num_steps == steps and grid.num_rows == rows), None)

    def _grid(self, view, cmd: ViewCommand) -> bool:
        grid = self.find_grid(view)
        return grid is not None and self.grid_command(grid, view, cmd)

    @staticmethod
    def grid_command(grid: NoteGrid, view, cmd: ViewCommand) -> bool:
        """BitwigNoteClip.handle: ``stepSize``, ``scroll``, ``page/+|-``, ``clearAll``,
        ``clearRow/<ch>/<row>``, ``clearColumn/<ch>/<step>``, ``step/<ch>/<x>/<y>/toggle|set|
        setFrom|update|clear|moveY|<attribute>``."""
        id_ = view.h[0]
        if id_ == Cmd.GRID_STEP_SIZE:
            grid.set_step_size(cmd.number)
        elif id_ == Cmd.GRID_SCROLL:
            grid.scroll_to(int(cmd.number))
        elif id_ == Cmd.GRID_PAGE:
            grid.scroll_page(1 if view.dir == protocol.Dir.NEXT else -1)
        elif id_ == Cmd.GRID_CLEAR_ALL:
            grid.clear_all()
        elif id_ == Cmd.GRID_CLEAR_ROW:
            grid.clear_row(view.channel, view.row)
        elif id_ == Cmd.GRID_CLEAR_COLUMN:
            grid.clear_column(view.channel, view.column)
        else:
            channel, x, y = view.channel, view.step, view.note
            if id_ == Cmd.GRID_STEP_TOGGLE:
                grid.toggle(channel, x, y, int(cmd.number))
            elif id_ == Cmd.GRID_STEP_CLEAR:
                grid.clear(channel, x, y)
            elif id_ == Cmd.GRID_STEP_MOVE_Y:
                grid.move_y(channel, x, y, int(cmd.number))
            elif id_ == Cmd.GRID_STEP_SET:
                velocity, duration = cmd.value
                grid.set(channel, x, y, int(velocity), float(duration))
            elif id_ == Cmd.GRID_STEP_SET_FROM:
                # CursorClipImpl.setStep (NotePosition, IStepInfo): the note, then its values
                # (Bitwig extension: 100 ms later; here at once, clip.py)
                values = (cmd.text or "").split()
                if len(values) < 24:
                    return False
                note = grid.set(channel, x, y, int(float(values[4]) * 127), float(values[3]))
                if note is not None:
                    for attribute, index in (("velocity", 4), ("gain", 11), ("pan", 9), ("pressure", 7), ("releaseVelocity", 6), ("timbre", 8), ("transpose", 10)):
                        set_attribute(note, attribute, float(values[index]))
            elif id_ == Cmd.GRID_STEP_UPDATE:
                values = (cmd.text or "").split()
                if len(values) < 24:
                    return False
                note = grid.note(channel, x, y)
                if note is not None:
                    update_from_wire(note, values)
            else:
                attribute, value = cmd.value
                name = STEP_ATTRIBUTES.get(attribute)
                if name not in ATTRIBUTES:
                    return False
                note = grid.note(channel, x, y)
                if note is not None:
                    set_attribute(note, name, value)
        return True

    def _quantize(self, view, cmd: ViewCommand) -> bool:
        """``quantize <amount>``: ``cursorClip.quantize`` if 0 < amount <= 1 (the note starts move
        towards the steps of 1/16 by the amount; Bitwig's grid of the quantization is not
        simulated)."""
        clip = self.daw.cursor_clip
        amount = cmd.number
        if clip is not None and 0 < amount <= 1:
            for note in clip.notes:
                target = round(note.start / 0.25) * 0.25
                note.start += (target - note.start) * amount
        return True

    # The clip launcher navigator

    def _navigator(self, view, cmd: ViewCommand) -> bool:
        """``navigator selectTrack|tracks/-|+|clips/-|+|scenes/-|+`` (CommandHandler.
        handleNavigator: registered only with a navigator)."""
        if view.h[0] == Cmd.NAVIGATOR_SELECT_TRACK:
            self.navigator.select_track(int(cmd.number))
        else:
            self.navigator.step(NAVIGATOR_TARGETS[view.target], 1 if view.dir == protocol.Dir.NEXT else -1)
        return True

    # Note repeat

    def _note_repeat(self, view, cmd: ViewCommand) -> bool:
        """``noterepeat active|rate|notelength|mode|velocity`` on the arpeggiator of the default
        note input (CommandHandler.handleNoteRepeat of the Bitwig extension). Without a note input
        the commands are ignored, as there."""
        note_input = self.daw.note_input
        if note_input is None:
            return True
        arpeggiator = note_input.arpeggiator
        id_ = view.h[0]
        if id_ == Cmd.NOTE_REPEAT_ACTIVE:
            arpeggiator.enabled = cmd.flag
        elif id_ == Cmd.NOTE_REPEAT_RATE:
            # The core sends the inverse of the period in beats
            if cmd.number > 0:
                arpeggiator.rate = 1.0 / cmd.number
        elif id_ == Cmd.NOTE_REPEAT_NOTE_LENGTH:
            arpeggiator.gate_length = cmd.number
        elif id_ == Cmd.NOTE_REPEAT_MODE:
            arpeggiator.set_mode_index(int(cmd.number))
        else:
            arpeggiator.use_pressure_to_velocity = cmd.flag
        return True

    # Devices

    def _device_command(self, view, cmd: ViewCommand) -> bool:
        """The device commands by their processor (the field dev, else ``device`` or ``eq``):
        ``cursor_device_command``, ``specific_command`` (``primary``, ``drum``),
        ``eq_command``; a device command to another processor is unknown."""
        id_ = view.h[0]
        if id_ in DEV_SLOT_IDS:
            processor = view.h[3]
        else:
            processor = Processor.EQ if id_ in (Cmd.EQ_ADD, Cmd.EQ_BAND_TYPE) else Processor.DEVICE
        if processor == Processor.DEVICE:
            return self.cursor_device_command(view, cmd)
        if processor in (Processor.PRIMARY, Processor.DRUM):
            return self.specific_command(processor, view, cmd)
        if processor == Processor.EQ:
            return self.eq_command(view, cmd)
        return False

    def cursor_device_command(self, view, cmd: ViewCommand) -> bool:
        """The cursor device (CommandHandler.handleDevice of the Bitwig extension; device
        positions are 1-based in the text, 0-based in the record): ``+``, ``-`` (selectNext /
        selectPrevious), ``page/+``, ``page/-`` (the sibling window), ``selected <n>`` (select
        the n-th sibling), ``parameterPage``, ``window``, ``pinned``, ``expand``, the commands of
        the device itself (``own_device_command``), ``preset`` (ignored), ``param/<n>/…``,
        ``sibling/<n>/…``, ``<position>/bypass|remove|duplicate|movePrev|moveNext``; the layers
        (``layer/<n>/…``) are channel commands."""
        daw = self.daw
        device = daw.cursor_device
        chain = daw.cursor_device_chain
        id_ = view.h[0]
        if id_ == Cmd.DEVICE_STEP:
            if device is not None and chain is not None:
                chain.select(device)
                chain.select_neighbour(1 if view.dir == protocol.Dir.NEXT else -1)
                if daw.cursor.device_pinned:
                    daw.cursor.pinned_device = chain.selected
        elif id_ == Cmd.DEVICE_PAGE_STEP:
            if chain is not None:
                chain.scroll_siblings(1 if view.dir == protocol.Dir.NEXT else -1, NUM_SIBLINGS)
        elif id_ == Cmd.DEVICE_SELECTED:
            index = int(cmd.number)
            if chain is not None and 0 <= index < NUM_SIBLINGS and chain.sibling_offset + index < len(chain.devices):
                chain.select_index(chain.sibling_offset + index)
        elif id_ == Cmd.DEVICE_PARAMETER_PAGE:
            pages = device.page_names(cursor=True) if device is not None else []
            if pages:
                device.select_page(max(0, min(int(cmd.number), len(pages) - 1)), cursor=True)
        elif id_ in (Cmd.DEVICE_WINDOW, Cmd.DEVICE_EXPAND):
            if device is not None:
                setattr(device, "window_open" if id_ == Cmd.DEVICE_WINDOW else "expanded", cmd.flag)
        elif id_ == Cmd.DEVICE_PINNED:
            daw.set_cursor_device_pinned(cmd.flag)
        elif id_ == Cmd.DEVICE_PRESET:
            pass  # Reaper's preset browser; Bitwig reports its own browser
        elif id_ == Cmd.DEVICE_ACTION:
            return self.own_device_command(device, DEVICE_ACTIONS[view.action])
        elif id_ in DEVICE_TOGGLES:
            return self.own_device_command(device, DEVICE_TOGGLES[id_])
        elif id_ in PARAM_SUBS:
            if cmd.op in OP_SUBS:
                return False
            return self.parameter_command(device, view.param, PARAM_SUBS[id_], cmd, cursor=True)
        elif id_ == Cmd.DEVICE_SIBLING:
            index = view.sibling
            if not 0 <= index < NUM_SIBLINGS:
                return False
            position = (chain.sibling_offset if chain is not None else 0) + index
            sibling = chain.devices[position] if chain is not None and position < len(chain.devices) else None
            return self.own_device_command(sibling, DEVICE_ACTIONS[view.action])
        elif id_ in (Cmd.DEVICE_INDEX_MOVE_PREV, Cmd.DEVICE_INDEX_MOVE_NEXT):
            self.move_cursor_device(-1 if id_ == Cmd.DEVICE_INDEX_MOVE_PREV else 1)
        elif id_ in DEVICE_INDEX_IDS:
            sibling = self.find_sibling(view.device)
            if sibling is None:
                return False
            if id_ == Cmd.DEVICE_INDEX_BYPASS:
                # As Reaper's DeviceProcessor: the value is the new bypass state
                sibling.enabled = cmd.number <= 0
            elif id_ == Cmd.DEVICE_INDEX_REMOVE:
                chain.remove(sibling)
            else:
                chain.duplicate(sibling)
        else:
            return False
        return True

    def find_sibling(self, position: int):
        """CommandHandler.findSibling: the device of the chain position among the siblings of the
        window, None if it is not there."""
        chain = self.daw.cursor_device_chain
        if chain is None or position < 0:
            return None
        if chain.sibling_offset <= position < chain.sibling_offset + NUM_SIBLINGS and position < len(chain.devices):
            return chain.devices[position]
        return None

    def move_cursor_device(self, direction: int) -> None:
        """CommandHandler.moveCursorDevice (CursorDeviceImpl.swapWithPrevious / swapWithNext):
        swap the cursor device with its neighbour and select the neighbour."""
        device = self.daw.cursor_device
        chain = self.daw.cursor_device_chain
        if device is None or chain is None:
            return
        index = chain.devices.index(device)
        other = index + direction
        if 0 <= other < len(chain.devices):
            neighbour = chain.devices[other]
            chain.devices[index], chain.devices[other] = neighbour, device
            chain.select(neighbour)

    def num_device_layers(self) -> int:
        return max(0, int(self.banks.setup.get("numDeviceLayers", 8)))

    def specific_device(self, processor: int):
        """(device, number of layers, has pages) of ``primary`` (the first instrument of the
        cursor track) and ``drum`` (its first Drum Machine); None if the controller did not ask
        for it (``deviceIds``, ``wantsDrumDevice``: the Bitwig extension drops the commands)."""
        if processor == Processor.PRIMARY:
            if not self.wants_device("FIRST_INSTRUMENT"):
                return None
            return self.daw.primary_device, NUM_LAYERS, True
        if not bool(self.banks.setup.get("wantsDrumDevice", True)):
            return None
        return self.daw.drum_device, 0, False

    def specific_command(self, processor: int, view, cmd: ViewCommand) -> bool:
        """``primary …`` and ``drum …`` (CommandHandler.handleSpecific): ``clearMute``,
        ``clearSolo``, the commands of the device itself, ``parameterPage`` and ``param/<n>/…``
        (only the first instrument has pages); the indication of the pads is BANK_INDICATE, the
        pads and layers are channel commands."""
        specific = self.specific_device(processor)
        if specific is None:
            return True
        device, _num_layers, has_pages = specific
        id_ = view.h[0]
        if id_ == Cmd.DRUM_CLEAR_MUTE:
            if device is not None:
                device.clear_muted_pads()
        elif id_ == Cmd.DRUM_CLEAR_SOLO:
            if device is not None:
                device.clear_soloed_pads()
        elif id_ == Cmd.DEVICE_ACTION:
            return self.own_device_command(device, DEVICE_ACTIONS[view.action])
        elif id_ in DEVICE_TOGGLES:
            return self.own_device_command(device, DEVICE_TOGGLES[id_])
        elif id_ == Cmd.DEVICE_PARAMETER_PAGE:
            if not has_pages:
                return False
            if device is not None and device.pages:
                device.select_page(max(0, min(int(cmd.number), len(device.pages) - 1)))
        elif id_ in PARAM_SUBS:
            if cmd.op in OP_SUBS:
                return False
            if not has_pages:
                return True  # No page cursors: an index beyond them is ignored
            return self.parameter_command(device, view.param, PARAM_SUBS[id_], cmd)
        else:
            return False
        return True

    def _bank_indicate(self, view, cmd: ViewCommand) -> bool:
        """``track|fxtrack indicate`` (the clip launcher feedback), ``drum|primary indicate``
        (the indication of the drum pads)."""
        bank = view.bank
        if bank == Processor.TRACK:
            self.banks.show_track_clip_launcher_feedback = cmd.flag
        elif bank == Processor.FX_TRACK:
            self.banks.show_fx_track_clip_launcher_feedback = cmd.flag
        elif bank in (Processor.PRIMARY, Processor.DRUM):
            specific = self.specific_device(bank)
            if specific is not None and specific[0] is not None:
                specific[0].drum_pad_indication = cmd.flag
        else:
            return False
        return True

    def layer_command(self, device, pads: bool, index: int, sub: list, cmd: Command, num_pads: int, num_layers: int) -> bool:
        """The channel commands on a drum pad or layer (CommandHandler.handleSpecific);
        ``select`` selects in the editor. Unknown beyond the bank sizes; nothing happens to an
        item without a device."""
        size = num_pads if pads else num_layers
        if not 0 <= index < size:
            return False
        if device is None:
            items = []
        elif pads:
            items = device.drum_pads or []
        else:
            items = device.layer_list()
        item = items[index] if index < len(items) else None
        command = sub[0]
        if command not in ("select", "mute", "solo", "active", "color", "volume", "pan"):
            return False
        if item is None or not item.exists:
            return True
        if command == "select":
            if cmd.flag:
                item.select()
            return True
        return self.channel_command(item, sub, cmd)

    def wants_device(self, device_id: str) -> bool:
        """The controller asks for the specific device (``deviceIds`` of the model setup)."""
        return device_id in self.banks.setup.get("deviceIds", [])

    def own_device_command(self, device, command: str) -> bool:
        """The commands of a device on the device itself (CommandHandler.handleOwnDevice):
        ``window/toggle``, ``expand/toggle``, ``parameters/toggle``, ``bypass`` (a toggle),
        ``remove``, ``duplicate``, ``select``; False if unknown. Nothing happens without the
        device (Bitwig ignores the commands of a device which does not exist)."""
        if command not in OWN_DEVICE_COMMANDS:
            return False
        if device is None:
            return True
        chain = self.daw.device_chain_of(device)
        if command == "window/toggle":
            device.window_open = not device.window_open
        elif command == "expand/toggle":
            device.expanded = not device.expanded
        elif command == "parameters/toggle":
            device.parameters_visible = not device.parameters_visible
        elif command == "bypass":
            device.enabled = not device.enabled
        elif chain is not None and command == "remove":
            chain.remove(device)
        elif chain is not None and command == "duplicate":
            chain.duplicate(device)
        elif chain is not None and command == "select":
            chain.select(device)
        return True

    def eq_command(self, view, cmd: ViewCommand) -> bool:
        """``eq …``: the first EQ+ of the cursor track in the layout of the Reaper EQ
        (CommandHandler.handleEqualizer of the Bitwig extension): ``add``, the commands of the
        device itself, ``band/<band> <Reaper type>``, ``param/<index>/value|touch|indicate``.
        Unhandled if the controller did not ask for the EQ (``deviceIds``).

        ``add`` inserts a new EQ+ at the end of the chain of the cursor track
        (``endOfDeviceChainInsertionPoint ().insertBitwigDevice``); the device selection stays
        (whether Bitwig selects the new device was not observed)."""
        if not self.wants_device("EQ"):
            return False
        device = self.daw.eq_device
        id_ = view.h[0]
        if id_ == Cmd.EQ_ADD:
            channel = self.daw.cursor_channel
            if channel is not None:
                channel.devices.add(eq_plus(self.daw.preferences))
        elif id_ == Cmd.DEVICE_ACTION:
            return self.own_device_command(device, DEVICE_ACTIONS[view.action])
        elif id_ in DEVICE_TOGGLES:
            return self.own_device_command(device, DEVICE_TOGGLES[id_])
        elif id_ == Cmd.EQ_BAND_TYPE:
            if device is None:
                return True
            return device.equalizer.set_reaper_type(view.band, int(cmd.number))
        elif id_ in PARAM_SUBS:
            if cmd.op in OP_SUBS:
                return False
            index = view.param
            parameter = device.equalizer.parameter(index) if device is not None else None
            if parameter is None:
                # BitwigModel: an index out of the range is unknown, a parameter of a device
                # which does not exist ignores the command
                return device is None and 0 <= index < 3 * EQ_NUM_BANDS
            name = PARAM_SUBS[id_]
            self.value_command(parameter, [] if name == "value" else [name], cmd)
        else:
            return False
        return True

    def parameter_command(self, device, index: int, name: str, cmd: Command, cursor: bool = False) -> bool:
        """``param/<index>/value|touch|indicate`` on the flat parameter list of the page cursors
        (CommandHandler.handleParameter): an index beyond the ``numParamPages`` page cursors or of
        a page the device does not have is ignored."""
        num_pages = max(0, int(self.banks.setup.get("numParamPages", 8)))
        parameter = device.parameter(index, self.daw.preferences, cursor) if device is not None and 0 <= index < num_pages * PARAMS_PER_PAGE else None
        if parameter is not None:
            self.value_command(parameter, [] if name == "value" else [name], cmd)
        return True

    # Channels

    @staticmethod
    def channel_sub(view) -> list:
        """The sub command of a channel command (CHANNEL_SUBS: the send or slot index, the
        touch or indication of a value)."""
        h = view.h
        sub = CHANNEL_SUBS[h[0]]
        if None in sub:
            sub = [protocol.signed(h[6]) if part is None else part for part in sub]
        if h[2] in OP_SUBS:
            sub = sub + OP_SUBS[h[2]]
        return sub

    def _channel(self, view, cmd: ViewCommand) -> bool:
        """The channel commands of the master, the tracks, the effect tracks (``typed_master``,
        ``typed_track``), the drum pads and the layers (``typed_layer``)."""
        h = view.h
        sub = self.channel_sub(view)
        kind = h[3]
        if kind == ChanKind.MASTER:
            return self.typed_master(sub, cmd)
        if kind == ChanKind.TRACK:
            if h[4] not in (Processor.TRACK, Processor.FX_TRACK):
                return False
            return self.typed_track(h[4] == Processor.FX_TRACK, protocol.signed(h[5]), sub, cmd)
        return self.typed_layer(h[4], kind == ChanKind.PAD, protocol.signed(h[5]), sub, cmd)

    def typed_master(self, sub: list, cmd: Command) -> bool:
        """A channel command of the master."""
        return self.channel_command(self.daw.master, sub, cmd)

    def typed_track(self, fx: bool, position: int, sub: list, cmd: Command) -> bool:
        """A channel command of a track or effect track."""
        tracks = self.daw.effect_tracks if fx else self.track_list()
        if not 0 <= position < len(tracks):
            return False
        return self.track_command(tracks[position], position, sub, cmd, tracks)

    def typed_layer(self, owner: int, pads: bool, index: int, sub: list, cmd: Command) -> bool:
        """A channel command of a drum pad or layer of ``drum``, ``primary`` (all, as
        CommandHandler.handleSpecific) or the cursor device (only the layers, with a sub command
        of one part)."""
        if owner in (Processor.PRIMARY, Processor.DRUM):
            specific = self.specific_device(owner)
            if specific is None:
                return True
            device, num_layers, _has_pages = specific
            return self.layer_command(device, pads, index, sub, cmd, NUM_DRUM_PADS, num_layers)
        if owner == Processor.DEVICE and not pads and len(sub) == 1:
            return self.layer_command(self.daw.cursor_device, False, index, sub, cmd, 0, self.num_device_layers())
        return False

    # Tracks

    def track_list(self) -> list[Channel]:
        return self.daw.track_list(self.banks.full_flat_track_list)

    def _track_bank(self, view, cmd: ViewCommand) -> bool:
        """The commands of the track bank and the cursor track."""
        daw = self.daw
        id_ = view.h[0]
        if id_ == Cmd.TRACK_PINNED:
            daw.set_cursor_pinned(cmd.flag)
        elif id_ == Cmd.TRACK_PINNED_TOGGLE:
            daw.set_cursor_pinned(not daw.cursor.pinned)
        elif id_ in (Cmd.TRACK_SELECT_PREVIOUS, Cmd.TRACK_SELECT_NEXT):
            daw.select_neighbour(self.track_list(), -1 if id_ == Cmd.TRACK_SELECT_PREVIOUS else 1)
        else:
            return self._add_track(cmd)
        return True

    def _add_track(self, cmd: Command) -> bool:
        """``track addTrack <type> <name> <RGB(…)> <devices…>`` (AbstractTrackBankImpl.addTrack):
        the new track is selected. The devices are Reaper creation names."""
        values = cmd.value if isinstance(cmd.value, list) else []
        if not values:
            return False
        track_type = values[0] if values[0] in ("INSTRUMENT", "EFFECT") else "AUDIO"
        name = values[1] if len(values) > 1 and values[1] else None
        color = color_from_command(values[2]) if len(values) > 2 else None
        track = self.daw.add_track(track_type, name, color)
        track.select()
        for device in values[3:]:
            self.not_handled_key("addTrack device " + device)
        return True

    def track_command(self, track: Channel, position: int, sub: list, cmd: Command, tracks: list[Channel]) -> bool:
        """The commands of a track of a list (TrackImpl, ChannelImpl)."""
        daw = self.daw
        name = sub[0]
        if not isinstance(track, Track):
            # The master in the full flat track list: it has no clip slots, Bitwig ignores the
            # slot commands
            if name == "createClip":
                return True
            return self.channel_command(track, sub, cmd)
        if sub == ["select"]:
            if cmd.flag:
                track.select()
        elif sub == ["scrollto"]:
            self.banks.scroll("fxtrack" if track.is_effect else "track", position)
        elif sub == ["recarm"]:
            track.set_arm(cmd.flag)
        elif sub == ["monitor"]:
            track.monitor_mode = "ON" if cmd.flag else "OFF"
        elif sub == ["autoMonitor"]:
            track.monitor_mode = "AUTO" if cmd.flag else "OFF"
        elif sub == ["isGroupExpanded"]:
            track.group_expanded = cmd.flag
        elif sub == ["name"]:
            if cmd.text is None:
                return False
            track.name = cmd.text
        elif sub == ["remove"]:
            daw.remove_track(track)
        elif sub == ["duplicate"]:
            daw.duplicate_track(track)
        elif sub in (["stop"], ["stopAlt"]):
            track.stop()
        elif sub == ["returnToArrangement"]:
            pass  # No arrangement in the simulation
        elif sub in (["movePrev"], ["moveNext"]):
            daw.move_track(track, -1 if name == "movePrev" else 1)
        elif sub == ["toggleMultiSelect"]:
            pass  # No multi-selection in the Bitwig API, as the Java version
        elif name == "createClip" and len(sub) == 2:
            index = parse_index(sub[1])
            if not 0 <= index < len(track.slots):
                return False
            track.slots[index].create_clip(cmd.number)
        elif name == "send":
            return self.send_command(track, sub[1:], cmd)
        else:
            return self.channel_command(track, sub, cmd)
        return True

    def channel_command(self, channel: Channel, sub: list, cmd: Command) -> bool:
        """The commands of all channels (ChannelImpl)."""
        if not sub:
            return False
        name = sub[0]
        if sub == ["select"]:
            if cmd.flag:
                channel.select()
        elif sub == ["mute"]:
            channel.set_mute(cmd.flag)
        elif sub == ["solo"]:
            channel.set_solo(cmd.flag)
        elif sub == ["solo", "toggle"]:
            channel.toggle_solo()
        elif sub == ["active"]:
            channel.activated = cmd.flag
        elif sub == ["color"]:
            color = cmd.color
            if color is None:
                return False
            channel.color = color
        elif sub == ["name"] and cmd.text is not None:
            channel.name = cmd.text
        elif name in ("volume", "pan"):
            return self.value_command(channel.volume if name == "volume" else channel.pan, sub[1:], cmd)
        else:
            return False
        return True

    def send_command(self, track: Track, sub: list, cmd: Command) -> bool:
        """``send/<index>/volume[/touch|/indicate]``, ``send/<index>/active`` (a toggle,
        SendImpl)."""
        index = parse_index(sub[0]) if sub else -1
        if not 0 <= index < min(len(track.sends), self.banks.num_sends) or len(sub) < 2:
            return False
        send = track.sends[index]
        if sub[1] == "volume":
            return self.value_command(send.volume, sub[2:], cmd)
        if sub[1:] == ["active"]:
            send.enabled = not send.enabled
            return True
        return False

    # Slots and scenes

    def _slot(self, view, cmd: ViewCommand) -> bool:
        """``track <t>/clip/<s>/…`` (SlotImpl): the slots of the track list."""
        tracks = self.track_list()
        position = view.track
        if not 0 <= position < len(tracks):
            return False
        track = tracks[position]
        if not isinstance(track, Track):
            return True  # The master: no slots, Bitwig ignores the slot commands
        return self.slot_command(track, view, cmd)

    def slot_command(self, track: Track, view, cmd: ViewCommand) -> bool:
        """SLOT_ACTION, SLOT_COLOR, SLOT_PASTE of a slot of the track."""
        index = view.slot
        if not 0 <= index < len(track.slots):
            return False
        slot = track.slots[index]
        id_ = view.h[0]
        if id_ == Cmd.SLOT_COLOR:
            color = cmd.color
            if color is None:
                return False
            slot.clip_color = color
            return True
        if id_ == Cmd.SLOT_PASTE:
            track_index, slot_index = cmd.value
            tracks = self.track_list()
            if not 0 <= track_index < len(tracks) or not 0 <= slot_index < len(tracks[track_index].slots):
                return False
            slot.copy_from(tracks[track_index].slots[slot_index])
            return True
        action = view.action
        if action in (protocol.SlotAction.LAUNCH, protocol.SlotAction.LAUNCH_ALT):
            slot.launch()
        elif action in (protocol.SlotAction.LAUNCH_RELEASE, protocol.SlotAction.LAUNCH_RELEASE_ALT):
            pass  # Releases only matter for the launch mode "gate", which is not simulated
        elif action == protocol.SlotAction.RECORD:
            slot.record()
        elif action == protocol.SlotAction.SELECT:
            slot.select()
        elif action == protocol.SlotAction.REMOVE:
            slot.delete()
        else:
            slot.duplicate()
        return True

    def _scene(self, view, cmd: ViewCommand) -> bool:
        """``scene <index>/<command>``, ``scene stop|stopAlt`` (SceneImpl, SceneBankImpl)."""
        daw = self.daw
        id_ = view.h[0]
        if id_ in (Cmd.SCENE_STOP_ALL, Cmd.SCENE_STOP_ALL_ALT):
            daw.stop_all_clips()
            return True
        index = view.scene
        if not 0 <= index < len(daw.scenes):
            return False
        scene = daw.scenes[index]
        if id_ == Cmd.SCENE_NAME:
            if cmd.text is None:
                return False
            scene.name = cmd.text
            return True
        if id_ == Cmd.SCENE_COLOR:
            color = cmd.color
            if color is None:
                return False
            scene.color = color
            return True
        action = view.action
        if action in (protocol.SceneAction.LAUNCH, protocol.SceneAction.LAUNCH_ALT):
            scene.launch()
        elif action in (protocol.SceneAction.LAUNCH_RELEASE, protocol.SceneAction.LAUNCH_RELEASE_ALT):
            pass  # See slot_command
        elif action == protocol.SceneAction.SELECT:
            scene.select()
        elif action == protocol.SceneAction.REMOVE:
            daw.remove_scene(scene)
        else:
            daw.duplicate_scene(scene)
        return True


__all__ = ["Command", "CommandHandler", "FEATURE_ORDER", "ViewCommand", "parse_index", "report_again_entity"]
