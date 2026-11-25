# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
"""The host's parameter pages, simulated (doc/HOST_PAGE_PLAN.md §3.4, §3.5, §12): a control grid
as zynthian's (zynthian-ui ``zyngui/zynthian_control_grid_data.py``) with a simulated **screen**.

The reference for the zynthian adapter: the same reports (``/hostpage/…``) and the same command
handling (``hostpage …``).

- **The content** (``HostPages``, shared by all bridges of a ``SimDaw``): a chain per track (the
  selected track is the shown chain), each with its sections (the chain's controllers, then the
  processors), each section with its pages of up to 4 controllers (``Control``: zynthian's zctrl
  kinds float, integer, list, toggle, trigger, with value texts).
- **The screen** (``HostPages.screen``): the selected page (a control set) and controller, as
  zynthian's screen shows them. It is the host's state: the JAM's input changes it through the
  commands, and ``select_on_screen`` changes it as zynthian's touch or its own encoders would.
  The core shows only what is reported, so the JAM follows either way.
- **The view of a bridge** (``HostPageView``): the layout for the shape of ``hostpage show``
  (8 columns: two pages a row, as ``grid8x6``; else one, as ``grid4x6``), the windows of 6 rows
  (the reported pages), the reports and the commands. Cell commands of another epoch are
  dropped.
- **Value math** (the host's): an encoder detent is zynthian's ``nudge`` (a list one item per
  click, floats by 1/100 of the range); a strip's travel moves the normalized value by
  delta/127 (fine: a tenth), stepped controllers accumulate and step at 1/(items - 1).
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import Callable

from drivenbymoss import protocol
from drivenbymoss.protocol import Cmd

M = protocol.Msg

# The cell kinds of /hostpage/cell/<c>/kind
KIND_EMPTY, KIND_FADER, KIND_LIST, KIND_TOGGLE, KIND_TRIGGER, KIND_RELATIVE, KIND_ACTION = range(7)
CELLS_PER_SET = 4
WINDOW_ROWS = 6
TRAVEL = 127.0
FINE = 10.0

Color = tuple[int, int, int]


@dataclass
class Control:
    """A controller (a zctrl): a float, an integer, a list, a toggle or a trigger."""

    name: str
    value: float = 0.0
    minimum: float = 0.0
    maximum: float = 1.0
    labels: list[str] | None = None
    toggle: bool = False
    trigger: bool = False
    integer: bool = False
    unit: str = ""
    default: float | None = None
    fmt: Callable[[float], str] | None = None
    fired: int = 0
    """How often a trigger fired (tests)."""
    _travel: float = 0.0
    """The accumulated travel of a stepped controller."""

    def __post_init__(self):
        if self.labels:
            self.minimum, self.maximum = 0, len(self.labels) - 1
        if self.default is None:
            self.default = self.value

    @property
    def kind(self) -> int:
        if self.trigger:
            return KIND_TRIGGER
        if self.toggle:
            return KIND_TOGGLE
        if self.labels:
            return KIND_LIST
        return KIND_FADER

    @property
    def steps(self) -> int:
        if self.labels:
            return len(self.labels)
        if self.toggle:
            return 2
        if self.integer:
            return int(self.maximum - self.minimum) + 1
        return 0

    @property
    def fraction(self) -> float:
        span = self.maximum - self.minimum
        return 0.0 if span <= 0 else min(1.0, max(0.0, (self.value - self.minimum) / span))

    @property
    def text(self) -> str:
        if self.trigger:
            return "Fire"
        if self.toggle:
            return "On" if self.value > self.minimum else "Off"
        if self.labels:
            return self.labels[int(self.value)]
        if self.fmt is not None:
            return self.fmt(self.value)
        if self.integer:
            return f"{int(self.value)}{self.unit}"
        return f"{self.value:.3g}{self.unit}"

    def set(self, value: float) -> None:
        value = min(self.maximum, max(self.minimum, value))
        self.value = float(round(value)) if self.integer or self.labels or self.toggle else value

    def nudge(self, delta: int, fine: bool = False) -> None:
        """An encoder detent (zynthian_controller.nudge): lists and integers by steps, toggles
        on (+) or off (-), floats by 1/100 of the range (fine: 1/1000)."""
        if self.trigger:
            return
        if self.toggle:
            self.set(self.maximum if delta > 0 else self.minimum)
        elif self.labels or self.integer:
            self.set(self.value + delta)
        else:
            self.set(self.value + delta * (self.maximum - self.minimum) / (1000.0 if fine else 100.0))
        self._travel = 0.0

    def turn_travel(self, delta: int, fine: bool = False) -> None:
        """A strip's travel: the normalized value by delta/127 (fine: a tenth); a stepped
        controller accumulates and steps at 1/(steps - 1) of the travel."""
        if self.trigger:
            return
        amount = delta / TRAVEL / (FINE if fine else 1.0)
        span = self.maximum - self.minimum
        steps = self.steps
        if steps > 1:
            if self._travel * amount < 0:
                self._travel = 0.0
            self._travel += amount
            step = 1.0 / (steps - 1)
            count = int(self._travel / step)
            if count:
                self._travel -= count * step
                self.set(self.value + count * span / (steps - 1))
        else:
            self.set(self.value + amount * span)

    def press(self) -> None:
        """A tap: a toggle switches, a trigger fires, a list steps to its next item (wrapping)."""
        if self.toggle:
            self.set(self.minimum if self.value > self.minimum else self.maximum)
        elif self.trigger:
            self.fired += 1
        elif self.labels:
            self.value = float((int(self.value) + 1) % len(self.labels))

    def reset(self) -> None:
        self.set(self.default)


@dataclass
class ControlPage:
    """A zynthian controller page: up to 4 controllers (None: a gap)."""

    title: str
    controls: list[Control | None]


@dataclass
class Processor:
    """A section: the chain's controllers or a processor, with its pages."""

    name: str
    color: Color
    pages: list[ControlPage]


