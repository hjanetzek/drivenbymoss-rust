# DrivenByMoss on Zynthian: implementation plan

Status: 2026-11-07.
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
- **The clip launcher** (phase 3, §3.6): the slots' play states, launch without zynseq's toggle,
  track stop, stop all, clip remove/duplicate/paste, scene create/remove/duplicate.
- **The Novation Launchpads** (phase 3, §7b): drivers for the five models of the core on their
  MIDI ports, beside zynthian's own drivers on the DAW ports (the user picks, the DrivenByMoss
  driver then unloads the stock one); the Launchpad X and Mini MkIII with the core against the
  adapter (session, note, drum, sequencer, clip length, mixer, device views). **Phase 3 is done
  in the adapter**; no Launchpad was connected (§7b has the checklist).
- **The Native Instruments Maschine JAM** (through dbm-nihost, §7c): its driver (autoload on
  `Maschine Jam - 1 IN 1`), the touch strips on the mixer and the parameters, the session grid on
  zynseq's launcher, the play and step modes; all five JAM conformance files judged against the
  adapter (with a clock in the stand-in zynseq); a desktop run with the JAM connected (input
  injected). Not yet checked by a person at the device.

126 tests. The analysis behind it: [ZYNTHIAN_INTEGRATION.md](ZYNTHIAN_INTEGRATION.md) ("analysis §n").

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
| `zynthian_ctrldev_dbm_<controller>.py` | One class per controller, the module's name: `dev_ids`, `dbm_controller_id`, `driver_name`, `driver_description`, `autoload_flag`, `replaces_dev_ids` (§7b). Now: `launchcontrol_xl`, `maschine_mk2`, `maschine_jam`, `launchpad_x`, `launchpad_mini_mk3`, `launchpad_pro_mk3`, `launchpad_pro`, `launchpad_mk2` |
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
- **Launchpads** (from the code, not seen on hardware): §7b.

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
| Scenes, slots | the phrases of zynseq's current scene; slot `s` of a MIDI chain = sequence (phrase `s`, its channel); the play states, launch, stop, remove/duplicate/paste, scene create/remove/duplicate: §3.6 | ✓ | no slot/scene names and colors (the chain's color, scenes gray); an empty slot's launch starts the sequence (no stop button); no restart on relaunch, no alternative quantization (`launchAlt` = `launch`) |
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
| `track <p>/clip/<s>/select`, `launch` | select; start if stopped (the launch semantics: §3.6) |
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
| `/clip/start`, `loopStart` | 0 (only 0 is accepted; a range with another start, e.g. from the Launchpad's Clip Length view, is not applied, also not its `loopLength` and `end`, until the next report) |
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

### 3.6 The clip launcher

zynseq's launcher (`dbm_zynthian/launcher.py`; tests `backends/zynthian/tests/test_launcher.py`).
zynseq's own launch is a toggle (`togglePlayState`); the core's commands are mapped to explicit
`setPlayState` calls with Bitwig's meaning (the reference host's `Slot.launch`, `Track.stop`):

