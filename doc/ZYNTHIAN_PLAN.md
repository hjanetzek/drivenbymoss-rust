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
- **The Native Instruments Maschine Mk2** (through dbm-nihost, §7): its driver, the note input
  (pads into the active chain, §3.4), the displays and paging, the conformance of its scenarios,
  a desktop run with the device; confirmed by the user on the device.
- **The step grid** (phase 3, pulled forward, §3.5): the cursor clip and its step grids on the
  zynseq pattern of the selected sequence, the Mk2's STEP mode and the LCXL's sequencers.

72 tests. The analysis behind it: [ZYNTHIAN_INTEGRATION.md](ZYNTHIAN_INTEGRATION.md) ("analysis §n").

**Goal.** zynthian controller drivers (ctrldev) host the DrivenByMoss core, so that the core's
controllers drive zynthian: the core runs the controller (modes, LEDs, displays), zynthian is its
DAW. First the Novation Launch Control XL.

**Decisions** (the user's answers of 2026-11-06):

1. The PipeWire uids (`PW:<client>/<client> IN <n>`) are accepted; MIDI routes stored under the old
   uids are lost (a migration is optional, §9).
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
| `zynthian_ctrldev_dbm_<controller>.py` | One class per controller, the module's name: `dev_ids`, `dbm_controller_id`, `driver_name`, `driver_description`, `autoload_flag`. Now: `launchcontrol_xl`, `maschine_mk2` |
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
| Note inputs | §3.4 | ✓ (the Mk2's pads) | – |
| Cursor clip, step grids `/clip/…` | §3.5: the pattern of the cursor slot | ✓ | no shuffle, accent, pinning; pan, gain, repeat … of the steps |
| Pinning, loop | `/track/pinned -1`, `/device/pinned 0`, `/repeat 0` (zynthian has neither) | ✓ | without them the core lights a Pin button and never writes the loop LED |
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

### 3.4 Note inputs

The core creates note inputs during `dbm_start_infrastructure` (`set_note_input_filters`, the
Mk2: one with `80`, `90`, the notes on channel 1) and changes their key and velocity tables with
every view or octave change (the pads in play, keyboard, chords and drum mode). Implemented in the
driver base (`zynthian_ctrldev_base_dbm.py`):

- `midi_event` (the state manager's fast thread) translates a note through the note inputs
  (`ZynBridge.translate_note`: the filters, the key table, −1 blocks, the velocity table) and
  sends it at once with `lib_zyncore.ui_send_note_on/off` on the active chain's MIDI channel
  (`ZMIP_FAKE_UI`, which reaches every chain on that channel, analysis §2). The message still goes
  through the queue to the core (for its LEDs). No `dbm_*` call: the tables are plain Python data
  the core's callbacks replace.
- A note-off goes to the channel and key of its note-on, also if the active chain or the tables
  changed meanwhile; stopping the controller (sleep, unload) ends the sounding notes.
- The core's own notes (`process_midi_arg`, the further notes of a chord) take the same way, on
  the tick thread.
- The sounding notes are reported as the cursor track's `/track/<p>/playingnotes`, so the pads
  light while they are held (as Bitwig's note input monitoring).
- No note without an active MIDI chain (an audio chain has no channel).

Latency (analysis §2): dbm-nihost + 0–10 ms fast-thread poll + 35–45 µs in the driver (measured in
desktop mode) + 0–5.3 ms JACK period, so up to ~15 ms on top of dbm-nihost, 5–8 ms on average;
waiting for the tick would add up to 30 ms. A real-time midiproc remains the option for less.

### 3.5 The cursor clip and the step grids

`dbm_zynthian/clip.py`. The cursor clip is the pattern (track 0, position 0) of the cursor slot
(the active chain's sequence in the selected phrase, §3.3), the clip of zynthian's pattern
editor. `ZynseqClip` is a `dbm_host.clip.Clip` whose notes are read from that pattern and whose
changes go to it, so the reference host's `NoteGrid` (one per size of the model setup's
`noteClips`: the steps in the core's format, toggle, set, setFrom, update, clear, rows, columns,
moveY, the attributes, step size, scrolling, paging) and `CommandHandler.grid_command` work on it
unchanged. The notes are read once per collection and per command.

| Core | zynseq |
| --- | --- |
| `/clip/exists` | the cursor slot has a pattern (a MIDI chain) |
| `/clip/start`, `loopStart` | 0 (only 0 is accepted) |
| `/clip/end`, `loopLength` (+ commands, `/inc`) | `getBeatsInPattern` / `setBeatsInPattern` (whole beats, at least 1) |
| `/clip/loop` | 1 (a pattern loops) |
| `/clip/shuffle`, `/clip/pinned` | 0 (`shuffle`, `accent`, `pinned`, `name`, `color`, `duplicate` not handled) |
| `/clip/color` | the chain's color |
| `/clip/grid/<id>/playingStep` | `getPatternPlayhead` of the playing sequence, in the grid's steps, relative to its scroll; −1 when it does not play |
| `/clip/grid/<id>/step/<ch>/<x>/<y>` | the events of the pattern; an emptied step once `0` |
| `clip clear`, `grid/<id>/clearAll` | `clearNotes` |
| `clip duplicateContent` | twice the beats, the notes again after the old length |
| `clip transpose` | `transpose` |

**Time.** A note at `start` beats is the event at `position = floor(start · S)`, `offset = start ·
S − position` (S = the pattern's steps per beat), its duration `duration · S` steps. So every
resolution the core's views set maps (1/3 beat on a 4-step pattern: position 1, offset 0.333).
The pattern's resolution stays the user's (the adapter never calls `setStepsPerBeat`, analysis §9
Z13). Limit: zynseq keeps one note per key and step, so with a core step finer than zynseq's a key
holds one note per zynseq step; a second one replaces the first.

**Attributes.** Mapped: velocity (7 bits), duration, chance (`play_chance`, always enabled), muted
(`play_freq` 0/1). Not in zynseq: velocity spread, release velocity, pressure, timbre, pan,
transpose, gain, occurrence, recurrence, repeat. They are reported with the values of a new
Bitwig note, and setting them changes nothing, so the core's edit views show those values again.
zynseq has no note channel: the notes are reported on the channel the core last addressed (0 at
first). The note selection of the core's step editing is kept by the adapter.

**zynthian's side.** The adapter selects the cursor pattern for each access and the previous one
again (the pattern editor's selection is kept); it never calls `isPatternModified` (read and
cleared by the pattern editor, which redraws on its own after the core's edits); it refreshes
zynseq's Python state (the launcher GUI) at most every 0.5 s after a change. Race: the pattern
editor (GUI thread) and the tick thread both select patterns; between the adapter's select and
restore the GUI could read the wrong pattern [not observed].

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

Done (pulled forward): the scenes and slots with select, launch, record, createClip (§3.3), the
note inputs (§3.4), the cursor clip and the step grids (§3.5) with the Mk2's STEP mode and the
LCXL's note and drum sequencers, conformance and a desktop check (§6, §7).

Open: the rest of the launcher (stopping, deleting and duplicating clips and scenes, the play
states as the Launchpad shows them) and a Launchpad (X or Mini MK3 on port 2) driver module.

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

**(a) The driver tests** (72), from this repository, after `cargo build`:

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
| `test_driver.py` | 9: the LCXL driver class (thread, MIDI, snapshot-only state, sleep/wake, end); the Mk2 driver with the core (its note input, pads into the active chain at once, note-off to the note-on's chain, no hanging notes, no note without a MIDI chain, the cost of a note) |
| `test_pipewire_ports.py` | 2: the drivers' dev_ids against zynthian's PipeWire uids (the LCXL's controls port, not HUI; dbm-nihost's Mk2 port, not the JAM's or the Mk2's own MIDI port) |
| `test_conformance_lcxl.py` | 5: §6 b (the LCXL's track mix, note and drum sequencers; the Mk2) |
| `test_clip.py` | 13: the cursor clip on a zynseq pattern (properties, steps both ways, resolutions that don't divide, one note per zynseq step, the attributes zynseq has and those it doesn't, set/update/moveY, rows, columns, length, duplicate, transpose, clear, the playhead, the pattern editor's selection kept, the note selection, zynseq's Python state); the Mk2's STEP mode with the core (a pad writes the pattern, a note added in zynseq lights its pad, the playhead, the step off) |
| `test_devices_transport.py` | 20: devices, `/primary`, transport, recording, scenes, the Mk2's reports (pinning, loop, slot colors, playing notes), a failing collector, the LCXL with the core (Device mode, pages, Layer Mix without layers, new clip, launcher overdub, tempo) |

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
- `maschine_mk2/global.toml`, `pad_modes.toml`: §7.
- Phase 3 (`… 3`): `note_sequencer.toml` (5 scenarios: the first instrument's pages, create and
  delete a note the same as Java; the repeat count and the panning of a step are zynseq's
  missing attributes), `drum_sequencer.toml` (6: steps on and off the same; the knobs of a step
  compare in velocity, duration and chance, within one 7-bit step; the drum pads' mute and paging
  are the missing drum machine). The fixture has empty 4-beat patterns where Bitwig's has its two
  empty clips; Bitwig's empty clip counts as content, a zynseq pattern only with notes (the slot's
  `hasContent`, the PATTERN pad of the Mk2).
- Not judged: the rest of `note_sequencer.toml` (step grid, phase 3), `layer_mix.toml` (zynthian
  has no layers: the core shows nothing where Bitwig shows the Instrument Layer's 2 layers,
  §3.2), `drum_sequencer.toml`, `equalizer.toml`.

**(c) Desktop mode** (the run command above; my runs used a probe wrapper with
`ZYNTHIAN_MY_DATA_DIR` on a copy): the driver loads from `ZYNTHIAN_CTRLDEV_PATH`, with
`zynthian_desktop.sh` itself as well; the phase 1–2 checks and the recording (§3.3) against the
real zynmixer and zynseq.

## 7. The Native Instruments Maschine Mk2

**Why:** the Mk2's two displays show the device's parameter names and values, the page and device
names, with paging over parameter pages, devices and track pages; its 16 pads play.

**The port** (analysis §1): dbm-nihost's ALSA client "Maschine MK2" (`Maschine MK2 In`/`Out`,
PipeWire `Midi-Bridge:Maschine MK2: In (capture)`/`Out (playback)`), dev_id `Maschine MK2 IN 1`.
Not the JAM's (`Maschine Jam - 1 IN 1`) nor the Mk2's own USB MIDI port (`Maschine Controller MK2
IN 1`, its MIDI mode). dbm-nihost (a systemd user service, `--virmidi`) must run; the driver sends
the core's display text (MCU sysex) and button labels (`F0 7D 44 42 4D 10`) to it and gets the
Shift button from it (`F0 7D 44 42 4D 11/12`). On ZynthianOS the a2j port names give no dev_id
[guess, not tested].

**Driver:** `backends/zynthian/ctrldev/zynthian_ctrldev_dbm_maschine_mk2.py` (UUID
`015E4908-C049-4808-A645-A7A44BEFE36B`, autoload).

**What the displays show** (analysis §4; checked in desktop mode with the MIDI to dbm-nihost
decoded): the track modes the chain names and the volume in zynthian's dB (`-1.9`), the
parameters mode the 8 parameters of the cursor device (names from the zctrls, values from their
labels), page and device names in the notifications, the labels of the 8 buttons above. The Mk2
does not use `/primary`.

**Paging** (checked in desktop mode): in the parameters mode the encoder up/down = the parameter
pages (two of zynthian's screens per page: "Page: params 1"), PAGE LEFT/RIGHT = the devices of the
chain ("Selected device: MDA DubDelay", then its parameters; back to the DX10 on its last page);
in the track modes the encoder up/down = the track bank page (tests only).

**Pads:** §3.4. In play, keyboard and chords mode the core's key table maps the pads to notes (the
chord's further notes come through `process_midi_arg`); in drum mode to the drum notes (36…).

**The clip launcher (PATTERN, SCENE)**: the sequences as slots, the phrases as scenes (§3.3);
**verified by the user on the device** (2026-11-06).

**STEP mode** (§3.5): in drum mode (PAD MODE) STEP shows the 16 steps of the selected drum cell
(the pad played last; without one the pads do nothing, the core's behaviour); a pad toggles its
step in the zynseq pattern of the selected chain's sequence, the pads light the steps and the
playhead while zynseq plays. In play mode STEP is the note sequencer on the same pattern. Paging
(steps, octaves, the resolution) is the core's own: it sets the grid's step size and scroll
position, the adapter maps them to zynseq's steps. Conformance: `mk2.drum.step-on`, `mute-step`
(MUTE + pad mutes the step: `play_freq` 0) and `step-off` the same as Java on Bitwig.

Desktop run with the device (2026-11-06; PAD MODE, pad 1, STEP and the pads injected as dbm-nihost
sends them, an MDA DX10 chain on channel 1, its pattern of 4 beats at 4 steps per beat): pad 13
wrote the note at zynseq step 12 and its pad LED went from 0 to 45; zynseq's Python state (the
launcher GUI) had the event; a note added in zynseq at step 13 lit pad 14 and was reported as
the step line; while the sequence played (120 BPM) the reported playhead ran 0…15 and wrapped,
with the pads' LEDs following; stopped: −1; pad 13 again removed the note. Not observed: the
pattern editor's redraw (it would have meant driving the Tk screen from the probe), the pads and
the sound on the device.

**Checklist for STEP mode** (a chain with a synth selected, its pattern open in zynthian's
pattern editor):

1. PAD MODE, play a pad (it selects the drum cell), STEP: the 16 pads show the steps of that
   note; press pads: the notes appear in zynthian's pattern editor, the pads light.
2. Add or delete a note in the pattern editor: the Mk2's pad follows.
3. Start the sequence (PATTERN mode, or zynthian's launcher): the playhead runs over the pads.
4. MUTE + pad: the step is muted (zynthian: the note does not play).
5. The resolution (the Mk2's step length setting) and paging: the notes land where zynthian's
   editor shows them; with a resolution finer than the pattern's, one note per key and step.
6. PLAY mode + STEP (the note sequencer): the same pattern, notes by pitch.

**Conformance** (`zyn_conformance.py maschine_mk2 global|pad_modes 2`): 13 scenarios of
`global.toml` and 6 of `pad_modes.toml` judged, all the same as Java on Bitwig apart from model
differences; `mk2.keyboard.play`, `mk2.chords.play`, `mk2.pattern.select` and
`mk2.variation.parameter` without any. The differences, with their reasons:

| Difference | Reason |
| --- | --- |
| AUTO, LOCK, RESTART LEDs | zynthian has no automation, arranger overdub (on in Bitwig's fixture) or loop: the LEDs are never written |
| REC lit after `mk2.global.shift-lock`, `/record 1` | in zynthian record and launcher overdub are the same MIDI record (§3.3) |
| The pads while Shift is held | the clip functions, lit only with a cursor clip; no `/clip` before phase 3 |
| Group button 1 with REC held, `/track/3/recarm` | Bitwig's auto-arm |
| The volume texts (`-5.3` vs `-10.0`) | zynthian's level is linear gain, Bitwig has its own curve |
| The parameter value texts | zynthian's zctrl labels (the names compare: the fixture's zctrls have Bitwig's remote-control names) |
| Time and Position empty in the tempo mode | no song position (zynseq knows only the beat in the bar) |
| The first pad's color in drum mode | Bitwig's Drum Machine reports its pads; zynthian has no drum machine |
| Scene 1 lit in scene mode | zynthian always has a selected phrase |

Not judged: automation (`mk2.global.auto`, `shift-auto`), the arranger overdub (`lock`), note
repeat (zynthian has no arpeggiator: `noterepeat …` stays unhandled), the loop, the step
sequencer (phase 3). Real gaps the scenarios showed and which are fixed: the Pin LED (pinning),
the loop LED (`/repeat`), `scene <s>/select|launch|launchRelease`, slot `launchRelease`, `drum
indicate`, empty slots without color.

**Desktop run** (2026-11-06, the Tk UI, `ZYNTHIAN_CTRLDEV_PATH`, my-data on a copy, a probe that
recorded the MIDI to and from the device): the driver autoloaded on `Maschine MK2 IN 1` (output
`OUT 1`), not on the JAM or the MIDI-mode port. At the start the core sent its Shift query and
dbm-nihost answered (`F0 7D 44 42 4D 11 01 F7`): the path to dbm-nihost works both ways. The
display sysex and the button labels went out (decoded above), 80 LED messages; Parameters, the
encoder and PAGE (injected as dbm-nihost sends them) changed the display as described; a pad
(injected) reached the DX10 chain's channel 1 after 35 µs. No core errors; unhandled only
`noterepeat …`.

**Not verified (needs a person at the device):** that dbm-nihost draws the texts readably on the
screens, the LEDs on the device, physical pads, encoder and buttons, the sound and the latency as
played.

**Checklist for the device** (zynthian started with the run command, dbm-nihost running, a chain
with a synth, e.g. MDA DX10, selected):

1. Screens: in Volume mode the chain names over the knobs and their dB below; the 8 labels above
   the displays (`Volume Pan Send- Send+ PinTrk TrkOn User Params`).
2. Knobs 1–8 in Volume mode move the chains' faders in zynthian's mixer; the values on the screen
   follow.
3. Params (button 8 above the display): the synth's parameter names and values; a knob turns a
   parameter in zynthian's control screen and its value changes on the Mk2.
4. Encoder up/down: "Page: …" and the next/previous pair of zynthian's screens.
5. PAGE LEFT/RIGHT: "Selected device: …" and the parameters of the next device of the chain.
6. The group buttons select the chains; MUTE/SOLO + group toggles them.
7. Pads (PAD MODE, KEYBOARD): the synth plays, the pads light while held; notes do not hang when
   the selected chain changes while a pad is held.
8. TEMPO + encoder changes zynthian's tempo; Shift (dbm-nihost's) + TAP toggles the metronome.

## 8. State (2026-11-06)

drivenbymoss-rust (after `ecb1d95`, not committed): the step grid (`dbm_zynthian/clip.py`, the
cursor clip in the model, reporter and commands, the stand-in libseq's pattern store,
`test_clip.py`, phase 3 of the conformance), this plan, the analysis.

zynthian-ui `dbm`: no changes since `bc396034`.

## 9. Open questions

1. A one-time migration of MIDI routes stored under the old nura uids: not done.
2. What `record` should mean besides zynseq's MIDI record: the audio recorder, the SMF recorder,
   or a setting (§3.3)?
3. `/eq` for the controllers that ask for it (the LCXL's Equalizer mode): an EQ plugin found by
   name, or nothing?
4. PipeWire's MIDI bridge did not deliver to the Launch Control XL in my later runs (analysis §2),
   while the user's test worked; a host problem of that moment?
5. The Mk2's note repeat: zynthian has no arpeggiator; implement one in the driver (from the
   core's `noterepeat` commands), or leave it?
6. ZynthianOS: a dev_id rule for a2j ports (dbm-nihost there), if the Mk2 should work on it.
