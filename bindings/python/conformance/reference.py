# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The reference results: what the Java DrivenByMoss did on Bitwig, reduced to what the
in-process comparison needs.

``tests/conformance/<setup>/replay/<run>-java.ref.json`` (next to ``<run>-java.leds`` and
``-java.known``; ``<run>`` is ``conformance_runner.run_name``) is written by ``extract`` from the
trace of a Bitwig run (``target/conformance/<run>-java.json``, about 5 MB, not committed). It
keeps:

- The LED state of the first scenario's start (``leds``), then per scenario and step only the
  LEDs which changed.
- The Bitwig state: of the keys the state view models (``state.is_modelled``), the state at the
  start of the first scenario, the changes until the start of each further scenario, and the
  changes of every step (``bitwig``).
- Per step the keys the Java run changed which the state view does not model
  (``not_modelled``), without the keys the comparison ignores (``DEFAULT_IGNORE``).

``expand`` turns it back into a trace in the format of ``conformance_runner.record`` (``start``
and step changes of the Bitwig state, the full LED state after every step), which
``conformance_runner.compare`` takes as the oracle.
"""

import json
from pathlib import Path

from conformance_runner import DEFAULT_IGNORE, diff, ignored

from .state import is_modelled

FORMAT = 1


def reduce_trace(trace: dict) -> dict:
    """The reference of a Java trace (see the module docstring)."""
    scenarios = []
    leds_before: dict[str, str] | None = None
    bitwig_before: dict[str, str] = {}
    for scenario in trace["scenarios"]:
        leds = scenario["initial"]["leds"]
        start = {k: v for k, v in scenario.get("start", {}).items() if is_modelled(k)}
        entry = {
            "id": scenario["id"],
            "leds": {} if leds_before is None else diff(leds_before, leds),
            "start": diff(bitwig_before, start),
            "steps": [],
        }
        if leds_before is None:
            first_leds = dict(leds)
        leds_before = leds
        state = dict(start)
        for step in scenario["steps"]:
            changes = step["bitwig"]
            entry["steps"].append({
                "leds": diff(leds_before, step["leds"]),
                "bitwig": {k: v for k, v in changes.items() if is_modelled(k)},
                "not_modelled": sorted(k for k in changes if not is_modelled(k) and not ignored(k, DEFAULT_IGNORE)),
            })
            leds_before = step["leds"]
            state.update(entry["steps"][-1]["bitwig"])
        bitwig_before = state
        scenarios.append(entry)
    return {
        "format": FORMAT,
        "controller": trace["controller"],
        "label": trace.get("label", "java"),
        "version": trace.get("version", "?"),
        "leds": first_leds if trace["scenarios"] else {},
        "scenarios": scenarios,
    }


def expand(reference: dict, known_differences: list | None = None) -> dict:
    """The reference as a trace for ``conformance_runner.compare`` (the oracle)."""
    leds = dict(reference["leds"])
    bitwig: dict[str, str] = {}
    scenarios = []
    for scenario in reference["scenarios"]:
        leds.update(scenario["leds"])
        bitwig.update(scenario["start"])
        entry = {"id": scenario["id"], "start": dict(bitwig), "steps": []}
        for step in scenario["steps"]:
            leds.update(step["leds"])
            bitwig.update(step["bitwig"])
            entry["steps"].append({"input": step.get("input", {}), "bitwig": step["bitwig"], "leds": dict(leds)})
        scenarios.append(entry)
    return {
        "controller": reference["controller"],
        "label": reference.get("label", "java"),
        "version": reference.get("version", "?"),
        "known_differences": list(known_differences or []),
        "scenarios": scenarios,
    }


def not_modelled(reference: dict) -> dict[str, list[str]]:
    """The keys the Java run changed which the state view does not model, per scenario."""
    result = {}
    for scenario in reference["scenarios"]:
        keys = sorted({key for step in scenario["steps"] for key in step["not_modelled"]})
        if keys:
            result[scenario["id"]] = keys
    return result


def path_for(scenario_file: Path, run: str) -> Path:
    """``tests/conformance/<setup>/replay/<run>-java.ref.json``."""
    return Path(scenario_file).resolve().parent / "replay" / f"{run}-java.ref.json"


def load(path: Path) -> dict:
    return json.loads(Path(path).read_text())


def dumps(reference: dict) -> str:
    """Stable and diffable: one scenario step per line."""
    lines = ["{"]
    head = {k: reference[k] for k in ("format", "controller", "label", "version")}
    for key, value in head.items():
        lines.append(f" {json.dumps(key)}: {json.dumps(value)},")
    lines.append(f' "leds": {json.dumps(reference["leds"], sort_keys=True)},')
    lines.append(' "scenarios": [')
    for i, scenario in enumerate(reference["scenarios"]):
        lines.append("  {" + f'"id": {json.dumps(scenario["id"])},')
        lines.append(f'   "leds": {json.dumps(scenario["leds"], sort_keys=True)},')
        lines.append(f'   "start": {json.dumps(scenario["start"], sort_keys=True)},')
        lines.append('   "steps": [')
        steps = [f'    {json.dumps(step, sort_keys=True)}' for step in scenario["steps"]]
        lines.append(",\n".join(steps))
        lines.append("   ]}" + ("," if i + 1 < len(reference["scenarios"]) else ""))
    lines.append(" ]")
    lines.append("}")
    return "\n".join(lines) + "\n"


__all__ = ["dumps", "expand", "load", "not_modelled", "path_for", "reduce_trace"]
