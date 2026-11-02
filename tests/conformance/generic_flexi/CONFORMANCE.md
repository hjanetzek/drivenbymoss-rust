# Conformance: Generic Flexi

Status: 2026-11-02, scenarios written, not yet run on Bitwig (no traces, no replays).

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
5. Run:

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
| `track.toml` | Track functions: selection, bank paging, the effect track bank, mute/solo/arm/monitor, volume/pan/send, Track Selected, pin, activation | 15 |
| `device.toml` | Device, Instrument Device, Track/Project Remotes, EQ, Layer functions | 15 |
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

None yet.
