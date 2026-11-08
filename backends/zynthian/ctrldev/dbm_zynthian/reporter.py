# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: reports zynthian's state to the core
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

"""The reports of zynthian's model to the core (doc/HOST_BRIDGE_GUIDE.md §2, §3.3).

``ZynReporter`` reuses the change tracking of the reference host's ``Reporter`` (the last text
per address, dumps after ``refresh``, the report again 300 ms after a value command) with its
own collectors: the host features, the tracks, the effect tracks and the master. All items are
reported with their absolute positions (§6 rule 4); ``/track/offset`` turns on the core's host
paging, ``/host/features`` lists the command forms of ``ZynCommandHandler``.

Phase 2 adds the transport (``collect_transport``, only with zynseq), the cursor device with
its siblings and parameter pages (``collect_device``) and the first instrument
(``collect_primary``).

The browser (``collect_browser``, browser.py): the banks and presets of the cursor device,
``/browser/exists 0`` while it is closed (with it the core sends ``browser …`` commands).

Two rates (doc/ZYNTHIAN_CPU.md option 2): a full pass reads the whole model, a fast pass only
what changes by itself (``fast_collectors``: the transport, the peak meters, the playing notes of
the cursor track, the playing steps of the step grids). A pass is full after ``request_full`` (a
command of the core, a change signal of zynthian: bridge.py), with a dump, when a "report again"
of a value command is due, and else every ``SLOW_INTERVAL_MS``; the others are fast.

The slow part is split into subtrees (``SUBTREES``), each callable on its own
(``collect_subtrees``); ``request_full`` records the subtrees a change affects (``dirty``). For
now a full pass collects all of them; the next step collects only the dirty ones, with the full
pass as the slow safety net (doc/ZYNTHIAN_CPU.md, "Next").

Not reported, so the core keeps its defaults for them: markers, ``/drum``, ``/eq``, the layers and drum pads of ``/primary``, the arranger position and loop, ``/track/<p>/active``
(zynthian has no deactivated chains), ``/track/hasPrevious|hasNext``. ``/track/pinned`` (-1) and ``/device/pinned`` (0) are reported:
zynthian has no pinning.
"""

import logging
import math

from dbm_host import Reporter
from dbm_host.reporter import NUM_DRUM_PADS, NUM_SIBLINGS, M, channel_key, grid_key
from drivenbymoss.protocol import ChKind, Dev, PadKind

from zyngine.ctrldev.dbm_zynthian.device import PARAMS_PER_PAGE
from zyngine.ctrldev.dbm_zynthian.launcher import slot_states

# The interval of the full pass without a change signal (the values zynthian signals no change
# for: the sends, the parameters of the devices, names of sequences, ...)
SLOW_INTERVAL_MS = 250.0

# The subtrees of the slow part, each collected by its collectors (``collector_subtrees``):
# the host features and the project, the track list (tracks, effect tracks, master: positions,
# names, types, selection; the peak meters), their mixer values and sends, the slots and scenes,
# the transport, the cursor device with its pages and the first instrument, the cursor clip with
# its step grids, the browser
SUBTREES = ("host", "tracks", "mixer", "slots", "transport", "device", "clip", "browser")
ALL_SUBTREES = frozenset(SUBTREES)

# The values of a slot, in the order of collect_slots
SLOT_VALUES = (M.SLOT_EXISTS, M.SLOT_HAS_CONTENT, M.SLOT_NAME, M.SLOT_COLOR, M.SLOT_SELECTED, M.SLOT_PLAYING,
               M.SLOT_PLAY_QUEUED, M.SLOT_STOP_QUEUED, M.SLOT_RECORDING, M.SLOT_REC_QUEUED)

# The values of a parameter, in the order of collect_pages
PARAM_VALUES = (M.PARAM_NAME, M.PARAM_VALUE, M.PARAM_VALUE_STR, M.PARAM_STEPS, M.PARAM_MODULATED)

# The banks of channel_base and collect_removed: the channel kind (None: the scenes)
BANKS = {"track": ChKind.TRACK, "fxtrack": ChKind.EFFECT, "scene": None}