| Core | zynseq |
| --- | --- |
| `/track/<p>/slot/<s>/playing` | `getPlayState` PLAYING, STOPPING, STOPPING_SYNC (a queued stop still plays, as Bitwig's) |
| `…/playQueued`, `…/stopQueued` | STARTING; STOPPING, STOPPING_SYNC (`recQueued` always 0: MIDI record starts at once) |
| `track <p>/clip/<s>/launch`, `launchAlt` | stopped (STOPPED, FORCED_STOP) → STARTING; a queued stop → PLAYING; playing or starting: unchanged (zynseq's toggle would stop it; Bitwig's relaunch keeps it playing) |
| `…/launchRelease`, `launchReleaseAlt` | nothing (no gate launch mode) |
| `track <p>/stop` | the chain's sequences in all phrases: PLAYING → STOPPING_SYNC (as zynseq's toggle), STARTING → STOPPED |
| `track <p>/stopAlt` | the same at once (STOPPED) |
| `scene stop`, `stopAlt` (stop all clips) | the same for every phrase launcher (PLAYING/CHILD_PLAYING → CHILD_STOPPING, its sequences stop at their end) and all 32 sequences of every phrase |
| `track <p>/clip/<s>/remove` | stop at once, `clearPattern` (the pattern stays with its length: zynseq has one per sequence) |
| `track <p>/clip/<s>/duplicate`, `clip duplicate` (cursor slot) | `copyPattern` into the slot below, if there is one and the slot has notes (the reference host's `duplicateClip`) |
| `track <p>/clip/<s>/paste <track>/<slot>` | `copyPattern` from the source slot |
| `scene <s>/launch`, `launchAlt` | select the phrase; its launcher (channel 32): stopped or only some sequences playing (CHILD_PLAYING/CHILD_STOPPING) → STARTING, a queued stop → PLAYING, playing/starting unchanged |
| `scene <s>/remove` | `zynseq.remove_phrase` (refuses the last phrase; libseq stops all sequences first, analysis §9 Z14) |
| `scene <s>/duplicate` | `zynseq.duplicate_phrase` (zynseq inserts the stopped copy *before* the phrase: the playing original moves one down) |
| `project createScene` | `zynseq.insert_phrase` at the end, selected |
| `project createSceneFromPlayingLauncherClips` | a new phrase at the end, the first playing pattern of each MIDI channel copied into it, selected |

Decided against Bitwig: launching an **empty** slot starts the (empty) sequence instead of
stopping the track (Bitwig's stop button). Every zynseq slot is a sequence with a pattern, and the
core sends `launch` right after `createClip` and `record` (`create_note_clip`,
`record_note_clip`), on the still empty pattern; Bitwig's empty clip counts as content there.

Fixed on the way: zynseq's toggle stops with STOPPING_SYNC (5), which the reporter did not know
(a slot toggled off in zynthian's GUI showed neither playing nor stopping).

Not handled (logged once): slot `color`, `insertFile`, scene `name`/`color` (zynseq has phrase
names, not mapped yet), `track <p>/returnToArrangement`. Not mapped: the restart of a playing
sequence on launch (zynseq has no queued restart), the alternative launch/stop quantization
(zynseq's start/end mode is per sequence, `setSequenceMode`), clippy's audio launchers as slots
(stop all stops them).

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
LCXL's note and drum sequencers, conformance and a desktop check (§6, §7). The clip launcher
(§3.6): the play states (playing, play/stop queued, recording), launch without zynseq's toggle,
track stop, stop all, clip remove/duplicate/paste, scene launch/create/remove/duplicate, with
tests of the adapter and of the Mk2's PATTERN and SCENE modes with the core
(`test_launcher.py`); the Mk2 conformance (`mk2.pattern.select`, `mk2.scene.select`) unchanged.
The JAM's files are judged since the JAM work (§7c): the LEDs that differed throughout were the
runner's (it dropped the startup output, which the JAM's files keep: the strips' setup), the
stand-in zynseq got a clock.

**Done in the adapter: the Launchpad** (§7b): driver modules for the X, Mini MkIII, Pro Mk3,
Pro and MkII; the X and the Mini MkIII with the core against the adapter (`test_launchpad.py`):
the session view launches and stops with the LED states, the scene buttons start phrases, the note
view plays the active chain, the note and drum sequencers edit the pattern, the clip length view,
the mixer and device views. Fixed on the way: a clip range not starting at 0 (§3.5).

Open: the Launchpad and the launcher on the real devices (the hand checks of §7 and §7b), the gaps
of §3.6 and §7b, two core bugs found with the Launchpad (§7b: the blinking pads' base color (fixed), a
core error from the Shift view's mode buttons).

Acceptance: session view launches and stops sequences with the right LED states (met, apart from
the core's blink color, §7b); scene launch starts the phrase (met); the note view plays the active
chain (met); the step sequencer edits the selected pattern (met); the fitting Launchpad
conformance scenarios pass against the adapter: **skipped**, `tests/conformance` has no Launchpad
scenarios yet (no Bitwig reference recorded for a Launchpad).

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

**(a) The driver tests** (126), from this repository, after `cargo build`:

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
| `test_conformance_lcxl.py` | 10: §6 b (the LCXL's track mix, note and drum sequencers; the Mk2; the JAM's five files) |
| `test_maschine_jam.py` | 12: the JAM driver's dev_id (dbm-nihost's PipeWire port, the output; per port its driver: the JAM's only on the JAM, the Mk2's not there); the JAM with the core (the volume strips show and move the faders, the meters while playing, pan, sends, parameters; the session launch at the next bar, PLAY lit, MUTE + pad stops, scenes, group buttons; the note mode into the active chain, the note sequencer on the pattern) |
| `test_clip.py` | 13: the cursor clip on a zynseq pattern (properties, steps both ways, resolutions that don't divide, one note per zynseq step, the attributes zynseq has and those it doesn't, set/update/moveY, rows, columns, length, duplicate, transpose, clear, the playhead, the pattern editor's selection kept, the note selection, zynseq's Python state); the Mk2's STEP mode with the core (a pad writes the pattern, a note added in zynseq lights its pad, the playhead, the step off) |
| `test_launchpad.py` | 21: the Launchpad drivers' dev_ids (from the core's discovery names, not the stock drivers' ports, PipeWire), autoload against the stock drivers, the stock driver of the same device unloaded; the X and the Mini MkIII with the core (session launch, stop and LEDs, scenes, the note view through the driver, sequencer, drum view, clip length, mixer, device view); 2 expected failures: the core's blink color (§7b) |
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
- `maschine_jam/*.toml` (phase 3): §7c. For them the runner keeps the startup output in the LED
  state (`keep_startup_output`, as the Bitwig runner), and the stand-in zynseq has a clock
  (`FakeLibseq.clock`: a launch starts the transport, at each bar the starting sequences play and
  the stopping ones stop, the playheads run). The state view has `playingNotes` (compared for the
  track zynthian has selected: Bitwig also reports the notes of other armed and monitored tracks).
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
**verified by the user on the device** (2026-11-06). The rest of the launcher (§3.6), tested with
the core (`test_launcher.py`, not yet on the device): in PATTERN mode a pad launches (a second
press does not stop: green-dim while starting, green while playing), PATTERN + pad stops the
track (Shift: at once), ERASE + pad clears the slot, DUPLICATE + pad copies it into the slot
below, SELECT + pad selects; in SCENE mode a pad starts the phrase, SCENE + pad stops all clips,
DUPLICATE/ERASE + pad duplicate/remove the phrase.

**Checklist for PATTERN mode** (a synth chain with notes in the pattern of phrase 1, zynthian's
launcher (mixer) visible):

1. PATTERN, pad 1: the sequence starts at the next sync point (pad dim green, then green;
   zynthian's pad shows it playing). Pad 1 again: it keeps playing.
2. PATTERN held + pad 1: it stops at the next sync point (zynthian's pad shows "stopping"), the
   Mk2's pad stays green until it stops. Before it stops, pad 1 again takes the stop back.
   SHIFT + PATTERN + pad: it stops at once.
3. DUPLICATE + pad 1: pad 2 gets the track color, zynthian's slot 2 has the notes (pattern
   editor). ERASE + pad 2 (playing or not): it stops, the slot is empty (unlit).
4. SCENE, pad 2: phrase 2 starts (zynthian's phrase launcher); SCENE held + any pad: all
   sequences stop.
5. SCENE mode, DUPLICATE + pad 1: a new phrase (zynseq inserts the copy before phrase 1);
   ERASE + the last pad: it is removed (zynseq stops everything first; the last phrase stays).
6. Toggle a sequence in zynthian's launcher: the Mk2's pad follows (start, stop queued, stop).

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

## 7b. The Novation Launchpads

**Why:** the Launchpads are the core's grid controllers: the session view on zynseq's launcher,
the note, drum and sequencer views on the active chain and its pattern, mixer and device views.
zynthian has its own drivers for four of them (a launcher view on the DAW port); DrivenByMoss
runs the device from its MIDI port in programmer mode.

**Drivers** (`backends/zynthian/ctrldev/zynthian_ctrldev_dbm_launchpad_*.py`, written by hand,
the same pattern as the LCXL's; the generator of §2 is phase 4):

| Module | Core model (UUID) | dev_id (the core's port) | zynthian's driver (its port) | Autoload |
| --- | --- | --- | --- | --- |
| `launchpad_x` | Launchpad X (`CD196CCF-…`) | `Launchpad X IN 2` (port 1 "LPX MIDI") | `zynthian_ctrldev_launchpad_x`, `Launchpad X IN 1` (DAW) | no |
| `launchpad_mini_mk3` | Launchpad Mini MkIII (`A17B269D-…`) | `Launchpad Mini MK3 IN 2` (port 1 "LPMiniMK3 MIDI") | `zynthian_ctrldev_launchpad_mini_mk3`, `Launchpad Mini MK3 IN 1` (DAW) | no |
| `launchpad_pro_mk3` | Launchpad Pro Mk3 (`4EDED44C-…`) | `Launchpad Pro MK3 IN 1` (port 0 "LPProMK3 MIDI") | `zynthian_ctrldev_launchpad_pro_mk3`, `Launchpad Pro MK3 IN 3` (DAW) | no |
| `launchpad_pro` | Launchpad Pro, 2015 (`80B63970-…`) | `Launchpad Pro IN 2` (port 1, Standalone) | `zynthian_ctrldev_launchpad_pro_mk2`, `Launchpad Pro IN 1` (Live) | no |
| `launchpad_mk2` | Launchpad MkII (`4E01A0B0-…`) | `Launchpad MK2 IN 1` (its only port) | none (`launchpad_mini` is the Mini MK1, `Launchpad Mini IN 1`) | yes |

**The port names: confirmed vs. inferred** (no Launchpad was connected):

- Confirmed by code: the card names `Launchpad X`, `Launchpad Mini MK3`, `Launchpad Pro MK3` and
  `Launchpad Pro` and the ports of zynthian's drivers (their `dev_ids`); the core's Linux input
  names `Launchpad X MIDI 2`, `Launchpad Mini MK3 MIDI 2`, `Launchpad Pro MK3 MIDI 1`, `Launchpad
  Pro MIDI 2`, `Launchpad MK2 MIDI 1` (`src/controller/launchpad/definition.rs`) and with them, by
  the rule of §2.1 (`<card> MIDI <n>` → `<card> IN <n>`), the dev_ids above. Both fit Novation's
  port order (X, Mini MK3: DAW, MIDI; Pro MK3: MIDI, DIN, DAW; Pro: Live, Standalone, MIDI).
- Inferred: that ALSA's port order is that order on Linux (seen only for the LCXL); the MkII's card
  name `Launchpad MK2` (only from the core's names); under PipeWire that the bridge's client is the
  card name and its ports come in ALSA's order (seen for the LCXL), so the `dbm` branch gives the
  same dev_ids; the ports' PipeWire names ("LPX MIDI Out" etc., not needed for the dev_id). On
  ZynthianOS (jackd) the dev_ids come from `/proc/asound/cardN/midi0` and the port index: the same.
  The tests check the dev_ids against the core's names, zynthian's drivers and
  `zynthian_pipewire_ports.py` with assumed port names.

**Which driver wins, and how the user picks.** Both drivers can't run one device: zynthian's puts
it into DAW mode and draws its launcher on the DAW port, DrivenByMoss switches it to programmer
mode and draws on the MIDI port. Decided:

1. **zynthian's driver wins by default**: it autoloads on its port (the stock drivers keep
   zynthian's `autoload_flag = True`); the DrivenByMoss drivers of these four models do not
   autoload (`autoload_flag = False`). The MkII has no stock driver and autoloads.
2. **The user picks DrivenByMoss** in zynthian's driver selection per device: the MIDI device
   options of the MIDI port (`Launchpad X IN 2`) list "DrivenByMoss Launchpad X"; loading it
   **unloads zynthian's driver on the DAW port of the same device** (`replaces_dev_ids`, the same
   device = the same uid before "/", so a second Launchpad X keeps its driver), as "disabled"
   (`unload_driver(izmip, True)`), before the core starts. The ZS3/snapshot stores both (per uid:
   `ctrldev_driver` on IN 2, `disable_ctrldev` on IN 1), so the choice survives a restart.
3. **Back to zynthian's driver**: unload DrivenByMoss on the MIDI port, then load zynthian's driver
   on the DAW port in its options. The other way round is not watched: loading zynthian's driver
   while DrivenByMoss runs gives two drivers on one device.

**What the views need from the adapter** (run with the core: `test_launchpad.py`, and probes
through all views of the X; nothing new was missing apart from the clip range):

| View | Adapter | State |
| --- | --- | --- |
| Session | §3.6: slots launch (not toggle), play states → LEDs (stopped: the chain's color; queued: flashing; playing: pulsing), scene buttons → phrases, stop through the Mix view's stop row or the Stop Clip mode | ✓ |
| Play, Piano, Drum 64, Chords | §3.4: note input `80????`/`90????`, the key tables of the views, the driver plays at once | ✓ (tested: Play through the driver; Piano, Drum 64 key tables probed; Chords not run) |
| Sequencer, Poly Seq., Raindrops, Drum, Drum 4/8 | §3.5: `grid/8x128`, `grid/32x128` … on the cursor slot's pattern | ✓ (tested: Sequencer, Drum; the others activated without errors). The Drum view shows 32 steps: at 1/16 its lower two rows are beats 4–8, beyond a 4-beat pattern, and are not stored (zynseq has nothing outside the pattern) |
| Clip Length | `loopLength`/`end` from the first pad; a range from another pad is not applied (**fixed**: its `end` became the pattern's length, e.g. 228 beats) | ✓ |
| Mix, Volume, Pan, Sends | the mixer (§3.2): stop, mute, solo, rec arm, volume, pan, sends | ✓ (tested: the Mix view's stop, mute, solo, rec arm; the fader views use the commands the LCXL uses, not run) |
| Device | the cursor device's 8 parameters as faders, bypass; window/expand/page section accepted | ✓; pinning not handled (logged) |
| User (Device twice) | Bitwig's project/track parameters | – zynthian has none: the faders do nothing |
| Drum pads' colors | `/drum` | – no drum machine (§3.2): the pads play, unlit as "no pad" |
| Note repeat (Shift) | `noterepeat …` | – logged (as the Mk2, §9 5) |

**Core bugs found:**

1. **The blinking pads lost their color** (X, Mini MkIII, Pro Mk3; the Pro and MkII similarly):
   **fixed** (2026-11-06). Java's `LaunchpadPadGrid` collects both the color and the blink color
   of a pad for the LED sysex; the core collected only the blink color (`src/framework/grid.rs`
   `send_state`, `BlinkMode::Collect`), so the sysex set the base color to 0: a queued slot
   flashed between black and GREEN_LO, a playing one pulsed black. Now the base color is
   collected too: as in Java (the Launchpad's session view ignores the clip color for play and
   record), a queued slot flashes between GREEN and GREEN_LO and a playing one pulses GREEN.
   Tests: `collected_blinks_keep_the_color` (core), `test_queued_and_playing_pads_blink_in_green`.
2. **The Shift view's mode buttons** (Shift tapped, then a scene button: Volume … Stop Clip, Mute,
   Solo, Rec Arm) give "Feature group is already in use while (de-)activating it." (also with the
   reference host's SimDaw); the mode is set, but the tests use the Mix view for stopping.

**Not covered:** Launchpad conformance (there are no Launchpad scenarios in `tests/conformance`);
the Pro, Pro Mk3 and MkII with the core (only their matching: their buttons differ); the device
itself.

**Checklist for the device** (zynthian started with the run command, a Launchpad X or Mini MK3,
a chain with a synth selected, a pattern with notes in phrase 1, zynthian's launcher visible):

1. Plug it in: zynthian's own driver loads on `… IN 1` (its launcher on the pads), DrivenByMoss
   does not load on `… IN 2`.
2. MIDI device options of `… IN 2`: load "DrivenByMoss Launchpad X": the pads change to the Play
   view (programmer mode), zynthian's driver on `… IN 1` is unloaded (unchecked). Save the
   snapshot, restart zynthian: the same again without picking.
3. Play view: the pads play the synth, light while held; change the chain while holding a pad: no
   hanging note; the latency as played.
4. Session (the Session button): the top-left pad is chain 1, phrase 1, in the chain's color;
   press it: it flashes green, then pulses green while playing;
   again: it keeps playing; zynthian's launcher shows the same. Scene buttons: the phrase starts.
5. Shift + Session (Mix): the fifth row stops a chain at the next sync point (the pad flashes
   until then); the mute, solo and rec arm rows change zynthian's mixer.
6. Note, then the pad in row 4, column 1 (Sequencer): the top rows toggle notes in zynthian's
   pattern editor; the playhead runs while it plays. Note, row 6, column 1 (Drum): the 4×4 pads
   play drum notes, the upper half edits their steps.
7. Note, row 7, column 1 (Clip Length): the first pad, or the first and a later one held, set
   the pattern's length in zynthian; a range from another pad changes nothing.
8. Device: the 8 columns move the synth's parameters (zynthian's control screen follows).
9. Unload DrivenByMoss in the options: the device leaves programmer mode; load zynthian's driver
   on `… IN 1` again.

## 7c. The Native Instruments Maschine JAM

**Why:** the JAM is the core's grid controller with touch strips: the 8×8 pads are the session
(zynseq's launcher), the play and drum modes and the step sequencers; the 8 touch strips the
mixer (volume, pan, sends), the selected track and the device parameters; the encoder master
volume, tempo and the rest of its encoder modes. As the Mk2 it is a HID device and needs
dbm-nihost.

**The port** (seen on this host, `aconnect -l`, `pw-link -i/-o`, 2026-11-07): dbm-nihost's ALSA
client "Maschine Jam - 1" (pid of the service) with the ports "Maschine Jam - 1 Input" (0) and
"Maschine Jam - 1 Output" (1), NI's macOS names and the core's Linux discovery names; PipeWire
shows `Midi-Bridge:Maschine Jam - 1: Input (capture)` / `…: Output (playback)`. The `dbm` branch's
uids give `PW:Maschine Jam - 1/Maschine Jam - 1 IN 1`, so the dev_id is **`Maschine Jam - 1 IN 1`**
(output `OUT 1`). dbm-nihost bridges one JAM ("- 1"; `support/linux/ni_host/src/daemon.cpp`), so
the driver has this one dev_id. Beside it is dbm-nihost's Mk2 client (`Maschine MK2 IN 1`): the
JAM's driver does not take it, the Mk2's driver does not take the JAM's (`test_maschine_jam.py`,
and seen in desktop mode). On ZynthianOS (jackd) the sequencer client reaches JACK through
a2jmidid (`a2j:Maschine Jam - 1 [129] (capture): Maschine Jam - 1 Input`), whose ports give no
dev_id (as for the Mk2, analysis §1) [guess, not tested; dbm-nihost is not packaged there].

**Driver:** `backends/zynthian/ctrldev/zynthian_ctrldev_dbm_maschine_jam.py` (UUID
`72F32A83-A697-446B-9016-64AA3F9476E4`, autoload: zynthian has no JAM driver). Nothing
JAM-specific in the driver base: the JAM's Shift is a sysex (`F0 00 21 09 15 00 4D 50 00 01 4D
<0|1> F7`), which reaches `midi_event` whole; its pads are a note input (`80`, `90` on channel 1,
§3.4), its strips and buttons CCs on channel 1, kept from the chains (`unroute_from_chains`).

**What the modes need from the adapter** (probed with the core against the stand-in, and in
desktop mode; no adapter change was needed):

| JAM | Adapter | State |
| --- | --- | --- |
| Session (SONG): pads launch, MUTE + pad stops, SELECT + pad selects, CLEAR/DUPLICATE + pad, scene buttons, flip, scrolling | §3.6 | ✓ |
| Play, chords, piano, drum 64 (PAD MODE + scene 1–4) | §3.4: the note input into the active chain | ✓ |
| Note and drum sequencers (STEP + scene) | §3.5 on the cursor slot's pattern | ✓ |
| LEVEL (volume / pan), AUX (sends), MACRO (the selected chain: volume, pan, sends) | the mixer (§3.2) | ✓ |
| CONTROL (device parameters), PAGE LEFT/RIGHT | the cursor device (§3.2) | ✓; the user parameters (CONTROL again) are Bitwig's: none |
| PLAY, RECORD, Shift + PAGE LEFT (metronome), TEMPO + encoder, the encoder's master volume | transport (§3.2), recording (§3.3) | ✓ |
| Group buttons A–H, with MUTE/SOLO/REC | the chains | ✓ |
| SWING + encoder (`groove amount`), GRP + encoder (`beats position`), GRID (`quantize`), CLEAR/DUPLICATE + group button (`track <n>/remove`, `duplicate`), AUTO, NOTE REPEAT, loop, punch-in | – | not handled (logged once): zynthian has no global groove (zynseq's swing is per pattern), no song position, no automation, arpeggiator, loop or punch; removing chains from the controller is left to a decision (§9) |

**The touch strips** (what the core sends, `src/controller/maschine/jam/{surface.rs,mode/mod.rs}`):
per strip a CC 8+i with its value and, when the type or a color changes, one setup sysex
(`F0 00 21 09 15 00 4D 50 00 01 05` + type and color of all 8 strips) and for dual strips one
sysex `… 01 04` with the second values. Types: single (volume, sends, parameters), pan (centered),
dual (volume while the transport plays: the meter in the strip, the fader as the dual value), dot
(no item: off). The master meter goes to CC 64/65. The adapter's values for them:

- volume: the strip zctrl `level` (0..1) itself, the position of zynthian's fader (0.8 → 102,
  0.6 → 76); the strip sets it the same way (32 → 0.252);
- pan: `balance` −1..1 → 0..1 (center 64; 100 → 0.575);
- sends: `send_<id>_level`; the selected-track mode: volume, pan, then the sends (no mixbus: off);
- parameters: the zctrls of the page (value by range or by index), the strips' colors are the
  core's fixed parameter colors; an empty parameter slot is a single strip at 0;
- the meters: `getDpm` dBFS −60..0 → 0..1 (`dbfs_to_vu`), per chain and of the main chain
  (CC 64/65). Seen in desktop mode with the DX10 playing a chord: strip 1 rose 71 → 99, the master
  67 → 97; silent: 0.

**Conformance** (`zyn_conformance.py maschine_jam <file> 3`): all five files, 36 scenarios judged,
all the same as Java on Bitwig apart from model differences. `note_sequencer.toml` only differs
in the first scene button. Two fixes in the runner (not the adapter) made them comparable: it
dropped the startup output, which the JAM's files keep (`keep_startup_output`: the strips' setup
is only sent at the start; without it every strip LED differed), and the stand-in zynseq had no
clock (a launched slot stayed STARTING, the transport stopped, so the volume strips stayed single
where Bitwig's are dual). The differences, with their reasons:

| Difference | Reason |
| --- | --- |
| AUTO (`B0 28`) | zynthian has no automation (on in Bitwig's fixture) |
| Scene 1 bright (`B0 50`) | zynthian always has a selected phrase |
| Group A with REC held, the empty slots of Synth dim red (`90 24`…`90 54` = 05) | Bitwig's auto-arm (Synth armed) |
| Group B bright after SELECT + pad of Drums | zynthian's selected slot is the active chain's in the selected phrase: selecting the slot selects the chain; Bitwig keeps a selected slot per track |
| Slot 1 of Drums unlit (flipped grid) | Bitwig's empty clip counts as content, an empty zynseq pattern does not |
| The 64 drum pads (drum 64, release) | Bitwig's Drum Machine reports its pads; zynthian has no drum machine |
| `/conformance/primary/drumpad/2/selected` | no drum machine: no pad to select (the core's sequencer still selects its pad) |

Not judged (not in zynthian): `jam.global.auto`, `shift-auto`, `note-repeat`,
`jam.transport.loop`, `punch-in`. The touch strip scenario `jam.strip.level` and the strips in all
judged steps compare without difference.

**Desktop run** (2026-11-07, the Tk UI, `ZYNTHIAN_CTRLDEV_PATH`, my-data on a copy, the JAM
connected through dbm-nihost; a probe recorded the MIDI to the device and injected the input as
dbm-nihost sends it; an MDA DX10 chain on channel 1 and an audio chain): the driver autoloaded on
zmip 6 `Maschine Jam - 1 IN 1`, output zmop 6 `Midi-Bridge:Maschine Jam - 1: Output (playback)`;
the Mk2 driver stayed on `Maschine MK2 IN 1`. At the start the core sent 324 messages (259 CCs,
64 pad notes, the strip setup sysex); no sysex query, the device sent nothing by itself. Seen:

- the strips as above (volume, pan, sends off without mixbus, the selected chain, the parameters:
  strip 5 moved the DX10's Attack to 90/127 and its CC showed 90);
- SONG: the DX10's slot unlit while empty, lit in the chain's color (45) after notes were added
  in zynseq; the pad launched it (transport stopped: it played at once, zynseq started the
  transport; pad 2D → 1E green, PLAY lit), the volume strips turned dual with the meter; MUTE +
  pad: STOPPING_SYNC (5) for 0.4 s, then stopped at the bar, the pad back to 45; scene 2: phrase
  2 selected and its launcher playing, the scene LEDs followed;
- PAD MODE + scene 1, pad 36: `ui_send_note_on(0, 36, 100)` 30 µs after `midi_event`, the pad lit
  while held;
- STEP + scene 1, pad 36: a note at step 0 in the pattern of the selected phrase (zynseq's
  pattern 17), again: removed;
- TEMPO + encoder: 120 → 121 BPM; Shift + PAGE LEFT: metronome on, again off;
- no core errors; unhandled only `noterepeat …`. zynthian stopped its jalv processes at the exit.

**Not verified (needs a person at the device):** that the JAM shows the strips as sent (types,
colors, the levels, the meters while playing), the pad and button LEDs, physical strips (touch and
slide), pads, encoder and buttons through dbm-nihost, the sound and the latency as played.

**Checklist for the device** (zynthian started with the run command, dbm-nihost running, the JAM
connected, a chain with a synth (e.g. MDA DX10) and a second chain, zynthian's mixer visible):

1. Plug in / start: the driver loads on `Maschine Jam - 1 IN 1` (MIDI device options), not on the
   Mk2's port; the JAM's LEDs and strips come on.
2. LEVEL (volume): one strip per chain in the chain's color, lit to the fader's level; slide a
   strip: zynthian's fader follows; move zynthian's fader: the strip follows.
3. LEVEL again (pan): the strips centered; slide: the chain's balance moves.
4. AUX with a mixbus: the strips are the sends of the first mixbus; MACRO: volume, pan, sends of
   the selected chain.
5. CONTROL: the synth's parameters on the strips (zynthian's control screen follows); PAGE
   LEFT/RIGHT: the next/previous device of the chain.
6. SONG: column 1 is chain 1; a slot with notes is lit in the chain's color; press it: it starts
   (green), zynthian's launcher shows it playing, PLAY lights; the volume strips show the meter
   and the fader's level (dual strips); MUTE + pad: it stops at the bar.
7. Scene buttons: the phrase starts (zynthian's phrase launcher); SELECT + pad: the chain and
   phrase are selected.
8. PAD MODE + scene 1 (note): the pads play the synth and light while held; no hanging note when
   the chain changes while a pad is held (group buttons A–H select the chains).
9. STEP + scene 1 (note sequencer): pads toggle notes in zynthian's pattern editor; the playhead
   runs while it plays. STEP + scene 4 (drum sequencer): a drum pad selects the note, the step pads toggle its steps.
10. TEMPO + encoder: zynthian's tempo; the encoder alone: the main chain's volume; Shift + PAGE
    LEFT: the metronome.

## 8. State (2026-11-07)

drivenbymoss-rust (after `476b2ac`, not committed): the Maschine JAM driver
(`zynthian_ctrldev_dbm_maschine_jam.py`), `test_maschine_jam.py`, the JAM's conformance files in
`zyn_conformance.py` (phase 3, known differences, `keep_startup_output`, the zynseq clock,
`playingNotes`) and `test_conformance_lcxl.py`, the clock in `zynstubs.FakeLibseq`, this plan
(status, §2, §4 phase 3, §6, §7c, §9), the README. No adapter change was needed for the JAM. Open
are the device checks (§7, §7b, §7c) and the Shift view core bug of §7b.

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
7. The Launchpads: should DrivenByMoss win by default (autoload, unloading zynthian's driver), or
   stay the user's choice as now (§7b)? And should a DrivenByMoss driver notice zynthian's driver
   being loaded on the DAW port later?
8. The Shift view's mode buttons (§7b): fix in the core. (The blink color is fixed, 476b2ac.)
9. The JAM (§7c): should CLEAR/DUPLICATE + group button remove or duplicate the chain (zynthian's
   `remove_chain`; there is no chain clone)? Should SWING map to the swing of the cursor slot's
   pattern (zynseq's swing is per pattern, Bitwig's groove global)? The ZynthianOS dev_id (6)
   applies to the JAM as well.

## 10. To investigate

Found in use; documented here, not fixed yet.

1. **Loop range selection shortens the pattern and deletes notes** (found by the user on the
   JAM, 2026-11-07; the Mk2, LCXL and Launchpad sequencers send the same).
   - In STEP mode, the top row of pads selects the shown page, and a long press selects a loop
     range. With a long press on the first pad of a 2-bar pattern, the core sends the loop start
     (0), the loop length (1 bar) and the play range (`src/framework/view/sequencer/
     abstract_note_sequencer_view.rs:188-190`, `abstract_drum_view.rs:437-439`,
     `clip_length_view.rs:85-87`).
   - The adapter maps the loop length (and `end`) to the pattern length
     (`dbm_zynthian/commands.py` `_clip`, `clip.py` `set_length`). zynseq has no loop range
     inside a pattern, and `Pattern::setBeatsInPattern` deletes every note beyond the new length
     (`zynlibs/zynseq/pattern.cpp:696-706`). So the notes of the second bar are gone; in Bitwig
     the loop is a region inside the clip and the notes beyond it stay.
   - Loop ranges which do not start at the first page are not applied at all (a zynseq pattern
     starts at 0, §3.5), so loop ranges in general don't work.
   - Options:
     - (a) Never shorten when it would cut notes: no data loss, but the loop is not followed.
     - (b) A loop region in zynseq: play start and end inside a pattern, an upstream-able
       feature. Faithful, but a zynthian change.
     - (c) Map the loop to something else zynseq has: the sequence, or several patterns in a
       sequence.
     - (d) Keep the cut notes in the adapter and restore them when the length grows again:
       fragile.
     - A quick safety net (a) was tried and reverted; it needs a decision together with (b)/(c).
2. **The loop length cannot be increased** (found by the user on the JAM, 2026-11-07; in Bitwig
   it can).
   - The core's gesture (`abstract_note_sequencer_view.rs:164-192`, the drum views the same): a
     single long-pressed page pad sets the loop to that page; holding one page pad and pressing
     a second sets the loop between them, also beyond the current end. So increasing means: hold
     the first pad, press a later one. The core then sends `loopStart`, `loopLength`, `start`
     and `end`.
   - The adapter can grow a pattern (`loopLength`/`end` → `Clip.set_length`, zynseq adds empty
     beats), so where it fails is not known yet. To check:
     - whether the page pads beyond the pattern's end are offered at all (the core's page count
       comes from the reported loop length; Bitwig lets a page beyond the end be chosen);
     - the order and values of the commands, e.g. a non-zero `start` or `loopStart` making the
       adapter refuse the whole range (§3.5), or `end` and `loopLength` disagreeing;
     - the rounding to zynseq's whole beats (a JAM page is 8 steps of the chosen resolution).
   - Belongs together with item 1: both come from mapping Bitwig's loop region onto zynseq's
     pattern length.