@dataclass
class Chain:
    name: str
    sections: list[Processor]


def hz(value: float) -> str:
    return f"{value / 1000:.2f} kHz" if value >= 1000 else f"{value:.0f} Hz"


def ms(value: float) -> str:
    return f"{value / 1000:.2f} s" if value >= 1000 else f"{value:.0f} ms"


def db(value: float) -> str:
    return "-inf" if value <= -60 else f"{value:+.1f} dB"


def percent(value: float) -> str:
    return f"{value * 100:.0f} %"


def pan(value: float) -> str:
    return "C" if abs(value) < 0.005 else f"{'L' if value < 0 else 'R'}{abs(value) * 100:.0f}"


# Section colours (as zynthian's themes give each section its own)
CHAIN_COLOR = (200, 200, 200)
PALETTE = [(255, 80, 0), (0, 160, 255), (40, 220, 80), (230, 0, 160), (255, 200, 0), (140, 60, 255)]

WAVES = ["Sine", "Triangle", "Saw", "Square", "Noise"]


def chain_controllers() -> Processor:
    return Processor("Chain", CHAIN_COLOR, [ControlPage("Mixer", [Control("Level", -6.0, -60, 6, fmt=db), Control("Balance", 0.0, -1, 1, fmt=pan),
                                                                     Control("Mute", 0, 0, 1, toggle=True), Control("Solo", 0, 0, 1, toggle=True)])])


