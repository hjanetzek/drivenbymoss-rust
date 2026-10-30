#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["python-rtmidi"]
# ///
# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt
#
# The scenario runner of the conformance tests (doc/CONFORMANCE_TESTING_PLAN.md). It plays the
# MIDI of a scenario into a controller extension in Bitwig (through a virtual MIDI port which the
# extension uses in place of the device), answers like the device (stub), and records after each
# step what the extension sent back (LEDs, displays) and the state of Bitwig: from the OSC
# controller of the Java DrivenByMoss (UDP 9000; with the conformance module of the branch
# conformance-osc and DBM_OSC_CONFORMANCE set it also reports the clip steps and the playing
# notes) and, optionally, the probe of the Rust extension (UDP 9100).
#
#   conformance_runner.py ports
#   conformance_runner.py record SCENARIOS.toml --label java|rust --port "VirMIDI 5-2" [--out DIR]
#   conformance_runner.py compare ORACLE.json PORT.json
#   conformance_runner.py pair SCENARIOS.toml... --java-port "VirMIDI 5-2" --rust-port "VirMIDI 5-3"
#
# "record" writes DIR/<controller>-<label>.json and checks the expectations of the scenarios;
# "compare" compares the run of the port with the run of the oracle.

import argparse
import fnmatch
import json
import socket
import sys
import threading
import time
import tomllib
from pathlib import Path

from osc_monitor import build_message, parse_message

OSC_PORT = 9000
PROBE_PORT = 9100
DEFAULT_SETTLE_MS = 400
# Values which change by themselves
DEFAULT_IGNORE = ["*/vu", "*/vu/*", "/time*", "/beats*", "/beat*", "/position*", "/play/*"]


# ------------------------------------------------------------------------------------------------
# Observers


class StateListener:
    """Collects the state of Bitwig from the OSC controller and the probe."""

    def __init__(self):
        self.state = {}
        self.lock = threading.Lock()
        self.sockets = []

    def start(self):
        for port, handler in ((OSC_PORT, self._osc), (PROBE_PORT, self._probe)):
            sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            sock.bind(("127.0.0.1" if port == PROBE_PORT else "0.0.0.0", port))
            self.sockets.append(sock)
            threading.Thread(target=self._receive, args=(sock, handler), daemon=True).start()
        self.request_dump()

    def stop(self):
        for sock in self.sockets:
            sock.close()
        self.sockets = []

    def request_dump(self):
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
            sock.sendto(b"dump", ("127.0.0.1", PROBE_PORT + 1))
            # The OSC controller sends everything on /refresh (port 8000)
            sock.sendto(build_message("/refresh", []), ("127.0.0.1", 8000))

    def _receive(self, sock, handler):
        while True:
            try:
                data, _ = sock.recvfrom(65536)
            except OSError:
                return
            handler(data)

    def _osc(self, data):
        with self.lock:
            for address, values in parse_message(data):
                self.state["osc:" + address] = " ".join(str(v) for v in values)

    def _probe(self, data):
        with self.lock:
            for line in data.decode("utf-8", "replace").splitlines():
                address, _, value = line.partition(" ")
                if address:
                    self.state["probe:" + address] = value

    def snapshot(self):
        with self.lock:
            return dict(self.state)


