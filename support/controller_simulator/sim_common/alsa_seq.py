# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""A minimal ALSA sequencer client through ctypes on libasound.so.2 (no Python MIDI packages).

As support/linux/ni_host/tools/dbm-nihost-bridge-test: messages are encoded with
``snd_midi_event_encode`` and sent direct to the subscribers of a port; received events are read
non-blocking (``snd_seq_event_input``, woken by the sequencer's poll descriptors in a thread) and
turned back into bytes with ``snd_midi_event_decode`` (no running status).
"""

import ctypes
import ctypes.util
import errno
import os
import select
import threading
from typing import Callable

SND_SEQ_OPEN_DUPLEX = 3
SND_SEQ_NONBLOCK = 1
SND_SEQ_PORT_CAP_READ = 1 << 0
SND_SEQ_PORT_CAP_WRITE = 1 << 1
SND_SEQ_PORT_CAP_DUPLEX = 1 << 4
SND_SEQ_PORT_CAP_SUBS_READ = 1 << 5
SND_SEQ_PORT_CAP_SUBS_WRITE = 1 << 6
SND_SEQ_PORT_TYPE_MIDI_GENERIC = 1 << 1
SND_SEQ_PORT_TYPE_HARDWARE = 1 << 16
SND_SEQ_PORT_TYPE_PORT = 1 << 19
SND_SEQ_PORT_TYPE_APPLICATION = 1 << 20
SND_SEQ_QUEUE_DIRECT = 253
SND_SEQ_ADDRESS_SUBSCRIBERS = 254
SND_SEQ_ADDRESS_UNKNOWN = 253
POLLIN = 1
EVENT_SIZE = 64  # sizeof(snd_seq_event_t) is 28 on 64 bit Linux; some room
BUFFER_SIZE = 1 << 16

DUPLEX_CAPS = (SND_SEQ_PORT_CAP_READ | SND_SEQ_PORT_CAP_SUBS_READ | SND_SEQ_PORT_CAP_WRITE
               | SND_SEQ_PORT_CAP_SUBS_WRITE | SND_SEQ_PORT_CAP_DUPLEX)
READ_CAPS = SND_SEQ_PORT_CAP_READ | SND_SEQ_PORT_CAP_SUBS_READ
WRITE_CAPS = SND_SEQ_PORT_CAP_WRITE | SND_SEQ_PORT_CAP_SUBS_WRITE


class AlsaError(RuntimeError):
    pass


class _PollFd(ctypes.Structure):
    _fields_ = [("fd", ctypes.c_int), ("events", ctypes.c_short), ("revents", ctypes.c_short)]


_lib = None


def library():
    global _lib
    if _lib is None:
        name = ctypes.util.find_library("asound") or "libasound.so.2"
        try:
            _lib = ctypes.CDLL(name)
        except OSError as ex:
            raise AlsaError(f"libasound not found: {ex}") from ex
        _lib.snd_seq_event_input.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p)]
        _lib.snd_midi_event_decode.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_long, ctypes.c_void_p]
        _lib.snd_midi_event_decode.restype = ctypes.c_long
        _lib.snd_midi_event_encode.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_long, ctypes.c_void_p]
        _lib.snd_midi_event_encode.restype = ctypes.c_long
        _lib.snd_seq_client_info_get_name.restype = ctypes.c_char_p
        _lib.snd_seq_port_info_get_name.restype = ctypes.c_char_p
        _lib.snd_seq_port_info_get_capability.restype = ctypes.c_uint
    return _lib


def available() -> bool:
    """True if the ALSA sequencer can be opened."""
    if not os.path.exists("/dev/snd/seq"):
        return False
    try:
        seq = AlsaSeq("dbm-push-probe", [])
    except AlsaError:
        return False
    seq.close()
    return True


class AlsaSeq:
    """A sequencer client with duplex ports (one per name)."""

    def __init__(self, client_name: str, port_names: list[str], port_type: int | None = None):
        self.lib = library()
        self.seq = ctypes.c_void_p()
        if self.lib.snd_seq_open(ctypes.byref(self.seq), b"default", SND_SEQ_OPEN_DUPLEX, SND_SEQ_NONBLOCK) != 0:
            raise AlsaError("can't open the ALSA sequencer")
        self.lib.snd_seq_set_client_name(self.seq, client_name.encode())
        self.client_id = self.lib.snd_seq_client_id(self.seq)
        if port_type is None:
            port_type = SND_SEQ_PORT_TYPE_MIDI_GENERIC | SND_SEQ_PORT_TYPE_HARDWARE | SND_SEQ_PORT_TYPE_PORT
        self.ports: dict[str, int] = {}
        for spec in port_names:
            # A name (a duplex port) or (name, capabilities), e.g. READ_CAPS for a port the DAW
            # reads from and WRITE_CAPS for one it writes to (as NI's driver and dbm-nihost)
            name, caps = (spec, DUPLEX_CAPS) if isinstance(spec, str) else spec
            port = self.lib.snd_seq_create_simple_port(self.seq, name.encode(), caps, port_type)
            if port < 0:
                raise AlsaError(f"can't create the port {name!r}")
            self.ports[name] = port
        self.encoder = ctypes.c_void_p()
        self.decoder = ctypes.c_void_p()
        if (self.lib.snd_midi_event_new(BUFFER_SIZE, ctypes.byref(self.encoder)) != 0
                or self.lib.snd_midi_event_new(BUFFER_SIZE, ctypes.byref(self.decoder)) != 0):
            raise AlsaError("can't create the MIDI event coder")
        self.lib.snd_midi_event_no_status(self.decoder, 1)
        self.lock = threading.Lock()
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None
        self._decode_buffer = ctypes.create_string_buffer(BUFFER_SIZE)
        self._closed = False

    # Sending

    def send(self, data: bytes, port: int) -> None:
        """Send one complete message to the subscribers of a port."""
        if not data:
            return
        event = ctypes.create_string_buffer(EVENT_SIZE)
        with self.lock:
            if self._closed:
                return
            self.lib.snd_midi_event_reset_encode(self.encoder)
            used = self.lib.snd_midi_event_encode(self.encoder, bytes(data), len(data), event)
            if used != len(data) or event.raw[0] == 0 and data[0] != 0xF8:
                return  # not a complete message
            # snd_seq_event_t: type, flags, tag, queue, time (8), source (client, port), dest (client, port)
            event[3:4] = bytes([SND_SEQ_QUEUE_DIRECT])
            event[12:14] = bytes([0, port])
            event[14:16] = bytes([SND_SEQ_ADDRESS_SUBSCRIBERS, SND_SEQ_ADDRESS_UNKNOWN])
            result = self.lib.snd_seq_event_output_direct(self.seq, event)
        if result < 0 and result != -errno.EAGAIN:
            raise AlsaError(f"snd_seq_event_output_direct failed ({result})")

    # Receiving

    def poll_fds(self) -> list[int]:
        count = self.lib.snd_seq_poll_descriptors_count(self.seq, POLLIN)
        fds = (_PollFd * count)()
        self.lib.snd_seq_poll_descriptors(self.seq, fds, count, POLLIN)
        return [fds[i].fd for i in range(count)]

    def read_pending(self) -> list[tuple[bytes, int]]:
        """All waiting events as (bytes, destination port)."""
        out = []
        with self.lock:
            while not self._closed:
                ev = ctypes.c_void_p()
                result = self.lib.snd_seq_event_input(self.seq, ctypes.byref(ev))
                if result == -errno.ENOSPC:
                    continue  # input overrun; events lost, go on
                if result < 0 or not ev.value:
                    break
                header = ctypes.string_at(ev.value, 16)
                port = header[15]
                n = self.lib.snd_midi_event_decode(self.decoder, self._decode_buffer, BUFFER_SIZE, ev)
                if n > 0:
                    out.append((self._decode_buffer.raw[:n], port))
        return out

    def start_reader(self, callback: Callable[[bytes, int], None]) -> None:
        """Call callback(data, port) from a thread for every received event."""
        fds = self.poll_fds()

        def run():
            poller = select.poll()
            for fd in fds:
                poller.register(fd, select.POLLIN)
            while not self._stop.is_set():
                if not poller.poll(200):
                    continue
                for data, port in self.read_pending():
                    callback(data, port)

        self._thread = threading.Thread(target=run, name="alsa-seq-in", daemon=True)
        self._thread.start()

    # Connections

    def connect_to(self, port: int, client: int, dest_port: int) -> None:
        if self.lib.snd_seq_connect_to(self.seq, port, client, dest_port) < 0:
            raise AlsaError(f"can't connect to {client}:{dest_port}")

    def connect_from(self, port: int, client: int, src_port: int) -> None:
        if self.lib.snd_seq_connect_from(self.seq, port, client, src_port) < 0:
            raise AlsaError(f"can't connect from {client}:{src_port}")

    def list_ports(self) -> list[tuple[int, int, str, str]]:
        """All ports of all clients: (client, port, client name, port name)."""
        lib = self.lib
        cinfo = ctypes.c_void_p()
        pinfo = ctypes.c_void_p()
        lib.snd_seq_client_info_malloc(ctypes.byref(cinfo))
        lib.snd_seq_port_info_malloc(ctypes.byref(pinfo))
        result = []
        try:
            lib.snd_seq_client_info_set_client(cinfo, -1)
            while lib.snd_seq_query_next_client(self.seq, cinfo) >= 0:
                client = lib.snd_seq_client_info_get_client(cinfo)
                cname = lib.snd_seq_client_info_get_name(cinfo).decode(errors="replace")
                lib.snd_seq_port_info_set_client(pinfo, client)
                lib.snd_seq_port_info_set_port(pinfo, -1)
                while lib.snd_seq_query_next_port(self.seq, pinfo) >= 0:
                    port = lib.snd_seq_port_info_get_port(pinfo)
                    pname = lib.snd_seq_port_info_get_name(pinfo).decode(errors="replace")
                    result.append((client, port, cname, pname))
        finally:
            lib.snd_seq_client_info_free(cinfo)
            lib.snd_seq_port_info_free(pinfo)
        return result

    def find_port(self, name: str) -> tuple[int, int] | None:
        """The first port whose name equals (or else contains) the given name."""
        ports = self.list_ports()
        for client, port, _, pname in ports:
            if pname == name and client != self.client_id:
                return (client, port)
        for client, port, _, pname in ports:
            if name in pname and client != self.client_id:
                return (client, port)
        return None

    def resolve(self, name: str) -> tuple[int, int] | None:
        """A port given as "client:port" or by its name (see find_port)."""
        if ":" in name and all(part.isdigit() for part in name.split(":", 1)):
            client, port = name.split(":", 1)
            return (int(client), int(port))
        return self.find_port(name)

    def connect_both(self, out_port: int, in_port: int, target: tuple[int, int]) -> None:
        """Connect our sending port to the target and the target to our receiving port (both the
        same port for a duplex port); an existing connection (EBUSY) is fine."""
        for connect, port in ((self.lib.snd_seq_connect_to, out_port), (self.lib.snd_seq_connect_from, in_port)):
            result = connect(self.seq, port, target[0], target[1])
            if result < 0 and result != -errno.EBUSY:
                raise AlsaError(f"can't connect to {target[0]}:{target[1]}")

    def close(self) -> None:
        self._stop.set()
        if self._thread:
            self._thread.join(timeout=2)
        with self.lock:
            if self._closed:
                return
            self._closed = True
            self.lib.snd_midi_event_free(self.encoder)
            self.lib.snd_midi_event_free(self.decoder)
            self.lib.snd_seq_close(self.seq)
