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
#   conformance_runner.py wait [--timeout 120] [--port "VirMIDI 5-2"]...
#   conformance_runner.py record SCENARIOS.toml --label java|rust --port "VirMIDI 5-2" [--out DIR]
#   conformance_runner.py compare ORACLE.json PORT.json
#   conformance_runner.py pair SCENARIOS.toml... --java-port "VirMIDI 5-2" --rust-port "VirMIDI 5-3"
#   conformance_runner.py flexi-mapping SCENARIOS.toml OUT.properties
#
# "record" writes DIR/<controller>-<label>.json and checks the expectations of the scenarios;
# "compare" compares the run of the port with the run of the oracle.
#
# Generic Flexi: a scenario file with a [mapping] table is a Flexi mapping (the slots and
# options of tests/conformance/generic_flexi/*.toml). Before the restart of the controller the
# runner writes it as a Flexi mapping file to the path of --flexi-mapping (or DBM_FLEXI_MAPPING,
# default: flexi-mapping.properties next to the scenario file), the file which the "Filename"
# setting of both Flexi controllers in Bitwig names; both versions load it 2 s after their
# startup. "flexi-mapping" writes the file of a scenario file by hand.

import argparse
import fnmatch
import json
import os
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
    """The virtual MIDI port in place of the device; records what the extension sends. Each port is
    opened once per run and reused (MidiPort (name, stubs) returns the open one): closing an ALSA
    input which receives a lot (e.g. the VU meters of the Maschine JAM) can hang in rtmidi."""

    _open = {}

    def __new__(cls, name, stubs):
        port = cls._open.get(name)
        if port is None:
            port = super().__new__(cls)
            port._init(name)
            cls._open[name] = port
        with port.lock:
            port.stubs = stubs
            port.received = []
        return port

    def __init__(self, name, stubs):
        # Everything is done in __new__ / _init
        pass

    def _init(self, name):
        import rtmidi

        self.midi_in = rtmidi.MidiIn()
        self.midi_in.ignore_types(sysex=False, timing=True, active_sense=True)
        self.midi_out = rtmidi.MidiOut()
        self.stubs = []
        self.received = []
        self.lock = threading.Lock()
        self.midi_in.open_port(find_port(self.midi_in.get_ports(), name))
        self.midi_out.open_port(find_port(self.midi_out.get_ports(), name))
        self.midi_in.set_callback(self._on_message)

    def _on_message(self, event, _data=None):
        message, _ = event
        with self.lock:
            self.received.append(message)
            stubs = self.stubs
        # The device stub answers like the hardware
        for stub in stubs:
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
        # The port stays open for the next use (see the class); only stop the stubs and the
        # collection
        with self.lock:
            self.stubs = []
            self.received = []


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


