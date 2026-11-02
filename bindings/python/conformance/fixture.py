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
| The bands of the EQ+ | ``dbm_host.device.Equalizer`` | ``osc:/eq/…`` (track EQ selected) |
| The pages of the Drum Machine as the cursor device | only "Perform" | ``osc:/device/page/2/exists 0``; ``/device/page/count 1`` in the ``.rec`` model lines |
| The remote controls of the devices | ``REMOTE_CONTROLS``: name, value, displayed value | ``/device/param/<n>/name``, ``value``, ``value/str`` in the ``.rec`` model lines (after the fresh project; the same in all files) |
| The remote controls section of the Drum Machine | shown | ``osc:/device/parameters 1``, ``/device/parameters 1`` in the ``.rec`` model lines |
| The kit of the Drum Machine | ``DRUM_KIT``: 16 pads from C1 | ``/primary/pad/<36..51>/name``, ``volume`` in the ``.rec`` model lines; ``osc:/conformance/primary/drumpad/N/…`` |
| The layers of the Instrument Layer | ``LAYERS``: Polysynth, Acido | ``osc:/device/layer/N/…``, ``osc:/conformance/primary/layer/N/…`` |
| The color of layers and pads | the color of the track | ``osc:/device/layer/N/color``, ``/primary/pad/N/color`` |
| Markers | M1 at bar 3, M2 at bar 5 (FIXTURE.md), rgb(229,229,0) | ``osc:/marker/N/…``; ``/marker/<i>/position`` in the ``.rec`` model lines |
| The colors of FX 1 and FX 2 | not observed (the OSC track bank has no effect tracks); gray | |

