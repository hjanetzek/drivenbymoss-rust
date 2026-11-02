# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The devices of the simulated DAW: a device chain per track, the cursor device, the first
instrument and the EQ+ (the equalizer).

Implements the DAW side of HOST_BRIDGE_GUIDE.md §3.5 (cursor device, specific devices, the
remote-control pages) for the Reporter, and the page and selection operations of §4.6 for the
CommandHandler.

Simulates the Java DrivenByMoss for Bitwig: ``CursorDeviceImpl`` (the cursor device, which
follows the device selection of the cursor track, ``CursorDeviceFollowMode.FOLLOW_SELECTION``),
``SpecificDeviceImpl`` with the instrument matcher (``DeviceID.FIRST_INSTRUMENT``, the first
instrument of the cursor track), ``DeviceBankImpl`` (the siblings), ``ParameterPageBankImpl``
(the remote-control pages, ``selectedPageIndex``), ``EqualizerDeviceImpl`` and ``EqTypeParameter``
(the first EQ+ of the cursor track, ``DeviceID.EQ``; ``Equalizer``).

Simplifications (each a deliberate choice, not a fit to a scenario):

- A device has page names; its parameters are created when a command uses them (value 0, no
  name, no displayed text: the fixture does not describe them yet, step 4 of
  PYTHON_HOST_PLAN.md §5 adds them, with layers and drum pads). The pages are reported with
  parameters without names.
- Every track remembers its selected device (Bitwig does); a new track selects its first device.
- Each device remembers its selected page (Bitwig does); selecting a page out of the range is
  ignored.
- The EQ+ (``Equalizer``): the defaults and the texts are what Bitwig showed for the fixture's
  EQ+ (see ``Equalizer``); Bitwig's EQ+ is not documented. Only the 7 types of the core are
  simulated, not the 16 values of Bitwig's type parameter.
