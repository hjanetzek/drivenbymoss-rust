# Conformance testing against the Java DrivenByMoss: plan

Status: draft, 2026-10-29. Nothing of this is implemented yet.

## 1. Goal

A ported controller must do what the user manual says, and do it the way the Java DrivenByMoss
does on Bitwig. Today this is checked by hand on the hardware, which finds differences one at a
time and late (e.g. the Launch Control XL Note Sequencer selected the instrument's parameter page
only inside the core, Bitwig stayed on page 1).

The plan: **manual-driven scenarios, recorded once with the Java version as the oracle, replayed
against the Rust version**, compared on what can be observed from outside: the MIDI the
controller receives back (LEDs, displays) and the state of Bitwig. The same scenarios then become
regression tests that run with `cargo test`, without Bitwig. Every future controller port
starts with this.

Principles:

- **The manual is the specification.** Each scenario names the manual bullet it covers
  (`~/bastel/github/DrivenByMoss-Documentation/<vendor>/<controller>.md`). The bullets give the
  coverage list of a controller.
- **The Java version on Bitwig is the oracle.** Where the manual is vague, Java decides. Where
  Rust has to differ (API limits, protocol), the difference is an entry in
  `BITWIG_DIVERGENCES.md` and the scenario is marked with it.
- **Compare behaviour, not code.** Java and Rust talk to Bitwig differently (observers vs. the
  backend protocol), so commands cannot be compared. What the user sees can.

## 2. What is observed

| Observable | Java run | Rust run | Notes |
| --- | --- | --- | --- |
| MIDI to the controller (LEDs, sysex displays, NI HID through `dbm-nihost`) | the extension's MIDI out port | same | Compared as final state per LED / display cell after each step, not as a message stream |
| Bitwig state | a neutral observer (below) | same observer | The primary result of most scenarios |
| Notes played into Bitwig (note inputs) | the observer's note monitor | same | Catches "pads play notes in the Shift view" |
| Backend protocol (commands, model lines) | - | `DBM_LOG_MIDI`, a protocol log | Diagnostics only, for finding the cause of a difference |

**The neutral observer.** The Java OSC controller (`osc_monitor.py`) already shows tracks, the
cursor device and its pages, slots and the transport. It does not show the first instrument,
drum pads and layers outside of the cursor device, the browser columns or the steps of the
cursor clip. Therefore a small **probe extension** is needed: a Bitwig controller extension
without a controller that reports a broad snapshot of the project as text lines over UDP. It can
reuse `BitwigModel` / `ModelReporter` of the Rust extension (same addresses as
BACKEND_PROTOCOL.md §3), extended by what the core does not need (e.g. which remote control page
Bitwig shows for each device). It only reads Bitwig values, so a bug of the Rust core cannot
hide in it; a reporting bug would show up in both runs equally and is reviewed with the OSC
monitor.

## 3. Scenarios

One file per manual section, e.g. `rust/tests/conformance/launchcontrol_xl/note_sequencer.toml`:

```toml
controller = "Novation Launch Control XL"
manual = "Novation/Novation-LaunchControlXL.md#note-sequencer---5th-factory-template"
fixture = "fixtures/instrument_two_pages.bwproject"   # Polysynth with 2+ remote control pages

[[scenario]]
id = "lcxl.noteseq.device-page"
bullet = "Device (yellow): Keep the device button pressed to select one of the 1st 8 parameter pages of the first instrument device."
steps = [
  { midi = "sysex F0 00 20 29 02 11 77 0C F7", note = "factory template 5" },
  { midi = "note-on 0x9C 105 127", note = "hold Device" },
  { midi = "note-on 0x9C 74 127", note = "Track Control 2" },
  { midi = "note-off 0x8C 74 0" },
  { midi = "note-off 0x8C 105 0" },
]
expect.bitwig = { "/primary/page/selected" = "1" }
expect.leds = { "track-control-2" = "yellow" }
divergences = []
```

- **Steps** are MIDI messages as the device sends them, plus `wait` and `tick` steps. Long
  presses are a `wait` between down and up.
- **Expectations** are only the observables the bullet talks about. Everything else is still
  recorded and compared, but differences outside the expectation are reported as warnings, so
  that a scenario does not break on unrelated LED changes.
- **Fixtures** are Bitwig projects with known content: tracks of all types, a Drum Machine with
  named and colored pads, an Instrument Layer, a synth with several remote control pages, clips
  with notes and step expressions, scenes. They are reopened before each scenario (open
  question 1).
- **Device stub:** some controllers ask the device for its state at startup (the LC XL template
  sysex, Maschine firmware queries). The runner answers like the device, per controller.

## 4. Running the oracle and the port