class ZynReporter(Reporter):

    def __init__(self, daw, banks, clock=lambda: 0.0, features=lambda: []):
        super().__init__(daw, banks, clock, features)
        self.collectors = [
            Reporter.collect_host_features,
            ZynReporter.collect_tracks,
            ZynReporter.collect_effect_tracks,
            ZynReporter.collect_scenes,
            ZynReporter.collect_master,
            ZynReporter.collect_project,
            ZynReporter.collect_transport,
            ZynReporter.collect_device,
            ZynReporter.collect_primary,
            ZynReporter.collect_clip,
            ZynReporter.collect_browser,
            Reporter.collect_grids,
        ]
        # The subtrees of each collector (collect_subtrees): the track collectors run for any of
        # theirs and check "mixer" and "slots" for those parts
        track_subtrees = frozenset(("tracks", "mixer", "slots"))
        self.collector_subtrees = {
            Reporter.collect_host_features: {"host"},
            ZynReporter.collect_tracks: track_subtrees,
            ZynReporter.collect_effect_tracks: track_subtrees,
            ZynReporter.collect_scenes: {"slots"},
            ZynReporter.collect_master: track_subtrees,
            ZynReporter.collect_project: {"host"},
            ZynReporter.collect_transport: {"transport"},
            ZynReporter.collect_device: {"device"},
            ZynReporter.collect_primary: {"device"},
            ZynReporter.collect_clip: {"clip"},
            ZynReporter.collect_browser: {"browser"},
            Reporter.collect_grids: {"clip"},
        }
        # The subtrees of the pass being collected
        self.refreshing = ALL_SUBTREES
        # What changes by itself, collected on every pass
        self.fast_collectors = [
            ZynReporter.collect_transport,
            ZynReporter.collect_meters,
            ZynReporter.collect_playing_steps,
        ]
        self._counts = {"track": 0, "fxtrack": 0, "scene": 0}
        self._failed = set()
        self.full_requested = True
        """The next pass reads the whole model (set from other threads too: bridge.py's signals)."""
        self.dirty = set(ALL_SUBTREES)
        """The subtrees changed since the last full pass (request_full; for now any of them
        makes the pass collect all, collect_subtrees is the entry point for collecting only
        these)."""
        self._last_full_ms = None
        # The peak meters [(channel key, channel)] and the playing notes key of the cursor track
        # of the last full pass, for the fast passes
        self._meters = []
        self._next_meters = []
        self._notes_key = None
        # The keys of the slots by slot key base, of the parameters by (owner, index), built once
        self._slot_keys = {}
        self._param_keys = {}
        self._pad_keys = None
        self.full_passes = 0
        self.fast_passes = 0

    def request_full(self, subtrees=ALL_SUBTREES):
        """Read the slow part with the next pass (a command, a change in zynthian); ``subtrees``
        are the ones the change affects (``SUBTREES``)."""
        self.dirty.update(subtrees)
        self.full_requested = True

    @staticmethod
    def channel_base(bank, position):
        """The key base of a channel (``bank``: "track" or "fxtrack", BANKS)."""
        return channel_key(BANKS[bank], position)

    def is_full_due(self, now):
        if self.full_requested:
            # Cleared before the collectors run: a signal meanwhile asks for another full pass
            self.full_requested = False
            return True
        if self.dump_all or self._last_full_ms is None or now - self._last_full_ms >= SLOW_INTERVAL_MS:
            return True
        # The values of a commanded item are reported again
        return any(due <= now for due in self.pending.values())

    def collect_pass(self):
        """As Reporter.collect_pass (the cursor clip is read once for it), but a collector that fails (a zynthian object in an unexpected
        state, a missing library function) is logged once and skipped: the others still report.
        A full or a fast pass (the module's docstring)."""
        now = self.clock()
        full = self.is_full_due(now)
        if full:
            self._last_full_ms = now
            self.full_passes += 1
            self.apply_pending_forgets()
            # For now all subtrees (the slow safety net); later only the dirty ones
            self.dirty.clear()
            self.collect_subtrees(ALL_SUBTREES)
        else:
            self.fast_passes += 1
            self.prepare_pass()
            self.run_collectors(self.fast_collectors)

    def prepare_pass(self):
        self.daw.invalidate_clip()
        # The commands of the last pass are done (commands.py, a clip range not starting at 0)
        self.daw.clip_start_request = None

    def collect_subtrees(self, subtrees):
        """Collect the given subtrees of the slow part (``SUBTREES``), in the order of
        ``collectors``; the lines stay for ``take``."""
        self.prepare_pass()
        self.refreshing = frozenset(subtrees)
        # The track collectors run together: they collect the peak meters for the fast passes
        tracks = not self.refreshing.isdisjoint(self.collector_subtrees[ZynReporter.collect_tracks])
        if tracks:
            self._next_meters = []
        try:
            self.run_collectors([c for c in self.collectors if not self.refreshing.isdisjoint(self.collector_subtrees.get(c, ("host",)))])
        finally:
            self.refreshing = ALL_SUBTREES
        if tracks:
            self._meters = self._next_meters

    def run_collectors(self, collectors):
        for collector in collectors:
            try:
                collector(self)
            except Exception as e:
                if collector.__name__ not in self._failed:
                    self._failed.add(collector.__name__)
                    logging.getLogger("dbm_zynthian").error("Reporter %s failed: %s", collector.__name__, e)

    # The fast pass

    def collect_meters(self):
        """The peak meters and the playing notes of the tracks of the last full pass."""
        for base, channel in self._meters:
            self.put_vu(base, channel)
        if self._notes_key is not None:
            self.put(self._notes_key, self.playing_notes_text())

    def collect_playing_steps(self):
        """The playing step of each step grid (the steps themselves: collect_grids)."""
        for grid in self.note_grids:
            self.put(M.GRID_PLAYING_STEP | grid_key(grid), self.playing_step(grid))

    def playing_notes_text(self):
        notes = dict(self.daw.playing_notes)
        return ";".join(f"0:0:0:0:0:{key}:{velocity}" for key, velocity in sorted(notes.items()))

    # The parts of a channel

    def collect_channel(self, base, channel, values=True):
        """Name and color (the track list), with ``values`` the mixer values; ``base``: the
        channel key."""
        put = self.put
        put(M.CHANNEL_NAME | base, channel.name)
        put(M.CHANNEL_COLOR | base, channel.color)
        if not values:
            return
        put(M.CHANNEL_MUTE | base, channel.mute)
        put(M.CHANNEL_SOLO | base, channel.solo)
        put(M.CHANNEL_VOLUME | base, channel.volume.value)
        self.put_parameter_text(M.CHANNEL_VOLUME_STR | base, channel.volume)
        put(M.CHANNEL_PAN | base, channel.pan.value)
        self.put_parameter_text(M.CHANNEL_PAN_STR | base, channel.pan)

    def collect_vu(self, base, channel):
        """The peak meters (also kept for the fast passes)."""
        self._next_meters.append((base, channel))
        self.put_vu(base, channel)

    def put_vu(self, base, channel):
        left, right = channel.vu()
        put = self.put
        put(M.CHANNEL_VU_LEFT | base, left)
        put(M.CHANNEL_VU_RIGHT | base, right)
        put(M.CHANNEL_VU | base, (left + right) / 2.0)

    def collect_track_flags(self, base, track):
        put = self.put
        put(M.CHANNEL_REC_ARM | base, track.arm)
        put(M.CHANNEL_CAN_HOLD_NOTES | base, track.can_hold_notes)
        put(M.CHANNEL_CAN_HOLD_AUDIO | base, track.can_hold_audio)

    def collect_removed(self, bank, count):
        """Positions which no longer exist (a chain was removed) are reported as not existing."""
        kind = BANKS[bank]
        for position in range(count, self._counts[bank]):
            self.put(M.SCENE_EXISTS | position << 16 if kind is None else M.CHANNEL_EXISTS | channel_key(kind, position), 0)
        self._counts[bank] = count

    # The collectors

    def collect_tracks(self):
        """The track list; with the subtrees "mixer" and "slots" also the mixer values and sends,
        the slots of each track."""
        mixer, slots = "mixer" in self.refreshing, "slots" in self.refreshing
        tracks = self.daw.track_list(self.banks.full_flat_track_list)
        selected = next((position for position, track in enumerate(tracks) if track.selected), -1)
        put = self.put
        put(M.TRACK_OFFSET, self.banks.track_offset)
        # No pinning in zynthian: the cursor track follows the selection (without the report the
        # core keeps a cursor of its own); /device/pinned likewise (without it the core takes the
        # device as always pinned, as Reaper's, and lights a Pin button)
        put(M.TRACK_PINNED, -1)
        # A deselection outside of the list (the selected track was removed)
        if self._last_selected != selected and self._last_selected >= len(tracks):
            put(M.CHANNEL_SELECTED | channel_key(ChKind.TRACK, self._last_selected), False)
        self._last_selected = selected
        for position, track in enumerate(tracks):
            base = channel_key(ChKind.TRACK, position)
            put(M.CHANNEL_EXISTS | base, 1)
            put(M.CHANNEL_NUMBER | base, position)
            put(M.CHANNEL_TYPE | base, track.type_name)
            if position != selected:
                put(M.CHANNEL_SELECTED | base, False)
            if mixer:
                self.collect_track_flags(base, track)
            self.collect_vu(base, track)
            self.collect_channel(base, track, mixer)
            if mixer:
                self.collect_sends(base, track, with_color=True)
            if not slots:
                continue
            try:
                self.collect_slots(base, track)
            except Exception as e:
                # A failing launcher must not hide the tracks
                if "collect_slots" not in self._failed:
                    self._failed.add("collect_slots")
                    logging.getLogger("dbm_zynthian").error("Reporter collect_slots failed: %s", e)
        self.collect_removed("track", len(tracks))
        put(M.TRACK_COUNT, len(tracks))
        if selected >= 0:
            put(M.CHANNEL_SELECTED | channel_key(ChKind.TRACK, selected), True)
        # The notes played on the cursor track (the core keeps one note cache: the notes of a new
        # cursor track are reported again)
        notes_key = M.CHANNEL_PLAYING_NOTES | channel_key(ChKind.TRACK, selected)
        if selected != self._last_cursor:
            self._last_cursor = selected
            self.forget(notes_key)
        self._notes_key = notes_key if selected >= 0 else None
        if self._notes_key is not None:
            put(self._notes_key, self.playing_notes_text())

    def launcher(self):
        launcher = self.daw.launcher
        return launcher if launcher is not None and launcher.available else None

    def collect_slots(self, base, track):
        """The slots of a track: the sequences of its MIDI channel in the phrases (launcher.py);
        none for audio chains, effect tracks and the master. ``base``: the channel key."""
        launcher = self.launcher()
        if launcher is None:
            return
        chan = launcher.sequence_of(track)
        if chan is None:
            self.put(M.SLOT_COUNT | base, 0)
            return
        count = launcher.num_phrases()
        selected = launcher.selected_phrase if track.selected else -1
        recording = launcher.recording and launcher.selected_sequence() is not None
        put = self.put
        for phrase in range(count):
            slot_base = base | phrase << 48
            keys = self._slot_keys.get(slot_base)
            if keys is None:
                keys = self._slot_keys[slot_base] = tuple(id_ | slot_base for id_ in SLOT_VALUES)
            exists, has_content, name, color, is_selected, is_playing, is_play_queued, is_stop_queued, is_recording, rec_queued = keys
            state = launcher.play_state(phrase, chan)
            content = launcher.has_content(phrase, chan)
            put(exists, True)
            put(has_content, content)
            put(name, launcher.slot_name(phrase, chan))
            # An empty slot has no color (as Bitwig's; zynthian's launcher shows it gray)
            put(color, track.color if content else (0, 0, 0))
            put(is_selected, phrase == selected)
            playing, play_queued, stop_queued = slot_states(state)
            put(is_playing, playing)
            put(is_play_queued, play_queued)
            put(is_stop_queued, stop_queued)
            put(is_recording, recording and phrase == selected)
            put(rec_queued, False)
        self.put(M.SLOT_COUNT | base, count)

    def collect_scenes(self):
        """The phrases of zynseq's current scene as scenes."""
        launcher = self.launcher()
        if launcher is None:
            return
        put = self.put
        put(M.SCENE_OFFSET, self.banks.scene_offset)
        count = launcher.num_phrases()
        for phrase in range(count):
            base = phrase << 16
            put(M.SCENE_EXISTS | base, True)
            put(M.SCENE_NUMBER | base, phrase)
            put(M.SCENE_NAME | base, str(phrase + 1))
            put(M.SCENE_COLOR | base, (128, 128, 128))
            put(M.SCENE_SELECTED | base, phrase == launcher.selected_phrase)
        self.collect_removed("scene", count)
        put(M.SCENE_COUNT, count)

    def collect_effect_tracks(self):
        tracks = self.daw.effect_tracks()
        put = self.put
        put(M.EFFECT_TRACK_OFFSET, self.banks.fx_track_offset)
        mixer = "mixer" in self.refreshing
        for position, track in enumerate(tracks):
            base = channel_key(ChKind.EFFECT, position)
            put(M.CHANNEL_EXISTS | base, 1)
            put(M.CHANNEL_NUMBER | base, position)
            put(M.CHANNEL_TYPE | base, track.type_name)
            put(M.CHANNEL_SELECTED | base, track.selected)
            if mixer:
                self.collect_track_flags(base, track)
            self.collect_channel(base, track, mixer)
            if mixer:
                self.collect_sends(base, track, with_color=False)
        self.collect_removed("fxtrack", len(tracks))
        put(M.EFFECT_TRACK_COUNT, len(tracks))

    def collect_master(self):
        master = self.daw.master()
        if master is None:
            return
        base = channel_key(ChKind.MASTER, 0)
        self.put(M.CHANNEL_SELECTED | base, master.selected)
        self.collect_vu(base, master)
        self.collect_channel(base, master, "mixer" in self.refreshing)

    def collect_project(self):
        """zynthian has no undo history (only the pattern editor's): nothing to undo or redo."""
        self.put(M.PROJECT_CAN_UNDO, False)
        self.put(M.PROJECT_CAN_REDO, False)

    def collect_transport(self):
        transport = self.daw.transport
        if transport is None or not transport.available:
            return
        put = self.put
        put(M.PLAY, transport.playing)
        put(M.REPEAT, False)  # No arranger loop in zynthian
        put(M.TEMPO, transport.tempo)
        put(M.NUMERATOR, transport.numerator)
        put(M.DENOMINATOR, 4)
        put(M.METRONOME, transport.metronome)
        launcher = self.launcher()
        recording = launcher is not None and launcher.recording
        # Recording is MIDI record into a zynseq pattern (launcher.py); there is no arranger
        put(M.RECORD, recording)
        put(M.OVERDUB_LAUNCHER, recording)
        put(M.OVERDUB_ARRANGER, False)
        volume = transport.metronome_volume
        put(M.METRONOME_VOLUME, volume)
        put(M.METRONOME_VOLUME_STR, f"{20.0 * math.log10(volume):.1f} dB" if volume > 0 else "-Inf dB")

    def num_param_pages(self):
        return max(0, int(self.banks.setup.get("numParamPages", 8)))

    def collect_device(self):
        """The cursor device, the sibling window and the parameter pages (as the reference
        host's collect_device and collect_pages)."""
        chain = self.daw.devices
        devices = chain.devices()
        device = chain.cursor()
        exists = device is not None
        position = devices.index(device) if exists else -1
        put = self.put
        put(M.DEVICE_COUNT, len(devices))
        put(M.DEVICE_EXISTS, exists)
        put(M.DEVICE_POSITION, position)
        put(M.DEVICE_NAME, device.name if exists else "")
        put(M.DEVICE_BYPASS, exists and not device.enabled)
        put(M.DEVICE_WINDOW, False)
        put(M.DEVICE_PINNED, False)
        put(M.DEVICE_EXPAND, False)
        put(M.DEVICE_HAS_DRUM_PADS, False)
        put(M.DEVICE_HAS_LAYERS, False)
        for index in range(NUM_SIBLINGS):
            sibling_position = chain.sibling_offset + index
            sibling = devices[sibling_position] if sibling_position < len(devices) else None
            # /device/sibling/<index + 1>/: the slot holds the index
            base = index << 32
            put(M.DEVICE_SIBLING_NAME | base, sibling.name if sibling is not None else "")
            put(M.DEVICE_SIBLING_BYPASS | base, sibling is not None and not sibling.enabled)
            put(M.DEVICE_SIBLING_POSITION | base, sibling_position if sibling is not None else -1)
            put(M.DEVICE_SIBLING_SELECTED | base, sibling is not None and sibling_position == position)
        self.collect_pages(Dev.DEVICE, device)

    def collect_pages(self, owner, device):
        """The parameters of the first numParamPages pages as one flat list, the page names and
        the selected page (as the reference host's collect_pages); ``owner``: the ParamOwner."""
        exists = device is not None
        num_pages = self.num_param_pages()
        pages = device.page_names() if exists else []
        used = min(len(pages), num_pages)
        put = self.put
        for index in range(num_pages * PARAMS_PER_PAGE):
            name, value, text, steps, modulated = self.param_keys(owner, index)
            parameter = device.parameter(index) if exists and index < used * PARAMS_PER_PAGE else None
            if parameter is not None and parameter.exists:
                put(name, parameter.name)
                put(value, parameter.value)
                self.put_parameter_text(text, parameter)
            else:
                put(name, "")
                put(value, 0.0)
                self.put_displayed(text, "", ["", ""])
            put(steps, -1)
            put(modulated, parameter.value if parameter is not None and parameter.exists else 0.0)
        owner_base = owner << 16
        put(M.PARAM_COUNT | owner_base, used * PARAMS_PER_PAGE)
        for index in range(used):
            put(M.PAGE_NAME | owner_base | index << 32, pages[index])
        put(M.PAGE_COUNT | owner_base, used)
        put(M.PAGE_SELECTED | owner_base, device.selected_page if exists else -1)

    def param_keys(self, owner, index):
        """The keys of a parameter: name, value, value/str, steps, modulated."""
        base = owner << 16 | index << 32
        keys = self._param_keys.get(base)
        if keys is None:
            keys = self._param_keys[base] = tuple(id_ | base for id_ in PARAM_VALUES)
        return keys

    def collect_primary(self):
        """The first instrument (SpecificDevice.collect for the instrument matcher), if the
        controller asks for it (deviceIds FIRST_INSTRUMENT): name, bypass, pages.

        zynthian's synths have no drum pads and no layers: hasDrumPads 0, hasLayers 0,
        layer/count 0, and the 128 pads reported as not existing. The pads must be reported: with
        hasLayers 0 the core's layer modes use the device's drum pad bank (``uses_layers`` =
        ``!hasDrumPads && hasLayers``), and until the host reports pads that bank has the core's
        own 128 existing pads ("Drum 36", … as for Reaper). Reported as absent, the layer and pad
        controls of the controller find no item (unlit, no commands)."""
        if "FIRST_INSTRUMENT" not in self.banks.setup.get("deviceIds", []):
            return
        device = self.daw.devices.primary()
        exists = device is not None
        put = self.put
        primary = Dev.PRIMARY << 16
        put(M.DEVICE_EXISTS | primary, exists)
        put(M.DEVICE_NAME | primary, device.name if exists else "")
        put(M.DEVICE_BYPASS | primary, exists and not device.enabled)
        put(M.DEVICE_WINDOW | primary, False)
        put(M.DEVICE_EXPAND | primary, False)
        put(M.DEVICE_PARAMETERS | primary, False)
        put(M.DEVICE_HAS_DRUM_PADS | primary, False)
        put(M.DEVICE_HAS_LAYERS | primary, False)
        self.collect_pages(Dev.PRIMARY, device)
        put(M.DEVICE_HAS_MUTED_PADS | primary, False)
        put(M.DEVICE_HAS_SOLOED_PADS | primary, False)
        put(M.DEVICE_LAYER_COUNT | primary, 0)
        if self._pad_keys is None:
            pads = M.PAD_EXISTS | PadKind.PAD << 16 | Dev.PRIMARY << 32
            self._pad_keys = tuple(pads | index << 48 for index in range(NUM_DRUM_PADS))
        for k in self._pad_keys:
            put(k, False)

    def collect_clip(self):
        """The cursor clip (clip.py): a pattern starts at 0 and loops over its beats; no
        shuffle, no accent, no pinning. Reporting loopStart/loopLength switches the core to the
        host's step grids."""
        launcher = self.launcher()
        if launcher is None:
            return
        launcher.flush()
        clip = self.daw.cursor_clip()
        exists = clip is not None
        put = self.put
        put(M.CLIP_EXISTS, exists)
        if exists:
            put(M.CLIP_START, 0.0)
        put(M.CLIP_END, float(clip.beats) if exists else 0.0)
        put(M.CLIP_LOOP, exists)
        if exists:
            put(M.CLIP_LOOP_START, 0.0)
        put(M.CLIP_LOOP_LENGTH, float(clip.beats) if exists else 0.0)
        put(M.CLIP_SHUFFLE, False)
        put(M.CLIP_PINNED, False)
        color = (0, 0, 0)
        if exists:
            track = next((t for t in self.daw.tracks() if t.selected), None)
            color = track.color if track is not None else (128, 128, 128)
        put(M.CLIP_COLOR, color)

    def browser_pages(self):
        """The page sizes of the filter columns and the results (model setup)."""
        setup = self.banks.setup
        return max(1, int(setup.get("numFilterColumnEntries", 16))), max(1, int(setup.get("numResults", 16)))

    def collect_browser(self):
        """The browser (§3.8): only ``/browser/exists`` while it is closed."""
        browser = getattr(self.daw, "browser", None)
        if browser is not None:
            browser.collect(self, *self.browser_pages())

    def playing_step(self, grid):
        clip = self.daw.cursor_clip()
        if clip is None:
            return -1
        return clip.playing_step(grid, clip.phrase, clip.chan)