"""

import math

from copy import deepcopy
from dataclasses import dataclass, field

from .parameter import Parameter, Preferences, format_three_digits

# The parameters of a remote-control page (BitwigModel.PARAMS_PER_PAGE)
PARAMS_PER_PAGE = 8

# ---------------------------------------------------------------------------------------------
# The EQ+

EQ_PLUS = "EQ+"
EQ_NUM_BANDS = 8

# The band types (EqualizerBandType of the Java version) with the start of the displayed value
# of Bitwig's type parameter (EqualizerDeviceImpl.EQ_TYPES), its index (EqTypeParameter, of 16
# values) and the index of the Reaper EQ which the core uses (EqualizerDevice.REAPER_TYPES of
# the Bitwig extension, -1: off).
EQ_TYPES = ("off", "lowcut", "lowshelf", "bell", "highcut", "highshelf", "notch")
EQ_TYPE_TEXTS = ("Off", "Low-cut", "Low-shelf", "Bell", "High-cut", "High-shelf", "Notch")
EQ_TYPE_INDICES = (0, 3, 6, 7, 10, 13, 14)
EQ_REAPER_TYPES = (-1, 4, 0, 8, 3, 1, 6)

# The remote-control pages of a new EQ+ (as the fixture's EQ+ reported them)
EQ_PLUS_PAGES = ("Gains", "Freqs", "Qs")

# The default frequencies of the bands, normalized, as Bitwig reported them for the fixture's
# EQ+ (the first ``/eq/param/<3n>/value`` model lines of
# tests/conformance/launchcontrol_xl/replay/novation_launch_control_xl-equalizer-rust.rec: 80 Hz
# to 10 kHz in octaves)
EQ_DEFAULT_FREQUENCIES = (0.200630, 0.300738, 0.401374, 0.501717, 0.602061, 0.698968, 0.799311, 0.899655)


def format_eq_frequency(value: float) -> str:
    """The frequency of an EQ+ band: 20 Hz * 1000^value (20 Hz .. 20 kHz), three significant
    digits (``format_three_digits``), from 1 kHz in kHz. Fits all texts Bitwig showed (80.0 Hz, 160 Hz, 320 Hz, 640 Hz,
    650 Hz, 1.28 kHz, 2.50 kHz, 5.00 kHz, 10.0 kHz in ``target/conformance/*-java.json``,
    ``osc:/eq/freq/N/valueStr``); the curve is not documented by Bitwig."""
    hertz = 20.0 * 1000.0 ** value
    if hertz >= 1000.0:
        return format_three_digits(hertz / 1000.0) + " kHz"
    return format_three_digits(hertz) + " Hz"


def format_eq_gain(value: float) -> str:
    """The gain of an EQ+ band. Observed: 0.0 -> ``-30.0 dB``, 0.5 -> ``0.0 dB`` (Java traces,
    ``osc:/eq/gain/N/valueStr``); a linear scale of -30..+30 dB is assumed between them."""
    gain = (value - 0.5) * 60.0
    return f"{0.0 if abs(gain) < 0.05 else gain:.1f} dB"


def format_eq_q(value: float) -> str:
    """The Q of an EQ+ band. Observed only 0.5 -> ``1.00``; a logarithmic scale of 0.1..10 is
    assumed (not observed)."""
    return f"{0.1 * 100.0 ** value:.2f}"


class Equalizer:
    """The bands of an EQ+ (EqualizerDeviceImpl, the parameters ``TYPE<n>``, ``FREQ<n>``,
    ``GAIN<n>``, ``Q<n>`` of Bitwig's EQ+).

    The defaults are the fixture's EQ+ as Bitwig reported it: all types off, the frequencies of
    ``EQ_DEFAULT_FREQUENCIES``, gains 0.5 (0.0 dB), Qs 0.5 (1.00).
    """

    def __init__(self, preferences: Preferences):
        self.types = ["off"] * EQ_NUM_BANDS
        self.frequencies = [Parameter(preferences, value, formatter=format_eq_frequency, name=f"Band {band + 1} Frequency")
                            for band, value in enumerate(EQ_DEFAULT_FREQUENCIES)]
        self.gains = [Parameter(preferences, 0.5, formatter=format_eq_gain, name=f"Band {band + 1} Gain") for band in range(EQ_NUM_BANDS)]
        self.qs = [Parameter(preferences, 0.5, formatter=format_eq_q, name=f"Band {band + 1} Q") for band in range(EQ_NUM_BANDS)]

    def parameter(self, index: int) -> Parameter | None:
        """The parameters in the layout of the Reaper EQ which the core uses
        (EqualizerDevice.getParameter): frequency, gain and Q of each band; None out of the
        range."""
        if not 0 <= index < 3 * EQ_NUM_BANDS:
            return None
        band, kind = divmod(index, 3)
        return (self.frequencies, self.gains, self.qs)[kind][band]

    def reaper_type(self, band: int) -> int:
        """The type of a band as the index of the Reaper EQ (-1: off)."""
        return EQ_REAPER_TYPES[EQ_TYPES.index(self.types[band])]

    def type_text(self, band: int) -> str:
        """The displayed value of the type parameter."""
        return EQ_TYPE_TEXTS[EQ_TYPES.index(self.types[band])]

    def set_reaper_type(self, band: int, reaper_type: int) -> bool:
        """EqualizerDevice.setType: set the type given as the index of the Reaper EQ; False if the
        band or the type is unknown. The type is set as a value with a resolution
        (``set (index, 15)``), which the takeover mode does not catch: in the recordings Bitwig
        took it at once (``/eq/band/0/value 8`` after ``eq band/0 8``, launchcontrol_xl
        equalizer)."""
        if not 0 <= band < EQ_NUM_BANDS or reaper_type not in EQ_REAPER_TYPES:
            return False
        self.types[band] = EQ_TYPES[EQ_REAPER_TYPES.index(reaper_type)]
        return True

INSTRUMENT = "instrument"
AUDIO_FX = "audio_fx"
NOTE_FX = "note_fx"


@dataclass
class Device:
    """A device of a chain (Bitwig ``Device``)."""

    name: str
    kind: str = INSTRUMENT
    """INSTRUMENT, AUDIO_FX or NOTE_FX (the instrument matcher finds the first INSTRUMENT)."""
    pages: list[str] = field(default_factory=list)
    """The names of the remote-control pages."""
    selected_page: int = 0
    enabled: bool = True
    expanded: bool = True
    window_open: bool = False
    parameters_visible: bool = False
    """The remote-control section is shown (``isRemoteControlsSectionVisible``)."""
    equalizer: "Equalizer | None" = None
    """The bands of an EQ+ (None for other devices)."""
    parameters: dict[int, Parameter] = field(default_factory=dict)
    """The remote-control parameters which a command used, by their index in the flat list of
    the pages (page * PARAMS_PER_PAGE + index)."""

    def parameter(self, index: int, preferences: Preferences) -> Parameter | None:
        """The parameter of the flat index (``page * 8 + n``), None if its page does not exist
        (Bitwig: a parameter of a page cursor without a page, which ignores the commands)."""
        if not 0 <= index < len(self.pages) * PARAMS_PER_PAGE:
            return None
        if index not in self.parameters:
            self.parameters[index] = Parameter(preferences, 0.0, name="")
        return self.parameters[index]

    def select_page(self, index: int) -> None:
        """``selectedPageIndex.set``: ignored out of the range."""
        if 0 <= index < len(self.pages):
            self.selected_page = index

    @property
    def selected_page_name(self) -> str:
        return self.pages[self.selected_page] if 0 <= self.selected_page < len(self.pages) else ""


def eq_plus(preferences: Preferences) -> Device:
    """A new EQ+ with its defaults (``insertBitwigDevice (EQ+)``)."""
    return Device(EQ_PLUS, AUDIO_FX, list(EQ_PLUS_PAGES), equalizer=Equalizer(preferences))


class DeviceChain:
    """The devices of a track and the selected one (the cursor device of the track)."""

    def __init__(self):
        self.devices: list[Device] = []
        self.selected_index = 0

    def add(self, device: Device) -> Device:
        self.devices.append(device)
        return device

    @property
    def selected(self) -> Device | None:
        if 0 <= self.selected_index < len(self.devices):
            return self.devices[self.selected_index]
        return None

    def select(self, device: Device) -> None:
        if device in self.devices:
            self.selected_index = self.devices.index(device)

    def select_index(self, index: int) -> bool:
        if 0 <= index < len(self.devices):
            self.selected_index = index
            return True
        return False

    def first_instrument(self) -> Device | None:
        return next((device for device in self.devices if device.kind == INSTRUMENT), None)

    def first_equalizer(self) -> Device | None:
        """The first EQ+ (the device matcher of the EQ+, DeviceID.EQ)."""
        return next((device for device in self.devices if device.equalizer is not None), None)

    def remove(self, device: Device) -> None:
        """``deleteObject``: the selection stays at its index (clamped)."""
        if device in self.devices:
            self.devices.remove(device)
            self.selected_index = max(0, min(self.selected_index, len(self.devices) - 1))

    def duplicate(self, device: Device) -> Device | None:
        """``afterDeviceInsertionPoint ().copyDevices``: a copy after the device."""
        if device not in self.devices:
            return None
        selected = self.selected
        copy = deepcopy(device)
        self.devices.insert(self.devices.index(device) + 1, copy)
        if selected is not None:
            self.select(selected)
        return copy

    def position(self, device: Device | None) -> int:
        return self.devices.index(device) if device in self.devices else -1


__all__ = ["AUDIO_FX", "Device", "DeviceChain", "EQ_NUM_BANDS", "EQ_PLUS", "EQ_TYPES", "Equalizer", "INSTRUMENT", "NOTE_FX", "PARAMS_PER_PAGE",
           "eq_plus", "format_eq_frequency", "format_eq_gain", "format_eq_q"]
