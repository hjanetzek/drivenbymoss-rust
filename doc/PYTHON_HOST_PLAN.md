# Python bindings and a reference host: plan

Status: 2026-11-02, steps 1–7 implemented: the binding, the reference host and the in-process
conformance runner (`python3 -m conformance`). All 21 scenario files of the four setups (195
scenarios) compare the same as Java on Bitwig (the list in `ci/linux.sh`). The model has the
devices with their remote controls, layers and drum pads, the clips with their step grids, the
note inputs and the playing notes, the markers, the undo history, the panel layout and the
arranger and mixer flags. Not simulated yet: the browser, groove, the project and track remote
controls, the crossfader, the sends of layers; the state view still leaves keys of the Java
traces out ("not modelled", e.g. `/device/drumpad`, `/vkb_midi`).

**Goal.** A Python binding of the C ABI, and on top of it a reference host for DAWs that want to
include DrivenByMoss. The reference host is a bridge with a simulated DAW, written in plain
Python. It has to pass the conformance scenarios of all four setups with the same results as the
Java DrivenByMoss on Bitwig.

It serves three purposes:

- **The reference for other DAWs.** It is the complete, readable implementation of
  [HOST_BRIDGE_GUIDE.md](HOST_BRIDGE_GUIDE.md): every address, every command, the host features
  and the call sequence. It is smaller and easier to follow than the Bitwig extension, which is
  bound to the Bitwig API, and the Reaper backend, which is bound to C++ and the Reaper API.
- **Conformance without Bitwig.** All 195 scenarios run in seconds, in `cargo test`'s
  neighbourhood and in CI. A behaviour change of the core shows at once, not only at the next
  Bitwig run.
- **A Python binding.** Zynthian and other Python hosts can use it (see
  [BINDINGS_PLAN.md](BINDINGS_PLAN.md), "Language wrappers").

**Why a simulated DAW.** The Bitwig conformance runs observe Bitwig itself: the runner sets and
reads Bitwig's state through the OSC conformance module of the Java DrivenByMoss (branch
`conformance-osc`). A Python host cannot run inside Bitwig. So the reference host brings its own
DAW model: just enough of a DAW for the scenarios. Its results are compared with the recorded
results of the Java version on Bitwig.

**Non-goals.**

- A usable DAW: no audio, no MIDI playback, no UI.
- A replacement for the Bitwig runs. Passing shows that the bridge implements the protocol as the
  core expects it, and that the core behaves as Java on Bitwig in what the simulation models. The
  Bitwig runs stay the reference.

## 1. Architecture

```
 scenario (TOML) ──► runner ──MIDI──► dbm_on_midi_event ─┐
        │              ▲                                 │  core (libdrivenbymoss)
        │              └──LEDs── send_midi_data ◄────────┤
        ▼                                                │
  fixture + OSC setup ──► SimDaw (model) ◄── commands ───┘  process_*_arg callbacks
                            │
                            └── reports (change-only lines) ──► dbm_update_model
```

| Layer | Path | Contents |
| --- | --- | --- |
| Binding | `bindings/python/drivenbymoss/` | ctypes over `include/drivenbymoss.h`: `App` (the handle), `Backend` (callbacks as overridable methods), the string, ownership and threading rules |
| Reference host | `bindings/python/dbm_host/` | `SimDaw` (the DAW model), `Reporter` (change-only reporting), `CommandHandler` (core → model), `Bridge` (connects them to an `App`) |
| Conformance | `bindings/python/conformance/` | The in-process runner, the fixture, the translation of the scenarios' OSC commands, the `/conformance` state view, the comparison with the Java results |

Rules for all three:

- Python 3.11 or newer (`tomllib`), standard library only, no dependencies. Same rule as the
  crate.
- The license is LGPLv3, with the header as in the other files.
- Every reference-host module names the section of HOST_BRIDGE_GUIDE.md it implements, and the
  Java Bitwig class whose behaviour it simulates.

## 2. Phase 1: the binding

- **Loading.** ctypes loads `libdrivenbymoss.so` (`.dll`, `.dylib`). Search order:
  `DBM_LIBRARY`, then `target/debug`, then `target/release`. The prototypes are written by hand
  from the header: 25 functions and 19 callbacks, in `_ffi.py`.
- **`App`:**
  - Creates the instance with `DBM_FLAG_HOST_TICK`; the host calls `tick()` from its own loop.
  - Methods: `create_controller`, `start_infrastructure`, `update_model`, `on_midi_event`,
    `tick`, and the settings and document-settings calls.
  - `close()` and a context manager call `dbm_destroy`.
  - Returned strings are copied and released with `dbm_free_string`.
