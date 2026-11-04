# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""``python3 -m conformance <setup> [file …]`` or ``--all`` (from ``bindings/python``, after
``cargo build``). A file is the name of a scenario file of the setup (``track_mix``,
``track_mix.toml``) or a path. ``--keep DIR`` writes the in-process traces and the expanded
references for a closer look. ``--controller-id UUID`` runs the scenarios with another
controller (e.g. the Lua port, doc/LUA_PLAN.md §4) against the same profile and references.
``--timing`` prints the time the core spent per tick. The exit status is 1 if a file failed (a difference, a failed
expectation, an unhandled command, a core error), else 0."""

import argparse
import sys
from pathlib import Path

from .fixture import SETUPS
from .runner import Timing, run_file, summary


def resolve(setup, name: str) -> Path:
    path = Path(name)
    if path.exists():
        return path
    candidate = setup.scenario_dir / (name if name.endswith(".toml") else name + ".toml")
    if not candidate.exists():
        raise SystemExit(f"No scenario file {name} in {setup.scenario_dir}")
    return candidate


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="python3 -m conformance", description="The conformance scenarios against the reference host (doc/PYTHON_HOST_PLAN.md §4)")
    parser.add_argument("setup", nargs="?", help=f"one of {', '.join(SETUPS)}")
    parser.add_argument("files", nargs="*", help="scenario files of the setup (default: all)")
    parser.add_argument("--all", action="store_true", help="all files of all setups")
    parser.add_argument("--keep", metavar="DIR", help="write the in-process traces and the expanded references to DIR")
    parser.add_argument("-q", "--quiet", action="store_true", help="only the per-file status and the summary")
    parser.add_argument("--controller-id", metavar="UUID", help="run the scenarios of the setup with this controller (same profile and references)")
    parser.add_argument("--timing", action="store_true", help="print the time the core spent in the ticks (and the MIDI and model input)")
    args = parser.parse_args(argv)
    if args.all == bool(args.setup):
        parser.error("give a setup or --all")
    if args.setup and args.setup not in SETUPS:
        parser.error(f"unknown setup {args.setup}; one of {', '.join(SETUPS)}")
    if args.controller_id and args.all:
        parser.error("--controller-id needs a setup")
    keep = Path(args.keep) if args.keep else None
    summaries = []
    failed = False
    timing = Timing() if args.timing else None
    for name in list(SETUPS) if args.all else [args.setup]:
        setup = SETUPS[name]
        if args.controller_id:
            setup = setup.with_controller(args.controller_id)
            print(f"(controller {setup.controller_id}, profile and references of {setup.profile_controller_id or setup.controller_id})")
        files = [resolve(setup, f) for f in args.files] if args.files else setup.scenario_files()
        results = []
        print(f"=== {name}")
        for path in files:
            print(f"--- {path.stem}")
            result = run_file(setup, path, keep, timing)
            if not args.quiet:
                for line in result.output:
                    print(line)
            status = "ok" if result.ok else "FAILED"
            print(f"--- {path.stem}: {status} ({result.same} same, {result.different} different, {result.failed_expectations} failed expectations, "
                  f"{result.not_modelled_count} not modelled keys, {len(result.unhandled) + len(result.unknown_osc)} unhandled commands, {len(result.core_errors)} core errors)")
            results.append(result)
            failed |= not result.ok
        summaries.append(summary(name, results))
    print("=== Summary")
    for line in summaries:
        print(line)
    if timing is not None:
        print(timing.summary())
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
