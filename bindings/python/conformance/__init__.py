# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The conformance scenarios in process: the core against the simulated DAW of the reference
host, compared with the results of the Java DrivenByMoss on Bitwig (doc/PYTHON_HOST_PLAN.md §4).

Run from ``bindings/python`` (after ``cargo build``)::

    python3 -m conformance launchcontrol_xl track_mix      # one file of a setup
    python3 -m conformance launchcontrol_xl                # all files of a setup
    python3 -m conformance --all                           # all setups
    python3 -m conformance.extract [setup ...]             # the reference results from the traces

Modules:

- ``runner``: runs a scenario file as ``backends/bitwig/tools/conformance_runner.py record``
  does, and compares with its ``compare``.
- ``fixture``: the fixture project (tests/conformance/FIXTURE.md) and the setups.
- ``osc``: the scenarios' OSC commands as model operations.
- ``state``: the state view, the keys of the observer (``osc:/…``) from the model.
- ``reference``: the reference results (``tests/conformance/<setup>/replay/<run>-java.ref.json``).
- ``extract``: writes them from the Java traces (``target/conformance/<run>-java.json``).

The scenario format, the LED state, the expectations, the comparison and the Flexi mapping are
the functions of ``conformance_runner`` (imported from ``backends/bitwig/tools``, not copied).
"""

import sys
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[3] / "backends" / "bitwig" / "tools"
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))
PYTHON = Path(__file__).resolve().parents[1]
if str(PYTHON) not in sys.path:
    sys.path.insert(0, str(PYTHON))

import conformance_runner  # noqa: E402,F401  (the Bitwig runner: scenario format, comparison)
