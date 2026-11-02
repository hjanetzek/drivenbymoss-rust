# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Runs a scenario file in process: the core of the setup's controller against the simulated
DAW, as ``conformance_runner.record`` runs it against Bitwig, then compares the run with the
reference results of the Java version (``conformance_runner.compare``).

The sequence follows ``record`` (``conformance_all.sh`` runs it with ``--fresh-project 8
--restart 15``), on the simulated clock (``Bridge.advance``, the core runs on it through
``dbm_set_time``; nothing sleeps):

1. The Flexi mapping of the file (``write_flexi_mapping``) and the controller settings of the
   profile (``fixture.copy_profile_config``) in a temporary config folder.
2. The restarts (``restart_controller``): a new ``App`` each, without the device stubs, then
   the waits of ``restart_controller`` (``RESTART_*``); the fresh project (``FRESH_PROJECT_MS``); the marker pause (1 s).
   The fixture is built once, before the first start.
3. The ``setup`` steps; their output is the start of the LED state (``led_state``), or the
   startup output too with ``keep_startup_output``.
4. The ``require`` check, then per scenario the ``before`` and ``steps`` (``run_step``: OSC
   commands with 50 ms between them, MIDI with ``interval_ms``, then ``wait_ms`` and the settle
   time), the LED state and the state view after each step, the expectations
   (``check_expectations``, for the label ``rust``: the core is the Rust port).

The run fails on a difference, a failed expectation, a command which the reference host does not
handle (``CommandHandler.unhandled``), an OSC command which the table does not know, and an
error logged by the core. Keys the Java run changed which the state view does not model are
counted separately ("not modelled").
"""

import contextlib
import io
import json
import tempfile
import tomllib
import types
from dataclasses import dataclass, field
from pathlib import Path

from conformance_runner import DEFAULT_SETTLE_MS, check_expectations, compare, diff, led_state, matches, parse_hex, run_name, to_hex, write_flexi_mapping

from dbm_host import Bridge

from . import osc, reference
from .fixture import Setup, build_fixture, copy_profile_config
from .state import OscController

# The pauses of record (in ms of simulated time)
RESTART_WAIT_MS = 3000
RESTART_REST_MS = 1000
RESTART_TIMEOUT_MS = 15000
"""restart_controller: 3 s, then until the controller sent something (at most the timeout of
``--restart 15``), then the rest of the startup output (1 s). For Generic Flexi the first output
comes after the mapping is applied (``FLEXI_MAPPING_FILE``)."""
FRESH_PROJECT_MS = 10000
"""--fresh-project 8: the wait for the new project, the engine, the stops (about 10 s)."""
MARKER_MS = 1000
OSC_PAUSE_MS = 50
LABEL = "rust"
"""The label of the expectations (``expect_labels``): the core under test is the Rust port."""
TICK_MS = 30


@dataclass
class FileResult:
    """The result of one scenario file."""

    setup: str
    file: str
    run: str
    scenarios: int = 0
    same: int = 0
    different: int = 0
    failed_expectations: int = 0
    not_modelled: dict[str, list[str]] = field(default_factory=dict)
    unhandled: list[str] = field(default_factory=list)
    unknown_osc: list[str] = field(default_factory=list)
    core_errors: list[str] = field(default_factory=list)
    has_reference: bool = True
    output: list[str] = field(default_factory=list)

    @property
    def not_modelled_count(self) -> int:
        return len({key for keys in self.not_modelled.values() for key in keys})

    @property
    def ok(self) -> bool:
        return not (self.different or self.failed_expectations or self.unhandled or self.unknown_osc or self.core_errors)


class Session:
    """The controller (an ``App`` in a ``Bridge``) on the fixture, with the device stubs and the
    observer (``OscController``)."""

    def __init__(self, setup: Setup, config_dir: Path):
        self.setup = setup
        self.config_dir = config_dir
        self.daw = build_fixture(setup.preferences())
        self.osc = OscController(self.daw, clock=lambda: self.daw.time_ms)
        self.bridge: Bridge | None = None
        self.stubs: list[dict] = []
        self.received: list[list[int]] = []
        self.unhandled: list[str] = []
        self.core_errors: list[str] = []
        self.unknown_osc: list[str] = []

    # The controller

    def start(self) -> None:
        """A (new) start of the controller: restart_controller in Bitwig."""
        self.close()
        self.bridge = Bridge(self.daw, self.setup.controller_id, str(self.config_dir), self.setup.app_version)
        self.bridge.on_midi_out = self._on_midi_out
        self.bridge.start()

    def close(self) -> None:
        if self.bridge is not None:
            self._collect_problems()
            self.bridge.close()
            self.bridge = None

    def _collect_problems(self) -> None:
        for key in self.bridge.commands.unhandled:
            if key not in self.unhandled:
                self.unhandled.append(key)
        for error in self.bridge.core_errors():
            if error not in self.core_errors:
                self.core_errors.append(error)

    def _on_midi_out(self, data: bytes) -> None:
        message = list(data)
        self.received.append(message)
        for stub in self.stubs:
            if matches(stub["match"], message):
                self.bridge.receive(bytes(message if stub["reply"] == "echo" else parse_hex(stub["reply"])))

    def take(self) -> list[list[int]]:
        result, self.received = self.received, []
        return result

    # Time and input

    def advance(self, ms: float) -> None:
        """Simulated time in ticks; the observer's tasks run in between."""
        remaining = float(ms)
        while True:
            delta = min(TICK_MS, remaining)
            self.bridge.advance(delta)
            self.osc.run_tasks()
            remaining -= delta
            if remaining <= 0:
                break

    def send_osc(self, text: str) -> None:
        try:
            osc.execute(self.osc, text)
        except osc.UnknownOscCommand as error:
            key = str(error)
            if key not in self.unknown_osc:
                self.unknown_osc.append(key)

    def run_step(self, step: dict, settle_ms: float) -> None:
        """conformance_runner.run_step on the simulated clock."""
        for _ in range(step.get("repeat", 1)):
            for command in step.get("osc", "").split("|"):
                if command.strip():
                    self.send_osc(command.strip())
                    self.advance(OSC_PAUSE_MS)
            for message in step.get("midi", "").split("|"):
                if message.strip():
                    self.bridge.receive(bytes(parse_hex(message)))
                    self.advance(step.get("interval_ms", 0))
        self.advance(step.get("wait_ms", 0) + settle_ms)

    def snapshot(self) -> dict[str, str]:
        return self.osc.snapshot()


