# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The USB transports of a Python host: where the USB bulk writes of the core go
(``dbm_set_usb_callback``, ``Backend.usb_bulk_out``), e.g. the frames of the Push 2/3 display.

``UsbOutput.from_environment()`` picks the transport:

- ``DBM_USB_TRANSPORT=none``: none (the writes are dropped).
- ``DBM_PUSH_DISPLAY_SOCKET`` set (or ``DBM_USB_TRANSPORT=socket``, then the default path
  ``$XDG_RUNTIME_DIR/dbm-push-display.sock``): every write goes as its exact byte stream to that
  UNIX stream socket, e.g. the Push simulator. The contract: per frame exactly the bytes of the
  write (a Push frame: the 16-byte header ``FF CC AA 88`` + 12 zero bytes, then 327,680 data
  bytes), no length prefix, frames back to back on one connection. A frame is either written
  completely or the connection is closed (never a partial frame on an open connection); the
  host connects lazily and reconnects at most once per second after an error.
- otherwise on Linux (``DBM_USB_TRANSPORT=usbdevfs`` or unset): the device itself through
  usbdevfs (``/dev/bus/usb/BBB/DDD``, found through ``/sys/bus/usb/devices``), which needs write
  access to the device node (the udev rule ``70-dbm-nihost.rules``).

The policy of all transports: the core's callback never blocks. It copies the data into one slot
per device (a newer frame replaces one which was not written yet: frames are dropped, never
queued up) and a writer thread writes it. The core resends the display about every second, so a
dropped frame or a reconnect heals itself.

