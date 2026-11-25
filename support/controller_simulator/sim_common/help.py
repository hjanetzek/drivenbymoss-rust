# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The help of the controls (doc/CONTROLLER_HELP_PLAN.md), without Qt: the socket the host writes
the core's help entries to, and the model which maps them to the simulator's controls.

The host (``tools/dbm_host_alsa.py``) connects to the UNIX stream socket and writes one JSON object
per line, as ``dbm_take_control_help`` returns them: ``id``, ``label``, ``bind`` (``"cc"``,
``"note"``, ... or null), ``channel``, ``control``, ``pressed``, ``presses``, ``scope`` (``global``,
``view``, ``mode``, ``modifier``: the colour of the box), ``members`` (a row of buttons: one box
around all of them), ``short``, ``long``; ``@view`` and ``@mode`` carry the names of the active
view and mode (the legend). Only changes
come; after a connect the host sends all entries. An empty ``short`` removes the help of a control.

``HelpModel`` keeps the entries and turns them into the boxes of the QML (``items()``): the
control's key, its rectangle in surface units (for the leader line and the highlight), the side of
the panel the box goes to, and the texts. A simulator gives it a ``resolve(entry)`` function which
returns the target of an entry (a ``HelpTarget``) or None (a control the simulator doesn't show).
"""

import json
import os
import socket
import threading
from dataclasses import dataclass
from typing import Callable

SOCKET_ENV = "DBM_SIM_HELP_SOCKET"
DEFAULT_SOCKET_NAME = "dbm-sim-help.sock"
SIDES = ("left", "right", "top", "bottom")


def default_socket_path() -> str:
    """``$DBM_SIM_HELP_SOCKET``, else ``$XDG_RUNTIME_DIR/dbm-sim-help.sock``."""
    path = os.environ.get(SOCKET_ENV)
    if path:
        return path
    runtime = os.environ.get("XDG_RUNTIME_DIR") or f"/tmp/dbm-{os.getuid()}"
    return os.path.join(runtime, DEFAULT_SOCKET_NAME)


@dataclass
class HelpTarget:
    """Where the box of an entry points to: the control's key (-1 for an area like the pads), its
    rectangle in surface units and the side of the panel (None: the nearest)."""

    key: int
    x: float
    y: float
    w: float
    h: float
    side: str | None = None
    label: str = ""
    """The name in the box (empty: the entry's label)."""


def nearest_side(target: HelpTarget, centre: tuple[float, float, float, float], surface: tuple[float, float]) -> str:
    """The side of a control: left / right of the central area (e.g. the pads), else above or
    below it; without a central area the nearest edge of the surface."""
    if target.side in SIDES:
        return target.side
    cx, cy = target.x + target.w / 2, target.y + target.h / 2
    x0, y0, x1, y1 = centre
    if x1 > x0:
        if target.x + target.w <= x0:
            return "left"
        if target.x >= x1:
            return "right"
        return "top" if cy < (y0 + y1) / 2 else "bottom"
    width, height = surface
    distances = {"left": cx, "right": width - cx, "top": cy, "bottom": height - cy}
    return min(distances, key=distances.get)


def anchor(target: HelpTarget, side: str) -> tuple[float, float]:
    """The point the leader line goes to: the centre of a control, the edge of an area facing the
    box (the pads, the strips)."""
    cx, cy = target.x + target.w / 2, target.y + target.h / 2
    if target.key >= 0:
        return cx, cy
    return {"left": (target.x, cy), "right": (target.x + target.w, cy), "top": (cx, target.y), "bottom": (cx, target.y + target.h)}[side]


SCOPES = ("global", "view", "mode", "modifier")


def row_label(labels: list[str]) -> str:
    """The name of a row of controls from the first and the last name: "Scene 1", "Scene 8" ->
    "Scene 1–8"."""
    first, last = labels[0], labels[-1]
    common = 0
    while common < min(len(first), len(last)) and first[common] == last[common]:
        common += 1
    # Not into a word ("Scene 1" / "Scene 12": "Scene 1–12")
    while common > 0 and first[common - 1].isalnum():
        common -= 1
    return f"{first[:common]}{first[common:]}–{last[common:]}"


class HelpModel:
    """The help entries by id and the boxes for the QML."""

    def __init__(self, resolve: Callable[[dict], HelpTarget | None], surface: tuple[float, float],
                 centre: tuple[float, float, float, float] = (0, 0, 0, 0)):
        self.resolve = resolve
        self.surface = surface
        self.centre = centre
        self.entries: dict[str, dict] = {}
        self.revision = 0
        # The id of the box which shows the full text
        self.expanded: str | None = None
        # The names of the active view and mode (the entries @view and @mode)
        self.context: dict[str, str] = {}
        # Hide the boxes of the global functions (only what depends on view, mode, modifiers)
        self.hide_global = False
        # Where the line of an area meets its edge, by the entry id (surface units along the
        # edge; default the middle): e.g. into a gap between the controls the line passes
        self.area_anchors: dict[str, float] = {}

    def clear(self) -> None:
        self.entries.clear()
        self.context.clear()
        self.expanded = None
        self.revision += 1

    def apply(self, entry: dict) -> None:
        """One changed entry of the host. A control which becomes pressed expands its box."""
        entry_id = entry.get("id")
        if not entry_id:
            return
        self.revision += 1
        if entry_id.startswith("@"):
            self.context[entry_id[1:]] = entry.get("short", "")
            return
        previous = self.entries.get(entry_id)
        if entry.get("short"):
            self.entries[entry_id] = entry
            was_pressed = bool(previous and previous.get("pressed"))
            presses = entry.get("presses", 0) - (previous.get("presses", 0) if previous else entry.get("presses", 0))
            if (entry.get("pressed") and not was_pressed) or presses > 0:
                self.expanded = entry_id
        else:
            self.entries.pop(entry_id, None)
            if self.expanded == entry_id:
                self.expanded = None

    def apply_line(self, line: str) -> bool:
        try:
            entry = json.loads(line)
        except ValueError:
            return False
        if not isinstance(entry, dict):
            return False
        self.apply(entry)
        return True

    def toggle_expanded(self, entry_id: str) -> None:
        self.expanded = None if self.expanded == entry_id else entry_id
        self.revision += 1

    def expand(self, entry_id: str) -> None:
        if entry_id in self.entries and self.expanded != entry_id:
            self.expanded = entry_id
            self.revision += 1

    def set_hide_global(self, hide: bool) -> None:
        if hide != self.hide_global:
            self.hide_global = hide
            self.revision += 1

    def target(self, entry: dict) -> tuple[HelpTarget | None, list[HelpTarget]]:
        """The target of an entry; a row of buttons (``members``) is the area around them."""
        members = entry.get("members")
        if not members:
            return self.resolve(entry), []
        parts = [t for t in (self.resolve(member) for member in members) if t is not None]
        if not parts:
            return None, []
        x0, y0 = min(t.x for t in parts), min(t.y for t in parts)
        x1, y1 = max(t.x + t.w for t in parts), max(t.y + t.h for t in parts)
        labels = [t.label for t in parts if t.label]
        label = row_label(labels) if len(labels) == len(parts) and len(parts) > 1 else ""
        side = parts[0].side or nearest_side(parts[0], self.centre, self.surface)
        return HelpTarget(-1, x0, y0, x1 - x0, y1 - y0, side=side, label=label), parts

    def items(self) -> list[dict]:
        """The boxes: id, key, label, short, long, pressed, expanded, scope, side, the control's
        rectangle (x, y, w, h; for a row the rectangles of its buttons in ``parts``) and the anchor
        of the line (ax, ay), in surface units; sorted by the side and the position along it."""
        result = []
        for entry_id, entry in self.entries.items():
            scope = entry.get("scope", "global")
            if self.hide_global and scope == "global":
                continue
            target, parts = self.target(entry)
            if target is None:
                continue
            side = nearest_side(target, self.centre, self.surface)
            ax, ay = anchor(target, side)
            if target.key < 0 and entry_id in self.area_anchors:
                if side in ("top", "bottom"):
                    ax = self.area_anchors[entry_id]
                else:
                    ay = self.area_anchors[entry_id]
            result.append({"id": entry_id, "key": target.key, "label": target.label or entry.get("label", ""), "short": entry.get("short", ""),
                           "long": entry.get("long", ""), "pressed": bool(entry.get("pressed")), "expanded": entry_id == self.expanded,
                           "scope": scope if scope in SCOPES else "global", "side": side, "x": target.x, "y": target.y, "w": target.w,
                           "h": target.h, "ax": ax, "ay": ay, "area": target.key < 0,
                           "parts": [{"x": t.x, "y": t.y, "w": t.w, "h": t.h} for t in parts]})
        result.sort(key=lambda item: (SIDES.index(item["side"]), item["ay"] if item["side"] in ("left", "right") else item["ax"],
                                      item["ax"] if item["side"] in ("left", "right") else item["ay"]))
        return result


def spread(desired: list[float], size: float, gap: float, low: float, high: float) -> list[float]:
    """The start positions of boxes of ``size`` along a side, at least ``gap`` apart and in the
    order of ``desired`` (their wished starts, sorted): the least squares fit to the wishes (the
    boxes of a crowded stretch are moved up and down evenly, not all pushed one way; "pool
    adjacent violators" on the wishes minus the spacing), then kept within [low, high] if they
    fit there (else from ``low`` on). HelpLayer.qml does the same; this one is tested."""
    step = size + gap
    count = len(desired)
    blocks: list[list[float]] = []
    for index, wish in enumerate(desired):
        blocks.append([wish - index * step, 1])
        while len(blocks) > 1 and blocks[-2][0] / blocks[-2][1] > blocks[-1][0] / blocks[-1][1]:
            total, number = blocks.pop()
            blocks[-1][0] += total
            blocks[-1][1] += number
    fitted: list[float] = []
    for total, number in blocks:
        fitted += [total / number] * int(number)
    positions = [fitted[index] + index * step for index in range(count)]
    if count * step - gap > high - low:
        return [low + index * step for index in range(count)]
    positions = [max(position, low + index * step) for index, position in enumerate(positions)]
    return [min(position, high - (count - index) * step + gap) for index, position in enumerate(positions)]


class HelpServer:
    """Listens on a UNIX stream socket, one connection at a time; hands every line to
    ``on_line`` (from its thread) and calls ``on_connect(True/False)``."""

    def __init__(self, path: str, on_line: Callable[[str], None], on_connect: Callable[[bool], None] | None = None):
        self.path = path
        self.on_line = on_line
        self.on_connect = on_connect
        self.connected = False
        self._stop = threading.Event()
        self._sock: socket.socket | None = None
        self._thread: threading.Thread | None = None

    def start(self) -> None:
        directory = os.path.dirname(self.path)
        if directory:
            os.makedirs(directory, exist_ok=True)
        if os.path.exists(self.path):
            os.unlink(self.path)
        self._sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self._sock.bind(self.path)
        self._sock.listen(1)
        self._sock.settimeout(0.2)
        self._thread = threading.Thread(target=self._run, name="help-server", daemon=True)
        self._thread.start()

    def _run(self) -> None:
        while not self._stop.is_set():
            try:
                connection, _ = self._sock.accept()
            except socket.timeout:
                continue
            except OSError:
                return
            self.connected = True
            if self.on_connect:
                self.on_connect(True)
            connection.settimeout(0.2)
            buffer = b""
            with connection:
                while not self._stop.is_set():
                    try:
                        data = connection.recv(65536)
                    except socket.timeout:
                        continue
                    except OSError:
                        break
                    if not data:
                        break
                    buffer += data
                    while b"\n" in buffer:
                        line, buffer = buffer.split(b"\n", 1)
                        if line.strip():
                            self.on_line(line.decode("utf-8", "replace"))
            self.connected = False
            if self.on_connect:
                self.on_connect(False)

    def stop(self) -> None:
        self._stop.set()
        if self._sock is not None:
            self._sock.close()
        if self._thread is not None:
            self._thread.join(timeout=1.0)
        try:
            os.unlink(self.path)
        except OSError:
            pass
