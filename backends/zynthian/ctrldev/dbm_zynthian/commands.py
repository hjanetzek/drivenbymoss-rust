# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: executes the core's commands on zynthian
#
# Copyright (C) 2026 Hannes Janetzek
#
# ******************************************************************************
#
# This program is free software; you can redistribute it and/or
# modify it under the terms of the GNU General Public License as
# published by the Free Software Foundation; either version 2 of
# the License, or any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
# GNU General Public License for more details.
#
# For a full copy of the GNU General Public License see the LICENSE.txt file.
#
# ******************************************************************************

"""The core's commands on zynthian (doc/HOST_BRIDGE_GUIDE.md §4.1, §4.3, §4.4), in the binary
form, dispatched by ``CmdId``.

``ZynCommandHandler`` reuses the reference host's ``CommandHandler`` for the dispatch, the value
operations, the report again after a value command and the log of unhandled commands (``Not
handled yet: …``, once per command), with zynthian's handlers instead of the simulated DAW's.
Only the IDs registered here are handled (``handled_commands``, passed to the core: it derives
the command features from them, TYPED_PROTOCOL_PLAN §6.6); the transport and the clip launcher
only with zynseq. The value operation features are ``OP_FEATURES``.

Handled (phase 1): ``refresh``, ``bank``, ``track <p>/…`` and ``fxtrack <p>/…`` (select,
volume, pan, mute, solo, solo/toggle, recarm, name, send/<s>/volume), ``track
selectPrevious|selectNext``, ``master …``. On the "add chain" placeholder after the last chain
(model.py) ``select`` opens zynthian's Add Chain view (with ``browse_add_chain`` also the
browser's session on it, add_chain.py), the other track commands are ignored.

Phase 2: ``transport play|stop|restart|stopAndRewind|metronome|toggleMetronome|
metronomeVolume[/inc]|tapTempo``, ``tempo <bpm>|+|-|++|--`` (with zynseq), and ``device …``: ``+``,
``-``, ``page/+``, ``page/-``, ``selected``, ``parameterPage``, ``param/<i>/value`` (+ suffixes,
``touch``, ``indicate``), ``bypass`` and ``select`` of the device and of ``sibling/<n>``,
``<n>/bypass``; ``window``/``expand`` are accepted and ignored (zynthian has no device windows);
``primary parameterPage|param/<i>/…|bypass|select`` (the first instrument; its pads and layers
are not handled).

Recording and the clip launcher (launcher.py, with zynseq): ``record``, ``transport
overdub/launcher|overdub/arranger``, ``track <p>/createClip/<s>``, ``track <p>/clip/<s>/record|
select|launch|launchAlt|launchRelease|launchReleaseAlt|remove|duplicate|paste``, ``track
<p>/stop|stopAlt``, ``scene <s>/select|launch|launchAlt|launchRelease|launchReleaseAlt|remove|
duplicate``, ``scene stop|stopAlt``, ``project createScene|createSceneFromPlayingLauncherClips``,
``clip duplicate`` (the cursor slot).

The browser (browser.py): ``browser replace/device``, ``stop``, ``preview``, ``contentType/+|-``
(one content type), ``column/0/+|-|page/+|page/-|reset`` (the bank), ``result/+|-|page/+|page/-``
(the preset); the device browser (``add/…``, ``insertBefore|insertAfter``, ``replace/slot|drum|
primary/…``) and the columns 1..6 are not handled.
While a session opened by the host is open (add_chain.py: the Add Chain view, the engine screen),
the ``browser`` commands go to it.

Everything else is logged as not handled, e.g. ``send/<s>/active`` (zynthian's sends have no
switch), ``device remove|duplicate``, slot and scene ``name``/``color``, the Reaper-only
transport forms.
"""

import logging
from collections import deque
from dataclasses import dataclass

from dbm_host import CommandHandler, NoteGrid
from dbm_host.commands import (DEVICE_ACTIONS, DEVICE_INDEX_IDS, DEVICE_TOGGLES, DEV_SLOT_IDS, OP_SUBS, PARAM_SUBS, parse_index)
from dbm_host.reporter import NUM_SIBLINGS, M, channel_key
from drivenbymoss import protocol
from drivenbymoss.protocol import ChKind, Cmd, Processor

logger = logging.getLogger("dbm_zynthian")

# The last commands kept in ``log`` (for tests and debugging; the reference host keeps all)
LOG_SIZE = 200

