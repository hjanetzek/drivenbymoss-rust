# Conformance testing against the Java DrivenByMoss: plan

Status: 2026-10-30, steps 1 and 2 done, step 3 in progress.

- Step 1: the runner (`backends/bitwig/tools/conformance_runner.py`), the OSC conformance module
  (branch `conformance-osc` of the Java repository), the probe (`ProbeExtension`, now only for
  the Bitwig version) and the first scenarios. The first pair runs found the master track twice
  (fixed) and a Java bug (Note Sequencer Track Control row after a template change, a known
  difference); after that Java and Rust were the same in all scenarios of the Note Sequencer.
- Step 2: the time of the core from the host (`Host::now`), the recorder in the extension and
  the replay test (§5); the first replay is `tests/conformance/launchcontrol_xl/replay/`.
- Step 3: the fixture project (`tests/conformance/FIXTURE.md`, also the default template for
  `--fresh-project`) and scenarios for all five templates of the Launch Control XL
  (`tests/conformance/launchcontrol_xl/*.toml`, 27 scenarios). The first full pair run found
  several Rust bugs; the state per controller is in `tests/conformance/<controller>/CONFORMANCE.md`
  (coverage, open and known differences, fixes). Next: fix the Launch Control XL differences,
  record the replays; then the JAM, the Mk2/Mk3 and the Launchpad.

Setup for a run (Linux):

1. Free virtual MIDI ports for the controllers under test: `snd-virmidi` with more devices than
   `dbm-nihost` uses (it uses the first two). With `midi_devs=4`
   (`sudo modprobe -r snd_virmidi && sudo modprobe snd_virmidi midi_devs=4`, then restart
   `dbm-nihost --virmidi`) VirMIDI 5-2 and 5-3 are free for the Java and the Rust controller.
   Bitwig does not reopen a port which was busy when a controller was activated: activate the
   controller again after the port is free.
2. Bitwig started with `DBM_OSC_CONFORMANCE=1` and the Java DrivenByMoss of the branch
   `conformance-osc`. In Bitwig: the Java "Open Sound Control" controller sending to
   127.0.0.1:9000 (receiving on 8000; its MIDI port setting can be any unused device, it does
   not use it), optionally the "Conformance Probe (Rust)" controller (Bitwig version), and the
   controllers under test, the Java and the Rust one, each with its own VirMIDI port as input
   and output. Restart both before a run, so that they start from the same state (§4).
3. The fixture project (`tests/conformance/FIXTURE.md`) is open, or better: it is saved as a
   template ("Save as Template…") and set as the default template for new projects. Then
   `--fresh-project <seconds>` starts each recording with a new project from it (Bitwig action
   `New` via `/conformance/action`, no dialog), a clean copy for every run. The used copies stay
   open (Bitwig switches their audio engine off); close them now and then, each asks once
   whether to save.
4. `cd backends/bitwig/tools && ./conformance_runner.py pair
   ../../../tests/conformance/launchcontrol_xl/*.toml --java-port "VirMIDI 5-2" --rust-port
   "VirMIDI 5-3"` records the Java and the Rust controller for each scenario file and compares
   them (traces, LED states and recordings in `target/conformance`). Single runs:
   `record <scenarios> --label java|rust --port <port>`, then `compare <java trace> <rust trace>`.
   A scenario file can have `setup` steps (once before the scenarios, e.g. a template change, the
   first scale and `osc` commands which select the fixture track), `before` steps per scenario
   (not compared, e.g. resetting a mute via `osc`), a `require` check of the fixture (after the
   setup), and `[[known_difference]]` entries (`match` = pattern of `<bitwig|leds> <key>`,
   `reason`) which are reported but do not fail. Nothing else may listen on UDP 9000 (e.g.
   `osc_monitor.py`).

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
- `/conformance/primary/layer/{1-8}/…`, `/conformance/primary/drumpad/{1-16}/…` (`exists`,
  `name`, `selected`, `mute`, `solo`, `volume`, `pan`, and `…/offset`): the layers and drum
  pads of the primary device (OSC has them only for the cursor device).
- Further values and commands of the other agent (actions, device slots): see
  `CONFORMANCE-OSC.md` in the Java repository.