- **`Backend`:**
  - One method per callback: `process(processor, command, value)` with typed variants,
    `send_midi`, `open_midi_input`, the note input filters and translation tables, `log`.
  - The defaults do nothing.
  - The trampolines keep references to the ctypes function objects for the lifetime of the
    `App`, catch exceptions and log them.
- **Model setup:** `get_model_setup()` returns the parsed JSON (`ModelSetup`).
- **Tests** in `bindings/python/tests/` (unittest), as `bindings/cpp/tests`:
  - Create, add a controller, tick, MIDI in and out.
  - Settings round trip.
  - An exception in a callback does not cross the boundary.

  `ci/linux.sh` runs them after `cargo build`.

## 3. Phase 2: the reference host

### The DAW model (`SimDaw`)

Plain classes, one per HOST_BRIDGE_GUIDE §3 area:

- `Transport`: play, record, loop, punch, click, tempo, position, automation write mode,
  overdub.
- `Application`, `Arranger`, `Mixer`: layout, panels, undo.
- `Project`: name, cue, remote controls.
- `Track`: name, type, color, volume, pan, mute, solo, arm, monitor, activated, sends, slots,
  VU, playing notes, devices.
- Track lists:
  - All tracks.
  - Effect tracks.
  - Master.
- `Scene`, `Slot`: content, playing, queued, recording, selected, launch quantization ignored.
- `Device`: bypass, window, expanded, pinned, siblings, remote-control pages, layers with
  volume/pan/mute/solo, drum pads.
- `Clip`: a note grid per requested size, steps with all note attributes, loop, play range.
- `Marker`, `Browser` (columns and results as a static list), `Groove`.

The model knows nothing about the core. It only has state and operations: `track.set_volume`,
`slot.launch`, `transport.play`. The behaviour of an operation follows the Java Bitwig
implementation where the scenarios can see it, for example:

- The slot selection of the clip launcher navigator.
- Exclusive solo, as the preferences.
- Launching a scene starts the transport.
- Recording into an empty slot.

Time is simulated: `SimDaw.advance(ms)` moves the playhead, starts queued clips at the next bar
and updates the VU and the playing notes.

### Reporting (`Reporter`)

- Reports the model as the Bitwig extension does (`BitwigModel.collect`, HOST_BRIDGE_GUIDE §2):
  change-only lines, a full dump on request, `/host/features` with every feature the
  simulation supports, `/track/offset` and the bank sizes of the model setup.
- **Windows.** The Bitwig extension reports windows of the banks, because Bitwig has only
  windows. The reference host reports all items, as the Reaper backend does. This is simpler for
  other DAWs, and it avoids the window lag (`BitwigModel.scrollTrackWindow`). The `bank` commands
  only store the offset, for `/track/offset`.

  The Launch Control XL pages correctly with all items reported (step 2); whether any other
  controller needs windowed reports shows in the later steps.

### Commands (`CommandHandler`)

- One handler per processor, as BACKEND_PROTOCOL.md §2 and HOST_BRIDGE_GUIDE §4.
  - Every command the core sends maps to a model operation.
  - An unknown command is logged once, as in the Bitwig extension (`Not handled yet: …`).
  - The conformance runner counts these logs and fails on any.
- The host features of HOST_BRIDGE_GUIDE §2 are all implemented: valueInc, valueReset,
  immediately, indication, soloToggle, transportCommands, deviceCommands, layoutStep,
  clipLauncherNavigator.

### `Bridge`

- Connects an `App`, a `SimDaw`, the `Reporter` and the `CommandHandler`.
- The loop: MIDI in, `tick`, report changes, `update_model`.
- The model setup of the controller creates the banks and clip grids, as `BitwigModel` does at
  init.
- This is the part another DAW copies: replace `SimDaw` by the DAW's own API.

## 4. Phase 3: conformance in process

### The runner

`python -m conformance <setup> [file…]` reads the same TOML files as
`backends/bitwig/tools/conformance_runner.py` and runs them in process:

- **Controller.** A fresh `App` with the controller of the setup and its settings: the profile's
  `.config` values, plus the Flexi mapping written by the runner's `flexi_mapping_properties`
  (reused, not copied).
- **MIDI.** Scenario MIDI goes directly to `on_midi_event`. The LEDs come from `send_midi`, with
  the same LED state logic as the runner (`led_state`, reused).
- **OSC steps.** The `setup`/`before` commands become model operations. The scenarios use 32
  addresses today, the `/track/N/…`, `/device/…`, `/primary/…` and transport commands of the
  DrivenByMoss OSC protocol. A table maps each one to a `SimDaw` call.