# The channel commands zynthian handles (the others: no switch, no arranger, …); with zynseq
# also the clip launcher's
CHANNEL_COMMANDS = (Cmd.CHANNEL_MUTE, Cmd.CHANNEL_SOLO, Cmd.CHANNEL_SOLO_TOGGLE, Cmd.CHANNEL_SELECT, Cmd.CHANNEL_SELECT_IN_EDITOR,
                    Cmd.CHANNEL_VOLUME, Cmd.CHANNEL_PAN, Cmd.CHANNEL_SEND_VOLUME, Cmd.TRACK_NAME, Cmd.TRACK_REC_ARM)
LAUNCHER_CHANNEL_COMMANDS = (Cmd.TRACK_STOP, Cmd.TRACK_STOP_ALT, Cmd.TRACK_CREATE_CLIP)
SLOT_ACTIONS = {protocol.SlotAction.LAUNCH: "launch", protocol.SlotAction.LAUNCH_ALT: "launchAlt",
                protocol.SlotAction.LAUNCH_RELEASE: "launchRelease", protocol.SlotAction.LAUNCH_RELEASE_ALT: "launchReleaseAlt",
                protocol.SlotAction.RECORD: "record", protocol.SlotAction.REMOVE: "remove",
                protocol.SlotAction.DUPLICATE: "duplicate", protocol.SlotAction.SELECT: "select"}
TEMPO_STEPS = {protocol.TempoStep.PLUS: "+", protocol.TempoStep.MINUS: "-", protocol.TempoStep.PLUS_PLUS: "++", protocol.TempoStep.MINUS_MINUS: "--"}
DEVICE_COMMANDS = (Cmd.DEVICE_STEP, Cmd.DEVICE_PAGE_STEP, Cmd.DEVICE_SELECTED, Cmd.DEVICE_PARAMETER_PAGE, Cmd.DEVICE_WINDOW,
                   Cmd.DEVICE_EXPAND, Cmd.DEVICE_ACTION, *DEVICE_TOGGLES, Cmd.DEVICE_SIBLING, Cmd.DEVICE_INDEX_BYPASS, *PARAM_SUBS)
CLIP_COMMANDS = (Cmd.CLIP_START, Cmd.CLIP_END, Cmd.CLIP_LOOP_START, Cmd.CLIP_LOOP_LENGTH, Cmd.CLIP_LOOP, Cmd.CLIP_CLEAR,
                 Cmd.CLIP_DUPLICATE_CONTENT, Cmd.CLIP_TRANSPOSE)
GRID_COMMANDS = (Cmd.GRID_STEP_TOGGLE, Cmd.GRID_STEP_SET, Cmd.GRID_STEP_SET_FROM, Cmd.GRID_STEP_UPDATE, Cmd.GRID_STEP_CLEAR,
                 Cmd.GRID_STEP_MOVE_Y, Cmd.GRID_STEP_ATTRIBUTE, Cmd.GRID_STEP_SIZE, Cmd.GRID_CLEAR_ALL, Cmd.GRID_CLEAR_ROW,
                 Cmd.GRID_CLEAR_COLUMN, Cmd.GRID_SCROLL, Cmd.GRID_PAGE)
# The commands of the browser: the name of the command, column/result, the move
BROWSER_COMMANDS = (Cmd.BROWSER_REPLACE_DEVICE, Cmd.BROWSER_STOP, Cmd.BROWSER_PREVIEW, Cmd.BROWSER_CONTENT_TYPE, Cmd.BROWSER_COLUMN_STEP,
                    Cmd.BROWSER_COLUMN_PAGE, Cmd.BROWSER_COLUMN_RESET, Cmd.BROWSER_RESULT_STEP, Cmd.BROWSER_RESULT_PAGE)


@dataclass
class BrowserCommand:
    """A command of the browser for browser.py and add_chain.py: ``name`` (``replace/device``,
    ``stop``, ``preview``, ``contentType``, ``column``, ``result``), the column of a column
    command, the move (``+``, ``-``, ``page/+``, ``page/-``, ``reset``), the flag (stop: commit,
    preview)."""

    name: str
    column: int = -1
    what: str = ""
    flag: bool = False


def browser_command(view, cmd) -> BrowserCommand:
    """The BrowserCommand of a record of BROWSER_COMMANDS."""
    id_ = view.h[0]
    step = "+" if getattr(view, "dir", 0) == protocol.Dir.NEXT else "-"
    if id_ == Cmd.BROWSER_REPLACE_DEVICE:
        return BrowserCommand("replace/device")
    if id_ in (Cmd.BROWSER_STOP, Cmd.BROWSER_PREVIEW):
        return BrowserCommand("stop" if id_ == Cmd.BROWSER_STOP else "preview", flag=cmd.flag)
    if id_ == Cmd.BROWSER_CONTENT_TYPE:
        return BrowserCommand("contentType", what=step)
    if id_ in (Cmd.BROWSER_COLUMN_STEP, Cmd.BROWSER_COLUMN_PAGE, Cmd.BROWSER_COLUMN_RESET):
        what = "reset" if id_ == Cmd.BROWSER_COLUMN_RESET else step if id_ == Cmd.BROWSER_COLUMN_STEP else "page/" + step
        return BrowserCommand("column", view.column, what)
    return BrowserCommand("result", what=step if id_ == Cmd.BROWSER_RESULT_STEP else "page/" + step)


