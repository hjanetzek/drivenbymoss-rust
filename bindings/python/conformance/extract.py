# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""Write the reference results (``reference``) from the traces of the Java runs on Bitwig.

    python3 -m conformance.extract [--traces DIR] [setup ...]      (all setups if none given)

For every scenario file of a setup it reads ``<traces>/<run>-java.json`` (default
``target/conformance``; ``<run>`` as ``conformance_runner.run_name``) and writes
``tests/conformance/<setup>/replay/<run>-java.ref.json``. A file without a trace is skipped.

Update the references together with the replays, from clean runs only (the conformance-run
skill): the Java run of a file with differences or failed expectations is no reference. As a
check, a trace with failed expectations and a pair whose Rust trace (``<run>-rust.json``) differs
in ``conformance_runner.compare`` are reported; their reference is written anyway (the caller
decides whether to keep it).

The reference keeps only the keys the state view models (``state.is_modelled``): when the view
grows, extract again from the traces.
"""

import argparse
import contextlib
import io
import json
import sys
import tomllib
import types
from pathlib import Path

from conformance_runner import compare, run_name

from . import reference
from .fixture import REPOSITORY, SETUPS


def extract_file(scenario_file: Path, traces: Path) -> tuple[Path | None, str]:
    """Write the reference of one scenario file; returns its path (None if there is no trace) and
    a note."""
    config = tomllib.loads(scenario_file.read_text())
    run = run_name(config, str(scenario_file))
    source = traces / f"{run}-java.json"
    if not source.exists():
        return None, f"no trace {source.name}"
    trace = json.loads(source.read_text())
    ref = reference.reduce_trace(trace)
    target = reference.path_for(scenario_file, run)
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(reference.dumps(ref))
    notes = []
    failed = [s["id"] for s in trace["scenarios"] if s.get("expectation_problems")]
    if failed:
        notes.append(f"failed expectations in the Java run: {', '.join(failed)}")
    rust = traces / f"{run}-rust.json"
    if rust.exists():
        with contextlib.redirect_stdout(io.StringIO()) as out:
            code = compare(types.SimpleNamespace(oracle=str(source), port=str(rust), ignore=None))
        if code:
            different = [line.split()[1] for line in out.getvalue().splitlines() if line.startswith(("DIFF ", "MISSING "))]
            notes.append(f"Java and Rust differ: {', '.join(different)}")
    count = sum(len(keys) for keys in reference.not_modelled(ref).values())
    notes.append(f"{target.stat().st_size} bytes, {count} not modelled keys")
    return target, "; ".join(notes)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Write the reference results of the in-process conformance runs")
    parser.add_argument("setups", nargs="*", help=f"setups ({', '.join(SETUPS)}); all if none")
    parser.add_argument("--traces", default=str(REPOSITORY / "target" / "conformance"), help="the folder of the Java traces")
    args = parser.parse_args(argv)
    unknown = [name for name in args.setups if name not in SETUPS]
    if unknown:
        parser.error(f"unknown setups: {', '.join(unknown)}")
    for name in args.setups or list(SETUPS):
        for scenario_file in SETUPS[name].scenario_files():
            target, note = extract_file(scenario_file, Path(args.traces))
            shown = target.relative_to(REPOSITORY) if target is not None else scenario_file.relative_to(REPOSITORY)
            print(f"{shown}: {note}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