class MidiPort:
    """The virtual MIDI port in place of the device; records what the extension sends."""

    def __init__(self, name, stubs):
        import rtmidi

        self.midi_in = rtmidi.MidiIn()
        self.midi_in.ignore_types(sysex=False, timing=True, active_sense=True)
        self.midi_out = rtmidi.MidiOut()
        self.stubs = stubs
        self.received = []
        self.lock = threading.Lock()
        self.midi_in.open_port(find_port(self.midi_in.get_ports(), name))
        self.midi_out.open_port(find_port(self.midi_out.get_ports(), name))
        self.midi_in.set_callback(self._on_message)

    def _on_message(self, event, _data=None):
        message, _ = event
        with self.lock:
            self.received.append(message)
        # The device stub answers like the hardware
        for stub in self.stubs:
            if matches(stub["match"], message):
                reply = message if stub["reply"] == "echo" else parse_hex(stub["reply"])
                self.midi_out.send_message(reply)

    def send(self, message):
        self.midi_out.send_message(message)

    def take(self):
        with self.lock:
            result, self.received = self.received, []
            return result

    def close(self):
        # Also free the ALSA sequencer clients, there is only a limited number of them
        self.midi_in.close_port()
        self.midi_out.close_port()
        self.midi_in.delete()
        self.midi_out.delete()


def send_osc(text):
    """Send an OSC command to the Java OSC controller (receive port 8000), e.g. "/track/1/select 1"
    (to bring Bitwig into the state of a scenario, independent of the controller under test)."""
    parts = text.split()
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.sendto(build_message(parts[0], parts[1:]), ("127.0.0.1", 8000))


def run_step(port, step, settle):
    """A step: "osc" commands (separated by "|"), then "midi" messages (separated by "|"), all
    "repeat" times with "interval_ms" between the messages, then wait "wait_ms" plus the settle
    time."""
    for _ in range(step.get("repeat", 1)):
        for command in step.get("osc", "").split("|"):
            if command.strip():
                send_osc(command.strip())
                time.sleep(0.05)
        for message in step.get("midi", "").split("|"):
            if message.strip():
                port.send(parse_hex(message))
                time.sleep(step.get("interval_ms", 0) / 1000.0)
    time.sleep(step.get("wait_ms", 0) / 1000.0 + settle)


def find_port(ports, name):
    for index, port in enumerate(ports):
        if name in port:
            return index
    sys.exit(f"MIDI port '{name}' not found in: {ports}")


def parse_hex(text):
    return [int(part, 16) for part in text.split()]


def matches(pattern, message):
    parts = pattern.split()
    return len(parts) == len(message) and all(p == "??" or int(p, 16) == b for p, b in zip(parts, message))


def to_hex(message):
    return " ".join(f"{b:02X}" for b in message)


def led_state(messages, leds):
    """The last value per controller output: status and data 1 for short messages, all but the
    last data byte for system exclusive messages."""
    for message in messages:
        if message and message[0] == 0xF0:
            leds[to_hex(message[:-2])] = f"{message[-2]:02X}"
        elif len(message) == 3:
            leds[to_hex(message[:2])] = f"{message[2]:02X}"
    return leds


# ------------------------------------------------------------------------------------------------
# Recording


def run_name(config, scenarios):
    """The name of the files of a run: controller and scenario file, e.g.
    novation_launch_control_xl-note_sequencer."""
    return config["controller"].lower().replace(" ", "_") + "-" + Path(scenarios).stem


def diff(before, after):
    return {k: v for k, v in after.items() if before.get(k) != v}


# The system exclusive message which restarts a controller (see --restart)
RESTART = [0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x03, 0xF7]


def restart_controller(port_name, timeout):
    """Restart the controller (both extensions call ControllerHost.restart on RESTART if Bitwig
    runs with DBM_OSC_CONFORMANCE) and wait until it has sent its startup output: the old one
    turns its lights off at once, the new one starts about 1 s later. Returns False if it did
    not start within the timeout (seconds)."""
    startup = MidiPort(port_name, [])
    started = time.time()
    startup.send(RESTART)
    print(f"Restarting the controller on {port_name}, waiting up to {timeout} s")
    last_output = None
    while time.time() - started < timeout:
        if startup.take():
            last_output = time.time()
        elif last_output is not None and time.time() - last_output > 2.0:
            break
        time.sleep(0.1)
    startup.close()
    if last_output is None:
        print(f"The controller did not start within {timeout} s")
        return False
    print(f"Started after {last_output - started:.1f} s")
    return True


