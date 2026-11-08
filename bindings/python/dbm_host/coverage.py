# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The coverage check of the command handlers (TYPED_PROTOCOL_PLAN §2.5): a record of every
command ID with plausible indices and values goes through ``process_commands`` of a fresh
handler; the IDs which end in its ``unhandled`` list are the ones it does not execute. The tests
of the reference host and of the Zynthian adapter compare them with the handler's
``UNHANDLED_COMMANDS`` (the set passed to the core)."""

import drivenbymoss
from drivenbymoss import protocol

COMMAND_SLOT_VALUES = {"ch_kind": protocol.ChanKind.TRACK, "ch_processor": protocol.Processor.TRACK, "dev": protocol.Processor.DEVICE,
                       "bank": protocol.Processor.TRACK, "owner": protocol.Processor.DRUM, "area": protocol.Processor.TRACK,
                       "grid_steps": 16, "grid_rows": 8}
COMMAND_VALUES = {"none": None, "bool": True, "i32": 1, "f64": 0.5, "text": "x", "text_list": ["x"], "color": (1, 2, 3), "value_op": 0.5,
                  "pair": (1, 1), "int_double": (1, 0.5), "notes": [], "step": None}
# Commands which need other values than the plausible ones
COMMAND_OVERRIDES = {
    protocol.Cmd.DRUM_CLEAR_MUTE: ({"dev": protocol.Processor.DRUM}, None),
    protocol.Cmd.DRUM_CLEAR_SOLO: ({"dev": protocol.Processor.DRUM}, None),
    protocol.Cmd.GRID_STEP_SET_FROM: ({}, " ".join(["0.5"] * 24)),
    protocol.Cmd.GRID_STEP_UPDATE: ({}, " ".join(["0.5"] * 24)),
}


def command_record(command_id: int) -> bytes:
    """A buffer with one record of the command: plausible indices (a track, the cursor device, the
    grid of 16 x 8 steps), index 0 else, a value of the kind (COMMAND_OVERRIDES)."""
    info = protocol.CMD_INFO[command_id]
    slots, value = COMMAND_OVERRIDES.get(command_id, ({}, COMMAND_VALUES[info.kind]))
    indices = [slots.get(slot[0][0], COMMAND_SLOT_VALUES.get(slot[0][0], 0)) for slot in info.slots]
    w = protocol.Writer()
    w.put(command_id, *(indices + [0] * (4 - len(indices))), value)
    return w.buffer()


def unhandled_commands(make_handler) -> set[int]:
    """The CmdIds which a fresh handler (make_handler()) does not handle (command_record)."""
    unhandled = set()
    for command_id in sorted(protocol.CMD_IDS):
        handler = make_handler()
        handler.process_commands(command_record(command_id))
        if handler.unhandled:
            unhandled.add(command_id)
    return unhandled


def _escape(text: str) -> str:
    return str(text).replace("\\", "\\\\").replace("\t", "\\t").replace("\n", "\\n")


def legacy_record(processor: str, command: str, value=None) -> bytes:
    """The buffer of one command given in the core's legacy text form (``processor``,
    ``command``, the value as the text callbacks had it: None, an int, a float, a str, a list
    of str), converted by the core (tests). Raises ValueError for a command which is not in the
    table (the core cannot send it)."""
    if value is None:
        kinds = [("noArg", "")]
    elif isinstance(value, list):
        kinds = [("strings", "\t".join(_escape(v) for v in value))]
    elif isinstance(value, str):
        kinds = [("string", _escape(value))]
    elif isinstance(value, float):
        kinds = [("double", repr(value))] + ([("int", str(int(value)))] if value.is_integer() else [])
    else:
        kinds = [("int", str(int(value))), ("double", repr(float(value)))]
    for kind, text in kinds:
        line = f"{kind}\t{processor}\t{command}" + (f"\t{text}" if kind != "noArg" else "")
        buffer = drivenbymoss.messages_from_text(line)
        if len(buffer) > protocol.HEADER_SIZE:
            return buffer
    raise ValueError(f"Not a command of the protocol table: {processor} {command} {value!r}")


def run_command(handler, processor: str, command: str, value=None) -> None:
    """Execute a command given in the legacy text form (legacy_record) with the handler's
    process_commands (tests)."""
    handler.process_commands(legacy_record(processor, command, value))
