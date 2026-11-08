# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The ctypes declarations of the C interface in include/drivenbymoss.h.

Written by hand from the header, in the same order. Keep both in sync: a change of the header
must be repeated here.

The functions which return an owned string (released with dbm_free_string) are declared with
the result type c_void_p, not c_char_p: ctypes would convert a c_char_p to bytes at once and
the pointer which must be released would be lost.
"""

import ctypes
import os
import sys
from pathlib import Path

# Flag for dbm_create: do not start the internal timer thread. The host must call dbm_tick
# regularly (every 30ms is recommended) instead.
DBM_FLAG_HOST_TICK = 1


def _library_name() -> str:
    if sys.platform == "win32":
        return "drivenbymoss.dll"
    if sys.platform == "darwin":
        return "libdrivenbymoss.dylib"
    return "libdrivenbymoss.so"


def _repository_root() -> Path:
    # bindings/python/drivenbymoss/_ffi.py -> the repository root
    return Path(__file__).resolve().parents[3]


def library_candidates() -> list[Path]:
    """The paths which are tried to load the library, in this order: the environment variable
    DBM_LIBRARY, then target/debug and target/release of the repository."""
    candidates = []
    explicit = os.environ.get("DBM_LIBRARY")
    if explicit:
        candidates.append(Path(explicit))
    root = _repository_root()
    name = _library_name()
    candidates.append(root / "target" / "debug" / name)
    candidates.append(root / "target" / "release" / name)
    return candidates


def load_library() -> ctypes.CDLL:
    """Load the library from the first existing candidate path and declare the prototypes."""
    candidates = library_candidates()
    if os.environ.get("DBM_LIBRARY"):
        # An explicit path is not skipped if it is wrong, the error shows it
        candidates = candidates[:1]
    for candidate in candidates:
        if candidate.exists() or os.environ.get("DBM_LIBRARY"):
            library = ctypes.CDLL(str(candidate))
            _declare(library)
            return library
    tried = ", ".join(str(c) for c in candidates)
    raise OSError(f"The DrivenByMoss library was not found (tried: {tried}). Build it with 'cargo build' or set DBM_LIBRARY.")


class DbmApp(ctypes.Structure):
    """An instance of DrivenByMoss (opaque for the host)."""


DbmAppPointer = ctypes.POINTER(DbmApp)

_char_p = ctypes.c_char_p
_int = ctypes.c_int
_void_p = ctypes.c_void_p
_strings = ctypes.POINTER(ctypes.c_char_p)
_ints = ctypes.POINTER(ctypes.c_int)
_bytes = ctypes.POINTER(ctypes.c_uint8)

# Adds one MIDI device (ID and name) to a list, see get_midi_inputs.
DbmAddDevice = ctypes.CFUNCTYPE(None, _void_p, _int, _char_p)

# The function types of the DbmCallbacks table, `user` is always the first argument
ProcessNoArg = ctypes.CFUNCTYPE(None, _void_p, _char_p, _char_p)
ProcessStringArg = ctypes.CFUNCTYPE(None, _void_p, _char_p, _char_p, _char_p)
ProcessStringArgs = ctypes.CFUNCTYPE(None, _void_p, _char_p, _char_p, _strings, _int)
ProcessIntArg = ctypes.CFUNCTYPE(None, _void_p, _char_p, _char_p, _int)
ProcessDoubleArg = ctypes.CFUNCTYPE(None, _void_p, _char_p, _char_p, ctypes.c_double)
DelayUpdates = ctypes.CFUNCTYPE(None, _void_p, _char_p)
EnableUpdates = ctypes.CFUNCTYPE(None, _void_p, _char_p, _int)
ProcessMidiArg = ctypes.CFUNCTYPE(None, _void_p, _int, _int, _int, _int)
GetMidiDevices = ctypes.CFUNCTYPE(None, _void_p, _void_p, DbmAddDevice)
OpenMidiPort = ctypes.CFUNCTYPE(_int, _void_p, _int)
CloseMidiPort = ctypes.CFUNCTYPE(None, _void_p, _int)
SendMidiData = ctypes.CFUNCTYPE(None, _void_p, _int, _bytes, _int)
SetNoteInputFilters = ctypes.CFUNCTYPE(None, _void_p, _int, _int, _strings, _int)
SetNoteInputTable = ctypes.CFUNCTYPE(None, _void_p, _int, _int, _ints, _int)
Log = ctypes.CFUNCTYPE(None, _void_p, _char_p)

# Receives the commands in the binary form (dbm_set_command_callback): user, buffer, length
DbmCommands = ctypes.CFUNCTYPE(None, _void_p, _bytes, ctypes.c_size_t)


class DbmCallbacks(ctypes.Structure):
    """The functions provided by the host. Function pointers may be null: the call is ignored
    (the open_* functions then report failure, the get_midi_* functions an empty list)."""

    _fields_ = [
        ("user", _void_p),
        ("process_no_arg", ProcessNoArg),
        ("process_string_arg", ProcessStringArg),
        ("process_string_args", ProcessStringArgs),
        ("process_int_arg", ProcessIntArg),
        ("process_double_arg", ProcessDoubleArg),
        ("delay_updates", DelayUpdates),
        ("enable_updates", EnableUpdates),
        ("process_midi_arg", ProcessMidiArg),
        ("get_midi_inputs", GetMidiDevices),
        ("get_midi_outputs", GetMidiDevices),
        ("open_midi_input", OpenMidiPort),
        ("open_midi_output", OpenMidiPort),
        ("close_midi_input", CloseMidiPort),
        ("close_midi_output", CloseMidiPort),
        ("send_midi_data", SendMidiData),
        ("set_note_input_filters", SetNoteInputFilters),
        ("set_note_input_key_translation_table", SetNoteInputTable),
        ("set_note_input_velocity_translation_table", SetNoteInputTable),
        ("log", Log),
    ]


# (name, result type, argument types), in the order of the header
_PROTOTYPES = [
    ("dbm_version", _char_p, []),
    ("dbm_create", DbmAppPointer, [ctypes.POINTER(DbmCallbacks), _char_p, _char_p, ctypes.c_uint32]),
    ("dbm_create_controller", DbmAppPointer, [ctypes.POINTER(DbmCallbacks), _char_p, _char_p, _char_p, ctypes.c_uint32]),
    ("dbm_list_controllers", _void_p, []),
    ("dbm_get_model_setup", _void_p, [DbmAppPointer]),
    ("dbm_destroy", None, [DbmAppPointer]),
    ("dbm_add_device", None, [DbmAppPointer, _char_p, _char_p]),
    ("dbm_start_infrastructure", None, [DbmAppPointer]),
    ("dbm_tick", None, [DbmAppPointer]),
    ("dbm_set_time", None, [DbmAppPointer, ctypes.c_uint64]),
    ("dbm_update_model", None, [DbmAppPointer, _char_p]),
    ("dbm_protocol_info", None, [ctypes.POINTER(ctypes.c_uint32), ctypes.POINTER(ctypes.c_uint32), ctypes.POINTER(ctypes.c_uint64)]),
    ("dbm_set_command_callback", _int, [DbmAppPointer, ctypes.c_uint32, DbmCommands, _void_p, ctypes.POINTER(ctypes.c_uint64), ctypes.c_size_t]),
    ("dbm_update_model_bin", None, [DbmAppPointer, _bytes, ctypes.c_size_t]),
    ("dbm_messages_to_text", _void_p, [_bytes, ctypes.c_size_t]),
    ("dbm_messages_from_text", ctypes.c_size_t, [_char_p, _bytes, ctypes.c_size_t]),
    ("dbm_display_window", None, [DbmAppPointer]),
    ("dbm_display_project_window", None, [DbmAppPointer]),
    ("dbm_display_parameter_window", None, [DbmAppPointer]),
    ("dbm_restart_controllers", None, [DbmAppPointer]),
    ("dbm_set_default_document_settings", None, [DbmAppPointer]),
    ("dbm_get_formatted_document_settings", _void_p, [DbmAppPointer]),
    ("dbm_get_settings", _void_p, [DbmAppPointer]),
    ("dbm_set_setting", None, [DbmAppPointer, _int, _char_p]),
    ("dbm_take_setting_changes", _void_p, [DbmAppPointer]),
    ("dbm_take_setting_states", _void_p, [DbmAppPointer]),
    ("dbm_take_notifications", _void_p, [DbmAppPointer]),
    ("dbm_free_string", None, [_void_p]),
    ("dbm_set_formatted_document_settings", None, [DbmAppPointer, _char_p]),
    ("dbm_on_midi_event", None, [DbmAppPointer, _int, _bytes, _int]),
]


def _declare(library: ctypes.CDLL) -> None:
    for name, result, arguments in _PROTOTYPES:
        function = getattr(library, name)
        function.restype = result
        function.argtypes = arguments


_library: ctypes.CDLL | None = None


def library() -> ctypes.CDLL:
    """The loaded library (loaded on the first use)."""
    global _library
    if _library is None:
        _library = load_library()
    return _library
