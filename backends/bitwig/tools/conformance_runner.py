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
#   conformance_runner.py pair SCENARIOS.toml --java-port "VirMIDI 5-2" --rust-port "VirMIDI 5-1"
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
        self.midi_in.close_port()
        self.midi_out.close_port()


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


def diff(before, after):
    return {k: v for k, v in after.items() if before.get(k) != v}


def record(args, listener=None):
    config = tomllib.loads(Path(args.scenarios).read_text())
    settle = config.get("settle_ms", DEFAULT_SETTLE_MS) / 1000.0
    own_listener = listener is None
    if own_listener:
        listener = StateListener()
        listener.start()
    port = MidiPort(args.port, config.get("stub", []))
    leds = {}
    try:
        time.sleep(1.0)
        led_state(port.take(), leds)
        # The fixture must be loaded, otherwise the scenarios fail for the wrong reason
        missing = check_expectations({"expect": {"bitwig": config.get("require", {})}}, {"bitwig": listener.snapshot(), "leds": {}})
        if missing:
            print("The fixture is not ready:")
            for problem in missing:
                print(f"       {problem}")
            return 2
        # Bring the controller into a known state, e.g. a full redraw of the LEDs
        for step in config.get("setup", []):
            # "repeat" sends the messages of "midi" (separated by "|") several times
            for _ in range(step.get("repeat", 1)):
                for message in step.get("midi", "").split("|"):
                    if message.strip():
                        port.send(parse_hex(message))
                        time.sleep(step.get("interval_ms", 0) / 1000.0)
            time.sleep(step.get("wait_ms", 0) / 1000.0 + settle)
        led_state(port.take(), leds)
        trace = {"controller": config["controller"], "manual": config.get("manual", ""), "label": args.label, "scenarios": [], "known_differences": config.get("known_difference", [])}
        failures = 0
        for scenario in config["scenario"]:
            listener.request_dump()
            time.sleep(settle)
            led_state(port.take(), leds)
            state = listener.snapshot()
            trace["version"] = state.get("probe:/probe/version", "?")
            entry = {"id": scenario["id"], "bullet": scenario.get("bullet", ""), "initial": {"bitwig": state, "leds": dict(leds)}, "steps": []}
            for step in scenario.get("steps", []):
                if "midi" in step:
                    port.send(parse_hex(step["midi"]))
                time.sleep(step.get("wait_ms", 0) / 1000.0 + settle)
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
    finally:
        port.close()
        if own_listener:
            listener.stop()

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    name = config["controller"].lower().replace(" ", "_")
    path = out / f"{name}-{args.label}.json"
    path.write_text(json.dumps(trace, indent=1, sort_keys=True))
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
        for index, (a, b) in enumerate(zip(scenario["steps"], other["steps"])):
            label = a["input"].get("note", a["input"].get("midi", ""))
            for kind in ("bitwig", "leds"):
                for key in sorted(set(a[kind]) | set(b[kind])):
                    if kind == "bitwig" and ignored(key, ignore):
                        continue
                    if not same_value(a[kind].get(key), b[kind].get(key)):
                        rule = next((k for k in known if fnmatch.fnmatchcase(f"{kind} {key}", k["match"])), None)
                        if rule is not None:
                            known_seen[rule["match"]] = rule.get("reason", "")
                            continue
                        differences.append(f"step {index + 1} ({label}) {kind} {key}: {a[kind].get(key)} | {b[kind].get(key)}")
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

    paths = []
    # One listener for both runs: the UDP ports are not free again right after closing them
    listener = StateListener()
    listener.start()
    for label, port in (("java", args.java_port), ("rust", args.rust_port)):
        print(f"--- {label} on {port}")
        result = record(types.SimpleNamespace(scenarios=args.scenarios, label=label, port=port, out=args.out), listener)
        if result == 2:
            return result
        config = tomllib.loads(Path(args.scenarios).read_text())
        paths.append(Path(args.out) / (config["controller"].lower().replace(" ", "_") + f"-{label}.json"))
    print("--- compare")
    return compare(types.SimpleNamespace(oracle=paths[0], port=paths[1], ignore=args.ignore))


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
    rec.set_defaults(func=record)
    both = commands.add_parser("pair", help="record the Java and the Rust extension, then compare")
    both.add_argument("scenarios")
    both.add_argument("--java-port", required=True)
    both.add_argument("--rust-port", required=True)
    both.add_argument("--out", default="target/conformance")
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
