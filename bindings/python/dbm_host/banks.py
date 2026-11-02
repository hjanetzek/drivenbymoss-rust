# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The banks of the bridge, created from the model setup of the controller.

Implements HOST_BRIDGE_GUIDE.md §1 "The model setup" (``dbm_get_model_setup``) and the paging
of §2 (``/track/offset``, ``/scene/offset``, ``/fxtrack/offset``) with the ``bank`` commands of
§4.3.

Simulates the bank setup of ``BitwigModel`` (the constructor) and ``ModelImpl`` of the Java
DrivenByMoss for Bitwig: the size of the track window, its sends and scenes, the effect track
bank (``numFxTracks``, -1 = ``numTracks``), the full flat track list (``hasFullFlatTrackList``:
the effect tracks and the master are in the track list).

The reference host reports all items, not windows (PYTHON_HOST_PLAN.md §3 "Windows"): the bank
commands only store the offsets for ``/track/offset`` etc. The sizes say how many sends are
reported and what the controller addresses. The later steps add their parts of the model setup
here (``numParamPages``, ``deviceIds``, ``noteClips``, ``numMarkers``, ...); ``setup`` keeps the
complete dict.
"""

from dataclasses import dataclass, field


@dataclass
class Banks:
    num_tracks: int = 8
    num_scenes: int = 8
    num_sends: int = 8
    num_fx_tracks: int = 8
    full_flat_track_list: bool = False
    wants_clip_launcher_navigator: bool = False
    track_offset: int = 0
    scene_offset: int = 0
    fx_track_offset: int = 0
    show_track_clip_launcher_feedback: bool = False
    """``track indicate`` (``trackBank.setShouldShowClipLauncherFeedback``)."""
    show_fx_track_clip_launcher_feedback: bool = False
    setup: dict = field(default_factory=dict)
    """The complete model setup of the controller."""

    def configure(self, setup: dict | None) -> None:
        """Size the banks from the model setup (keys as the Java ModelSetup)."""
        setup = setup or {}
        self.setup = dict(setup)
        self.num_tracks = max(1, int(setup.get("numTracks", 8)))
        self.num_scenes = max(1, int(setup.get("numScenes", 8)))
        self.num_sends = max(0, int(setup.get("numSends", 8)))
        fx_tracks = int(setup.get("numFxTracks", -1))
        self.num_fx_tracks = self.num_tracks if fx_tracks < 0 else fx_tracks
        self.full_flat_track_list = bool(setup.get("hasFullFlatTrackList", False))
        self.wants_clip_launcher_navigator = bool(setup.get("wantsClipLauncherNavigator", False))

    @classmethod
    def from_model_setup(cls, setup: dict | None) -> "Banks":
        banks = cls()
        banks.configure(setup)
        return banks

    def scroll(self, bank: str, offset: int) -> bool:
        """The ``bank track|scene|fxtrack <offset>`` command; False for an unknown bank."""
        offset = max(0, int(offset))
        if bank == "track":
            self.track_offset = offset
        elif bank == "scene":
            self.scene_offset = offset
        elif bank == "fxtrack":
            self.fx_track_offset = offset
        else:
            return False
        return True