- `/conformance/device/slots`: the slots of the cursor device (e.g. `FX|Note`), `|`-separated.
- `/conformance/action {id}` (command): invokes a Bitwig action; it acts on the GUI focus.
- `/conformance/actions {text}` (command): reports `/conformance/actions`, the matching actions as
  `identifier|name` lines.

For building the fixture, `DeviceModule` also has `/device/layer/{1-8}/browse`,
`/device/drumpad/{1-16}/browse` and `…/addDevice {name}`. They fill existing layers; nothing
creates a layer over OSC, so the Instrument Layer of the fixture comes from a preset.

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

Host reactions to commands are part of the recorded model lines, so a replay is only valid as
long as the core sends the same commands; when it does not, the test fails with the first
differing command, which is the point where the scenario has to be recorded again in Bitwig.

**Implemented (step 2):**

- **Time:** the core only runs when the host calls it. `App` sets the time of each call
  (`Host::now`, used by the scheduled tasks, the delayed startup, blinking, long presses); a
  replay sets the recorded time with `App::set_time`.
- **Recorder** (`ProtocolRecorder` in the Bitwig extension): records from the start of the
  controller, in dev mode into `target/conformance/recordings` (or `DBM_RECORD_DIR`), one event
  per line `<ms>\t<kind>\t<fields>`: the controller ID, its configuration files and document
  settings, model lines, ticks, MIDI in and out, commands and note input tables. The runner marks
  a Rust run with a system exclusive message (`F0 7D 44 42 4D 01|02 <name> F7`, not passed to the
  core); the end marker saves a copy as `<controller>-rust.rec`.
- **Replay test** (`tests/conformance_replay.rs`): replays `tests/conformance/*/replay/*.rec` (or
  `DBM_REPLAY=<file>`) in a fresh core: after each input the same outputs as recorded; at the end
  marker the LED state of `<controller>-java.leds` (written by the runner with
  `<controller>-java.known`, the known differences).

To record a replay: restart the Rust controller (the recording starts with it, from a fresh
state), run `pair`, then copy `target/conformance/recordings/<run>-rust.rec` and
`target/conformance/<run>-java.leds|known` to `tests/conformance/<controller>/replay/`; `<run>`
is the controller and the scenario file, e.g. `novation_launch_control_xl-note_sequencer`.

**Fixture (step 3):** one Bitwig project for all controllers, described in
`tests/conformance/FIXTURE.md`. Scenario files select their track and clip and reset the states
they change with `osc` steps (sent to the OSC controller, port 8000) in `setup` (once) and
`before` (per scenario, not compared); `require` is checked after `setup`. `pair` takes several
scenario files.

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
   end to end with both extensions. Done.
2. **Protocol recorder** in the Rust extension and the replay test harness. Done.
3. **Fixtures** for the four ported controllers; scenarios for the Launch Control XL (small,
   five templates), then the JAM, the Mk2/Mk3 and the Launchpad. In progress: the fixture and
   the Launch Control XL scenarios are written, not yet run. The Maschine controllers need the
   MIDI which `dbm-nihost` sends for the hardware (the runner takes its place on a VirMIDI port)
   and a stub for their startup messages.
4. Add the scenario step to PORTING_GUIDE.md §3 and §6 for all further ports.

## 9. Open questions

1. **Resetting Bitwig between scenarios:** solved per run with a new project from the fixture
   template (`--fresh-project`); within a run the scenario files reset what they change via OSC
   (`setup`, `before`). No action reopens or reverts a project file ("Open…" and "Save as…"
   show dialogs, "Close" asks for a modified project), so the used copies stay open.
2. **Switching between the Java and the Rust extension:** solved: both are active at the same
   time, each on its own virtual port; the runner drives one at a time (`pair`).
3. **Bitwig versions:** traces recorded with Bitwig 5.3 (the current test setup) and 6.1 (the
   baseline) may differ; the trace records the version.
4. **Timing-dependent features** (long press, double click, blinking, note repeat): the replay
   runs on the recorded times (`App::set_time`); the Bitwig runs still need tolerances or
   `wait_ms` in the scenarios.
5. **Coverage of the observer:** the probe must not grow into a second copy of the model. It
   reports what the scenarios ask for, added scenario by scenario.
