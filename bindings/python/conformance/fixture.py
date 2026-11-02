# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The fixture project and the setups of the conformance runs, for the simulated DAW.

The project is tests/conformance/FIXTURE.md: the Bitwig runs start each scenario file on a new
project from the default template, which is the fixture. The details which FIXTURE.md does not
give come from what the observer reported of the fixture in the Java runs (the ``initial``
state of ``target/conformance/*-java.json``); each is listed below with its source. They
describe the project, not a behaviour.

| Detail | Value | Source |
| --- | --- | --- |
| Track colors | ``TRACK_COLORS`` | ``osc:/track/N/color`` |
| Track volume | -10.0 dB (0.540742) | ``osc:/track/N/volume`` 69, ``volumeStr`` |
| Selected track and slot | Synth, slot 1 of Drums | ``osc:/track/1/selected``, ``osc:/track/2/clip/1/isSelected`` |
| Clips | Synth and Drums slot 1, color of the track | ``osc:/track/N/clip/1/hasContent``, ``color`` |
| Scenes | 8, ``Scene N``, rgb(80,80,80) | ``osc:/scene/N/…`` |
| Tempo | 110 BPM | ``osc:/tempo/raw`` |
| Arranger overdub | on | ``osc:/overdub`` |
| Devices and their pages | ``DEVICES`` | ``osc:/device/…``, ``osc:/primary/…`` per selected track |
| The colors of FX 1 and FX 2 | not observed (the OSC track bank has no effect tracks); gray | |

The preferences of Bitwig in the profiles (``Setup.preferences``; Bitwig's preferences file is
binary, the values come from the setups' CONFORMANCE.md):

- The takeover mode: "Catch" (Bitwig's default) for the Launch Control XL, the Maschine Mk2 and
  JAM (launchcontrol_xl/CONFORMANCE.md: "Bitwig's takeover mode is set to catch"), "Immediate"
  for Generic Flexi (generic_flexi/CONFORMANCE.md, profile ``generic_flexi_immediate``).
- Auto-arm of the selected instrument track: on in all runs (the arm follows the selection in
  the Java traces).
- Exclusive solo: off (not observed yet).

The settings of the controller come from the profile (``~/.local/share/dbm-conformance/
profiles/<setup>/config/*.config``, ``DBM_PROFILE_DIR``), copied into the run's own config
folder (the profiles are never written).
"""

import os
import shutil
from dataclasses import dataclass, field
from pathlib import Path

from dbm_host import TAKEOVER_CATCH, TAKEOVER_IMMEDIATE, Preferences, SimDaw
from dbm_host.device import AUDIO_FX, INSTRUMENT, Device

TRACKS = [
    ("Synth", "INSTRUMENT"),
    ("Drums", "INSTRUMENT"),
    ("Layers", "INSTRUMENT"),
    ("EQ", "AUDIO"),
    *((f"Audio {n}", "AUDIO") for n in range(5, 11)),
]
TRACK_COLORS = [(87, 97, 198), (149, 73, 203), (217, 56, 113), (217, 46, 36), (255, 87, 6), (217, 157, 16), (67, 210, 185), (115, 152, 20), (0, 157, 71), (68, 200, 255)]
EFFECT_TRACKS = ["FX 1", "FX 2"]
NUM_SCENES = 8
TRACK_VOLUME = 0.540742
TEMPO = 110.0

# The devices of the tracks: (name, kind, remote-control pages). Drum pads and layers come with
# step 4 of PYTHON_HOST_PLAN.md §5.
DEVICES = {
    "Synth": [("Polysynth", INSTRUMENT, ["OSC1", "OSC2", "MIX", "FILTER", "FILTER/EG", "AMP", "Envelope", "Common"])],
    "Drums": [("Drum Machine", INSTRUMENT, ["Perform", "Default"])],
    "Layers": [("Instrument Layer", INSTRUMENT, ["Gain", "Common"])],
    "EQ": [("EQ+", AUDIO_FX, ["Gains", "Freqs", "Qs"])],
}


def build_fixture(preferences: Preferences | None = None) -> SimDaw:
    """The fixture project (see the module docstring)."""
    daw = SimDaw(preferences=preferences or Preferences())
    for name in EFFECT_TRACKS:
        daw.add_effect_track(name)
    for (name, track_type), color in zip(TRACKS, TRACK_COLORS):
        track = daw.add_track(track_type, name, color)
        track.volume.value = TRACK_VOLUME
        for device_name, kind, pages in DEVICES.get(name, []):
            track.devices.add(Device(device_name, kind, list(pages)))
    for _ in range(NUM_SCENES):
        daw.add_scene()
    for track in daw.tracks[:2]:
        track.slots[0].create_clip(4.0)
    daw.transport.tempo = TEMPO
    daw.transport.overdub_arranger = True
    daw.tracks[1].slots[0].select()
    daw.tracks[0].select()
    return daw


# ---------------------------------------------------------------------------------------------
# Setups


@dataclass
class Setup:
    """A setup of tests/conformance/<name>: the controller and the DAW's preferences."""

    name: str
    controller_id: str
    takeover_mode: str = TAKEOVER_CATCH
    auto_arm: bool = True
    exclusive_solo: bool = False
    app_version: str = "Bitwig Studio 5.3.13"
    files: list[str] = field(default_factory=list)

    def preferences(self) -> Preferences:
        return Preferences(takeover_mode=self.takeover_mode, exclusive_solo=self.exclusive_solo, auto_arm=self.auto_arm)

    @property
    def scenario_dir(self) -> Path:
        return REPOSITORY / "tests" / "conformance" / self.name

    def scenario_files(self) -> list[Path]:
        return sorted(self.scenario_dir.glob("*.toml"))

    @property
    def profile_dir(self) -> Path:
        base = os.environ.get("DBM_PROFILE_DIR") or str(Path.home() / ".local/share/dbm-conformance/profiles")
        return Path(base) / self.name


REPOSITORY = Path(__file__).resolve().parents[3]

SETUPS = {
    setup.name: setup
    for setup in (
        Setup("launchcontrol_xl", "5AFA5045-FF92-4737-A091-5B371CD5E529"),
        Setup("maschine_mk2", "015E4908-C049-4808-A645-A7A44BEFE36B"),
        Setup("maschine_jam", "72F32A83-A697-446B-9016-64AA3F9476E4"),
        Setup("generic_flexi", "1B43F14F-CAAF-468A-9418-417C678653A0", takeover_mode=TAKEOVER_IMMEDIATE),
    )
}

# The setting of the Flexi controllers which names the mapping file
FLEXI_FILENAME_KEY = "LOAD_/_SAVE_FILENAME"


def copy_profile_config(setup: Setup, config_dir: Path, flexi_mapping: Path | None = None) -> list[str]:
    """Copy the controller settings of the profile into config_dir; with flexi_mapping, the
    Flexi "Filename" setting names it. Returns the copied file names (empty without a
    profile: the controller then runs with its defaults)."""
    source = setup.profile_dir / "config"
    copied = []
    if source.is_dir():
        for path in sorted(source.glob("*.config")):
            target = config_dir / path.name
            shutil.copyfile(path, target)
            copied.append(path.name)
    if flexi_mapping is not None:
        _set_property(config_dir / "DrivenByMoss4Reaper-Flexi.config", FLEXI_FILENAME_KEY, str(flexi_mapping))
    return copied


def _set_property(path: Path, key: str, value: str) -> None:
    """Set one line ``key=value`` of a Java properties file (``:`` escaped as Properties.store
    does)."""
    lines = path.read_text().splitlines() if path.exists() else ["#"]
    escaped = value.replace("\\", "\\\\").replace(":", "\\:").replace("=", "\\=")
    lines = [line for line in lines if not line.startswith(key + "=")] + [f"{key}={escaped}"]
    path.write_text("\n".join(lines) + "\n")


__all__ = ["DEVICES", "SETUPS", "Setup", "build_fixture", "copy_profile_config"]
