# DrivenByMoss on Zynthian: implementation plan

Status: 2026-11-06. Phase 1 started: the driver base, the Launch Control XL driver, the mixer
adapter, the PipeWire port names and 31 tests are in the zynthian-ui worktree (§7). The analysis
behind it: [ZYNTHIAN_INTEGRATION.md](ZYNTHIAN_INTEGRATION.md) (cited as "analysis §n").

**Goal.** zynthian controller drivers (ctrldev) host the DrivenByMoss core, so that the core's
controllers drive zynthian: the core runs the controller (modes, LEDs, displays), zynthian is its
DAW. First the Novation Launch Control XL on zynthian's mixer.

**Where the code lives.**

| Part | Repository | Path | License |
| --- | --- | --- | --- |
| Core, C ABI, binding, reference host | drivenbymoss-rust (`main`) | `include/drivenbymoss.h`, `bindings/python/drivenbymoss/`, `bindings/python/dbm_host/` | LGPLv3 |
| Drivers, model adapter, tests | zynthian-ui worktree `~/bastel/github/zynthian-dbm` (branch `dbm`, from `nura`) | `zyngine/ctrldev/zynthian_ctrldev_base_dbm.py`, `zyngine/ctrldev/zynthian_ctrldev_dbm_*.py`, `zyngine/ctrldev/dbm_zynthian/` | GPLv2+ (zynthian's) |
| PipeWire port names | same | `zynautoconnect/zynthian_pipewire_ports.py` | GPLv2+ |
| Packages | same | `nura/drivenbymoss/APKBUILD` (phase 4) | – |

The dbm side stays Python ≥ 3.11 with the standard library only. The zynthian side follows
zynthian-ui's style (4 spaces, its header, its imports) and adds no dependency.

## 1. Architecture

```
 Launch Control XL ──USB──► PipeWire ──JACK──► ZynMidiRouter (zyncoder, C)
                                                 │ UI ring buffer (unroute_from_chains: only to the UI)
                                                 ▼
        state manager fast thread: zynmidi_read ─► ctrldev_manager.midi_event ─► driver.midi_event
                                                                                   │ queue (no dbm_* call)
                                                                                   ▼
 driver tick thread (the only thread calling dbm_*):  ZynBridge.step
   MIDI → dbm_on_midi_event → dbm_set_time → dbm_tick → ZynReporter.collect → dbm_update_model
   callbacks during these calls:  process_* → ZynCommandHandler → zctrl.set_value (zynmixer)
                                  send_midi_data → lib_zyncore.dev_send_midi_event(idev_out)
                                  notifications → zynsigman SS_GUI_SHOW_MESSAGE
```

## 2. The driver

### Files

| File | Content |
| --- | --- |
| `zyngine/ctrldev/zynthian_ctrldev_base_dbm.py` | `zynthian_ctrldev_base_dbm(zynthian_ctrldev_base)`: lifecycle, tick thread, MIDI queue, state, sleep. Skipped by the manager (name prefix `zynthian_ctrldev_base`) |
| `zyngine/ctrldev/zynthian_ctrldev_dbm_<controller>.py` | One class per controller, the module's name: `dev_ids`, `dbm_controller_id`, `driver_name`, `driver_description`, `autoload_flag` |
| `zyngine/ctrldev/dbm_zynthian/__init__.py` | `import_dbm()`: finds `drivenbymoss` and `dbm_host` (`sys.path`, `$DBM_PYTHON_DIR`, `/usr/share/drivenbymoss/python`), defaults `$DBM_LIBRARY` |
| `zyngine/ctrldev/dbm_zynthian/{model,reporter,commands,bridge}.py` | The model adapter (§3) |
| `zyngine/ctrldev/dbm_zynthian/tests/` | `zynstubs.py` (stand-ins), `run_tests.py`, the tests (§6) |
| `zyngine/ctrldev/dbm_zynthian/generate.py` (phase 4) | Writes the per-controller modules from `dbm_list_controllers()` |

**Per-controller modules: hand-written first, generated later.** The Launch Control XL module is
hand-written in the form the generator will produce. The generator (phase 4) needs per controller:
the UUID (catalog), the dev_ids (from `discovery.linux`, §2.2, plus a table of hand fixes), the
names, and an autoload flag (off where a stock driver exists, analysis §8). Only controllers with
one MIDI port each way are generated.

### 2.1 Matching (dev_id ↔ the core's discovery names)

- The manager matches `get_midi_in_devid(izmip)` exactly against `dev_ids` (analysis §1).
- The dev_id of a port is `<card> IN <n>` on ZynthianOS and, with
  `zynautoconnect/zynthian_pipewire_ports.py`, on nura's PipeWire too.
- The core's `discovery.linux` input names map to dev_ids: `<card>` → `<card> IN 1`;
  `<card> MIDI <n>` → `<card> IN <n>`; other forms (`Launch Control XL Launch Contro`, the a2j
  truncation) are dropped. Example: `"Launch Control XL"` → `"Launch Control XL IN 1"`,
  `"Launchpad X MIDI 2"` → `"Launchpad X IN 2"`. Each generated dev_id is confirmed on hardware
  (`/proc/asound/cardN/midi0` on ZynthianOS, `pw-link -o` on nura) before its module ships.
- The output port is `dev_in_2_dev_out` (fixed for names containing "IN", analysis §9 Z1). Without
  an output (`idev_out` None) the driver runs but sends nothing.

### 2.2 The tick thread

- `init()` (called by the manager after `unroute_from_chains`): `super().init()` registers the chain
  and snapshot signals (they call `refresh`), then starts the thread `dbm <driver name>`.
- The loop (`loop()`): wait on an `Event` with 30 ms timeout (`midi_event`, `refresh`,
  `set_state`, sleep and `end` wake it), then `step(now_ms)`: apply pending document settings,
  request a dump if asked, MIDI from the queue → `receive`, `ZynBridge.tick(now_ms)` (time, tick,
  report, update_model), notifications → `SS_GUI_SHOW_MESSAGE`. Once per second it caches
  `get_formatted_document_settings()` for `get_state`.
- `midi_event(ev)` (fast thread): queues `ev` if its first byte is a status byte (analysis §9 Z4),
  returns True always (the device belongs to the core).
- `end()`: sets the exit flag, wakes the thread, joins it (5 s); the thread closes the bridge
  (`dbm_destroy`: the controller's shutdown, final MIDI, the `.config`), then `super().end()`
  unregisters the signals.
- Time: the core's clock is driven with `dbm_set_time` (ms since the controller started), so the
  same `ZynBridge` runs on simulated time in the tests.

### 2.3 Start, stop, sleep, state

| Event | Driver |
| --- | --- |
| load (manager) | `__init__` (model, queue), `init` (thread); the thread: `ZynBridge.start` = `dbm_create_controller` (config `$ZYNTHIAN_CONFIG_DIR/ctrldev/dbm`), `dbm_start_infrastructure`, model setup → banks, first report |
| unsupported controller / missing library | logged once, the driver stays loaded and inert (`start_failed`) |
| `sleep_on` (power save) | the thread stops the controller (`dbm_destroy` switches the LEDs off) and keeps its document settings |
| `sleep_off` | a new controller with the kept document settings |
| `get_state` (ZS3 save, any thread) | `{"dbm_document": <cached formatted document settings>}` |
| `set_state` (ZS3 load) | queued; the thread calls `dbm_set_formatted_document_settings` |
| chains added/removed/moved, snapshot loaded | `refresh` → dump with the next tick |
| unload (manager) | `end` |

Global settings stay in the core's `<model>.config` in `$ZYNTHIAN_CONFIG_DIR/ctrldev/dbm/`
(analysis §5); a settings UI is phase 4.

## 3. The model adapter

### 3.1 Reuse of the reference host: decided

**Decision: reuse `dbm_host`'s infrastructure classes by subclassing; do not imitate `SimDaw`.**

- `ZynBridge(dbm_host.Bridge)`: the startup sequence (create, start, model setup → `Banks`,
  `CommandHandler.configure`, discard setting changes, first report), the step (MIDI → time → tick
  → report → update_model, notifications), the note inputs (`set_note_input_*`, `NoteInput`
  translation) and `advance` for simulated time. Overridden: no MIDI/report/log histories (the
  reference host keeps them for its tests), MIDI out to a sink, `tick(now_ms)` for real time.
- `ZynReporter(dbm_host.Reporter)`: the change tracking, dumps, `forget_later` (report again
  300 ms after a value command, HOST_BRIDGE_GUIDE §6.1), text formatting and limits. Own
  collectors: host features, tracks, effect tracks, master (plus "exists 0" for removed positions).
- `ZynCommandHandler(dbm_host.CommandHandler)`: `Command` parsing (`/inc`, `/reset`,
  `/immediately`), dispatch, the once-only "Not handled yet" log, `_report_again`, `value_command`,
  `host_features`. Own handlers; only theirs are registered, so the host features are exactly
  what zynthian implements. No undo (`set_value` without `edit_value`).
- `ZynthianModel` (not a `SimDaw`): live views over zynthian's objects (`ChainTrack`,
  `ChainSend`, `ZctrlParameter`, which duck-types `dbm_host.Parameter`).

Reasons:

- The reusable part is the protocol logic, which took the conformance work to get right
  (selection only on change, report again after value commands, the dump after `refresh`, the
  value suffixes, the unhandled log). Copying it would fork it; subclassing keeps one copy and
  the reference host's tests cover it.
- A `SimDaw`-compatible `ZynthianDaw` (the first analysis' guess) would have to fake about 30
  classes (application, arranger, markers, devices, clips, …) whose methods the collectors and
  handlers call; most would be stubs, and every new collector in `dbm_host` would break it.
  Zynthian-specific collectors are shorter than the fakes.
- zynthian's state must be read live (the GUI, MIDI learn and snapshots change it); a mirror
  `SimDaw` would need a two-way sync.

The change in `dbm_host` for this: `Bridge.reporter_class` and `Bridge.command_handler_class`
(class attributes, defaults unchanged). Its tests still pass (81). If `dbm_host` grows a public
"host kit" later, the zynthian classes move to it unchanged.

### 3.2 Mapping

| Core area | zynthian | Phase | What the core sees for the gaps |
| --- | --- | --- | --- |
| Tracks `/track/<p>` | chains without chain 0 and mixbuses, display order; with `hasFullFlatTrackList` (the LCXL) then the mixbuses and the master | 1 | no `active`, `depth`, groups; colors from `LAUNCHER_COLOUR` by MIDI channel, gray for audio chains |
| Volume, pan, mute, solo, recarm | strip zctrls `level`, `balance`, `mute`, `solo`, `record`; texts `x.x dB`, pan percent | 1 | a MIDI chain without strip: values 0, its commands ignored |
| Selection | `chain_manager.active_chain` / `set_active_chain_by_id` | 1 | – |
| VU | `getDpm` dBFS −60..0 → 0..1 | 1 | no hold values (`vuhold*` not reported) |
| Effect tracks `/fxtrack/<p>`, sends | mixbus chains; `send_<id>_level` | 1 | `send/<s>/active` not handled (no switch); always reported active |
| Master `/master` | chain 0, main mixbus | 1 | – |
| Paging | all items reported, `bank` stores the offset (`/track/offset`, `/fxtrack/offset`) | 1 | – |
| Host features | `valueInc valueReset immediately soloToggle` | 1 | without `indication` the core sends no indications (zynthian has none); without `transportCommands` the core sends the Reaper transport forms, unhandled until phase 2 |
| Devices `/device/…` | the active chain's processors; cursor = `chain.current_processor`; siblings = the chain's processors; pages = `get_ctrl_screens()` (4 zctrls each, two screens per 8-parameter page); value normalized from `value_min/max` (logarithmic zctrls by their own scale), text `get_value2label`; bypass | 2 | no window/expand/pin: not reported; no layers/drum pads (`wantsDrumDevice` sees no `/drum`) |
| Transport, tempo | `/tempo` from zynseq, `play`/`stop` → `zynseq.transport_toggle/stop`, metronome `setMetronomeMode/Volume`, `record` → audio recorder | 2 | no arranger: `/beats/position`, loop, punch, automation not reported; "play" means zynseq transport (decision for phase 2) |
| Scenes, slots | zynseq phrases = scenes; sequences per chain MIDI channel = slots; play states → playing/queued | 3 | launch is a toggle in zynseq: the adapter launches only stopped sequences and stops on `stop`; no recording slots |
| Note inputs | `NoteInput.translate` → `lib_zyncore.ui_send_note_on/off` on the active chain's MIDI channel (`ZynthianModel.note_sink`); `process_midi_arg` the same | 3 | latency 0–40 ms via Python; a real-time midiproc later |
| Step grid `/clip/…` | the selected sequence's pattern (`libseq.getPattern`, `addNote`, `removeNote`, velocity, duration, chance) | 3 | one pattern per sequence; only velocity, duration and chance of the 24 step attributes |
| Browser, markers, groove, project, application | – | – | not reported; their commands are logged once as not handled |

## 4. Phases

### Phase 1: the Launch Control XL on the mixer

Scope: volume, pan, mute, solo, arm, sends, track selection and paging with the Track Mix
template; the driver lifecycle; nura's port names.

| Step | State |
| --- | --- |
| 1.1 `Bridge` factory hooks in `dbm_host` | done |
| 1.2 `ZynthianModel`, `ZynReporter`, `ZynCommandHandler`, `ZynBridge` | done |
| 1.3 `zynthian_ctrldev_base_dbm` (thread, queue, state, sleep) | done |
| 1.4 `zynthian_ctrldev_dbm_launchcontrol_xl` | done |
| 1.5 PipeWire uids and the `dev_in_2_dev_out` fix in zynautoconnect | done |
| 1.6 Tests (a) and the LCXL against the core (§6) | done (31 tests) |
| 1.7 In-process conformance of `track_mix.toml` against the adapter (§6 b) | open |
| 1.8 Manual test in desktop mode (§6 c) | open (§7) |

Acceptance:

- The tests of §6 (a) pass with `python3 zyngine/ctrldev/dbm_zynthian/tests/run_tests.py`;
  the dbm repository's tests still pass.
- On a PC in desktop mode with the device: the driver autoloads for the Launch Control XL
  (MIDI device options show it checked), the template LEDs light, fader 1 moves the first chain's
  level in the mixer GUI, the mute/solo/arm buttons toggle and light, a mute in the GUI lights the
  button within 100 ms, track left/right pages, Track Focus selects the chain in the GUI.
- No `ERROR` from the core in the log; the unhandled commands are only those of later phases.
- Unloading the driver (MIDI device options) gives the device back to the chains; power save
  switches the LEDs off and back on with the same template state.

Risks: the PipeWire uid change breaks the routing stored in existing nura snapshots; the LCXL's
real PipeWire port name differs from the inferred one; the tick thread's zctrl changes race with
the GUI (as stock drivers); Python cost at 33 Hz on the Duet.

### Phase 2: devices and parameters, transport

Scope: the cursor device (the active chain's current processor), siblings, the 8-parameter pages
from zynthian's 4-parameter screens, bypass; tempo, play/stop, metronome; `deviceCommands` and
`transportCommands` in `/host/features`.

Files: `dbm_zynthian/device.py` (`ProcessorDevice`, page mapping), `dbm_zynthian/transport.py`,
collectors and handlers in `reporter.py`/`commands.py`; tests with mocked processors.

Acceptance: the LCXL's Device mode (or the Track Mix knobs in device mode) turns the parameters
of the active processor, its names and values show in the core's model (`/device/param/<i>/name`);
parameter pages page through zynthian's screens; the LCXL's transport-related commands of the
Track Mix are no longer unhandled; the `equalizer.toml` scenarios run in the adapter conformance
with listed known differences.

Risks: zctrl ranges (logarithmic, ticks, integer, labels) need per-kind normalization; the
screens change when a processor changes (`SS_PROCESSOR_CTRL_SCREENS` → dump); what "play" means
(zynseq vs. audio player) needs the user's decision.

### Phase 3: Launchpad and zynseq

Scope: scenes and slots on zynseq, launch/stop, the note inputs into the chains, the step grid;
a Launchpad (X or Mini MK3 on port 2) driver module, conflicts with the stock drivers on port 1
documented.

Files: `dbm_zynthian/launcher.py` (phrases/sequences), `dbm_zynthian/clip.py` (pattern grid),
`note_sink` in the driver base (`ui_send_*`), `zynthian_ctrldev_dbm_launchpad_x.py`.

Acceptance: session view launches and stops sequences with the right LED states (playing, queued,
stopped); scene launch starts the phrase; the note view plays the active chain; the step sequencer
edits the selected pattern and the zynseq GUI shows it after a refresh; the Launchpad X conformance
scenarios that fit zynseq pass in the adapter conformance.

Risks: launch is a toggle; one pattern per sequence; note latency (then a midiproc and a
zynautoconnect change); two drivers on two ports of one Launchpad.

### Phase 4: packaging and the device

Scope: packages for Nura, the generator, settings, a test on the Duet.

- `nura/drivenbymoss/APKBUILD` (§5); zynthian-ui's APKBUILD `depends` on it (or the drivers stay
  inert without it, logged).
- `dbm_zynthian/generate.py` for the single-port controllers.
- A settings screen: `get_settings`/`set_setting` through a small patch of the MIDI device options
  (`zyngui/zynthian_midi_config_data.py`, used by both UIs), or the controller's own menus.
- The Qt UI shows driver messages (`SS_GUI_SHOW_MESSAGE` in `zynqt`).

Acceptance: `pmbootstrap build` of the package for aarch64 and x86_64 without network; on the Duet
the LCXL works as in phase 1; CPU of the tick thread measured (`nura/tools/thread_cpu.sh`) below
5 % of one core with 16 chains [target, not measured].

Risks: the vendored crates and the Lua C build in pmbootstrap; musl specifics; a stale
`libdrivenbymoss.so` against a newer binding (version check: `dbm_version`).

## 5. Packaging for Nura

- **Native library.** `cargo build --release --target aarch64-unknown-linux-musl` and
  `--target x86_64-unknown-linux-musl`. In an APKBUILD (pmbootstrap builds natively in a chroot of
  the target, with qemu for aarch64) the target is the host: `cargo build --release --frozen`.
  - **First version without Lua:** `--no-default-features`. The `lua` feature builds Lua 5.4 from
    C (mlua `vendored`), which needs a C compiler for the target: fine natively in Alpine's chroot
    (`gcc`), but a cross build needs a musl cross compiler (`aarch64-linux-musl-gcc`) and
    `CC_aarch64_unknown_linux_musl`. The Lua controllers (`Launch Control XL (Lua)`) are not
    needed on Zynthian yet.
  - **No network in pmbootstrap**: the crates (only cbindgen and its dependencies without Lua) go
    into a `cargo vendor` tarball listed in `source` (or the distfiles cache, as
    `nura/seed-distfiles.sh` does for zynthian-ui), with `.cargo/config.toml` pointing to it.
  - `crate-type` includes `staticlib`; only `libdrivenbymoss.so` is installed.
- **Package `drivenbymoss`** (`nura/drivenbymoss/APKBUILD`, from a `--src` checkout as zyncoder):
  - `/usr/lib/drivenbymoss/libdrivenbymoss.so` (the drivers' default `DBM_LIBRARY`);
  - `/usr/share/drivenbymoss/python/{drivenbymoss,dbm_host}/` (compiled with `compileall`, as
    zyncoder), the drivers' default `DBM_PYTHON_DIR`;
  - `license="LGPL-3.0-only"`, `depends="python3"`, `makedepends="cargo"` (+ `gcc` with Lua),
    `arch="aarch64 x86_64"`, `options="!check"` at first (the Python tests need no network and
    could run in `check()` later).
- The drivers are part of zynthian-ui (`zyngine/ctrldev/`), so the zynthian-ui package carries
  them; `nura/deploy.sh drivenbymoss zynthian-ui` updates a device.

## 6. Testing

**(a) Unit tests without the UI** (done, 31 tests). `zyngine/ctrldev/dbm_zynthian/tests/`:

- `zynstubs.py`: `zyngine` as a bare package (its `__init__` imports every engine), stand-ins for
  `zynautoconnect`, `zynlibs.zynseq`, `pyliblo3`, `zyngui.zynthian_gui_config` and a recording
  `lib_zyncore`; real `zynthian_controller` zctrls configured as the mixer engine does, and the
  real signal manager. `ZynthianState` builds chains, strips, mixbuses and send zctrls as
  `refresh_mixbus_sends`.
- `test_model.py`: mapping, texts, VU, change-only reports, selection batches, removed chains,
  new mixbuses, refresh, every phase-1 command, report again, unhandled log, bank.
- `test_launchcontrol_xl.py`: the real core with the LCXL on simulated time, the device stub
  echoing the template: startup LEDs, faders, pan and send knobs, mute/solo/arm with LED
  feedback both ways, Track Focus selection both ways, paging, all controls (unhandled only of
  later phases), no core errors.
- `test_driver.py`: the driver class as the manager runs it, real time: tick thread, MIDI from
  another thread, LED feedback, state, sleep/wake, end.
- `test_pipewire_ports.py`: the PipeWire uids and the `dev_out_uid` fix.
- zynthian-ui has no unit test suite (only manual scripts such as
  `zyngui/zynthian_gui_control_test.py`, `zynlibs/zynseq/test.py`); the runner is
  `python3 zyngine/ctrldev/dbm_zynthian/tests/run_tests.py` (plain `unittest discover` cannot be
  used: it imports the real `zyngine` package before the stand-ins).

**(b) In-process conformance against the adapter** (step 1.7, phases 2–3). Reusable from
`bindings/python/conformance` and `backends/bitwig/tools/conformance_runner.py`: the scenario
TOML format and parser, the MIDI steps, the device stubs (`[[stub]]` echo), the LED state logic,
`settle_ms` as simulated time, `known_difference` matching, the profile settings. Not reusable:

- The fixture (`tests/conformance/FIXTURE.md`): Bitwig tracks with devices, clips, a drum machine
  and the EQ+. A zynthian fixture (`ZynthianState` with the same track names and count) covers only
  the mixer part; `require` entries such as `/primary/page/2/exists` fail by design.
- The Java references (`replay/*-java.ref.json`): Bitwig's values (its volume curve and texts,
  colors, transport) differ from zynthian's, so LED values that depend on them differ. Compare
  only the LED keys whose meaning is model-independent (button on/off of mute/solo/arm/select)
  and the `expect.bitwig` keys translated to the zynthian state view (`/track/<n>/volume`,
  `mute`, `solo`, `selected`).
- The OSC `setup`/`before` steps: the track ones (`/track/<n>/select|mute|solo|volume`) map to
  `ZynthianState`; device, clip and transport steps skip the scenario until their phase.

Plan: `zyngine/ctrldev/dbm_zynthian/tests/conformance.py`, a runner over
`tests/conformance/launchcontrol_xl/track_mix.toml` (path from `$DBM_REPO`) with a scenario
allow-list per phase and its own known differences. It imports the dbm runner's functions; it
needs nothing in the dbm repository except read access.

**(c) Manual testing in desktop mode on a PC** (`desktop/zynthian_desktop.sh` with
`DBM_PYTHON_DIR=<dbm>/bindings/python`, the library from `<dbm>/target/debug`). zynautoconnect
connects **every** hardware MIDI port it finds, so this test needs the user's go-ahead: it opens
the attached Launch Control XL and the VirMIDI ports (used by the conformance tools). Checklist:
the acceptance list of phase 1.

## 7. State of phase 1 (2026-11-06)

Done (zynthian-ui worktree, branch `dbm`, not committed):

- `zynautoconnect/zynthian_pipewire_ports.py` (new), `zynautoconnect/zynthian_autoconnect.py`
  (PipeWire uids in `build_midi_port_name`, `dev_in_2_dev_out` fix).
- `zyngine/ctrldev/zynthian_ctrldev_base_dbm.py`, `zyngine/ctrldev/zynthian_ctrldev_dbm_launchcontrol_xl.py`.
- `zyngine/ctrldev/dbm_zynthian/` (`__init__`, `model`, `reporter`, `commands`, `bridge`) and
  `tests/` (31 tests, all passing).
- drivenbymoss-rust: `bindings/python/dbm_host/bridge.py` (factory hooks), this plan, the updated
  analysis.

Open: 1.7 (adapter conformance), 1.8 (manual test: not run, it would open the hardware MIDI ports,
§6 c). The real zynthian UI was not started.

## 8. Open questions

1. The PipeWire uid change (`PW:<client>/<client> IN <n>`) breaks MIDI routes stored in existing
   nura snapshots. Acceptable (nura is not deployed), or keep the old uid and only add a dev_id?
2. Should the core's document settings follow the ZS3 (zynthian's model) or the snapshot only?
3. What "play" means for the core in phase 2: zynseq's transport, or the audio player as
   zynthian's play button does depending on the screen?
4. May the manual desktop test open the Launch Control XL (and therefore all hardware MIDI ports,
   including VirMIDI) once the conformance tools are idle?
5. Ship the drivers in the zynthian-ui package (as now) or as user drivers in the drivenbymoss
   package?
