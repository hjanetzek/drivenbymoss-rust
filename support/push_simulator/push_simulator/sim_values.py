# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The simulator side channel on the display socket: ``DBMV`` messages between the frames,
without Qt (the Rust side: ``src/controller/push/sim_values.rs``; doc/HOST_BRIDGE_GUIDE.md,
"Simulator side channel").

A message::

    "DBMV" version(1) kind(1) length(3: 21 bits, 7 bits per byte, least significant first) payload

Kind 1 (version 1), the encoder state: the number of encoders (1 byte), then per encoder its
index (the touch note: 0-7 the encoders over the display, 8 master, 9 swing/position, 10 tempo),
flags (1 has value, 2 touched, 4 bipolar), value and maximum (14 bits each, 7 bits per byte, least
significant first), the name and the value text (a length byte, 0-127, and UTF-8 bytes).

No message contains the byte 0xFF (all numbers are 7 bit, UTF-8 has no 0xFF), so it can never
contain the frame header ``FF CC AA 88`` + 12 zero bytes: a reader which scans for the frame
header skips a message like garbage, and a reader of messages rejects a "message" which contains
0xFF (garbage which happened to contain ``DBMV``).
"""

from dataclasses import dataclass

MAGIC = b"DBMV"
VERSION = 1
KIND_ENCODERS = 1
HEADER_SIZE = 9
MAX_PAYLOAD = (1 << 21) - 1
MAX_TEXT = 127
MAX_VALUE = (1 << 14) - 1

MASTER = 8
POSITION = 9
TEMPO = 10

FLAG_HAS_VALUE = 1
FLAG_TOUCHED = 2
FLAG_BIPOLAR = 4


@dataclass
class EncoderState:
    index: int
    has_value: bool = False
    value: int = 0
    max: int = 0
    touched: bool = False
    bipolar: bool = False
    name: str = ""
    text: str = ""

    @property
    def normalized(self) -> float:
        """The value as 0..1 (0 without a value)."""
        if not self.has_value or self.max <= 0:
            return 0.0
        return max(0.0, min(1.0, self.value / self.max))

    def as_dict(self) -> dict:
        return {"index": self.index, "hasValue": self.has_value, "value": self.normalized, "touched": self.touched,
                "bipolar": self.bipolar, "name": self.name, "text": self.text}


@dataclass
class UnknownMessage:
    """A message of another version or kind (skipped)."""
    version: int
    kind: int


def _cut(text: str) -> bytes:
    data = text.encode("utf-8")
    if len(data) <= MAX_TEXT:
        return data
    return data[:MAX_TEXT].decode("utf-8", errors="ignore").encode("utf-8")


def _u14(value: int) -> bytes:
    value = max(0, min(MAX_VALUE, value))
    return bytes((value & 0x7F, value >> 7))


def encode_message(kind: int, payload: bytes) -> bytes:
    length = min(len(payload), MAX_PAYLOAD)
    return MAGIC + bytes((VERSION, kind, length & 0x7F, (length >> 7) & 0x7F, (length >> 14) & 0x7F)) + payload[:length]


def encode_encoders(states: list[EncoderState]) -> bytes:
    states = states[:127]
    payload = bytearray([len(states)])
    for s in states:
        flags = (FLAG_HAS_VALUE if s.has_value else 0) | (FLAG_TOUCHED if s.touched else 0) | (FLAG_BIPOLAR if s.bipolar else 0)
        payload += bytes((s.index & 0x7F, flags)) + _u14(s.value) + _u14(s.max)
        for text in (s.name, s.text):
            data = _cut(text)
            payload.append(len(data))
            payload += data
    return encode_message(KIND_ENCODERS, bytes(payload))


def header_length(data) -> int | None:
    """The payload length of the message header at the start of ``data`` (at least HEADER_SIZE
    bytes starting with MAGIC), None if the header is invalid."""
    if any(b >= 0x80 for b in data[6:9]):
        return None
    return data[6] | data[7] << 7 | data[8] << 14


def decode_encoders(payload: bytes) -> list[EncoderState] | None:
    if not payload:
        return None
    count, pos = payload[0], 1
    states = []
    for _ in range(count):
        if pos + 6 > len(payload):
            return None
        index, flags = payload[pos], payload[pos + 1]
        value = payload[pos + 2] | payload[pos + 3] << 7
        maximum = payload[pos + 4] | payload[pos + 5] << 7
        pos += 6
        texts = []
        for _ in range(2):
            if pos >= len(payload):
                return None
            length = payload[pos]
            data = payload[pos + 1:pos + 1 + length]
            if len(data) != length:
                return None
            try:
                texts.append(data.decode("utf-8"))
            except UnicodeDecodeError:
                return None
            pos += 1 + length
        states.append(EncoderState(index, bool(flags & FLAG_HAS_VALUE), value, maximum, bool(flags & FLAG_TOUCHED),
                                   bool(flags & FLAG_BIPOLAR), texts[0], texts[1]))
    return states


def decode_message(data: bytes):
    """Decode the message at the start of ``data``: (message, size) with a list of
    ``EncoderState`` or an ``UnknownMessage``; None if there is no complete valid message."""
    if len(data) < HEADER_SIZE or data[:4] != MAGIC:
        return None
    length = header_length(data)
    if length is None or len(data) < HEADER_SIZE + length:
        return None
    payload = bytes(data[HEADER_SIZE:HEADER_SIZE + length])
    if 0xFF in payload:
        return None
    version, kind = data[4], data[5]
    if version != VERSION or kind != KIND_ENCODERS:
        return UnknownMessage(version, kind), HEADER_SIZE + length
    states = decode_encoders(payload)
    if states is None:
        return None
    return states, HEADER_SIZE + length
