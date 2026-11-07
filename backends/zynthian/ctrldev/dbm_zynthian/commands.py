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

"""The core's commands on zynthian (doc/HOST_BRIDGE_GUIDE.md §4.1, §4.3, §4.4).

``ZynCommandHandler`` reuses the reference host's ``CommandHandler`` for the parsing of the
commands (the value suffixes ``/inc``, ``/reset``, ``/immediately``), the dispatch per
processor, the report again after a value command and the log of unhandled commands
(``Not handled yet: …``, once per command), with zynthian's handlers instead of the simulated
DAW's. The host features are those of the registered handlers.

Handled (phase 1): ``refresh``, ``bank``, ``track <p>/…`` and ``fxtrack <p>/…`` (select,
volume, pan, mute, solo, solo/toggle, recarm, name, send/<s>/volume), ``track
selectPrevious|selectNext``, ``master …``.

Phase 2: ``transport play|stop|restart|stopAndRewind|metronome|toggleMetronome|
metronomeVolume[/inc]|tapTempo``, ``tempo <bpm>|+|-|++|--`` (with zynseq), and ``device …``: ``+``,
``-``, ``page/+``, ``page/-``, ``selected``, ``parameterPage``, ``param/<i>/value`` (+ suffixes,
``touch``, ``indicate``), ``bypass`` and ``select`` of the device and of ``sibling/<n>``,
``<n>/bypass``; ``window``/``expand`` are accepted and ignored (zynthian has no device windows);
``primary parameterPage|param/<i>/…|bypass|select`` (the first instrument; its pads and layers
are not handled).

Recording and the clip launcher (launcher.py): ``record``, ``transport overdub/launcher|
overdub/arranger``, ``track <p>/createClip/<s>``, ``track <p>/clip/<s>/record|select|launch|
launchAlt|launchRelease|launchReleaseAlt|remove|duplicate|paste``, ``track <p>/stop|stopAlt``,
``scene <s>/select|launch|launchAlt|launchRelease|launchReleaseAlt|remove|duplicate``, ``scene
stop|stopAlt``, ``project createScene|createSceneFromPlayingLauncherClips``, ``clip duplicate``
(the cursor slot).

The browser (browser.py): ``browser replace/device``, ``stop``, ``preview``, ``contentType/+|-``
(one content type), ``column/0/+|-|page/+|page/-|reset`` (the bank), ``result/+|-|page/+|page/-``
(the preset); the device browser (``add/…``, ``insertBefore|insertAfter``, ``replace/slot|drum|
primary/…``) and the columns 1..6 are not handled.

Everything else is logged as not handled, e.g. ``send/<s>/active`` (zynthian's sends have no
switch), ``device remove|duplicate``, slot and scene ``name``/``color``, the Reaper-only
transport forms.
"""

import logging
from collections import deque

from dbm_host import CommandHandler, NoteGrid
from dbm_host.commands import parse_index
from dbm_host.reporter import NUM_SIBLINGS

logger = logging.getLogger("dbm_zynthian")

# The last commands kept in ``log`` (for tests and debugging; the reference host keeps all)
LOG_SIZE = 200

TRACK_FEATURES = ("valueInc", "valueReset", "immediately", "soloToggle")
TRANSPORT_FEATURES = ("transportCommands",)
DEVICE_FEATURES = ("deviceCommands",)


