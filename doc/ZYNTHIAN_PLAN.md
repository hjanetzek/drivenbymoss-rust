# DrivenByMoss on Zynthian: implementation plan

Status: 2026-11-06.
- **Phase 1 done** (§4): the Launch Control XL on the mixer, tested in desktop mode with the
  device (also by the user: "everything seems to work").
- **Phase 2 done in the adapter** (§4): devices and parameters, the first instrument (`/primary`),
  zynseq's transport.
- **Recording** into zynseq's launcher slots, with the scenes and slots it needs (§3.3), taken
  from phase 3.
- **The driver sources moved to this repository** (`backends/zynthian/`); zynthian loads them
  with `ZYNTHIAN_CTRLDEV_PATH` (§2).

48 tests. The analysis behind it: [ZYNTHIAN_INTEGRATION.md](ZYNTHIAN_INTEGRATION.md) ("analysis §n").

**Goal.** zynthian controller drivers (ctrldev) host the DrivenByMoss core, so that the core's
controllers drive zynthian: the core runs the controller (modes, LEDs, displays), zynthian is its
DAW. First the Novation Launch Control XL.

**Decisions** (the user's answers of 2026-11-06):

1. The PipeWire uids (`PW:<client>/<client> IN <n>`) are accepted; MIDI routes stored under the old
   uids are lost (a migration is optional, §8).
2. The Launch Control XL's driver takes its first port (`Launch Contro`, `IN 1`), never HUI.
3. The core's document settings belong to the **snapshot**, not the ZS3 (§2.3).
4. "Play" is **zynseq's transport**; "record" is **MIDI record into zynseq** (§3.3).
5. The drivers ship in the **drivenbymoss package** (§5); their sources live in this repository,
   the zynthian-ui `dbm` branch only has the changes to zynthian itself.

**Run zynthian with the drivers of this checkout** (after `cargo build`):

```sh
ZYNTHIAN_CTRLDEV_PATH=<dbm>/backends/zynthian/ctrldev <zynthian-ui>/desktop/zynthian_desktop.sh
```

**Where the code lives.**

| Part | Repository | Path | License |
| --- | --- | --- | --- |
| Core, C ABI, binding, reference host | drivenbymoss-rust | `include/drivenbymoss.h`, `bindings/python/drivenbymoss/`, `bindings/python/dbm_host/` | LGPLv3 |
| Drivers, model adapter | drivenbymoss-rust | `backends/zynthian/ctrldev/`: `zynthian_ctrldev_base_dbm.py`, `zynthian_ctrldev_dbm_*.py`, `dbm_zynthian/` (`model`, `reporter`, `commands`, `bridge`, `device`, `transport`, `launcher`) | GPLv2+ (as zynthian, `backends/zynthian/LICENSE.txt`) |
| Tests of the drivers | drivenbymoss-rust | `backends/zynthian/tests/` (`run_tests.py`, `zynstubs.py`, `zyn_conformance.py`, `test_*.py`) | GPLv2+ |
| The changes to zynthian | zynthian-ui, branch `dbm` (from `nura`) | `zynautoconnect/zynthian_pipewire_ports.py` + `test_zynthian_pipewire_ports.py` and `zynthian_autoconnect.py` (PipeWire uids, §9 N1, Z1), `zyngine/zynthian_ctrldev_manager.py` (`ZYNTHIAN_CTRLDEV_PATH`), `zynlibs/zynseq/zynseq.h` (`getTransportState`, §9 Z10) | GPLv2+ |

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

### Files and how zynthian finds them

`backends/zynthian/ctrldev/` holds the files as they are installed into zynthian's
`zyngine/ctrldev/`:

| File | Content |
| --- | --- |
| `zynthian_ctrldev_base_dbm.py` | `zynthian_ctrldev_base_dbm(zynthian_ctrldev_base)`: lifecycle, tick thread, MIDI queue, state, sleep. Skipped by the manager (name prefix `zynthian_ctrldev_base`) |
| `zynthian_ctrldev_dbm_<controller>.py` | One class per controller, the module's name: `dev_ids`, `dbm_controller_id`, `driver_name`, `driver_description`, `autoload_flag` |
| `dbm_zynthian/__init__.py` | `import_dbm()`: finds `drivenbymoss` and `dbm_host` (`sys.path`, `$DBM_PYTHON_DIR`, this repository's `bindings/python`, `/usr/share/drivenbymoss/python`), defaults `$DBM_LIBRARY` |
| `dbm_zynthian/{model,reporter,commands,bridge,device,transport,launcher}.py` | The model adapter (§3) |
| `dbm_zynthian/generate.py` (phase 4) | Writes the per-controller modules from `dbm_list_controllers()` and a port table |

**Decided: `ZYNTHIAN_CTRLDEV_PATH`**, a small zynthian-ui change on the `dbm` branch
(`zyngine/zynthian_ctrldev_manager.py`, `setup_extra_ctrldev_dirs`): folders separated by `:`
which become part of the package `zyngine.ctrldev` and are scanned after `zyngine/ctrldev`. Why
this one:

- It matches the installed layout: the drivers import `zyngine.ctrldev.dbm_zynthian…` both in
  the checkout and installed in `zyngine/ctrldev/`; nothing is copied or linked.
- The user's driver folder (`$ZYNTHIAN_MY_DATA_DIR/ctrldev`) with a symlink would import the
  modules as `zyngine.ctrldev_user.*`, another package than installed, and needs a setup script
  and a link in the user's data.
- A package can use it too (its own folder instead of the zynthian-ui folder, §5).
- The run command stays one variable: the drivers find `bindings/python` and `target/debug` of
  their checkout themselves (`DBM_PYTHON_DIR` is no longer needed).

### 2.1 Matching (dev_id ↔ the core's discovery names)

- The manager matches `get_midi_in_devid(izmip)` exactly against `dev_ids` (analysis §1).
- The dev_id of a port is `<card> IN <n>` on ZynthianOS and, with
  `zynautoconnect/zynthian_pipewire_ports.py`, on nura's PipeWire too (`<n>` = 1-based index of
  the port among the device's ports of that direction, in ALSA's order).
- The core's `discovery.linux` input names → dev_ids: `<card>` and `<card> <ALSA name of port k>`
  → `<card> IN <k+1>`; `<card> MIDI <n>` → `<card> IN <n>`. The port index is not in the name, so
  the generator takes it from a table of each device's ports, filled from hardware.
- **Launch Control XL** (seen on hardware): ports "Launch Contro" (0) and "HUI" (1) → dev_id
  `Launch Control XL IN 1`; HUI (`IN 2`) gets no driver. Output: `OUT 1` via `dev_in_2_dev_out`.

### 2.2 The tick thread

- `init()` (after `unroute_from_chains`): `super().init()` registers the chain and snapshot
  signals (they call `refresh`), then starts the thread `dbm <driver name>`.
- The loop: wait on an `Event` with 30 ms timeout (`midi_event`, `refresh`, `set_state`, sleep and
  `end` wake it), then `step(now_ms)`: pending document settings, a requested dump, MIDI →
  `receive`, `ZynBridge.tick(now_ms)`, notifications → `SS_GUI_SHOW_MESSAGE`. Once per second it
  caches the document settings.
- `midi_event(ev)` (fast thread): queues `ev` if its first byte is a status byte (analysis §9 Z4),
  returns True always.
- `end()`: exit flag, wake, join (5 s); the thread closes the bridge (`dbm_destroy`).
- Time: `dbm_set_time` with the ms since the controller started; the tests run the same bridge on
  simulated time.
- A failing collector is logged once and skipped, the others still report; the slots of a track
  likewise (a missing library function must not hide the tracks).

### 2.3 Start, stop, sleep, state

| Event | Driver |
| --- | --- |
| load (manager) | `__init__` (model, queue), `init` (thread); the thread: `ZynBridge.start` = `dbm_create_controller` (config `$ZYNTHIAN_CONFIG_DIR/ctrldev/dbm`), `dbm_start_infrastructure`, model setup → banks, first report |
| unsupported controller / missing library | logged once, the driver stays loaded and inert |
| `sleep_on` / `sleep_off` | the controller is stopped / started again with its document settings |
| `get_state` (ZS3 or snapshot save) | `{"dbm_document": <cached formatted document settings>}` |
| `set_state` | applied only during a snapshot load (`state_manager.is_busy("load snapshot")`); a ZS3 recall leaves the settings (decision 3) |
| chains added/removed/moved, snapshot loaded | `refresh` → dump |
| unload (manager) | `end` |

## 3. The model adapter

### 3.1 Reuse of the reference host: decided

Subclasses of `dbm_host`'s infrastructure, no imitation of `SimDaw`: `ZynBridge(Bridge)`,
`ZynReporter(Reporter)` (change tracking, dumps, the report again after value commands),
`ZynCommandHandler(CommandHandler)` (parsing, dispatch, the unhandled log, host features of the
registered handlers), and live views over zynthian's objects (`ZynthianModel`: `ChainTrack`,
`ChainSend`, `ZctrlParameter`; `DeviceChain`, `ProcessorDevice`, `DeviceParameter`;
`ZynTransport`; `ZynLauncher`). Reasons as before: one copy of the protocol logic, no ~30 stub
classes, zynthian's state read live. The `dbm_host` change: `Bridge.reporter_class` and
`Bridge.command_handler_class`.

