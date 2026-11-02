# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Executes the commands of the core (processor, command, value) on the simulated DAW.

Implements HOST_BRIDGE_GUIDE.md §4 (the command reference, core -> host): 4.1 the value command
suffixes, 4.2 transport, the ``refresh`` and ``bank`` commands of 4.3, 4.4 tracks and master
(and the effect tracks of §3.3), 4.5 slots and scenes, ``parameterPage`` of 4.6; the processors as BACKEND_PROTOCOL.md §2.

Simulates the command handling of the Bitwig extension (``CommandHandler``, ``DbmExtension.
unhandled``), which calls the Bitwig API as the Java DrivenByMoss for Bitwig does
(``TransportImpl``, ``ChannelImpl``, ``TrackImpl``, ``SendImpl``, ``SlotImpl``, ``SceneImpl``,
``CursorTrackImpl``, ``AbstractTrackBankImpl``).

Dispatch: one handler per processor (``CommandHandler.register``). A handler gets a parsed
``Command`` and returns False if it does not know the command; the command is then logged once
as ``Not handled yet: <processor> <command>`` (digits replaced by ``<n>``, as the Bitwig
extension) and kept in ``unhandled``. Each handler declares the host features
(HOST_BRIDGE_GUIDE.md §2) it implements; ``host_features`` is reported as ``/host/features``.
The later steps extend ``device`` and ``primary`` and register the processors ``drum``, ``eq``,
``clip``, ``marker``, ``browser``, ``application``, ``arranger``, ``mixer``, ``project``,
``groove``, ``noterepeat``, ``navigator``, ``undo``, ``redo``, ... the same way.
"""

import logging
import re
from dataclasses import dataclass
from typing import Any, Callable

from .banks import Banks
from .model import PREROLLS, Channel, SimDaw, Track, color_from_command
from .parameter import Parameter
from .reporter import Reporter

logger = logging.getLogger("dbm_host")

# The order of HostFeature::ALL in src/daw/context.rs
FEATURE_ORDER = ("valueInc", "valueReset", "immediately", "indication", "soloToggle", "transportCommands",
                 "deviceCommands", "layoutStep", "clipLauncherNavigator")

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
        self.register("device", self._device)
        self.register("primary", self._device)

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

    def handle(self, processor: str, command: str, value: Any = None) -> None:
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

    # Values

    def set_value(self, parameter: Parameter, cmd: Command) -> None:
        """Set, reset, change (``/inc``) or set immediately, as the command says; a change is an
        edit of the project."""
        before = parameter.value
        self._apply_value(parameter, cmd)
        if parameter.value != before:
            self.daw.application.edit()

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
                self.daw.application.edit()
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

    # Devices

    def _device(self, cmd: Command) -> bool:
        """``device|primary parameterPage <page>``: show the page in the DAW (CommandHandler.
        handleDevice / handleSpecific: ``selectedPageIndex.set``, clamped). The other device
        commands come with step 4 (PYTHON_HOST_PLAN.md §5)."""
        device = self.daw.cursor_device if cmd.processor == "device" else self.daw.primary_device
        if cmd.command == "parameterPage":
            if device is not None and device.pages:
                device.select_page(max(0, min(int(cmd.number), len(device.pages) - 1)))
            return True
        return False

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