def synth_chain(name: str, synth: str) -> Chain:
    """A synth chain: the chain's controllers, a synth with 8 pages, a reverb with 2."""
    osc = lambda n, wave: ControlPage(f"Osc {n}", [Control("Wave", wave, labels=WAVES), Control("Transpose", 0, -24, 24, integer=True, unit=" st"),  # noqa: E731
                                                   Control("Tune", 0.0, -1, 1, fmt=lambda v: f"{v * 100:+.0f} ct"), Control("Volume", 0.7, fmt=percent)])
    synth_pages = [
        osc(1, 2), osc(2, 3),
        ControlPage("Filter", [Control("Cutoff", 2000.0, 20, 20000, fmt=hz), Control("Resonance", 0.3, fmt=percent),
                               Control("Drive", 0.0, fmt=percent), Control("Style", 0, labels=["12 dB", "24 dB", "Band", "High"])]),
        ControlPage("Filter env", [Control("Attack", 5.0, 0, 5000, fmt=ms), Control("Decay", 300.0, 0, 5000, fmt=ms),
                                   Control("Sustain", 0.5, fmt=percent), Control("Release", 400.0, 0, 5000, fmt=ms)]),
        ControlPage("Amp env", [Control("Attack", 2.0, 0, 5000, fmt=ms), Control("Decay", 600.0, 0, 5000, fmt=ms),
                                Control("Sustain", 0.8, fmt=percent), Control("Release", 800.0, 0, 5000, fmt=ms)]),
        ControlPage("LFO", [Control("Rate", 2.0, 0.05, 20, fmt=lambda v: f"{v:.2f} Hz"), Control("Depth", 0.2, fmt=percent),
                            Control("Wave", 0, labels=WAVES), Control("Sync", 1, 0, 1, toggle=True)]),
        ControlPage("Voices", [Control("Polyphony", 8, 1, 32, integer=True), Control("Glide", 0.0, 0, 2000, fmt=ms),
                               Control("Legato", 0, 0, 1, toggle=True), Control("Panic", 0, 0, 1, trigger=True)]),
        ControlPage("Unison", [Control("Voices", 1, 1, 8, integer=True), Control("Detune", 0.1, fmt=percent),
                               Control("Spread", 0.5, fmt=percent), Control("Mode", 0, labels=["Off", "Stack", "Spread"])]),
        ControlPage("Arpeggiator", [Control("On", 0, 0, 1, toggle=True), Control("Pattern", 0, labels=["Up", "Down", "Up/Down", "Random"]),
                                    Control("Octaves", 1, 1, 4, integer=True), Control("Gate", 0.5, fmt=percent)]),
        ControlPage("Mod wheel", [Control("Target", 0, labels=["Cutoff", "LFO depth", "Volume"]), Control("Amount", 0.5, fmt=percent), None, None]),
        ControlPage("Master", [Control("Volume", -3.0, -60, 6, fmt=db), Control("Pan", 0.0, -1, 1, fmt=pan), None, None]),
    ]
    reverb = [ControlPage("Room", [Control("Size", 0.6, fmt=percent), Control("Damping", 0.4, fmt=percent),
                                   Control("Width", 1.0, fmt=percent), Control("Wet", 0.25, fmt=percent)]),
              ControlPage("Mode", [Control("Freeze", 0, 0, 1, toggle=True), Control("Pre-delay", 20.0, 0, 200, fmt=ms), None, None])]
    return Chain(name, [chain_controllers(), Processor(synth, PALETTE[0], synth_pages), Processor("Reverb", PALETTE[1], reverb)])


def drum_chain(name: str) -> Chain:
    kit = [ControlPage("Kit", [Control("Kit", 1, labels=["Standard", "Room", "Power", "Electronic", "TR-808"]),
                               Control("Volume", 0.8, fmt=percent), Control("Reverb", 0.2, fmt=percent), Control("Chorus", 0.0, fmt=percent)])]
    comp = [ControlPage("Compressor", [Control("Threshold", -18.0, -60, 0, fmt=db), Control("Ratio", 4.0, 1, 20, fmt=lambda v: f"{v:.1f}:1"),
                                       Control("Attack", 10.0, 0, 200, fmt=ms), Control("Release", 120.0, 0, 2000, fmt=ms)])]
    return Chain(name, [chain_controllers(), Processor("FluidSynth", PALETTE[2], kit), Processor("Compressor", PALETTE[3], comp)])


def audio_chain(name: str) -> Chain:
    eq = [ControlPage("Low", [Control("Freq", 120.0, 20, 1000, fmt=hz), Control("Gain", 0.0, -18, 18, fmt=db), Control("Q", 0.7, 0.1, 10), None]),
          ControlPage("High", [Control("Freq", 6000.0, 1000, 20000, fmt=hz), Control("Gain", 0.0, -18, 18, fmt=db), Control("Q", 0.7, 0.1, 10),
                               Control("Bypass", 0, 0, 1, toggle=True)])]
    return Chain(name, [chain_controllers(), Processor("EQ", PALETTE[4], eq)])


@dataclass
class Screen:
    """The host screen's selection: a page (global index in the chain's pages) and a controller
    of it (0..3)."""

    page: int = 0
    slot: int = 0


class HostPages:
    """The simulated host's pages: one chain per track of the DAW (the selected track is shown)
    and the screen's selection."""

    def __init__(self, daw, chains: list[Chain] | None = None):
        self.daw = daw
        if chains is None:
            chains = []
            for track in daw.tracks:
                if "drum" in track.name.lower():
                    chains.append(drum_chain(track.name))
                elif track.type == "AUDIO":
                    chains.append(audio_chain(track.name))
                else:
                    chains.append(synth_chain(track.name, "Helm" if len(chains) % 2 == 0 else "ZynAddSubFX"))
        self.chains = chains
        self.screens = [Screen() for _ in chains]
        self.generation = 0
        """Counts the changes of the screen and the chains' structure (the views report them)."""

    @property
    def chain_index(self) -> int:
        tracks = self.daw.tracks
        selected = next((i for i, t in enumerate(tracks) if t.selected), 0)
        return min(selected, len(self.chains) - 1)

    @property
    def chain(self) -> Chain:
        return self.chains[self.chain_index]

    @property
    def screen(self) -> Screen:
        return self.screens[self.chain_index]

    def pages(self) -> list[tuple[int, ControlPage]]:
        """The chain's pages in order: (section index, page)."""
        return [(s, page) for s, section in enumerate(self.chain.sections) for page in section.pages]

    def select_on_screen(self, page: int, slot: int = 0) -> None:
        """Select a page and its controller on the screen (zynthian's touch or encoders)."""
        count = len(self.pages())
        page = max(0, min(count - 1, page))
        screen = self.screen
        if (screen.page, screen.slot) != (page, slot):
            screen.page, screen.slot = page, max(0, min(CELLS_PER_SET - 1, slot))
            self.generation += 1

    def step_on_screen(self, delta: int) -> None:
        """The previous / next page on the screen (zynthian's ARROW_UP / ARROW_DOWN)."""
        self.select_on_screen(self.screen.page + delta)


