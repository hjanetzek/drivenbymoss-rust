# Conformance: Generic Flexi

Status: 2026-11-02, three runs; replays of device, function_layers, keyboard, marker, project, transport.

Generic Flexi has no hardware of its own: the runner is the controller. The mapping (which MIDI
message triggers which of the 1121 functions) is part of each scenario file (`[mapping]`), so
the same scenarios drive the Java and the Rust version, and they reach parts of the framework
which the other controllers do not use on Bitwig (markers, layouts, the master, scenes, the
note input). Manual: `Generic-Tools-Protocols/Generic-Flexi.md`.

## Setup

In addition to doc/CONFORMANCE_TESTING_PLAN.md (Bitwig with `DBM_OSC_CONFORMANCE=1`, the Java
OSC controller, the fixture):

1. Add the Java controller "Generic" → "Flexi" and the Rust one "Generic" → "Flexi (Rust)", each with its own
   free VirMIDI port as input and output (e.g. VirMIDI 5-2 and 5-3).
2. In the settings of both set **Filename** (Load/Save) to the same file, the mapping file in
   this folder: `<checkout>/tests/conformance/generic_flexi/flexi-mapping.properties`. Both load
   it 2 s after their start and apply it 1 s later. The runner rewrites it for every scenario file
   (it is not under version control); it starts with the mapping of `slots.toml`.
3. The OSC controller: **Value resolution** Low (128 values, the default); the scenarios set and
   expect values in 0-127.
4. The fixture needs two cue markers for `marker.toml` (tests/conformance/FIXTURE.md).
5. Bitwig's takeover mode (a global preference) set to **Immediate**. With the default "Catch" an
   absolute value only applies once it passes the current one, so the scenarios which set a
   volume or pan to 0 (and the values the `before` steps set over OSC) change nothing. The
   profile of this setup is `generic_flexi_immediate` (`generic_flexi` is a link to it, the
   script uses the setup name as the profile name); `flexi` is the same with "Catch".
6. Run:

   ```
   cd backends/bitwig/tools
   ./conformance_runner.py pair ../../../tests/conformance/generic_flexi/*.toml \
       --java-port "VirMIDI 5-2" --rust-port "VirMIDI 5-3" --restart 15 --fresh-project 8
   ```

   `--restart` is required: the runner writes the mapping of the scenario file to the mapping
   file of step 2 (or to `--flexi-mapping <path>` / `DBM_FLEXI_MAPPING`) before it restarts the
   controller. It adds a last slot which sends the
   master volume on CC 119, channel 16 (`FLEXI_STARTUP_SLOT`): without a slot which sends its
   value Flexi sends nothing at its start, and the restart waits for the startup output. Since
   the value is sent when the mapping is applied, the startup output also means "mapping loaded".
   `keyboard.toml` sets keyboard options, which only take effect at the start after the one which
   loaded them: the runner restarts twice for a mapping with a `keyboard` table.

`flexi-mapping SCENARIOS.toml OUT.properties` writes the mapping file of a scenario file, e.g. to
load it by hand.

Without Bitwig, `cargo test --test generic_flexi_scenarios` checks the files: every function name
exists, the port loads all slots, and the MIDI of all steps runs through the port without
errors. All 121 function names of the files also exist in the Java `FlexiCommand` (checked
2026-11-02).

## Scenario files

| File | Manual section | Scenarios |
| --- | --- | --- |
| `slots.toml` | Parameters for each slot: CC, note, program change, pitchbend and MMC triggers, the six knob modes, 14 bit, MIDI channel All, send value (also when received), candidate M | 17 |
| `function_layers.toml` | Functions Layer: default layer, inactive layer, activate, temporary | 4 |
| `modes.toml` | Features of Modes: Track, Volume, Panorama, Send, Device and Browser mode, next/previous mode | 16 |
| `transport.toml` | Global: Shift Button (Play with Shift, slow knob), Transport functions | 15 |
| `track.toml` | Track functions: selection, bank paging, the effect track bank, mute/solo/arm/monitor, volume/pan/send, Track Selected, pin, activation | 16 |
| `device.toml` | Device, Instrument Device, Track/Project Remotes, EQ, Layer functions | 16 |
| `project.toml` | Global (undo/redo), Master, Scene, Clip, Layout functions | 10 |
| `marker.toml` | Marker functions | 3 |
| `keyboard.toml` | Configure the keyboard / pads: blocked mapped notes, Note Input transposition | 3 |

## What OSC cannot show (and how the scenarios deal with it)

- **Bank positions** are per extension: OSC shows its own track bank. The scenarios which page or
  scroll the bank of Flexi select a track through Flexi afterwards and check
  `/track/selected/name` (the selection is shared). The Launch Control XL scenario
  `lcxl.mix.track-page` expects `/track/1/name` after paging and is therefore satisfied even if
  the paging does nothing.
- **Pinning** (track, device, clip) is per cursor: compared only, and each scenario unpins again,
  otherwise the following scenarios act on the pinned cursor.
- **Toggles without an OSC "set"** (overdub, automation write, device bypass/expand/parameters,
  layout visibility): toggled twice, the per-step differences are compared.
- Values with no OSC address: groove (shuffle, accent), note repeat, the knob speed settings
  themselves, MIDI CC sent to the DAW (`MIDI CC: ...`), program banks.

## Not covered

- The settings UI: file dialogs, Set / Save / Load (no UI in the port; their functions are
  covered by the Rust tests).