def record(args, listener=None):
    config = tomllib.loads(Path(args.scenarios).read_text())
    settle = config.get("settle_ms", DEFAULT_SETTLE_MS) / 1000.0
    own_listener = listener is None
    if own_listener:
        listener = StateListener()
        listener.start()
    restart = getattr(args, "restart", 0)
    if restart and not restart_controller(args.port, restart):
        return 2
    port = MidiPort(args.port, config.get("stub", []))
    leds = {}
    name = run_name(config, args.scenarios) + f"-{args.label}"
    # The Rust extension records the protocol of the run for the replay test (§5 of the plan)
    marker = (lambda kind: port.send([0xF0, 0x7D, 0x44, 0x42, 0x4D, kind] + list(name.encode("ascii")) + [0xF7])) if args.label == "rust" else (lambda kind: None)
    try:
        if getattr(args, "fresh_project", 0):
            # A new project from the default template (the fixture): a clean copy for this run
            print(f"New project from the default template, waiting {args.fresh_project} s")
            send_osc("/conformance/action New")
            time.sleep(args.fresh_project)
        marker(1)
        time.sleep(1.0)
        led_state(port.take(), leds)
        # Bring the controller into a known state, e.g. a full redraw of the LEDs
        for step in config.get("setup", []):
            run_step(port, step, settle)
        led_state(port.take(), leds)
        # The fixture must be loaded, otherwise the scenarios fail for the wrong reason
        listener.request_dump()
        time.sleep(settle)
        missing = check_expectations({"expect": {"bitwig": config.get("require", {})}}, {"bitwig": listener.snapshot(), "leds": {}})
        if missing:
            print("The fixture is not ready (tests/conformance/FIXTURE.md):")
            for problem in missing:
                print(f"       {problem}")
            return 2
        trace = {"controller": config["controller"], "manual": config.get("manual", ""), "label": args.label, "scenarios": [], "known_differences": config.get("known_difference", [])}
        failures = 0
        for scenario in config["scenario"]:
            listener.request_dump()
            time.sleep(settle)
            led_state(port.take(), leds)
            state = listener.snapshot()
            trace["version"] = state.get("probe:/probe/version", "?")
            entry = {"id": scenario["id"], "bullet": scenario.get("bullet", ""), "initial": {"bitwig": state, "leds": dict(leds)}, "steps": []}
            for step in scenario.get("before", []):
                run_step(port, step, settle)
            led_state(port.take(), leds)
            state = listener.snapshot()
            # The state the steps start from: the comparison uses absolute values
            entry["start"] = state
            for step in scenario.get("steps", []):
                run_step(port, step, settle)
                led_state(port.take(), leds)
                new_state = listener.snapshot()
                entry["steps"].append({"input": step, "bitwig": diff(state, new_state), "leds": dict(leds)})
                state = new_state
            entry["final"] = {"bitwig": state, "leds": dict(leds)}
            problems = check_expectations(scenario, entry["final"])
            entry["expectation_problems"] = problems
            failures += bool(problems)
            print(f"{'FAIL' if problems else 'ok  '} {scenario['id']}")
            for problem in problems:
                print(f"       {problem}")
            trace["scenarios"].append(entry)
        marker(2)
        time.sleep(0.2)
    finally:
        port.close()
        # A fresh controller without an active mode does not react on the recording of the
        # other one (e.g. the Equalizer of the Launch Control XL adds an EQ+ to a selected track)
        if restart:
            restart_controller(args.port, restart)
        if own_listener:
            listener.stop()

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    path = out / f"{name}.json"
    path.write_text(json.dumps(trace, indent=1, sort_keys=True))
    # For the replay test: the final LED state and the known differences as text
    (out / f"{name}.leds").write_text("".join(f"{k}\t{v}\n" for k, v in sorted(leds.items())))
    (out / f"{name}.known").write_text("".join(f"{k['match']}\t{k.get('reason', '')}\n" for k in trace["known_differences"]))
    print(f"Trace written to {path}")
    return 1 if failures else 0