def led_state(messages, leds, sysex_key_bytes=0):
    """The last value per controller output: status and data 1 for short messages; for system
    exclusive messages the first sysex_key_bytes bytes (the rest is the value, e.g. the colors of
    the Maschine JAM strips), or all but the last data byte if 0."""
    for message in messages:
        if message and message[0] == 0xF0 and 0 < sysex_key_bytes < len(message):
            leds[to_hex(message[:sysex_key_bytes])] = to_hex(message[sysex_key_bytes:-1])
        elif message and message[0] == 0xF0:
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
    runs with DBM_OSC_CONFORMANCE) and wait for it. The restart cannot be seen reliably in the
    output (e.g. the Maschine JAM sends its VU meters all the time, the old and the new instance
    without a gap); all controllers restarted within about 1.2 s, so this waits 3 s and then
    until the controller has sent something. Returns what the controller sent (its startup
    output, all LEDs), or None if it did not send anything within the timeout (seconds)."""
    startup = MidiPort(port_name, [])
    started = time.time()
    startup.send(RESTART)
    print(f"Restarting the controller on {port_name}, waiting up to {timeout} s")
    time.sleep(3.0)
    output = startup.take()
    while not output and time.time() - started < timeout:
        time.sleep(0.1)
        output = startup.take()
    # The rest of the startup output
    time.sleep(1.0)
    output += startup.take()
    startup.close()
    if not output:
        print(f"The controller did not send anything within {timeout} s after the restart")
        return None
    return output


# ------------------------------------------------------------------------------------------------
# Generic Flexi mappings

# The slot types (CommandSlot.TYPE_*) and the knob modes (KnobMode, in the order of the enum)
FLEXI_TYPES = {"off": -1, "cc": 0, "note": 1, "pc": 2, "pitchbend": 3, "mmc": 4}
FLEXI_KNOB_MODES = ["absolute", "relative_twos_complement", "relative_signed_bit", "relative_signed_bit_2", "relative_offset_binary", "absolute_toggle"]
FLEXI_NUM_SLOTS = 300
# The default mapping file, next to the scenario files; the "Filename" setting of both Flexi
# controllers in Bitwig names it
FLEXI_MAPPING_FILE = "flexi-mapping.properties"
# Added to every mapping as the last slot: Flexi sends nothing without a slot which sends its
# value to the device, and restart_controller waits until the controller has sent something. Its
# value is sent right after the mapping is applied (the value cache starts at -1), so the
# startup output also tells that the mapping is loaded. CC 119 on channel 16 is not used by the
# scenarios.
FLEXI_STARTUP_SLOT = {"type": "cc", "number": 119, "channel": 16, "command": "Master: Set Volume", "send_value": True}


def properties_escape(text, is_key=False):
    """Escape a key or value as java.util.Properties.store does (without the unicode escapes,
    the mappings are ASCII)."""
    out = []
    for i, char in enumerate(text):
        if char == "\\":
            out.append("\\\\")
        elif char == " " and (is_key or i == 0):
            out.append("\\ ")
        elif char in "=:#!":
            out.append("\\" + char)
        elif char == "\t":
            out.append("\\t")
        else:
            out.append(char)
    return "".join(out)


def flexi_mapping_properties(mapping):
    """The Flexi mapping file (GenericFlexiConfiguration.exportTo) for the [mapping] table of a
    scenario file: "slots" (one inline table per slot: type, number, channel 1-16 or "all",
    command, knob_mode, layer 1-10 or "all", high_res, send_value, send_value_when_received),
    "actions" (the Bitwig action IDs of Action 1-8) and "keyboard" (channel "Off"/"1"-"16"/"All",
    route_timbre, route_modulation, route_expression, route_sustain, route_program_change,
    route_pitchbend: true/false, mpe, mpe_pitchbend_range)."""
    slots = list(mapping.get("slots", []))
    if len(slots) >= FLEXI_NUM_SLOTS:
        raise ValueError(f"at most {FLEXI_NUM_SLOTS - 1} slots (the last one is the startup slot)")
    lines = ["#Generic Flexi"]

    def put(key, value):
        lines.append(f"{properties_escape(key, True)}={properties_escape(str(value))}")

    for index, slot in enumerate(slots + [None] * (FLEXI_NUM_SLOTS - 1 - len(slots)) + [FLEXI_STARTUP_SLOT]):
        if slot is None:
            continue
        unknown = set(slot) - {"type", "number", "channel", "command", "knob_mode", "layer", "high_res", "send_value", "send_value_when_received", "note"}
        if unknown:
            raise ValueError(f"unknown keys {sorted(unknown)} in slot {slot}")
        channel = slot.get("channel", 1)
        layer = slot.get("layer", 1)
        name = f"SLOT{index}_"
        put(name + "TYPE", FLEXI_TYPES[slot["type"]])
        put(name + "NUMBER", slot.get("number", 0))
        # 0-15, 16 is "All"
        put(name + "MIDI_CHANNEL", 16 if channel == "all" else channel - 1)
        put(name + "RESOLUTION", str(slot.get("high_res", False)).lower())
        put(name + "KNOB_MODE", FLEXI_KNOB_MODES.index(slot.get("knob_mode", "absolute")))
        # 0-9, -1 is "All"
        put(name + "FUNCTION_LAYER", -1 if layer == "all" else layer - 1)
        put(name + "COMMAND", slot["command"])
        put(name + "SEND_VALUE", str(slot.get("send_value", False)).lower())
        put(name + "SEND_VALUE_WHEN_RECEIVED", str(slot.get("send_value_when_received", False)).lower())
    actions = mapping.get("actions", [])
    for i in range(8):
        put(f"ASSIGNABLE_FUNCTION_ACTIONS{i}", actions[i] if i < len(actions) else "")
    # The keyboard options are always written: the values of the previous mapping must not stay
    keyboard = mapping.get("keyboard", {})
    on_off = lambda key, default: "On" if keyboard.get(key, default) else "Off"
    put("KEYBOARD_INPUT_NAME", "Generic Flexi")
    put("KEYBOARD_CHANNEL", keyboard.get("channel", "Off"))
    put("KEYBOARD_ROUTE_TIMBRE", on_off("route_timbre", False))
    put("KEYBOARD_ROUTE_MODULATION", on_off("route_modulation", False))
    put("KEYBOARD_ROUTE_EXPRESSION", on_off("route_expression", False))
    put("KEYBOARD_ROUTE_SUSTAIN", on_off("route_sustain", False))
    put("KEYBOARD_ROUTE_PROGRAMCHANGE", on_off("route_program_change", False))
    put("KEYBOARD_ROUTE_PITCHBEND", on_off("route_pitchbend", False))
    put("ENABLE_MPE", on_off("mpe", False))
    put("MPE_PITCHBEND_RANGE", keyboard.get("mpe_pitchbend_range", 48))
    return "\n".join(lines) + "\n"


def write_flexi_mapping(config, args):
    """Write the mapping of a scenario file for the next start of the Flexi controller. Returns
    the number of restarts it needs: the keyboard options only take effect on the start after
    the one which loaded them (GenericFlexiConfiguration reads them during init)."""
    if "mapping" not in config:
        return 1
    # Default: next to the scenario file (tests/conformance/generic_flexi/flexi-mapping.properties,
    # not under version control)
    path = getattr(args, "flexi_mapping", None) or os.environ.get("DBM_FLEXI_MAPPING") or str(Path(args.scenarios).resolve().parent / FLEXI_MAPPING_FILE)
    if not getattr(args, "restart", 0):
        sys.exit("The scenario file has a Flexi mapping, which is loaded at the start of the controller: use --restart")
    Path(path).expanduser().write_text(flexi_mapping_properties(config["mapping"]))
    print(f"Flexi mapping written to {path}")
    return 2 if "keyboard" in config["mapping"] else 1


def write_flexi_mapping_file(args):
    config = tomllib.loads(Path(args.scenarios).read_text())
    if "mapping" not in config:
        sys.exit(f"{args.scenarios} has no [mapping]")
    Path(args.out).write_text(flexi_mapping_properties(config["mapping"]))
    print(f"Written {args.out}")
    return 0


def stop_transport(listener):
    """Stop the transport until Bitwig reports it stopped (/stop of the OSC controller stops a
    playing transport, otherwise it stops and rewinds)."""
    listener.request_dump()
    time.sleep(0.5)
    for _ in range(10):
        if listener.snapshot().get("osc:/play") == "0":
            return
        send_osc("/stop")
        time.sleep(1.0)


# A recording which takes longer is hanging: print the stacks and end the run
RECORD_TIMEOUT = 600


def record(args, listener=None):
    import faulthandler

    faulthandler.dump_traceback_later(RECORD_TIMEOUT, exit=True)
    config = tomllib.loads(Path(args.scenarios).read_text())
    settle = config.get("settle_ms", DEFAULT_SETTLE_MS) / 1000.0
    sysex_key_bytes = config.get("sysex_key_bytes", 0)
    own_listener = listener is None
    if own_listener:
        listener = StateListener()
        listener.start()
    restart = getattr(args, "restart", 0)
    # A playing transport changes the startup output (e.g. the strips of the Maschine JAM show
    # the volume as dual values while playing)
    stop_transport(listener)
    # A Generic Flexi mapping is loaded at the start of the controller
    restarts = write_flexi_mapping(config, args)
    startup = []
    for _ in range(restarts if restart else 0):
        startup = restart_controller(args.port, restart)
        if startup is None:
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
            # "New" keeps changed projects open: close them now and then, many open projects
            # let Bitwig run out of memory ("Close" asks for a confirmation)
            send_osc("/conformance/action New")
            time.sleep(args.fresh_project)
            # A new project does not always get the audio engine; without it Bitwig does not
            # monitor, the pads of a play mode play nothing
            send_osc("/project/engine 1")
            # The transport of the old project keeps playing, and Bitwig resumes it when the
            # engine runs again (after the run of the other version): wait until Bitwig reports the
            # engine (it takes some seconds, the state of /play is meaningless before), stop
            # until Bitwig reports it stopped, then rewind with one more /stop (TransportModule of
            # the OSC controller: stop if playing, otherwise stop and rewind; /play 0 would start
            # the playback again)
            deadline = time.monotonic() + 20.0
            while listener.snapshot().get("osc:/project/engine") != "1" and time.monotonic() < deadline:
                time.sleep(0.2)
            # Bitwig resumes the playback a moment after the engine is up
            time.sleep(1.0)
            stop_transport(listener)
            send_osc("/stop")
            time.sleep(1.0)
        marker(1)
        time.sleep(1.0)
        # With keep_startup_output the LED state starts with the startup output of the restarted
        # controller (all LEDs): the full redraw of the setup is not complete in the Java version
        # on Bitwig (HwLightImpl.forceFlush turns a light off and restores it 100 ms later, Bitwig
        # sends only changes), so it would miss the LEDs which are off (Maschine JAM). Otherwise
        # the output before the setup is dropped: it also holds messages which are no LED state
        # (e.g. the LEDs of another template)
        startup += port.take()
        if config.get("keep_startup_output", False):
            led_state(startup, leds, sysex_key_bytes)
        # Bring the controller into a known state, e.g. a full redraw of the LEDs
        for step in config.get("setup", []):
            run_step(port, step, settle)
        setup_output = port.take()
        led_state(setup_output, leds, sysex_key_bytes)
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
        # The raw output before the scenarios, to find where an unexpected LED value comes from
        trace["startup_output"] = [to_hex(message) for message in startup]
        trace["setup_output"] = [to_hex(message) for message in setup_output]
        failures = 0
        for scenario in config["scenario"]:
            listener.request_dump()
            time.sleep(settle)
            led_state(port.take(), leds, sysex_key_bytes)
            state = listener.snapshot()
            trace["version"] = state.get("probe:/probe/version", "?")
            entry = {"id": scenario["id"], "bullet": scenario.get("bullet", ""), "initial": {"bitwig": state, "leds": dict(leds)}, "steps": []}
            for step in scenario.get("before", []):
                run_step(port, step, settle)
            led_state(port.take(), leds, sysex_key_bytes)
            state = listener.snapshot()
            # The state the steps start from: the comparison uses absolute values
            entry["start"] = state
            for step in scenario.get("steps", []):
                run_step(port, step, settle)
                led_state(port.take(), leds, sysex_key_bytes)
                new_state = listener.snapshot()
                entry["steps"].append({"input": step, "bitwig": diff(state, new_state), "leds": dict(leds)})
                state = new_state
            entry["final"] = {"bitwig": state, "leds": dict(leds)}
            # expect_labels: the versions the expectations apply to (default: all)
            problems = check_expectations(scenario, entry["final"]) if args.label in scenario.get("expect_labels", [args.label]) else []
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
    # The first line tells the replay test how system exclusive messages are keyed
    (out / f"{name}.leds").write_text(f"#sysex_key_bytes\t{sysex_key_bytes}\n" + "".join(f"{k}\t{v}\n" for k, v in sorted(leds.items())))
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
            if restart_controller(port, args.restart) is None:
                return 2
    for scenarios in args.scenarios:
        paths = []
        config = tomllib.loads(Path(scenarios).read_text())
        for label, port in (("java", args.java_port), ("rust", args.rust_port)):
            print(f"--- {Path(scenarios).stem}: {label} on {port}")
            result = record(types.SimpleNamespace(scenarios=scenarios, label=label, port=port, out=args.out, fresh_project=args.fresh_project, restart=args.restart, flexi_mapping=args.flexi_mapping), listener)
            if result == 2:
                # Not ready (fixture, controller): the other scenario files may still run
                print(f"--- {Path(scenarios).stem}: SKIPPED, {label} not ready")
                break
            paths.append(Path(args.out) / (run_name(config, scenarios) + f"-{label}.json"))
        if len(paths) < 2:
            failures += 1
            continue
        print(f"--- {Path(scenarios).stem}: compare")
        failures += compare(types.SimpleNamespace(oracle=paths[0], port=paths[1], ignore=args.ignore))
    return 1 if failures else 0


def wait_ready(args):
    """Wait until Bitwig is ready: the OSC controller answers /refresh (port 8000) on port 9000 and
    the MIDI ports of the controllers exist."""
    import rtmidi

    deadline = time.monotonic() + args.timeout
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.bind(("127.0.0.1", OSC_PORT))
        sock.settimeout(1.0)
        while time.monotonic() < deadline:
            sock.sendto(build_message("/refresh", []), ("127.0.0.1", 8000))
            try:
                sock.recvfrom(65536)
            except TimeoutError:
                continue
            ports = rtmidi.MidiOut().get_ports()
            missing = [name for name in args.port if not any(name in port for port in ports)]
            if not missing:
                print("Bitwig is ready")
                return 0
            time.sleep(1.0)
    print(f"Bitwig is not ready after {args.timeout} s", file=sys.stderr)
    return 1


def list_ports(_args):
    import rtmidi

    print("Inputs: ", rtmidi.MidiIn().get_ports())
    print("Outputs:", rtmidi.MidiOut().get_ports())
    return 0


def main():
    parser = argparse.ArgumentParser(description="Conformance scenario runner")
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("ports", help="list the MIDI ports").set_defaults(func=list_ports)
    wait = commands.add_parser("wait", help="wait until Bitwig answers on OSC and has the MIDI ports")
    wait.add_argument("--timeout", type=float, default=120)
    wait.add_argument("--port", action="append", default=[], help="a MIDI port which must exist (repeatable)")
    wait.set_defaults(func=wait_ready)
    rec = commands.add_parser("record", help="run the scenarios and write a trace")
    rec.add_argument("scenarios")
    rec.add_argument("--label", required=True, help="java or rust")
    rec.add_argument("--port", required=True, help="part of the name of the virtual MIDI port")
    rec.add_argument("--out", default="target/conformance")
    rec.add_argument("--fresh-project", type=float, default=0, metavar="SECONDS", help="start with a new project from the default template (the fixture) and wait up to SECONDS until it has sent its startup output")
    rec.add_argument("--restart", type=float, default=0, metavar="SECONDS", help="restart the controller before each recording (a system exclusive message which both extensions handle with DBM_OSC_CONFORMANCE) and wait up to SECONDS until it has sent its startup output")
    rec.add_argument("--flexi-mapping", metavar="PATH", help="Generic Flexi: the mapping file which the Filename setting of the controller names; the [mapping] of a scenario file is written to it before the restart (default: DBM_FLEXI_MAPPING, else flexi-mapping.properties next to the scenario file)")
    rec.set_defaults(func=record)
    both = commands.add_parser("pair", help="record the Java and the Rust extension, then compare")
    both.add_argument("scenarios", nargs="+")
    both.add_argument("--java-port", required=True)
    both.add_argument("--rust-port", required=True)
    both.add_argument("--out", default="target/conformance")
    both.add_argument("--fresh-project", type=float, default=0, metavar="SECONDS", help="start each recording with a new project from the default template (the fixture) and wait up to SECONDS until it has sent its startup output")
    both.add_argument("--restart", type=float, default=0, metavar="SECONDS", help="restart the controller before each recording (a system exclusive message which both extensions handle with DBM_OSC_CONFORMANCE) and wait up to SECONDS until it has sent its startup output")
    both.add_argument("--ignore", action="append", help="address pattern to ignore (repeatable)")
    both.add_argument("--flexi-mapping", metavar="PATH", help="Generic Flexi: the mapping file which the Filename setting of the controller names; the [mapping] of a scenario file is written to it before the restart (default: DBM_FLEXI_MAPPING, else flexi-mapping.properties next to the scenario file)")
    both.set_defaults(func=pair)
    flexi = commands.add_parser("flexi-mapping", help="write the Generic Flexi mapping of a scenario file")
    flexi.add_argument("scenarios")
    flexi.add_argument("out")
    flexi.set_defaults(func=write_flexi_mapping_file)
    cmp = commands.add_parser("compare", help="compare the trace of the port with the oracle")
    cmp.add_argument("oracle")
    cmp.add_argument("port")
    cmp.add_argument("--ignore", action="append", help="address pattern to ignore (repeatable)")
    cmp.set_defaults(func=compare)
    args = parser.parse_args()
    code = args.func(args)
    # The MIDI ports stay open (see MidiPort): rtmidi can crash when the interpreter shuts down
    # with open ports, so end without the shutdown
    sys.stdout.flush()
    sys.stderr.flush()
    os._exit(code or 0)


if __name__ == "__main__":
    main()
