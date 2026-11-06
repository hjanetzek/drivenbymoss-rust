# DrivenByMoss on Zynthian: implementation plan

Status: 2026-11-06. **Phase 1 done** (§4, §7): the driver base, the Launch Control XL driver, the
mixer adapter, the PipeWire port names, the conformance scenarios of the Track Mix against the
adapter and a run in desktop mode with the device. **Phase 2 started**: devices and parameters, and
zynseq's transport, with tests. 41 tests on the zynthian side. The analysis behind it:
[ZYNTHIAN_INTEGRATION.md](ZYNTHIAN_INTEGRATION.md) (cited as "analysis §n").

**Goal.** zynthian controller drivers (ctrldev) host the DrivenByMoss core, so that the core's
controllers drive zynthian: the core runs the controller (modes, LEDs, displays), zynthian is its
DAW. First the Novation Launch Control XL on zynthian's mixer.

**Decisions** (the user's answers of 2026-11-06):

1. The PipeWire uids (`PW:<client>/<client> IN <n>`) are accepted; MIDI routes stored under the old
   uids are lost (a migration is optional, §8).
2. The Launch Control XL's driver takes its first port (`Launch Contro`, `IN 1`), never HUI.
3. The core's document settings belong to the **snapshot**, not the ZS3 (§2.3).
4. "Play" is **zynseq's transport** (§3.2).
5. The drivers ship in the **drivenbymoss package**, not in zynthian-ui (§5).

**Where the code lives.**

| Part | Repository | Path | License |
| --- | --- | --- | --- |
| Core, C ABI, binding, reference host | drivenbymoss-rust (`main`) | `include/drivenbymoss.h`, `bindings/python/drivenbymoss/`, `bindings/python/dbm_host/` | LGPLv3 |
| Drivers, model adapter, tests | zynthian-ui worktree `~/bastel/github/zynthian-dbm` (branch `dbm`, from `nura`) | `zyngine/ctrldev/zynthian_ctrldev_base_dbm.py`, `zyngine/ctrldev/zynthian_ctrldev_dbm_*.py`, `zyngine/ctrldev/dbm_zynthian/` | GPLv2+ (zynthian's) |
| zynthian fixes the drivers need | same | `zynautoconnect/zynthian_pipewire_ports.py` and `zynthian_autoconnect.py` (PipeWire uids, analysis §9 N1, Z1), `zynlibs/zynseq/zynseq.h` (`getTransportState`, §9 Z10) | GPLv2+ |
| Package | drivenbymoss-rust (phase 4) | `nura/drivenbymoss/APKBUILD` or the Nura packaging repo (§5) | – |

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
   callbacks during these calls:  process_* → ZynCommandHandler → zctrls, chain manager, libseq
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
| `zyngine/ctrldev/dbm_zynthian/{model,reporter,commands,bridge}.py` | The model adapter (§3); phase 2: `device.py`, `transport.py` |
| `zyngine/ctrldev/dbm_zynthian/tests/` | `zynstubs.py` (stand-ins), `run_tests.py`, the tests, `conformance.py` (§6) |
| `zyngine/ctrldev/dbm_zynthian/generate.py` (phase 4) | Writes the per-controller modules from `dbm_list_controllers()` and a port table |

**Per-controller modules: hand-written first, generated later.** The Launch Control XL module is
hand-written in the form the generator will produce. The generator (phase 4) needs per
controller: the UUID (catalog), the dev_ids (§2.1, from a table of each device's ports), the
names, and an autoload flag (off where a stock driver exists, analysis §8). Only controllers with
one MIDI port each way in the core are generated.

### 2.1 Matching (dev_id ↔ the core's discovery names)

- The manager matches `get_midi_in_devid(izmip)` exactly against `dev_ids` (analysis §1).
- The dev_id of a port is `<card> IN <n>` on ZynthianOS and, with
  `zynautoconnect/zynthian_pipewire_ports.py`, on nura's PipeWire too (`<n>` = 1-based index of
  the port among the device's ports of that direction, in ALSA's order).
- The core's `discovery.linux` input names → dev_ids: `<card>` and `<card> <ALSA name of port k>`
  → `<card> IN <k+1>`; `<card> MIDI <n>` → `<card> IN <n>`. Since the port index is not in the
  name, the generator takes it from a table of each device's ports, filled from hardware
  (`/proc/asound/seq/clients` or `pw-link -o`).
- **Launch Control XL** (seen on hardware): ports "Launch Contro" (0) and "HUI" (1) → dev_id
  `Launch Control XL IN 1`; HUI (`IN 2`) gets no driver. Output: `OUT 1` via `dev_in_2_dev_out`.
- Without an output (`idev_out` None) the driver runs but sends nothing.

### 2.2 The tick thread

- `init()` (called by the manager after `unroute_from_chains`): `super().init()` registers the chain
  and snapshot signals (they call `refresh`), then starts the thread `dbm <driver name>`.
- The loop: wait on an `Event` with 30 ms timeout (`midi_event`, `refresh`, `set_state`, sleep and
  `end` wake it), then `step(now_ms)`: apply pending document settings, request a dump if asked,
  MIDI from the queue → `receive`, `ZynBridge.tick(now_ms)` (time, tick, report, update_model),
  notifications → `SS_GUI_SHOW_MESSAGE`. Once per second it caches the document settings.
- `midi_event(ev)` (fast thread): queues `ev` if its first byte is a status byte (analysis §9 Z4),
  returns True always (the device belongs to the core).
- `end()`: sets the exit flag, wakes the thread, joins it (5 s); the thread closes the bridge
  (`dbm_destroy`), then `super().end()` unregisters the signals.
- Time: the core's clock is driven with `dbm_set_time` (ms since the controller started), so the
  same `ZynBridge` runs on simulated time in the tests.
- A failing collector (a zynthian object in an unexpected state, a missing library function) is
  logged once and skipped, the others still report (found in desktop mode: a missing
  `getTransportState` stopped the controller's start).

### 2.3 Start, stop, sleep, state

| Event | Driver |
| --- | --- |
| load (manager) | `__init__` (model, queue), `init` (thread); the thread: `ZynBridge.start` = `dbm_create_controller` (config `$ZYNTHIAN_CONFIG_DIR/ctrldev/dbm`), `dbm_start_infrastructure`, model setup → banks, first report |
| unsupported controller / missing library | logged once, the driver stays loaded and inert (`start_failed`) |
| `sleep_on` (power save) | the thread stops the controller (`dbm_destroy`) and keeps its document settings |
| `sleep_off` | a new controller with the kept document settings |
| `get_state` (ZS3 or snapshot save) | `{"dbm_document": <cached formatted document settings>}`: every ZS3 gets the current settings |
| `set_state` | applied (by the tick thread) only during a snapshot load (`state_manager.is_busy("load snapshot")`); a ZS3 recall leaves the settings (decision 3) |
| chains added/removed/moved, snapshot loaded | `refresh` → dump with the next tick |
| unload (manager) | `end` |

Global settings stay in the core's `<model>.config` in `$ZYNTHIAN_CONFIG_DIR/ctrldev/dbm/`
(analysis §5); a settings UI is phase 4.

## 3. The model adapter

### 3.1 Reuse of the reference host: decided

**Decision: reuse `dbm_host`'s infrastructure classes by subclassing; do not imitate `SimDaw`.**

- `ZynBridge(dbm_host.Bridge)`: the startup sequence, the step, the note inputs and `advance` for
  simulated time. Overridden: no histories, MIDI out to a sink, `tick(now_ms)` for real time.
- `ZynReporter(dbm_host.Reporter)`: the change tracking, dumps, `forget_later` (report again 300 ms
  after a value command), texts and limits. Own collectors (and a collector failure is contained).
- `ZynCommandHandler(dbm_host.CommandHandler)`: `Command` parsing, dispatch, the once-only
  "Not handled yet" log, the report again, `value_command`, `host_features`. Only zynthian's
  handlers are registered, so the host features are exactly what zynthian implements. No undo.
- `ZynthianModel`: live views over zynthian's objects (`ChainTrack`, `ChainSend`, `ZctrlParameter`;
  phase 2 `DeviceChain`, `ProcessorDevice`, `DeviceParameter`, `ZynTransport`).

Reasons: the reusable part is the protocol logic which the conformance work got right; copying
would fork it. A `SimDaw`-compatible fake would need about 30 classes of stubs and break with
every new collector in `dbm_host`. zynthian's state must be read live (GUI, MIDI learn,
snapshots); a mirror would need a two-way sync. The change in `dbm_host`: `Bridge.reporter_class`
and `Bridge.command_handler_class` (defaults unchanged; its 81 tests pass).

### 3.2 Mapping

| Core area | zynthian | Phase | What the core sees for the gaps |
| --- | --- | --- | --- |
| Tracks `/track/<p>` | chains without chain 0 and mixbuses, display order; with `hasFullFlatTrackList` (the LCXL) then the mixbuses and the master | 1 ✓ | no `active`, `depth`, groups; colors from `LAUNCHER_COLOUR` by MIDI channel, gray for audio chains |
| Volume, pan, mute, solo, recarm | strip zctrls `level`, `balance`, `mute`, `solo`, `record`; texts `x.x dB`, pan percent | 1 ✓ | a MIDI chain without strip: values 0, its commands ignored |
| Selection | `chain_manager.active_chain` / `set_active_chain_by_id` | 1 ✓ | no auto-arm (Bitwig arms the selected instrument track) |
| VU | `getDpm` dBFS −60..0 → 0..1 | 1 ✓ | no hold values |
| Effect tracks, sends | mixbus chains; `send_<id>_level` | 1 ✓ | `send/<s>/active` not handled (no switch) |
| Master | chain 0, main mixbus | 1 ✓ | – |
| Paging | all items reported, `bank` stores the offset | 1 ✓ | – |
| Undo | `/project/canUndo 0`, `/project/canRedo 0` (zynthian has no undo history) | 1 ✓ | `undo`/`redo` not handled |
| Host features | `valueInc valueReset immediately soloToggle`, phase 2 `transportCommands deviceCommands` | 1 ✓, 2 ✓ | no `indication`: the core sends none |
| Devices `/device/…` | the active chain's processors without the mixer strip; cursor = `chain.current_processor`; page of 8 = two screens of 4; page name = the first screen's; `parameterPage p` → screen `2p`; values linear or by index (ticks/labels), texts `get_value2label`; bypass; sibling window | 2 ✓ (adapter, tests) | no window/expand/pin/remove/duplicate/move (`window`/`expand` accepted and ignored); no `/primary`, layers or drum pads (`hasDrumPads 0`) |
| Transport, tempo | `getTransportState` (`/play`), `transportStart(6)` / `transportStop(255)`, `getTempo` / `zynseq.set_tempo`, ±0.01/±1, `tapTempo`, `getBpb`, metronome mode `TRANSPORT`/`OFF`, metronome volume | 2 ✓ (adapter, tests, desktop run) | no arranger: position, loop, punch, automation, overdub not reported; record (the audio recorder) open |
| Scenes, slots | zynseq phrases = scenes; sequences per chain MIDI channel = slots | 3 | none reported: while Solo is held the LCXL pages scenes |
| Note inputs | `NoteInput.translate` → `lib_zyncore.ui_send_note_on/off` on the active chain's channel (`ZynthianModel.note_sink`) | 3 | – |
| Step grid `/clip/…` | the selected sequence's pattern | 3 | – |
| Browser, markers, groove, application | – | – | not reported; their commands logged once |

## 4. Phases

### Phase 1: the Launch Control XL on the mixer — done

| Step | State |
| --- | --- |
| 1.1 `Bridge` factory hooks in `dbm_host` | done |
| 1.2 `ZynthianModel`, `ZynReporter`, `ZynCommandHandler`, `ZynBridge` | done |
| 1.3 `zynthian_ctrldev_base_dbm` (thread, queue, state per snapshot, sleep) | done |
| 1.4 `zynthian_ctrldev_dbm_launchcontrol_xl` (port 1, not HUI) | done |
| 1.5 PipeWire uids and the `dev_in_2_dev_out` fix in zynautoconnect | done, checked with the device |
| 1.6 Unit tests (§6 a) | done |
| 1.7 Conformance of `track_mix.toml` against the adapter (§6 b) | done: the 6 mixer scenarios compare the same, with listed model differences |
| 1.8 Desktop mode with the device (§6 c) | done, with the limits below |

Acceptance criteria and what was met:

| Criterion | Result |
| --- | --- |
| The tests pass (`run_tests.py`), the dbm tests pass | met: 41 and 81 |
| The driver autoloads for the Launch Control XL, not for HUI | met (desktop: zmip 7 = `Launch Control XL IN 1`, output zmop 7) |
| The template LEDs light | the core's template selection and 49 LED messages went to the device and the device answered the template selection (runs 1–3). That the LEDs are lit was not seen (nobody looked at the device) |
| Fader 1 moves the first chain's level | met for the message as the device sends it (`B8 4D 20`, injected into the driver): level and zynmixer 0.252. A physical fader was not moved |
| Mute/solo/arm toggle and light; a mute in zynthian lights the button | a mute set in zynthian sent `98 4A 3F` to the device; the buttons: unit tests and conformance only |
| Track paging, Track Focus selects the chain | unit tests and conformance only |
| No core `ERROR`, unhandled only later phases | met (desktop and tests) |
| Unload gives the device back; power save off/on | sleep/wake restarts the controller (desktop: template selection resent and answered); routing restore and LEDs off not seen |

Not verified on the device: anything that needs a person at the device (looking at the LEDs,
moving faders, pressing buttons). In the later runs PipeWire's JACK MIDI path stopped delivering
to the device (analysis §2; a plain JACK client got no answer either, `amidi` did), so those runs
injected the device's answer.

Risks (remaining): the zynthian GUI's timer threads raise at exit (§9 Z11, harmless); thread
safety of zctrl changes from the tick thread; Python cost on the Duet (phase 4).

### Phase 2: devices and parameters, transport — started

Scope: the cursor device (the active chain's current processor), siblings, 8-parameter pages from
zynthian's 4-parameter screens, bypass; zynseq's transport (play, stop, restart, tempo, tap,
metronome); `deviceCommands` and `transportCommands`.

Done: `dbm_zynthian/device.py`, `dbm_zynthian/transport.py`, the collectors and handlers,
`getTransportState` exported from `libzynseq` (with a fallback to `g_nTransportState` for older
libraries), `test_devices_transport.py` (9 tests: the page mapping, reports, commands, the cursor
following the active chain, the transport, the fallback, a failing collector, and the LCXL with
the core: Device on → the third knob row turns the parameters, Device held + Track Control 2 →
page 2 = screens 3 and 4, Record Arm held + Track Focus 4 → tempo +1). Desktop mode: tempo +1 from
the LCXL's transport row reached the real zynseq (120 → 121), `transport play`/`stop` drove its
transport.

Open:

- `/primary` (the first synth of the active chain) for the controllers that ask for
  `FIRST_INSTRUMENT`.
- Record = the audio recorder (`audio_recorder.toggle_recording()`, `SS_AUDIO_RECORDER_STATE`).
- Logarithmic zctrls: normalized linearly now; their own scale would match the GUI's knob feel.
- `SS_PROCESSOR_CTRL_SCREENS` → dump (the screens change when a processor changes).
- The conformance of `equalizer.toml` and of `lcxl.mix.device-page`, `lcxl.mix.transport-*`
  (needs processors and zynseq in the conformance fixture).
- The desktop check of the devices needs a chain with an engine (not done: the run had only mixer
  chains).

Acceptance: as before (the LCXL's Device mode turns the active processor's parameters, pages page
through the screens, the transport functions of the Track Mix are handled); met in the tests, the
device part not yet on the desktop.

Risks: zctrl kinds (logarithmic, ticks, integer, labels, paths); the transport "stopping" state
(`/play` stays 1 until the transport has stopped).

### Phase 3: Launchpad and zynseq

Scope: scenes and slots on zynseq, launch/stop, the note inputs into the chains, the step grid; a
Launchpad (X or Mini MK3 on port 2) driver module, conflicts with the stock drivers on port 1.

Files: `dbm_zynthian/launcher.py` (phrases/sequences), `dbm_zynthian/clip.py` (pattern grid),
`note_sink` in the driver base (`ui_send_*`), `zynthian_ctrldev_dbm_launchpad_x.py`.

Acceptance: session view launches and stops sequences with the right LED states; scene launch
starts the phrase; the note view plays the active chain; the step sequencer edits the selected
pattern; the fitting Launchpad conformance scenarios pass against the adapter; the LCXL's Solo-held
scene paging matches Bitwig's.

Risks: launch is a toggle; one pattern per sequence; note latency (then a midiproc); two drivers on
two ports of one Launchpad.

### Phase 4: packaging and the device

Scope: the drivenbymoss package with the drivers (§5), the generator, settings, a test on the
Duet. Acceptance: `pmbootstrap build` for aarch64 and x86_64 without network; on the Duet the LCXL
works as in phase 1; tick thread CPU measured (`nura/tools/thread_cpu.sh`) [target < 5 % of a core
with 16 chains, not measured].

## 5. Packaging for Nura

- **Native library.** `cargo build --release --frozen --no-default-features` natively in
  pmbootstrap's chroot (aarch64 under qemu, x86_64); a cross build would be
  `--target aarch64-unknown-linux-musl` / `x86_64-unknown-linux-musl`.
  - **Without Lua at first:** the `lua` feature builds Lua 5.4 from C (mlua `vendored`), which
    needs a C compiler for the target (natively `gcc`; cross: a musl cross compiler and
    `CC_aarch64_unknown_linux_musl`). The Lua controllers are not needed on Zynthian yet.
  - **No network in pmbootstrap**: the crates (without Lua only cbindgen and its dependencies) as
    a `cargo vendor` tarball in `source` or the distfiles cache, `.cargo/config.toml` pointing to it.
- **Package `drivenbymoss`** (decision 5: the drivers ship in it):
  - `/usr/lib/drivenbymoss/libdrivenbymoss.so` (the drivers' default `DBM_LIBRARY`);
  - `/usr/share/drivenbymoss/python/{drivenbymoss,dbm_host}/` (`compileall`), the drivers' default
    `DBM_PYTHON_DIR`;
  - **the drivers** in `/usr/share/zynthian/zynthian-ui/zyngine/ctrldev/`: `zynthian_ctrldev_base_dbm.py`,
    `zynthian_ctrldev_dbm_*.py`, `dbm_zynthian/` (without `tests/`). Why there and not in "the user
    driver location": on nura that is `$ZYNTHIAN_MY_DATA_DIR/ctrldev` in the user's home, which a
    package cannot fill; a system `zyngine/ctrldev_user/` would make the manager ignore the
    user's own folder (analysis §1). apk lets a package own files in a folder of another package.
    For ZynthianOS (no apk) an install script copies the same files to `zyngine/ctrldev_user/`.
  - `depends="python3 zynthian-ui"`, `makedepends="cargo"` (+ `gcc` with Lua),
    `license="LGPL-3.0-only AND GPL-2.0-or-later"` (the drivers are GPL), `arch="aarch64 x86_64"`.
- **Where the driver sources live for the package**: pmbootstrap's `--src` takes one checkout, so
  the package needs the driver files in the drivenbymoss repository or as a tarball of the
  zynthian-ui branch. Proposal: move `zynthian_ctrldev_base_dbm.py`, `zynthian_ctrldev_dbm_*.py`
  and `dbm_zynthian/` to `backends/zynthian/ctrldev/` of drivenbymoss-rust (GPL headers kept, as
  the Bitwig extension keeps its own); `zynstubs.UI_DIR` then comes from `$ZYNTHIAN_UI_DIR`. The
  zynthian fixes (zynautoconnect, `zynseq.h`) stay in the zynthian-ui branch and its package (open
  question 1, §8).
- `nura/deploy.sh drivenbymoss zynthian-ui` updates a device.

## 6. Testing

**(a) Unit tests without the UI** (41 tests): `python3 zyngine/ctrldev/dbm_zynthian/tests/run_tests.py`
from the zynthian-ui checkout (plain `unittest discover` cannot be used: it imports the real
`zyngine` package, all engines, before the stand-ins; zynthian-ui has no unit test suite, only
manual scripts such as `zynlibs/zynseq/test.py`).

- `zynstubs.py`: `zyngine` as a bare package, stand-ins for `zynautoconnect`, `zynlibs.zynseq`,
  `pyliblo3`, `zyngui.zynthian_gui_config` and a recording `lib_zyncore`; real
  `zynthian_controller` zctrls and the real signal manager; `ZynthianState` (chains, strips,
  mixbuses, sends, processors with screens, `is_busy`), `FakeZynseq`/`FakeLibseq`.
- `test_model.py` (17): mapping, texts, VU, reports, selection, removed chains, commands.
- `test_launchcontrol_xl.py` (6): the core with the LCXL on simulated time.
- `test_driver.py` (4): the driver class as the manager runs it (thread, MIDI, snapshot-only state,
  sleep/wake, end).
- `test_pipewire_ports.py` (4): the PipeWire uids with the real port names, `dev_out_uid`.
- `test_conformance_lcxl.py` (1): §6 b.
- `test_devices_transport.py` (9): phase 2.

**(b) Conformance against the adapter** (`tests/conformance.py`, done for `track_mix.toml`).
Reused (read only, from `$DBM_REPO` or the checkout next to the worktree): the scenario files and
device stubs, `conformance_runner` (`led_state`, `check_expectations`, `matches`, `parse_hex`), the
in-process runner's timing, the setups and profile settings, the Java references. zynthian's own: a
fixture (the fixture's 10 tracks and 2 effect tracks as chains), the track OSC steps, a state view
of the observer's bank of 8 tracks, a comparison of the complete LED state and the modelled state
keys per step, and a list of zynthian's known differences with reasons. All scenarios run in
order, only the phase's are judged. Result for phase 1: `track-focus`, `mute`, `solo`, `recarm`,
`knobs`, `track-page` compare the same; the differences are all model differences:

- Bitwig's **auto-arm** (the fixture's preference): selecting an instrument track arms it;
- Bitwig's **takeover mode "catch"** (the profile): a jumping knob is not taken, zynthian takes it;
- **Solo held** pages the scene bank (no scenes before phase 3);
- **Record Arm held**, Track Focus 5 = undo: Bitwig had an undo history, zynthian has none.

Not judged yet: `device-page`, `transport-click`, `transport-tempo`, `transport-defeat` (phase 2:
needs processors and zynseq in the fixture), the other LCXL files.

**(c) Desktop mode on a PC** (done 2026-11-06, the Tk UI from the worktree, `DBM_PYTHON_DIR` and
the library of the dbm checkout, `ZYNTHIAN_MY_DATA_DIR` on a copy, zynlibs built in the worktree):
a probe thread recorded the MIDI to and from the device, added three audio chains and a mixbus,
restarted the controller, set a mute in zynthian, injected a fader and the transport row's tempo+,
and exited zynthian through `zyngui.exit`. Results in §4 (phase 1 acceptance, phase 2).

## 7. State (2026-11-06)

zynthian-ui worktree (branch `dbm`, after `c7d384e2`, not committed): the PipeWire port names and
their tests updated to the real names; snapshot-only document settings; `/project/canUndo|canRedo`;
contained collector failures; `tests/conformance.py`, `test_conformance_lcxl.py`; phase 2
`device.py`, `transport.py`, `test_devices_transport.py`; `zynlibs/zynseq/zynseq.h` declares
`getTransportState`.

drivenbymoss-rust (after `1087aa7`, not committed): this plan, the analysis.

## 8. Open questions

1. Move the driver sources into the drivenbymoss repository (`backends/zynthian/ctrldev/`), so the
   drivenbymoss package can be built from one checkout (§5)?
2. A one-time migration of MIDI routes stored under the old nura uids (port names → `PW:` uids):
   not done; worth it only if nura snapshots with MIDI routes exist.
3. PipeWire's MIDI bridge stopped delivering to the Launch Control XL during the session (analysis
   §2); a restart of PipeWire (not done) would show whether it is a host problem.
4. Phase 2: report `/primary` for the controllers that ask for it, and map record to the audio
   recorder?
