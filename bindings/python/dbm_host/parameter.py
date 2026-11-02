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
  (``tests/conformance/*/replay/*.rec``): volumes ``-10.0 dB`` / ``-Inf dB``, panorama ``0.00 %``, ``56.3 %``, ``-100 %``
  (``format_pan``).
  The volume curve is ``dB = 20 * log10(2 * v**3)`` (0 dB at 0.793701, +6 dB at 1), which gives the
  reported pairs (0.540742 = -10.0 dB, 0.793701 = 0.0 dB). The panorama text is the signed percent
  of the deviation from the center with three significant digits (``format_pan`` lists the
  observed values).
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
    auto_arm: bool = False
    """Selecting an instrument track arms it, and disarms the track it armed before (Bitwig's
    auto-arm of the selected track for the keyboard input)."""


def format_volume(value: float) -> str:
    """The displayed value of a volume or send (see the module docstring)."""
    if value <= 0:
        return "-Inf dB"
    return f"{20.0 * math.log10(2.0 * value ** 3):.1f} dB"


def format_three_digits(number: float) -> str:
    """A number with three significant digits, at most two decimals (``0.79``, ``56.3``,
    ``-100``, ``1.28``, ``160``). The number of decimals depends on the rounded number:
    9999.88 Hz is ``10.0 kHz`` (an EQ+ frequency Bitwig showed, ``format_eq_frequency``)."""
    for decimals in (2, 1):
        text = f"{number:.{decimals}f}"
        if abs(float(text)) < 10.0 ** (3 - decimals):
            return text
    return f"{number:.0f}"


def format_pan(value: float) -> str:
    """The displayed value of a panorama: the deviation from the center in percent, with three
    significant digits (``format_three_digits``): ``0.00 %``, ``0.79 %``, ``56.3 %``,
    ``-100 %``.

    Bitwig does not document the format. The rule fits all values Bitwig showed in the
    recordings (the ``/…/pan/str`` model lines of ``tests/conformance/*/replay/*-rust.rec`` and
    ``osc:/…/panStr`` of the Java traces ``target/conformance/*-java.json``, 2026-11-02):

    | value | text |
    | --- | --- |
    | 0.0 | ``-100 %`` |
    | 0.5 | ``0.00 %`` |
    | 0.503937 (64/127) | ``0.79 %`` |
    | 0.781298 | ``56.3 %`` |

    The center is ``0.00 %``, never ``-0.00 %``. That the border between two and one decimals
    is decided on the rounded value is known from the EQ+ frequencies, not from a panorama.
    """
    percent = (value * 2.0 - 1.0) * 100.0
    if abs(percent) < 0.005:
        return "0.00 %"
    return format_three_digits(percent) + " %"


def format_percent(value: float) -> str:
    return f"{value * 100.0:.0f} %"


def format_metronome_volume(value: float) -> str:
    """The metronome volume, -48..0 dB linear (Bitwig reported 0.75 as -12.0 dB)."""
    return f"{(value - 1.0) * 48.0:.1f} dB"


def format_percent_three_digits(value: float) -> str:
    """A value 0..1 as percent with three significant digits (``0.00 %``, ``50.0 %``, ``100 %``),
    the format of many of Bitwig's device parameters."""
    return format_three_digits(value * 100.0) + " %"


def format_semitones(range_semitones: float) -> Callable[[float], str]:
    """A bipolar pitch of +-range semitones with a sign and two decimals (``-7.00 st``,
    ``+4.02 st``, ``0.00 st``): the Osc1Pitch of the Polysynth (7 semitones) as Bitwig showed it
    (``/device/param/0/value/str`` of tests/conformance/generic_flexi/replay/
    generic_flexi-device-rust.rec at 0, 0.787402, 0.5 and 1)."""
    def format_value(value: float) -> str:
        semitones = (value - 0.5) * 2.0 * range_semitones
        return "0.00 st" if abs(semitones) < 0.005 else f"{semitones:+.2f} st"
    return format_value


def recorded_formatter(recorded_value: float, recorded_text: str, formatter: Callable[[float], str] | None = None) -> Callable[[float], str]:
    """The displayed value of a parameter whose curve is known only at one point (the value of
    the fixture): the recorded text at the recorded value, else the formatter (by default the
    percent of ``format_percent_three_digits``, which is only a guess for other units)."""
    fallback = formatter or format_percent_three_digits

    def format_value(value: float) -> str:
        if abs(value - recorded_value) < 1e-6:
            return recorded_text
        return fallback(value)
    return format_value


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
        self.listeners: list[Callable[[float], None]] = []
        """Called with the new value after every change (also by the takeover mode and undo)."""
        # The last value of the control which set this value (for the takeover mode catch)
        self._control: float | None = None

    @staticmethod
    def _clamp(value: float) -> float:
        return min(1.0, max(0.0, float(value)))

    @property
    def value(self) -> float:
        return self._value

    @value.setter
    def value(self, value: float) -> None:
        changed = getattr(self, "_value", None) != value
        self._value = value
        if changed:
            for listener in getattr(self, "listeners", []):
                listener(value)

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
