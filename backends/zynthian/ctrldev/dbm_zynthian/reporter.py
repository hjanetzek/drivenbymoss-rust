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
from dbm_host.reporter import NUM_DRUM_PADS, NUM_SIBLINGS

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
SLOT_VALUES = ("exists", "hasContent", "name", "color", "selected", "playing", "playQueued", "stopQueued", "recording",
               "recQueued")


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
        # The peak meters [(prefix, channel)] and the playing notes address of the cursor track of
        # the last full pass, for the fast passes
        self._meters = []
        self._next_meters = []
        self._notes_address = None
        # The addresses, built once: track prefixes by (bank, position), slot addresses by
        # (track prefix, phrase)
        self._prefixes = {}
        self._slot_addresses = {}
        self._param_addresses = {}
        self._pad_addresses = None
        self._grid_addresses = {}
        self.full_passes = 0
        self.fast_passes = 0

    def request_full(self, subtrees=ALL_SUBTREES):
        """Read the slow part with the next pass (a command, a change in zynthian); ``subtrees``
        are the ones the change affects (``SUBTREES``)."""
        self.dirty.update(subtrees)
        self.full_requested = True

    def prefix(self, bank, position):
        """``/<bank>/<position>/``."""
        key = (bank, position)
        prefix = self._prefixes.get(key)
        if prefix is None:
            prefix = self._prefixes[key] = f"/{bank}/{position}/"
        return prefix

    def is_full_due(self, now):
        if self.full_requested:
            # Cleared before the collectors run: a signal meanwhile asks for another full pass
            self.full_requested = False
            return True
        if self.dump_all or self._last_full_ms is None or now - self._last_full_ms >= SLOW_INTERVAL_MS:
            return True
        # The values of a commanded item are reported again
        return any(due <= now for due in self.pending.values())

    def collect(self):
        """As Reporter.collect (the cursor clip is read once for it), but a collector that fails (a zynthian object in an unexpected
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
        return self.take()

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
        for prefix, channel in self._meters:
            self.put_vu(prefix, channel)
        if self._notes_address is not None:
            self.put(self._notes_address, self.playing_notes_text())

    def collect_playing_steps(self):
        """The playing step of each step grid (the steps themselves: collect_grids)."""
        for grid in self.note_grids:
            address = self._grid_addresses.get(grid.id)
            if address is None:
                address = self._grid_addresses[grid.id] = f"/clip/grid/{grid.id}/playingStep"
            self.put(address, self.playing_step(grid))

    def playing_notes_text(self):
        notes = dict(self.daw.playing_notes)
        return ";".join(f"0:0:0:0:0:{key}:{velocity}" for key, velocity in sorted(notes.items()))

    # The parts of a channel

    def collect_channel(self, prefix, channel, values=True):
        """Name and color (the track list), with ``values`` the mixer values."""
        self.put(prefix + "name", channel.name)
        self.put(prefix + "color", channel.color)
        if not values:
            return
        self.put(prefix + "mute", channel.mute)
        self.put(prefix + "solo", channel.solo)
        self.put(prefix + "volume", channel.volume.value)
        self.put_parameter_text(prefix + "volume/str", channel.volume)
        self.put(prefix + "pan", channel.pan.value)
        self.put_parameter_text(prefix + "pan/str", channel.pan)

    def collect_vu(self, prefix, channel):
        """The peak meters (also kept for the fast passes)."""
        self._next_meters.append((prefix, channel))
        self.put_vu(prefix, channel)

    def put_vu(self, prefix, channel):
        left, right = channel.vu()
        self.put(prefix + "vuleft", left)
        self.put(prefix + "vuright", right)
        self.put(prefix + "vu", (left + right) / 2.0)

    def collect_track_flags(self, prefix, track):
        self.put(prefix + "recarm", track.arm)
        self.put(prefix + "canHoldNotes", track.can_hold_notes)
        self.put(prefix + "canHoldAudio", track.can_hold_audio)

    def collect_removed(self, bank, count):
        """Positions which no longer exist (a chain was removed) are reported as not existing."""
        for position in range(count, self._counts[bank]):
            self.put(self.prefix(bank, position) + "exists", 0)
        self._counts[bank] = count

    # The collectors

    def collect_tracks(self):
        """The track list; with the subtrees "mixer" and "slots" also the mixer values and sends,
        the slots of each track."""
        mixer, slots = "mixer" in self.refreshing, "slots" in self.refreshing
        tracks = self.daw.track_list(self.banks.full_flat_track_list)
        selected = next((position for position, track in enumerate(tracks) if track.selected), -1)
        self.put("/track/offset", self.banks.track_offset)
        # No pinning in zynthian: the cursor track follows the selection (without the report the
        # core keeps a cursor of its own); /device/pinned likewise (without it the core takes the
        # device as always pinned, as Reaper's, and lights a Pin button)
        self.put("/track/pinned", -1)
        # A deselection outside of the list (the selected track was removed)
        if self._last_selected != selected and self._last_selected >= len(tracks):
            self.put(f"/track/{self._last_selected}/select", False)
        self._last_selected = selected
        for position, track in enumerate(tracks):
            prefix = self.prefix("track", position)
            self.put(prefix + "exists", 1)
            self.put(prefix + "number", position)
            self.put(prefix + "type", track.type_name)
            if position != selected:
                self.put(prefix + "select", False)
            if mixer:
                self.collect_track_flags(prefix, track)
            self.collect_vu(prefix, track)
            self.collect_channel(prefix, track, mixer)
            if mixer:
                self.collect_sends(prefix, track, with_color=True)
            if not slots:
                continue
            try:
                self.collect_slots(prefix, track)
            except Exception as e:
                # A failing launcher must not hide the tracks
                if "collect_slots" not in self._failed:
                    self._failed.add("collect_slots")
                    logging.getLogger("dbm_zynthian").error("Reporter collect_slots failed: %s", e)
        self.collect_removed("track", len(tracks))
        self.put("/track/count", len(tracks))
        if selected >= 0:
            self.put(f"/track/{selected}/select", True)
        # The notes played on the cursor track (the core keeps one note cache: the notes of a new
        # cursor track are reported again)
        if selected != self._last_cursor:
            self._last_cursor = selected
            self.forget(f"/track/{selected}/playingnotes")
        self._notes_address = self.prefix("track", selected) + "playingnotes" if selected >= 0 else None
        if self._notes_address is not None:
            self.put(self._notes_address, self.playing_notes_text())

    def launcher(self):
        launcher = self.daw.launcher
        return launcher if launcher is not None and launcher.available else None

    def collect_slots(self, prefix, track):
        """The slots of a track: the sequences of its MIDI channel in the phrases (launcher.py);
        none for audio chains, effect tracks and the master."""
        launcher = self.launcher()
        if launcher is None:
            return
        chan = launcher.sequence_of(track)
        if chan is None:
            self.put(prefix + "slot/count", 0)
            return
        count = launcher.num_phrases()
        selected = launcher.selected_phrase if track.selected else -1
        recording = launcher.recording and launcher.selected_sequence() is not None
        put = self.put
        for phrase in range(count):
            key = (prefix, phrase)
            addresses = self._slot_addresses.get(key)
            if addresses is None:
                addresses = self._slot_addresses[key] = tuple(f"{prefix}slot/{phrase}/{name}" for name in SLOT_VALUES)
            exists, has_content, name, color, is_selected, is_playing, is_play_queued, is_stop_queued, is_recording, rec_queued = addresses
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
        self.put(prefix + "slot/count", count)

    def collect_scenes(self):
        """The phrases of zynseq's current scene as scenes."""
        launcher = self.launcher()
        if launcher is None:
            return
        self.put("/scene/offset", self.banks.scene_offset)
        count = launcher.num_phrases()
        for phrase in range(count):
            prefix = self.prefix("scene", phrase)
            self.put(prefix + "exists", True)
            self.put(prefix + "number", phrase)
            self.put(prefix + "name", str(phrase + 1))
            self.put(prefix + "color", (128, 128, 128))
            self.put(prefix + "select", phrase == launcher.selected_phrase)
        self.collect_removed("scene", count)
        self.put("/scene/count", count)

    def collect_effect_tracks(self):
        tracks = self.daw.effect_tracks()
        self.put("/fxtrack/offset", self.banks.fx_track_offset)
        for position, track in enumerate(tracks):
            prefix = self.prefix("fxtrack", position)
            self.put(prefix + "exists", 1)
            self.put(prefix + "number", position)
            self.put(prefix + "type", track.type_name)
            self.put(prefix + "select", track.selected)
            mixer = "mixer" in self.refreshing
            if mixer:
                self.collect_track_flags(prefix, track)
            self.collect_channel(prefix, track, mixer)
            if mixer:
                self.collect_sends(prefix, track, with_color=False)
        self.collect_removed("fxtrack", len(tracks))
        self.put("/fxtrack/count", len(tracks))

    def collect_master(self):
        master = self.daw.master()
        if master is None:
            return
        self.put("/master/select", master.selected)
        self.collect_vu("/master/", master)
        self.collect_channel("/master/", master, "mixer" in self.refreshing)

    def collect_project(self):
        """zynthian has no undo history (only the pattern editor's): nothing to undo or redo."""
        self.put("/project/canUndo", False)
        self.put("/project/canRedo", False)

    def collect_transport(self):
        transport = self.daw.transport
        if transport is None or not transport.available:
            return
        self.put("/play", transport.playing)
        self.put("/repeat", False)  # No arranger loop in zynthian
        self.put("/tempo", transport.tempo)
        self.put("/numerator", transport.numerator)
        self.put("/denominator", 4)
        self.put("/click", transport.metronome)
        launcher = self.launcher()
        recording = launcher is not None and launcher.recording
        # Recording is MIDI record into a zynseq pattern (launcher.py); there is no arranger
        self.put("/record", recording)
        self.put("/overdub/launcher", recording)
        self.put("/overdub/arranger", False)
        volume = transport.metronome_volume
        self.put("/click/volume", volume)
        self.put("/click/volumeStr", f"{20.0 * math.log10(volume):.1f} dB" if volume > 0 else "-Inf dB")

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
        self.put("/device/count", len(devices))
        self.put("/device/exists", exists)
        self.put("/device/position", position)
        self.put("/device/name", device.name if exists else "")
        self.put("/device/bypass", exists and not device.enabled)
        self.put("/device/window", False)
        self.put("/device/pinned", False)
        self.put("/device/expand", False)
        self.put("/device/hasDrumPads", False)
        self.put("/device/hasLayers", False)
        for index in range(NUM_SIBLINGS):
            sibling_position = chain.sibling_offset + index
            sibling = devices[sibling_position] if sibling_position < len(devices) else None
            prefix = f"/device/sibling/{index + 1}/"
            self.put(prefix + "name", sibling.name if sibling is not None else "")
            self.put(prefix + "bypass", sibling is not None and not sibling.enabled)
            self.put(prefix + "position", sibling_position if sibling is not None else -1)
            self.put(prefix + "selected", sibling is not None and sibling_position == position)
        self.collect_pages("/device", device)

    def collect_pages(self, prefix, device):
        """The parameters of the first numParamPages pages as one flat list, the page names and
        the selected page (as the reference host's collect_pages)."""
        exists = device is not None
        num_pages = self.num_param_pages()
        pages = device.page_names() if exists else []
        used = min(len(pages), num_pages)
        for index in range(num_pages * PARAMS_PER_PAGE):
            name, value, text, steps, modulated = self.param_addresses(prefix, index)
            parameter = device.parameter(index) if exists and index < used * PARAMS_PER_PAGE else None
            if parameter is not None and parameter.exists:
                self.put(name, parameter.name)
                self.put(value, parameter.value)
                self.put_parameter_text(text, parameter)
            else:
                self.put(name, "")
                self.put(value, 0.0)
                self.put_displayed(text, "", ["", ""])
            self.put(steps, -1)
            self.put(modulated, parameter.value if parameter is not None and parameter.exists else 0.0)
        self.put(f"{prefix}/param/count", used * PARAMS_PER_PAGE)
        for index in range(used):
            self.put(f"{prefix}/page/{index}/name", pages[index])
        self.put(f"{prefix}/page/count", used)
        self.put(f"{prefix}/page/selected", device.selected_page if exists else -1)

    def param_addresses(self, prefix, index):
        """The addresses of a parameter: name, value, value/str, steps, modulated."""
        key = (prefix, index)
        addresses = self._param_addresses.get(key)
        if addresses is None:
            param = f"{prefix}/param/{index}/"
            addresses = self._param_addresses[key] = tuple(param + name for name in ("name", "value", "value/str", "steps", "modulated"))
        return addresses

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
        self.put("/primary/exists", exists)
        self.put("/primary/name", device.name if exists else "")
        self.put("/primary/bypass", exists and not device.enabled)
        self.put("/primary/window", False)
        self.put("/primary/expand", False)
        self.put("/primary/parameters", False)
        self.put("/primary/hasDrumPads", False)
        self.put("/primary/hasLayers", False)
        self.collect_pages("/primary", device)
        self.put("/primary/hasMutedPads", False)
        self.put("/primary/hasSoloedPads", False)
        self.put("/primary/layer/count", 0)
        if self._pad_addresses is None:
            self._pad_addresses = tuple(f"/primary/pad/{index}/exists" for index in range(NUM_DRUM_PADS))
        for address in self._pad_addresses:
            self.put(address, False)

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
        self.put("/clip/exists", exists)
        if exists:
            self.put("/clip/start", 0.0)
        self.put("/clip/end", float(clip.beats) if exists else 0.0)
        self.put("/clip/loop", exists)
        if exists:
            self.put("/clip/loopStart", 0.0)
        self.put("/clip/loopLength", float(clip.beats) if exists else 0.0)
        self.put("/clip/shuffle", False)
        self.put("/clip/pinned", False)
        color = (0, 0, 0)
        if exists:
            track = next((t for t in self.daw.tracks() if t.selected), None)
            color = track.color if track is not None else (128, 128, 128)
        self.put("/clip/color", color)

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
