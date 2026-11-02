# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The devices of the simulated DAW: a device chain per track, the cursor device and the first
instrument.

Implements the DAW side of HOST_BRIDGE_GUIDE.md §3.5 (cursor device, specific devices, the
remote-control pages) for the Reporter, and the page and selection operations of §4.6 for the
CommandHandler.

Simulates the Java DrivenByMoss for Bitwig: ``CursorDeviceImpl`` (the cursor device, which
follows the device selection of the cursor track, ``CursorDeviceFollowMode.FOLLOW_SELECTION``),
``SpecificDeviceImpl`` with the instrument matcher (``DeviceID.FIRST_INSTRUMENT``, the first
instrument of the cursor track), ``DeviceBankImpl`` (the siblings), ``ParameterPageBankImpl``
(the remote-control pages, ``selectedPageIndex``).

Simplifications (each a deliberate choice, not a fit to a scenario):

- A device has page names but no parameters yet (step 4 of PYTHON_HOST_PLAN.md §5 adds them,
  with layers and drum pads). The pages are reported with parameters that do not exist.
- Every track remembers its selected device (Bitwig does); a new track selects its first device.
- Each device remembers its selected page (Bitwig does); selecting a page out of the range is
  ignored.
"""

from dataclasses import dataclass, field

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

    def select_page(self, index: int) -> None:
        """``selectedPageIndex.set``: ignored out of the range."""
        if 0 <= index < len(self.pages):
            self.selected_page = index

    @property
    def selected_page_name(self) -> str:
        return self.pages[self.selected_page] if 0 <= self.selected_page < len(self.pages) else ""


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

    def position(self, device: Device | None) -> int:
        return self.devices.index(device) if device in self.devices else -1


__all__ = ["AUDIO_FX", "Device", "DeviceChain", "INSTRUMENT", "NOTE_FX"]
