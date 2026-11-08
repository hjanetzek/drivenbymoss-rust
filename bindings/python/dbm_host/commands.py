# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Executes the commands of the core (processor, command, value) on the simulated DAW.

Implements HOST_BRIDGE_GUIDE.md §4 (the command reference, core -> host): 4.1 the value command
suffixes, 4.2 transport, ``noterepeat``, ``application layout``, ``arranger``, ``mixer``,
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

Dispatch: one handler per processor (``CommandHandler.register``). A handler gets a parsed
``Command`` and returns False if it does not know the command; the command is then logged once
as ``Not handled yet: <processor> <command>`` (digits replaced by ``<n>``, as the Bitwig
extension) and kept in ``unhandled``. Each handler declares the host features
(HOST_BRIDGE_GUIDE.md §2) it implements; ``host_features`` is reported as ``/host/features``.
The processors which are not simulated yet (``browser``, ``groove``) are registered the same
way.
"""

import logging
import re
from dataclasses import dataclass
from typing import Any, Callable, Iterable

import drivenbymoss
from drivenbymoss import protocol
from drivenbymoss.protocol import ChanKind, Cmd, Processor, signed

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

IMMEDIATELY = "/immediately"
INCREMENT = "/inc"
RESET = "/reset"


@dataclass
class Command:
    """A command of the core with the value command suffixes split off (HOST_BRIDGE_GUIDE §4.1)."""

    processor: str
    full_command: str
    """The command as the core sent it."""
    command: str
    """The command without ``/immediately``, ``/inc``, ``/reset``."""
    value: Any
    """None, a str, a list of str, an int or a float."""
    immediately: bool = False
    increment: bool = False
    reset: bool = False

    @classmethod
    def parse(cls, processor: str, full_command: str, value: Any) -> "Command":
        command = full_command
        flags = {}
        for suffix, name in ((IMMEDIATELY, "immediately"), (INCREMENT, "increment"), (RESET, "reset")):
            flags[name] = command.endswith(suffix)
            if flags[name]:
                command = command[: -len(suffix)]
        return cls(processor, full_command, command, value, **flags)

    @property
    def parts(self) -> list[str]:
        return self.command.split("/") if self.command else []

    @property
    def number(self) -> float:
        """The value as a number; a command without a value counts as 1 (as the Bitwig
        extension)."""
        if self.value is None:
            return 1.0
        if isinstance(self.value, (int, float)):
            return float(self.value)
        try:
            return float(str(self.value).strip())
        except ValueError:
            return 0.0

    @property
    def flag(self) -> bool:
        return self.number > 0

    @property
    def text(self) -> str | None:
        return self.value if isinstance(self.value, str) else None


def parse_index(text: str) -> int:
    try:
        return int(text)
    except (TypeError, ValueError):
        return -1


Handler = Callable[[Command], bool]


# The typed commands (TYPED_PROTOCOL_PLAN §2.7, §6.4): the binary form of the core's commands is
# dispatched by CmdId; the IDs without a typed handler go through their legacy text (the
# processor handlers above, the transition). A typed handler gets the view of the record and a
# ViewCommand and calls the same hooks as the text handlers (channel_command, track_command,
# value_command, set_value, …) with constant ``sub`` lists, so the overrides of a host
# (ZynCommandHandler) work for both forms. It returns True (handled), False (not handled:
# logged) or FALLBACK (the text handler decides, nothing was done yet).

FALLBACK = None

# The sub commands of the typed channel commands (the parts after the channel in the text form)
CHANNEL_SUBS = {
    Cmd.CHANNEL_MUTE: ["mute"], Cmd.CHANNEL_SOLO: ["solo"], Cmd.CHANNEL_SOLO_TOGGLE: ["solo", "toggle"],
    Cmd.CHANNEL_SELECT: ["select"], Cmd.TRACK_REC_ARM: ["recarm"],
}
CHANNEL_VALUES = {Cmd.CHANNEL_VOLUME: "volume", Cmd.CHANNEL_PAN: "pan"}
# The suffix of a value command by its op (/touch, /indicate; set, inc, reset, immediately are
# the flags of the command)
OP_SUBS = {protocol.OP_TOUCH: ["touch"], protocol.OP_INDICATE: ["indicate"]}
PARAM_SUBS = {Cmd.PARAM_VALUE: "value", Cmd.PARAM_TOUCH: "touch", Cmd.PARAM_INDICATE: "indicate"}
# The processors of the typed parameter commands (the field dev of PARAM_*)
PARAM_PROCESSORS = {Processor.DEVICE: "device", Processor.PRIMARY: "primary", Processor.EQ: "eq", Processor.DRUM: "drum"}


def _record_text(view) -> str:
    """The legacy text of the record of a view (the core's conversion, transition)."""
    n = view.h[7]
    return drivenbymoss.messages_to_text(protocol.HEADER + bytes(view.b[view.p:view.p + 24 + ((n + 7) & ~7)]))


_NO_VALUE = object()


class ViewCommand(Command):
    """A command of the binary form with the interface of Command: the operation (``reset``,
    ``increment``, ``immediately``) and the value from the record; ``processor``,
    ``full_command`` and ``command`` from the core's legacy text of the record, converted only
    when they are read (logs, the not handled list, check)."""

    def __init__(self, view):  # noqa: D107 - no dataclass __init__: the fields are properties
        self.view = view
        self._legacy = None
        op = view.h[2]
        self.immediately = op == protocol.OP_IMMEDIATELY
        self.increment = op == protocol.OP_INC
        self.reset = op == protocol.OP_RESET
        # The value of the typed kinds, read once (as the text form's: a flag as 1/0)
        self._value = _NO_VALUE
        if view.KIND in ("bool", "value_op", "none"):
            value = view.value
            self._value = int(value) if type(value) is bool else value

    @property
    def legacy(self) -> tuple:
        """(kind, processor, command, value) of the legacy text."""
        if self._legacy is None:
            lines = drivenbymoss.legacy_commands(_record_text(self.view))
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
        return Command.parse(self.processor, self.full_command, None).command

    @property
    def value(self):
        """As the text form's value: a flag as 1/0, a double, None (no value, a reset); the
        others from the legacy text."""
        return self.legacy[3] if self._value is _NO_VALUE else self._value

    @property
    def number(self) -> float:
        value = self.value
        if value is None:
            return 1.0
        if type(value) in (int, float):
            return float(value)
        return Command.number.fget(self)

    def __repr__(self) -> str:
        return f"ViewCommand({self.processor!r}, {self.full_command!r}, {self.value!r})"

TypedHandler = Callable[[Any, ViewCommand], "bool | None"]


class CommandHandler:
    """Maps the commands of the core to model operations."""

    def __init__(self, daw: SimDaw, banks: Banks, reporter: Reporter, clock: Callable[[], float] = lambda: 0.0):
        self.daw = daw
        self.banks = banks
        self.reporter = reporter
        self.clock = clock
        self.handlers: dict[str, Handler] = {}
        self.features: set[str] = set()
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
        """The typed handlers of the binary form by CmdId (process_commands)."""
        self.register_typed((Cmd.REFRESH,), lambda view, cmd: self._refresh(cmd))
        self.register_typed((*CHANNEL_SUBS, *CHANNEL_VALUES, Cmd.CHANNEL_SEND_VOLUME), self._typed_channel)
        self.register_typed(PARAM_SUBS, self._typed_param)

        self.register("refresh", self._refresh)
        self.register("bank", self._bank)
        for processor in ("play", "stop", "record", "repeat", "tempo", "time", "beats", "automation", "inifile", "undo", "redo"):
            self.register(processor, self._transport_processor)
        self.register("transport", self._transport, ("transportCommands",))
        self.register("track", self._track, ("valueInc", "valueReset", "immediately", "indication", "soloToggle"))
        self.register("fxtrack", self._fx_track)
        self.register("master", self._master)
        self.register("scene", self._scene)
        self.register("action", self._action)
        self.register("device", self._device, ("deviceCommands",))
        self.register("primary", self._specific)
        self.register("drum", self._specific)
        self.register("noterepeat", self._note_repeat)
        self.register("eq", self._equalizer)
        self.register("navigator", self._navigator, ("clipLauncherNavigator",))
        self.register("clip", self._clip)
        self.register("project", self._project)
        self.register("marker", self._marker)
        self.register("application", self._application)
        self.register("arranger", self._panel_toggle)
        self.register("mixer", self._panel_toggle)
        self.register("quantize", self._quantize)

    def configure(self) -> None:
        """Create the objects which depend on the model setup of the controller (after
        ``Banks.configure``): the clip launcher navigator (BitwigModel: only if the model setup
        has ``wantsClipLauncherNavigator``), the step grids of the note clips (``noteClips``, one
        per size, as BitwigModel)."""
        self.navigator = ClipLauncherNavigator(self.daw, self.banks) if self.banks.wants_clip_launcher_navigator else None
        self.note_grids = [NoteGrid(int(steps), int(rows), lambda: self.daw.cursor_clip) for steps, rows in self.banks.setup.get("noteClips", [])]
        self.reporter.note_grids = self.note_grids

    def register(self, processor: str, handler: Handler, features: tuple[str, ...] = ()) -> None:
        """Handle the commands of a processor; the handler implements the given host features."""
        self.handlers[processor] = handler
        self.features.update(features)

    def host_features(self) -> list[str]:
        """The host features for ``/host/features``, in the order of the core."""
        features = [name for name in FEATURE_ORDER if name in self.features]
        if not self.banks.wants_clip_launcher_navigator and "clipLauncherNavigator" in features:
            features.remove("clipLauncherNavigator")
        return features

    # Entry point (the process_* callbacks)

    def before_command(self) -> None:
        """Called before every command (both forms)."""

    def handle(self, processor: str, command: str, value: Any = None) -> None:
        self.before_command()
        cmd = Command.parse(processor, command, value)
        self.log.append(cmd)
        if not isinstance(value, (str, list)):
            self._report_again(cmd)
        handler = self.handlers.get(processor)
        try:
            handled = handler is not None and handler(cmd)
        except (IndexError, ValueError, TypeError) as error:
            logger.warning("Command %s %s %r failed: %s", processor, command, value, error)
            handled = False
        if not handled:
            self.not_handled(cmd)

    # The binary form (TYPED_PROTOCOL_PLAN §6.4)

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
    """The CmdIds the handler does not execute (later steps: the application and project
    commands, the browser, groove, the clip notes, …). The coverage test of TYPED_PROTOCOL_PLAN
    §2.5 (test_host.CommandCoverageTest) keeps the set exact."""

    def handled_commands(self) -> set[int]:
        """The CmdIds the handler executes, for App.use_binary_commands (the core logs a command
        of another ID once)."""
        return set(protocol.CMD_IDS) - self.UNHANDLED_COMMANDS

    def register_typed(self, command_ids: Iterable[int], handler: TypedHandler) -> None:
        """Handle the commands of the IDs in the binary form with a typed handler."""
        for command_id in command_ids:
            self.typed[int(command_id)] = handler

    def process_commands(self, buffer) -> None:
        """The commands of one call in the binary form (Backend.process_commands): by CmdId
        through the typed handlers; the others, and those a typed handler leaves to the text
        (FALLBACK), through their legacy text (``handle``)."""
        typed = self.typed
        for view in protocol.iter_commands(buffer):
            handler = typed.get(view.h[0])
            if handler is None or not self.handle_typed(handler, view):
                self.handle_text(view)

    def handle_typed(self, handler: TypedHandler, view) -> bool:
        """A command by its typed handler; False if the handler left it to the text."""
        cmd = ViewCommand(view)
        self.before_command()
        try:
            handled = handler(view, cmd)
        except (IndexError, ValueError, TypeError) as error:
            logger.warning("Command %s %s %r failed: %s", cmd.processor, cmd.full_command, cmd.value, error)
            handled = False
        if handled is FALLBACK:
            return False
        self.log.append(cmd)
        if getattr(self.reporter, "check", False):
            self._check_typed(cmd)
        if not handled:
            self.not_handled(cmd)
        return True

    def handle_text(self, view) -> None:
        """A command through its legacy text (the core's conversion; the transition)."""
        for kind, processor, command, value in drivenbymoss.legacy_commands(_record_text(view)):
            if kind not in ("delay", "enable"):
                self.handle(processor, command, value)

    @staticmethod
    def _check_typed(cmd: ViewCommand) -> None:
        """The dual run of the commands (DBM_HOST_PROTOCOL=check): the operation and the value
        of a typed command are the text form's."""
        _kind, processor, command, value = cmd.legacy
        text = Command.parse(processor, command, value)
        typed = (cmd.number, cmd.flag, cmd.text, cmd.reset, cmd.increment, cmd.immediately)
        expected = (text.number, text.flag, text.text, text.reset, text.increment, text.immediately)
        if typed != expected:
            raise AssertionError(f"The typed command {cmd!r} differs from its text {processor} {command} {value!r}: {typed} != {expected}")

    def _typed_channel(self, view, cmd: ViewCommand) -> "bool | None":
        """CHANNEL_MUTE, _SOLO, _SOLO_TOGGLE, _SELECT, _VOLUME, _PAN, _SEND_VOLUME,
        TRACK_REC_ARM of the master, the tracks and the effect tracks (typed_master,
        typed_track); the pads and layers through the text."""
        h = view.h
        id_ = h[0]
        name = CHANNEL_VALUES.get(id_)
        if name is not None:
            sub = [name, *OP_SUBS.get(h[2], ())]
        elif id_ == Cmd.CHANNEL_SEND_VOLUME:
            sub = ["send", str(signed(h[6])), "volume", *OP_SUBS.get(h[2], ())]
        else:
            sub = CHANNEL_SUBS[id_]
        kind = h[3]
        if kind == ChanKind.MASTER:
            handled = self.typed_master(sub, cmd)
            if handled is not FALLBACK:
                self.reporter.forget_later("/master/")
            return handled
        if kind == ChanKind.TRACK and h[4] in (Processor.TRACK, Processor.FX_TRACK):
            fx = h[4] == Processor.FX_TRACK
            position = signed(h[5])
            handled = self.typed_track(fx, position, sub, cmd)
            if handled is not FALLBACK and position >= 0:
                self.reporter.forget_later(f"/{'fxtrack' if fx else 'track'}/{position}/")
            return handled
        return FALLBACK

    def typed_master(self, sub: list[str], cmd: Command) -> "bool | None":
        """A typed channel command of the master (as ``_master``)."""
        return self.channel_command(self.daw.master, sub, cmd)

    def typed_track(self, fx: bool, position: int, sub: list[str], cmd: Command) -> "bool | None":
        """A typed channel command of a track or effect track (as ``_track``, ``_fx_track``)."""
        tracks = self.daw.effect_tracks if fx else self.track_list()
        if not 0 <= position < len(tracks):
            return False
        return self.track_command(tracks[position], position, sub, cmd, tracks)

    def _typed_param(self, view, cmd: ViewCommand) -> "bool | None":
        """PARAM_VALUE, _TOUCH, _INDICATE (typed_param)."""
        h = view.h
        processor = PARAM_PROCESSORS.get(h[3])
        if processor is None or (h[0] == Cmd.PARAM_VALUE and h[2] in OP_SUBS):
            return FALLBACK
        index = signed(h[4])
        handled = self.typed_param(processor, index, PARAM_SUBS[h[0]], cmd)
        if handled is not FALLBACK:
            self.reporter.forget_later(f"/{processor}/param/{index}/")
        return handled

    def typed_param(self, processor: str, index: int, name: str, cmd: Command) -> "bool | None":
        """``<processor> param/<index>/<name>`` (name: value, touch, indicate): the cursor
        device (``parameter_command``); the others through the text."""
        if processor != "device":
            return FALLBACK
        return self.parameter_command(self.daw.cursor_device, ["param", str(index), name], cmd, cursor=True)

    def not_handled(self, cmd: Command) -> None:
        """Log a command which is not handled, once (DbmExtension.unhandled)."""
        key = re.sub(r"\d+", "<n>", f"{cmd.processor} {cmd.full_command}".strip())
        if key not in self._unhandled_keys:
            self._unhandled_keys.add(key)
            self.unhandled.append(key)
            logger.info("Not handled yet: %s", key)

    def _report_again(self, cmd: Command) -> None:
        """CommandHandler.reportAgain: the values of the commanded item are reported again 300 ms
        later, also if they did not change (HOST_BRIDGE_GUIDE §6.1)."""
        parts = cmd.parts
        if cmd.processor == "master":
            self.reporter.forget_later("/master/")
        elif cmd.processor in ("track", "fxtrack") and parts and parse_index(parts[0]) >= 0:
            self.reporter.forget_later(f"/{cmd.processor}/{parts[0]}/")
        elif cmd.processor in ("device", "primary", "drum", "eq", "project") and len(parts) >= 2 and parts[0] in ("param", "pad", "layer"):
            self.reporter.forget_later(f"/{cmd.processor}/{parts[0]}/{parts[1]}/")

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

    def value_command(self, parameter: Parameter, sub: list[str], cmd: Command) -> bool:
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

    # Processors

    def _refresh(self, cmd: Command) -> bool:
        self.reporter.request_dump()
        return True

    def _bank(self, cmd: Command) -> bool:
        return self.banks.scroll(cmd.command, int(cmd.number))

    def _transport_processor(self, cmd: Command) -> bool:
        """The Reaper-style transport processors (HOST_BRIDGE_GUIDE §4.2): play, stop, record,
        repeat, tempo, time, beats, automation, inifile, undo, redo."""
        t = self.daw.transport
        p, c = cmd.processor, cmd.command
        if p == "undo" and c == "":
            self.daw.application.undo()
        elif p == "redo" and c == "":
            self.daw.application.redo()
        elif p == "play" and c == "":
            t.toggle_play()
        elif p == "stop" and c == "":
            t.stop()
        elif p == "record" and c == "":
            t.record()
        elif p == "repeat" and c == "":
            t.loop = not t.loop
        elif p == "tempo":
            delta = {"+": 0.01, "-": -0.01, "++": 1.0, "--": -1.0}
            before = t.tempo
            if c == "" and cmd.value is not None:
                t.set_tempo(cmd.number)
            elif c in delta:
                t.change_tempo(delta[c])
            else:
                return False
            if t.tempo != before:
                self.daw.application.edit_value("tempo", t.set_tempo, before, t.tempo)
        elif p == "time":
            if c == "signature":
                value = int(cmd.number)
                t.set_time_signature(value % 100, value // 100)
            elif c in ("", "nosnap", "loop/start", "loop/length"):
                beats = cmd.number * t.tempo / 60.0 if cmd.value is not None else 0.0
                if c == "loop/start":
                    t.loop_start = max(0.0, beats)
                elif c == "loop/length":
                    t.loop_length = max(0.0, beats)
                else:
                    t.set_position(beats)
            else:
                return False
        elif p == "beats":
            if c in ("position", "position/nosnap"):
                t.set_position(cmd.number)
            elif c == "loop/start":
                t.loop_start = max(0.0, cmd.number)
            elif c == "loop/length":
                t.loop_length = max(0.0, cmd.number)
            else:
                return False
        elif p == "automation":
            if c == "toggleArranger":
                t.automation_arranger = not t.automation_arranger
            elif c == "toggleLauncher":
                t.automation_launcher = not t.automation_launcher
            elif c == "resetOverrides":
                pass  # No automation overrides in the simulation
            elif c == "" and cmd.value is not None:
                t.set_automation_write_mode(int(cmd.number))
            else:
                return False
        elif p == "inifile":
            # Only the pre-roll length "reaper/prerollmeas <bars>" (TransportImpl.setPrerollMeasures)
            bars = int(cmd.number)
            if c != "reaper/prerollmeas" or not 0 <= bars < len(PREROLLS) or PREROLLS[bars] is None:
                return False
            t.preroll = PREROLLS[bars]
        else:
            return False
        return True

    def _transport(self, cmd: Command) -> bool:
        """``transport <command>`` (host feature transportCommands, TransportImpl)."""
        t = self.daw.transport
        c = cmd.command
        toggles = {"toggleMetronome": "metronome", "togglePrerollMetronome": "preroll_metronome", "togglePunchIn": "punch_in",
                   "togglePunchOut": "punch_out", "toggleFill": "fill", "toggleMetronomeTicks": "metronome_ticks"}
        switches = {"metronome": "metronome", "punchIn": "punch_in", "punchOut": "punch_out", "fill": "fill",
                    "metronomeTicks": "metronome_ticks", "overdub/arranger": "overdub_arranger", "overdub/launcher": "overdub_launcher"}
        actions = {"play": t.play, "stop": t.stop, "restart": t.restart, "stopAndRewind": t.stop_and_rewind}
        if c in toggles:
            setattr(t, toggles[c], not getattr(t, toggles[c]))
        elif c in switches:
            setattr(t, switches[c], cmd.flag)
        elif c in actions:
            actions[c]()
        elif c == "tapTempo":
            t.tap_tempo(self.clock())
        elif c == "metronomeVolume":
            self.set_value(t.metronome_volume, cmd)
        else:
            return False
        return True

    def _action(self, cmd: Command) -> bool:
        """``action <id>``: there are no actions in the simulation; unknown IDs are ignored, as
        the Java version."""
        return True

    # Application, arranger, mixer

    def _application(self, cmd: Command) -> bool:
        """``application layout <index>`` (ApplicationImpl.setPanelLayout; the other application
        commands act on Bitwig's UI, which is not simulated)."""
        if cmd.command == "layout" and cmd.value is not None:
            index = int(cmd.number)
            if 0 <= index < len(PANEL_LAYOUTS):
                self.daw.application.panel_layout = PANEL_LAYOUTS[index]
            return True
        return False

    def _panel_toggle(self, cmd: Command) -> bool:
        """``arranger|mixer toggle…``: the visibility flags (CommandHandler.handleArranger,
        handleMixer)."""
        target, toggles = (self.daw.arranger, ARRANGER_TOGGLES) if cmd.processor == "arranger" else (self.daw.mixer, MIXER_TOGGLES)
        attribute = toggles.get(cmd.command)
        if attribute is None:
            return False
        setattr(target, attribute, not getattr(target, attribute))
        return True

    # Markers

    def _marker(self, cmd: Command) -> bool:
        """``marker add|previous|next``, ``marker <i>/launch|select|remove`` (CommandHandler.
        handleMarker; ``select`` and ``previous|next`` set the play start position as
        TransportImpl.setPosition). Unknown without a marker bank (``numMarkers``)."""
        daw = self.daw
        markers = daw.markers
        has_bank = int(self.banks.setup.get("numMarkers", 0)) > 0
        c = cmd.command
        if c == "add":
            markers.add_at_playback_position()
        elif c in ("previous", "next"):
            marker = markers.find(c == "next")
            if marker is not None:
                daw.transport.set_position(marker.position)
        else:
            parts = cmd.parts
            index = parse_index(parts[0]) if len(parts) == 2 else -1
            if not has_bank or not 0 <= index < NUM_MARKERS or parts[1] not in ("launch", "select", "remove"):
                return False
            if index >= len(markers.items):
                return True
            marker = markers.items[index]
            if parts[1] == "launch":
                markers.launch(marker)
            elif parts[1] == "select":
                daw.transport.set_position(marker.position)
            else:
                markers.remove(marker)
        return True

    # The project

    def _project(self, cmd: Command) -> bool:
        """``project createScene``: a new empty scene at the end, selected
        (CommandHandler.handleProject, ``Project.createScene``; that Bitwig selects it is known
        only from the observer, which showed the selected slot deselected, flexi.scene.create of
        generic_flexi project). The other project commands (cue, remote controls, …) are not
        simulated."""
        if cmd.command == "createScene":
            self.daw.add_scene().select()
            return True
        return False

    # The cursor clip and its step grids

    def _clip(self, cmd: Command) -> bool:
        """``clip …``: the cursor clip (CommandHandler.handleClip of the Bitwig extension): the
        clip properties, ``grid/<id>/…`` on a step grid (``grid_command``). The commands of the
        Reaper note list (``note/<pitch>/…``, sent only without a host grid) are unknown, as
        there."""
        daw = self.daw
        clip = daw.cursor_clip
        slot = daw.cursor_clip_slot
        c = cmd.command
        parts = cmd.parts
        if c == "name":
            if clip is not None:
                clip.name = slot.name = cmd.text or ""
        elif c == "color":
            color = color_from_command(cmd.text)
            if color is not None and slot is not None:
                slot.clip_color = color
        elif c in ("start", "end", "loopStart", "loopLength"):
            if clip is not None:
                attribute = {"start": "play_start", "end": "play_stop", "loopStart": "loop_start", "loopLength": "loop_length"}[c]
                value = getattr(clip, attribute) + cmd.number if cmd.increment else cmd.number
                setattr(clip, attribute, max(0.0, value))
        elif c in ("loop", "shuffle"):
            if clip is not None:
                setattr(clip, c, cmd.flag)
        elif c == "accent":
            if clip is not None:
                self.set_value(clip.accent, cmd)
        elif c == "duplicate":
            if slot is not None:
                slot.duplicate()
        elif c == "duplicateContent":
            if clip is not None:
                clip.duplicate_content()
        elif c == "clear":
            if clip is not None:
                clip.notes.clear()
        elif c == "pinned":
            daw.set_cursor_clip_pinned(cmd.flag)
        elif c == "transpose":
            # CursorClipImpl: only on tracks with notes
            if clip is not None and slot.track.can_hold_notes:
                clip.transpose(int(cmd.number))
        elif len(parts) >= 3 and parts[0] == "grid":
            grid = next((grid for grid in self.note_grids if grid.id == parts[1]), None)
            return grid is not None and self.grid_command(grid, parts[2:], cmd)
        else:
            return False
        return True

    @staticmethod
    def grid_command(grid: NoteGrid, parts: list[str], cmd: Command) -> bool:
        """BitwigNoteClip.handle: ``stepSize``, ``scroll``, ``page/+|-``, ``clearAll``,
        ``clearRow/<ch>/<row>``, ``clearColumn/<ch>/<step>``, ``step/<ch>/<x>/<y>/toggle|set|
        setFrom|update|clear|moveY|<attribute>``."""
        name = parts[0]
        if name == "stepSize" and len(parts) == 1:
            grid.set_step_size(cmd.number)
        elif name == "scroll" and len(parts) == 1:
            grid.scroll_to(int(cmd.number))
        elif name == "page" and len(parts) == 2:
            grid.scroll_page(1 if parts[1] == "+" else -1)
        elif name == "clearAll" and len(parts) == 1:
            grid.clear_all()
        elif name in ("clearRow", "clearColumn") and len(parts) == 3:
            channel, index = parse_index(parts[1]), parse_index(parts[2])
            if name == "clearRow":
                grid.clear_row(channel, index)
            else:
                grid.clear_column(channel, index)
        elif name == "step" and len(parts) == 5:
            channel, x, y = (parse_index(part) for part in parts[1:4])
            action = parts[4]
            values = (cmd.text or "").split()
            if action == "toggle":
                grid.toggle(channel, x, y, int(cmd.number))
            elif action == "clear":
                grid.clear(channel, x, y)
            elif action == "moveY":
                grid.move_y(channel, x, y, int(cmd.number))
            elif action == "set":
                if len(values) < 2:
                    return False
                grid.set(channel, x, y, int(float(values[0])), float(values[1]))
            elif action == "setFrom":
                # CursorClipImpl.setStep (NotePosition, IStepInfo): the note, then its values
                # (Bitwig extension: 100 ms later; here at once, clip.py)
                if len(values) < 24:
                    return False
                note = grid.set(channel, x, y, int(float(values[4]) * 127), float(values[3]))
                if note is not None:
                    for attribute, index in (("velocity", 4), ("gain", 11), ("pan", 9), ("pressure", 7), ("releaseVelocity", 6), ("timbre", 8), ("transpose", 10)):
                        set_attribute(note, attribute, float(values[index]))
            elif action == "update":
                if len(values) < 24:
                    return False
                note = grid.note(channel, x, y)
                if note is not None:
                    update_from_wire(note, values)
            elif action in ATTRIBUTES:
                note = grid.note(channel, x, y)
                if note is not None:
                    set_attribute(note, action, cmd.number)
            else:
                return False
        else:
            return False
        return True

    def _quantize(self, cmd: Command) -> bool:
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

    def _navigator(self, cmd: Command) -> bool:
        """``navigator selectTrack|tracks/-|+|clips/-|+|scenes/-|+`` (CommandHandler.
        handleNavigator: unhandled without a navigator)."""
        return self.navigator is not None and self.navigator.handle(cmd.command, cmd.number)

    # Note repeat

    def _note_repeat(self, cmd: Command) -> bool:
        """``noterepeat active|rate|notelength|mode|velocity`` on the arpeggiator of the default
        note input (CommandHandler.handleNoteRepeat of the Bitwig extension). Without a note input
        the commands are ignored, as there."""
        note_input = self.daw.note_input
        if note_input is None:
            return True
        arpeggiator = note_input.arpeggiator
        c = cmd.command
        if c == "active":
            arpeggiator.enabled = cmd.flag
        elif c == "rate":
            # The core sends the inverse of the period in beats
            if cmd.number > 0:
                arpeggiator.rate = 1.0 / cmd.number
        elif c == "notelength":
            arpeggiator.gate_length = cmd.number
        elif c == "mode":
            arpeggiator.set_mode_index(int(cmd.number))
        elif c == "velocity":
            arpeggiator.use_pressure_to_velocity = cmd.flag
        else:
            return False
        return True

    # Devices

    def _device(self, cmd: Command) -> bool:
        """``device …``: the cursor device (CommandHandler.handleDevice of the Bitwig extension;
        device positions are 1-based): ``+``, ``-`` (selectNext / selectPrevious), ``page/+``,
        ``page/-`` (the sibling window), ``selected <n>`` (select the n-th sibling),
        ``parameterPage``, ``window``, ``pinned``, ``expand``, the commands of the device itself
        (``own_device_command``), ``preset`` (ignored), ``param/<n>/…``, ``layer/<n>/…`` (the
        layers of the cursor device, ``numDeviceLayers``), ``sibling/<n>/…``,
        ``<position>/bypass|remove|duplicate|movePrev|moveNext``."""
        daw = self.daw
        device = daw.cursor_device
        chain = daw.cursor_device_chain
        c = cmd.command
        parts = cmd.parts
        if c in ("+", "-"):
            if device is not None and chain is not None:
                chain.select(device)
                chain.select_neighbour(1 if c == "+" else -1)
                if daw.cursor.device_pinned:
                    daw.cursor.pinned_device = chain.selected
        elif c in ("page/+", "page/-"):
            if chain is not None:
                chain.scroll_siblings(1 if c == "page/+" else -1, NUM_SIBLINGS)
        elif c == "selected":
            index = int(cmd.number) - 1
            if chain is not None and 0 <= index < NUM_SIBLINGS and chain.sibling_offset + index < len(chain.devices):
                chain.select_index(chain.sibling_offset + index)
        elif c == "parameterPage":
            pages = device.page_names(cursor=True) if device is not None else []
            if pages:
                device.select_page(max(0, min(int(cmd.number), len(pages) - 1)), cursor=True)
        elif c in ("window", "expand"):
            if device is not None:
                setattr(device, "window_open" if c == "window" else "expanded", cmd.flag)
        elif c == "pinned":
            daw.set_cursor_device_pinned(cmd.flag)
        elif c == "preset":
            pass  # Reaper's preset browser; Bitwig reports its own browser
        elif self.own_device_command(device, c):
            pass
        elif len(parts) == 3 and parts[0] == "param":
            return self.parameter_command(device, parts, cmd, cursor=True)
        elif len(parts) == 3 and parts[0] == "layer":
            return self.layer_command(device, parts, cmd, 0, self.num_device_layers())
        elif len(parts) == 3 and parts[0] == "sibling":
            index = parse_index(parts[1]) - 1
            if not 0 <= index < NUM_SIBLINGS or parts[2] not in OWN_DEVICE_COMMANDS:
                return False
            position = (chain.sibling_offset if chain is not None else 0) + index
            sibling = chain.devices[position] if chain is not None and position < len(chain.devices) else None
            return self.own_device_command(sibling, parts[2])
        elif len(parts) == 2 and parts[1] in ("movePrev", "moveNext"):
            self.move_cursor_device(-1 if parts[1] == "movePrev" else 1)
        elif len(parts) == 2 and parts[1] in ("bypass", "remove", "duplicate"):
            position = parse_index(parts[0]) - 1
            sibling = self.find_sibling(position)
            if sibling is None:
                return False
            if parts[1] == "bypass":
                # As Reaper's DeviceProcessor: the value is the new bypass state
                sibling.enabled = cmd.number <= 0
            elif parts[1] == "remove":
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

    def _specific(self, cmd: Command) -> bool:
        """``primary …`` (the first instrument of the cursor track, if the controller asks for
        it in ``deviceIds``) and ``drum …`` (its first Drum Machine, unless ``wantsDrumDevice``
        is false), as CommandHandler.handleSpecific: ``clearMute``, ``clearSolo``, ``indicate``
        (of the drum pads), ``window``, ``expand``, the commands of the device itself,
        ``parameterPage`` and ``param/<n>/…`` (only the first instrument has pages),
        ``pad/<n>/…`` (128 pads), ``layer/<n>/…`` (32 layers of the first instrument)."""
        if cmd.processor == "primary":
            if not self.wants_device("FIRST_INSTRUMENT"):
                return True  # Bitwig extension: no device, the command is dropped
            device, num_layers, has_pages = self.daw.primary_device, NUM_LAYERS, True
        else:
            if not bool(self.banks.setup.get("wantsDrumDevice", True)):
                return True
            device, num_layers, has_pages = self.daw.drum_device, 0, False
        c = cmd.command
        parts = cmd.parts
        if c == "clearMute":
            if device is not None:
                device.clear_muted_pads()
        elif c == "clearSolo":
            if device is not None:
                device.clear_soloed_pads()
        elif c == "indicate":
            if device is not None:
                device.drum_pad_indication = cmd.flag
        elif c in ("window", "expand"):
            if device is not None:
                setattr(device, "window_open" if c == "window" else "expanded", cmd.flag)
        elif self.own_device_command(device, c):
            pass
        elif c == "parameterPage":
            if not has_pages:
                return False
            if device is not None and device.pages:
                device.select_page(max(0, min(int(cmd.number), len(device.pages) - 1)))
        elif len(parts) == 3 and parts[0] == "param":
            if not has_pages:
                return True  # No page cursors: an index beyond them is ignored
            return self.parameter_command(device, parts, cmd)
        elif len(parts) >= 3 and parts[0] in ("pad", "layer"):
            # As handleSpecific (split ("/", 3)): the rest is a channel command, e.g. solo/toggle
            return self.layer_command(device, parts, cmd, NUM_DRUM_PADS, num_layers)
        else:
            return False
        return True

    def layer_command(self, device, parts: list[str], cmd: Command, num_pads: int, num_layers: int) -> bool:
        """``pad/<n>/<command>``, ``layer/<n>/<command>``: the channel commands on a drum pad or
        layer (CommandHandler.handleSpecific); ``select`` selects in the editor. Unknown beyond
        the bank sizes; nothing happens to an item without a device."""
        index = parse_index(parts[1])
        size = num_pads if parts[0] == "pad" else num_layers
        if not 0 <= index < size:
            return False
        if device is None:
            items = []
        elif parts[0] == "pad":
            items = device.drum_pads or []
        else:
            items = device.layer_list()
        item = items[index] if index < len(items) else None
        sub = parts[2:]
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

    def _equalizer(self, cmd: Command) -> bool:
        """``eq …``: the first EQ+ of the cursor track in the layout of the Reaper EQ
        (CommandHandler.handleEqualizer of the Bitwig extension): ``add``, ``window``,
        ``expand``, the commands of the device itself, ``band/<band> <Reaper type>`` (text),
        ``param/<index>/value|touch|indicate``. Unhandled if the controller did not ask for the EQ
        (``deviceIds``).

        ``add`` inserts a new EQ+ at the end of the chain of the cursor track
        (``endOfDeviceChainInsertionPoint ().insertBitwigDevice``); the device selection stays
        (whether Bitwig selects the new device was not observed)."""
        if not self.wants_device("EQ"):
            return False
        device = self.daw.eq_device
        c = cmd.command
        parts = cmd.parts
        if c == "add":
            channel = self.daw.cursor_channel
            if channel is not None:
                channel.devices.add(eq_plus(self.daw.preferences))
        elif c in ("window", "expand"):
            if device is not None:
                setattr(device, "window_open" if c == "window" else "expanded", cmd.flag)
        elif self.own_device_command(device, c):
            pass
        elif len(parts) == 2 and parts[0] == "band" and cmd.text is not None:
            try:
                reaper_type = int(cmd.text.strip())
            except ValueError:
                return False
            if device is None:
                return True
            return device.equalizer.set_reaper_type(parse_index(parts[1]), reaper_type)
        elif len(parts) == 3 and parts[0] == "param":
            if parts[2] not in ("value", "touch", "indicate"):
                return False
            parameter = device.equalizer.parameter(parse_index(parts[1])) if device is not None else None
            if parameter is None:
                # BitwigModel: an index out of the range is unknown, a parameter of a device
                # which does not exist ignores the command
                return device is None and 0 <= parse_index(parts[1]) < 3 * EQ_NUM_BANDS
            self.value_command(parameter, [] if parts[2] == "value" else [parts[2]], cmd)
        else:
            return False
        return True

    def parameter_command(self, device, parts: list[str], cmd: Command, cursor: bool = False) -> bool:
        """``param/<index>/value|touch|indicate`` on the flat parameter list of the page cursors
        (CommandHandler.handleParameter): an index beyond the ``numParamPages`` page cursors or of
        a page the device does not have is ignored."""
        index = parse_index(parts[1])
        if parts[2] not in ("value", "touch", "indicate"):
            return False
        num_pages = max(0, int(self.banks.setup.get("numParamPages", 8)))
        parameter = device.parameter(index, self.daw.preferences, cursor) if device is not None and 0 <= index < num_pages * PARAMS_PER_PAGE else None
        if parameter is not None:
            self.value_command(parameter, [] if parts[2] == "value" else [parts[2]], cmd)
        return True

    # Tracks

    def track_list(self) -> list[Channel]:
        return self.daw.track_list(self.banks.full_flat_track_list)

    def _track(self, cmd: Command) -> bool:
        """``track <position>/<command>`` and the commands of the track bank and the cursor track."""
        daw = self.daw
        c = cmd.command
        if c == "indicate":
            self.banks.show_track_clip_launcher_feedback = cmd.flag
        elif c == "pinned":
            daw.set_cursor_pinned(cmd.flag)
        elif c == "pinned/toggle":
            daw.set_cursor_pinned(not daw.cursor.pinned)
        elif c in ("selectPrevious", "selectNext"):
            daw.select_neighbour(self.track_list(), -1 if c == "selectPrevious" else 1)
        elif c == "addTrack":
            return self._add_track(cmd)
        else:
            parts = cmd.parts
            tracks = self.track_list()
            position = parse_index(parts[0]) if parts else -1
            if not 0 <= position < len(tracks) or len(parts) < 2:
                return False
            return self.track_command(tracks[position], position, parts[1:], cmd, tracks)
        return True

    def _fx_track(self, cmd: Command) -> bool:
        """``fxtrack <position>/<command>``, ``fxtrack indicate``."""
        if cmd.command == "indicate":
            self.banks.show_fx_track_clip_launcher_feedback = cmd.flag
            return True
        parts = cmd.parts
        tracks = self.daw.effect_tracks
        position = parse_index(parts[0]) if parts else -1
        if not 0 <= position < len(tracks) or len(parts) < 2:
            return False
        return self.track_command(tracks[position], position, parts[1:], cmd, tracks)

    def _master(self, cmd: Command) -> bool:
        return self.channel_command(self.daw.master, cmd.parts, cmd)

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
            self.not_handled(Command("addTrack", "device " + device, "device " + device, None))
        return True

    def track_command(self, track: Channel, position: int, sub: list[str], cmd: Command, tracks: list[Channel]) -> bool:
        """The commands of a track of a list (TrackImpl, ChannelImpl)."""
        daw = self.daw
        name = sub[0]
        if not isinstance(track, Track):
            # The master in the full flat track list: it has no clip slots, Bitwig ignores the
            # slot commands
            if name in ("createClip", "clip"):
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
        elif name == "clip":
            return self.slot_command(track, sub[1:], cmd)
        else:
            return self.channel_command(track, sub, cmd)
        return True

    def channel_command(self, channel: Channel, sub: list[str], cmd: Command) -> bool:
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
            color = color_from_command(cmd.text)
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

    def send_command(self, track: Track, sub: list[str], cmd: Command) -> bool:
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

    def slot_command(self, track: Track, sub: list[str], cmd: Command) -> bool:
        """``clip/<slot>/<command>`` (SlotImpl)."""
        index = parse_index(sub[0]) if sub else -1
        if not 0 <= index < len(track.slots) or len(sub) != 2:
            return False
        slot = track.slots[index]
        action = sub[1]
        if action in ("launch", "launchAlt"):
            slot.launch()
        elif action in ("launchRelease", "launchReleaseAlt"):
            pass  # Releases only matter for the launch mode "gate", which is not simulated
        elif action == "record":
            slot.record()
        elif action == "select":
            slot.select()
        elif action == "remove":
            slot.delete()
        elif action == "duplicate":
            slot.duplicate()
        elif action == "color":
            color = color_from_command(cmd.text)
            if color is None:
                return False
            slot.clip_color = color
        elif action == "paste":
            source = (cmd.text or "").split("/")
            tracks = self.track_list()
            if len(source) != 2:
                return False
            track_index, slot_index = parse_index(source[0]), parse_index(source[1])
            if not 0 <= track_index < len(tracks) or not 0 <= slot_index < len(tracks[track_index].slots):
                return False
            slot.copy_from(tracks[track_index].slots[slot_index])
        else:
            return False
        return True

    def _scene(self, cmd: Command) -> bool:
        """``scene <index>/<command>``, ``scene stop|stopAlt`` (SceneImpl, SceneBankImpl)."""
        daw = self.daw
        if cmd.command in ("stop", "stopAlt"):
            daw.stop_all_clips()
            return True
        parts = cmd.parts
        index = parse_index(parts[0]) if parts else -1
        if not 0 <= index < len(daw.scenes) or len(parts) != 2:
            return False
        scene = daw.scenes[index]
        action = parts[1]
        if action in ("launch", "launchAlt"):
            scene.launch()
        elif action in ("launchRelease", "launchReleaseAlt"):
            pass  # See slot_command
        elif action == "select":
            scene.select()
        elif action == "remove":
            daw.remove_scene(scene)
        elif action == "duplicate":
            daw.duplicate_scene(scene)
        elif action == "name" and cmd.text is not None:
            scene.name = cmd.text
        elif action == "color":
            color = color_from_command(cmd.text)
            if color is None:
                return False
            scene.color = color
        else:
            return False
        return True


__all__ = ["Command", "CommandHandler", "FEATURE_ORDER", "parse_index"]
