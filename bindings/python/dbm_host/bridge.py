# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Connects a DrivenByMoss ``App`` (one controller) with the simulated DAW, the Reporter and the
CommandHandler. This is the part another DAW copies: replace ``SimDaw`` by the DAW's own API.

Implements HOST_BRIDGE_GUIDE.md §1 (architecture, "Startup and shutdown": create the
controller, start the infrastructure, size the model from ``dbm_get_model_setup``, the loop),
§5 (MIDI and note inputs) and the minimal bridge checklist of §7.

Simulates ``DbmExtension`` of the Bitwig extension of this repository (``init``, ``tickLoop``,
the ``NativeCore.Callbacks``), which hosts the core in Bitwig as the Java DrivenByMoss's
``ControllerExtension`` hosts its controllers.

Time: the bridge has a simulated clock. ``advance(ms)`` moves it in ticks of 30 ms; each tick
moves the DAW (``SimDaw.advance``), gives the time to the core (``dbm_set_time``), sends the
pending MIDI, ticks the core, reports the changes and calls ``update_model``. Nothing sleeps: the
core's own timers (the 1 s startup delay, the 75 ms selection after a page change, long presses,
blinking) run on the simulated clock too.
"""

import logging
from typing import Callable, Iterable

import drivenbymoss

from .banks import Banks
from .commands import CommandHandler
from .model import SimDaw
from .reporter import Reporter

logger = logging.getLogger("dbm_host")

TICK_MS = 30
# The delay of the controller's startup in the core (STARTUP_DELAY_MS of src/app.rs) and a margin
STARTUP_DELAY_MS = 1000
STARTUP_MARGIN_MS = 60

# The device ID of the controller's MIDI ports (dbm_create_controller)
DEVICE = 0


class Bridge(drivenbymoss.Backend):
    """Runs one controller of the core against a SimDaw."""

    def __init__(self, daw: SimDaw, controller_id: str, config_path: str, app_version: str = "DrivenByMoss reference host"):
        self.daw = daw
        self.controller_id = controller_id
        self.config_path = config_path
        self.app_version = app_version
        self.clock_ms = 0.0
        self.banks = Banks()
        self.reporter = Reporter(daw, self.banks, clock=lambda: self.clock_ms)
        self.commands = CommandHandler(daw, self.banks, self.reporter, clock=lambda: self.clock_ms)
        self.reporter.features = self.commands.host_features
        self.app: drivenbymoss.App | None = None
        self.midi_in: list[bytes] = []
        """MIDI from the controller, sent to the core with the next tick."""
        self.midi_out: list[bytes] = []
        """All MIDI the core sent to the controller."""
        self.on_midi_out: Callable[[bytes], None] | None = None
        """Called with every message to the controller (e.g. a device stub which answers)."""
        self.played_notes: list[tuple[int, int, int]] = []
        """The MIDI the core played into the DAW (process_midi_arg)."""
        self.note_inputs: dict[int, dict] = {}
        self.logs: list[str] = []
        self.notifications: list[str] = []
        self.reports: list[str] = []
        """Every batch given to update_model."""

    # Lifecycle

    def start(self) -> "Bridge":
        """Create the controller, start it, size the banks and send the first (complete) report."""
        self.app = drivenbymoss.App.create_controller(self, self.controller_id, self.config_path, self.app_version)
        self.app.set_time(int(self.clock_ms))
        self.app.start_infrastructure()
        self.banks.configure(self.app.get_model_setup())
        self.app.take_setting_changes()
        self.report()
        return self

    def close(self) -> None:
        if self.app is not None:
            self.app.close()
            self.app = None

    def __enter__(self) -> "Bridge":
        return self

    def __exit__(self, *exc_info) -> None:
        self.close()

    # The loop

    def receive(self, data: bytes | Iterable[int]) -> None:
        """MIDI from the controller (sent to the core with the next step)."""
        self.midi_in.append(bytes(data))

    def report(self) -> str:
        """Collect the changes of the DAW and send them to the core."""
        lines = self.reporter.collect()
        if lines:
            self.reports.append(lines)
            self.app.update_model(lines)
        return lines

    def step(self) -> None:
        """One loop pass: MIDI in -> tick -> report changes -> update_model."""
        self.app.set_time(int(self.clock_ms))
        pending, self.midi_in = self.midi_in, []
        for data in pending:
            self.app.on_midi_event(DEVICE, data)
        self.app.tick()
        self.report()
        self.notifications.extend(self.app.take_notifications())
        self.app.take_setting_changes()

    def advance(self, ms: float) -> None:
        """Move the simulated time by ms, with a step every tick (at least one step)."""
        remaining = float(ms)
        while True:
            delta = min(TICK_MS, remaining)
            self.clock_ms += delta
            self.daw.advance(delta)
            self.step()
            remaining -= delta
            if remaining <= 0:
                break

    def wait_for_startup(self) -> None:
        """Advance the simulated time until the core ran the delayed startup of the controller."""
        self.advance(STARTUP_DELAY_MS + STARTUP_MARGIN_MS)

    def send(self, data: bytes | Iterable[int], ms: float = TICK_MS) -> None:
        """Receive a MIDI message from the controller and advance the time."""
        self.receive(data)
        self.advance(ms)

    def core_errors(self) -> list[str]:
        """The errors the core logged, e.g. ``ERROR: Unhandled …`` for an address it does not
        know."""
        return [message for message in self.logs if message.startswith("ERROR")]

    # The callbacks of the core (drivenbymoss.Backend)

    def process(self, processor: str, command: str, value) -> None:
        self.commands.handle(processor, command, value)

    def open_midi_input(self, device_id: int) -> bool:
        return device_id == DEVICE

    def open_midi_output(self, device_id: int) -> bool:
        return device_id == DEVICE

    def send_midi_data(self, device_id: int, data: bytes) -> None:
        self.midi_out.append(data)
        if self.on_midi_out is not None:
            self.on_midi_out(data)

    def process_midi_arg(self, device_id: int, status: int, data1: int, data2: int) -> None:
        self.played_notes.append((status, data1, data2))

    def set_note_input_filters(self, device_id: int, note_input_index: int, filters: list[str]) -> None:
        self.note_inputs.setdefault(note_input_index, {})["filters"] = filters

    def set_note_input_key_translation_table(self, device_id: int, note_input_index: int, table: list[int]) -> None:
        self.note_inputs.setdefault(note_input_index, {})["keys"] = table

    def set_note_input_velocity_translation_table(self, device_id: int, note_input_index: int, table: list[int]) -> None:
        self.note_inputs.setdefault(note_input_index, {})["velocities"] = table

    def log(self, message: str) -> None:
        self.logs.append(message)
        logger.debug("core: %s", message)