Written in Python (ctypes, no C extension): the writer is a few ioctls, the Python hosts (the
reference host, zynthian) load no native code besides the core this way, and nothing has to be
built or packaged for it.
"""

import ctypes
import errno
import glob
import logging
import os
import socket
import sys
import threading
import time
from typing import Callable

logger = logging.getLogger("drivenbymoss.usb")

SOCKET_ENV = "DBM_PUSH_DISPLAY_SOCKET"
TRANSPORT_ENV = "DBM_USB_TRANSPORT"
DEFAULT_SOCKET_NAME = "dbm-push-display.sock"

# The header of a Push 2/3 display frame
PUSH_FRAME_HEADER = bytes([0xFF, 0xCC, 0xAA, 0x88]) + bytes(12)

# How long to wait after a failed connect or open before the next try
RETRY_SECONDS = 1.0
# The timeout of a socket write and of one USB transfer
SOCKET_TIMEOUT_SECONDS = 1.0
USB_TIMEOUT_MS = 1000
# The size of one bulk transfer (old kernels limit usbfs transfers to 16 KiB)
USB_CHUNK_BYTES = 16384


def default_socket_path() -> str:
    """The socket of the Push simulator: ``$DBM_PUSH_DISPLAY_SOCKET``, else
    ``$XDG_RUNTIME_DIR/dbm-push-display.sock`` (``/tmp`` without XDG_RUNTIME_DIR)."""
    path = os.environ.get(SOCKET_ENV)
    if path:
        return path
    return os.path.join(os.environ.get("XDG_RUNTIME_DIR") or "/tmp", DEFAULT_SOCKET_NAME)


def split_frame(data: bytes) -> list[bytes]:
    """The transfers of a write: the header of a Push frame as its own transfer (as the Java
    version writes it), the rest in chunks of USB_CHUNK_BYTES."""
    parts = []
    start = 0
    if len(data) > len(PUSH_FRAME_HEADER) and data.startswith(PUSH_FRAME_HEADER[:4]):
        parts.append(data[:len(PUSH_FRAME_HEADER)])
        start = len(PUSH_FRAME_HEADER)
    for offset in range(start, len(data), USB_CHUNK_BYTES):
        parts.append(data[offset:offset + USB_CHUNK_BYTES])
    return parts


class SocketSink:
    """Writes the frames to a UNIX stream socket (the Push simulator), see the module
    documentation for the contract. Used by the writer thread only."""

    def __init__(self, path: str, clock: Callable[[], float] = time.monotonic):
        self.path = path
        self.clock = clock
        self.sock: socket.socket | None = None
        self.retry_at = 0.0
        self.logged = False

    def _connect(self) -> bool:
        if self.sock is not None:
            return True
        if self.clock() < self.retry_at:
            return False
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.settimeout(SOCKET_TIMEOUT_SECONDS)
        try:
            sock.connect(self.path)
        except OSError as error:
            sock.close()
            self.retry_at = self.clock() + RETRY_SECONDS
            if not self.logged:
                self.logged = True
                logger.info("USB display socket %s: %s (retrying)", self.path, error)
            return False
        self.sock = sock
        self.logged = False
        logger.info("USB display socket %s connected", self.path)
        return True

    def write(self, data: bytes) -> bool:
        if not self._connect():
            return False
        try:
            self.sock.sendall(data)
            return True
        except OSError as error:
            # Maybe a partial frame: the stream is out of step, start again with a new connection
            logger.info("USB display socket %s: %s (reconnecting)", self.path, error)
            self.close()
            self.retry_at = self.clock() + RETRY_SECONDS
            return False

    def close(self) -> None:
        sock, self.sock = self.sock, None
        if sock is not None:
            try:
                sock.close()
            except OSError:
                pass


# usbdevfs (linux/usbdevice_fs.h); the ioctl numbers of asm-generic (x86, ARM, AArch64, RISC-V)
_IOC_WRITE = 1
_IOC_READ = 2


def _ioc(direction: int, number: int, size: int) -> int:
    return (direction << 30) | (size << 16) | (ord("U") << 8) | number


class _BulkTransfer(ctypes.Structure):
    _fields_ = [("ep", ctypes.c_uint), ("len", ctypes.c_uint), ("timeout", ctypes.c_uint), ("data", ctypes.c_void_p)]


class _UsbIoctl(ctypes.Structure):
    _fields_ = [("ifno", ctypes.c_int), ("ioctl_code", ctypes.c_int), ("data", ctypes.c_void_p)]


USBDEVFS_BULK = _ioc(_IOC_READ | _IOC_WRITE, 2, ctypes.sizeof(_BulkTransfer))
USBDEVFS_CLAIMINTERFACE = _ioc(_IOC_READ, 15, ctypes.sizeof(ctypes.c_uint))
USBDEVFS_RELEASEINTERFACE = _ioc(_IOC_READ, 16, ctypes.sizeof(ctypes.c_uint))
USBDEVFS_IOCTL = _ioc(_IOC_READ | _IOC_WRITE, 18, ctypes.sizeof(_UsbIoctl))
USBDEVFS_DISCONNECT = (ord("U") << 8) | 22

# Errors after which the device is closed and opened again (unplugged, reset)
_GONE = {errno.ENODEV, errno.ESHUTDOWN, errno.EPIPE, errno.EIO, errno.ENOENT, errno.EBADF}


def find_usb_device(vendor_id: int, product_id: int, sysfs_root: str = "/sys/bus/usb/devices", dev_root: str = "/dev/bus/usb") -> str | None:
    """The device node (``/dev/bus/usb/BBB/DDD``) of the first USB device with the IDs, None if
    there is none."""

    def read(path):
        with open(path) as file:
            return file.read().strip()

    for device in sorted(glob.glob(os.path.join(sysfs_root, "*"))):
        try:
            if int(read(os.path.join(device, "idVendor")), 16) != vendor_id or int(read(os.path.join(device, "idProduct")), 16) != product_id:
                continue
            bus, number = int(read(os.path.join(device, "busnum"))), int(read(os.path.join(device, "devnum")))
        except (OSError, ValueError):
            continue
        return os.path.join(dev_root, f"{bus:03d}", f"{number:03d}")
    return None


class UsbdevfsSink:
    """Writes to a bulk OUT endpoint of a USB device through usbdevfs (Linux): opens the device
    node, claims the interface (detaching a kernel driver from it if one has it; the Push display
    interface has none, its MIDI interfaces stay with snd-usb-audio), writes the transfers
    (``split_frame``) and opens the device again after an unplug. Used by the writer thread
    only."""

    def __init__(self, vendor_id: int, product_id: int, interface: int, clock: Callable[[], float] = time.monotonic, finder=find_usb_device):
        self.vendor_id = vendor_id
        self.product_id = product_id
        self.interface = interface
        self.clock = clock
        self.finder = finder
        self.fd: int | None = None
        self.retry_at = 0.0
        self.logged = False
        self.libc = ctypes.CDLL(None, use_errno=True)
        self.libc.ioctl.argtypes = [ctypes.c_int, ctypes.c_ulong, ctypes.c_void_p]
        self.libc.ioctl.restype = ctypes.c_int

    def _ioctl(self, request: int, argument) -> None:
        if self.libc.ioctl(self.fd, request, argument) < 0:
            code = ctypes.get_errno()
            raise OSError(code, os.strerror(code))

    def _log_once(self, message: str, *args) -> None:
        if not self.logged:
            self.logged = True
            logger.info(message, *args)

    def _open(self) -> bool:
        if self.fd is not None:
            return True
        if self.clock() < self.retry_at:
            return False
        self.retry_at = self.clock() + RETRY_SECONDS
        path = self.finder(self.vendor_id, self.product_id)
        if path is None:
            self._log_once("USB device %04x:%04x not found (retrying)", self.vendor_id, self.product_id)
            return False
        try:
            self.fd = os.open(path, os.O_RDWR | os.O_CLOEXEC)
        except OSError as error:
            self._log_once("USB device %04x:%04x: %s: %s (udev rule missing? retrying)", self.vendor_id, self.product_id, path, error)
            return False
        number = ctypes.c_uint(self.interface)
        try:
            try:
                self._ioctl(USBDEVFS_CLAIMINTERFACE, ctypes.addressof(number))
            except OSError as error:
                if error.errno != errno.EBUSY:
                    raise
                # A kernel driver has the interface: detach it and claim again
                command = _UsbIoctl(self.interface, USBDEVFS_DISCONNECT, None)
                try:
                    self._ioctl(USBDEVFS_IOCTL, ctypes.addressof(command))
                except OSError:
                    pass
                self._ioctl(USBDEVFS_CLAIMINTERFACE, ctypes.addressof(number))
        except OSError as error:
            self._log_once("USB device %04x:%04x: claiming interface %d: %s (retrying)", self.vendor_id, self.product_id, self.interface, error)
            os.close(self.fd)
            self.fd = None
            return False
        self.logged = False
        logger.info("USB device %04x:%04x opened (%s, interface %d)", self.vendor_id, self.product_id, path, self.interface)
        return True

    def write(self, endpoint: int, data: bytes) -> bool:
        if not self._open():
            return False
        for part in split_frame(data):
            buffer = ctypes.create_string_buffer(part, len(part))
            transfer = _BulkTransfer(endpoint, len(part), USB_TIMEOUT_MS, ctypes.cast(buffer, ctypes.c_void_p))
            try:
                self._ioctl(USBDEVFS_BULK, ctypes.addressof(transfer))
            except OSError as error:
                if error.errno in _GONE:
                    logger.info("USB device %04x:%04x: %s (reopening)", self.vendor_id, self.product_id, error)
                    self.close()
                    self.retry_at = self.clock() + RETRY_SECONDS
                # A timeout: the rest of the frame is dropped, the next frame starts with its header
                return False
        return True

    def close(self) -> None:
        fd, self.fd = self.fd, None
        if fd is None:
            return
        number = ctypes.c_uint(self.interface)
        self.libc.ioctl(fd, USBDEVFS_RELEASEINTERFACE, ctypes.addressof(number))
        os.close(fd)


class UsbOutput:
    """The USB writes of a host (one per App): ``write`` (from the core's callback) puts the data
    into the slot of its device and returns at once; a writer thread (started on the first write)
    writes it with the transport. ``close`` stops the thread and closes the devices.

    ``factory(vendor_id, product_id, interface)`` creates the sink of a device: an object with
    ``write(endpoint, data) -> bool`` and ``close()``."""

    def __init__(self, factory: Callable[[int, int, int], object] | None):
        self.factory = factory
        self.sinks: dict[tuple[int, int, int], object] = {}
        self.slots: dict[tuple[int, int, int, int], bytes] = {}
        self.lock = threading.Condition()
        self.thread: threading.Thread | None = None
        self._busy = False
        self.closed = False
        self.written = 0
        self.dropped = 0
        self.failed = 0

    @classmethod
    def from_environment(cls) -> "UsbOutput":
        """The transport of the environment, see the module documentation."""
        kind = os.environ.get(TRANSPORT_ENV, "").lower()
        if kind == "none":
            return cls(None)
        if kind == "socket" or (kind != "usbdevfs" and os.environ.get(SOCKET_ENV)):
            return cls.socket(default_socket_path())
        if sys.platform.startswith("linux"):
            return cls(lambda vendor_id, product_id, interface: UsbdevfsSink(vendor_id, product_id, interface))
        return cls(None)

    @classmethod
    def socket(cls, path: str, clock: Callable[[], float] = time.monotonic) -> "UsbOutput":
        """All writes to one UNIX stream socket (the Push simulator), whatever the device."""
        sink = SocketSink(path, clock)

        class Shared:
            def write(self, _endpoint, data):
                return sink.write(data)

            def close(self):
                sink.close()

        shared = Shared()
        return cls(lambda vendor_id, product_id, interface: shared)

    @property
    def enabled(self) -> bool:
        return self.factory is not None

    def write(self, vendor_id: int, product_id: int, interface: int, endpoint: int, data: bytes) -> bool:
        """Queue the data for the device (a frame not written yet is replaced). False if there is
        no transport or the output is closed. Never blocks for long."""
        if self.factory is None:
            return False
        with self.lock:
            if self.closed:
                return False
            key = (vendor_id, product_id, interface, endpoint)
            if key in self.slots:
                self.dropped += 1
            self.slots[key] = bytes(data)
            if self.thread is None:
                self.thread = threading.Thread(target=self._run, name="dbm usb", daemon=True)
                self.thread.start()
            self.lock.notify()
        return True

    def flush(self, timeout: float = 5.0) -> bool:
        """Wait until the queued data is written (tests); False on timeout."""
        deadline = time.monotonic() + timeout
        with self.lock:
            while self.slots or self._busy:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    return False
                self.lock.wait(remaining)
        return True

    def _run(self) -> None:
        while True:
            with self.lock:
                self._busy = False
                self.lock.notify_all()
                while not self.slots and not self.closed:
                    self.lock.wait()
                if self.closed:
                    return
                key, data = self.slots.popitem()
                self._busy = True
            vendor_id, product_id, interface, endpoint = key
            try:
                sink = self.sinks.get(key[:3])
                if sink is None:
                    sink = self.sinks[key[:3]] = self.factory(vendor_id, product_id, interface)
                ok = sink.write(endpoint, data)
            except Exception:  # noqa: BLE001 - the thread must survive a broken sink
                logger.exception("USB write to %04x:%04x failed", vendor_id, product_id)
                ok = False
            with self.lock:
                if ok:
                    self.written += 1
                else:
                    self.failed += 1

    def close(self) -> None:
        with self.lock:
            self.closed = True
            self.slots.clear()
            self.lock.notify_all()
        if self.thread is not None:
            self.thread.join(SOCKET_TIMEOUT_SECONDS * 2 + USB_TIMEOUT_MS / 1000.0)
            self.thread = None
        closed = set()
        for sink in self.sinks.values():
            if id(sink) not in closed:
                closed.add(id(sink))
                try:
                    sink.close()
                except Exception:  # noqa: BLE001
                    pass
        self.sinks.clear()