def check_expectations(scenario, final):
    """Check the expectations: "bitwig" maps addresses (OSC unless prefixed with "probe:", may
    contain wildcards: the first match which is not "0" counts, e.g. the step with a note, so
    "0" means that no match has a value) to a value or to { field = n, value = v } for the n-th
    value of a line (e.g. of a clip step); "leds" maps output keys to values."""
    problems = []
    expect = scenario.get("expect", {})
    for address, value in expect.get("bitwig", {}).items():
        key = address if ":" in address else "osc:" + address
        matching = sorted(k for k in final["bitwig"] if fnmatch.fnmatchcase(k, key) and final["bitwig"][k] != "0")
        actual = final["bitwig"][matching[0]] if matching else None
        if isinstance(value, dict):
            fields = (actual or "").split()
            actual = fields[value["field"]] if value["field"] < len(fields) else None
            value = value["value"]
        # An empty step is "0" or not reported at all
        if not same_value(actual, str(value)) and not (actual is None and str(value) == "0"):
            problems.append(f"bitwig {address}: expected {value}, got {actual}")
    for key, value in expect.get("leds", {}).items():
        actual = final["leds"].get(key)
        if actual != str(value).upper():
            problems.append(f"led {key}: expected {value}, got {actual}")
    return problems


def same_value(a, b, tolerance=1e-3):
    if a == b:
        return True
    try:
        return abs(float(a) - float(b)) <= tolerance
    except (TypeError, ValueError):
        return False


# ------------------------------------------------------------------------------------------------
# Comparison


def compare(args):
    oracle = json.loads(Path(args.oracle).read_text())
    port = json.loads(Path(args.port).read_text())
    ignore = DEFAULT_IGNORE + list(args.ignore or [])
    port_scenarios = {s["id"]: s for s in port["scenarios"]}
    # Known differences (e.g. bugs of the Java version): "match" is a pattern of "<kind> <key>"
    known = oracle.get("known_differences", [])
    known_seen = {}
    print(f"{oracle['controller']}: {oracle['label']} ({oracle.get('version')}) vs. {port['label']} ({port.get('version')})")
    failures = 0
    for scenario in oracle["scenarios"]:
        other = port_scenarios.get(scenario["id"])
        if other is None:
            print(f"MISSING {scenario['id']}")
            failures += 1
            continue
        differences = []
        # Bitwig: the absolute values after each step of the keys which changed in one of the
        # runs (the runs can start from different states, e.g. the Rust run from the end of the
        # Java run); LEDs: the state after each step
        state_a = dict(scenario.get("start", {}))
        state_b = dict(other.get("start", {}))
        for index, (a, b) in enumerate(zip(scenario["steps"], other["steps"])):
            label = a["input"].get("note", a["input"].get("midi", ""))
            state_a.update(a["bitwig"])
            state_b.update(b["bitwig"])
            values = {"bitwig": (state_a, state_b), "leds": (a["leds"], b["leds"])}
            for kind in ("bitwig", "leds"):
                keys = set(a[kind]) | set(b[kind])
                for key in sorted(keys):
                    if kind == "bitwig" and ignored(key, ignore):
                        continue
                    value_a, value_b = values[kind][0].get(key), values[kind][1].get(key)
                    if not same_value(value_a, value_b):
                        rule = next((k for k in known if fnmatch.fnmatchcase(f"{kind} {key}", k["match"])), None)
                        if rule is not None:
                            known_seen[rule["match"]] = rule.get("reason", "")
                            continue
                        differences.append(f"step {index + 1} ({label}) {kind} {key}: {value_a} | {value_b}")
        if len(scenario["steps"]) != len(other["steps"]):
            differences.append(f"different number of steps: {len(scenario['steps'])} | {len(other['steps'])}")
        failures += bool(differences)
        print(f"{'DIFF' if differences else 'same'} {scenario['id']}")
        for line in differences:
            print(f"       {line}")
    for pattern, reason in known_seen.items():
        print(f"KNOWN {pattern}: {reason}")
    return 1 if failures else 0


