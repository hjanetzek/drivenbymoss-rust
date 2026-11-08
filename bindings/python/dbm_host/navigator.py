# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The clip launcher navigator of the host (host feature ``clipLauncherNavigator``).

Implements HOST_BRIDGE_GUIDE.md §2 (the host feature ``clipLauncherNavigator``) and its
commands: ``navigator selectTrack <position>`` (the core selects the track afterwards with
``track <p>/select``) and ``navigator tracks|clips|scenes/-|+``. Only created if the model setup
of the controller has ``wantsClipLauncherNavigator`` (e.g. Generic Flexi).

Simulates ``ClipLauncherNavigator`` of the Bitwig extension of this repository, which is
``ClipLauncherNavigatorImpl`` of the Java DrivenByMoss for Bitwig: a track bank of one track and
one scene (``createTrackBank (1, 0, 1)``) which follows its own cursor track
(``followCursorTrack``), its only slot (``theClip``) and the scene bank of that track bank. The
navigator selects ``theClip`` when it moves; the core's scene window follows the scene of
``theClip`` (the ``sceneIndex`` observer).

What the scenarios observe (the Java traces of generic_flexi ``track`` and ``slots``,
``target/conformance/generic_flexi-*-java.json``): Bitwig has one selected slot; selecting a
track with ``selectTrack`` keeps it where it is (the slot of another track stays selected),
``tracks/-|+`` moves it to the slot of the new track in the navigator's scene.

Simplifications (each a deliberate choice, not a fit to a scenario):

- The navigator's track bank holds the main tracks of the project (``SimDaw.tracks``); when the
  selected channel is not one of them (an effect track, the master), it keeps its position.
  Which tracks Bitwig's ``createTrackBank (1, 0, 1)`` holds is not documented.
- Following the cursor track: the position of the bank is the position of the selected track
  (a page size of 1 shows exactly the cursor track). It is updated when the navigator is used,
  not by an observer; the result is the same, as only the navigator reads it.
- The ``sceneIndex`` observer of Bitwig also fires once when the slot is created; in the Java
  traces no scene is selected at the start, so the simulation only reacts to changes.
- ``host.scheduleTask`` (the scene selection 100 ms after ``scenes/-|+``) runs on the simulated
  time of the DAW (``SimDaw.add_time_listener``).
"""

from typing import Callable

from .banks import Banks
from .model import SimDaw, Slot, Track

# ClipLauncherNavigator.handle: the delay of the scene selection after "scenes/-|+"
SELECT_SCENE_DELAY_MS = 100


class ClipLauncherNavigator:
    """The navigator: the position of its one-track bank and of its one-scene bank."""

    def __init__(self, daw: SimDaw, banks: Banks):
        self.daw = daw
        self.banks = banks
        self.track_position = 0
        """The scroll position of the single track bank (an index of ``SimDaw.tracks``)."""
        self.scene_position = 0
        """The scroll position of the scene bank of the single track bank (the scene of
        ``theClip``)."""
        self._followed = None
        self._tasks: list[tuple[float, Callable[[], None]]] = []
        daw.add_time_listener(self._run_tasks)
        self._follow()

    # The single track bank

    def _follow(self) -> None:
        """``followCursorTrack``: the bank shows the cursor track (the selection) when it
        changed."""
        selected = self.daw.selected_channel
        if selected is self._followed:
            return
        self._followed = selected
        if selected in self.daw.tracks:
            self.track_position = self.daw.tracks.index(selected)

    @property
    def track(self) -> Track | None:
        """``theTrack``: the track of the single track bank, None if the position is empty."""
        tracks = self.daw.tracks
        return tracks[self.track_position] if 0 <= self.track_position < len(tracks) else None

    @property
    def clip(self) -> Slot | None:
        """``theClip``: the slot of the track in the navigator's scene."""
        track = self.track
        if track is None or not 0 <= self.scene_position < len(track.slots):
            return None
        return track.slots[self.scene_position]

    def _scroll_tracks(self, direction: int) -> None:
        """``singleTrackBank.scrollBackwards / scrollForwards``: one track, within the list."""
        position = self.track_position + direction
        if 0 <= position < len(self.daw.tracks):
            self.track_position = position

    def _scroll_scenes(self, direction: int) -> None:
        """``sceneBank.scrollBackwards / scrollForwards``: one scene, within the scenes; the
        ``sceneIndex`` observer of ``theClip`` sees a change."""
        position = self.scene_position + direction
        if 0 <= position < len(self.daw.scenes) and position != self.scene_position:
            self.scene_position = position
            self._scene_index_changed(position)

    def _scene_index_changed(self, position: int) -> None:
        """The ``sceneIndex`` observer: the scene window of the controller scrolls to the page of
        the scene, the scene is selected."""
        page_size = max(1, self.banks.num_scenes)
        self.banks.scene_offset = position // page_size * page_size
        self.daw.scenes[position].select()

    def _select_clip(self) -> None:
        """``theClip.select``."""
        clip = self.clip
        if clip is not None:
            clip.select()

    # The commands

    def select_track(self, position: int) -> None:
        """``navigator selectTrack <position>``: the core selects the track afterwards."""
        self._follow()
        self.track_position = int(position)

    def step(self, target: str, direction: int) -> None:
        """``navigator tracks|clips|scenes/-|+`` (target: tracks, clips, scenes; direction -1,
        1)."""
        self._follow()
        if target == "tracks":
            self._scroll_tracks(direction)
            self._select_clip()
            track = self.track
            if track is not None:
                track.select()
            self._followed = self.daw.selected_channel
        else:
            self._scroll_scenes(direction)
            self._select_clip()
            if target == "scenes":
                self._later(SELECT_SCENE_DELAY_MS, self._select_scene)

    def _select_scene(self) -> None:
        """``sceneBank.getItemAt (0).selectInEditor``: the navigator's scene."""
        if 0 <= self.scene_position < len(self.daw.scenes):
            self.daw.scenes[self.scene_position].select()

    # host.scheduleTask on the simulated time

    def _later(self, delay_ms: float, task: Callable[[], None]) -> None:
        self._tasks.append((self.daw.time_ms + delay_ms, task))

    def _run_tasks(self, _elapsed_ms: float) -> None:
        now = self.daw.time_ms
        due = [task for when, task in self._tasks if when <= now]
        self._tasks = [(when, task) for when, task in self._tasks if when > now]
        for task in due:
            task()


__all__ = ["ClipLauncherNavigator", "SELECT_SCENE_DELAY_MS"]
