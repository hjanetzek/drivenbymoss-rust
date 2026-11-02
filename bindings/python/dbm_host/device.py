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

- A device has page names and remote controls (``RemoteControl``: the name of the slot and its
  parameter; a slot without a parameter is empty). The same parameter can be on several pages
  (the EQ+ bands are the parameters of ``Equalizer``). A device without remote controls (no
  fixture data) creates its parameters when a command uses them (value 0, no name, no
  displayed text).
- The cursor device may show fewer pages than the device has (``Device.cursor_pages``): Bitwig
  showed only the page "Perform" of the fixture's Drum Machine as the cursor device
  (``/device/page/count 1``), but "Perform" and "Default" as the first instrument
  (``/primary/page/count 2``), in all recordings (tests/conformance/*/replay/*-rust.rec, the
  model lines after the fresh project) and all Java traces (``osc:/device/page/2/exists 0``,
  ``osc:/primary/page/2/exists 1`` while the Drum Machine is selected). Bitwig does not say why.
- The layers (an Instrument Layer) and drum pads (a Drum Machine) are ``layer.Layer`` objects;
  the layers of a Drum Machine are its pads with a device (see layer.py).
- ``selectNext`` / ``selectPrevious`` move in the device chain of the track and stop at its
  ends; they do not enter nested chains (the Polysynths in the layers of the fixture's
  Instrument Layer: neither Bitwig nor the OSC observer reported a change,
  ``flexi.device.next-previous-device`` of generic_flexi-device).
- The pinned cursor device stays on its device when the selection changes; the cursor track is
  not pinned with it.
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

from .layer import FIRST_KIT_PAD, NUM_DRUM_PADS, Layer, empty_pad
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

@dataclass
class RemoteControl:
    """A slot of a remote-control page (Bitwig ``RemoteControl``): its own name and the parameter
    of the device it controls."""

    name: str
    parameter: Parameter


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
    """Without remote controls: the parameters which a command used, by their index in the flat
    list of the pages (page * PARAMS_PER_PAGE + index)."""
    remote_controls: list[RemoteControl | None] = field(default_factory=list)
    """The slots of all pages as one flat list (page * PARAMS_PER_PAGE + index); None for an
    empty slot."""
    cursor_pages: list[str] | None = None
    """The page names as the cursor device shows them, if they differ from ``pages`` (see the
    module docstring)."""
    layers: list[Layer] = field(default_factory=list)
    """The layers of a layered device (an Instrument Layer); see ``layer_list`` for a Drum
    Machine."""
    drum_pads: list[Layer] | None = None
    """The 128 pads of a Drum Machine (index = note), None for other devices."""
    drum_pad_indication: bool = False
    """``drumPadBank.setIndication``."""

    def page_names(self, cursor: bool = False) -> list[str]:
        """The page names; as the cursor device shows them with ``cursor``."""
        return self.cursor_pages if cursor and self.cursor_pages is not None else self.pages

    def parameter(self, index: int, preferences: Preferences, cursor: bool = False) -> Parameter | None:
        """The parameter of the flat index (``page * 8 + n``), None if its page does not exist
        (Bitwig: a parameter of a page cursor without a page, which ignores the commands)."""
        if not 0 <= index < len(self.page_names(cursor)) * PARAMS_PER_PAGE:
            return None
        if self.remote_controls:
            control = self.remote_control(index)
            return control.parameter if control is not None else None
        if index not in self.parameters:
            self.parameters[index] = Parameter(preferences, 0.0, name="")
        return self.parameters[index]

    def remote_control(self, index: int) -> RemoteControl | None:
        """The slot of the flat index, None if it is empty or does not exist."""
        return self.remote_controls[index] if 0 <= index < len(self.remote_controls) else None

    def select_page(self, index: int, cursor: bool = False) -> None:
        """``selectedPageIndex.set``: ignored out of the range."""
        if 0 <= index < len(self.page_names(cursor)):
            self.selected_page = index

    def selected_page_name(self, cursor: bool = False) -> str:
        pages = self.page_names(cursor)
        return pages[self.selected_page] if 0 <= self.selected_page < len(pages) else ""

    # Layers and drum pads (LayerBankImpl, DrumPadBankImpl)

    @property
    def has_drum_pads(self) -> bool:
        return self.drum_pads is not None

    @property
    def has_layers(self) -> bool:
        """A Drum Machine and an Instrument Layer have layers (Bitwig ``hasLayers``)."""
        return self.has_drum_pads or bool(self.layers)

    def layer_list(self) -> list[Layer]:
        """The layers: the pads with a device of a Drum Machine, else ``layers``."""
        if self.drum_pads is not None:
            return [pad for pad in self.drum_pads if pad.exists]
        return list(self.layers)

    def add_layer(self, layer: Layer) -> Layer:
        layer.device = self
        self.layers.append(layer)
        return layer

    def make_drum_machine(self, preferences: Preferences, pads: dict[int, Layer]) -> None:
        """Give the device the 128 pads; ``pads`` are the pads with a device by their note, the
        others are empty."""
        self.drum_pads = [pads.get(note) or empty_pad(preferences) for note in range(NUM_DRUM_PADS)]
        for pad in self.drum_pads:
            pad.device = self

    def select_layer(self, layer: Layer) -> None:
        """Select a layer or pad in the editor: the only selected one of the device."""
        for other in self.layers + (self.drum_pads or []):
            other.selected = other is layer

    @property
    def has_muted_pads(self) -> bool:
        return any(pad.exists and pad.mute for pad in self.drum_pads or [])

    @property
    def has_soloed_pads(self) -> bool:
        return any(pad.exists and pad.solo for pad in self.drum_pads or [])

    def clear_muted_pads(self) -> None:
        for pad in self.drum_pads or []:
            pad.set_mute(False)

    def clear_soloed_pads(self) -> None:
        for pad in self.drum_pads or []:
            pad.set_solo(False)


def eq_plus(preferences: Preferences) -> Device:
    """A new EQ+ with its defaults (``insertBitwigDevice (EQ+)``): the pages Gains, Freqs, Qs
    control the bands, named ``<band> Gain|Freq|Q`` (as Bitwig named the slots of the fixture's
    EQ+, ``/device/param/<n>/name`` in tests/conformance/*/replay/*-rust.rec)."""
    equalizer = Equalizer(preferences)
    controls = [RemoteControl(f"{band + 1} {kind}", parameters[band])
                for kind, parameters in (("Gain", equalizer.gains), ("Freq", equalizer.frequencies), ("Q", equalizer.qs))
                for band in range(EQ_NUM_BANDS)]
    return Device(EQ_PLUS, AUDIO_FX, list(EQ_PLUS_PAGES), equalizer=equalizer, remote_controls=controls)


class DeviceChain:
    """The devices of a track and the selected one (the cursor device of the track)."""

    def __init__(self):
        self.devices: list[Device] = []
        self.selected_index = 0
        self.sibling_offset = 0
        """The first device of the sibling window (``DeviceBank.scrollPosition``)."""

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

    def select_neighbour(self, direction: int) -> None:
        """``selectPrevious`` (-1) / ``selectNext`` (+1): stops at the ends of the chain (see the
        module docstring)."""
        if self.selected is not None:
            self.select_index(self.selected_index + direction)

    def scroll_siblings(self, direction: int, page_size: int) -> None:
        """``scrollPageBackwards`` / ``scrollPageForwards`` of the sibling window: a page if
        there are devices on it."""
        offset = self.sibling_offset + direction * page_size
        if 0 <= offset < len(self.devices):
            self.sibling_offset = offset
        elif direction < 0:
            self.sibling_offset = 0

    def first_drum_machine(self) -> Device | None:
        """The first Drum Machine (the device matcher of BitwigModel.drumDevice)."""
        return next((device for device in self.devices if device.has_drum_pads), None)

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


__all__ = ["AUDIO_FX", "Device", "DeviceChain", "RemoteControl", "EQ_NUM_BANDS", "EQ_PLUS", "EQ_TYPES", "Equalizer", "FIRST_KIT_PAD", "INSTRUMENT", "NOTE_FX",
           "PARAMS_PER_PAGE", "eq_plus", "format_eq_frequency", "format_eq_gain", "format_eq_q"]
