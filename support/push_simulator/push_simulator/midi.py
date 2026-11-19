# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""A MIDI byte stream parser (complete messages out of chunks) and a short describer."""

NOTE_OFF = 0x80
NOTE_ON = 0x90
POLY_PRESSURE = 0xA0
CC = 0xB0
PROGRAM = 0xC0
CHANNEL_PRESSURE = 0xD0
PITCHBEND = 0xE0


def data_length(status: int) -> int:
    """The number of data bytes of a channel or system common message (-1: SysEx)."""
    if status < 0xF0:
        return 1 if status & 0xF0 in (PROGRAM, CHANNEL_PRESSURE) else 2
    return {0xF0: -1, 0xF1: 1, 0xF2: 2, 0xF3: 1}.get(status, 0)


class MidiParser:
    """Splits a byte stream into messages; handles running status, real time bytes inside other
    messages and SysEx split over several chunks."""

    def __init__(self):
        self.status = 0
        self.data: list[int] = []
        self.sysex: bytearray | None = None

    def feed(self, chunk: bytes) -> list[bytes]:
        out: list[bytes] = []
        for b in chunk:
            if b >= 0xF8:
                out.append(bytes([b]))
                continue
            if self.sysex is not None:
                if b == 0xF7:
                    self.sysex.append(b)
                    out.append(bytes(self.sysex))
                    self.sysex = None
                    continue
                if b < 0x80:
                    self.sysex.append(b)
                    continue
                self.sysex = None  # an unterminated SysEx, dropped
            if b == 0xF0:
                self.sysex = bytearray([b])
                self.status = 0
                continue
            if b >= 0x80:
                self.status = b if b < 0xF0 else 0
                self.data = []
                length = data_length(b)
                if length == 0:
                    if b != 0xF7:
                        out.append(bytes([b]))
                    continue
                if b >= 0xF0:
                    self.status = b
                continue
            if not self.status:
                continue
            self.data.append(b)
            if len(self.data) == data_length(self.status):
                out.append(bytes([self.status, *self.data]))
                self.data = []
                if self.status >= 0xF0:
                    self.status = 0
        return out


def hexstr(data: bytes) -> str:
    return data.hex(" ").upper()


def describe(data: bytes) -> str:
    """A short text of a message for the monitor."""
    if not data:
        return ""
    status = data[0]
    if status == 0xF0:
        return f"SysEx ({len(data)} bytes)"
    kind = status & 0xF0
    ch = (status & 0x0F) + 1
    d1 = data[1] if len(data) > 1 else 0
    d2 = data[2] if len(data) > 2 else 0
    if kind == NOTE_ON:
        return f"Note On  ch{ch} {d1} {d2}"
    if kind == NOTE_OFF:
        return f"Note Off ch{ch} {d1} {d2}"
    if kind == CC:
        return f"CC ch{ch} {d1} {d2}"
    if kind == POLY_PRESSURE:
        return f"PolyAT ch{ch} {d1} {d2}"
    if kind == CHANNEL_PRESSURE:
        return f"ChanAT ch{ch} {d1}"
    if kind == PITCHBEND:
        return f"PitchBend ch{ch} {d1 | d2 << 7}"
    if kind == PROGRAM:
        return f"Program ch{ch} {d1}"
    return hexstr(data)
