# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The layers and drum pads of a device: the channels inside a layered device (an Instrument
Layer) and the 128 pads of a Drum Machine.

Implements the DAW side of HOST_BRIDGE_GUIDE.md §3.5 "Layers and drum pads" (``/<device>/layer/
<n>/…``, ``/<device>/pad/<n>/…``, ``hasDrumPads``, ``hasLayers``, ``hasMutedPads``,
``hasSoloedPads``) for the Reporter, and the channel commands of §4.6 (``pad/<n>/…``,
``layer/<n>/…``, ``clearMute``, ``clearSolo``, ``indicate``) for the CommandHandler.

Simulates the Java DrivenByMoss for Bitwig: ``LayerImpl`` and ``DrumPadImpl`` (channels of a
device, a layer exists if it has a device), ``LayerBankImpl`` and ``DrumPadBankImpl`` (the
banks, ``clearMutedPads``, ``clearSoloedPads``, ``setIndication``), with the Bitwig API behind
them (``DeviceLayer``, ``DrumPad``, ``DrumPadBank``).

Simplifications and what is known only from recordings:

- The layers of a Drum Machine are its pads with a device, in the order of the notes (Bitwig
  reported the same names, values and selection for ``/primary/layer/<n>`` and the pads of
  ``/primary/pad/<36 + n>`` in tests/conformance/launchcontrol_xl/replay/
  novation_launch_control_xl-layer_mix-rust.rec, and the OSC observer the same for
  ``/conformance/primary/layer`` and ``/conformance/primary/drumpad``): they are the same
  objects here.
- One layer (or pad) of a device is selected at a time (``selectInEditor``; selecting layer 2
  deselected layer 1 in target/conformance/generic_flexi-device-java.json, ``flexi.layer.
  select``). Selecting a layer changes neither the cursor track nor the cursor device.
- A pad without a device is reported with the values of an empty Bitwig channel (no name, black,
  volume and panorama 0, activated) and ignores the commands.
- Sends of layers and pads, their VU meters and the drum pad indication are not simulated (the
  indication is only stored).
"""

from copy import deepcopy

from .parameter import VOLUME_0DB, Parameter, Preferences, format_pan, format_volume

Color = tuple[int, int, int]

NO_COLOR: Color = (0, 0, 0)

# The pads of a Drum Machine (BitwigModel.NUM_DRUM_PADS) and the note of its first pad in a new
# kit (C1)
NUM_DRUM_PADS = 128
FIRST_KIT_PAD = 36


class Layer:
    """A layer of a layered device or a drum pad (LayerImpl, DrumPadImpl; Bitwig ``Channel``).

    Has the values and operations of ``model.Channel`` which the channel commands use (``name``,
    ``color``, ``volume``, ``pan``, ``mute``, ``solo``, ``activated``, ``select``, ``set_mute``,
    ``set_solo``, ``toggle_solo``), so that ``CommandHandler.channel_command`` works on it.
    """

    def __init__(self, preferences: Preferences, name: str = "", color: Color = NO_COLOR, volume: float = VOLUME_0DB,
                 has_device: bool = True, edit=None):
        """``has_device``: the layer (pad) has a device, i.e. it exists; ``edit`` is called on
        an undoable change (``Application.edit``)."""
        self.name = name
        self.color = color
        self.volume = Parameter(preferences, volume, default=VOLUME_0DB, formatter=format_volume, name="Volume")
        self.pan = Parameter(preferences, 0.5, formatter=format_pan, name="Pan")
        self.mute = False
        self.solo = False
        self.activated = True
        self.selected = False
        self.has_device = has_device
        self.device = None
        """The device which owns the layer (set by Device.add_layer / make_drum_machine)."""
        self._edit = edit

    @property
    def exists(self) -> bool:
        return self.has_device

    def __deepcopy__(self, memo):
        """A copy (of a duplicated device) shares the edit function, not the application."""
        copy = Layer.__new__(Layer)
        memo[id(self)] = copy
        for name, value in self.__dict__.items():
            setattr(copy, name, value if name == "_edit" else deepcopy(value, memo))
        return copy

    def _edited(self) -> None:
        if self._edit is not None:
            self._edit()

    def select(self) -> None:
        """``selectInEditor``: the only selected layer of its device."""
        if self.device is not None:
            self.device.select_layer(self)
        else:
            self.selected = True

    def set_mute(self, mute: bool) -> None:
        if self.exists and self.mute != bool(mute):
            self.mute = bool(mute)
            self._edited()

    def set_solo(self, solo: bool, exclusive: bool = False) -> None:
        if self.exists and self.solo != bool(solo):
            self.solo = bool(solo)
            self._edited()

    def toggle_solo(self) -> None:
        """``solo ().toggleUsingPreferences (false)``; the exclusive solo of layers is not
        simulated (it is off in all setups)."""
        self.set_solo(not self.solo)


def empty_pad(preferences: Preferences) -> Layer:
    """A drum pad without a device."""
    return Layer(preferences, has_device=False, volume=0.0)


__all__ = ["FIRST_KIT_PAD", "Layer", "NUM_DRUM_PADS", "empty_pad"]