def ignored(key, patterns):
    address = key.split(":", 1)[-1]
    return any(fnmatch.fnmatch(address, p) for p in patterns)


def pair(args):
    """Record the oracle and the port (both extensions active, each on its own MIDI port), then
    compare."""
    import types

    # One listener for all runs: the UDP ports are not free again right after closing them
    listener = StateListener()
    listener.start()
    failures = 0
    if args.restart:
        for port in (args.java_port, args.rust_port):
            if not restart_controller(port, args.restart):
                return 2
    for scenarios in args.scenarios:
        paths = []
        config = tomllib.loads(Path(scenarios).read_text())
        for label, port in (("java", args.java_port), ("rust", args.rust_port)):
            print(f"--- {Path(scenarios).stem}: {label} on {port}")
            result = record(types.SimpleNamespace(scenarios=scenarios, label=label, port=port, out=args.out, fresh_project=args.fresh_project, restart=args.restart), listener)
            if result == 2:
                return result
            paths.append(Path(args.out) / (run_name(config, scenarios) + f"-{label}.json"))
        print(f"--- {Path(scenarios).stem}: compare")
        failures += compare(types.SimpleNamespace(oracle=paths[0], port=paths[1], ignore=args.ignore))
    return 1 if failures else 0


def list_ports(_args):
    import rtmidi

    print("Inputs: ", rtmidi.MidiIn().get_ports())
    print("Outputs:", rtmidi.MidiOut().get_ports())
    return 0


def main():
    parser = argparse.ArgumentParser(description="Conformance scenario runner")
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("ports", help="list the MIDI ports").set_defaults(func=list_ports)
    rec = commands.add_parser("record", help="run the scenarios and write a trace")
    rec.add_argument("scenarios")
    rec.add_argument("--label", required=True, help="java or rust")
    rec.add_argument("--port", required=True, help="part of the name of the virtual MIDI port")
    rec.add_argument("--out", default="target/conformance")
    rec.add_argument("--fresh-project", type=float, default=0, metavar="SECONDS", help="start with a new project from the default template (the fixture) and wait up to SECONDS until it has sent its startup output")
    rec.add_argument("--restart", type=float, default=0, metavar="SECONDS", help="restart the controller before each recording (a system exclusive message which both extensions handle with DBM_OSC_CONFORMANCE) and wait up to SECONDS until it has sent its startup output")
    rec.set_defaults(func=record)
    both = commands.add_parser("pair", help="record the Java and the Rust extension, then compare")
    both.add_argument("scenarios", nargs="+")
    both.add_argument("--java-port", required=True)
    both.add_argument("--rust-port", required=True)
    both.add_argument("--out", default="target/conformance")
    both.add_argument("--fresh-project", type=float, default=0, metavar="SECONDS", help="start each recording with a new project from the default template (the fixture) and wait up to SECONDS until it has sent its startup output")
    both.add_argument("--restart", type=float, default=0, metavar="SECONDS", help="restart the controller before each recording (a system exclusive message which both extensions handle with DBM_OSC_CONFORMANCE) and wait up to SECONDS until it has sent its startup output")
    both.add_argument("--ignore", action="append", help="address pattern to ignore (repeatable)")
    both.set_defaults(func=pair)
    cmp = commands.add_parser("compare", help="compare the trace of the port with the oracle")
    cmp.add_argument("oracle")
    cmp.add_argument("port")
    cmp.add_argument("--ignore", action="append", help="address pattern to ignore (repeatable)")
    cmp.set_defaults(func=compare)
    args = parser.parse_args()
    sys.exit(args.func(args))


if __name__ == "__main__":
    main()