class HostPageView:
    """The pages of one bridge: the layout for its surface, the reports and the commands."""

    def __init__(self, pages: HostPages):
        self.pages = pages
        self.shown = False
        self.columns = 8
        self.epoch = 0
        self._structure = None
        self._generation = None
        self.dropped = 0
        """Cell commands of another epoch (tests)."""

    # Layout

    @property
    def sets_per_row(self) -> int:
        return self.columns // CELLS_PER_SET

    @property
    def sets_per_window(self) -> int:
        return self.sets_per_row * WINDOW_ROWS

    @property
    def window(self) -> int:
        return self.pages.screen.page // self.sets_per_window

    def window_count(self) -> int:
        return max(1, math.ceil(len(self.pages.pages()) / self.sets_per_window))

    def shown_sets(self) -> list[tuple[int, int, ControlPage]]:
        """The pages of the window: (global index, section index, page)."""
        first = self.window * self.sets_per_window
        all_pages = self.pages.pages()
        return [(first + i, s, page) for i, (s, page) in enumerate(all_pages[first:first + self.sets_per_window])]

    def cell_of(self, cell: int) -> tuple[int, Control | None] | None:
        """The global page index and the controller of a cell, None if no page is there."""
        row, column = divmod(cell, self.columns)
        index = row * self.sets_per_row + column // CELLS_PER_SET
        sets = self.shown_sets()
        if not 0 <= row < WINDOW_ROWS or index >= len(sets):
            return None
        global_index, _, page = sets[index]
        slot = column % CELLS_PER_SET
        return global_index, (page.controls[slot] if slot < len(page.controls) else None)

    def _check_structure(self) -> None:
        """A new epoch when the cells hold other controllers (chain, window, layout)."""
        structure = (self.pages.chain_index, self.window, self.columns)
        if structure != self._structure:
            self._structure = structure
            self.epoch = (self.epoch + 1) & 0xFFFF

    # Reports

    def report(self, put) -> None:
        """Report the page (change-only through ``put``); nothing while not shown."""
        if not self.shown:
            return
        self._check_structure()
        pages = self.pages
        chain = pages.chain
        screen = pages.screen
        sets = self.shown_sets()
        put(M.HOST_PAGE_EPOCH, self.epoch)
        put(M.HOST_PAGE_EXISTS, True)
        put(M.HOST_PAGE_TITLE, f"Host · {chain.name}")
        put(M.HOST_PAGE_COLUMNS, self.columns)
        put(M.HOST_PAGE_ROWS, WINDOW_ROWS)
        all_pages = pages.pages()
        windows = self.window_count()
        put(M.HOST_PAGE_COUNT, windows)
        for w in range(windows):
            first = all_pages[w * self.sets_per_window]
            put(M.HOST_PAGE_NAME | w << 16, f"{chain.sections[first[0]].name} {first[1].title}")
        put(M.HOST_PAGE_SELECTED, self.window)
        put(M.HOST_SECTION_COUNT, len(chain.sections))
        for s, section in enumerate(chain.sections):
            put(M.HOST_SECTION_NAME | s << 16, section.name)
            put(M.HOST_SECTION_COLOR | s << 16, section.color)
        put(M.HOST_SET_COUNT, len(sets))
        selected_set = -1
        for i, (global_index, s, page) in enumerate(sets):
            put(M.HOST_SET_NAME | i << 16, page.title)
            put(M.HOST_SET_SECTION | i << 16, s)
            put(M.HOST_SET_COLOR | i << 16, chain.sections[s].color)
            if global_index == screen.page:
                selected_set = i
        put(M.HOST_SET_SELECTED, selected_set)
        put(M.HOST_SECTION_SELECTED, all_pages[screen.page][0] if all_pages else -1)
        selected_cell = -1
        for cell in range(self.columns * WINDOW_ROWS):
            base = cell << 16
            place = self.cell_of(cell)
            control = place[1] if place else None
            index = cell // self.columns * self.sets_per_row + cell % self.columns // CELLS_PER_SET
            if control is None:
                put(M.HOST_CELL_KIND | base, KIND_EMPTY)
                put(M.HOST_CELL_NAME | base, "")
                put(M.HOST_CELL_ENABLED | base, False)
                put(M.HOST_CELL_SET | base, index if place else -1)
                continue
            section = chain.sections[sets[index][1]]
            put(M.HOST_CELL_KIND | base, control.kind)
            put(M.HOST_CELL_NAME | base, control.name)
            put(M.HOST_CELL_COLOR | base, section.color)
            put(M.HOST_CELL_VALUE | base, control.fraction)
            put(M.HOST_CELL_TEXT | base, control.text)
            put(M.HOST_CELL_STEPS | base, control.steps)
            put(M.HOST_CELL_ON | base, control.toggle and control.value > control.minimum)
            put(M.HOST_CELL_ENABLED | base, True)
            put(M.HOST_CELL_SET | base, index)
            if place[0] == screen.page and cell % CELLS_PER_SET == screen.slot:
                selected_cell = cell
        put(M.HOST_PAGE_ROW, (selected_set // self.sets_per_row) if selected_set >= 0 else 0)
        put(M.HOST_CELL_SELECTED, selected_cell)

    # Commands

    def show(self, columns: int, on: bool, forget: Callable[[], None]) -> None:
        """``hostpage show``: the layout for the surface (8 columns: two pages a row), a dump."""
        self.shown = bool(on)
        if on:
            self.columns = 8 if columns >= 8 else CELLS_PER_SET
            forget()

    def _control(self, cell: int, epoch: int):
        if epoch != self.epoch:
            self.dropped += 1
            return None
        return self.cell_of(cell)

    def turn(self, cell: int, epoch: int, unit: int, fine: bool, delta: int) -> bool:
        place = self._control(cell, epoch)
        if place is not None and place[1] is not None:
            if unit == protocol.TurnUnit.DETENT:
                place[1].nudge(delta, fine)
            else:
                place[1].turn_travel(delta, fine)
        return True

    def press(self, cell: int, epoch: int, pressed: bool) -> bool:
        place = self._control(cell, epoch)
        if place is None or not pressed:
            return True
        global_index, control = place
        # A tap selects the page and the controller on the screen, then acts by its kind
        self.pages.select_on_screen(global_index, cell % self.columns % CELLS_PER_SET)
        if control is not None:
            control.press()
        return True

    def reset(self, cell: int, epoch: int) -> bool:
        place = self._control(cell, epoch)
        if place is not None and place[1] is not None:
            place[1].reset()
        return True

    def select_row(self, row: int) -> bool:
        """The page in the row (with two a row: the same half as before)."""
        half = self.pages.screen.page % self.sets_per_row
        index = self.window * self.sets_per_window + row * self.sets_per_row + half
        count = len(self.pages.pages())
        if 0 <= row < WINDOW_ROWS and count:
            self.pages.select_on_screen(min(index, count - 1))
        return True

    def select_window(self, window: int, last: bool = False) -> bool:
        """Show the window: select its first page, or its last one (``<`` from a window's first
        set, ``HostPagePage`` with ``PageEnd.LAST``)."""
        if 0 <= window < self.window_count():
            first = window * self.sets_per_window
            if last:
                count = len(self.pages.pages())
                first = min(first + self.sets_per_window, count) - 1
            self.pages.select_on_screen(first)
        return True

    def select_set(self, index: int) -> bool:
        sets = self.shown_sets()
        if 0 <= index < len(sets):
            self.pages.select_on_screen(sets[index][0])
        return True

    def select_section(self, section: int) -> bool:
        first = next((i for i, (s, _) in enumerate(self.pages.pages()) if s == section), None)
        if first is not None:
            self.pages.select_on_screen(first)
        return True


HOST_PAGE_COMMANDS = (Cmd.HOST_PAGE_SHOW, Cmd.HOST_PAGE_CELL_TURN, Cmd.HOST_PAGE_CELL_PRESS, Cmd.HOST_PAGE_CELL_TOUCH,
                      Cmd.HOST_PAGE_CELL_RESET, Cmd.HOST_PAGE_ROW, Cmd.HOST_PAGE_PAGE, Cmd.HOST_PAGE_SET, Cmd.HOST_PAGE_SECTION)