class ZynCommandHandler(CommandHandler):

    OP_FEATURES = ("valueInc", "valueReset", "immediately")

    UNHANDLED_COMMANDS = frozenset((
        Cmd.TRANSPORT_TOGGLE_METRONOME_TICKS, Cmd.TRANSPORT_TOGGLE_PUNCH_IN,
        Cmd.TRANSPORT_TOGGLE_PUNCH_OUT, Cmd.TRANSPORT_TOGGLE_FILL, Cmd.PLAY_TOGGLE, Cmd.STOP_TOGGLE,
        Cmd.REPEAT_TOGGLE, Cmd.TRANSPORT_METRONOME_TICKS, Cmd.TRANSPORT_TOGGLE_PREROLL_METRONOME,
        Cmd.METRO_VOL_SET, Cmd.METRO_VOL_STEP, Cmd.INI_FILE_PREROLL_MEASURES, Cmd.TRANSPORT_PUNCH_IN,
        Cmd.TRANSPORT_PUNCH_OUT, Cmd.TRANSPORT_FILL, Cmd.TRANSPORT_END, Cmd.TRANSPORT_CROSSFADE,
        Cmd.TIME_SET, Cmd.TIME_SET_INT, Cmd.TIME_SET_NO_SNAP, Cmd.TIME_LOOP_START, Cmd.TIME_LOOP_LENGTH,
        Cmd.TIME_SIGNATURE, Cmd.BEATS_POSITION, Cmd.BEATS_POSITION_NO_SNAP, Cmd.BEATS_LOOP_START,
        Cmd.BEATS_LOOP_LENGTH, Cmd.AUTOMATION_MODE, Cmd.AUTOMATION_TOGGLE_ARRANGER,
        Cmd.AUTOMATION_TOGGLE_LAUNCHER, Cmd.AUTOMATION_RESET_OVERRIDES, Cmd.QUANTIZE, Cmd.UNDO,
        Cmd.REDO, Cmd.ACTION, Cmd.PROJECT_ENGINE, Cmd.APPLICATION_LAYOUT, Cmd.APPLICATION_LAYOUT_STEP,
        Cmd.APPLICATION_TOGGLE_NOTE_EDITOR, Cmd.APPLICATION_TOGGLE_AUTOMATION_EDITOR,
        Cmd.APPLICATION_TOGGLE_DEVICES, Cmd.APPLICATION_TOGGLE_INSPECTOR, Cmd.APPLICATION_TOGGLE_MIXER,
        Cmd.APPLICATION_TOGGLE_FULL_SCREEN, Cmd.APPLICATION_TOGGLE_BROWSER, Cmd.APPLICATION_DUPLICATE,
        Cmd.APPLICATION_DELETE_SELECTION, Cmd.APPLICATION_ARROW_LEFT, Cmd.APPLICATION_ARROW_UP,
        Cmd.APPLICATION_ARROW_RIGHT, Cmd.APPLICATION_ARROW_DOWN, Cmd.APPLICATION_ZOOM_OUT,
        Cmd.APPLICATION_ZOOM_IN, Cmd.APPLICATION_DEC_TRACK_HEIGHT, Cmd.APPLICATION_INC_TRACK_HEIGHT,
        Cmd.APPLICATION_ENTER, Cmd.APPLICATION_ESCAPE, Cmd.APPLICATION_SLICE_TO_SAMPLER,
        Cmd.APPLICATION_SLICE_TO_DRUM_MACHINE, Cmd.APPLICATION_RECORD_QUANTIZATION,
        Cmd.APPLICATION_TOGGLE_RECORD_QUANTIZATION_NOTE_LENGTH, Cmd.ARRANGER_TOGGLE_CUE_MARKERS,
        Cmd.ARRANGER_TOGGLE_PLAYBACK_FOLLOW, Cmd.ARRANGER_TOGGLE_TRACK_ROW_HEIGHT,
        Cmd.ARRANGER_TOGGLE_CLIP_LAUNCHER, Cmd.ARRANGER_TOGGLE_TIME_LINE,
        Cmd.ARRANGER_TOGGLE_IO_SECTION, Cmd.ARRANGER_TOGGLE_EFFECT_TRACKS,
        Cmd.MIXER_TOGGLE_CLIP_LAUNCHER_SECTION, Cmd.MIXER_TOGGLE_CROSS_FADE_SECTION,
        Cmd.MIXER_TOGGLE_DEVICE_SECTION, Cmd.MIXER_TOGGLE_IO_SECTION, Cmd.MIXER_TOGGLE_METER_SECTION,
        Cmd.MIXER_TOGGLE_SENDS_SECTION, Cmd.PROJECT_PREVIOUS, Cmd.PROJECT_NEXT, Cmd.PROJECT_SAVE,
        Cmd.PROJECT_LOAD, Cmd.PROJECT_UNSOLO_ALL, Cmd.PROJECT_UNMUTE_ALL, Cmd.PROJECT_CUE,
        Cmd.CHANNEL_COLOR, Cmd.CHANNEL_ACTIVE, Cmd.CHANNEL_REMOVE, Cmd.CHANNEL_DUPLICATE,
        Cmd.CHANNEL_SEND_ACTIVE, Cmd.TRACK_TOGGLE_MULTI_SELECT, Cmd.TRACK_GROUP_EXPANDED,
        Cmd.TRACK_MONITOR, Cmd.TRACK_AUTO_MONITOR, Cmd.TRACK_RETURN_TO_ARRANGEMENT,
        Cmd.TRACK_IN_QUANT_RESOLUTION, Cmd.TRACK_RECORD_CLIP, Cmd.TRACK_MOVE_PREV, Cmd.TRACK_MOVE_NEXT,
        Cmd.TRACK_PIN, Cmd.TRACK_OVERDUB, Cmd.TRACK_SCROLL_TO, Cmd.TRACK_PINNED_TOGGLE,
        Cmd.TRACK_PINNED, Cmd.TRACK_ADD, Cmd.SLOT_COLOR, Cmd.SLOT_INSERT_FILE, Cmd.SCENE_NAME,
        Cmd.SCENE_COLOR, Cmd.DEVICE_INDEX_REMOVE, Cmd.DEVICE_INDEX_DUPLICATE,
        Cmd.DEVICE_INDEX_MOVE_PREV, Cmd.DEVICE_INDEX_MOVE_NEXT, Cmd.DEVICE_PINNED, Cmd.DEVICE_ADD,
        Cmd.DEVICE_PRESET, Cmd.DRUM_CLEAR_MUTE, Cmd.DRUM_CLEAR_SOLO, Cmd.EQ_ADD, Cmd.EQ_BAND_TYPE,
        Cmd.TRACK_PARAM_VALUE, Cmd.MASTER_CROSSFADER, Cmd.CLIP_PINNED, Cmd.CLIP_NAME, Cmd.CLIP_COLOR,
        Cmd.CLIP_SHUFFLE, Cmd.CLIP_ACCENT, Cmd.CLIP_NOTE_TOGGLE, Cmd.CLIP_NOTE_SET,
        Cmd.CLIP_NOTE_UPDATE, Cmd.CLIP_NOTE_CLEAR, Cmd.CLIP_NOTE_MOVE_Y, Cmd.CLIP_NOTE_CLEAR_ROW,
        Cmd.CLIP_NOTE_CLEAR_POSITION, Cmd.BROWSER_REPLACE_SLOT,
        Cmd.BROWSER_REPLACE_PAD, Cmd.BROWSER_ADD_TO_TRACK, Cmd.BROWSER_ADD_TO_MASTER,
        Cmd.BROWSER_INSERT_BEFORE, Cmd.BROWSER_INSERT_AFTER, Cmd.MARKER_ACTION, Cmd.MARKER_ADD,
        Cmd.MARKER_PREVIOUS, Cmd.MARKER_NEXT, Cmd.NAVIGATOR_STEP, Cmd.NAVIGATOR_SELECT_TRACK,
        Cmd.NOTE_REPEAT_ACTIVE, Cmd.NOTE_REPEAT_RATE, Cmd.NOTE_REPEAT_NOTE_LENGTH,
        Cmd.NOTE_REPEAT_VELOCITY, Cmd.NOTE_REPEAT_MODE, Cmd.GROOVE_ACTIVE, Cmd.GROOVE_AMOUNT,
        Cmd.GROOVE_PARAM,
    ))
    """The CmdIds zynthian does not execute with zynseq (no arranger, no automation, no undo, no
    markers, no note repeat, …). A snapshot: the coverage test of TYPED_PROTOCOL_PLAN §2.5
    (test_commands.CommandCoverageTest) keeps it and ``handled_commands`` exact; without zynseq
    the transport and the clip launcher are not handled either (``ZYNSEQ_COMMANDS``)."""

    def __init__(self, daw, banks, reporter, clock=lambda: 0.0):
        super().__init__(daw, banks, reporter, clock)
        self.log = deque(maxlen=LOG_SIZE)
        self._ignored_on_add_chain = set()

    def register_commands(self):
        """Only zynthian's handlers; the transport and the clip launcher only with zynseq."""
        r = self.register_typed
        r((Cmd.REFRESH,), lambda view, cmd: self.refresh())
        r((Cmd.BANK_SCROLL,), self._bank_scroll)
        r(CHANNEL_COMMANDS, self._channel)
        r((Cmd.BANK_INDICATE,), self._bank_indicate)
        r((Cmd.TRACK_SELECT_PREVIOUS, Cmd.TRACK_SELECT_NEXT), lambda view, cmd: self.select_neighbour(-1 if view.h[0] == Cmd.TRACK_SELECT_PREVIOUS else 1))
        r(DEVICE_COMMANDS, self._device_command)
        r(GRID_COMMANDS, self._grid)
        r(CLIP_COMMANDS, self._clip)
        r((Cmd.CLIP_DUPLICATE,), lambda view, cmd: self.duplicate_cursor_slot())
        r(BROWSER_COMMANDS, self._browser)
        transport = self.daw.transport
        if transport is not None and transport.available:
            r((Cmd.TRANSPORT_PLAY, Cmd.TRANSPORT_STOP, Cmd.TRANSPORT_STOP_AND_REWIND, Cmd.TRANSPORT_RESTART, Cmd.TRANSPORT_METRONOME,
               Cmd.TRANSPORT_TOGGLE_METRONOME, Cmd.TRANSPORT_METRONOME_VOLUME, Cmd.TRANSPORT_TAP_TEMPO, Cmd.TRANSPORT_OVERDUB_ARRANGER), self._transport)
            r((Cmd.TEMPO_STEP, Cmd.TEMPO_SET), self._tempo)
        launcher = self.daw.launcher
        if launcher is not None and launcher.available:
            r(LAUNCHER_CHANNEL_COMMANDS, self._channel)
            r((Cmd.SLOT_ACTION, Cmd.SLOT_PASTE), self._slot)
            r((Cmd.SCENE_ACTION, Cmd.SCENE_STOP_ALL, Cmd.SCENE_STOP_ALL_ALT), self._scene)
            r((Cmd.PROJECT_CREATE_SCENE, Cmd.PROJECT_CREATE_SCENE_FROM_PLAYING_LAUNCHER_CLIPS), self._project)
            if transport is not None and transport.available:
                r((Cmd.RECORD_TOGGLE,), lambda view, cmd: launcher.toggle_recording())
                r((Cmd.TRANSPORT_OVERDUB_LAUNCHER,), lambda view, cmd: launcher.set_recording(cmd.flag))

    def configure(self):
        """No clip launcher navigator; one step grid per size of the model setup's ``noteClips``
        on the cursor clip (clip.py)."""
        self.navigator = None
        self.note_grids = [NoteGrid(int(steps), int(rows), self.daw.cursor_clip) for steps, rows in self.banks.setup.get("noteClips", [])]
        self.reporter.note_grids = self.note_grids

    def before_command(self):
        # A command sees the pattern as it is now (also changes made in zynthian)
        self.daw.invalidate_clip()
        # The core relies on the reports after its commands: the next pass reads the whole model
        # (reporter.py, the two rates)
        request_full = getattr(self.reporter, "request_full", None)
        if request_full is not None:
            request_full()

    # Channels: the master, the tracks (with the "add chain" placeholder), the effect tracks

    def typed_master(self, sub, cmd):
        master = self.daw.master()
        if master is None:
            return False
        return self.channel_command(master, sub, cmd)

    def typed_track(self, fx, position, sub, cmd):
        tracks = self.daw.effect_tracks() if fx else self.daw.track_list(self.banks.full_flat_track_list)
        if not 0 <= position < len(tracks):
            return False
        if not fx and tracks[position].is_placeholder:
            return self.add_chain_command(tracks[position], position, sub, cmd)
        return self.track_command(tracks[position], sub, cmd)

    def typed_layer(self, owner, pads, index, sub, cmd):
        """No drum pads or layers in zynthian."""
        return False

    def set_value(self, parameter, cmd):
        """No undo history in zynthian."""
        self._apply_value(parameter, cmd)

    def _bank_indicate(self, view, cmd):
        """The clip launcher feedback and the indication of the drum pads: accepted (zynthian
        has neither)."""
        return view.bank in (Processor.TRACK, Processor.FX_TRACK, Processor.DRUM)

    def add_chain_command(self, track, position, sub, cmd):
        """The "add chain" placeholder (model.py): select opens zynthian's Add Chain view, the
        other commands are ignored (logged once each). The core keeps its own idea of the
        selection after a select: the selection is reported again, the active chain as
        selected."""
        if sub == ["select"]:
            if cmd.flag:
                track.select()
                self.reporter.forget(M.CHANNEL_SELECTED | channel_key(ChKind.TRACK, position))
                tracks = self.daw.track_list(self.banks.full_flat_track_list)
                selected = next((p for p, t in enumerate(tracks) if t.selected), -1)
                if selected >= 0:
                    self.reporter.forget(M.CHANNEL_SELECTED | channel_key(ChKind.TRACK, selected))
            return True
        key = "/".join("<n>" if isinstance(part, int) or part.isdigit() else part for part in sub)
        if key not in self._ignored_on_add_chain:
            self._ignored_on_add_chain.add(key)
            logger.debug("Ignored on the add chain track: %s %s", cmd.processor, cmd.full_command)
        return True

    def select_neighbour(self, direction):
        # Not onto the "add chain" placeholder: it would open the Add Chain view
        tracks = [t for t in self.daw.track_list(self.banks.full_flat_track_list) if not t.is_placeholder]
        selected = next((position for position, track in enumerate(tracks) if track.selected), -1)
        position = selected + direction
        if selected >= 0 and 0 <= position < len(tracks):
            tracks[position].select()
        return True

    def track_command(self, track, sub, cmd):
        if sub == ["recarm"]:
            track.set_arm(cmd.flag)
            return True
        if sub[0] == "send":
            return self.send_command(track, sub[1:], cmd)
        if sub[0] == "createClip" and len(sub) == 2:
            launcher = self.daw.launcher
            return launcher.create_clip(track, parse_index(sub[1]), cmd.number)
        if sub in (["stop"], ["stopAlt"]):
            return self.daw.launcher.stop_track(track, sub[0] == "stopAlt")
        return self.channel_command(track, sub, cmd)

    def channel_command(self, channel, sub, cmd):
        if not sub:
            return False
        name = sub[0]
        if sub == ["select"]:
            if cmd.flag:
                channel.select()
        elif sub == ["name"]:
            if cmd.text is None:
                return False
            channel.rename(cmd.text)
        elif not channel.has_mixer and name in ("mute", "solo", "volume", "pan"):
            logger.debug("No mixer strip for %s %s", cmd.processor, cmd.full_command)
        elif sub == ["mute"]:
            channel.set_mute(cmd.flag)
        elif sub == ["solo"]:
            channel.set_solo(cmd.flag)
        elif sub == ["solo", "toggle"]:
            channel.toggle_solo()
        elif name in ("volume", "pan"):
            return self.value_command(channel.volume if name == "volume" else channel.pan, sub[1:], cmd)
        else:
            return False
        return True

    def send_command(self, track, sub, cmd):
        """``send/<s>/volume[/touch|/indicate]``; ``send/<s>/active`` is not handled (no switch)."""
        sends = track.sends
        index = parse_index(sub[0]) if sub else -1
        if not 0 <= index < min(len(sends), self.banks.num_sends) or len(sub) < 2:
            return False
        if sub[1] == "volume":
            return self.value_command(sends[index].volume, sub[2:], cmd)
        return False

    # The clip launcher (launcher.py, with zynseq)

    def _slot(self, view, cmd):
        """``track <p>/clip/<s>/record|select|launch|launchAlt|launchRelease|launchReleaseAlt|
        remove|duplicate|paste`` (launcher.py); ignored on the "add chain" placeholder."""
        tracks = self.daw.track_list(self.banks.full_flat_track_list)
        position = view.track
        if not 0 <= position < len(tracks):
            return False
        track = tracks[position]
        phrase = view.slot
        action = "paste" if view.h[0] == Cmd.SLOT_PASTE else SLOT_ACTIONS[view.action]
        if track.is_placeholder:
            return self.add_chain_command(track, position, ["clip", phrase, action], cmd)
        launcher = self.daw.launcher
        if action == "record":
            return launcher.record(track, phrase)
        if action == "select":
            return launcher.select(track, phrase)
        if action in ("launch", "launchAlt"):
            return launcher.launch(track, phrase)
        if action in ("launchRelease", "launchReleaseAlt"):
            return 0 <= phrase < launcher.num_phrases()  # zynseq launches on press
        if action == "remove":
            return launcher.remove(track, phrase)
        if action == "duplicate":
            return launcher.duplicate(track, phrase)
        source_track, source_phrase = cmd.value
        if not 0 <= source_track < len(tracks):
            return False
        return launcher.paste(track, phrase, tracks[source_track], source_phrase)

    def _scene(self, view, cmd):
        """``scene <s>/select|launch|launchAlt|launchRelease|launchReleaseAlt|remove|duplicate``,
        ``scene stop|stopAlt`` (launcher.py)."""
        launcher = self.daw.launcher
        id_ = view.h[0]
        if id_ in (Cmd.SCENE_STOP_ALL, Cmd.SCENE_STOP_ALL_ALT):
            return launcher.stop_all(id_ == Cmd.SCENE_STOP_ALL_ALT)
        phrase = view.scene
        action = view.action
        if action == protocol.SceneAction.SELECT:
            return launcher.select_scene(phrase)
        if action in (protocol.SceneAction.LAUNCH, protocol.SceneAction.LAUNCH_ALT):
            return launcher.launch_scene(phrase)
        if action in (protocol.SceneAction.LAUNCH_RELEASE, protocol.SceneAction.LAUNCH_RELEASE_ALT):
            return 0 <= phrase < launcher.num_phrases()
        if action == protocol.SceneAction.REMOVE:
            return launcher.remove_scene(phrase)
        return launcher.duplicate_scene(phrase)

    def _project(self, view, cmd):
        """``project createScene`` (the length is ignored, as by Bitwig),
        ``createSceneFromPlayingLauncherClips`` (launcher.py)."""
        if view.h[0] == Cmd.PROJECT_CREATE_SCENE:
            return self.daw.launcher.create_scene()
        return self.daw.launcher.create_scene_from_playing()

    def duplicate_cursor_slot(self):
        """``clip duplicate``: the cursor slot's pattern into the slot below."""
        launcher = self.daw.launcher
        selected = launcher.selected_sequence() if launcher is not None and launcher.available else None
        track = next((t for t in self.daw.tracks() if t.selected), None)
        if selected is None or track is None or launcher.sequence_of(track) != selected[1]:
            return True  # No cursor clip: nothing to duplicate
        return launcher.duplicate(track, selected[0])

    # The cursor clip (clip.py)

    def _grid(self, view, cmd):
        """The grid commands (the reference host's) on the cursor clip; a step command selects
        the MIDI channel of the clip."""
        grid = self.find_grid(view)
        if grid is None:
            return False
        if view.h[0] in (Cmd.GRID_STEP_TOGGLE, Cmd.GRID_STEP_SET, Cmd.GRID_STEP_SET_FROM, Cmd.GRID_STEP_UPDATE, Cmd.GRID_STEP_CLEAR,
                         Cmd.GRID_STEP_MOVE_Y, Cmd.GRID_STEP_ATTRIBUTE, Cmd.GRID_CLEAR_ROW, Cmd.GRID_CLEAR_COLUMN):
            channel = view.channel
            if 0 <= channel < 16 and channel != self.daw.clip_channel:
                self.daw.clip_channel = channel
                self.daw.invalidate_clip()
        return self.grid_command(grid, view, cmd)

    def _clip(self, view, cmd):
        """``clip …`` on the cursor clip (clip.py): ``loopLength``/``end`` (the pattern's beats,
        ``/inc`` relative), ``start`` and ``loopStart`` only 0, ``loop`` only on, ``clear``,
        ``duplicateContent``, ``transpose``. A zynseq pattern starts at 0: a range with another
        start (the Launchpad's Clip Length view sends ``loopStart``, ``loopLength``, ``end``,
        ``start`` for it) is not applied, also not its length and end, until the next report.
        Not handled: ``name``, ``color``, ``shuffle``, ``accent``, ``pinned``."""
        id_ = view.h[0]
        if cmd.op in OP_SUBS:
            return False  # No touch or indication of the clip range
        clip = self.daw.cursor_clip()
        if clip is None:
            return True
        if id_ in (Cmd.CLIP_LOOP_LENGTH, Cmd.CLIP_END):
            if self.daw.clip_start_request is not None:
                return False
            clip.set_length(clip.beats + cmd.number if cmd.increment else cmd.number)
        elif id_ in (Cmd.CLIP_START, Cmd.CLIP_LOOP_START):
            if cmd.number == 0 and not cmd.increment:
                return True
            self.daw.clip_start_request = cmd.number
            return False
        elif id_ == Cmd.CLIP_LOOP:
            return cmd.flag
        elif id_ == Cmd.CLIP_CLEAR:
            clip.notes.clear()
        elif id_ == Cmd.CLIP_DUPLICATE_CONTENT:
            clip.duplicate_content()
        else:
            clip.transpose(int(cmd.number))
        return True

    # The browser (browser.py)

    def _browser(self, view, cmd):
        """``browser …`` on the cursor device's banks and presets (browser.py)."""
        browser = getattr(self.daw, "browser", None)
        if browser is None:
            return False
        try:
            return browser.command(browser_command(view, cmd), *self.reporter.browser_pages())
        except Exception as e:
            # A processor in an unexpected state must not stop the tick thread
            logger.exception("browser %s failed: %s", cmd.full_command, e)
            return False

    # Transport (phase 2, with zynseq)

    def _transport(self, view, cmd):
        transport = self.daw.transport
        id_ = view.h[0]
        if id_ == Cmd.TRANSPORT_PLAY:
            transport.play()
        elif id_ in (Cmd.TRANSPORT_STOP, Cmd.TRANSPORT_STOP_AND_REWIND):
            transport.stop()
        elif id_ == Cmd.TRANSPORT_RESTART:
            transport.restart()
        elif id_ == Cmd.TRANSPORT_METRONOME:
            transport.set_metronome(cmd.flag)
        elif id_ == Cmd.TRANSPORT_TOGGLE_METRONOME:
            transport.set_metronome(not transport.metronome)
        elif id_ == Cmd.TRANSPORT_METRONOME_VOLUME:
            if cmd.op in OP_SUBS:
                return False
            if cmd.increment:
                transport.set_metronome_volume(transport.metronome_volume + cmd.number)
            elif cmd.reset:
                transport.set_metronome_volume(0.8)
            else:
                transport.set_metronome_volume(cmd.number)
        elif id_ == Cmd.TRANSPORT_TAP_TEMPO:
            transport.tap_tempo()
        else:
            pass  # overdub/arranger: no arranger in zynthian (reported 0)
        return True

    def _tempo(self, view, cmd):
        transport = self.daw.transport
        if view.h[0] == Cmd.TEMPO_STEP:
            transport.change_tempo(TEMPO_STEPS[view.step])
        else:
            transport.set_tempo(cmd.number)
        return True

    # Devices (phase 2)

    def _device_command(self, view, cmd):
        """``device …`` (the cursor device) and ``primary …`` (the first instrument; its pads and
        layers do not exist); the other devices are not handled."""
        id_ = view.h[0]
        processor = view.h[3] if id_ in DEV_SLOT_IDS else Processor.DEVICE
        if processor not in (Processor.DEVICE, Processor.PRIMARY):
            return False
        chain = self.daw.devices
        device = chain.cursor() if processor == Processor.DEVICE else chain.primary()
        if id_ == Cmd.DEVICE_PARAMETER_PAGE:
            if device is not None:
                device.select_page(int(cmd.number))
        elif id_ in (Cmd.DEVICE_WINDOW, Cmd.DEVICE_EXPAND, *DEVICE_TOGGLES):
            pass  # No device windows in zynthian
        elif id_ == Cmd.DEVICE_ACTION:
            return self.own_device_command(device, DEVICE_ACTIONS[view.action])
        elif id_ in PARAM_SUBS:
            if cmd.op in OP_SUBS:
                return False
            index = view.param
            if device is None or index < 0:
                return True
            name = PARAM_SUBS[id_]
            return self.value_command(device.parameter(index), [] if name == "value" else [name], cmd)
        elif processor != Processor.DEVICE:
            return False
        elif id_ == Cmd.DEVICE_STEP:
            chain.select_neighbour(1 if view.dir == protocol.Dir.NEXT else -1)
        elif id_ == Cmd.DEVICE_PAGE_STEP:
            chain.scroll_siblings(1 if view.dir == protocol.Dir.NEXT else -1, NUM_SIBLINGS)
        elif id_ == Cmd.DEVICE_SELECTED:
            index = chain.sibling_offset + int(cmd.number)
            devices = chain.devices()
            if 0 <= index < len(devices):
                chain.select(devices[index])
        elif id_ == Cmd.DEVICE_SIBLING:
            index = view.sibling
            devices = chain.devices()
            position = chain.sibling_offset + index
            if not 0 <= index < NUM_SIBLINGS:
                return False
            return self.own_device_command(devices[position] if position < len(devices) else None, DEVICE_ACTIONS[view.action])
        elif id_ in DEVICE_INDEX_IDS and id_ == Cmd.DEVICE_INDEX_BYPASS:
            # The Reaper path: the value is the new bypass state
            devices = chain.devices()
            position = view.device
            if not 0 <= position < len(devices):
                return False
            devices[position].set_enabled(cmd.number <= 0)
        else:
            return False
        return True

    def own_device_command(self, device, command):
        if command not in ("bypass", "select"):
            return False
        if device is None:
            return True
        if command == "bypass":
            device.toggle_bypass()
        else:
            self.daw.devices.select(device)
        return True