def record(setup: Setup, scenario_file: Path, config: dict, work_dir: Path) -> tuple[dict, Session, int]:
    """Run the scenario file; returns the trace (the format of conformance_runner.record), the
    session and the number of failed expectations."""
    settle = config.get("settle_ms", DEFAULT_SETTLE_MS)
    sysex_key_bytes = config.get("sysex_key_bytes", 0)
    config_dir = work_dir / "config"
    config_dir.mkdir()
    mapping = work_dir / "flexi-mapping.properties"
    with contextlib.redirect_stdout(io.StringIO()):
        restarts = write_flexi_mapping(config, types.SimpleNamespace(scenarios=str(scenario_file), flexi_mapping=str(mapping), restart=1))
    copy_profile_config(setup, config_dir, mapping if "mapping" in config else None)

    session = Session(setup, config_dir)
    startup = []
    for _ in range(restarts):
        session.stubs = []
        session.start()
        session.advance(RESTART_WAIT_MS)
        waited = RESTART_WAIT_MS
        while not session.received and waited < RESTART_TIMEOUT_MS:
            session.advance(100)
            waited += 100
        session.advance(RESTART_REST_MS)
        startup = session.take()
    session.stubs = config.get("stub", [])
    session.advance(FRESH_PROJECT_MS + MARKER_MS)
    startup += session.take()
    leds: dict[str, str] = {}
    if config.get("keep_startup_output", False):
        led_state(startup, leds, sysex_key_bytes)
    for step in config.get("setup", []):
        session.run_step(step, settle)
    setup_output = session.take()
    led_state(setup_output, leds, sysex_key_bytes)
    session.advance(settle)
    trace = {"controller": config["controller"], "manual": config.get("manual", ""), "label": LABEL, "version": "dbm_host",
             "known_differences": config.get("known_difference", []), "scenarios": [],
             "startup_output": [to_hex(m) for m in startup], "setup_output": [to_hex(m) for m in setup_output]}
    missing = check_expectations({"expect": {"bitwig": config.get("require", {})}}, {"bitwig": session.snapshot(), "leds": {}})
    trace["fixture_problems"] = missing
    failures = 0
    for scenario in config["scenario"]:
        session.advance(settle)
        led_state(session.take(), leds, sysex_key_bytes)
        state = session.snapshot()
        entry = {"id": scenario["id"], "bullet": scenario.get("bullet", ""), "initial": {"bitwig": state, "leds": dict(leds)}, "steps": []}
        for step in scenario.get("before", []):
            session.run_step(step, settle)
        led_state(session.take(), leds, sysex_key_bytes)
        state = session.snapshot()
        entry["start"] = state
        for step in scenario.get("steps", []):
            session.run_step(step, settle)
            led_state(session.take(), leds, sysex_key_bytes)
            new_state = session.snapshot()
            entry["steps"].append({"input": step, "bitwig": diff(state, new_state), "leds": dict(leds)})
            state = new_state
        entry["final"] = {"bitwig": state, "leds": dict(leds)}
        problems = check_expectations(scenario, entry["final"]) if LABEL in scenario.get("expect_labels", [LABEL]) else []
        entry["expectation_problems"] = problems
        failures += bool(problems)
        trace["scenarios"].append(entry)
    session.close()
    return trace, session, failures