- MPE, note repeat, the routing of modulation/sustain/pitchbend (no observable in OSC).
- Program banks (need a `.programs` file next to the mapping).
- Actions (`Action: Execute Action N`): need Bitwig action IDs which do not disturb the run.
- Add Audio/Effect/Instrument Track and Create Scene (only `project.toml` creates a scene, as its
  last scenario): they change the fixture.

## Expected differences and Java bugs

- **Candidate M** (doc/UPSTREAM_CANDIDATES.md): `flexi.slot.candidate-m-all-layers` documents the
  Java behaviour (two 7-bit "All layers" slots on CC 3: only the last gets the value), which the
  port keeps. With the upstream fix both slots change; then change the expectation.
- **Candidate A** (`setMonitor (false)` switches monitoring on): Flexi's "Set Monitor" only calls it
  for a pressed button, so it cannot show here (as the candidate says).

## Results

### First run, 2026-11-02

The setup works: all 9 files ran, every restart loaded its mapping. 83 of 98 scenarios the same,
15 DIFF, 40 failed expectations (19 Java, 21 Rust). No replays copied.

Fixed afterwards (to be checked by the next run):

- The replays loaded the mapping the runner wrote last: the recorder now records the mapping file
  (a `file` event) and the replay test uses it.
- Scenarios: absolute toggle (a toggle button sends one message per press), monitor and auto
  monitor separated (one setting in Bitwig), the effect track bank scenario ends after the toggle.
- Port: the MPE configuration (RPN) was sent at every start, also without the keyboard note
  input (Java: only with it). It made the runner's restart see the start before the mapping was
  loaded, so keyboard.toml restarted too early and the mapped note was not blocked.
- Bitwig extension: the cursor device did not report `hasLayers` / `hasDrumPads`, so the layer
  commands did nothing.
- Core: no cursor clip without a sequencer (Java `ensureClip` in every `init`): the selected clip
  slot did not follow the selected track in Bitwig.

Open:

- **Absolute values had no effect in both versions** (track volume and pan to 0 from Flexi, also
  the `before` values set with OSC), relative changes worked. Probably Bitwig's takeover mode
  (a global preference) was "Catch" (the default) in the profile; the second run uses a profile
  with "Immediate". Device parameters: Rust set them, Java did not (to be understood).
- **Effect track bank:** the core has none (the model was written for Reaper), so "Track: Toggle
  Trackbank" and the FX Track functions do nothing in the port; Java on Bitwig switches to the
  effect tracks. `flexi.track.trackbank` fails in Rust until the core gets an effect track bank.
- The startup output (not compared) also differs in the RPN messages; after the fix both send them
  only with the keyboard.

### Second run, 2026-11-02 (takeover "Immediate")

88 of 99 scenarios the same, 11 DIFF, 6 failed expectations; replays copied for function_layers,
keyboard, marker, project, transport (later removed again: the note input change below changes
their outputs, the next run records them again). The absolute values now apply in both, the MPE
messages and the note blocking of keyboard.toml are the same.

Fixed afterwards (to be checked by the next run):

- `flexi.device.instrument` checked parameter 1 after the page change: split into two scenarios.
- Bitwig extension: the cursor device reported `hasLayers` but not its layers; it now reports its
  layer bank (`/device/layer/<i>/…`, numDeviceLayers) and handles `device layer/<i>/…`.
- Core: without a keyboard channel (and MPE) no note input, as the Java version on Bitwig (it
  passed all notes with the default filters, `flexi.slot.note`). The Java Reaper version has the
  default note input there; the port now follows Bitwig and the manual ("can be turned off").
- Core: the 0x0 cursor clip of `ensure_clip` was left out of the model setup, so the Bitwig
  extension created no launcher cursor clip (the clip selection still did not follow the track).
- "Device: Parameters" toggles the remote controls section (`parameters/toggle`, reported as
  `/device/parameters`); it did nothing in the port.

Still open: the effect track bank (`flexi.track.trackbank`).

### Third run, 2026-11-02

94 of 100 scenarios the same, 6 DIFF, 1 failed expectation (`flexi.track.trackbank`, Rust). The
layer, note input and parameter fixes work. Replays: keyboard and marker copied by the script;
device, function_layers, project and transport did not replay only because the replay test wrote
commands and lists without a value with a trailing space ("filters 0 ", "noArg refresh "); fixed
in the replay test, the four recordings copied by hand from this clean run.

Remaining differences:

- `flexi.track.trackbank`: no effect track bank in the core.
Fixed afterwards (to be checked by the next run): the core has an effect track bank (reported
by the Bitwig extension as `/fxtrack/…`, the FX Track functions and "Toggle Trackbank" use it),
and the Bitwig extension has the clip launcher navigator of the Java Bitwig version (host
feature `clipLauncherNavigator`).

Before these fixes:

- The selected clip slot (`/track/N/clip/1/isSelected`) in `modes.track-next-previous-item`,
  `modes.track-previous-page`, `slot.send-value-when-received`, `track.bank-page-previous`,
  `track.scroll-back`. The port's clip launcher navigator is the one of the Java Reaper version
  (`ClipLauncherNavigatorImpl`: it moves the selected slot to the selected track itself); the Java
  version on Bitwig only selects the track through a single track bank which follows the cursor
  track, and Bitwig moves the slot. A host navigator for Bitwig would be needed.