class ZynCommandHandler(CommandHandler):

    def __init__(self, daw, banks, reporter, clock=lambda: 0.0):
        super().__init__(daw, banks, reporter, clock)
        # Only zynthian's handlers and their features
        self.handlers = {}
        self.features = set()
        self.log = deque(maxlen=LOG_SIZE)
        self.register("refresh", self._refresh)
        self.register("bank", self._bank)
        self.register("track", self._track, TRACK_FEATURES)
        self.register("fxtrack", self._fx_track)
        self.register("master", self._master)
        self.register("device", self._device, DEVICE_FEATURES)
        self.register("primary", self._primary)
        self.register("scene", self._scene)
        self.register("drum", self._drum)
        self.register("clip", self._clip)
        self.register("project", self._project)
        self.register("browser", self._browser)
        if self.daw.transport is not None and self.daw.transport.available:
            self.register("transport", self._transport, TRANSPORT_FEATURES)
            self.register("tempo", self._tempo)
            self.register("record", self._record)

    def configure(self):
        """No clip launcher navigator; one step grid per size of the model setup's ``noteClips``
        on the cursor clip (clip.py)."""
        self.navigator = None
        self.note_grids = [NoteGrid(int(steps), int(rows), self.daw.cursor_clip) for steps, rows in self.banks.setup.get("noteClips", [])]
        self.reporter.note_grids = self.note_grids

    def handle(self, processor, command, value=None):
        # A command sees the pattern as it is now (also changes made in zynthian)
        self.daw.invalidate_clip()
        super().handle(processor, command, value)

    def set_value(self, parameter, cmd):
        """No undo history in zynthian."""
        self._apply_value(parameter, cmd)

    # Tracks

    def _track(self, cmd):
        c = cmd.command
        if c == "indicate":
            return True  # No clip launcher (phase 3)
        if c in ("selectPrevious", "selectNext"):
            self.select_neighbour(-1 if c == "selectPrevious" else 1)
            return True
        parts = cmd.parts
        tracks = self.daw.track_list(self.banks.full_flat_track_list)
        position = parse_index(parts[0]) if parts else -1
        if not 0 <= position < len(tracks) or len(parts) < 2:
            return False
        return self.track_command(tracks[position], parts[1:], cmd)

    def _fx_track(self, cmd):
        if cmd.command == "indicate":
            return True
        parts = cmd.parts
        tracks = self.daw.effect_tracks()
        position = parse_index(parts[0]) if parts else -1
        if not 0 <= position < len(tracks) or len(parts) < 2:
            return False
        return self.track_command(tracks[position], parts[1:], cmd)

    def _master(self, cmd):
        master = self.daw.master()
        if master is None:
            return False
        return self.channel_command(master, cmd.parts, cmd)

    def select_neighbour(self, direction):
        tracks = self.daw.track_list(self.banks.full_flat_track_list)
        selected = next((position for position, track in enumerate(tracks) if track.selected), -1)
        position = selected + direction
        if selected >= 0 and 0 <= position < len(tracks):
            tracks[position].select()

    def track_command(self, track, sub, cmd):
        if sub == ["recarm"]:
            track.set_arm(cmd.flag)
            return True
        if sub[0] == "send":
            return self.send_command(track, sub[1:], cmd)
        if sub[0] in ("createClip", "clip"):
            return self.slot_command(track, sub, cmd)
        if sub in (["stop"], ["stopAlt"]):
            launcher = self.daw.launcher
            return launcher is not None and launcher.available and launcher.stop_track(track, sub[0] == "stopAlt")
        return self.channel_command(track, sub, cmd)

    def slot_command(self, track, sub, cmd):
        """``createClip/<s>``, ``clip/<s>/record|select|launch|remove|duplicate|paste``
        (launcher.py)."""
        launcher = self.daw.launcher
        if launcher is None or not launcher.available or len(sub) < 2:
            return False
        phrase = parse_index(sub[1])
        if sub[0] == "createClip" and len(sub) == 2:
            return launcher.create_clip(track, phrase, cmd.number)
        if sub[0] == "clip" and len(sub) == 3:
            if sub[2] == "record":
                return launcher.record(track, phrase)
            if sub[2] == "select":
                return launcher.select(track, phrase)
            if sub[2] in ("launch", "launchAlt"):
                return launcher.launch(track, phrase)
            if sub[2] in ("launchRelease", "launchReleaseAlt"):
                return 0 <= phrase < launcher.num_phrases()  # zynseq launches on press
            if sub[2] == "remove":
                return launcher.remove(track, phrase)
            if sub[2] == "duplicate":
                return launcher.duplicate(track, phrase)
            if sub[2] == "paste":
                source = (cmd.text or "").split("/")
                tracks = self.daw.track_list(self.banks.full_flat_track_list)
                if len(source) != 2 or not 0 <= parse_index(source[0]) < len(tracks):
                    return False
                return launcher.paste(track, phrase, tracks[parse_index(source[0])], parse_index(source[1]))
        return False

    def _clip(self, cmd):
        """``clip …`` on the cursor clip (clip.py): ``grid/<id>/…`` (the reference host's grid
        commands), ``loopLength``/``end`` (the pattern's beats, ``/inc`` relative), ``start`` and
        ``loopStart`` only 0, ``loop`` only on, ``clear``, ``duplicateContent``, ``transpose``.
        A zynseq pattern starts at 0: a range with another start (the Launchpad's Clip Length
        view sends ``loopStart``, ``loopLength``, ``end``, ``start`` for it) is not applied, also
        not its length and end, until the next report.
        Not handled: ``duplicate``, ``name``, ``color``, ``shuffle``, ``accent``, ``pinned``."""
        parts = cmd.parts
        c = cmd.command
        if len(parts) >= 3 and parts[0] == "grid":
            grid = next((grid for grid in self.note_grids if grid.id == parts[1]), None)
            if grid is None:
                return False
            if len(parts) >= 4 and parts[2] in ("step", "clearRow", "clearColumn"):
                channel = parse_index(parts[3])
                if 0 <= channel < 16 and channel != self.daw.clip_channel:
                    self.daw.clip_channel = channel
                    self.daw.invalidate_clip()
            return self.grid_command(grid, parts[2:], cmd)
        if c == "duplicate":
            return self.duplicate_cursor_slot()
        clip = self.daw.cursor_clip()
        if clip is None:
            return c in ("loopLength", "end", "start", "loopStart", "loop", "clear", "duplicateContent", "transpose")
        if c in ("loopLength", "end"):
            if self.daw.clip_start_request is not None:
                return False
            clip.set_length(clip.beats + cmd.number if cmd.increment else cmd.number)
        elif c in ("start", "loopStart"):
            if cmd.number == 0 and not cmd.increment:
                return True
            self.daw.clip_start_request = cmd.number
            return False
        elif c == "loop":
            return cmd.flag
        elif c == "clear":
            clip.notes.clear()
        elif c == "duplicateContent":
            clip.duplicate_content()
        elif c == "transpose":
            clip.transpose(int(cmd.number))
        else:
            return False
        return True

    def _drum(self, cmd):
        """``drum …``: zynthian has no drum machine (``/drum`` is not reported); ``indicate`` is
        accepted (no indication in zynthian), the rest is not handled."""
        return cmd.command == "indicate"

    def _scene(self, cmd):
        """``scene <s>/select|launch|launchAlt|launchRelease|launchReleaseAlt|remove|duplicate``,
        ``scene stop|stopAlt`` (launcher.py)."""
        launcher = self.daw.launcher
        parts = cmd.parts
        if launcher is None or not launcher.available:
            return False
        if cmd.command in ("stop", "stopAlt"):
            return launcher.stop_all(cmd.command == "stopAlt")
        if len(parts) != 2:
            return False
        phrase = parse_index(parts[0])
        if parts[1] == "select":
            return launcher.select_scene(phrase)
        if parts[1] in ("launch", "launchAlt"):
            return launcher.launch_scene(phrase)
        if parts[1] in ("launchRelease", "launchReleaseAlt"):
            return 0 <= phrase < launcher.num_phrases()
        if parts[1] == "remove":
            return launcher.remove_scene(phrase)
        if parts[1] == "duplicate":
            return launcher.duplicate_scene(phrase)
        return False

    def _project(self, cmd):
        """``project createScene`` (the length is ignored, as by Bitwig),
        ``createSceneFromPlayingLauncherClips`` (launcher.py)."""
        launcher = self.daw.launcher
        if launcher is None or not launcher.available:
            return False
        if cmd.command == "createScene":
            return launcher.create_scene()
        if cmd.command == "createSceneFromPlayingLauncherClips":
            return launcher.create_scene_from_playing()
        return False

    def _browser(self, cmd):
        """``browser …`` on the cursor device's banks and presets (browser.py)."""
        browser = getattr(self.daw, "browser", None)
        if browser is None:
            return False
        try:
            return browser.command(cmd, *self.reporter.browser_pages())
        except Exception as e:
            # A processor in an unexpected state must not stop the tick thread
            logger.exception("browser %s failed: %s", cmd.full_command, e)
            return False

    def duplicate_cursor_slot(self):
        """``clip duplicate``: the cursor slot's pattern into the slot below."""
        launcher = self.daw.launcher
        selected = launcher.selected_sequence() if launcher is not None and launcher.available else None
        track = next((t for t in self.daw.tracks() if t.selected), None)
        if selected is None or track is None or launcher.sequence_of(track) != selected[1]:
            return True  # No cursor clip: nothing to duplicate
        return launcher.duplicate(track, selected[0])

    def _record(self, cmd):
        """``record``: MIDI record on/off into the cursor slot's pattern."""
        launcher = self.daw.launcher
        if launcher is None or cmd.command:
            return False
        return launcher.toggle_recording()

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

    # Transport (phase 2)

    def _transport(self, cmd):
        transport = self.daw.transport
        c = cmd.command
        if c == "play":
            transport.play()
        elif c in ("stop", "stopAndRewind"):
            transport.stop()
        elif c == "restart":
            transport.restart()
        elif c == "metronome":
            transport.set_metronome(cmd.flag)
        elif c == "toggleMetronome":
            transport.set_metronome(not transport.metronome)
        elif c == "metronomeVolume":
            if cmd.increment:
                transport.set_metronome_volume(transport.metronome_volume + cmd.number)
            elif cmd.reset:
                transport.set_metronome_volume(0.8)
            else:
                transport.set_metronome_volume(cmd.number)
        elif c == "tapTempo":
            transport.tap_tempo()
        elif c == "overdub/launcher" and self.daw.launcher is not None:
            return self.daw.launcher.set_recording(cmd.flag)
        elif c == "overdub/arranger":
            pass  # No arranger in zynthian (reported 0)
        else:
            return False
        return True

    def _tempo(self, cmd):
        transport = self.daw.transport
        if cmd.command in ("+", "-", "++", "--"):
            transport.change_tempo(cmd.command)
        elif cmd.command == "" and cmd.value is not None:
            transport.set_tempo(cmd.number)
        else:
            return False
        return True

    # Devices (phase 2)

    def _device(self, cmd):
        chain = self.daw.devices
        device = chain.cursor()
        c = cmd.command
        parts = cmd.parts
        if c in ("+", "-"):
            chain.select_neighbour(1 if c == "+" else -1)
        elif c in ("page/+", "page/-"):
            chain.scroll_siblings(1 if c == "page/+" else -1, NUM_SIBLINGS)
        elif c == "selected":
            index = chain.sibling_offset + int(cmd.number) - 1
            devices = chain.devices()
            if 0 <= index < len(devices):
                chain.select(devices[index])
        elif c == "parameterPage":
            if device is not None:
                device.select_page(int(cmd.number))
        elif c in ("window", "expand", "window/toggle", "expand/toggle", "parameters/toggle"):
            pass  # No device windows in zynthian
        elif c in ("bypass", "select"):
            return self.own_device_command(device, c)
        elif len(parts) == 3 and parts[0] == "param":
            index = parse_index(parts[1])
            if device is None or index < 0:
                return True
            sub = [] if parts[2] == "value" else [parts[2]]
            return self.value_command(device.parameter(index), sub, cmd)
        elif len(parts) == 3 and parts[0] == "sibling":
            index = parse_index(parts[1]) - 1
            devices = chain.devices()
            position = chain.sibling_offset + index
            if not 0 <= index < NUM_SIBLINGS or parts[2] not in ("bypass", "select"):
                return False
            return self.own_device_command(devices[position] if position < len(devices) else None, parts[2])
        elif len(parts) == 2 and parts[1] == "bypass":
            # The Reaper path: the value is the new bypass state
            devices = chain.devices()
            position = parse_index(parts[0]) - 1
            if not 0 <= position < len(devices):
                return False
            devices[position].set_enabled(cmd.number <= 0)
        else:
            return False
        return True

    def _primary(self, cmd):
        """``primary …``: the first instrument (``parameterPage``, ``param/<i>/…``, ``bypass``,
        ``select``; ``window``/``expand`` ignored). Its pads and layers do not exist."""
        device = self.daw.devices.primary()
        c = cmd.command
        parts = cmd.parts
        if c == "parameterPage":
            if device is not None:
                device.select_page(int(cmd.number))
        elif c in ("window", "expand", "window/toggle", "expand/toggle", "parameters/toggle"):
            pass
        elif c in ("bypass", "select"):
            return self.own_device_command(device, c)
        elif len(parts) == 3 and parts[0] == "param":
            index = parse_index(parts[1])
            if device is None or index < 0:
                return True
            return self.value_command(device.parameter(index), [] if parts[2] == "value" else [parts[2]], cmd)
        else:
            return False
        return True

    def own_device_command(self, device, command):
        if device is None:
            return True
        if command == "bypass":
            device.toggle_bypass()
        elif command == "select":
            self.daw.devices.select(device)
        else:
            return False
        return True
