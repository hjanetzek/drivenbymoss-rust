# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The Push 2/3 display: the USB bulk stream of frames over a UNIX socket, without Qt.

A host writes per frame exactly what goes to the USB bulk OUT endpoint 0x01 of Push 2 (Ableton's
"Push 2 MIDI and Display Interface Manual" 3.x, DrivenByMoss ``PushUsbDisplay``):

* the 16 byte frame header ``FF CC AA 88`` + 12 zero bytes,
* 160 lines of 2048 bytes: 960 pixels of 16 bit little endian (bits 0-4 red, 5-10 green, 11-15
  blue, "BGR565") and 128 filler bytes; the whole 327,680 bytes XORed with the repeating pattern
  ``E7 F3 E7 FF`` ("signal shaping").

``FrameAssembler`` finds the header in a byte stream (resynchronising after garbage), collects
the 327,680 bytes and returns the unmasked frame, and the side channel messages between the frames
(``sim_values``, ``DBMV``: the encoder state); ``DisplayServer`` accepts connections one after
the other on a UNIX stream socket and hands every complete frame to a callback (from its thread).
"""

import os
import socket
import threading
import time
from typing import Callable

from . import sim_values

WIDTH = 960
HEIGHT = 160
LINE_BYTES = 2048
PIXEL_BYTES = WIDTH * 2
FRAME_BYTES = HEIGHT * LINE_BYTES  # 327,680 = 20 * 0x4000
HEADER = bytes([0xFF, 0xCC, 0xAA, 0x88]) + bytes(12)
MASK = bytes([0xE7, 0xF3, 0xE7, 0xFF])

_MASK_INT = int.from_bytes(MASK * (FRAME_BYTES // 4), "little")


def default_socket_path() -> str:
    env = os.environ.get("DBM_PUSH_DISPLAY_SOCKET")
    if env:
        return env
    runtime = os.environ.get("XDG_RUNTIME_DIR") or f"/tmp/dbm-{os.getuid()}"
    return os.path.join(runtime, "dbm-push-display.sock")


def xor_mask(data: bytes) -> bytes:
    """Mask or unmask (the XOR is its own inverse) a frame of FRAME_BYTES."""
    if len(data) == FRAME_BYTES:
        return (int.from_bytes(data, "little") ^ _MASK_INT).to_bytes(FRAME_BYTES, "little")
    pattern = (MASK * (len(data) // 4 + 1))[:len(data)]
    return bytes(a ^ b for a, b in zip(data, pattern))


def rgb_to_pixel(r: int, g: int, b: int) -> int:
    """DrivenByMoss' ``PushUsbDisplay.sPixelFromRGB``."""
    return ((b & 0xF8) >> 3) << 11 | ((g & 0xFC) >> 2) << 5 | (r & 0xF8) >> 3


def pixel_to_rgb(pixel: int) -> tuple[int, int, int]:
    r = pixel & 0x1F
    g = (pixel >> 5) & 0x3F
    b = (pixel >> 11) & 0x1F
    return (r << 3 | r >> 2, g << 2 | g >> 4, b << 3 | b >> 2)


def encode_frame(pixel_at: Callable[[int, int], tuple[int, int, int]]) -> bytes:
    """The complete masked USB stream of one frame (header + data) of an image given by a
    function (x, y) -> (r, g, b), as the host sends it. For tests and the test sender."""
    data = bytearray(FRAME_BYTES)
    for y in range(HEIGHT):
        base = y * LINE_BYTES
        for x in range(WIDTH):
            p = rgb_to_pixel(*pixel_at(x, y))
            data[base + 2 * x] = p & 0xFF
            data[base + 2 * x + 1] = p >> 8
    return HEADER + xor_mask(bytes(data))


def frame_pixel(frame: bytes, x: int, y: int) -> tuple[int, int, int]:
    """The colour of a pixel of an unmasked frame."""
    i = y * LINE_BYTES + 2 * x
    return pixel_to_rgb(frame[i] | frame[i + 1] << 8)


def frame_to_rgb888(frame: bytes) -> bytes:
    """An unmasked frame as 960x160 RGB888 (slow, pure Python; for tests and PNG dumps)."""
    out = bytearray(WIDTH * HEIGHT * 3)
    o = 0
    for y in range(HEIGHT):
        base = y * LINE_BYTES
        for x in range(WIDTH):
            r, g, b = pixel_to_rgb(frame[base + 2 * x] | frame[base + 2 * x + 1] << 8)
            out[o:o + 3] = bytes((r, g, b))
            o += 3
    return bytes(out)