- **State view.** The state view produces the same keys as the OSC conformance module
  (`osc:/track/1/volume`, `osc:/conformance/primary/…`) from the model. Scenario expectations
  use 57 of the 3711 keys a Java trace has.
- **Settling.** `settle_ms` becomes a simulated time advance with ticks: no sleeping, no damaged
  MIDI. The core runs on the simulated time through `dbm_set_time` (added for this, 2026-11-02).
- **Restarts.** A restart of the controller (`restart_controller`) is a new `App`.

### The reference results

The Java traces in `target/conformance/<setup>-<file>-java.json` (about 5 MB each, not committed)
hold, for every scenario:

- The LED state and the Bitwig state before it starts.
- The changes after every step.

A script, `bindings/python/conformance/extract.py`, writes `<setup>-<file>-java.ref.json` next to
the committed `.leds` and `.known` files. Per scenario and step it keeps the LED changes and the
Bitwig state changes, limited to the keys the state view knows. It is updated with the replays,
from clean runs only, as the conformance-run skill describes.

### Comparison

- For each step, compare the LED changes and the state changes with the reference.
- `known_difference` entries of the scenario files apply as in the Bitwig runs.
- A key the reference changes and the state view does not model is reported as "not modelled",
  not as a difference.
- The run fails on:
  - A difference.
  - A failed expectation.
  - An unhandled command.
  - Too many "not modelled" keys in a file. The allowed number is listed per file and goes down
    as the model grows.

### CI

- `ci/linux.sh` runs `python -m conformance --all` after the binding tests.
- The Bitwig runs stay manual.

## 5. Order and effort

| Step | Scope | Proves | Estimate |
| --- | --- | --- | --- |
| 1 | Binding + tests | The C ABI from Python | 0.5 day |
| 2 | `SimDaw` core (transport, tracks, sends, master), `Reporter`, `CommandHandler`, `Bridge` | A controller runs against the simulation | 1 day |
| 3 | Runner, fixture, OSC table, state view, `extract.py` | One file compares | 1 day |
| 4 | Launch Control XL (track mix, equalizer, layer mix, note/drum sequencer): devices, layers, the clip grid | The first setup passes | 1–2 days |
| 5 | Maschine Mk2 (global, pad modes): drum pads, browser, text display | | 1 day |
| 6 | Maschine JAM: session (slots, scenes, launch, the clip launcher), play modes, sequencers, touch strips | | 1–2 days |
| 7 | Generic Flexi (100 scenarios): markers, effect tracks, navigator, groove, project, layouts, everything else | All setups pass | 2–3 days |
| 8 | Docs: HOST_BRIDGE_GUIDE points to the reference host for each section; README of `bindings/python` | | 0.5 day |

After step 3, every later step adds model parts until its setup passes. A difference which the
model cannot explain is a finding about the core. It goes through the usual path: check the Java
original, then fix the core or record a known difference.

## 6. Risks

- **Fitting the simulation to the results.** A model operation should be written from the Java
  Bitwig implementation and the Bitwig API documentation, not by tuning it until a scenario
  passes. Where the simulation deliberately simplifies (launch quantization, scroll timing,
  automation), the module says so.
- **The reference results age.** They change with the Java version and the scenarios. They are
  extracted only from clean Bitwig runs, together with the replays.
- **Bitwig details the scenarios depend on without saying so** (preferences such as exclusive
  solo, the takeover mode, the fixture's devices). The fixture builder documents each one with
  its source.
- **Effort of Flexi.** It touches nearly every address. It goes last, and its "not modelled"
  allowance may stay above 0 for a while.

## 7. Sources

- The behaviour to simulate:
  - The Java DrivenByMoss `bitwig/framework/**` (LGPL).
  - The Bitwig extension of this repository (`BitwigModel`, `CommandHandler`).
  - [HOST_BRIDGE_GUIDE.md](HOST_BRIDGE_GUIDE.md), [BACKEND_PROTOCOL.md](BACKEND_PROTOCOL.md),
    [BITWIG_DIVERGENCES.md](BITWIG_DIVERGENCES.md).
- Bitwig's public Controller API documentation, to understand what a Bitwig call does. It is
  used for semantics only: no text or code is copied from it, and the Bitwig API is not a
  dependency of the reference host.

## 8. Open questions

1. A target DAW besides the simulation (Zynthian, Ardour through its Lua/OSC, Carla)? It would
   shape the `SimDaw` interface towards what that DAW offers.
2. The package names `drivenbymoss` (binding) and `dbm_host` (reference host). Publish the
   binding on PyPI later?
3. Should the in-process conformance also replay the existing Rust `.rec` recordings? It would
   be a cross-check of the binding against `tests/conformance_replay.rs`.