```
scenario runner (Python, next to osc_monitor.py)
   │ MIDI in (virtual port in place of the device)       ▲ MIDI out (LEDs, displays)
   ▼                                                     │
Bitwig: controller extension under test (Java DrivenByMoss  OR  DrivenByMoss (Rust))
        probe extension ──UDP──> runner (Bitwig state, played notes)
```

1. **Record the oracle:** run the scenarios against the Java controller extension. The runner
   stores a trace per scenario: inputs, the observer snapshots and the controller output after
   each step (`tests/conformance/<controller>/traces/*.json`). Traces are checked in; they only
   change when the Java version or the fixture changes.
2. **Run the port:** the same scenarios against the Rust extension, compared with the traces.
   Result: a report per scenario (pass, expected divergence, fail with the differing values and
   the protocol log).
3. **Triage** each failure: a bug of the port (fix it), a Bitwig API limit or protocol gap
   (document it in `BITWIG_DIVERGENCES.md`, mark the scenario), or a bug of the Java version
   (report it to the maintainer, keep Rust like Java unless decided otherwise).

The virtual MIDI ports: ALSA sequencer ports (e.g. with `python-rtmidi`) take the place of the
device port in Bitwig's controller settings. NI controllers already go through `dbm-nihost`
`--virmidi`; the runner sends the MIDI that `dbm-nihost` would send, and reads the LED and
display output from it. Only one of the two controller extensions is active at a time, on the
same ports.

## 5. Replay in `cargo test`

A trace of a Rust run contains everything the core saw: the model lines from the host and the
MIDI in, with their timing. A replay test feeds the same into `App` + `RecordingBackend` (as
`tests/launchpad_smoke.rs` does) and checks:

- the controller output against the **Java** trace (the conformance part), and
- the backend commands against the recorded Rust run (the regression part, so that a later
  change shows up without Bitwig).

This needs a protocol recorder in the extension (environment variable, like `DBM_LOG_MIDI`): it
writes the model lines, MIDI in/out and commands with timestamps. The replay runs the core's
tick at the recorded times. Host reactions to commands are part of the recorded model lines, so
a replay is only valid as long as the core sends the same commands; when it does not, the test
fails with the first differing command, which is the point where the scenario has to be
recorded again in Bitwig.

## 6. Comparison rules

- LEDs and displays: the state after the step is compared (last value per LED / cell), after the
  core has flushed (a few ticks). Timing and the order of messages are ignored, except where a
  bullet is about blinking.
- Bitwig state: values per address; floats with a tolerance (1e-3, displayed values as text);
  play position, VU meters and other moving values are left out unless a scenario asks for them.
- Notes played into Bitwig: as a set of (note, velocity > 0) per step.
- Names that the controller shortens or converts (ASCII, display width) are compared as shown.

## 7. Process for a controller port

1. Turn the manual sections of the controller into scenario files: one scenario per bullet, the
   bullet text copied in. Bullets which cannot be tested automatically (e.g. touch-sensitive
   knobs) are listed with `manual = true`.
2. Prepare the fixtures and the device stub.
3. Record the oracle with the Java version.
4. Port the controller (PORTING_GUIDE.md), run the scenarios, triage.
5. Check in the traces and the replay tests; `cargo test` runs them from then on.

For the controllers which are already ported (Launchpad, Launch Control XL, Maschine JAM and
Mk2/Mk3), the scenarios are written section by section of their manuals, starting with the
sections where differences were found by hand.

## 8. Steps

1. **Probe extension** with the observer snapshot over UDP (reusing `BitwigModel`), and the
   runner skeleton: virtual ports, scenario file, trace writing, report. One scenario (the LC XL
   device page) end to end with both extensions.
2. **Protocol recorder** in the Rust extension and the replay test harness.
3. **Fixtures** for the four ported controllers; scenarios for the Launch Control XL (small,
   five templates), then the JAM, the Mk2/Mk3 and the Launchpad.
4. Add the scenario step to PORTING_GUIDE.md §3 and §6 for all further ports.

## 9. Open questions

1. **Resetting Bitwig between scenarios:** reopening the fixture project from the probe
   (`Application` has no "open file"; maybe an action ID, or undo back to a marker), or one
   Bitwig session per scenario file. To be tried.
2. **Switching between the Java and the Rust extension** without the GUI: two Bitwig profiles
   or settings files, or both extensions installed with only one assigned to the virtual ports.
3. **Bitwig versions:** traces recorded with Bitwig 5.3 (the current test setup) and 6.1 (the
   baseline) may differ; the trace records the version.
4. **Timing-dependent features** (long press, double click, blinking, note repeat) need a
   deterministic tick in the replay, and tolerances in the Bitwig run.
5. **Coverage of the observer:** the probe must not grow into a second copy of the model. It
   reports what the scenarios ask for, added scenario by scenario.