class FrameAssembler:
    """Collects frames and side channel messages (``sim_values``: ``DBMV``) from a byte stream of
    arbitrary chunks. Outside a frame it scans for the frame header or the message magic,
    whichever comes first; everything else is garbage and skipped. A message must be complete
    and valid (no 0xFF byte, see ``sim_values``), otherwise its magic counts as garbage."""

    # Bytes kept at the end of the buffer while scanning: a possible beginning of a header
    _KEEP = max(len(HEADER), sim_values.HEADER_SIZE) - 1

    def __init__(self):
        self.buffer = bytearray()
        self.in_frame = False
        self.frames = 0
        self.messages = 0
        self.skipped_bytes = 0

    def reset(self) -> None:
        """A new connection: a partial frame or message is dropped."""
        self.buffer.clear()
        self.in_frame = False

    def feed(self, chunk: bytes) -> list[bytes]:
        """Returns the completed (unmasked) frames; the messages are dropped."""
        return [item for kind, item in self.feed_items(chunk) if kind == "frame"]

    def feed_items(self, chunk: bytes) -> list[tuple[str, object]]:
        """Returns the completed items in stream order: ("frame", unmasked frame bytes) and
        ("encoders", list of sim_values.EncoderState). Messages of an unknown kind are skipped."""
        self.buffer += chunk
        items = []
        while True:
            if not self.in_frame:
                frame_at = self.buffer.find(HEADER)
                message_at = self.buffer.find(sim_values.MAGIC)
                if message_at >= 0 and (frame_at < 0 or message_at < frame_at):
                    self.skipped_bytes += message_at
                    del self.buffer[:message_at]
                    if not self._message(items):
                        break
                    continue
                if frame_at < 0:
                    if len(self.buffer) > self._KEEP:
                        self.skipped_bytes += len(self.buffer) - self._KEEP
                        del self.buffer[:-self._KEEP]
                    break
                self.skipped_bytes += frame_at
                del self.buffer[:frame_at + len(HEADER)]
                self.in_frame = True
            if len(self.buffer) < FRAME_BYTES:
                break
            data = bytes(self.buffer[:FRAME_BYTES])
            del self.buffer[:FRAME_BYTES]
            self.in_frame = False
            self.frames += 1
            items.append(("frame", xor_mask(data)))
        return items

    def _message(self, items: list) -> bool:
        """The buffer starts with the message magic: consume the message or its magic (garbage).
        False if more data is needed."""
        if len(self.buffer) < sim_values.HEADER_SIZE:
            return False
        length = sim_values.header_length(self.buffer)
        if length is not None:
            end = sim_values.HEADER_SIZE + length
            # A 0xFF byte ends a "message" early: it is no message (and maybe a frame starts)
            if self.buffer.find(0xFF, sim_values.HEADER_SIZE, end) < 0:
                if len(self.buffer) < end:
                    return False
                decoded = sim_values.decode_message(self.buffer[:end])
                if decoded is not None:
                    del self.buffer[:end]
                    message, _ = decoded
                    self.messages += 1
                    if isinstance(message, list):
                        items.append(("encoders", message))
                    return True
        # Not a message: skip its first byte and scan again
        self.skipped_bytes += 1
        del self.buffer[:1]
        return True


class DisplayServer:
    """Listens on a UNIX stream socket; one connection at a time, the next after it closed."""

    def __init__(self, path: str, on_frame: Callable[[bytes], None],
                 on_status: Callable[[str], None] | None = None,
                 on_encoders: Callable[[list | None], None] | None = None):
        self.path = path
        self.on_frame = on_frame
        self.on_status = on_status
        # The encoder state of the side channel; None when the host connects or disconnects
        self.on_encoders = on_encoders
        self.assembler = FrameAssembler()
        self.connected = False
        self.connections = 0
        self._stop = threading.Event()
        self._sock: socket.socket | None = None
        self._thread: threading.Thread | None = None

    def start(self) -> None:
        os.makedirs(os.path.dirname(self.path) or ".", exist_ok=True)
        try:
            os.unlink(self.path)
        except FileNotFoundError:
            pass
        self._sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self._sock.bind(self.path)
        self._sock.listen(4)
        self._sock.settimeout(0.2)
        self._thread = threading.Thread(target=self._run, name="push-display", daemon=True)
        self._thread.start()
        self._status(f"display: listening on {self.path}")

    def stop(self) -> None:
        self._stop.set()
        if self._thread:
            self._thread.join(timeout=2)
        if self._sock:
            self._sock.close()
        try:
            os.unlink(self.path)
        except OSError:
            pass

    def _status(self, text: str) -> None:
        if self.on_status:
            self.on_status(text)

    def _encoders(self, states: list | None) -> None:
        if self.on_encoders:
            self.on_encoders(states)

    def _run(self) -> None:
        while not self._stop.is_set():
            try:
                conn, _ = self._sock.accept()
            except socket.timeout:
                continue
            except OSError:
                break
            self.connections += 1
            self.connected = True
            self.assembler.reset()
            self._encoders(None)
            self._status(f"display: host connected (#{self.connections})")
            conn.settimeout(0.2)
            with conn:
                while not self._stop.is_set():
                    try:
                        chunk = conn.recv(1 << 18)
                    except socket.timeout:
                        continue
                    except OSError:
                        break
                    if not chunk:
                        break
                    for kind, item in self.assembler.feed_items(chunk):
                        if kind == "frame":
                            self.on_frame(item)
                        else:
                            self._encoders(item)
            self.connected = False
            self._encoders(None)
            self._status("display: host disconnected")


class FpsCounter:
    def __init__(self, window: float = 2.0):
        self.window = window
        self.times: list[float] = []

    def tick(self, now: float | None = None) -> None:
        now = time.monotonic() if now is None else now
        self.times.append(now)
        while self.times and self.times[0] < now - self.window:
            self.times.pop(0)

    def fps(self, now: float | None = None) -> float:
        now = time.monotonic() if now is None else now
        recent = [t for t in self.times if t >= now - self.window]
        if len(recent) < 2:
            return 0.0
        return (len(recent) - 1) / max(1e-6, recent[-1] - recent[0])
