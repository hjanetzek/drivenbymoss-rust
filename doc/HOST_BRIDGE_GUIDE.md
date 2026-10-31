# Connecting a DAW to the DrivenByMoss core: host bridge guide

For someone who wants to run the DrivenByMoss controllers of the Rust core in another DAW
(Ableton Live, Studio One, Ardour, a custom host). The DAW side is a **bridge**: it loads the
core, feeds it the DAW state and the controller's MIDI, and executes the commands the core sends
back. The reference is the **Bitwig bridge** (`backends/bitwig`, Java, FFM binding). It is the
most complete host: since the core port of the Bitwig classes (BITWIG_CORE_PORT_PLAN.md) the
core follows the Java DrivenByMoss for Bitwig wherever the host reports what Bitwig has. The
Reaper bridge (`backends/reaper`, C++) speaks the older, minimal variant of the same protocol.

As of 2026-10-31. Compiled from the code: `include/drivenbymoss.h`, `src/ffi.rs`, `src/app.rs`,
`src/host.rs`, `src/backend.rs`, `src/midi.rs`, `src/parser.rs`, `src/daw/`, and the bridge
classes in `backends/bitwig/src/main/java/de/mossgrabers/dbm/bitwig/` (`NativeCore`,
`DbmExtension`, `BitwigModel`, `ModelReporter`, `CommandHandler`, `SpecificDevice`,
`EqualizerDevice`, `BitwigNoteClip`, `BitwigBrowser`, `HostSettings`, `ProtocolRecorder`).
Points not confirmed from the code are marked **(unverified)**. Related documents:
[BACKEND_PROTOCOL.md](BACKEND_PROTOCOL.md) (the protocol from the Reaper side, with the Reaper
processors), [BITWIG_DIVERGENCES.md](BITWIG_DIVERGENCES.md) (where Bitwig + core differ from the
Java version), [CONFORMANCE_TESTING_PLAN.md](CONFORMANCE_TESTING_PLAN.md).

Notation: `<p>` absolute track position (0-based), `<s>` slot/scene position or send index,
`<i>` index, "flag" = `1`/`0` (the parser takes any value > 0 as true). "Bitwig:" names the API
value the reference bridge reads or calls.

## 1. Architecture

```
 DAW  <-- host API -->  bridge  <-- C ABI (drivenbymoss.h) -->  core (one DbmApp per controller)
                          |  dbm_update_model("addr value\n...")  -->  parser -> model
                          |  dbm_on_midi_event(bytes)              -->  controller surface
                          |  dbm_tick()  (every 30 ms)             -->  tasks, modes, LEDs/displays
                          |  <-- process_*_arg(processor, command, value)   commands
                          |  <-- send_midi_data / process_midi_arg          MIDI out / "played" notes
                          |  <-- set_note_input_*                           note inputs
```

- The core never asks for values. The bridge pushes the DAW state as text lines; the core sends
  fire-and-forget commands; their effect comes back as reported values.
- **One instance per controller.** Create it with `dbm_create_controller(callbacks,
  controller_id, config_path, app_version, flags)`: exactly one controller, its MIDI input and
  output are device ID `0`, no main configuration, no port discovery (`get_midi_inputs/outputs`
  are not used). `controller_id` is the UUID from `dbm_list_controllers()` (JSON: `uuid`,
  `vendor`, `model`, `numMidiInPorts`, `numMidiOutPorts`, `discovery`). Returns null if the
  controller is not supported. (`dbm_create` is the Reaper variant: several controllers from
  `DrivenByMoss4Reaper.config`, ports found by name through `get_midi_inputs/outputs`.) The
  Bitwig bridge generates one extension definition per controller from the catalog
  (`tools/gen_definitions.py`).
- **Threading.** All calls on one instance are serialized by a mutex. Callbacks run on the
  thread that called into the core, *during* that call; calling any `dbm_*` of the same
  instance from a callback deadlocks. Panics are caught. Strings passed in either direction are
  only valid during the call; strings returned by the core are released with
  `dbm_free_string` (not `dbm_version`).
- **Tick.** Pass `DBM_FLAG_HOST_TICK` and call `dbm_tick` every 30 ms from the DAW's thread
  (both bridges do). Without the flag the core starts its own timer thread, and callbacks then
  also arrive on that thread. `dbm_tick` runs the due scheduled tasks, the delayed controller
  startup, `handle_changes` and `flush` (all LED/display output).
- **Time base.** The C ABI has no time parameter. At every entry (`tick`, `update_model`,
  `on_midi_event`, settings calls, `start_infrastructure`) the core takes `Instant::now()` as
  `Host::now`; all core timing (scheduled tasks such as the 75 ms selection after a page change,
  the 1 s startup delay, long presses, notification lifetimes, blinking) uses this time. So
  timing resolution is your tick interval. A replay sets a fixed time instead
  (`App::set_time`, Rust API only, §8).
- **Commands may arrive during any call**, not only during `tick` (e.g. `bank track <offset>`
  while you report `/track/offset`, LED updates are only in `tick`). The Bitwig bridge executes
  them right away in the callback (`CommandHandler`); the Reaper bridge queues them and runs the
  queue after `tick`. Both are fine as long as the callback does not call back into the core.

### Startup and shutdown (Bitwig sequence, `DbmExtension.init/exit`)

1. Load the library, `dbm_create_controller(..., DBM_FLAG_HOST_TICK)`. `app_version` is free
   text (Bitwig: `getHostProduct() + " " + getHostVersion()`).
2. Install the MIDI input callbacks (short messages and sysex → `dbm_on_midi_event(app, 0, …)`).
3. `dbm_start_infrastructure`. During this call the core opens the ports
   (`open_midi_input(0)`, `open_midi_output(0)`, must return non-zero), creates its note inputs
   (`set_note_input_filters`, then key/velocity tables) and sends `refresh`. Bitwig can only
   create note inputs during `init`, so this call must happen there. The controller's `startup`
   (initial LEDs etc.) runs in the first `tick` 1 s later (`STARTUP_DELAY_MS`).
4. `dbm_get_model_setup` (empty before step 3) and create all DAW objects from it (below).
5. Settings: `dbm_get_settings` → build the DAW's settings UI; push the stored values with
   `dbm_set_setting(id, value)`; discard `dbm_take_setting_changes` once.
6. Every 30 ms: collect the state → `dbm_update_model(lines)` (skip if empty) → `dbm_tick` →
   apply `dbm_take_setting_changes` to the UI → show `dbm_take_notifications` (one per line) as
   popups.
