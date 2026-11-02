# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The cue markers of the arranger.

Implements the DAW side of HOST_BRIDGE_GUIDE.md §3 "Markers" (``/marker/<i>/exists|number|name|
color|position``, ``/marker/count``) for the Reporter and the commands of §4.8 (``marker
previous|next|add``, ``marker <i>/launch|select|remove``) for the CommandHandler.

Simulates the Bitwig extension (``BitwigModel.collectMarkers``, ``CommandHandler.handleMarker``,
``findAndSelectMarker``) and the Java DrivenByMoss for Bitwig (``MarkerImpl``,
``MarkerBankImpl.findAndSelect``), with the Bitwig API behind them (``CueMarker.launch``,
``position``, ``deleteObject``; ``Transport.addCueMarkerAtPlaybackPosition``).

Known only from the recordings (tests/conformance/generic_flexi/replay/generic_flexi-marker-rust.rec
and the Java trace of generic_flexi marker): a quantized launch while the transport is stopped
moves the play start position to the marker and starts playing (``/beats/position 16.0``, then
``/play 1``; a stop returns to the marker). While playing, the jump happens at the next bar
(Bitwig's launch quantization, not observed). How Bitwig names a new marker is not known: it
gets no name and the color of the fixture's markers.
"""

from dataclasses import dataclass

Color = tuple[int, int, int]

# The color of the fixture's markers (Bitwig's default cue marker color, /marker/<i>/color)
MARKER_COLOR: Color = (229, 229, 0)


@dataclass
class Marker:
    """A cue marker (Bitwig ``CueMarker``): its position in beats."""

    name: str
    position: float
    color: Color = MARKER_COLOR


class Markers:
    """The cue markers of the arranger, ordered by their position."""

    def __init__(self, transport):
        self.transport = transport
        self.items: list[Marker] = []
        self.pending_jump: float | None = None
        """The position of a quantized launch while playing (taken at the next bar)."""

    def add(self, name: str, position: float, color: Color = MARKER_COLOR) -> Marker:
        marker = Marker(name, float(position), color)
        self.items.append(marker)
        self.items.sort(key=lambda item: item.position)
        return marker

    def add_at_playback_position(self) -> Marker:
        """``addCueMarkerAtPlaybackPosition``."""
        return self.add("", self.transport.position)

    def remove(self, marker: Marker) -> None:
        if marker in self.items:
            self.items.remove(marker)

    def launch(self, marker: Marker) -> None:
        """``launch (true)``: see the module docstring."""
        if self.transport.playing:
            self.pending_jump = marker.position
            return
        self.transport.set_position_absolute(marker.position)
        self.transport.play()

    def on_bar(self) -> None:
        """A bar starts: a pending launch jumps."""
        if self.pending_jump is not None:
            self.transport.position = self.pending_jump
            self.pending_jump = None

    def find(self, forward: bool) -> Marker | None:
        """MarkerBankImpl.findAndSelect: the closest marker after (before) the play position."""
        position = self.transport.position
        candidates = [marker for marker in self.items if (marker.position - position if forward else position - marker.position) > 0]
        if not candidates:
            return None
        return min(candidates, key=lambda marker: abs(marker.position - position))


__all__ = ["MARKER_COLOR", "Marker", "Markers"]