### 3.2 Mapping

| Core area | zynthian | State | What the core sees for the gaps |
| --- | --- | --- | --- |
| Tracks `/track/<p>` | chains without chain 0 and mixbuses, display order; with `hasFullFlatTrackList` (the LCXL) then the mixbuses and the master | ✓ | no `active`, `depth`, groups; colors from `LAUNCHER_COLOUR` by MIDI channel |
| Volume, pan, mute, solo, recarm | strip zctrls `level`, `balance`, `mute`, `solo`, `record` (the audio recorder's arm) | ✓ | a MIDI chain without strip: values 0, commands ignored |
| Selection | the active chain | ✓ | no auto-arm |
| VU | `getDpm` dBFS −60..0 → 0..1 | ✓ | no hold values |
| Effect tracks, sends | mixbus chains; `send_<id>_level` | ✓ | `send/<s>/active` not handled |
| Master | chain 0 | ✓ | – |
| Undo | `/project/canUndo 0`, `/project/canRedo 0` | ✓ | `undo`/`redo` not handled |
| Host features | `valueInc valueReset immediately soloToggle transportCommands deviceCommands` | ✓ | no `indication` |
| Cursor device `/device` | the active chain's processors without the mixer strip; cursor = `current_processor`; page of 8 = two screens of 4; values linear or by index (ticks/labels); texts `get_value2label`; bypass; sibling window | ✓ | no window/expand/pin/remove/duplicate/move |
| First instrument `/primary` (`deviceIds` `FIRST_INSTRUMENT`) | the active chain's first processor of type "MIDI Synth" (the synth engine): name, bypass, pages and parameters as the cursor device; commands `parameterPage`, `param/<i>/…`, `bypass`, `select` | ✓ | `hasDrumPads 0`, `hasLayers 0`, `layer/count 0`, the 128 pads reported absent (`pad/<i>/exists 0`): the core's layer and pad views find no item (unlit, no layer commands); Track Select next stays lit (the core keeps 128 pads); `primary pad/<n>/mute` is still sent by the Mute mode and logged once as not handled (analysis §4) |
| Transport, tempo | `getTransportState`, `transportStart(6)`/`transportStop(255)`, tempo, tap, bpb, metronome mode `TRANSPORT`/`OFF` and volume | ✓ | no position, loop, punch, automation |
| Recording | §3.3 | ✓ | – |
| Scenes, slots | the phrases of zynseq's current scene; slot `s` of a MIDI chain = sequence (phrase `s`, its channel) | reported, select/launch/record/createClip (§3.3) | scene launch, stop, clip delete/duplicate: phase 3 |
| Note inputs | `NoteInput.translate` → `lib_zyncore.ui_send_note_on/off` on the active chain's channel | phase 3 | – |
| Step grid `/clip/…` | the selected sequence's pattern | phase 3 | – |
| Browser, markers, groove, application, `/eq`, `/drum` | – | – | not reported; their commands logged once |

### 3.3 Recording

zynseq has one recording mode (analysis §0): `enableMidiRecord(true)` puts the notes and CCs of
its MIDI input into the selected pattern of the selected sequence, in real time while the
sequence plays, else step by step. No arranger or timeline recording. So recording maps to "record
into a launcher slot", as Bitwig records into a clip launcher slot (`dbm_zynthian/launcher.py`):

| Core command / report | zynseq |
| --- | --- |
| `track <p>/createClip/<s> <beats>` (LCXL: Record Arm + Track Focus 1 "new clip") | the slot's pattern, created and added if there is none, its length set |
| `track <p>/clip/<s>/record` | select the phrase, the sequence, its pattern; MIDI record on; start the sequence |
| `track <p>/clip/<s>/select`, `launch` | select; start if stopped |
| `record` (toggle) | MIDI record on/off into the cursor slot (the active chain in the selected phrase) |
| `transport overdub/launcher <0\|1>` (LCXL: Record Arm + Track Focus 2) | MIDI record on/off for the selected pattern |
| `transport overdub/arranger` | ignored (no arranger) |
| `/record`, `/overdub/launcher`, `/track/<p>/slot/<s>/recording` | MIDI record is on (and the slot is the selected sequence) |
| `/overdub/arranger` | 0 |

Checked in desktop mode with the real zynseq: `createClip` set the length of the slot's pattern,
`clip/0/record` switched MIDI record on and started the sequence, `record` switched it off, the
LCXL's launcher overdub on again; the reports followed.

**Open: the other recorders** (later, each a decision of what the core's record means):

- **The audio recorder** (`state_manager.audio_recorder`, `jack_capture`, armed per strip by the
  `record` zctrl, which `recarm` already maps): `/record` could mean "audio recording running",
  `record` → `audio_recorder.toggle_recording()`, state from `SS_AUDIO_RECORDER_STATE`. It
  records the mixer output to a file; no launcher slot.
- **The SMF recorder** (`state_manager.toggle_midi_record`, `libsmf`): records all MIDI to a
  file; could be `record` while zynseq's MIDI record stays on the slots.
- A setting (document or global) could choose what `record` means.

## 4. Phases

### Phase 1: the Launch Control XL on the mixer — done

All steps done (`dbm_host` hooks, model, reporter, commands, bridge, driver base, LCXL driver,
PipeWire uids, tests, conformance of `track_mix.toml`, desktop mode with the device). The
acceptance results are those of the previous state of this plan; since then the user tested it in
desktop mode with the device ("everything seems to work"). Not checked by me on the device:
looking at the LEDs, physical faders and buttons. In my later runs PipeWire's MIDI bridge did not
deliver to the device (analysis §2), so they injected the device's answer.

### Phase 2: devices and parameters, transport — done in the adapter

| Item | State |
| --- | --- |
| Cursor device, siblings, pages, bypass | done, tests; LCXL Device mode and pages with the core |
| `/primary` (first instrument) | done, tests; `lcxl.noteseq.device-page` compares the same with no differences; Layer Mix behaviour without layers tested (§3.2) |
| zynseq transport | done, tests, desktop (tempo +1 from the LCXL, play/stop) |
| `getTransportState` exported, fallback, masked to `uint8_t` | done |
| Conformance | all ten scenarios of `track_mix.toml` and `lcxl.noteseq.device-page` (§6 b) |
| Recording (from phase 3, §3.3) | done, tests, desktop |

Open: record = audio recorder or SMF recorder (§3.3); logarithmic zctrls (normalized linearly);
`SS_PROCESSOR_CTRL_SCREENS` → dump; a desktop check of the devices with a chain that has an engine;
`/eq` (the LCXL asks for it: zynthian has no general EQ; could be an EQ plugin by name).

Acceptance (met in tests; the device part of the devices not yet on the desktop): the LCXL's
Device mode turns the active processor's parameters, pages page through the screens, the
transport functions and the first instrument's pages are handled.

### Phase 3: Launchpad and zynseq

Scope: the rest of the launcher (scene launch, stop, clip delete/duplicate, the play states as the
Launchpad shows them), the note inputs into the chains, the step grid; a Launchpad (X or Mini MK3
on port 2) driver module. The scenes, slots, select/launch/record/createClip are in place (§3.3).

Acceptance: session view launches and stops sequences with the right LED states; scene launch
starts the phrase; the note view plays the active chain; the step sequencer edits the selected
pattern; the fitting Launchpad conformance scenarios pass against the adapter.

Risks: launch is a toggle in zynseq; one pattern per sequence; note latency (then a midiproc); two
drivers on two ports of one Launchpad.

### Phase 4: packaging and the device

The drivenbymoss package (§5), the generator, settings, a test on the Duet; acceptance as before
(`pmbootstrap build` for aarch64/x86_64 without network, the LCXL on the Duet, tick thread CPU).

## 5. Packaging for Nura

- **Native library.** `cargo build --release --frozen --no-default-features` natively in
  pmbootstrap's chroot (aarch64 under qemu, x86_64); without Lua at first (the `lua` feature
  builds Lua's C source: a C compiler for the target, a musl cross compiler when cross building).
  No network in pmbootstrap: the crates as a `cargo vendor` tarball or in the distfiles cache.
- **Package `drivenbymoss`**, built from this repository alone (the driver sources are here now):
  - `/usr/lib/drivenbymoss/libdrivenbymoss.so` (the drivers' default `DBM_LIBRARY`);
  - `/usr/share/drivenbymoss/python/{drivenbymoss,dbm_host}/` (the drivers' default
    `DBM_PYTHON_DIR`);
  - the drivers: `backends/zynthian/ctrldev/*` (without the tests) into
    `/usr/share/zynthian/zynthian-ui/zyngine/ctrldev/`. Not into the user's driver folder: on nura
    that is `$ZYNTHIAN_MY_DATA_DIR/ctrldev` in the home directory, which a package cannot fill, and
    a system `zyngine/ctrldev_user/` would hide it (analysis §1). Alternative with
    `ZYNTHIAN_CTRLDEV_PATH`: `/usr/share/drivenbymoss/zynthian/ctrldev/`, with zynthian's start
    script (`/usr/bin/zynthian`) reading the variable from a drop-in (a further zynthian-ui change;
    not needed while installing into `zyngine/ctrldev/` works);
  - `depends="python3 zynthian-ui"` (the `dbm` branch), `makedepends="cargo"`,
    `license="LGPL-3.0-only AND GPL-2.0-or-later"`, `arch="aarch64 x86_64"`.
  - ZynthianOS (no apk): an install script copies the drivers into `zyngine/ctrldev_user/`.
- `nura/deploy.sh drivenbymoss zynthian-ui` updates a device.

## 6. Testing

**(a) The driver tests** (48), from this repository, after `cargo build`:

```sh
python3 backends/zynthian/tests/run_tests.py [-v]
```

They need a zynthian-ui checkout of the `dbm` branch (`$ZYNTHIAN_UI_DIR`, else `../zynthian-dbm`
or `../zynthian-ui`; exit code 77 without one) and plain Python (`zynstubs.py` stands in for
JACK, zyncore, zynseq and the UI; zctrls and the signal manager are zynthian's). So they are not in
`ci/linux.sh`. zynthian-ui's own tests of the PipeWire names:
`python3 zynautoconnect/test_zynthian_pipewire_ports.py` (4).

| File | Tests |
| --- | --- |
| `test_model.py` | 17: mapping, texts, VU, reports, selection, removed chains, commands |
| `test_launchcontrol_xl.py` | 6: the core with the LCXL on simulated time |
| `test_driver.py` | 4: the driver class (thread, MIDI, snapshot-only state, sleep/wake, end) |
| `test_pipewire_ports.py` | 1: the driver's dev_id against zynthian's PipeWire uids |
| `test_conformance_lcxl.py` | 2: §6 b |
| `test_devices_transport.py` | 18: devices, `/primary`, transport, recording, a failing collector, the LCXL with the core (Device mode, pages, Layer Mix without layers, new clip, launcher overdub, tempo) |

**(b) Conformance against the adapter** (`zyn_conformance.py`):
`python3 backends/zynthian/tests/zyn_conformance.py launchcontrol_xl track_mix [1|2|all]`. It reuses
the scenario files, device stubs, `conformance_runner`, the in-process runner's timing, the setups,
profiles and Java references; zynthian's own are a fixture (the fixture's tracks, effect tracks,
devices with the same names and pages, 8 phrases), OSC steps (track addresses, `/click`,
`/tempo/raw`, `/device|primary/page/N/selected`), a state view and a list of zynthian's known
differences with reasons. Results (phase 2):

- `track_mix.toml`: all ten scenarios compare the same;
- `note_sequencer.toml`: `lcxl.noteseq.device-page` (the first instrument) without any difference;
- the known differences, all of the model: Bitwig's auto-arm; Bitwig's takeover mode "catch"
  (the knob values, carried into the later scenarios); with Solo held the scenes, where zynthian
  always has a selected phrase; Track Focus 5 (undo) with Record Arm held: zynthian has no undo.
- Not judged: the rest of `note_sequencer.toml` (step grid, phase 3), `layer_mix.toml` (zynthian
  has no layers: the core shows nothing where Bitwig shows the Instrument Layer's 2 layers,
  §3.2), `drum_sequencer.toml`, `equalizer.toml`.

**(c) Desktop mode** (the run command above; my runs used a probe wrapper with
`ZYNTHIAN_MY_DATA_DIR` on a copy): the driver loads from `ZYNTHIAN_CTRLDEV_PATH`, with
`zynthian_desktop.sh` itself as well; the phase 1–2 checks and the recording (§3.3) against the
real zynmixer and zynseq.

## 7. State (2026-11-06)

drivenbymoss-rust (after `ae8b9ed`, not committed): `backends/zynthian/` (moved from the
zynthian-ui branch; `README.md`, `LICENSE.txt`), `/primary`, `launcher.py` (scenes, slots,
recording), the conformance extended, this plan, the analysis.

zynthian-ui `dbm` (after `ad24433d`, not committed): the drivers and their tests removed;
`ZYNTHIAN_CTRLDEV_PATH` in `zynthian_ctrldev_manager.py`;
`zynautoconnect/test_zynthian_pipewire_ports.py`.

## 8. Open questions

1. A one-time migration of MIDI routes stored under the old nura uids: not done.
2. What `record` should mean besides zynseq's MIDI record: the audio recorder, the SMF recorder,
   or a setting (§3.3)?
3. `/eq` for the controllers that ask for it (the LCXL's Equalizer mode): an EQ plugin found by
   name, or nothing?
4. PipeWire's MIDI bridge did not deliver to the Launch Control XL in my later runs (analysis §2),
   while the user's test worked; a host problem of that moment?