7. Exit: `dbm_destroy` (stops the controller, which writes its final MIDI and the `.config`
   file). Not from within a callback. `close_midi_*` are called; Bitwig ignores them (the ports
   are Bitwig's).

Settings: `dbm_get_settings` returns one tab separated line per setting: ID, scope (`global` /
`document`), kind (`enum`, `integer`, `boolean`, `text`), category, label, default, current
value, then the enum options or integer min/max. `dbm_set_setting` changes are not echoed by
`dbm_take_setting_changes`, which returns `ID<tab>value` lines of changes made by the controller
(e.g. a scale chosen on the device). The bridge stores global settings in the DAW preferences and
document settings in the project (`HostSettings`). The alternative used by Reaper is
`dbm_set_default_document_settings` / `dbm_get_formatted_document_settings` /
`dbm_set_formatted_document_settings` (one opaque text per project). The core also keeps a
`<model>.config` file in `config_path` as a fallback.

### The model setup (`dbm_get_model_setup`)

JSON with the keys of the Java `ModelSetup` (`src/daw/model_setup.rs`). Hosts that must create
their objects up front (Bitwig) size them from it; a host that can query anything at any time can
ignore most of it, but the numbers tell you what the controller will address.

| Key | Default | Bitwig bridge creates |
| --- | --- | --- |
| `hasFlatTrackList`, `hasFullFlatTrackList` | true, false | `createTrackBank(..., true)` (with effect tracks and master) if `hasFullFlatTrackList`, else `createMainTrackBank`. The core adds its own master only if the host lists none. Hierarchical lists (`hasFlatTrackList` false) are not supported |
| `numTracks`, `numScenes`, `numSends` | 8, 8, 8 | the track window (with `numSends` sends and `numScenes` slots), its scene bank; plus a 400-track bank without sends/slots for commands to tracks outside the window |
| `numFxTracks` | -1 | nothing: **effect track banks are not supported yet** |
| `wantsDrumDevice`, `additionalDrumDevices`, `numDrumPadLayers` | true, [], 16 | one drum machine (Bitwig Drum Machine matcher) with all 128 pads; the core pages over them for every drum device |
| `numDevicesInBank` | 8 | ignored: always 8 siblings |
| `numParamPages`, `numParams` | 8, 8 | `numParamPages` remote-control page cursors of 8, each fixed to its page (flat list of `numParamPages*8`), plus one cursor that follows the page selected in the DAW; project remote controls the same way if `numParams > 0` |
| `numDeviceLayers` | 8 | ignored: 32 layers of the first instrument |
| `deviceIds` | [] | `"FIRST_INSTRUMENT"` → `/primary` (instrument matcher, params, 128 pads, 32 layers); `"EQ"` → `/eq` (first EQ+); `"NI_KOMPLETE"` not handled |
| `numFilterColumnEntries`, `numResults` | 16, 16 | popup browser with 7 filter columns (no smart collections), if `numResults > 0` |
| `noteClips` | [] | `[[steps, rows], …]`: one launcher cursor clip with a step grid per size, id `"<steps>x<rows>"` (only the sizes the views requested during setup; a later size has no grid) |
| `numMarkers`, `numListParams`, `wantsCursorLayer`, `wantsFocusedParameter`, `wantsClipLauncherNavigator` | 0, 0, false, false, false | not used yet |

## 2. The reporting protocol (host → core)

### Lines and values

`dbm_update_model` takes lines `"<address> <value>\n"`; the address ends at the first space, the
value is the rest (may contain spaces; replace line breaks by spaces). A leading `/` is optional.
Unknown addresses are logged as `ERROR: Unhandled …` and ignored.

| Kind | Format | Examples |
| --- | --- | --- |
| normalized | double 0..1 (Bitwig `value().get()`); Bitwig formats `%.6f` with `Locale.ROOT` | volume, pan, sends, params, VU |
| number | int or double; ints may be sent as `3.000000` | counts, positions, tempo (BPM), beats, seconds |
| flag | `1` / `0` (> 0 is true) | mute, exists, select |
| text | rest of the line, verbatim | names, displayed values |
| color | `r g b`, each 0..255 (decimal); `r < 0` = no color, parse errors become gray | `/track/0/color 255 128 0` |
| displayed value | `<value address>/str <text>`; shortened variants `<…>/str/6`, `<…>/str/8` | `/track/0/volume/str/6 -6.0` |

Note the asymmetry: reported colors are `r g b`, colors in **commands** are `RGB(r,g,b)`.

Existence: for parameters, sends and device siblings the **name** sets existence (empty name =
does not exist). Tracks, scenes, slots, pads and layers have an explicit `exists`.

### Change-only reporting and dumps (`ModelReporter`)

- Remember the last value per address and report only changes.
- On `refresh` (processor `refresh`, no arg) report **everything** with the next collection. The
  core sends it from `dbm_start_infrastructure` (and on restart). Start in dump mode anyway, so
  the first report is complete.
- Report items of the window with their **absolute** positions (`/track/<offset+i>/…`).
- After a value command, report the addressed item again even if unchanged (§6.1).
- The core parses a batch top to bottom, line by line. Report a selection change in one batch
  (old item `select 0`, new item `select 1`); reporting a `count` with its items is the safe
  order (the Bitwig bridge reports items first and the count after them).

### Effects of reports on the core

- A changed bank size (`/track/count`, `…/send/count`, `…/param/count`) re-activates the active
  mode (rebinds the knobs).
- A newly selected track or `/project/name` schedules a note mapping update 1 s later.
- `/track/<p>/select` is handled **only when it changes** (§6.2).

### Host features detected from reports (progressive support)

The core starts as a Reaper-like host and switches behaviour per feature when the host reports a
certain address. This is how another DAW can grow support step by step. Once switched on, a
feature stays on.

| Report | Feature in the core | What the bridge must then handle |
| --- | --- | --- |
| `/track/offset <any>` | **Host paging** (`ctx.is_host_paging`): bank offsets kept for the host, scrolling without selecting, the first item of the previous page is selected, scrolling back from any offset > 0, delayed selections after page changes (also of the device siblings). Without `/host/features` also all host features, see below | `bank track <offset>`; scroll the track window there. The value of `/track/offset` is ignored, the core keeps its own offset |
| `/host/features <names>` | The command forms the host understands (`ctx.host_supports`), see below | the listed command forms |
| `/scene/offset <any>` | Host paging of scenes (and the slots) | `bank scene <offset>` |
| `/automode/arranger`, `/automode/launcher` | Bitwig's two automation write switches + modes Read/Touch/Write/Latch instead of Reaper's modes | `automation toggleArranger\|toggleLauncher\|resetOverrides`, `automation <mode index>` |
| `/overdub/arranger`, `/overdub/launcher` | Arranger and launcher overdub switches (instead of the track's `overdub`) | `transport overdub/arranger\|overdub/launcher <0\|1>` |
| `/punchIn`, `/punchOut`, `/fill`, `/click/ticks` | Independent punch in/out, fill mode, metronome ticks | – (the commands come with the feature `transportCommands`) |
| `/recordQuantization`, `/recordQuantization/noteLength` | Global record quantization (instead of per track `inQuantResolution`) | `application recordQuantization <grid>`, `…/noteLength/toggle` |
| `/track/pinned <p or -1>` | The cursor track is the pinned track of the host | `track pinned <0\|1>`, `track pinned/toggle` |
| `/track/hasPrevious`, `/track/hasNext` | Cursor track navigation availability from the host | `track selectPrevious\|selectNext` |
| `/track/<p>/canHoldNotes`, `canHoldAudio` | Track capabilities (default: true) | – |
| `/track/<p>/playingnotes` | Playing notes of the cursor track light the pads | – |
| `/project/hasSolo`, `/project/hasMute` | Project-wide solo/mute state | `project unsoloAll` (with `hasSolo`), `project unmuteAll` (with `hasMute`) |
| `/project/param/…`, `/project/page/…` | Project remote controls (instead of a bank on the master FX) | `project param/<i>/…`, `project parameterPage <page>` |
| `/project/cueVolume`, `/project/cueMix` | The cue parameters exist | `project cueVolume\|cueMix` (+ `/inc`, `/reset`, `/touch`) |
| `/groove/<name>` (six names) | Bitwig's six groove parameters (instead of `groove active\|amount`, −1..1) | `groove <name>` (+ suffixes) |
| `<…>/str/6`, `<…>/str/8` | Host-shortened texts for these lengths (other lengths: the core cuts) | – |
| `/device/page/selected`, `…/page/count`, `…/page/<i>/name` | Named pages; the page selected in the host is shown, also after a device change | `device\|primary\|project parameterPage <page>` |
| `/drum/pad/<i>/…`, `/device\|primary/pad/<i>/…` | The pads come from the host, the core pages over all of them | `drum\|primary pad/<i>/…` |
| `/browser/exists` | The host's browser (content types, filter columns, results) | `browser …` (§4.9) |
| any of `/clip/loopStart\|loopLength\|shuffle\|accent\|pinned` | **Host step grid** of the note clips (instead of Reaper's note list) | `clip grid/<id>/…`, `clip start\|end\|loopStart\|loopLength/inc` |
| `/device/pinned` | Pinnable cursor device (Reaper: always pinned) | `device pinned <0\|1>` |
| `/track/<p>/active`, `/master/active` | Activation state from the host (also makes `set_skip_disabled_items` work); `/master/active` lets the core (de)activate the master | `track <p>/active`, `master active` |
| `/track/<p>/vuholdleft\|vuholdright` | The VU clip state is the hold value above 0 dB (Reaper); without them it is `vuleft\|vuright >= 16241/16384` (Bitwig) | – |

### Host features (`/host/features`)

Command forms which cannot be detected from a report are switched on by one line which lists the
features the host understands (space separated, unknown names are ignored, a new line replaces
the earlier list; `HostFeature` in `src/daw/context.rs`):

```
/host/features valueInc valueReset immediately indication soloToggle transportCommands deviceCommands layoutStep
```

Report it with the first report and with every dump (the Bitwig bridge does). Without a feature
the core sends the basic (Reaper) form of the command or nothing. **Compatibility:** a host which
reports `/track/offset` but never `/host/features` supports all features (the Bitwig extension
before it reported its features); a host which reports neither (Reaper) none. So a new host which
pages should report `/host/features`, even an empty list.

| Name | What it enables in the core | Commands (instead of) |
| --- | --- | --- |
| `valueInc` | Relative value changes (knobs, `inc`); the core does not change the value itself and shows the host's values also while a control is touched (Reaper: held back until released). Not for the Reaper groove (`groove active\|amount`) | `<value command>/inc <normalized delta>` (absolute `<value command> <0..1>`) |
| `valueReset` | Resets to the DAW's default (track/master/layer/pad volume and pan, sends, device and project parameters, cue, groove) | `<value command>/reset`, no value (the core's default as absolute value) |
| `immediately` | Values without the host's takeover mode (pads, buttons) | `<value command>/immediately <0..1>` (`<value command> <0..1>`) |
| `indication` | Indication of volume, panorama and sends, and of the clip launcher of the track bank (device parameters are always indicated) | `track <p>/volume\|pan/indicate`, `master volume\|pan/indicate`, `track <p>/send/<s>/volume/indicate`, `track indicate <0\|1>` (nothing) |
| `soloToggle` | Solo toggles with the host's exclusive solo preference | `track <p>/solo/toggle`, `master solo/toggle` (`solo <0\|1>`) |
| `transportCommands` | Bitwig's transport commands, the metronome volume of the transport | `transport play\|stop\|restart\|stopAndRewind` (the `play`/`stop`/`record` processors, `time 0`), `transport togglePunchIn\|togglePunchOut\|toggleFill\|toggleMetronomeTicks`, `transport punchIn\|punchOut <0\|1>` without the Reaper punch mode, `transport fill\|metronomeTicks <0\|1>` (nothing), `transport metronomeVolume[/inc]` with the default 0.8 (`metro_vol`, `metro_vol +\|-`) |
| `deviceCommands` | Commands to the device itself instead of the sibling index | `device\|primary\|drum\|eq window/toggle\|expand/toggle\|bypass\|remove\|duplicate\|select`, `device sibling/<i+1>/bypass\|remove\|duplicate\|select` (`device <index+1>/…`, `device window <0\|1>`), `drum\|primary pad/<p>/select` (nothing) |
| `layoutStep` | Previous/next panel layout of the host | `application layout/-\|+` (`application layout <index>`) |

Features detected from their own reports need no name (table above): paging, the automation and
overdub switches, punch/fill/ticks state, pinned track, cursor track neighbours, project solo and
mute, playing notes, the host step grid, shortened texts, activation, VU hold.


## 3. Address reference (host → core)

Columns: **Need**: R = required for a working controller, Rec = recommended, O = optional /
feature. **Bitwig** = what the reference bridge reports. "–" in Bitwig = not reported by the
Bitwig bridge (Reaper only, or nobody).

### 3.1 Transport

| Address | Value | Bitwig | Need |
| --- | --- | --- | --- |
| `/play` | flag | `isPlaying` | R |
| `/stop` | flag, 0 = playing | – (Reaper) | O |
| `/record` | flag | `isArrangerRecordEnabled` | R |
| `/repeat` | flag (loop) | `isArrangerLoopEnabled` | Rec |
| `/tempo` | BPM | `tempo().value().getRaw()` | R |
| `/numerator`, `/denominator` | int > 0 | `timeSignature()` | Rec |
| `/time` | play position in seconds | position × 60 / tempo | Rec |
| `/time/text` | formatted time, used verbatim | `TimeFormat.formatTimeLong` | Rec |
| `/time/str` | time text formatted by the core | – (Reaper) | O |
| `/time/hzoom` | zoom resolution | `1 / getContentPerPixel` | O |
| `/time/loop/start`, `/time/loop/length` (+ `/str`, `/beat`) | seconds, texts | – (Reaper) | O |
| `/beats/position` | quarters (the position to move from) | `playStartPosition` | Rec |
| `/beats/text` | formatted position, verbatim | `getPosition().getFormatted(BEAT_POSITION)` | Rec |
| `/beats/loop/start`, `/beats/loop/length` | quarters | `arrangerLoopStart`, `arrangerLoopDuration` | O |
| `/beats/loop/start/text`, `/beats/loop/length/text` | verbatim | `getFormatted` | O |
| `/beat` | beat text | – (Reaper) | O |
| `/click` | flag | `isMetronomeEnabled` | Rec |
| `/click/preroll` | bit field, 2 = metronome during pre-roll | `isMetronomeAudibleDuringPreRoll ? 2 : 0` | O |
| `/click/prerollMeasures` | int | – | O |
| `/click/volume` | 0..1 | `metronomeVolume().get()` | O |
| `/click/volumeStr` | text | `displayedValue().getLimited(6) + " dB"` | O |
| `/click/ticks` | flag (feature) | `isMetronomeTickPlaybackEnabled` | O |
| `/punchIn`, `/punchOut`, `/fill` | flag (feature) | `isPunchIn/OutEnabled`, `isFillModeActive` | O |
| `/followPlayback` | flag | `arranger.isPlaybackFollowEnabled` | O |
| `/automode` | 0 TrimRead, 1 Read, 2 Touch, 3 Write, 4 Latch, 5 LatchPreview | 1 if no switch on, else the index of `automationWriteMode` | Rec |
| `/automode/arranger`, `/automode/launcher` | flag (feature) | `isArranger/ClipLauncherAutomationWriteEnabled` | O |
| `/overdub/arranger`, `/overdub/launcher` | flag (feature) | `isArranger/ClipLauncherOverdubEnabled` | O |
| `/noterepeat/active`, `/noterepeat/period` (1 / period in beats), `/noterepeat/notelength`, `/noterepeat/mode` (0 down, 1 up, 2 down-up, 3 up-down), `/noterepeat/velocity` (flag: pressure → velocity) | | the arpeggiator of the default note input | O |
| `/quantize/strength` | int | – (Reaper) | O |

### 3.2 Application and project

| Address | Value | Bitwig | Need |
| --- | --- | --- | --- |
| `/project/canUndo`, `/project/canRedo` | flag | `application.canUndo/canRedo` | Rec |
| `/host/features` | space separated feature names (§2 "Host features") | all features, constant | Rec with host paging |
| `/project/engine` | flag | `hasActiveEngine` | O |
| `/project/name` | text (also schedules a note mapping update) | – | O |
| `/project/isDirty` | flag | – | O |
| `/project/hasSolo`, `/project/hasMute` | flag (feature) | `hasSoloedTracks`, `hasMutedTracks` | O |
| `/project/cueVolume`, `/project/cueMix` (+ `/str`, `/str/6\|8`, `/name`) | 0..1 (feature) | `project.cueVolume/cueMix` | O |
| `/project/param/<i>/…`, `/project/param/count`, `/project/page/count\|selected`, `/project/page/<i>/name` | as device params (§3.5) | root track group remote controls | O |
| `/layout` | `ARRANGE`, `MIX`, `EDIT`, `PLAY` | `panelLayout` | O |
| `/arranger/<flag>`: `cueMarkers`, `doubleRowTrackHeight`, `clipLauncher`, `timeline`, `ioSection`, `effectTracks` | flag | `arranger.*` | O |
| `/mixer/<flag>`: `clipLauncherSection`, `crossFadeSection`, `deviceSection`, `ioSection`, `meterSection`, `sendSection` | flag | `mixer.*` | O |
| `/recordQuantization` | `OFF`, `1/32`, `1/16`, `1/8`, `1/4` (feature) | `recordQuantizationGrid` | O |
| `/recordQuantization/noteLength` | flag | `recordQuantizeNoteLength` | O |
| `/groove/<name>` with `<name>` = `enabled`, `shuffleAmount`, `shuffleRate`, `accentAmount`, `accentRate`, `accentPhase` (+ `/name`, `/str`, `/str/6\|8`) | 0..1 (feature) | `createGroove()` parameters | O |
| `/groove/active`, `/groove/amount` | flag, −1..1 | – (Reaper) | O |
| `/action/select` | text (Reaper configuration dialog) | – | – |

### 3.3 Tracks, master, sends

Track addresses are `/track/<p>/<field>` with the absolute position. Bitwig reports the window
(`offset .. offset + numTracks`, stopping at the count).

| Address | Value | Bitwig | Need |
| --- | --- | --- | --- |
| `/track/count` | number of all tracks | `trackBank.itemCount` | R |
| `/track/offset` | window offset (feature: host paging) | `scrollPosition` | Rec |
| `/track/pinned` | position or −1 (feature) | `cursorTrack.isPinned ? position : -1` | O |
| `/track/hasPrevious`, `/track/hasNext` | flag (feature) | `cursorTrack.hasPrevious/hasNext` | O |
| `/track/<p>/exists` | flag | 1 for each reported position | R |
| `/track/<p>/number` | position (ignored for the master) | position | R |
| `/track/<p>/name` | text | `name` | R |
| `/track/<p>/type` | `AUDIO`, `INSTRUMENT`, `HYBRID`, `GROUP`, `GROUP_OPEN`, `EFFECT`, `MASTER`, `LAYER`, `CUE`, `UNKNOWN` | `GROUP` if `isGroup`, else `trackType` upper-cased (`HYBRID` if empty) | R |
| `/track/<p>/color` | `r g b` | `color` | R |
| `/track/<p>/select` | flag, only changes count (§6.2) | the position of a second, never pinned cursor track; the old one is reported `0` | R |
| `/track/<p>/volume`, `/track/<p>/pan` | 0..1 | `volume/pan().value()` | R |
| `/track/<p>/volume/str`, `/pan/str` (+ `/str/6\|8`) | text | `displayedValue()`, `getLimited(6\|8)` | R (`/str`), O (limited) |
| `/track/<p>/mute`, `solo`, `recarm` | flag | `mute`, `solo`, `arm` | R |
| `/track/<p>/active` | flag (feature) | `isActivated` | Rec |
| `/track/<p>/monitor` | flag (effective monitoring) | `isMonitoring` | O |
| `/track/<p>/autoMonitor` | flag | `monitorMode == AUTO` | O |
| `/track/<p>/isGroupExpanded` | flag | `isGroupExpanded` | O |
| `/track/<p>/depth` | int | – (Reaper; Bitwig: flat, 0) | O |
| `/track/<p>/canHoldNotes`, `canHoldAudio` | flag (feature, default true) | `canHoldNoteData/AudioData` | O |
| `/track/<p>/vu`, `vuleft`, `vuright` | 0..1 (`vu` = average) | VU observers with range 16384 | Rec |
| `/track/<p>/vuholdleft`, `vuholdright` | 0..1 | – (Reaper) | O |
| `/track/<p>/isStopped` | flag | `isStopped` | O |
| `/track/<p>/overdub`, `inQuantLengthEnabled`, `inQuantResolution` | flag / beats | – (Reaper per-track variants) | O |
| `/track/<p>/playingnotes` | `sel:mute:start:end:channel:pitch:velocity` per note, `;` separated (feature) | `cursorTrack.playingNotes()` as `0:0:0:0:0:<pitch>:<velocity>`, on the cursor track's position only | Rec |
| `/track/<p>/send/count` | int | `min(sendBank.itemCount, numSends)` | Rec |
| `/track/<p>/send/<s>/name` | text, empty = no send | `send.name` | Rec |
| `/track/<p>/send/<s>/volume` (+ `/str`, `/str/6\|8`) | 0..1 | `send.value`, `displayedValue` | Rec |
| `/track/<p>/send/<s>/active` | flag | `isEnabled` | O |
| `/track/<p>/send/<s>/color` | `r g b` | `sendChannelColor` | O |
| `/track/fx/param/<i>/…`, `/track/fx/param/count` | params of the selected track's FX | – (Reaper) | O |
| `/master/<field>` | same fields as a track (`name`, `color`, `select`, `volume`, `pan` + texts, `mute`, `solo`, `active`, `vu*`) | `createMasterTrack(0)` | Rec |
| `/master/fx/param/<i>/…`, `/master/fx/param/count` | master FX (also the crossfader from param 0) | – (Reaper) | O |

### 3.4 Clip launcher: slots and scenes

| Address | Value | Bitwig | Need |
| --- | --- | --- | --- |
| `/scene/count` | int | `sceneBank.itemCount` | Rec |
| `/scene/offset` | (feature: host scene paging) | `scrollPosition` | Rec with host paging |
| `/scene/<s>/exists`, `number`, `name`, `color`, `select` | | `exists`, position, `name`, `color`, `addIsSelectedInEditorObserver` | Rec |
| `/scene/<s>/position`, `endPosition` | beats (Reaper regions) | – | O |
| `/track/<p>/slot/count` | int (all scenes) | scene count | Rec |
| `/track/<p>/slot/<s>/exists`, `hasContent`, `name`, `color`, `selected`, `playing`, `recording`, `playQueued`, `recQueued`, `stopQueued` | flags, text, `r g b` | the `ClipLauncherSlot` values; group tracks report the track color | Rec |

Note the slot selection field is `selected`, the scene one `select`.

### 3.5 Cursor device, specific devices, parameters

`/device` is the cursor device, `/primary` the first instrument of the cursor track, `/eq` the
first EQ, `/drum` the drum machine of the cursor track (parsed by its own code: only `exists`,
`name`, `bypass`, `window`, `expand`, `hasDrumPads`, `hasLayers`, `hasMutedPads`,
`hasSoloedPads`, `pad/…`).

| Address | Value | Bitwig | Need |
| --- | --- | --- | --- |
| `/device/count` | number of siblings (cursor only) | `siblings.itemCount` | Rec |
| `/<dev>/exists` | flag | `exists` | R |
| `/<dev>/name` | text | `name` | R |
| `/device/position`, `/primary/position` | int, −1 = none | cursor: `position` | Rec |
| `/<dev>/bypass` | flag (1 = bypassed) | `!isEnabled` | Rec |
| `/<dev>/window`, `/<dev>/expand` | flag | `isWindowOpen`, `isExpanded` | O |
| `/device/pinned` | flag (feature) | `isPinned` | O |
| `/device/sibling/<n>/name`, `bypass`, `position`, `selected` | `<n>` is **1-based** (1..8); empty name = none | sibling bank of 8 | Rec |
| `/<dev>/param/count` | number of params (flat list over all pages) | `min(pageCount, numParamPages) * 8` | R |
| `/<dev>/param/<i>/name` | text, empty = no parameter | page `i/8`, param `i%8` | R |
| `/<dev>/param/<i>/value` | 0..1 | `value` | R |
| `/<dev>/param/<i>/value/str` (+ `/str/6\|8`) | text | `displayedValue` | R |
| `/<dev>/param/<i>/steps` | int (number of steps) | always −1 | O |
| `/<dev>/param/<i>/modulated` | 0..1 | `modulatedValue` | O |
| `/<dev>/page/count`, `/<dev>/page/selected`, `/<dev>/page/<i>/name` | (feature) | `pageNames`, `selectedPageIndex` of the following cursor | Rec |
| `/device/touchedParam` | index | – (Reaper) | O |
| `/<dev>/hasDrumPads`, `hasLayers`, `hasMutedPads`, `hasSoloedPads` | flag | `hasDrumPads`, `hasLayers`, `drumPadBank.hasMuted/SoloedPads` | O |
| `/eq/band/<b>` (Bitwig appends `/value`, ignored) | band type as Reaper EQ index (−1 off, 0 low shelf, 1 high shelf, 3 high cut, 4 low cut, 6 notch, 8 bell) | EQ+ `TYPE<n>` mapped by name | O |
| `/eq/param/<3*b + 0\|1\|2>/…` | frequency, gain, Q of band `b` (8 bands) | EQ+ `FREQ/GAIN/Q<n>` | O |

The core assumes a device has drum pads until `hasDrumPads` says otherwise.

### 3.6 Drum pads and layers

`/<dev>/pad/<i>/<field>` with `<dev>` = `drum`, `device`, `primary`; `/<dev>/layer/<i>/<field>`
with `device`, `primary`; `/<dev>/layer/count`. Reporting any pad switches the pad bank to host
pads (the core pages locally over all reported pads; Bitwig reports all 128).

Fields: `exists` (Bitwig: the pad has a first device), `name`, `color`, `select`
(`addIsSelectedInEditorObserver`), `active`, `mute`, `solo`, `volume`, `volume/str`, `pan`,
`pan/str` (no shortened texts). Pad sends and devices are not reported.

### 3.7 Clip and step grid

| Address | Value | Bitwig | Need |
| --- | --- | --- | --- |
| `/clip/exists` | flag | cursor clip `exists` | Rec |
| `/clip/start`, `/clip/end` | beats | `getPlayStart`, `getPlayStop` | O |
| `/clip/loop` | flag | `isLoopEnabled` | O |
| `/clip/color` | `r g b` | `color` | O |
| `/clip/loopStart`, `loopLength`, `shuffle`, `accent`, `pinned` | beats, flag, 0..1, flag (**feature: host step grid**) | `getLoopStart` etc. | O |
| `/clip/grid/<id>/playingStep` | step or −1 | `playingStep` | O (with grid) |
| `/clip/grid/<id>/step/<ch>/<x>/<y>` | `0` = empty, else 24 space separated values (`StepInfo::to_wire`) | `addNoteStepObserver`; empty steps reported once as `0` | O (with grid) |
| `/clip/playposition` | beats | – (Reaper) | O |
| `/clip/notes` | notes as `playingnotes` | – (Reaper note list) | O |
| `/clip/all` | all clips (Reaper) | – | O |

`<id>` is `<steps>x<rows>` from the model setup. The 24 step values: state (1 = note start, 2 =
continue), selected, muted, duration, velocity, velocity spread, release velocity, pressure,
timbre, pan, transpose, gain, chance enabled, chance, occurrence enabled, occurrence index (order
of `NoteOccurrenceType::ALL`: always, first, not first, prev, not prev, prev channel, not prev
channel, prev key, not prev key, fill, not fill), recurrence enabled, recurrence length,
recurrence mask, repeat enabled, repeat count, repeat curve, repeat velocity curve, repeat
velocity end. Bitwig reports `gain() / 2` as the Java version; whether the range matches the
`setGain` of the commands is open (BITWIG_DIVERGENCES.md §6).

### 3.8 Browser

Reported only while the browser is open (`/browser/exists 0` otherwise).

| Address | Value | Bitwig |
| --- | --- | --- |
| `/browser/exists` | flag (feature) | `popupBrowser.exists` |
| `/browser/preview` | flag | `shouldAudition` |
| `/browser/contentType/count`, `/selected`, `/selected/name`, `/<i>/name` | | `contentTypeNames`, `selectedContentTypeIndex`; selected name is always `Result` (as Java) |
| `/browser/column/count` | 7 | |
| `/browser/column/<c>/exists`, `name`, `wildcard`, `cursorIndex`, `cursor/exists`, `cursor/name` | | the filter column, its wildcard item, its cursor item |
| `/browser/column/<c>/item/<i>/exists`, `name`, `select`, `hits` | | siblings bank of the cursor item |
| `/browser/result/<i>/exists`, `name`, `select` | | results bank |
| `/browser/result/cursor/name` | | cursor result |
| `/browser/presetsfile`, `/browser/selected/index` | Reaper preset browser | – |

### 3.9 Markers

`/marker/count`, `/marker/<i>/exists`, `number`, `name`, `color` (`position`, `endPosition` are
accepted and ignored). Not reported by the Bitwig bridge (no marker bank); Reaper reports them.

## 4. Command reference (core → host)

Callbacks: `process_no_arg(processor, command)`, `process_int_arg` (booleans are 1/0),
`process_double_arg`, `process_string_arg`, `process_string_args` (only `track addTrack`). The
command is the path without leading `/`, its parts separated by `/`. The Bitwig bridge treats
int and double the same (`handle(processor, command, double)`), and turns a no-arg command
ending in `/reset` into the value path.

### 4.1 Value command suffixes (host features)

| Suffix | Arg | Meaning | Bitwig |
| --- | --- | --- | --- |
| *(none)* | double 0..1 | set absolutely; the DAW's takeover/pickup mode applies | `SettableRangedValue.set` |
| `/inc` | double | **relative**: the delta normalized to the parameter range (`increment / upperBound`); the core does not change its own value, it waits for the report | `inc(delta)` |
| `/reset` | none (no-arg) | reset to **the DAW's** default | `Parameter.reset()` |
| `/immediately` | double | set, bypassing takeover/pickup (pads, buttons without a physical position) | `setImmediately` |
| `/touch` | int 0/1 | the control is touched / released | `touch(bool)` |
| `/indicate` | int 0/1 | show the control's mapping in the DAW | `setIndication(bool)` |

They apply to the "value commands": `track <p>/volume|pan`, `master volume|pan`, `track
<p>/send/<s*>/volume`, `device|primary|eq|project param/<i>/value`, `drum|primary pad|layer/<i>/
volume|pan`, `project cueVolume|cueMix`, `groove <name>`, `transport metronomeVolume`
(`set`/`inc`), `clip accent` (`/immediately`). Clip positions: `clip start|end|loopStart|
loopLength/inc <beats>` (beats, not normalized). `/inc` needs the feature `valueInc`, `/reset`
`valueReset`, `/immediately` `immediately`, `/indicate` (volume, pan, sends) `indication`
(§2 "Host features"); without them the core sends absolute values and its own defaults.

### 4.2 Transport

| Command | Arg | Meaning | Bitwig |
| --- | --- | --- | --- |
| `play` | none | toggle play (Reaper path) | `togglePlay` |
| `stop` | none | stop | `stop` |
| `record` | none | toggle record | `record` |
| `repeat` | int 1 | toggle loop | `isArrangerLoopEnabled.toggle` |
| `tempo` | double | set BPM | `tempo.value.setRaw` |
| `tempo +`, `-`, `++`, `--` | none | change by ±0.01 (Bitwig; Reaper 0.1) / ±1 BPM | `setRaw(tempo ± d)` |
| `time` / `time nosnap` | double seconds | set position | converted to beats at the tempo, `playStartPosition.set`, jump if playing |
| `time` | int 0 | rewind (Reaper path of stop and rewind) | as above |
| `time loop/start`, `time loop/length` | double seconds | Reaper path | unhandled |
| `time signature` | int `num + den*100` | set time signature | **unhandled** |
| `beats position`, `beats position/nosnap` | double quarters | set position | `playStartPosition.set`, `jumpToPlayStartPosition` if playing |
| `beats loop/start`, `beats loop/length` | double quarters | set loop | `arrangerLoopStart/Duration.set` |
| `transport play\|stop\|restart` | none (`transportCommands`) | | `play()`, `stop()`, `restart()` |
| `transport stopAndRewind` | none | | `stop()`, `setPosition(0)` 100 ms later |
| `transport end` | none | to the end of the arrangement | action `jump_to_end_of_arrangement` |
| `transport metronome` / `toggleMetronome` | int / none | | `isMetronomeEnabled.set/toggle` |
| `transport togglePrerollMetronome` | none | | `isMetronomeAudibleDuringPreRoll.toggle` |
| `transport metronomeTicks <0\|1>`, `toggleMetronomeTicks` | | | `isMetronomeTickPlaybackEnabled` |
| `transport metronomeVolume[/inc]` | double | | `metronomeVolume` set/inc |
| `transport punchIn\|punchOut <0\|1>`, `togglePunchIn\|togglePunchOut` | | | `isPunchIn/OutEnabled` |
| `transport fill <0\|1>`, `toggleFill` | | | `isFillModeActive` |
| `transport overdub/arranger\|overdub/launcher <0\|1>` | | | `isArranger/ClipLauncherOverdubEnabled.set` |
| `transport tapTempo` | none | | `tapTempo` |
| `automation <index>` | int | set mode (0..5 as `/automode`) | ≤ Read: both switches off; else both on and `automationWriteMode` = touch/write/latch |
| `automation toggleArranger\|toggleLauncher\|resetOverrides` | none | | `toggleWriteArrangerAutomation` etc. |
| `metro_vol`, `metro_vol +\|-` | int 0..127 / none | Reaper path | unhandled (not sent with `transportCommands`) |
| `quantize` | double 0..1 | quantize the cursor clip | `cursorClip.quantize(v)` if 0 < v ≤ 1 |
| `noterepeat active\|velocity` | int 0/1 | | arpeggiator `isEnabled`, `usePressureToVelocity` |
| `noterepeat rate` | double = 1 / period | | `rate.set(1 / value)` |
| `noterepeat notelength` | double | | `gateLength` |
| `noterepeat mode` | int 0..3 | | `mode` = down, up, down-up, up-down |
| `undo`, `redo` | none | | `application.undo/redo` |

### 4.3 Application, arranger, mixer, project

| Command | Arg | Bitwig |
| --- | --- | --- |
| `application layout` | int 0..3 (ARRANGE, MIX, EDIT, PLAY) | `setPanelLayout` |
| `application layout/-\|+` | none | `previous/nextPanelLayout` |
| `application recordQuantization` | string grid | `recordQuantizationGrid.set` |
| `application recordQuantization/noteLength/toggle` | none | `recordQuantizeNoteLength.toggle` |
| `application toggleNoteEditor\|toggleAutomationEditor\|toggleDevices\|toggleInspector\|toggleMixer\|toggleFullScreen\|toggleBrowser` | none | the `Application` toggles (`toggleBrowserVisibility`) |
| `application duplicate\|deleteSelection\|arrowLeft\|arrowRight\|arrowUp\|arrowDown\|zoomIn\|zoomOut\|enter\|escape` | none | `duplicate`, `remove`, `arrowKey*`, `zoomIn/Out`, `enter`, `escape` |
| `application incTrackHeight\|decTrackHeight` | none | `arranger.zoomIn/OutLaneHeightsAll` |
| `application sliceToSampler\|sliceToDrumMachine` | none | actions `slice_to_multi_sampler_track`, `slice_to_drum_track` |
| `arranger toggleCueMarkers\|togglePlaybackFollow\|toggleTrackRowHeight\|toggleClipLauncher\|toggleTimeLine\|toggleIoSection\|toggleEffectTracks` | none | `arranger.*.toggle` |
| `mixer toggleClipLauncherSection\|toggleCrossFadeSection\|toggleDeviceSection\|toggleIoSection\|toggleMeterSection\|toggleSendsSection` | none | `mixer.*.toggle` |
| `project engine` | int 0/1 | `activate/deactivateEngine` |
| `project unsoloAll\|unmuteAll` | none | `project.unsoloAll/unmuteAll` |
| `project cueVolume\|cueMix` (+ suffixes) | double | `project.cueVolume/cueMix` |
| `project param/<i>/value\|touch\|indicate` | | project remote controls (flat index, 8 per page) |
| `project parameterPage` | int page | `selectedPageIndex.set` (clamped) |
| `project createScene` | int (length in beats, ignored by Bitwig) | `createScene` |
| `project createSceneFromPlayingLauncherClips` | none | same name |
| `project previous\|next` | none | `application.previous/nextProject` |
| `project save\|load` | none | actions `Save`, `Open` |
| `groove <name>` (+ `/inc`, `/reset`, `/touch`, `/indicate`) | double | the six groove parameters |
| `groove active\|amount` | double | Reaper path, unhandled |
| `refresh` | none | **report everything with the next collection** |
| `bank track\|scene` | int offset | `trackBank/sceneBank.scrollPosition.set` |

### 4.4 Tracks and master

`track <p>/<cmd>` with the absolute position; `master <cmd>` without position. The Bitwig bridge
resolves `<p>` in the window, else in the 400-track bank; sends and slots only in the window.

| Command | Arg | Meaning / Bitwig |
| --- | --- | --- |
| `track <p>/select`, `master select` | int 1 | select: `selectInEditor`, `selectInMixer`, `makeVisibleInArranger/Mixer`. `0` is ignored |
| `track <p>/scrollto` | none | Reaper path (select and scroll); Bitwig: `scrollPosition.set(p)` |
| `track <p>/volume\|pan`, `master volume\|pan` | double (+ suffixes §4.1) | `volume/pan` |
| `…/volume/touch`, `…/pan/touch` | int | `touch` |
| `…/volume/indicate`, `…/pan/indicate` | int | `setIndication` |
| `track <p>/mute\|solo\|active`, `master …` | int 0/1 | `mute/solo/isActivated.set` |
| `track <p>/solo/toggle`, `master solo/toggle` | none (`soloToggle`) | `solo.toggleUsingPreferences(false)` (exclusive solo preference applies). **The master variant is unhandled**, see §6.8 |
| `track <p>/recarm` | int | `arm.set` |
| `track <p>/monitor` | int | `monitorMode` ON / OFF |
| `track <p>/autoMonitor` | int | `monitorMode` AUTO / OFF |
| `track <p>/isGroupExpanded` | int | `isGroupExpanded.set` |
| `track <p>/color`, `master color` | string `RGB(r,g,b)` | `color.set` |
| `track <p>/name` | string | `name.set` |
| `track <p>/remove\|duplicate` | none | `deleteObject`, `duplicateObject` |
| `track <p>/stop\|stopAlt\|returnToArrangement` | none | same names |
| `track <p>/createClip/<slot>` | int length in beats | `createNewLauncherClip(slot, length)` (absolute slot) |
| `track <p>/send/<s>/volume` | double (+ `/inc`, `/reset`, `/immediately`, `/indicate`) | `sendBank.getItemAt(s)` |
| `track <p>/send/<s>/active` | int | Bitwig **toggles** `isEnabled` (value ignored) |
| `track <p>/clip/<s>/…` | see §4.5 | |
| `track indicate` | int | `trackBank.setShouldShowClipLauncherFeedback` |
| `track pinned` / `track pinned/toggle` | int / none | `cursorTrack.isPinned.set/toggle` |
| `track selectPrevious\|selectNext` | none | `cursorTrack.selectPrevious/Next` |
| `track addTrack` | strings: type (`INSTRUMENT`, `EFFECT`, else audio), name, `RGB(…)` color, Reaper device names… | `createInstrument/Effect/AudioTrack(-1)`; name and color set on the cursor track 300 ms later; devices dropped |
| `track <p>/pin <pos>`, `toggleMultiSelect`, `movePrev\|moveNext`, `recordClip`, `overdub`, `inQuantResolution`, `param/<i>/value` (track FX), `clearAutomation` | | Reaper commands, unhandled in Bitwig (most are not sent with the matching host feature) |
| `master user/param/0/value` | double | crossfader (Reaper master FX), unhandled |

### 4.5 Slots and scenes

| Command | Arg | Bitwig |
| --- | --- | --- |
| `track <p>/clip/<s>/launch\|launchAlt\|launchRelease\|launchReleaseAlt` | none | same names on the slot (absolute `<s>`, must be in the window) |
| `track <p>/clip/<s>/record\|select\|remove\|duplicate` | none | `record`, `select`, `deleteObject`, `duplicateClip` |
| `track <p>/clip/<s>/insertFile` | string path | `replaceInsertionPoint.insertFile` |
| `track <p>/clip/<s>/color` | string `RGB(…)` | `color.set` |
| `track <p>/clip/<s>/paste` | string `<track>/<slot>` source | `replaceInsertionPoint.copySlotsOrScenes(source)` |
| `scene <s>/launch\|launchAlt\|launchRelease\|launchReleaseAlt\|select\|remove\|duplicate` | none | same / `selectInEditor` / `deleteObject` / `nextSceneInsertionPoint.copySlotsOrScenes` |
| `scene <s>/name`, `scene <s>/color` | string | `name.set`, `color.set` |
| `scene stop\|stopAlt` | none | `sceneBank.stop/stopAlt` |

### 4.6 Cursor device, siblings, specific devices

Processors: `device` (cursor device), `primary` (first instrument), `drum` (drum machine), `eq`.

| Command | Arg | Bitwig |
| --- | --- | --- |
| `device +` / `device -` | none | `cursorDevice.selectNext/Previous` |
| `device page/+` / `page/-` | none | sibling bank page |
| `device selected` | int, 1-based index in the sibling window | `siblings.getItemAt(i-1).selectInEditor` |
| `device sibling/<i+1>/bypass\|remove\|duplicate\|select` | none (`deviceCommands`) | on the sibling of the window |
| `<dev> window/toggle\|expand/toggle\|bypass\|remove\|duplicate\|select` | none (`deviceCommands`) | on the device itself (`duplicate` = `afterDeviceInsertionPoint.copyDevices`) |
| `<dev> window`, `<dev> expand` | int | `isWindowOpen/isExpanded.set` |
| `device pinned` | int | `cursorDevice.isPinned.set` |
| `device <i+1>/bypass\|remove\|duplicate` | int / none (Reaper path) | the sibling whose chain position is `i` (bypass toggles) |
| `device\|primary\|project parameterPage` | int page | show the page in the DAW (`selectedPageIndex.set`, clamped) |
| `<dev> param/<i>/value` (+ §4.1), `param/<i>/touch`, `param/<i>/indicate` | | flat index → page `i/8`, param `i%8`; EQ: frequency/gain/Q of band `i/3` |
| `eq band/<b>` | string Reaper type index | sets the EQ+ `TYPE` parameter |
| `eq add` | none | `insertBitwigDevice(EQ+)` at the end of the cursor track's chain |
| `drum\|primary clearMute\|clearSolo` | none | `drumPadBank.clearMutedPads/clearSoloedPads` |
| `drum\|primary indicate` | int | `drumPadBank.setIndication` |
| `drum\|primary pad/<i>/select` | none (`deviceCommands`) | `selectInEditor` only |
| `primary layer/<i>/select` | none | as channel select |
| `drum\|primary pad\|layer/<i>/mute\|solo\|active\|volume\|pan\|…` | as channel commands | on the pad/layer channel |
| `device preset`, `device add/0`, `device <i+1>/movePrev\|moveNext` | | Reaper, unhandled |

### 4.7 Cursor clip and step grid

| Command | Arg | Bitwig |
| --- | --- | --- |
| `clip name`, `clip color` | string (`RGB(…)`) | `setName`, `color.set` |
| `clip start\|end\|loopStart\|loopLength` | double beats (absolute; `/inc` relative) | `getPlayStart/PlayStop/LoopStart/LoopLength` set/inc |
| `clip loop\|shuffle\|pinned` | int | `isLoopEnabled`, `getShuffle`, `isPinned` (on all grids) |
| `clip accent`, `clip accent/immediately` | double 0..1 | `getAccent` set / setImmediately |
| `clip duplicate\|duplicateContent` | none | same names |
| `clip transpose` | int semitones | `transpose` (only on note tracks) |
| `clip clear` | none | **unhandled** in Bitwig |
| `clip grid/<id>/stepSize` | double | `setStepSize` |
| `clip grid/<id>/scroll` | int step | `scrollToStep` |
| `clip grid/<id>/page/+\|-` | none | `scrollStepsPageForward/Backwards` |
| `clip grid/<id>/clearAll`, `clearRow/<ch>/<row>`, `clearColumn/<ch>/<step>` | none | `clearSteps`, `clearStepsAtY`, `clearStepsAtX` |
| `clip grid/<id>/step/<ch>/<x>/<y>/toggle` | int velocity | `toggleStep` |
| `…/set` | string `"<velocity> <duration>"` | `setStep` |
| `…/setFrom` | string, 24 step values | `setStep`, the other values 100 ms later |
| `…/update` | string, 24 step values (in an edit, every 100 ms) | all attributes of the step |
| `…/clear` | none | `clearStep` |
| `…/moveY` | int new row | `moveStep` |
| `…/<attribute>` | double (flags 0/1): `muted`, `duration`, `velocity`, `velocitySpread`, `releaseVelocity`, `pressure`, `timbre`, `pan`, `transpose`, `gain`, `chanceEnabled`, `chance`, `occurrenceEnabled`, `occurrence`, `recurrenceEnabled`, `recurrenceLength`, `recurrenceMask`, `repeatEnabled`, `repeatCount`, `repeatCurve`, `repeatVelocityCurve`, `repeatVelocityEnd` | the `NoteStep` setter |
| `clip note/<pitch>/…` | Reaper note commands (without host grid) | unhandled |

### 4.8 Markers

`marker previous|next` → `jumpToPrevious/NextCueMarker`. `marker add` and `marker <i>/<cmd>`
are sent by the core but unhandled in Bitwig (no marker bank).

### 4.9 Browser

| Command | Arg | Bitwig |
| --- | --- | --- |
| `browser replace/device` | none | browse at `cursorDevice.replaceDeviceInsertionPoint` |
| `browser replace/slot/<p>/<s>` | none | slot insertion point (window only) |
| `browser replace/drum\|primary/pad/<i>` | none | pad insertion point |
| `browser add/track/<p>`, `add/master` | none | start of the channel's chain |
| `browser insertBefore\|insertAfter` | none | before/after the cursor device (start/end of the chain if none) |
| `browser stop` | int 1 = commit, 0 = cancel | `commit` / `cancel` |
| `browser preview` | int | `shouldAudition.set` |
| `browser contentType/+\|-` | none | `selectedContentTypeIndex.inc(±1)` |
| `browser column/<c>/+\|-\|page/+\|page/-\|reset` | none | cursor `selectNext/Previous`, item bank page, `selectFirst` |
| `browser result/+\|-\|page/+\|page/-` | none | cursor result / results bank |

Opening cancels a running browser and browses 100 ms later.

### 4.10 Not executed by the Bitwig bridge

Logged once as `Not handled yet: …`: `inifile` (pre-roll measures), `action` (Reaper action
IDs), `cursor`, and the Reaper-only commands listed in the tables above.

### 4.11 `delay_updates` / `enable_updates`

`enable_updates(processor, enable)`: the core does not need values of this area (e.g. the browser
is closed); `delay_updates(processor)`: do not report this area for ~300 ms (sent while
automation is recorded, so that old values do not jump back). Reaper honours both
(`DataCollector`); Bitwig ignores them (it only records them). Optional, an optimisation.

## 5. MIDI and note inputs

- **From the controller:** `dbm_on_midi_event(app, 0, data, length)`: 3-byte short messages,
  complete sysex (`F0 … F7`). Other lengths are logged as unknown (a single `FE` is ignored),
  so pad 2-byte messages (program change, channel pressure) to 3 bytes (Bitwig delivers them as
  status, data1, data2 anyway).
- **To the controller:** `send_midi_data(0, bytes)`: sysex if the first byte is `F0`, else a
  short message of 3 (or 2) bytes.
- **Note inputs** (the notes/CCs that go straight into the DAW as instrument input, e.g. the
  pads): created by `set_note_input_filters(0, index, filters)` during
  `dbm_start_infrastructure`; index 0 is the default note input, more may follow. The filters
  are hex **prefixes** with the channel wildcard already expanded: e.g. `90`, `80`, `B040`,
  `B6`; the empty filter list of a controller becomes `90 80 B001 B00B B040 E0 D0 A0` (notes,
  mod wheel, CC 11, sustain, pitch bend, pressures). A message matches if its hex starts with a
  filter. Bitwig needs 3-byte masks: pad to 6 characters with `?` (`B040??`) and
  `createNoteInput(name, masks)`; `setShouldConsumeEvents(false)`, because the core must see
  the same messages (e.g. to light the pads).
- **Key translation table:** `set_note_input_key_translation_table(0, index, table, 128)`:
  `table[note]` = the note sent to the DAW, **−1 blocks** the note. Sent at every view or
  octave change. **Velocity table:** same, 128 entries, the velocity sent to the DAW. Both must
  be applied before the note reaches the instrument. Bitwig: `setKeyTranslationTable`,
  `setVelocityTranslationTable`.
- **Played notes** (`process_midi_arg(device_id, status, data1, data2)`): a MIDI event "as if
  played on the device", e.g. the further notes of a chord in the chord views. Inject it into the
  default note input (Bitwig: `noteInput.sendRawMidiEvent`, as `MidiInputImpl.sendRawMidiEvent`
  of the Java version; Reaper: the track input). Without this only the root note of a chord
  plays.
- **Note repeat** works on the default note input (Bitwig: its `arpeggiator()`), see §4.2.

## 6. Behavioural rules learned from the conformance work

1. **Report again after a value command (takeover/pickup).** The core sets the value it sends
   locally right away (as for Reaper). If the DAW does not take it (Bitwig's takeover mode
   "catch" ignores a value until the control reaches the current one), change-only reporting
   would leave the wrong value on the controller. The Bitwig bridge forgets the reported values
   of the addressed item 300 ms after a value command (`CommandHandler.reportAgain` →
   `ModelReporter.forgetLater`: `/track/<p>/`, `/master/`, `/<dev>/param|pad|layer/<i>/`), so the
   next collection reports the DAW's real value. Not immediately: while a knob turns, the DAW may
   not have applied the last value yet, and the old value would set the core back.
2. **Selections only count on change.** Because of rule 1, values (including `select 1`) are
   reported again; the core ignores a `/track/<p>/select` equal to its state (a repeated
   selection recalled the preferred view, ending the Shift view on the Maschine Mk2). Report the
   deselection of the previously selected track explicitly, also when it is outside the window.
   Report the selection from a cursor that is **never pinned** (Bitwig: a second cursor track),
   and the pinned track separately as `/track/pinned`.
3. **Report changes only, honour `refresh`.** Full dumps on every tick cost time and retrigger
   things; but after `refresh` the next report must be complete.
4. **Absolute positions, host-side windows.** With host paging the core keeps the bank offset and
   tells the bridge (`bank track|scene <offset>`); the bridge scrolls its window and reports the
   window with absolute positions. Commands address absolute positions; a bridge with only a
   window must resolve positions outside it (Bitwig: a 400-track bank for channel commands; sends
   and slots outside the window cannot be addressed).
5. **Pinning model.** The core shows the pinned track (`/track/pinned`) as its cursor track; the
   cursor device, the remote controls, the drum machine, the first instrument, the EQ, the clip
   and the playing notes must follow that (pinned) cursor track in the DAW, while the selection
   follows the DAW's selection. When the cursor track changes, report its playing notes again
   (the core keeps one note cache).
6. **Commands take effect through reports.** Toggles (`…/toggle`, `bypass`, Bitwig's
   `send/<s>/active`) do not carry the new state: the core shows the new state when it is
   reported. With `/inc` and `solo/toggle` the core does not change its own value at all, so the
   LED follows one tick after the report.
7. **Timing is core time.** Scheduled actions (75 ms selections after a page change, 1 s note
   mapping update and startup, long presses, blinking, notification lifetimes) run in `tick`,
   measured with the time of the calls into the core. Tick regularly; notifications appear up to
   one tick late (Bitwig shows them after the tick).
8. **Known gaps of the reference bridge** (useful when a new host does better): effect track
   banks, hierarchical track lists, extra scene/slot banks, a slot bank of the cursor track (scene
   navigation), the cursor layer, markers, sends and devices of pads/layers, pre-roll length,
   `time signature`, `clip clear`, device/track move, `master solo/toggle` (sent without value,
   the bridge's no-arg handler has no `master` case).

## 7. Minimal bridge checklist, then progressive features

**Step 1: a controller that lights up and mixes**

1. Load the library, `dbm_create_controller(…, DBM_FLAG_HOST_TICK)`, MIDI in →
   `dbm_on_midi_event`, `send_midi_data` → MIDI out, `open_midi_*` return 1 for device 0.
2. `dbm_start_infrastructure`, then a 30 ms timer: report changes → `dbm_update_model` →
   `dbm_tick`. Show `dbm_take_notifications`.
3. `refresh` → full dump.
4. Note inputs: filters, key and velocity tables, `process_midi_arg`.
5. Tracks: `/track/count`, per track `exists`, `number`, `name`, `type`, `color`, `select`,
   `volume`(+`/str`), `pan`(+`/str`), `mute`, `solo`, `recarm`, `vu*`; master the same.
   Commands: `track <p>/select|volume|pan|mute|solo|recarm`, `master …`.
6. Transport: `/play`, `/record`, `/repeat`, `/tempo`, `/beats/position`, `/beats/text`,
   `/time`; commands `play`, `stop`, `record`, `repeat`, `tempo`, `beats position`.
7. Cursor device: `/device/exists|name|bypass|count|position`, `/device/param/count`,
   `/device/param/<i>/name|value|value/str`, siblings; commands `device +|-`,
   `device param/<i>/value`, `device parameterPage`.

Without `/track/offset` the core pages the tracks itself and sends Reaper-style commands
(absolute values, `<p>/scrollto`, `<index+1>/bypass`): simplest to start with.

**Step 2: host paging.** Report `/track/offset` and `/scene/offset` and implement `bank`. Report
`/host/features` at the same time (an empty list at first), otherwise the core assumes all
features. Then add the features one by one (§2 "Host features"): `valueInc` (with the re-report
after value commands, §6.1), `valueReset`, `immediately`, `indication`, `soloToggle`,
`transportCommands`, `deviceCommands`, `layoutStep`. `track pinned|selectPrevious|selectNext` come
with the reports `/track/pinned|hasPrevious|hasNext`.

**Step 3: features, each independent** (report the address, handle its commands, §2 table):
sends; slots and scenes; named parameter pages; shortened texts; automation and overdub switches;
punch/fill/ticks; record quantization; project solo/mute, cue, remote controls; application
layout and arranger/mixer flags; groove; drum pads (`/drum`, `/primary`), layers, EQ; host step
grid of the note clips; browser; playing notes; markers.

## 8. Testing a bridge

- **Unit tests of the core** (`cargo test`): every address has a parser test in
  `src/parser.rs` (e.g. `host_paging`, `host_automation_switches`), every command a test of the
  core type that sends it, using `RecordingBackend` (records `BackendCall`s). A new address or
  command for your host should get the same.
- **Recording.** `ProtocolRecorder` (enabled by `DBM_RECORD_DIR`, or in dev mode) writes
  everything into and out of the core, one event per line: `<ms with 3 decimals>\t<kind>\t
  <field>…`, fields with `\\`, `\n`, `\t` escaped. Kinds: `init` (core ID, app version), `config`
  (file name, content), `setting` (ID, value), `start`, `model` (the lines), `tick`, `in`/`out`
  (hex bytes), `raw` (`process_midi_arg`), `noArg`, `string`, `int`, `double` (processor,
  command, value), `strings`, `delay`, `enable`, `filters`, `keys`, `velocities` (note input,
  values…), `mark` (begin/end, name; from sysex `F0 7D 44 42 4D 01|02 <name> F7` of the runner,
  not passed to the core; the end copies the file to `<name>.rec`).
- **Replay** (`tests/conformance_replay.rs`): feeds the inputs of each `.rec` in
  `tests/conformance/*/replay/` (or `DBM_REPLAY=<file>`) into a fresh `App::new_single` at the
  recorded times (`App::set_time`) and checks that the core's outputs after each input are the
  same set as recorded (regression), and, if `<controller>-java.leds` exists, that the LED state
  at the end marker equals the Java version's except the patterns in `…-java.known`
  (conformance). Any host that writes this format with `dbm_create_controller` semantics (device
  0, one controller) can be replayed unchanged; the replay does not need the DAW.
- **Conformance runner** (`backends/bitwig/tools/conformance_runner.py`,
  CONFORMANCE_TESTING_PLAN.md): plays the MIDI of TOML scenarios (`tests/conformance/<ctrl>/
  *.toml`) into the extension through a virtual MIDI port, answers like the device, records the
  LEDs/displays and the DAW state (OSC of the Java DrivenByMoss on UDP 9000 and the probe
  extension on UDP 9100), and compares the Java version (oracle) with the Rust port
  (`pair … --java-port … --rust-port …`). It is Bitwig specific in its DAW observers (OSC, probe,
  restart via sysex `F0 7D 44 42 4D 03 F7` with `DBM_OSC_CONFORMANCE`), but the scenario format,
  the MIDI stub and the LED comparison are host independent. For another DAW: replace the state
  observers (or leave the DAW-state expectations out), keep the scenarios, and compare the LED
  traces against a Bitwig run of the same scenarios. **(unverified: not tried with another host.)**
- **Debugging aids of the Bitwig bridge worth copying:** log each unhandled command once
  (with digits replaced by `<n>`), `DBM_LOG_MIDI` to log MIDI in/out, a dev mode that reloads the
  library when it is rebuilt (`DBM_DEV_LIBRARY`), and `tools/osc_monitor.py`.
