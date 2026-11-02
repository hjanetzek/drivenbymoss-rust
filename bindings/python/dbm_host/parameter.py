# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""A ranged value of the simulated DAW: a normalized value 0..1 with a default, a displayed text,
touch and indication, and the takeover mode of the DAW.

Implements the value side of HOST_BRIDGE_GUIDE.md §4.1 (the value command suffixes: set, ``/inc``,
``/reset``, ``/immediately``, ``/touch``, ``/indicate``) and the displayed values of §2 ("Lines and
values": ``<value address>/str``, ``/str/6``, ``/str/8``).

Simulates the Bitwig API ``Parameter`` / ``SettableRangedValue`` as the Java DrivenByMoss uses it
(``ParameterImpl``, ``RangedValueImpl`` in ``bitwig/framework/daw/data``): ``set``, ``inc``,
``setImmediately``, ``reset``, ``touch``, ``setIndication``, ``displayedValue`` and
``getLimited``.

Simplifications (Bitwig details without public documentation):

- The texts follow what Bitwig reported in the recorded sessions
  (``tests/conformance/*/replay/*.rec``): volumes ``-10.0 dB`` / ``-Inf dB``, panorama ``0.00 %``.
  The volume curve is ``dB = 20 * log10(2 * v**3)`` (0 dB at 0.793701, +6 dB at 1), which gives the
  reported pairs (0.540742 = -10.0 dB, 0.793701 = 0.0 dB). The panorama text is the signed percent
  of the deviation from the center; only the center was observed.
- ``limit_text`` imitates Bitwig's ``getLimited`` from the observed pairs: up to 6 characters the
  unit of a number is dropped (``0.0 dB`` -> ``0.0``), a text which is no number is cut
  (``-Inf dB`` -> ``-Inf d``).
- The takeover mode "catch" takes a value only if it is close to the current one or if the
  control crossed it since its last value. Bitwig's "relative scaling" is not simulated.
"""

import math
import re
from dataclasses import dataclass
from typing import Callable

TAKEOVER_IMMEDIATE = "immediate"
TAKEOVER_CATCH = "catch"

# The distance at which the takeover mode "catch" takes a value (about one MIDI step)
CATCH_TOLERANCE = 1.0 / 127.0

_NUMBER = re.compile(r"[-+]?\d+(\.\d+)?")

# Bitwig's 0 dB of a volume (0.5 ** (1/3))
VOLUME_0DB = 0.5 ** (1.0 / 3.0)


@dataclass
class Preferences:
    """The preferences of the simulated DAW which change the behaviour of commands (Bitwig:
    Settings > Controllers and Settings > Behavior). A fixture sets them as the DAW of the
    reference recordings had them."""

    takeover_mode: str = TAKEOVER_IMMEDIATE
    """How a value set by a controller is taken: TAKEOVER_IMMEDIATE or TAKEOVER_CATCH."""
    exclusive_solo: bool = False
    """Soloing a channel with ``solo/toggle`` unsolos all others (Bitwig: "exclusive solo")."""


def format_volume(value: float) -> str:
    """The displayed value of a volume or send (see the module docstring)."""
    if value <= 0:
        return "-Inf dB"
    return f"{20.0 * math.log10(2.0 * value ** 3):.1f} dB"


def format_pan(value: float) -> str:
    """The displayed value of a panorama: the deviation from the center in percent."""
    percent = (value * 2.0 - 1.0) * 100.0
    return f"{0.0 if abs(percent) < 0.005 else percent:.2f} %"


def format_percent(value: float) -> str:
    return f"{value * 100.0:.0f} %"


def format_metronome_volume(value: float) -> str:
    """The metronome volume, -48..0 dB linear (Bitwig reported 0.75 as -12.0 dB)."""
    return f"{(value - 1.0) * 48.0:.1f} dB"


def limit_text(text: str, limit: int) -> str:
    """A displayed value shortened to the limit, as Bitwig's getLimited (see the module
    docstring)."""
    number, _, unit = text.partition(" ")
    is_number = bool(unit) and _NUMBER.fullmatch(number) is not None
    if limit >= 8 and len(text) <= limit:
        return text
    if is_number:
        return number[:limit]
    return text[:limit]


class Parameter:
    """A ranged value 0..1 with a default (Bitwig ``Parameter``)."""

    def __init__(self, preferences: Preferences, value: float = 0.0, default: float | None = None,
                 formatter: Callable[[float], str] = format_percent, name: str = ""):
        self.preferences = preferences
        self.value = float(value)
        self.default = float(value if default is None else default)
        self.formatter = formatter
        self.name = name
        self.touched = False
        self.indicated = False
        # The last value of the control which set this value (for the takeover mode catch)
        self._control: float | None = None

    @staticmethod
    def _clamp(value: float) -> float:
        return min(1.0, max(0.0, float(value)))

    def set(self, value: float) -> bool:
        """Set the value from a control, with the takeover mode of the preferences. Returns
        False if the value was not taken."""
        value = self._clamp(value)
        if self.preferences.takeover_mode == TAKEOVER_CATCH:
            previous = self._control
            self._control = value
            caught = abs(value - self.value) <= CATCH_TOLERANCE
            if not caught and previous is not None:
                caught = (previous - self.value) * (value - self.value) <= 0
            if not caught:
                return False
        self.value = value
        return True

    def set_immediately(self, value: float) -> None:
        """Set the value without the takeover mode (``setImmediately``)."""
        self.value = self._clamp(value)
        self._control = self.value

    def inc(self, delta: float) -> None:
        """Change the value by a normalized delta (``inc``)."""
        self.value = self._clamp(self.value + delta)

    def reset(self) -> None:
        """Set the default value (``reset``)."""
        self.value = self.default

    def touch(self, is_touched: bool) -> None:
        self.touched = bool(is_touched)

    def set_indication(self, is_indicated: bool) -> None:
        self.indicated = bool(is_indicated)

    def displayed(self) -> str:
        return self.formatter(self.value)

    def displayed_limited(self, limit: int) -> str:
        return limit_text(self.displayed(), limit)
