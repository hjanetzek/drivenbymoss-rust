# Conformance testing against the Java DrivenByMoss: plan

Status: 2026-10-30, step 1 done: the runner (`backends/bitwig/tools/conformance_runner.py`), the
OSC conformance module (branch `conformance-osc` of the Java repository), the probe
(`ProbeExtension`) and the first scenarios (`tests/conformance/launchcontrol_xl/note_sequencer.toml`).
The first pair run found the master track twice (fixed) and a Java bug (Note Sequencer Track
Control row after a template change, a known difference); after that Java and Rust were the same
in all five scenarios.

Setup for a run (Linux):

1. A free virtual MIDI port for the controller under test: `snd-virmidi` with one device more
   than `dbm-nihost` uses (`sudo modprobe -r snd_virmidi && sudo modprobe snd_virmidi
   midi_devs=3`, then restart `dbm-nihost --virmidi`; it uses the first two).
2. Bitwig started with `DBM_OSC_CONFORMANCE=1` and the Java DrivenByMoss of the branch
   `conformance-osc`. In Bitwig: the Java "Open Sound Control" controller sending to
   127.0.0.1:9000 (receiving on 8000; its MIDI port setting can be any unused device, it does
   not use it), optionally the "Conformance Probe (Rust)" controller (Bitwig version), and the
   controller under test (Java or Rust, only one active) with the free VirMIDI port as input and
   output.
3. The fixture of the scenario file (described at its top).
4. `cd backends/bitwig/tools && ./conformance_runner.py record
   ../../../tests/conformance/launchcontrol_xl/note_sequencer.toml --label java --port "VirMIDI 5-2"`,
   the same with `--label rust` after switching the controller, then
   `./conformance_runner.py compare <java trace> <rust trace>` (traces in `target/conformance`).
   With both controllers active, each on its own port: `./conformance_runner.py pair
   <scenarios> --java-port "VirMIDI 5-2" --rust-port "VirMIDI 5-1"` records both and compares.
   A scenario file can have `setup` steps (run once before the scenarios, e.g. a template change
   and the first scale), a `require` check of the fixture, and `[[known_difference]]` entries
   (`match` = pattern of `<bitwig|leds> <key>`, `reason`) which are reported but do not fail.
   Nothing else may listen on UDP 9000 (e.g. `osc_monitor.py`).

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

**The neutral observer** is the Java OSC controller of DrivenByMoss (protocol:
`DrivenByMoss-Documentation/Generic-Tools-Protocols/Open-Sound-Control-(OSC).md`), read by the
runner on UDP 9000 as `osc_monitor.py` does. It is written by the original author and shares no
code with the port: tracks, sends, slots and scenes, the transport, the cursor device with its
parameter pages, layers and drum pads, the primary device (`/primary/...`, the same attributes),
the EQ and the browser (filter columns, results, tab). It does not report the steps of the cursor
clip (only `/clip/exists`, `color`, `pinned`, `name`) and the notes played into a track.

For these gaps the extension of the Rust port contains a small **probe**
(`ProbeExtension`, "Conformance Probe (Rust)", no MIDI ports): it reports the steps of a fixed
grid of the cursor clip (64 steps of 1/16 from the clip start, all keys,
`/clip/grid/64x128/step/<ch>/<x>/<y>` in the step format of BACKEND_PROTOCOL.md), the notes
playing on the cursor track (`/probe/playingNotes`) and the Bitwig version (`/probe/version`) as
text lines on UDP 9100; a datagram `dump` to UDP 9101 requests all values. It reuses
`BitwigModel` of the extension, so it can share a reporting bug with the port; it is therefore
only used where OSC has no value, and checked against the Bitwig GUI when it is extended.

**The OSC implementation may be changed for the tests.** It is a development tool here, not a
product: a local build of the Java DrivenByMoss
(`~/bastel/github/DrivenByMoss`, `de.mossgrabers.controller.osc`, one class per area in
`module/`, e.g. `ClipModule`) can report what the scenarios need, e.g. the steps of the cursor
clip, the notes played into a track, the selected remote control page of the first instrument
outside of the cursor device, or values with more precision. Then the observer stays independent
of the Rust code and the probe can shrink or go away. Such changes are kept on a branch of the
Java repository and listed here; they must only add values, never change what the controller
under test does.

Branch `conformance-osc` of `~/bastel/github/DrivenByMoss` (from `linux-maschine`):
`ConformanceModule`, enabled with the environment variable `DBM_OSC_CONFORMANCE` (the clip grid
must be created during init):

- `/clip/step/{channel}/{step}/{note}`: the steps of a fixed grid of the cursor clip (64 steps of
  1/16 from the clip start, all notes, 0-based) in the step format of the core, `0` when a step
  became empty.
- `/track/{1-8}/playingNotes`: the notes playing on the track, `note:velocity` separated by
  spaces.

With it, the probe of the Rust extension is only needed for the Bitwig version.

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
  { midi = "F0 00 20 29 02 11 77 0C F7", note = "factory template 5" },
  { midi = "9C 69 7F", note = "hold Device" },
  { midi = "9C 4A 7F", note = "Track Control 2" },
  { midi = "8C 4A 00" },
  { midi = "8C 69 00" },
]
expect.bitwig = { "/primary/page/2/selected" = "1" }   # OSC addresses are 1-based
expect.leds = { "9C 4A" = "3E" }                         # last value per status and data 1
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
        Java OSC controller ──UDP 9000──> runner (Bitwig state)
        probe extension ─────UDP 9100──> runner (clip steps, played notes)
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

**Same starting state.** Both extensions keep state between scenarios and runs: modes they
remember (e.g. the track button mode per main mode of the Launch Control XL), the scale and other
settings (Java: Bitwig's preferences, Rust: `~/.config/DrivenByMoss-Bitwig`). Before a pair run
both controllers are restarted (deactivated and activated in Bitwig) and their settings are the
same; the `setup` steps of a scenario file then bring them into the state the scenarios start
from (e.g. a template change for a full LED redraw). The first pair run (2026-10-30, Launch
Control XL Note Sequencer) had identical Bitwig states in all steps, but LED differences which
came from a Java extension with state from earlier manual tests.

Running both extensions at the same time is fine, each on its own virtual port; the runner only
drives one at a time (`pair`). Bitwig does not reopen a port which was busy when a controller
was activated: activate the controller again after the port is free.

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

1. **Runner skeleton** (OSC observer, probe for the gaps, virtual MIDI port, device stub,
   scenario file, trace writing, comparison report) and one scenario (the LC XL Note Sequencer)
   end to end with both extensions.
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