The preferences of Bitwig in the profiles (``Setup.preferences``; Bitwig's preferences file is
binary, the values come from the setups' CONFORMANCE.md):

- The takeover mode: "Catch" (Bitwig's default) for the Launch Control XL, the Maschine Mk2 and
  JAM (launchcontrol_xl/CONFORMANCE.md: "Bitwig's takeover mode is set to catch"), "Immediate"
  for Generic Flexi (generic_flexi/CONFORMANCE.md, profile ``generic_flexi_immediate``).
- Auto-arm of the selected instrument track: on in all runs (the arm follows the selection of
  the tracks which can hold notes in the Java traces; the rules in dbm_host/model.py).
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
from dbm_host.device import AUDIO_FX, EQ_PLUS, FIRST_KIT_PAD, INSTRUMENT, Device, RemoteControl, eq_plus
from dbm_host.layer import Layer
from dbm_host.parameter import Parameter, format_semitones, recorded_formatter

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
# The cue markers: (name, position in beats)
MARKERS = [("M1", 8.0), ("M2", 16.0)]

# The devices of the tracks: (name, kind, remote-control pages)
DEVICES = {
    "Synth": [("Polysynth", INSTRUMENT, ["OSC1", "OSC2", "MIX", "FILTER", "FILTER/EG", "AMP", "Envelope", "Common"])],
    "Drums": [("Drum Machine", INSTRUMENT, ["Perform", "Default"])],
    "Layers": [("Instrument Layer", INSTRUMENT, ["Gain", "Common"])],
    "EQ": [(EQ_PLUS, AUDIO_FX, None)],
}

# The remote controls of the devices: (name, value, displayed value) per slot of the flat list of
# the pages, None for an empty slot, as Bitwig reported them for the fixture (the module
# docstring). The displayed values are known only at these values (``recorded_formatter``),
# except for the semitones of ``FORMATTERS``. Slots with the same name, value and text are taken
# to be the same parameter of the device (e.g. "Filt Freq" on three pages of the Polysynth); the
# two "Output" slots of the Drum Machine differ.
REMOTE_CONTROLS = {
    "Polysynth": [
        ('Osc1Pitch', 0.5, '0.00 st'), ('Sync1', 0.0, '0.00'), ('Uni1', 0.109533, '11 cents'), ('Osc1 Pan', 0.5, '0.00 %'),
        ('Shape1', 0.5, '0.00 %'), ('PW1', 0.5, '50.0 %'), ('Sub1', 0.0, '0.00 %'), ('Sub PW1', 0.5, '50.0 %'),
        ('Osc2Pitch', 0.5, '0.00 st'), ('Sync2', 0.0, '0.00'), ('Uni2', 0.056, '6 cents'), ('Osc2Pan', 0.5, '0.00 %'),
        ('Shape2', 0.0, '-100 %'), ('PW2', 0.48, '48.0 %'), ('Sub2', 0.0, '0.00 %'), ('Sub PW2', 0.5, '50.0 %'),
        ('1/2 Mix', 0.0, '0.00 %'), ('Noise', 0.0, '0.00 %'), ('Pitch', 0.5, '0.00 st'), ('Glide time', 0.0, '0.00 ms'),
        ('HPF Freq', 0.369345, '50.0 Hz'), ('HPF Reso', 0.0, '0.00 %'), ('Drive', 0.5, '0.0 dB'), ('FB', 0.0, '0.00 %'),
        ('Filt Freq', 0.692913, '2.59 kHz'), ('Reso', 0.394762, '39.5 %'), ('Shaper%', 0.0, '0.00 %'), ('Filt FM', 0.5, '0.00 st'),
        ('Keytrack', 0.5, '50.0 %'), ('FEG D', 0.825, '82.5 %'), ('FEG Curve', 0.5, '0.00'), ('FEG Amount', 0.5, '0.00 st'),
        ('Filt Freq', 0.692913, '2.59 kHz'), ('Reso', 0.394762, '39.5 %'), ('FEG Amt', 0.5, '0.00 st'), ('FEG Curve', 0.5, '0.00'),
        ('FEG A', 0.0, '0.00 %'), ('FEG D', 0.825, '82.5 %'), ('FEG S', 0.0, '0.00 %'), ('FEG R', 0.0, '0.00 %'),
        ('Gain', 1.0, '0.0 dB'), ('Vel Sens.', 0.5, '+15.0 dB'), ('Pan', 0.5, '0.00 %'), ('Output', 0.684, '-3.9 dB'),
        ('AEG A', 0.065, '6.50 %'), ('AEG D', 0.5, '50.0 %'), ('AEG S', 0.95, '95.0 %'), ('AEG R', 0.3, '30.0 %'),
        ('FEG A', 0.0, '0.00 %'), ('FEG D', 0.825, '82.5 %'), ('FEG S', 0.0, '0.00 %'), ('FEG R', 0.0, '0.00 %'),
        ('AEG A', 0.065, '6.50 %'), ('AEG D', 0.5, '50.0 %'), ('AEG S', 0.95, '95.0 %'), ('AEG R', 0.3, '30.0 %'),
        ('Filt Freq', 0.692913, '2.59 kHz'), ('Reso', 0.394762, '39.5 %'), ('FEG Amt', 0.5, '0.00 st'), ('Filt FM', 0.5, '0.00 st'),
        ('1/2 Mix', 0.0, '0.00 %'), ('Noise', 0.0, '0.00 %'), ('Drive', 0.5, '0.0 dB'), ('Output', 0.684, '-3.9 dB'),
    ],
    "Drum Machine": [
        ('LoPass', 1.0, '100 %'), ('HiPass', 0.015748, '1.57 %'), ('ComAt', 0.5, '0.00 %'), ('ComSust', 0.0, '0.00 %'),
        ('Distortion', 0.055118, '5.51 %'), ('Delay', 0.0, '0.00 %'), ('Room', 0.192874, '19.3 %'), ('Output', 0.892126, '89.2 %'),
        ('Output', 0.0, '-Inf dB'), None, None, None,
        None, None, None, None,
    ],
    "Instrument Layer": [
        None, None, None, None,
        None, None, None, ('Gain', 0.5, '0.0 dB'),
        None, None, None, None,
        None, None, None, ('Gain', 0.5, '0.0 dB'),
    ],
}

# The parameters whose displayed value is known beyond the fixture's value
FORMATTERS = {("Polysynth", "Osc1Pitch"): format_semitones(7.0), ("Polysynth", "Osc2Pitch"): format_semitones(7.0)}


def remote_controls(daw: SimDaw, device_name: str) -> list:
    """The remote controls of a device of the fixture (REMOTE_CONTROLS)."""
    controls, parameters = [], {}
    for spec in REMOTE_CONTROLS.get(device_name, []):
        if spec is None:
            controls.append(None)
            continue
        name, value, text = spec
        if spec not in parameters:
            formatter = recorded_formatter(value, text, FORMATTERS.get((device_name, name)))
            parameters[spec] = Parameter(daw.preferences, value, formatter=formatter, name=name)
        controls.append(RemoteControl(name, parameters[spec]))
    return controls

# The pages of the Drum Machine as the cursor device (see the module docstring)
DRUM_MACHINE_CURSOR_PAGES = ["Perform"]

# The kit of the Drum Machine: the pads with a device from C1, (name, volume) as Bitwig reported
# them (/primary/pad/<n>/name and volume in tests/conformance/launchcontrol_xl/replay/
# novation_launch_control_xl-layer_mix-rust.rec after the fresh project); panorama center
DRUM_KIT = [
    ("Kick", 0.793701), ("Snare", 0.793701), ("HH Closed", 0.633701), ("HH Open", 0.680367),
    ("Ride", 0.710367), ("Rimshot", 0.793701), ("Rim", 0.680754), ("Crash", 0.647034),
    ("Tom1", 0.793701), ("Tom2", 0.758701), ("Tom3", 0.793701), ("Tom4", 0.793701),
    ("Tambourine SingleHit 1", 0.793701), ("Shaker Three Eggs", 0.793701), ("Cowbell Big", 0.743701), ("Cowbell Big Muted", 0.730367),
]

# The layers of the Instrument Layer, named after their devices (the second one is an Acido,
# not a Polysynth as FIXTURE.md says), volume 0 dB, panorama center
LAYERS = ["Polysynth", "Acido"]


def build_fixture(preferences: Preferences | None = None) -> SimDaw:
    """The fixture project (see the module docstring)."""
    daw = SimDaw(preferences=preferences or Preferences())
    for name in EFFECT_TRACKS:
        daw.add_effect_track(name)
    for (name, track_type), color in zip(TRACKS, TRACK_COLORS):
        track = daw.add_track(track_type, name, color)
        track.volume.value = TRACK_VOLUME
        for device_name, kind, pages in DEVICES.get(name, []):
            if device_name == EQ_PLUS:
                track.devices.add(eq_plus(daw.preferences))
                continue
            device = track.devices.add(Device(device_name, kind, list(pages), remote_controls=remote_controls(daw, device_name)))
            if device_name == "Drum Machine":
                device.cursor_pages = list(DRUM_MACHINE_CURSOR_PAGES)
                device.parameters_visible = True
                pads = {FIRST_KIT_PAD + index: Layer(daw.preferences, pad_name, color, volume, edit=daw.application.edit)
                        for index, (pad_name, volume) in enumerate(DRUM_KIT)}
                device.make_drum_machine(daw.preferences, pads)
            elif device_name == "Instrument Layer":
                for layer_name in LAYERS:
                    device.add_layer(Layer(daw.preferences, layer_name, color, edit=daw.application.edit))
    for _ in range(NUM_SCENES):
        daw.add_scene()
    for track in daw.tracks[:2]:
        track.slots[0].create_clip(4.0)
    for name, position in MARKERS:
        daw.markers.add(name, position)
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


__all__ = ["DEVICES", "DRUM_KIT", "LAYERS", "REMOTE_CONTROLS", "SETUPS", "Setup", "build_fixture", "copy_profile_config"]
