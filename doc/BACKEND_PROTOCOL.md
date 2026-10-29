# Backend protocol

The interface between the DrivenByMoss core (the Rust crate) and a DAW host (a "backend"). Until
now it was defined only by the C++ processors of the Reaper backend (`backends/reaper`); this
document writes it down as the host interface, with the Reaper backend as the reference.

As of 2026-10-29. Compiled from the code: Rust `src/ffi.rs`, `src/backend.rs`, `src/app.rs`,
`src/parser.rs`, `src/daw/`; host `backends/reaper/reaper_drivenbymoss/` (`dllmain.cpp`,
`OscParser.cpp`, `*Processor.*`, `DataCollector.cpp`, `Collectors.h`, `Model.cpp`, `Track.cpp`,
`Send.cpp`, `Parameter.cpp`, `Marker.cpp`). Points not confirmed from the code are marked (?).

## 1. Overview

The protocol has three parts, all over the C ABI (`include/drivenbymoss.h`, see
[BINDINGS_PLAN.md](BINDINGS_PLAN.md)):

| Direction | Mechanism | Content |
| --- | --- | --- |
| Core → host | `DbmCallbacks`: `process_*_arg(processor, command[, value])`, `delay_updates`, `enable_updates` | Commands: change the DAW state ([§2](#2-commands-core--host)) |
| Host → core | `dbm_update_model(app, text)` | Model updates: the DAW state as text lines ([§3](#3-model-updates-host--core)) |
| Both | `process_midi_arg`, `send_midi_data`, `dbm_on_midi_event`, the MIDI port callbacks | Controller MIDI; not covered here |

The core never asks for values: the host pushes changes, and on a `refresh` command the full
state. Commands are fire-and-forget; their effect comes back as model updates.

### API calls and call sequence

| Function (C / C++ wrapper) | Role |
|---|---|
| `dbm_version()` / `dbm::version` | static version string, not freed |
| `dbm_create(callbacks, ini_path, app_version, flags)` / `dbm::App(backend, iniPath, appVersion, flags)` | create instance; loads `DrivenByMoss4Reaper.config` (`CONTROLLER_INSTANCE<n>`) and per-controller configs. `flags`: `DBM_FLAG_HOST_TICK` (= 1) → no internal 30 ms timer thread. Returns null if `callbacks` null. `app_version` parsed to major.minor |
| `dbm_add_device(app, name, identifier)` / `addDevice` | register an installed plugin (device browser); currently only stored in `App::devices` (?) unused |
| `dbm_start_infrastructure(app)` / `startInfrastructure` | start all controller instances (open MIDI ports via callbacks), send `refresh`; start timer unless `DBM_FLAG_HOST_TICK`. Idempotent (`is_started`). Controller `startup` runs ~1 s later (`STARTUP_DELAY_MS`) inside `tick` |
| `dbm_update_model(app, data)` / `updateModel` | push model text (this document) |
| `dbm_tick(app)` / `tick` | run due tasks, delayed startup, `handle_changes`, `flush` (controller output); every 30 ms recommended |
| `dbm_restart_controllers(app)` / `restartControllers` | stop + start all instances, sends `refresh` |
| `dbm_set_default_document_settings(app)` / `setDefaultDocumentSettings` | reset document-scope settings (Reaper: on project load/new) |
| `dbm_get_formatted_document_settings(app)` / `getFormattedDocumentSettings` | settings text for the project file; free with `dbm_free_string` |
| `dbm_set_formatted_document_settings(app, data)` / `setFormattedDocumentSettings` | apply project-stored settings (kept pending for non-running instances) |
| `dbm_display_window`, `dbm_display_project_window`, `dbm_display_parameter_window` | no UI: all print the recent log to stderr |
| `dbm_free_string(text)` | release returned strings |
| `dbm_destroy(app)` / `~App` | stop timer, `shutdown` all instances, free. Not from within a callback |

Threading: all calls on one instance are serialized by a mutex; calling `dbm_*` from inside a callback deadlocks; panics are caught.

Required sequence (as used by Reaper, `dllmain.cpp` / `DrivenByMossSurface.cpp`):

1. `dbm_create(..., GetResourcePath(), GetAppVersion(), DBM_FLAG_HOST_TICK)`; check non-null (`isValid`).
2. (project load) `dbm_set_default_document_settings`, then `dbm_set_formatted_document_settings` per stored line — may happen before step 3.
3. First `Run` with `Audio_IsRunning() != 0`: `dbm_add_device` for every `EnumInstalledFX` entry, then `dbm_start_infrastructure` (core sends `refresh` → next update is a full dump).
4. Every `Run` (~30 Hz): incoming MIDI → `dbm_on_midi_event`; `dbm_tick`; drain outgoing MIDI; every 2nd `Run`: `dbm_update_model(CollectData(ShouldDump()))` if non-empty.
5. (project save) `dbm_get_formatted_document_settings` + `dbm_free_string`.
6. Shutdown: `dbm_destroy` (C++: `dbmApp.reset()`), then flush the final MIDI.

## 2. Commands (core → host)

### Rust side

| Layer | What it does |
|---|---|
| `DawContext::send / send_int / send_double / send_bool / send_str / send_strs` (`src/daw/context.rs`) | Shortcuts used by the model. All are dropped if the context is `muted` (`DawContext::muted()`). |
| `DawContext::invoke_action(id)` | `Sender::invoke_action` -> `process_int_arg("action", "", id)`. |
| `Sender` (`src/backend.rs`) | Maps `Processor` enum to its id string (`Processor::id()`), calls `Backend`. `bool_arg` = `int_arg` with 1/0. `refresh()` = `no_arg(Refresh, "")`. |
| `FfiBackend` (`src/ffi.rs`) | Converts all strings to NUL-terminated C strings (interior NULs removed) and calls the `DbmCallbacks` function pointer with `user` as first arg. A null pointer = call is silently ignored. |
| `Channel::send_cmd/send_int/send_bool/send_double/send_str` (`src/daw/channel.rs`) | Processor = `master` for `ChannelKind::Master`, else `track`. Command = `<position>/<cmd>` for tracks (and drum pads!), plain `<cmd>` for master. |

Callbacks and value types:

| Callback | Value | Rust helper |
|---|---|---|
| `process_no_arg(user, processor, command)` | none | `send`, `no_arg` |
| `process_int_arg(..., int value)` | `c_int` (booleans are 1/0) | `send_int`, `send_bool`, `invoke_action` |
| `process_double_arg(..., double value)` | `f64` | `send_double` |
| `process_string_arg(..., const char* value)` | UTF-8 string | `send_str` |
| `process_string_args(..., const char** values, int count)` | string list | `send_strs` (only `track addTrack`) |
| `delay_updates(user, processor)` | - | `DawContext::delay_updates` |
| `enable_updates(user, processor, int enable)` | 1/0 | `DawContext::enable_updates` |

Processor id strings (`Processor::id()`): `transport project track master device eq browser marker clip
session scene noterepeat playingnotes groove play stop record repeat time tempo automation action
quantize metro_vol undo redo cursor refresh inifile`.

### Host side (Reaper)

- `ReaperBackend::process*` (`dllmain.cpp`) -> `OscParser::Process(processor, command[, value])`.
- `OscParser::Process` does NOT execute immediately: it queues a lambda via `Model::AddFunction`.
  The surface loop (`DrivenByMossSurface.cpp`) runs `dbmApp->tick()`, then
  `functionExecutor.ExecuteFunctions()`, then `CollectData(...)`. So commands sent during a tick run
  right after that tick, before the next state collection. All on the main thread.
- Split: `OscParser::Split` splits `command` at `'/'` (via `getline`) into a `std::deque<std::string>`.
  Empty command `""` -> empty deque (0 parts). No leading `/` is used; a leading `/` would give an
  empty first part.
- Lookup: `processors.at(processor)`. Unknown processor -> `std::out_of_range` -> log
  `"No function <command> registered!"`, ignored. Registered names: `automation play stop record
  repeat time tempo action quantize metro_vol undo redo cursor project master track noterepeat
  device eq clip marker refresh scene groove inifile`.
  NOT registered (update-control names only): `transport`, `browser`, `session`, `playingnotes`.
- Overloads: `Process(path)`, `Process(path, const std::string&)`, `Process(path, const
  std::vector<std::string>&)`, `Process(path, int)`, `Process(path, double)`.
  Default `OscProcessor::Process(path, int)`: if `value == 1` calls `Process(path)` (no-arg), else
  nothing. Several processors override it and fall through to `Process(path, double)` for
  unknown int commands (track, master, noterepeat, clip, time).
- Index parts are parsed with `atoi` (non-number -> 0). Track index = index among *visible*
  tracks (`TrackProcessor::GetTrackIndex` skips tracks with `GetTrackState & 1024` = hidden),
  0-based. Device index in `device` commands is 1-based (`atoi(part) - 1`).
  Marker / scene index = index into `Marker::GetMarkers` / `Marker::GetRegions`, 0-based.
- Colors: string `RGB(r,g,b)` (0..255), parsed by regex `RGB\((\d+),(\d+),(\d+)\)`.
- Exceptions inside a processor are caught and logged.

### delay_updates / enable_updates (host: `DataCollector`)

| Function | Host effect |
|---|---|
| `enable_updates(p, enable)` | `disableUpdateMap[p] = !enable`. While disabled, `DataCollector::IsActive(p)` is false and the collector for `p` is skipped entirely (no data sent). Default: enabled. |
| `delay_updates(p)` | Stores now (ms) in `delayUpdateMap[p]`. For `DELAY` = 300 ms, `IsActive(p)` is false -> no updates for `p` (prevents Reaper sending old values before the new ones are applied). Entry removed after expiry. |

Collector names checked with `IsActive`: `project transport track device master browser marker clip
session noterepeat groove` (`DataCollector.cpp` ~l.67-87) and `playingnotes` (~l.406).
`eq` is never checked (EQ data is part of `device`).

Rust usage:

| Call | Processor | Rust file |
|---|---|---|
| `enable_updates` | `track` | `src/daw/track_bank.rs` |
| `enable_updates` | `master` | `src/daw/track.rs` (master track only) |
| `enable_updates` | `device` (all `SpecificDevice` kinds incl. EQ) | `src/daw/device.rs` |
| `enable_updates` | `project` | `src/daw/project.rs` |
| `enable_updates` | `transport` | `src/daw/transport.rs` |
| `enable_updates` | `browser` | `src/daw/browser.rs` |
| `enable_updates` | `marker` | `src/daw/marker.rs` |
| `enable_updates` | `session` | `src/daw/scene.rs` |
| `enable_updates` | `clip` | `src/daw/clip/cursor_clip.rs` |
| `enable_updates` | `noterepeat`, `groove`, `playingnotes` | never sent (always active) |
| `delay_updates` | `track` | `src/daw/channel.rs` (mute), `src/daw/parameter.rs` (volume/pan/send) - only while `is_automation_rec_active()` |
| `delay_updates` | `master` | `src/daw/parameter.rs` (master volume/pan) - same condition |
| `delay_updates` | `transport` | `src/daw/transport.rs` (position, loop start/length) - same condition |

### Commands per processor

Legend "Rust": `yes (<file>)` = sent by the Rust core; `host only` = handled by host, never sent.
`<t>` = visible-track index (0-based), `<n>` = number part. Value types: none/int/double/string/strings.

#### `track` (`TrackProcessor.cpp`)

| Command | Type | Value | Host does | Rust |
|---|---|---|---|---|
| `<t>/scrollto` | none | - | Select only this track, scroll into view, `deviceSelected = 0` | yes (`track_bank.rs`) |
| `<t>/remove` | none | - | Delete track, select neighbour | yes (`channel.rs`) |
| `<t>/movePrev` | none | - | `ReorderSelectedTracks(t-1)` (moves *selected* tracks) | yes (`track.rs`, `model.rs`) |
| `<t>/moveNext` | none | - | `ReorderSelectedTracks(t+2)` | yes (`track.rs`, `model.rs`) |
| `<t>/clearAutomation` | none | - | Delete all automation envelopes | host only |
| `<t>/recordClip` | none | - | `RecordMidiClip` | yes (`track.rs`) |
| `<t>/clip/<item>/select` | none | - | Unselect all items, select item | host only |
| `<t>/clip/<item>/launch` | none | - | Edit cursor to item start, play if stopped | yes (`slot.rs`) |
| `<t>/clip/<item>/record` | none | - | Edit cursor to item start, record if not recording | yes (`slot.rs`) |
| `<t>/clip/<item>/remove` | none | - | Delete media item | yes (`slot.rs`) |
| `<t>/clip/<item>/duplicate` | none | - | Select item, action `DUPLICATE_ITEMS` | yes (`slot.rs`) |
| `<t>/clip/<item>/insertFile` | string | file path | Select item, `InsertMedia(path, 3)` | yes (`slot.rs`) |
| `<t>/select` | int | 1 (Rust also sends 0) | Select only this track + scroll; **value ignored** | yes (`channel.rs`, `track_bank.rs`) |
| `<t>/toggleMultiSelect` | int | 1 | Toggle selection of track | yes (`track.rs`) |
| `<t>/isGroupExpanded` | int | 1/0 | `I_FOLDERCOMPACT` 0 / 2 | yes (`track.rs`) |
| `<t>/createClip` | int | length in beats | `CreateMidiClip` | yes (`track.rs`) |
| `<t>/active` | int | 1/0 | Actions unlock/FX online/unmute (or reverse) on **selected** tracks; `<t>` only validated | yes (`channel.rs`) |
| `<t>/solo` | int | 1/0 | `SetTrackUISolo` | yes (`channel.rs`) |
| `<t>/mute` | int | 1/0 | `SetTrackUIMute` | yes (`channel.rs`) |
| `<t>/recarm` | int | 1/0 | `SetTrackUIRecArm` | yes (`track.rs`) |
| `<t>/monitor` | int | 1/0 | Input monitor 1 / 0 | yes (`track.rs`) |
| `<t>/autoMonitor` | int | 1/0 | Input monitor 2 / 0 | yes (`track.rs`) |
| `<t>/pin` | int | pinned track position or -1 | `model.pinnedTrackIndex = value` | yes (`model.rs`) |
| `<t>/overdub` | int | 1/0 | `I_RECMODE` 7 / 8 | yes (`model.rs`) |
| `<t>/volume` | double | 0..1 normalized | `DBToValue(SLIDER2DB(v*1000))`, `CSurf_OnVolumeChange` | yes (`parameter.rs`) |
| `<t>/volume/touch` | int/double | 1/0 | `isVolumeTouch` (int falls through to double) | yes (`parameter.rs`) |
| `<t>/pan` | double | 0..1 (-> -1..1) | `CSurf_OnPanChange` | yes (`parameter.rs`) |
| `<t>/pan/touch` | int/double | 1/0 | `isPanTouch` | yes (`parameter.rs`) |
| `<t>/send/<s>/volume` | double | 0..1 normalized | `CSurf_OnSendVolumeChange` | yes (`parameter.rs`) |
| `<t>/send/<s>/active` | double | ignored (Rust sends current value) | `ToggleTrackSendUIMute` (toggle) | yes (`parameter.rs`) |
| `<t>/inQuantResolution` | double | 0, 1, 0.5, 0.25, 0.125 (beats) | MIDI input quantize actions (off / 1/4 / 1/8 / 1/16 / 1/32) | yes (`track.rs`) |
| `<t>/param/<p>/value` | double | 0..1 | `GetTCPFXParm(p)` -> `TrackFX_SetParamNormalized` (TCP-visible FX params) | yes (`parameter.rs`, cursor-track param bank) |
| `<t>/color` | string | `RGB(r,g,b)` | Set track color | yes (`channel.rs`) |
| `<t>/name` | string | name | `P_NAME` | yes (`track.rs`) |
| `addTrack` | strings | `[type, name, color, device...]`; type `INSTRUMENT`/`EFFECT`/`AUDIO`; name may be ""; color `RGB(..)` or "" ; devices = FX creation names | Insert after selected track, name (auto if empty), instrument: all-MIDI input + overdub, color, add FX | yes (`track_bank.rs`) |

#### `master` (`MastertrackProcessor.cpp`)

No-arg and strings overloads are empty `{}`.

| Command | Type | Value | Host does | Rust |
|---|---|---|---|---|
| `select` | int | 1 | Select master, `SetMixerScroll`, `deviceSelected = 0` | yes (`channel.rs`) |
| `solo` | int | 1/0 | `I_SOLO` | yes (`channel.rs`) |
| `mute` | int | 1/0 | `B_MUTE` | yes (`channel.rs`) |
| `volume` | double | 0..1 | `SetTrackUIVolume` (surface update unless touched) | yes (`parameter.rs`) |
| `volume/touch` | int/double | 1/0 | `isMasterVolumeTouch` | yes (`parameter.rs`) |
| `pan` | double | 0..1 | `SetTrackUIPan` | yes (`parameter.rs`) |
| `pan/touch` | int/double | 1/0 | `isMasterPanTouch` | yes (`parameter.rs`) |
| `param/<p>/value` | double | 0..1 | `GetTCPFXParm` on master -> set param | yes (`parameter.rs`: project param bank and cursor-track bank when cursor = master) |
| `color` | string | `RGB(r,g,b)` | `I_CUSTOMCOLOR` | yes (`channel.rs`) |

#### `device` (`DeviceProcessor.cpp`) - always acts on the first selected track

`devicePosition` = `deviceBankOffset + deviceSelected` (the cursor device).

| Command | Type | Value | Host does | Rust |
|---|---|---|---|---|
| `page/+`, `page/-` | none | - | Cursor device +/- `DEVICE_BANK_SIZE` | yes (`device.rs`) |
| `+`, `-` | none | - | Cursor device +/- 1 | yes (`device.rs`) |
| `<d>/remove` | none | d 1-based | `TrackFX_Delete` | yes (`device.rs`) |
| `<d>/duplicate` | none | | Copy FX to d+1 | yes (`device.rs`) |
| `<d>/movePrev` | none | | Move FX to d-1 | yes (`device.rs`) |
| `<d>/moveNext` | none | | Move FX to d+1 | yes (`device.rs`) |
| `selected` | int | 1-based index in page | Cursor = `deviceBankOffset + value-1` | yes (`device.rs`) |
| `page/selected` | int | 1-based page | Cursor = `offset + (v-1)*DEVICE_BANK_SIZE` | host only |
| `preset` | int | preset index | `TrackFX_SetPresetByIndex(cursor)` | yes (`browser.rs`) |
| `bypass` | int | 1/0 | Bypass cursor device (value>0 -> disabled) | host only |
| `<d>/bypass` | int | 1 = currently enabled (bypass it) | `TrackFX_SetEnabled(d-1, value>0 ? 0 : 1)` | yes (`device.rs`) |
| `window` | int | 1/0 | `TrackFX_SetOpen(cursor)` (floating window) | yes (`device.rs`) |
| `expand` | int | 1/0 | `TrackFX_Show(cursor, 1/3)` (FX chain window) | yes (`device.rs`) |
| `param/<p>/value` | double (int also accepted) | 0..1 | `TrackFX_SetParamNormalized(cursor, p)` | yes (`parameter.rs`) |
| `add/<pos>` | string | FX creation name | `TrackFX_AddByName` + move to pos | yes (`channel.rs`, pos=0) |

#### `eq` (`EqDeviceProcessor.cpp`, subclass of DeviceProcessor)

`GetDeviceSelection()` = `TrackFX_GetEQ(track, false)` (ReaEQ index), so inherited `param/...`,
`bypass`, `window`, `expand`, `preset` act on the EQ. Inherited `selected`, `page/*`, `+/-` change
the normal cursor device (?).

| Command | Type | Value | Host does | Rust |
|---|---|---|---|---|
| `add` | none | - | `TrackFX_GetEQ(track, true)` (create ReaEQ) | yes (`channel.rs`) |
| `band/<b>` | string | ReaEQ band type number, `-1` = off (`EqualizerBandType::reaper_index`: Lowshelf 0, Highshelf 1, Highcut 3, Lowcut 4, Notch 6, Bell 8) | `BANDTYPE<b>` + `BANDENABLED<b>` | yes (`device.rs`) |
| `param/<p>/value` | double | 0..1 | Set EQ param | yes (`parameter.rs`) |
| `<d>/bypass`, `selected`, `<d>/remove`, `<d>/duplicate` | as device | | inherited (fx index from `<d>`) | yes (`device.rs`, Eq kind) |

#### `clip` (`ClipProcessor.cpp`) - acts on the first selected media item

All overloads return if no item is selected. Int -> double.

| Command | Type | Value | Host does | Rust |
|---|---|---|---|---|
| `clear` | none | - | Clear all notes | yes (`cursor_clip.rs`) |
| `duplicate` | none | - | Action `DUPLICATE_ITEMS` | yes (`cursor_clip.rs`) |
| `duplicateContent` | none | - | Duplicate + glue | yes (`cursor_clip.rs`) |
| `note/<pitch>/clear/<ch>` | none | - | Clear row | yes (`cursor_clip.rs`) |
| `start` | double | beats (QN) | `D_POSITION = v*60/BPM(at item start)` | yes (`cursor_clip.rs`) |
| `end` | double | beats | `D_LENGTH` from `v*60/BPM(at item end)` | yes (`cursor_clip.rs`) |
| `transpose` | int/double | semitones | `TransposeClip` | yes (`cursor_clip.rs`) |
| `loop` | int/double | 1/0 | `B_LOOPSRC` | yes (`cursor_clip.rs`) |
| `note/<pitch>/clear/<ch>` | double | position in beats (QN from clip start) | Clear one note | yes (`cursor_clip.rs`) |
| `note/<pitch>/clearPosition/<ch>` | double | position beats | Clear all notes at position (Rust uses pitch 0) | yes (`cursor_clip.rs`) |
| `note/<pitch>/moveY/<ch>/<newPitch>` | double | position beats | Move note to new pitch | yes (`cursor_clip.rs`) |
| `color` | string | `RGB(r,g,b)` | Item color | yes (`cursor_clip.rs`) |
| `name` | string | name | Take name (?) | yes (`cursor_clip.rs`) |
| `insertFile` | string | path | `InsertMedia(path, 3)` | host only |
| `note/<pitch>/toggle` | string | `"pos len vel ch muted"` (5 space-separated; pos/len beats, Java double format; Rust always muted=0) | Clear if exists else insert | yes (`cursor_clip.rs`) |
| `note/<pitch>/set` | string | same | Insert note | yes (`cursor_clip.rs`) |
| `note/<pitch>/update` | string | same, muted 1/0 | `MIDI_SetNote` on note at pos | yes (`cursor_clip.rs`) |

String note commands require a MIDI take and exactly 5 parts, else ignored.

#### `scene` (`SceneProcessor.cpp`) - scenes = Reaper regions

Int overload = default (1 -> no-arg). Double/strings empty.

| Command | Type | Value | Host does | Rust |
|---|---|---|---|---|
| `<i>/select` | none | - | Edit cursor + loop to region | yes (`scene.rs`) |
| `<i>/launch` | none | - | Same + play if stopped | yes (`scene.rs`) |
| `<i>/remove` | none | - | Delete region | yes (`scene.rs`) |
| `<i>/duplicate` | none | - | `DuplicateScene` (actions 40717, 40200, 41319, add region " - Copy") | yes (`scene.rs`) |
| `<i>/color` | string | `RGB(r,g,b)` | Region color | yes (`scene.rs`) |
| `<i>/name` | string | name | Rename region | yes (`scene.rs`) |

#### `marker` (`MarkerProcessor.cpp`)

| Command | Type | Value | Host does | Rust |
|---|---|---|---|---|
| `add` | none | - | Add marker "Marker <count+1>" at cursor | yes (`marker.rs`) |
| `<i>/select` | none | - | Edit cursor to marker | yes (`marker.rs`) |
| `<i>/launch` | none | - | Same + play if stopped | yes (`marker.rs`) |
| `<i>/remove` | none | - | Delete marker | yes (`marker.rs`) |

#### `project` (`ProjectProcessor.cpp`)

| Command | Type | Value | Host does | Rust |
|---|---|---|---|---|
| `createSceneFromPlayingLauncherClips` | none | - | Duplicate region at cursor | yes (`project.rs`) |
| `engine` | int | 1/0 | `Audio_Init` / `Audio_Quit` | yes (`application.rs`) |
| `createScene` | int | length in beats | Add region after last region, length `v*60/BPM` | yes (`project.rs`, `scene.rs`) |

#### Transport-type processors (`TransportProcessor.h`) - command is `""` unless noted

| Processor | Command | Type | Value | Host does | Rust |
|---|---|---|---|---|---|
| `play` | `""` | none | - | Toggle play/pause (`CSurf_OnPlay`/`OnPause`) | yes (`transport.rs`) |
| `stop` | `""` | none | - | `CSurf_OnStop` | yes (`transport.rs`) |
| `record` | `""` | none | - | `CSurf_OnRecord` (toggle) | yes (`transport.rs`) |
| `repeat` | `""` | int | 1 (-> no-arg) | Action `TRANSPORT_REPEAT` 1068 (toggle loop) | yes (`transport.rs`) |
| `time` | `""` | double (int) | seconds | Edit cursor, snapped to grid, clamped to project length | yes (`transport.rs`; int 0 = rewind) |
| `time` | `nosnap` | double | seconds | Edit cursor, no snap | yes (`transport.rs`) |
| `time` | `loop/start` | double | seconds | Move loop so it starts at snapped value (keeps length) | yes (`transport.rs`) |
| `time` | `loop/<anything else>` | double | seconds | Set loop **end** = snapped value. Rust sends `loop/length` with the value the host reports as `/time/loop/length` (= end), so consistent; name is misleading | yes (`transport.rs`) |
| `time` | `signature` | int | `numerator + denominator*100` | Time-sig marker at cursor | yes (`transport.rs`) |
| `tempo` | `+` / `++` / `-` / `--` | none | - | Actions 41137 (+0.1) / 41129 (+1) / 41138 (-0.1) / 41130 (-1) BPM | yes (`transport.rs`) |
| `tempo` | `""` | double (int) | BPM | `CSurf_OnTempoChange` | yes (`transport.rs`) |
| `automation` | `""` | int | 0 trim/read,1 read,2 touch,3 write,4 latch,5 (Rust: LatchPreview; Reaper: bypass) | `SetGlobalAutomationOverride` | yes (`transport.rs`) |
| `quantize` | `""` | double | amount 0..1 (**ignored**) | Open MIDI editor (40153), select all (40003), quantize (40728), close if it was closed | yes (`cursor_clip.rs`) |
| `metro_vol` | `""` | int | 0..127 | `KBD_OnMainActionEx(999, v, -1, 0, ...)` (absolute) | yes (`transport.rs`) |
| `metro_vol` | `+` / `-` | none | - | Same, relative (mode 2, 0x40 +/- 1) | yes (`transport.rs`) |
| `undo` | `""` | none | - | `Undo_DoUndo2` | yes (`application.rs`) |
| `redo` | `""` | none | - | `Undo_DoRedo2` | yes (`application.rs`) |
| `cursor` | `""` | int | direction | `CSurf_OnArrow(value, 0)` | host only |
| `refresh` | `""` | none | - | `model.SetDump()` -> next collection sends everything | yes (`app.rs`) |

#### `noterepeat` (`NoteRepeatProcessor.cpp`) - arpeggiator JSFX in input FX of the selected track

| Command | Type | Value | Host does | Rust |
|---|---|---|---|---|
| `active` | int | 1/0 | Add/enable (or disable) `MIDI_ARP_PLUGIN` in rec-FX chain | yes (`midi.rs`) |
| `rate` | double | `1/period` | Param `MIDI_ARP_PARAM_RATE`, normalized `v/(max-min)` | yes (`midi.rs`) |
| `notelength` | double | length | Param `MIDI_ARP_PARAM_NOTE_LENGTH` | yes (`midi.rs`) |
| `mode` | int/double | 0..3 (`ARP_MODES`: Down, Up, Down-Up, Up-Down) | Param `MIDI_ARP_PARAM_MODE` | yes (`midi.rs`) |
| `velocity` | int/double | 1 = use pressure/velocity (?) | Param `MIDI_ARP_PARAM_VELOCITY` = 127 if v==0 else 0 | yes (`midi.rs`) |

#### `groove` (`GrooveProcessor.cpp`)

| Command | Type | Value | Host does | Rust |
|---|---|---|---|---|
| `active` | double | >0 = on | Project grid swing mode | yes (`parameter.rs`) |
| `amount` | double | -1..1 (Rust: `value*2-1`) | Project grid swing amount | yes (`parameter.rs`) |

#### `action` (`ActionProcessor.cpp`)

| Command | Type | Value | Host does | Rust |
|---|---|---|---|---|
| `""` | int | Reaper command ID | `Main_OnCommandEx(id)` | yes (`context.rs` `invoke_action`, ids in `actions.rs`) |
| `""` | string | numeric ID or named command (e.g. `_SWS_...`) | `atoi`, else `NamedCommandLookup`, then `Main_OnCommandEx` | yes (`application.rs` `invoke_action(&str)`) |
| `select` | none | - | Open Reaper action list (`PromptForAction`); selection reported back as `/action/select` | host only |

#### `inifile` (`IniFileProcessor.cpp`)

| Command | Type | Value | Host does | Rust |
|---|---|---|---|---|
| `<section>/<key>` (exactly 2 parts) | int | value | Write `REAPER.ini` `[section] key=value` if changed; also writes the in-memory config var if it is a double | yes: `reaper/prerollmeas` (`transport.rs`) |

#### Protocol v2: commands instead of Reaper action IDs

The core used to invoke Reaper action IDs for these features (`action` with an int). It now sends
commands named after its methods; the Reaper backend maps them to the same actions
(`CommandProcessors.h`, `ProjectProcessor.cpp`, `MarkerProcessor.cpp`, `TrackProcessor.cpp`), the
Bitwig backend to the Bitwig API. A backend ignores the commands of features it does not have.

| Processor | Command | Type | Meaning | Reaper | Bitwig |
|---|---|---|---|---|---|
| `transport` | `metronome` | int | metronome on (1) / off (0) | 41745 / 41746 | `isMetronomeEnabled` |
| `transport` | `toggleMetronome` | none | | 40364 | `isMetronomeEnabled ().toggle` |
| `transport` | `togglePrerollMetronome` | none | | 41819 | `isMetronomeAudibleDuringPreRoll` |
| `transport` | `punchIn` / `punchOut` | int | on / off | 40076 / 40253, off: 40252 | `isPunchInEnabled` / `isPunchOutEnabled` |
| `transport` | `tapTempo` | none | | 1134 | `tapTempo` |
| `transport` | `end` | none | go to the end of the project | 40043 | - |
| `project` | `previous` / `next` | none | previous / next project | 40862 / 40861 | `previousProject` / `nextProject` |
| `project` | `save` / `load` | none | | 40026 / 40025 | - |
| `marker` | `previous` / `next` | none | go to the previous / next marker | 40172 / 40173 | `jumpToPreviousCueMarker` / `jumpToNextCueMarker` |
| `track` | `<t>/duplicate` | none | | select, 40062 | `duplicateObject` |
| `application` | `layout` | int | panel layout: 0 arrange, 1 mix, 2 edit, 3 play | window sets 40454 + n | `setPanelLayout` |
| `application` | `toggleNoteEditor`, `toggleAutomationEditor`, `toggleDevices`, `toggleInspector`, `toggleMixer`, `toggleFullScreen`, `toggleBrowser` | none | | 40716, 40926, -, 40906, 40078, 40346, 50124 | the `Application` toggles |
| `application` | `duplicate`, `deleteSelection` | none | | -, 40006 | `duplicate`, `remove` |
| `application` | `arrowLeft`, `arrowUp`, `arrowRight`, `arrowDown` | none | | previous / next track (40286 / 40285) | `arrowKey*` |
| `application` | `zoomIn`, `zoomOut`, `incTrackHeight`, `decTrackHeight` | none | | 1012, 1011, 40111, 40112 | `zoomIn`, `zoomOut`, `zoom*LaneHeightsAll` |
| `application` | `sliceToSampler`, `sliceToDrumMachine` | none | | 40760 | - |
| `application` | `enter`, `escape` | none | | - | `enter`, `escape` |
| `arranger` | `toggleCueMarkers`, `togglePlaybackFollow`, `toggleTrackRowHeight` | none | | 42328, 40036, 42698 | the `Arranger` toggles |
| `arranger` | `toggleClipLauncher`, `toggleTimeLine`, `toggleIoSection`, `toggleEffectTracks` | none | | - | the `Arranger` toggles |
| `mixer` | `toggleDeviceSection`, `toggleIoSection`, `toggleSendsSection` | none | | 40549, 40910, 40557 | the `Mixer` toggles |
| `mixer` | `toggleClipLauncherSection`, `toggleCrossFadeSection`, `toggleMeterSection` | none | | - | the `Mixer` toggles |

`action` with a string (user-configured Reaper actions, `Application::invoke_action`) stays.

#### Protocol v2: time in beats

| Processor | Command | Type | Meaning |
|---|---|---|---|
| `beats` | `position` | double | play position in quarter notes, snapped to the grid (Reaper) |
| `beats` | `position/nosnap` | double | play position in quarter notes |
| `beats` | `loop/start` | double | loop start in quarter notes, the loop keeps its length |
| `beats` | `loop/length` | double | loop length in quarter notes |

Reaper converts with the tempo map (`TimeMap2_QNToTime`). Bitwig sets `playStartPosition` (and
jumps there while playing), `arrangerLoopStart` and `arrangerLoopDuration`, without snapping,
as the Java version.

#### Protocol v2: clip launcher slots and scenes

Hosts with a clip launcher report the slot states (§3, `/track/<n>/slot/...`); the core then uses
them instead of deriving the states from the transport and the regions. `<s>` is the absolute
slot (scene) index.

| Processor | Command | Type | Meaning | Reaper |
|---|---|---|---|---|
| `track` | `<t>/clip/<s>/launch`, `launchAlt` | none | launch on press (normal / alternative) | both launch the item |
| `track` | `<t>/clip/<s>/launchRelease`, `launchReleaseAlt` | none | release of the launch button | ignored |
| `track` | `<t>/clip/<s>/color` | string | `RGB(r,g,b)` | ignored (the core keeps it locally) |
| `track` | `<t>/clip/<s>/paste` | string | paste the slot `"<track>/<slot>"` into this slot | duplicates the source item |
| `track` | `<t>/createClip/<s>` | int | new clip of the given length in beats in slot `<s>` | creates the clip at the edit cursor (`<s>` ignored) |
| `track` | `<t>/stop`, `<t>/stopAlt` | none | stop the clips of the track | ignored |
| `track` | `<t>/returnToArrangement` | none | | ignored |
| `scene` | `<n>/launch`, `launchAlt` | none | launch on press | both launch the region |
| `scene` | `<n>/launchRelease`, `launchReleaseAlt` | none | release | ignored |
| `scene` | `stop`, `stopAlt` | none | stop all clips | ignored |

#### Protocol v2: parameter pages

| Processor | Command | Type | Meaning | Reaper |
|---|---|---|---|---|
| `device` | `parameterPage` | int | the core selected this parameter page (0-based); the host shows the same page | ignored |
| `device` | `param/<i>/touch` | int | a touch sensitive control touches the parameter (1) or releases it (0) | ignored |
| `device` | `param/<i>/indicate` | int | indicate the parameter on the DAW's user interface | ignored |

#### Protocol v2: host-side paging

A host which only reports a window of the tracks or scenes sends the window offset
(`/track/offset`, `/scene/offset`, §3). From then on the core tells it every change of its bank
offset; the core stays in charge of the paging, the host scrolls its window to follow. Reaper sends
everything and never reports an offset.

| Processor | Command | Type | Meaning |
|---|---|---|---|
| `bank` | `track` | int | the offset of the core's track bank (the first visible track) |
| `bank` | `scene` | int | the offset of the core's scene bank (the first visible scene and slot) |

#### Protocol v2: drum pads

The pads of the cursor track's drum machine are channels of their own. Their commands used to go
to `track <pad>/…` (mismatch f); Reaper has no drum machines and ignores the `drum` commands.

| Processor | Command | Type | Meaning |
|---|---|---|---|
| `drum` | `pad/<p>/select` | none | select the pad (`<p>` = 0..127, the note) |
| `drum` | `pad/<p>/mute`, `solo`, `active` | int | 1 / 0 |
| `drum` | `pad/<p>/volume`, `pan` | double | 0..1 |
| `drum` | `pad/<p>/color` | string | `RGB(r,g,b)` |
| `drum` | `clearMute`, `clearSolo` | none | unmute / unsolo all pads |
| `drum` | `indicate` | int | indicate the pads on the DAW's user interface |

#### Protocol v2: the first instrument (`primary`)

The commands of the first instrument of the cursor track (`DeviceId::FirstInstrument`) used to go
to `device` (mismatch d). They now go to `primary`: its parameters, its drum pads (if it is a drum
machine) and its layers (if it is a container, e.g. an Instrument Layer). Reaper's
`PrimaryDeviceProcessor` is a `DeviceProcessor` on `TrackFX_GetInstrument` of the selected track;
Reaper has no pads and layers and ignores those commands.

| Processor | Command | Type | Meaning |
|---|---|---|---|
| `primary` | `param/<i>/value`, `touch`, `indicate` | double / int | as for `device` |
| `primary` | `pad/<p>/…`, `clearMute`, `clearSolo`, `indicate` | | as for `drum` |
| `primary` | `layer/<i>/select` | none | select the layer (`<i>` = 0-based over all layers) |
| `primary` | `layer/<i>/mute`, `solo`, `active`, `volume`, `pan`, `color` | | as for the pads |
| `primary` | `window`, `expand` | int | open the plugin window, expand the device |

#### Protocol v2: the browser of the host

A host with its own browser (Bitwig) reports `/browser/exists`; from then on the core sends all
browser functions to it. Reaper reports no `/browser/exists` and keeps browsing the presets of
the cursor device in the core (`device preset <index>`); it has no `browser` processor.

| Processor | Command | Type | Meaning |
|---|---|---|---|
| `browser` | `replace/device` | none | browse to replace the cursor device |
| `browser` | `replace/slot/<t>/<s>` | none | browse for a clip for the slot (absolute track and slot) |
| `browser` | `replace/<drum\|primary>/pad/<p>` | none | browse for the device of a drum pad |
| `browser` | `add/track/<t>`, `add/master` | none | browse for a device at the start of the channel's device chain |
| `browser` | `insertBefore`, `insertAfter` | none | browse for a device before / after the cursor device (without one: start / end of the cursor track's chain) |
| `browser` | `stop` | int | close the browser, 1 = commit the selection, 0 = cancel |
| `browser` | `preview` | int | preview (audition) on / off |
| `browser` | `contentType/+`, `contentType/-` | none | next / previous content type (tab) |
| `browser` | `column/<c>/+`, `-`, `page/+`, `page/-`, `reset` | none | the selection of a filter column; `reset` selects the wildcard |
| `browser` | `result/+`, `-`, `page/+`, `page/-` | none | the selected result |

#### Names without a command processor

`browser`, `session`, `playingnotes` are only valid for `enable_updates`/`delay_updates`. Rust
never sends commands to them. `transport` is valid for both.

## 3. Model updates (host → core)

### Lines and addresses

| Item | Rule | Source |
|---|---|---|
| Entry point | `dbm_update_model(DbmApp*, const char* data)`; C++: `dbm::App::updateModel(const std::string&)` | ffi.rs, drivenbymoss.hpp:206 |
| Encoding | NUL-terminated UTF-8; invalid UTF-8 is replaced (`to_string_lossy`); null = empty | ffi.rs `from_c` |
| Lines | `data.split('\n')`; empty lines skipped. `\r` is **not** stripped: use `\n` only (numbers still parse because they are trimmed; strings would keep `\r`) | app.rs `update_model` |
| Line | `<address> <value>`; split at the **first** space (`split_once(' ')`), rest of line = value (may contain spaces, may be empty). Line with no space: value = `""` | parser.rs `parse_line` |
| Address | `/`-separated path; leading `/` optional (stripped). Host always writes a leading `/` | parser.rs `parse_osc` |
| Escaping | None. A value must not contain `\n` (or NUL). Host replaces `;` by space in clip names inside `/clip/all` only | DataCollector.cpp:761 |
| Host writer | `Collectors::Collect{String,Int,Double}Value`: `ss << command << " " << value << "\n"` | Collectors.h |
| Dispatch | Every line is parsed once per **running** controller instance (`instance.running`), each with its own `Parser`/`Model`. Instances not running drop the data. After each line `model.after_parse()` runs (track list recalc, cursor track) | app.rs:299, model.rs:204 |
| Unknown address | logged via `host.error("Unhandled ...")`, otherwise ignored | parser.rs |

### Value encodings

| Type | Host output | Parser | Notes |
|---|---|---|---|
| double | `std::ostringstream << double` (default precision: 6 significant digits, "C" locale, e.g. `0.716`, `1e-05`) (?) precision not set explicitly | `parse_double`: trim + Rust `f64::parse` (accepts `1e-05`, `inf`, `NaN`) | Change detection threshold `1e-10` (Collectors.h) |
| int | `<< int` | `parse_int`: i32, or f64 truncated to i32 | |
| bool | int `0`/`1` | `> 0` (via int or double) | Exception: `bypass` 1 = bypassed → parser `set_enabled(v == 0)` |
| normalized | double 0..1 (volume `DB2SLIDER(dB)/1000`, pan `(pan+1)/2`, param `TrackFX_GetParamNormalized`, VU `ReaperUtils::ValueToVURange` clipped −60..+6 dB→0..1) | stored as internal value | |
| string | raw text after first space; may be empty | taken verbatim | Host truncates: track name 20 (`Track::NAME_LENGTH`), send name 20, device/primary name 50, param name/value 60, project name 50, time strings 20 (`TIME_LENGTH`), preset filename/name 1024 |
| colour | `R G B` ints 0..255, single spaces (`Collectors::FormatColor`); `-1 -1 -1` = no custom colour | `Model::parse_color` / `parse_color_static`: exactly 3 space-separated numbers, `/255`; `R < 0` → `None` → GRAY fallback (track, send, clip, marker, scene). Wrong count → error logged | |
| dB / pan text | `FormatDB`: `-inf dB` (≤ −150), `+0.0 dB`, `-6.0 dB`; `FormatPan`: `C`, `50L`, `30R` | verbatim display string | |
| note list | `sel:mute:start:end:chan:pitch:vel;` repeated; empty list = `" "` | `Note::parse_notes`: trim, split `;`, split `:`, 7 fields (i32,i32,f64,f64,i32,i32,i32); bad entries dropped | `/clip/notes`: start/end in quarter notes relative to item start; `/track/<n>/playingnotes`: start/end in project seconds (?) |
| clip list (`/clip/all`) | per visible track: `trackIdx;clipCount;` then per clip `name;selected;R G B;muted;` | `Model::set_clips`: same order; stops at first unparsable int | colour `-1 ...` → slot colour unchanged |

### Full dump vs. incremental

| Item | Rule |
|---|---|
| Cadence | `DrivenByMossSurface::Run` (~30 Hz); `updateModel` only on every 2nd call (~60 ms); not called if collected text is empty |
| Incremental | Host caches the last sent value per address and emits a line only on change |
| Full dump | `dump=true`: every collector emits unconditionally. Triggered by `Model::ShouldDump()` (one-shot flag) |
| Core requests dump | Core calls backend `process_no_arg("refresh", "")` (`Sender::refresh`) → Reaper `RefreshProcessor` → `Model::SetDump()` → next update is a dump. Core sends it in `App::start_controllers` (i.e. `dbm_start_infrastructure`, `dbm_restart_controllers`) and `App::add_controller` |
| Slow values | Only every 16th update (`SLOW_UPDATE`, ~1 s) or on dump: `/project/name`, `/project/engine`, `/device/name`, `/device/bypass`, `/device/sibling/*`, `/master/color`, all `/marker/*` |
| Device block | Dumped also when the cursor track changed (`dump \|\| hasTrackChanged`) |
| Session block | `/clip/all`, `/scene/*` only when `GetProjectStateChangeCount` changed, or on a dump |
| Playing notes | `/track/<n>/playingnotes` only for selected tracks while `/play` = 1 and processor `playingnotes` enabled |
| Update gating (core→host) | `enable_updates(processor, bool)` disables a collector group; `delay_updates(processor)` suppresses it for 300 ms (`DataCollector::DELAY`). Group names checked by `IsActive`: `project`, `transport`, `track`, `device`, `master`, `browser`, `marker`, `clip`, `session`, `noterepeat`, `groove`, `playingnotes` |

### Ordering (as emitted by host, per update)

`/action/select` → project → transport → tracks (`/track/<n>/*` incl. sends, then `/track/count` **last**) → device (`/device/count` first, …, `/device/param/count` **before** params; primary; eq; `/track/fx/param/count` before params) → master (`/master/fx/param/count` before params) → browser → marker (`/marker/count` first) → clip → session (`/clip/all`, `/scene/count`, `/scene/<n>/*`) → noterepeat → groove.

Parser requirements: none strict. Indexed items auto-grow (`Bank::unpaged_item_mut` → `ensure_items`), so items may arrive before their `count`. `count` lines call `set_item_count` (clamps bank offset) and emit `ParserEvent::RebindKnobs` (track, send, param counts). Within a track the host sends `send/<i>/*` before `send/count`.

### Addresses

Columns: **Host** = sent by Reaper DataCollector; **Parser** = handled in parser.rs (`no-op` = accepted and discarded).

#### Project / action

| Address | Value | Meaning | Host | Parser |
|---|---|---|---|---|
| `/project/name` | string | project name; parser also emits `UpdateNoteMapping` | yes (slow) | yes |
| `/project/engine` | int bool | `Audio_IsRunning()` | yes (slow) | yes |
| `/project/canUndo` | int bool | | yes | yes |
| `/project/canRedo` | int bool | | yes | yes |
| `/project/isDirty` | int bool | | yes | yes |
| `/action/select` (or `action/select`) | string `_<named cmd>` or int id | action chosen in Reaper action dialog → `ParserEvent::ActionSelected` (bypasses OSC parsing) | yes (once per selection) | yes |

#### Transport / click / time

| Address | Value | Meaning | Host | Parser |
|---|---|---|---|---|
| `/play` | int bool | playing | yes | yes |
| `/stop` | number | play state = (v == 0) | **no** | yes |
| `/record` | int bool | | yes | yes |
| `/repeat` | int bool | loop | yes | yes |
| `/tempo` | double BPM | | yes | yes |
| `/numerator` | int > 0 | time sig at cursor | yes | yes (ignored if ≤ 0) |
| `/denominator` | int > 0 | | yes | yes (ignored if ≤ 0) |
| `/time` | double seconds | cursor/play position | yes | yes |
| `/time/str` | string | `format_timestr` (+project offset) | yes | yes (`reformat_time`) |
| `/beat` | string `measures.beats.ticks` | `format_timestr_pos(...,2)` | yes | yes (`reformat_beats`) |
| `/time/hzoom` | double px/s | `GetHZoomLevel()` (Bitwig: 1 / content per pixel) | yes | yes |
| `/beats/position` | double quarters | play position (Bitwig: play start position) | yes (v2) | yes |
| `/beats/loop/start` | double quarters | loop start | yes (v2) | yes |
| `/beats/loop/length` | double quarters | loop length (not the end) | yes (v2) | yes |
| `/time/text`, `/beats/text`, `/beats/loop/start/text`, `/beats/loop/length/text` | string | position and loop texts formatted by the host, used as is | Bitwig only (v2) | yes |
| `/time/loop/start` | double seconds | | yes | yes |
| `/time/loop/start/str` | string | | yes | yes → `set_loop_start_text` |
| `/time/loop/start/beat` | string | | yes | yes (any 4th part) → `set_loop_start_beat_text` |
| `/time/loop/length` | double seconds | host sends loop **end** (`endOut`) — see S2 | yes | yes → `set_loop_length_value` |
| `/time/loop/length/str` | string | | yes | yes → `set_loop_length_text` |
| `/time/loop/length/beat` | string | | yes | yes → `set_loop_length_beat_text` |
| `/followPlayback` | int bool | toggle 40036 | yes | yes |
| `/automode` | int | `GetGlobalAutomationOverride()`: −1 none, 0 TRIM_READ, 1 READ, 2 TOUCH, 3 WRITE, 4 LATCH, 5 LATCH_PREVIEW, 6 bypass | yes | 0..5 mapped; −1/6 ignored (state kept) |
| `/click` | int bool | metronome (toggle 40364) | yes | yes |
| `/click/preroll` | int | config var `preroll`, a bit field: 1 = pre-roll before playback, 2 = before recording (action 41819 toggles 2) | yes | yes: `(v & 2) > 0` |
| `/click/prerollMeasures` | number (string from config `prerollmeas`) | | yes | yes (int; outside 0..4 → 2) |
| `/click/volume` | double 0..1 | metronome volume | yes | yes |
| `/click/volumeStr` | string dB | | yes | yes |

#### Tracks (`/track/<n>/...`, n = 0-based index over **visible** tracks)

| Address | Value | Meaning | Host | Parser |
|---|---|---|---|---|
| `/track/count` | int | number of visible tracks (sent after tracks) | yes | yes (+RebindKnobs) |
| `/track/offset` | int | offset of the host's track window; the host pages the tracks (v2) | Bitwig (v2) | yes |
| `/track/<n>/exists` | int (always 1) | | yes | yes |
| `/track/<n>/number` | int | = n | yes | yes (`set_position`) |
| `/track/<n>/depth` | int | `GetTrackDepth` (folder depth) | yes | yes |
| `/track/<n>/name` | string ≤ 20 | | yes | yes |
| `/track/<n>/type` | `GROUP` \| `HYBRID` | folder or not | yes | yes; accepts `UNKNOWN AUDIO INSTRUMENT HYBRID GROUP GROUP_OPEN EFFECT MASTER LAYER CUE` |
| `/track/<n>/isGroupExpanded` | int bool | `I_FOLDERCOMPACT == 0` | yes | yes |
| `/track/<n>/select` | int bool | parser: bank selection handling, `UpdateNoteMapping` on select | yes | yes |
| `/track/<n>/mute` | int bool | envelope-aware | yes | yes |
| `/track/<n>/solo` | int bool | | yes | yes |
| `/track/<n>/recarm` | int bool | | yes | yes |
| `/track/<n>/monitor` | int bool | `I_RECMON == 1` | yes | yes |
| `/track/<n>/autoMonitor` | int bool | `I_RECMON == 2` | yes | yes |
| `/track/<n>/overdub` | int bool | `I_RECMODE == 7` | yes | yes |
| `/track/<n>/color` | colour | | yes | yes |
| `/track/<n>/volume` | double 0..1 | | yes | yes |
| `/track/<n>/volume/str` | string dB | | yes | yes |
| `/track/<n>/pan` | double 0..1 | | yes | yes |
| `/track/<n>/pan/str` | string | | yes | yes |
| `/track/<n>/vu`, `vuleft`, `vuright` | double 0..1 | | yes | yes |
| `/track/<n>/vuholdleft`, `vuholdright` | double | `Track_GetPeakHoldDB(...)*100` (range ?) | yes | yes |
| `/track/<n>/playingnotes` | note list | notes under play position of selected track | yes (conditional) | yes (`handle_notes`; not for master) |
| `/track/<n>/active` | number bool | track activated | **no** | yes |
| `/track/<n>/inQuantLengthEnabled` | number bool | record quantize note length | **no** | yes |
| `/track/<n>/inQuantResolution` | double beats | record quantize grid | **no** | yes |

#### Sends (`/track/<n>/send/<s>/...`, s = 0-based, all sends)

| Address | Value | Meaning | Host | Parser |
|---|---|---|---|---|
| `/track/<n>/send/count` | int | `GetTrackNumSends(track,0)` | yes | yes (+RebindKnobs) |
| `/track/<n>/send/<s>/active` | int bool | not UI-muted | yes | yes |
| `/track/<n>/send/<s>/name` | string ≤ 20 | parser: `exists = !empty` | yes | yes |
| `/track/<n>/send/<s>/volume` | double 0..1 | | yes | yes |
| `/track/<n>/send/<s>/volume/str` | string dB | | yes | yes |
| `/track/<n>/send/<s>/color` | colour | colour of destination track | yes | yes |

#### Master (`/master/...`) — handled by the same track code (`TrackRef::Master`)

| Address | Value | Host | Parser |
|---|---|---|---|
| `/master/select`, `mute`, `solo` | int bool | yes | yes |
| `/master/volume`, `/master/pan` | double 0..1 | yes | yes |
| `/master/volume/str`, `/master/pan/str` | string | yes | yes |
| `/master/vu`, `vuleft`, `vuright` | double 0..1 | yes | yes |
| `/master/color` | colour (slow) | yes | yes |
| `/master/number` | int | no | accepted, ignored for master |
| `/master/fx/param/count` | int (TCP FX params of master) | yes | yes → `project.parameter_bank` (+RebindKnobs); ignored if none |
| `/master/fx/param/<i>/{name,value,value/str,steps}` | see 2.6 | yes | yes → project parameter bank; i = 0 also copied to crossfade parameter |

#### Devices and parameters

Prefixes: `/device` = cursor device (selected FX of cursor track; pinned track if `pinnedTrackIndex` set), `/primary` = first instrument (`DeviceId::FirstInstrument`), `/eq` = first ReaEQ (`DeviceId::Eq`). `/primary` and `/eq` are silently dropped if the controller's `ModelSetup` does not enable that device (`specific_device_mut` → None).

Parameter sub-addresses (all banks; `<i>` = absolute 0-based parameter index; host sends all `0..count-1`):

| Address | Value | Meaning | Host | Parser |
|---|---|---|---|---|
| `<prefix>/param/<i>/name` | string ≤ 60 | parser: `exists = !empty`, position = i | yes | yes |
| `<prefix>/param/<i>/value` | double 0..1 normalized | | yes | yes (+value observers) |
| `<prefix>/param/<i>/value/str` | string ≤ 60 | `TrackFX_FormatParamValue`; only sent with a value change or dump | yes | yes |
| `<prefix>/param/<i>/steps` | int: 2 = toggle, −1 = continuous | | yes | yes |
| `<prefix>/param/<i>/modulated` | double 0..1 | the modulated value; without it the core uses the value | Bitwig (v2) | yes |

`<prefix>` ∈ `/device`, `/primary`, `/eq`, `/track/fx` (TCP FX params of the cursor track → selected track's parameter bank), `/master/fx`.

| Address | Value | Meaning | Host | Parser |
|---|---|---|---|---|
| `/device/count` | int | `TrackFX_GetCount` | yes | yes (cursor only) |
| `/device/exists` | int bool | | yes | yes |
| `/device/position` | int (−1 none) | selected FX index | yes | yes |
| `/device/window` | int bool | FX window open | yes | yes |
| `/device/expand` | int bool | FX chain visible | yes | yes |
| `/device/name` | string ≤ 50 (slow) | core shortens it (test: `VST: ReaEQ (Cockos)` → `ReaEQ`) | yes | yes |
| `/device/bypass` | int, 1 = bypassed (slow) | | yes | yes |
| `/device/touchedParam` | int | last touched param (only when FX focused on cursor track) | yes (conditional) | yes |
| `/device/page/count` | int | number of named parameter pages | Bitwig (v2) | yes |
| `/device/page/<p>/name` | string | name of a parameter page; without names the core shows "Page n" | Bitwig (v2) | yes |
| `/device/page/selected` | int | the page selected in the host's user interface; the core shows it | Bitwig (v2) | yes |
| `/device/param/count` | int | | yes | yes (+RebindKnobs) |
| `/device/sibling/<k>/name` | string; k = **1..8** | FX at `deviceBankOffset + k-1`; parser: `exists = !empty` | yes (slow) | yes (k-1 must be < device bank page size) |
| `/device/sibling/<k>/bypass` | int, 1 = bypassed | | yes (slow) | yes |
| `/device/sibling/<k>/position` | int absolute FX index | | yes (slow) | yes |
| `/device/sibling/<k>/selected` | int bool | | yes (slow) | yes |
| `/primary/exists` | int bool | | yes | yes |
| `/primary/position` | int | `TrackFX_GetInstrument` | yes | yes |
| `/primary/name` | string ≤ 50 | | yes | yes |
| `/primary/param/count` | int | | yes | yes |
| `/eq/exists` | int bool | | yes | yes |
| `/eq/param/count` | int | | yes | yes |
| `/eq/band/<b>` | string int, b = 0..7; `-1` = disabled | ReaEQ `BANDTYPE<b>` if `BANDENABLED<b>` > 0 | yes (only if eq exists) | yes (`parse_int` → `EqualizerBandType::from_reaper_index`: 4 Lowcut, 0 Lowshelf, 8/9/2/5/7 Bell, 3 Highcut, 1 Highshelf, 6 Notch, else Off) |
| `/track/fx/param/count` | int | `CountTCPFXParms` of cursor track | yes | yes (+RebindKnobs) |
| `/primary`/`/eq` `bypass`, `window`, `expand`, `touchedParam`; `/eq/name`, `/eq/position` | | generic device fields | **no** | yes |
| `/primary/count`, `/eq/count`, `/primary/sibling/*`, `/eq/sibling/*` | | | no | accepted, ignored (cursor only) |

#### Clip (cursor clip = first selected media item) and session

| Address | Value | Meaning | Host | Parser |
|---|---|---|---|---|
| `/clip/exists` | int bool | ≥ 1 selected item | yes | yes |
| `/clip/start` | double quarters (−1 none) | `bpm*itemStart/60` | yes | yes |
| `/clip/end` | double quarters (−1 none) | | yes | yes |
| `/clip/playposition` | double quarters (−1 outside) | | yes | yes |
| `/clip/loop` | int bool | `B_LOOPSRC` | yes | yes |
| `/clip/color` | colour | | yes | yes |
| `/clip/notes` | note list (MIDI take only; recomputed on `MIDI_GetHash` change) | | yes | yes |
| `/clip/all` | clip list (1.2) | all items of all visible tracks = slots | yes (session, on project change) | yes (`Model::set_clips`) |
| `/track/<n>/slot/count` | int | number of slots of the track | Bitwig (v2) | yes |
| `/track/<n>/slot/<s>/exists`, `hasContent`, `selected` | int bool | | Bitwig (v2) | yes |
| `/track/<n>/slot/<s>/name`, `color` | string, colour | | Bitwig (v2) | yes |
| `/track/<n>/slot/<s>/playing`, `recording`, `playQueued`, `recQueued`, `stopQueued` | int bool | the launcher states; reported states replace the derived ones | Bitwig (v2) | yes |
| `/track/<n>/isStopped` | int bool | no clip of the track plays | Bitwig (v2) | yes |

#### Scenes (= Reaper regions) and markers

| Address | Value | Meaning | Host | Parser |
|---|---|---|---|---|
| `/scene/count` | int | number of regions | yes (session) | yes (all scene banks) |
| `/scene/offset` | int | offset of the host's scene window; the host pages the scenes and slots (v2) | Bitwig (v2) | yes |
| `/scene/<n>/exists` | int (1) | | yes | yes |
| `/scene/<n>/number` | int = n | | yes | yes |
| `/scene/<n>/name` | string | empty → default name `Scene <n+1>` | yes | yes |
| `/scene/<n>/position` | double seconds | region start | yes | yes |
| `/scene/<n>/endPosition` | double seconds | region end | yes | yes |
| `/scene/<n>/color` | colour | | yes | yes |
| `/scene/<n>/select` | int bool | selected in the editor | Bitwig (v2) | yes |
| `/marker/count` | int | non-region markers (slow) | yes | yes |
| `/marker/<n>/exists` | int (1) | | yes | yes |
| `/marker/<n>/number` | int = n | | yes | yes |
| `/marker/<n>/name` | string | empty → `Marker <n+1>` | yes | yes |
| `/marker/<n>/color` | colour | | yes | yes |
| `/marker/<n>/position` | double seconds | | yes | **no-op** |
| `/marker/<n>/endPosition` | double seconds | | yes | **no-op** |

#### Browser, note repeat, groove, quantize

| Address | Value | Meaning | Host | Parser |
|---|---|---|---|---|
| `/browser/presetsfile` | string path | `TrackFX_GetUserPresetFilename` | yes | yes (if browser exists) |
| `/browser/selected/name` | string | current preset name | yes | **no-op** |
| `/browser/selected/index` | int | `TrackFX_GetPresetIndex` | yes | yes |
| `/browser/exists` | int bool | the host browser is open; enables the host browser | Bitwig (v2) | yes |
| `/browser/contentType/count`, `/browser/contentType/<i>/name` | int, string | content types (tabs) | Bitwig (v2) | yes |
| `/browser/contentType/selected` (`/name`) | int, string | selected content type; index 1 = presets | Bitwig (v2) | yes |
| `/browser/preview` | int bool | | Bitwig (v2) | yes |
| `/browser/column/count` | int | number of filter columns | Bitwig (v2) | yes |
| `/browser/column/<c>/exists`, `name`, `wildcard`, `cursor/exists`, `cursor/name`, `cursorIndex` | | a filter column, its selected item and the index of it on the item page (-1 if not on it) | Bitwig (v2) | yes |
| `/browser/column/<c>/item/<i>/exists`, `name`, `select`, `hits` | | the item page of a filter column | Bitwig (v2) | yes |
| `/browser/result/<i>/exists`, `name`, `select` | | the result page | Bitwig (v2) | yes |
| `/browser/result/cursor/name` | string | the selected result | Bitwig (v2) | yes |
| `/drum/exists` | int bool | the cursor track has a drum machine | Bitwig (v2) | yes (all drum devices) |
| `/drum/hasMutedPads`, `/drum/hasSoloedPads` | int bool | | Bitwig (v2) | yes |
| `/drum/pad/<p>/exists`, `select`, `active`, `mute`, `solo` | int bool | a pad exists if it has a device | Bitwig (v2) | yes |
| `/drum/pad/<p>/name`, `color` | string, colour | reported colours are not replaced with the track colour | Bitwig (v2) | yes |
| `/drum/pad/<p>/volume`, `pan` (`/str`) | double 0..1, string | | Bitwig (v2) | yes |
| `/drum/name`, `bypass`, `window`, `expand`, `hasDrumPads`, `hasLayers` | | generic device fields | Bitwig (v2) | yes |
| `/primary/hasDrumPads`, `/primary/hasLayers` | int bool | the first instrument is a drum machine / has layers; without pads but with layers the layer modes show the layers (default: pads) | Bitwig (v2) | yes |
| `/primary/pad/<p>/…`, `/primary/hasMutedPads`, `/primary/hasSoloedPads` | | as `/drum/…` | Bitwig (v2) | yes |
| `/primary/layer/count` | int | number of layers | Bitwig (v2) | yes |
| `/primary/layer/<i>/…` | | as `/drum/pad/<p>/…`; a layer exists if it has a device | Bitwig (v2) | yes |
| `/primary/page/count`, `/primary/page/<p>/name` | | as `/device/page/…` (no page selection) | Bitwig (v2) | yes |
| `/noterepeat/active` | int bool | MIDI arp plugin enabled on selected track input FX | yes | yes → `NoteRepeatActive` |
| `/noterepeat/period` | double (plugin rate param) | parser emits `NoteRepeatPeriod(1/v)` | yes | yes |
| `/noterepeat/notelength` | double | | yes | yes |
| `/noterepeat/mode` | int | arpeggiator mode index | yes | yes |
| `/noterepeat/velocity` | int bool (1 = use pressure; host inverts plugin param) | | yes | yes → `NoteRepeatUsePressure` |
| `/groove/active` | int | `swingmode` of `GetSetProjectGrid` | yes | yes → groove `Enabled` |
| `/groove/amount` | double | `swingamt` | yes | yes → groove `ShuffleAmount` |
| `/quantize/strength` | int | → `ParserEvent::QuantizeAmount` | **no** | yes |

## 4. Bank sizes and index ranges

| Area | Host sends | Core side |
|---|---|---|
| Tracks | all visible tracks, n = 0..count−1 (hidden tracks with `trackState & 1024` skipped → n ≠ Reaper track index) | pages locally; `ModelSetup::num_tracks` default 8 |
| Sends | all sends per track | `num_sends` default 8 |
| Device params (`/device`, `/primary`, `/eq`) | all `TrackFX_GetNumParams` | `num_params` default 8 per page, `num_param_pages` 8 |
| Track/master FX params | all TCP FX params (`CountTCPFXParms`) | parameter banks of selected track / project |
| Device siblings | exactly 8 (`Model::DEVICE_BANK_SIZE`), k = 1..8, window `deviceBankOffset = floor(sel/8)*8` | `num_devices_in_bank` default 8; parser drops k−1 ≥ device bank page size. If a controller uses < 8, extra siblings are lost; > 8 never filled. Window moved by core commands `device page/-`, `page/+`, `selected <k>` (device.rs) |
| EQ bands | 8 (b = 0..7) | `EQ_NUMBER_OF_BANDS` = 8 |
| Markers | all non-region markers | `num_markers` default 0 (?) per controller |
| Scenes | all regions | `num_scenes` default 8; applied to every scene bank |
| Clip slots | all items per track in `/clip/all` | slot banks set to the max clip count |

Who decides: the host never pages (sends everything flat, except device siblings); the core pages internally. There is **no message** by which the core tells the host a bank size; the only fixed coupling is the sibling window size 8 (host constant).

## 5. Portability: what a host has to implement

A new host (Zynthian, another DAW) does not have to implement everything. Every callback may be
NULL, unknown processors and commands are ignored, and the core keeps working with the addresses
it gets.

**Required** for a working controller:

- MIDI: the port callbacks and `dbm_on_midi_event`.
- `refresh`: resend the full state. Without it the controllers start with an empty model.
- The model updates of the areas the host has: transport, tracks (with sends), master, cursor
  device with parameters, and the commands that change them (`track`, `master`, `device`, `play`,
  `stop`, `record`, `repeat`, `time`, `tempo`).

**Optional:** clips and scenes (session), markers, browser, EQ, note repeat, groove, the track and
master FX parameter banks, undo and redo. `enable_updates` and `delay_updates` are optimizations
only; a host that ignores them sends all data all the time.

**Reaper-specific** parts. A non-Reaper host can leave them out or map them; if it ignores them,
the feature does nothing:

All commands are fire-and-forget; the Rust core never gets a result. If the host ignores a command,
the model only changes if the setter also updates local state (e.g. mute/solo state, transport
position, loop start/length, preroll measures, clip start/end, device `is_expanded`); otherwise the
controller display stays at the last value the host reported. No error surfaces.

| Part | Rust users | If the host ignores it |
|---|---|---|
| ~~`action` with Reaper command IDs~~ (replaced by commands in protocol v2, see §2; the former list: `src/daw/actions.rs`: `PROJECT_TAB_PREVIOUS/NEXT`, `PROJECT_SAVE/LOAD`, `TRANSPORT_TAP_TEMPO`, `REMOVE_ITEMS`, `RECORD_MODE_NORMAL/PUNCH_ITEMS/AUTO_PUNCH`, `RECORD_PREROLL`, `TOGGLE/ENABLE/DISABLE_METRONOME`, `DUPLICATE_TRACKS`, `LOAD_WINDOW_SET_1..4`, `SHOW_ALL_ACTIVE_ENVELOPES`, `TOGGLE_MIXER_VISIBLE`, `TOGGLE_SHOW_MIDI_EDITOR_WINDOWS`, `SHOW_TRACK_MANAGER_WINDOW`, `TOGGLE_FULLSCREEN`, `TOGGLE_MEDIA_EXPLORER`, `TOGGLE_FX_INSERTS/SENDS/PARAMETERS`, `DYNAMIC_SPLIT`, `GO_TO_NEXT/PREV_TRACK`, `GO_TO_NEXT/PREV_MARKER`, `ZOOM_*`, `TOGGLE_FOLLOW_PLAYBACK`, `TOGGLE_MARKER_LANE`, `CYCLE_TRACK_ZOOM`, `GO_TO_END_OF_PROJECT`) | `transport.rs`, `project.rs`, `application.rs`, `marker.rs`, `channel.rs` | Those features do nothing. Note: metronome on/off, tap tempo, preroll, punch modes, track duplicate, marker prev/next and project save/load exist **only** as action IDs - a portable host must map these IDs or the core needs real commands. `INSERT_NEW_TRACK_FROM_TEMPLATE` is defined but unused. |
| `action ""` string (named commands) | `application.rs` | User-configured Reaper actions do nothing. |
| `action select` (action list prompt) | host only | - |
| `inifile reaper/prerollmeas` | `transport.rs` | Preroll length not written; local `preroll_measures` still updated. |
| `metro_vol` (implemented as Reaper action 999 via `KBD_OnMainActionEx`) | `transport.rs` | Semantics portable (0..127 / +/-), impl Reaper-specific. Ignored -> no metronome volume. |
| `project engine` (`Audio_Init`/`Audio_Quit`) | `application.rs` | Audio engine toggle does nothing. |
| `tempo +/++/-/--` (fixed 0.1 / 1 BPM actions) | `transport.rs` | Relative tempo does nothing; absolute `tempo ""` double still portable. |
| `automation` (Reaper global override numbering 0..5) | `transport.rs` | Automation mode unchanged. |
| `quantize` (MIDI editor actions) | `cursor_clip.rs` | No quantize. |
| `track <t>/active` (lock + FX offline + mute actions on selected tracks) | `channel.rs` | No track activation. |
| `track <t>/inQuantResolution` (MIDI input quantize actions) | `track.rs` | No record quantization. |
| `track <t>/param/<p>/value`, `master param/<p>/value` (TCP-visible FX params via `GetTCPFXParm`) | `parameter.rs` | Cursor-track/project parameter banks do nothing. |
| `eq` (ReaEQ: `TrackFX_GetEQ`, `BANDTYPE`/`BANDENABLED`, Reaper band type numbers) | `device.rs`, `channel.rs` | EQ mode does nothing. |
| `device add/<pos>`, `addTrack` device list (Reaper FX creation names for `TrackFX_AddByName`) | `channel.rs`, `track_bank.rs` | Names are meaningless elsewhere; device add fails. |
| `device window`, `device expand` (floating FX window / FX chain window) | `device.rs` | No window toggling. |
| `noterepeat` (Reaper arpeggiator JSFX in input FX) | `midi.rs` | Note repeat does nothing. |
| `scene` = regions, `marker` = project markers, `clip` = selected media item, slots = media items of a track (`track <t>/clip/<item>/...`) | `scene.rs`, `marker.rs`, `cursor_clip.rs`, `slot.rs` | Mapping is Reaper's arrangement model; other hosts must map or ignore. |
| `track <t>/clip/<item>/insertFile`, `clip insertFile` (`InsertMedia`) | `slot.rs` | No file insert. |
| `track <t>/pin` (stores `pinnedTrackIndex` for the host's own data collection) | `model.rs` | Pinning works only locally. |
| `track <t>/overdub` (`I_RECMODE` 7/8) | `model.rs` | No overdub toggle. |
| `delay_updates` / `enable_updates` | see 1.3 | Pure host optimizations: ignoring them means the host always sends all data; risk of stale values overwriting fresh ones while automation recording (no 300 ms hold-off). |
| `refresh` | `app.rs` | Host must resend full state after controllers start; ignoring breaks initial display. Not Reaper-specific - required. |
| `cursor` (`CSurf_OnArrow`) | host only | - |

**Todo (from the bindings plan):** turn the Reaper-specific parts into optional capabilities
that a backend declares, e.g. a capability flag set passed to `dbm_create`. The action IDs are
replaced by real commands (protocol v2, §2).

## 6. Known mismatches and issues

Found while writing this document. S1, S3, S4 and S5 are fixed (2026-10-29); the rest is open. The "Rust" column of the tables in §2 and
the "Host"/"Parser" columns in §3 show the same information per command and address.

### Commands

#### Not handled at all (silently ignored)

| # | Processor / command | Type | Rust file | Note |
|---|---|---|---|---|
| 1 | `master` `user/param/0/value` | double | `parameter.rs` (`ParameterKind::MasterCrossfader`) | Master has no `user` branch. Same in Java (`MasterTrackImpl`). Always ignored. |
| 2 | `master` `pin` | int | `model.rs` | Only if selected track is master (?). Master int handles only select/solo/mute. |
| 3 | `master` `name` | string | `track.rs` | If invoked on master track. Master string handles only `color`. |
| 4 | `master` `isGroupExpanded` | int | `track.rs` | If invoked on master (?). |
| 5 | `master` `inQuantResolution` | double | `track.rs` | If invoked on master (?). |
| 6 | `master` `createClip` | int | `track.rs` | If invoked on master (?). |
| 7 | `master` `recordClip` | none | `track.rs` | Master no-arg overload is empty. |
| 8 | `master` `movePrev` / `moveNext` | none | `track.rs`, `model.rs` | Master no-arg overload is empty. |
| 9 | `master` `remove` | none | `channel.rs` | Master no-arg overload is empty (deleting master is impossible anyway). |

(`recarm`, `monitor`, `autoMonitor`, `toggleMultiSelect`, `active` are guarded with `is_master()` /
`ChannelKind::Master` in Rust and never reach `master`.)

#### Handled, but with different semantics

| # | Command | Rust file | Issue |
|---|---|---|---|
| a | `track <t>/select` int 0 | `track_bank.rs` (`send_page_selection`) | Host ignores the value: "deselect" selects the old track; the following `select 1` then selects the new one. Net effect OK, extra work. |
| b | `quantize ""` double amount | `cursor_clip.rs` | Host ignores amount, always runs the MIDI-editor quantize action. |
| c | `automation ""` int 5 | `transport.rs` | Rust means LatchPreview; Reaper `SetGlobalAutomationOverride(5)` = bypass. Same in Java (?). |
| d (fixed) | `device param/<p>/value` from FirstInstrument / Drum device banks | `device.rs`, `parameter.rs` | Always applied to the cursor (selected) device, not the first instrument (?). Fixed in protocol v2: the first instrument uses the `primary` processor. |
| e | `eq selected` int | `device.rs` | Inherited DeviceProcessor: moves the normal cursor device, not EQ-specific (?). |
| f (fixed) | Drum pad channel commands (`ChannelKind::DrumPad` -> `track <padPosition>/mute`, `/solo`, `/volume`, `/color`, ...) | `channel.rs` | Pads are virtual; commands hit the *track* with the pad's position. Same in Java (`DrumPadImpl` uses `Processor.TRACK`) (?). Fixed in protocol v2: the `drum` processor. |
| g | Master send parameters | `channel.rs` (`SendBank::new(.., position)` with master position 0) | Would be sent as `track 0/send/<s>/volume` (track 0, not master); only if master sends exist (?). |
| h | `time loop/length` | `transport.rs` | Host sets loop end (not a length); consistent with what the host reports, just misnamed. |

Count: 9 unhandled command entries (#1 always; #2-#9 only when invoked on the master track) +
8 semantic differences (a-h).

### Model updates

#### Host sends, parser ignores (3)

| # | Address | Parser behaviour |
|---|---|---|
| H1 | `/browser/selected/name` | explicit no-op |
| H2 | `/marker/<n>/position` | explicit no-op |
| H3 | `/marker/<n>/endPosition` | explicit no-op |

Conditional drops (not counted): `/primary/*`, `/eq/*` if not enabled in `ModelSetup`; `/master/fx/param/*` if the project has no parameter bank; `/browser/*` if no browser; `/automode` values −1 and 6.

#### Parser handles, host never sends (6)

| # | Address |
|---|---|
| P1 | `/stop` |
| P2 | `/track/<n>/active` |
| P3 | `/track/<n>/inQuantLengthEnabled` (Track.cpp has unused `INPUT_QUANTIZE_PATTERN`) |
| P4 | `/track/<n>/inQuantResolution` |
| P5 | `/quantize/strength` |
| P6 | generic device fields for `/primary` and `/eq`: `bypass`, `window`, `expand`, `touchedParam`, and `/eq/name`, `/eq/position` |

#### Semantic / host-side issues (5)

| # | Issue |
|---|---|
| S1 (fixed) | `/click/preroll` is emitted twice per update with different meanings: `GetToggleCommandState(41819)` (0/1) and config var `preroll` (bitfield). Parser uses `(v & 2) > 0`, so the toggle line always yields false and the two lines can contradict each other. Fix: the toggle line is removed; the bit field carries the same state. |
| S2 | `/time/loop/length` carries `endOut` (loop **end** time), not `endOut - startOut`; parser stores it as length (`set_loop_length_value`). (?) intentional |
| S3 (fixed) | `/time/loop/length/str` is stored by the parser as beat text (`set_loop_length_beat_text`, any sub-part) and then overwritten by `/time/loop/length/beat`; `Transport::set_loop_length_text` is never called. Fix: `str` sets the time text, as for the loop start. The Java parser has the same bug. |
| S4 (fixed) | `Parameter::ClearData` assigns the int result of `CollectIntValue` to `this->addressNumberOfSteps` (a `std::string`): the steps address is replaced by one char (value −1 → `'\xFF'`), producing invalid lines (`"\xFF -1"`, logged as unhandled) for that track/master FX parameter afterwards. Fix: the result goes to `numberOfSteps`. |
| S5 (fixed) | `CollectSessionData` returns early when `GetProjectStateChangeCount` is unchanged, even with `dump=true`. A `refresh` (e.g. after `dbm_restart_controllers` / `add_controller`) therefore does not resend `/clip/all` or `/scene/*`; newly started instances lack slots and scenes until the project changes. Fix: the early return is skipped on a dump. |