def run_file(setup: Setup, scenario_file: Path, keep: Path | None = None) -> FileResult:
    """Run and compare one scenario file. With keep, the in-process trace and the expanded
    reference are written there (``<run>-sim.json``, ``<run>-ref.json``)."""
    scenario_file = Path(scenario_file)
    config = tomllib.loads(scenario_file.read_text())
    run = run_name(config, str(scenario_file))
    result = FileResult(setup.name, scenario_file.stem, run, scenarios=len(config.get("scenario", [])))
    out = result.output
    with tempfile.TemporaryDirectory(prefix="dbm-conformance-") as temp:
        work = Path(temp)
        trace, session, failures = record(setup, scenario_file, config, work)
        result.failed_expectations = failures
        result.unhandled = session.unhandled
        result.unknown_osc = session.unknown_osc
        result.core_errors = session.core_errors
        if trace["fixture_problems"]:
            out.append("The fixture is not ready (tests/conformance/FIXTURE.md):")
            out.extend(f"       {problem}" for problem in trace["fixture_problems"])
            result.failed_expectations += 1
        for entry in trace["scenarios"]:
            out.append(f"{'FAIL' if entry['expectation_problems'] else 'ok  '} {entry['id']}")
            out.extend(f"       {problem}" for problem in entry["expectation_problems"])

        ref_path = reference.path_for(scenario_file, run)
        if not ref_path.exists():
            result.has_reference = False
            out.append(f"NOREF {ref_path.name}")
        else:
            ref = reference.load(ref_path)
            oracle = reference.expand(ref, config.get("known_difference", []))
            inputs = {s["id"]: s.get("steps", []) for s in config["scenario"]}
            for scenario in oracle["scenarios"]:
                for step, step_input in zip(scenario["steps"], inputs.get(scenario["id"], [])):
                    step["input"] = step_input
            result.not_modelled = reference.not_modelled(ref)
            oracle_path, port_path = work / "oracle.json", work / "port.json"
            oracle_path.write_text(json.dumps(oracle))
            port_path.write_text(json.dumps(trace))
            with contextlib.redirect_stdout(io.StringIO()) as captured:
                compare(types.SimpleNamespace(oracle=str(oracle_path), port=str(port_path), ignore=None))
            lines = captured.getvalue().splitlines()
            # As conformance_all.sh counts them
            result.same = sum(line.startswith("same") for line in lines)
            result.different = sum(line.startswith(("DIFF", "MISSING")) for line in lines)
            out.extend(lines)
            for scenario_id, keys in result.not_modelled.items():
                out.append(f"NOT MODELLED {scenario_id}: {len(keys)} keys ({', '.join(keys[:4])}{', …' if len(keys) > 4 else ''})")
        if keep is not None:
            keep.mkdir(parents=True, exist_ok=True)
            (keep / f"{run}-sim.json").write_text(json.dumps(trace, indent=1, sort_keys=True))
            if result.has_reference:
                (keep / f"{run}-ref.json").write_text(json.dumps(oracle, indent=1, sort_keys=True))
    for key in result.unhandled:
        out.append(f"UNHANDLED {key}")
    for key in result.unknown_osc:
        out.append(f"UNKNOWN OSC {key}")
    for error in result.core_errors:
        out.append(f"CORE {error}")
    return result


def summary(setup: str, results: list[FileResult]) -> str:
    """A line in the style of conformance_all.sh."""
    same = sum(r.same for r in results)
    different = sum(r.different for r in results)
    failed = sum(r.failed_expectations for r in results)
    not_modelled = sum(r.not_modelled_count for r in results)
    unhandled = sorted({key for r in results for key in r.unhandled + r.unknown_osc})
    errors = sum(len(r.core_errors) for r in results)
    missing = [r.file for r in results if not r.has_reference]
    line = f"{setup}: {same} same, {different} different, {failed} failed expectations; {not_modelled} not modelled keys"
    line += f"; {len(unhandled)} unhandled commands; {errors} core errors"
    if missing:
        line += f"; no reference: {' '.join(missing)}"
    return line


__all__ = ["FileResult", "Session", "record", "run_file", "summary"]
