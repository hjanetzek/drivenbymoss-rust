# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Python binding of the C interface of DrivenByMoss (include/drivenbymoss.h), with ctypes.

Usage::

    class MyBackend(drivenbymoss.Backend):
        ... override what the host supports ...

    with drivenbymoss.App(MyBackend(), ini_path, "1.0/linux-x86_64") as app:
        app.start_infrastructure()
        while running:
            app.on_midi_event(device_id, data)   # for every received MIDI message
            app.tick()                           # every 30ms
            app.update_model(changes)            # lines of '<address> <value>'

The rules of drivenbymoss.h apply:

- All strings are UTF-8. Strings returned by the library are copied into Python strings and
  released at once; arguments of the callbacks are copied before the backend sees them.
- All calls on one instance are serialized. Calling a method of an App from within a callback
  of the same App would deadlock in the library; the binding raises a RuntimeError instead.
- The App is created with DBM_FLAG_HOST_TICK: there is no timer thread in the library, the
  host calls tick() regularly (every 30ms is recommended) from its own loop. The callbacks are
  then only called on the thread which calls the App.
- Exceptions raised by the backend are caught in the trampolines and logged (logger
  "drivenbymoss"); they never cross into the library.
"""

import ctypes
import json
import logging
import threading
from dataclasses import dataclass, field
from typing import Any, Iterable, NamedTuple

from . import _ffi
from ._ffi import DBM_FLAG_HOST_TICK

__all__ = [
    "App",
    "Backend",
    "DBM_FLAG_HOST_TICK",
    "MidiDevice",
    "Setting",
    "SettingState",
    "list_controllers",
    "version",
]

logger = logging.getLogger("drivenbymoss")


class MidiDevice(NamedTuple):
    """A MIDI device of the host: its ID and name."""

    id: int
    name: str


@dataclass
class Setting:
    """One setting of the controller, see App.get_settings."""

    id: int
    scope: str
    """'global' or 'document'."""
    kind: str
    """'enum', 'integer', 'boolean', 'text', 'action' or 'signal'."""
    category: str
    label: str
    default_value: str
    value: str
    extra: list[str] = field(default_factory=list)
    """The options of an enumeration, the minimum and maximum of an integer or the button text of
    a signal."""


class SettingState(NamedTuple):
    """The visibility and enabled state of a setting, see App.take_setting_states."""

    id: int
    visible: bool
    enabled: bool


class Backend:
    """The functions the host provides to the library (the callbacks of DbmCallbacks).

    All methods do nothing by default; the open functions report failure and the device lists
    are empty. The arguments are copies, they can be kept. A method must not call methods of the
    App which called it.
    """

    def process(self, processor: str, command: str, value: Any) -> None:
        """Execute a command of the core. The typed process_* methods forward to this method by
        default: value is None, a str, a list of str, an int or a float."""

    def process_no_arg(self, processor: str, command: str) -> None:
        """Execute a command without a value, e.g. ('transport', 'play')."""
        self.process(processor, command, None)

    def process_string_arg(self, processor: str, command: str, value: str) -> None:
        self.process(processor, command, value)

    def process_string_args(self, processor: str, command: str, values: list[str]) -> None:
        self.process(processor, command, values)

    def process_int_arg(self, processor: str, command: str, value: int) -> None:
        self.process(processor, command, value)

    def process_double_arg(self, processor: str, command: str, value: float) -> None:
        self.process(processor, command, value)

    def delay_updates(self, processor: str) -> None:
        """Delay the updates of the processor, to prevent that old values are sent back."""

    def enable_updates(self, processor: str, enable: bool) -> None:
        pass

    def process_midi_arg(self, device_id: int, status: int, data1: int, data2: int) -> None:
        """Send a MIDI message to the DAW as if it was received on the MIDI input."""

    def get_midi_inputs(self) -> Iterable[MidiDevice]:
        """The MIDI inputs of the host, as MidiDevice or (id, name) tuples."""
        return []

    def get_midi_outputs(self) -> Iterable[MidiDevice]:
        """The MIDI outputs of the host, as MidiDevice or (id, name) tuples."""
        return []

    def open_midi_input(self, device_id: int) -> bool:
        return False

    def open_midi_output(self, device_id: int) -> bool:
        return False

    def close_midi_input(self, device_id: int) -> None:
        pass

    def close_midi_output(self, device_id: int) -> None:
        pass

    def send_midi_data(self, device_id: int, data: bytes) -> None:
        """Send a MIDI message (short message or sysex) to a MIDI output."""

    def set_note_input_filters(self, device_id: int, note_input_index: int, filters: list[str]) -> None:
        """Filters (hex strings like '90', 'B040') of the events which go directly to the DAW."""

    def set_note_input_key_translation_table(self, device_id: int, note_input_index: int, table: list[int]) -> None:
        """Translation table (128 entries, -1 blocks a note) for the notes of a note input."""

    def set_note_input_velocity_translation_table(self, device_id: int, note_input_index: int, table: list[int]) -> None:
        pass

    def log(self, message: str) -> None:
        """A log message of the library (it is also written to stderr)."""


def _str(text: bytes | None) -> str:
    """A string argument of a callback (null is an empty string)."""
    return "" if text is None else text.decode("utf-8", errors="replace")


def _strs(values, count: int) -> list[str]:
    if not values or count <= 0:
        return []
    return [_str(values[i]) for i in range(count)]


def _ints(values, count: int) -> list[int]:
    if not values or count <= 0:
        return []
    return list(values[:count])


def _bytes(text: str | None) -> bytes:
    return b"" if text is None else text.encode("utf-8")


def _take(pointer: int | None) -> str:
    """Copy a string returned by the library and release it with dbm_free_string."""
    if not pointer:
        return ""
    try:
        return ctypes.string_at(pointer).decode("utf-8", errors="replace")
    finally:
        _ffi.library().dbm_free_string(pointer)


def _lines(text: str) -> list[str]:
    return text.split("\n") if text else []


def version() -> str:
    """The version of the library, e.g. '26.1.0'."""
    return _str(_ffi.library().dbm_version())


def list_controllers() -> list[dict]:
    """The supported controllers (parsed JSON, see dbm_list_controllers): per controller its
    'uuid', 'vendor', 'model', 'numMidiInPorts', 'numMidiOutPorts' and 'discovery'."""
    return json.loads(_take(_ffi.library().dbm_list_controllers()) or "[]")


class _Trampolines:
    """The C callbacks of one App. They convert the arguments, call the backend and catch all
    exceptions. The ctypes function objects are kept here for the lifetime of the App: the
    library calls them until dbm_destroy returned."""

    def __init__(self, backend: Backend):
        self.backend = backend
        self.local = threading.local()

        def guard(name: str, default, function):
            def trampoline(_user, *args):
                self.local.depth = getattr(self.local, "depth", 0) + 1
                try:
                    return function(*args)
                except BaseException:  # noqa: BLE001 - nothing may cross into the library
                    logger.exception("Exception in the backend callback %s", name)
                    return default
                finally:
                    self.local.depth -= 1

            return trampoline

        def add_devices(devices, device_list, add):
            for device_id, name in devices:
                add(device_list, int(device_id), _bytes(name))

        b = backend
        self.functions = {
            "process_no_arg": _ffi.ProcessNoArg(guard("process_no_arg", None, lambda p, c: b.process_no_arg(_str(p), _str(c)))),
            "process_string_arg": _ffi.ProcessStringArg(guard("process_string_arg", None, lambda p, c, v: b.process_string_arg(_str(p), _str(c), _str(v)))),
            "process_string_args": _ffi.ProcessStringArgs(guard("process_string_args", None, lambda p, c, v, n: b.process_string_args(_str(p), _str(c), _strs(v, n)))),
            "process_int_arg": _ffi.ProcessIntArg(guard("process_int_arg", None, lambda p, c, v: b.process_int_arg(_str(p), _str(c), v))),
            "process_double_arg": _ffi.ProcessDoubleArg(guard("process_double_arg", None, lambda p, c, v: b.process_double_arg(_str(p), _str(c), v))),
            "delay_updates": _ffi.DelayUpdates(guard("delay_updates", None, lambda p: b.delay_updates(_str(p)))),
            "enable_updates": _ffi.EnableUpdates(guard("enable_updates", None, lambda p, e: b.enable_updates(_str(p), e != 0))),
            "process_midi_arg": _ffi.ProcessMidiArg(guard("process_midi_arg", None, lambda d, s, d1, d2: b.process_midi_arg(d, s, d1, d2))),
            "get_midi_inputs": _ffi.GetMidiDevices(guard("get_midi_inputs", None, lambda lst, add: add_devices(b.get_midi_inputs(), lst, add))),
            "get_midi_outputs": _ffi.GetMidiDevices(guard("get_midi_outputs", None, lambda lst, add: add_devices(b.get_midi_outputs(), lst, add))),
            "open_midi_input": _ffi.OpenMidiPort(guard("open_midi_input", 0, lambda d: 1 if b.open_midi_input(d) else 0)),
            "open_midi_output": _ffi.OpenMidiPort(guard("open_midi_output", 0, lambda d: 1 if b.open_midi_output(d) else 0)),
            "close_midi_input": _ffi.CloseMidiPort(guard("close_midi_input", None, lambda d: b.close_midi_input(d))),
            "close_midi_output": _ffi.CloseMidiPort(guard("close_midi_output", None, lambda d: b.close_midi_output(d))),
            "send_midi_data": _ffi.SendMidiData(guard("send_midi_data", None, lambda d, data, n: b.send_midi_data(d, ctypes.string_at(data, n) if data and n > 0 else b""))),
            "set_note_input_filters": _ffi.SetNoteInputFilters(guard("set_note_input_filters", None, lambda d, i, f, n: b.set_note_input_filters(d, i, _strs(f, n)))),
            "set_note_input_key_translation_table": _ffi.SetNoteInputTable(guard("set_note_input_key_translation_table", None, lambda d, i, t, n: b.set_note_input_key_translation_table(d, i, _ints(t, n)))),
            "set_note_input_velocity_translation_table": _ffi.SetNoteInputTable(guard("set_note_input_velocity_translation_table", None, lambda d, i, t, n: b.set_note_input_velocity_translation_table(d, i, _ints(t, n)))),
            "log": _ffi.Log(guard("log", None, lambda m: b.log(_str(m)))),
        }
        self.table = _ffi.DbmCallbacks(user=None, **self.functions)

    def in_callback(self) -> bool:
        """True if the current thread is inside a callback of this App."""
        return getattr(self.local, "depth", 0) > 0


class App:
    """An instance of DrivenByMoss (the DbmApp handle), created with DBM_FLAG_HOST_TICK.

    Close it with close() or use it as a context manager. The backend is kept alive by the App.
    """

    def __init__(self, backend: Backend, ini_path: str, app_version: str = "", *, _handle_factory=None):
        """Create an instance (dbm_create).

        ini_path is the folder of the configuration files (DrivenByMoss4Reaper.config with the
        CONTROLLER_INSTANCE<n> entries and the configurations of the controllers), app_version
        the version of the host application (e.g. '7.22/linux-x86_64').
        """
        self._library = _ffi.library()
        self._trampolines = _Trampolines(backend)
        self._handle = None
        if _handle_factory is None:
            handle = self._library.dbm_create(ctypes.byref(self._trampolines.table), _bytes(ini_path), _bytes(app_version), DBM_FLAG_HOST_TICK)
        else:
            handle = _handle_factory(ctypes.byref(self._trampolines.table))
        if not handle:
            raise ValueError("The DrivenByMoss instance could not be created")
        self._handle = handle

    @classmethod
    def create_controller(cls, backend: Backend, controller_id: str, config_path: str, app_version: str = "") -> "App":
        """Create an instance which runs exactly one controller (dbm_create_controller), for hosts
        which declare each controller themselves and assign its MIDI ports. controller_id is the
        UUID of the controller (see list_controllers); its MIDI input and output are the device
        IDs 0. config_path is the folder of the controller's configuration file. There is no main
        configuration and no port discovery.

        Raises ValueError if the controller is not supported.
        """
        library = _ffi.library()
        return cls(backend, "", app_version, _handle_factory=lambda callbacks: library.dbm_create_controller(callbacks, _bytes(controller_id), _bytes(config_path), _bytes(app_version), DBM_FLAG_HOST_TICK))

    @property
    def backend(self) -> Backend:
        return self._trampolines.backend

    @property
    def closed(self) -> bool:
        return self._handle is None

    def _app(self):
        if self._handle is None:
            raise RuntimeError("The App is closed")
        if self._trampolines.in_callback():
            raise RuntimeError("Methods of an App must not be called from within its callbacks (the library would deadlock)")
        return self._handle

    def close(self) -> None:
        """Shutdown all controllers and release the instance (dbm_destroy). The backend still
        receives the last calls (e.g. closing the MIDI ports). Calling it again does nothing."""
        if self._handle is None:
            return
        handle = self._app()
        self._handle = None
        self._library.dbm_destroy(handle)

    def __enter__(self) -> "App":
        return self

    def __exit__(self, *exc_info) -> None:
        self.close()

    def __del__(self):
        try:
            self.close()
        except Exception:  # noqa: BLE001 - e.g. at interpreter shutdown
            pass

    def add_device(self, name: str, identifier: str) -> None:
        """Add an available device (e.g. a plugin) with its identifier."""
        self._library.dbm_add_device(self._app(), _bytes(name), _bytes(identifier))

    def start_infrastructure(self) -> None:
        """Start the controller instances (opens the MIDI ports through the backend)."""
        self._library.dbm_start_infrastructure(self._app())

    def tick(self) -> None:
        """Execute scheduled tasks and update the controllers; call it every 30ms."""
        self._library.dbm_tick(self._app())

    def update_model(self, data: str) -> None:
        """Update the model with data from the DAW: lines of '<address> <value>'."""
        self._library.dbm_update_model(self._app(), _bytes(data))

    def on_midi_event(self, device_id: int, data: bytes | bytearray | Iterable[int]) -> None:
        """Handle a MIDI message received on a MIDI input opened with open_midi_input."""
        message = bytes(data)
        buffer = (ctypes.c_uint8 * len(message)).from_buffer_copy(message)
        self._library.dbm_on_midi_event(self._app(), device_id, buffer, len(message))

    def display_window(self) -> None:
        self._library.dbm_display_window(self._app())

    def display_project_window(self) -> None:
        self._library.dbm_display_project_window(self._app())

    def display_parameter_window(self) -> None:
        self._library.dbm_display_parameter_window(self._app())

    def restart_controllers(self) -> None:
        self._library.dbm_restart_controllers(self._app())

    def get_model_setup(self) -> dict | None:
        """The model setup of the running controller (parsed JSON, keys as in the Java
        ModelSetup, e.g. 'numTracks'). None until start_infrastructure started the controller."""
        text = _take(self._library.dbm_get_model_setup(self._app()))
        return json.loads(text) if text else None

    # Document settings (stored in the project of the host)

    def set_default_document_settings(self) -> None:
        """Set the default initial settings for the document/project."""
        self._library.dbm_set_default_document_settings(self._app())

    def get_formatted_document_settings(self) -> str:
        """The document settings formatted to be stored in the project."""
        return _take(self._library.dbm_get_formatted_document_settings(self._app()))

    def set_formatted_document_settings(self, data: str) -> None:
        """Set the document settings from the project (kept pending for controllers which do not
        run yet)."""
        self._library.dbm_set_formatted_document_settings(self._app(), _bytes(data))

    # Settings for hosts which show them in their own UI. They address the first running
    # controller and are empty until start_infrastructure started it.

    def get_settings(self) -> list[Setting]:
        """All settings of the controller in the order of their activation."""
        settings = []
        for line in _lines(_take(self._library.dbm_get_settings(self._app()))):
            fields = line.split("\t")
            fields += [""] * (7 - len(fields))
            settings.append(Setting(int(fields[0]), *fields[1:7], extra=fields[7:]))
        return settings

    def set_setting(self, setting_id: int, value: str) -> None:
        """Set a setting from the UI of the host. The change is not reported back by
        take_setting_changes."""
        self._library.dbm_set_setting(self._app(), setting_id, _bytes(value))

    def take_setting_changes(self) -> list[tuple[int, str]]:
        """The settings which the controller changed since the last call, as (ID, value)."""
        changes = []
        for line in _lines(_take(self._library.dbm_take_setting_changes(self._app()))):
            setting_id, _, value = line.partition("\t")
            changes.append((int(setting_id), value))
        return changes

    def take_setting_states(self) -> list[SettingState]:
        """The settings whose visibility or enabled state changed since the last call. The first
        call after the start reports the states which the controller set while it was created."""
        states = []
        for line in _lines(_take(self._library.dbm_take_setting_states(self._app()))):
            setting_id, visible, enabled = line.split("\t")
            states.append(SettingState(int(setting_id), visible == "1", enabled == "1"))
        return states

    def take_notifications(self) -> list[str]:
        """The notifications (popups of the host) since the last call."""
        return _lines(_take(self._library.dbm_take_notifications(self._app())))
