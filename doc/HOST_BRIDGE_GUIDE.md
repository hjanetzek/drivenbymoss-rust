# Connecting a DAW to the DrivenByMoss core: host bridge guide

For someone who wants to run the DrivenByMoss controllers of the Rust core in another DAW
(Ableton Live, Studio One, Ardour, a custom host). The DAW side is a **bridge**: it loads the
core, feeds it the DAW state and the controller's MIDI, and executes the commands the core sends
back. The reference is the **Bitwig bridge** (`backends/bitwig`, Java, FFM binding). It is the
most complete host: since the core port of the Bitwig classes (BITWIG_CORE_PORT_PLAN.md) the
core follows the Java DrivenByMoss for Bitwig wherever the host reports what Bitwig has. The
Reaper bridge (`backends/reaper`, C++) speaks the older, minimal variant of the same protocol.

**The Python reference host** (`bindings/python/dbm_host/`, on the Python binding
`bindings/python/drivenbymoss/`) is the readable companion of this guide.
- It is a complete bridge on a simulated DAW, in plain Python without the API of a real DAW.
- It passes all 195 conformance scenarios of the four setups with the results of the Java
  version on Bitwig (`python3 -m conformance --all`, §8).
- Each section below names the part that implements it.
- Copy its structure: `Bridge` (the loop), `Reporter` (host → core), `CommandHandler` (core →
  host). Replace `SimDaw` by your DAW's API.
- The Bitwig bridge stays the reference for how a real DAW API is used.

As of 2026-10-31, the reference host pointers 2026-11-02. Compiled from the code: `include/drivenbymoss.h`, `src/ffi.rs`, `src/app.rs`,
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

*Reference host:* `bindings/python/dbm_host/bridge.py`: `Bridge.start` (create, start, model setup, first report), `step` (MIDI in → `dbm_set_time` → `tick` → report → `update_model`), `advance` (simulated time); the callbacks as `drivenbymoss.Backend` methods.

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
  output are device ID `0` (a controller with more ports, `numMidiInPorts`/`numMidiOutPorts` > 1,
  uses the device IDs `1..n` for the further ones, e.g. the CTRL port of the Electra One; the
  reference host's `Bridge` opens them all), no main configuration, no port discovery
  (`get_midi_inputs/outputs` are not used). `controller_id` is the UUID from `dbm_list_controllers()` (JSON: `uuid`,
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
  startup, `handle_changes` and `flush` (all LED/display output). The OSC controller also
  reads its UDP socket (non-blocking) and sends its OSC bundles in `dbm_tick`: OSC commands are
  executed at the next tick, nothing in the host is needed for the network.
- **Time base.** At every entry (`tick`, `update_model`, `on_midi_event`, the settings calls,
  `start_infrastructure`) the core takes `Instant::now()` as `Host::now`. All core timing uses
  this time: scheduled tasks such as the 75 ms selection after a page change, the 1 s startup
  delay, long presses, notification lifetimes and blinking. So the timing resolution is your
  tick interval.

  A host with a simulated or offline time calls `dbm_set_time(app, ms)` instead: the
  milliseconds since the creation of the instance, before each `tick`. After the first call the
  instance never reads the clock again. The Python reference host does this
  (`bindings/python/dbm_host/bridge.py`), and the replay does the same through `App::set_time`
  (§8).
- **Commands may arrive during any call**, not only during `tick` (e.g. `bank track <offset>`
  while you report `/track/offset`, LED updates are only in `tick`). The Bitwig bridge executes
  them right away in the callback (`CommandHandler`); the Reaper bridge queues them and runs the
  queue after `tick`. Both are fine as long as the callback does not call back into the core.

### Startup and shutdown (Bitwig sequence, `DbmExtension.init/exit`)

1. Load the library, `dbm_create_controller(..., DBM_FLAG_HOST_TICK)`. `app_version` is free
   text (Bitwig: `getHostProduct() + " " + getHostVersion()`).
2. Install the MIDI input callbacks (short messages and sysex → `dbm_on_midi_event(app, 0, …)`).
3. `dbm_start_infrastructure`. During this call the core opens the ports
   (`open_midi_input(0)`, `open_midi_output(0)`, must return non-zero; no `open_midi_output`
   for a controller with `numMidiOutPorts` 0, e.g. OSC), creates its note inputs
   (`set_note_input_filters`, then key/velocity tables) and sends `refresh`. Bitwig can only
   create note inputs during `init`, so this call must happen there. The controller's `startup`
   (initial LEDs etc.) runs in the first `tick` 1 s later (`STARTUP_DELAY_MS`).
4. `dbm_get_model_setup` (empty before step 3) and create all DAW objects from it (below).
5. Settings: `dbm_get_settings` → build the DAW's settings UI; push the stored values with
   `dbm_set_setting(id, value)`; discard `dbm_take_setting_changes` once; apply
   `dbm_take_setting_states` (hidden and disabled settings).
6. Every 30 ms: collect the state → `dbm_update_model(lines)` (skip if empty) → `dbm_tick` →
   apply `dbm_take_setting_changes` and `dbm_take_setting_states` to the UI → show
   `dbm_take_notifications` (one per line) as popups. Optional: `dbm_take_control_help` (the
   help of the controls for a simulator, JSON lines of the changes, doc/CONTROLLER_HELP_PLAN.md;
   `dbm_reset_control_help` hands out all again).
7. Exit: `dbm_destroy` (stops the controller, which writes its final MIDI and the `.config`
   file). Not from within a callback. `close_midi_*` are called; Bitwig ignores them (the ports
   are Bitwig's).

Settings: `dbm_get_settings` returns one tab separated line per setting: ID, scope (`global` /
`document`), kind (`enum`, `integer`, `boolean`, `text`, `action`, `signal`), category, label,
default, current value, then the enum options, the integer min/max or the button text of a signal.
An `action` value is the ID of a DAW action (Bitwig: shown as a category and the actions of the
category, as `SettingsUIImpl.getActionSetting` of the Java version; a DAW without an action list
shows a text). A `signal` is a button without a value: on a press the bridge calls
`dbm_set_setting(id, "")`. `dbm_set_setting` changes are not echoed by
`dbm_take_setting_changes`, which returns `ID<tab>value` lines of changes made by the controller
(e.g. a scale chosen on the device). `dbm_take_setting_states` returns `ID<tab>visible<tab>enabled`
lines (1 or 0) of the settings whose state changed (Java `setVisible` / `setEnabled`, e.g. the
slot settings of Generic Flexi, hidden while the slot is off); the first call reports the states
set during the creation of the controller. A setting which has no effect on the host is reported
hidden as well: one whose effects all need a host feature or commands the host does not have
(`Configuration::require`, e.g. Push's "Stop automation recording on knob release" needs one of
`automation toggleArranger|toggleLauncher|<mode>` in the handled-commands bitset; a text host and
an empty bitset have all commands). The states follow a later `dbm_set_command_callback`. A host without these functions in an older core sees
all settings visible and enabled. The bridge stores global settings in the DAW preferences and
document settings in the project (`HostSettings`). The alternative used by Reaper is
`dbm_set_default_document_settings` / `dbm_get_formatted_document_settings` /
`dbm_set_formatted_document_settings` (one opaque text per project). The core also keeps a
`DrivenByMoss-<model>.config` file in `config_path` (spaces and slashes of the hardware model
as `-`; Reaper's `dbm_create` keeps `DrivenByMoss4Reaper-<model>.config`) as a fallback: it holds
the global settings (not the document settings, the host stores them) and is written about a
second after a global setting changed (by `dbm_set_setting` or by the controller; the changes of
that second are written together) and when the controller stops. Reaper writes it only when the
controller stops.

### The model setup (`dbm_get_model_setup`)

*Reference host:* `bindings/python/dbm_host/banks.py` (`Banks.configure`) and `CommandHandler.configure` (the navigator, `wantsClipLauncherNavigator`).

JSON with the keys of the Java `ModelSetup` (`src/daw/model_setup.rs`). Hosts that must create
their objects up front (Bitwig) size them from it; a host that can query anything at any time can
ignore most of it, but the numbers tell you what the controller will address.

| Key | Default | Bitwig bridge creates |
| --- | --- | --- |
| `hasFlatTrackList`, `hasFullFlatTrackList` | true, false | `createTrackBank(..., true)` (with effect tracks and master) if `hasFullFlatTrackList`, else `createMainTrackBank`. The core adds its own master only if the host lists none. Hierarchical lists (`hasFlatTrackList` false) are not supported |
| `numTracks`, `numScenes`, `numSends` | 8, 8, 8 | the track window (with `numSends` sends and `numScenes` slots), its scene bank; plus a 400-track bank without sends/slots for commands to tracks outside the window |
| `numFxTracks` | -1 | `createEffectTrackBank (numFxTracks, numSends, 0)`, -1 = `numTracks`; reported as `/fxtrack/…` (§3.3) |
| `wantsDrumDevice`, `additionalDrumDevices`, `numDrumPadLayers` | true, [], 16 | one drum machine (Bitwig Drum Machine matcher) with all 128 pads; the core pages over them for every drum device |
| `numDevicesInBank` | 8 | ignored: always 8 siblings |
| `numParamPages`, `numParams` | 8, 8 | `numParamPages` remote-control page cursors of 8, each fixed to its page (flat list of `numParamPages*8`), plus one cursor that follows the page selected in the DAW; project remote controls and the cursor track's remote controls the same way if `numParams > 0` |
| `numDeviceLayers` | 8 | ignored: 32 layers of the first instrument |
| `deviceIds` | [] | `"FIRST_INSTRUMENT"` → `/primary` (instrument matcher, params, 128 pads, 32 layers); `"EQ"` → `/eq` (first EQ+); `"NI_KOMPLETE"` not handled |
| `numFilterColumnEntries`, `numResults` | 16, 16 | popup browser with 7 filter columns (no smart collections), if `numResults > 0` |
| `noteClips` | [] | `[[steps, rows], …]`: one launcher cursor clip with a step grid per size, id `"<steps>x<rows>"` (only the sizes the views requested during setup; a later size has no grid) |
| `numMarkers` | 0 | if > 0: one cue marker bank of 100 markers (reported completely, the core pages over them) |
| `numListParams`, `wantsCursorLayer`, `wantsFocusedParameter`, `wantsClipLauncherNavigator` | 0, false, false, false | not used yet |

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

**The binary form** (TYPED_PROTOCOL_PLAN.md §2.2–§2.7, step 1): the same reports and commands as
typed records. A host writes one buffer of records per report and passes it to
`dbm_update_model_bin(app, buffer, length)` (text and binary can be mixed); after
`dbm_set_command_callback(app, DBM_PROTOCOL_VERSION_MAJOR, callback, user, handled, words)`
(before `dbm_start_infrastructure`; 0 = another major version, the text callbacks stay) the
commands of a call arrive as one buffer of records, before any other callback and at the end
of the call. The IDs, index slots and value types are generated from the tables in
`src/protocol/` into `include/dbm_protocol.h` (C: writers `dbm_put_*`, reader
`dbm_next_record`) and `bindings/cpp/include/dbm_protocol.hpp` (C++: a `Writer` with a method per
report, a `Reader` and a view per command), by `cargo run --bin dbm-protocol-gen`.
`dbm_messages_to_text` / `dbm_messages_from_text` convert between both forms (logs, recordings,
tests); `dbm_protocol_info` gives the version and the hash of the tables.

**Which hosts use which form** (TYPED_PROTOCOL_PLAN.md §6.6, step 5): the Bitwig bridge, the
Python reference host and the Zynthian adapter use only the binary form (reports, commands,
exact doubles); this guide writes the addresses and commands in their text form, which is the
core's text of the records. Reaper uses the text form (`dbm_update_model`, the `process_*`
callbacks); the core keeps both. A binary host passes the commands it handles to
`dbm_set_command_callback` (`handled`), and the core derives the host's command features from
them (§2 "Host features"); it can call the function again, e.g. once the model setup is known.

Existence: for parameters, sends and device siblings the **name** sets existence (empty name =
does not exist). Tracks, scenes, slots, pads and layers have an explicit `exists`.

### Change-only reporting and dumps (`ModelReporter`)

*Reference host:* `bindings/python/dbm_host/reporter.py`: `Reporter.collect` (the last text per address, only changes; `request_dump` after `refresh`; `forget_later` for the report again after a value command); the collectors run in a fixed order.

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
| `/host/features <names>` | The command forms the host understands (`ctx.host_supports`; a binary host: the value operations, its command features come from its handled commands), see below | the listed command forms |
| `/scene/offset <any>` | Host paging of scenes (and the slots) | `bank scene <offset>` |
| `/automode/arranger`, `/automode/launcher` | Bitwig's two automation write switches + modes Read/Touch/Write/Latch instead of Reaper's modes | `automation toggleArranger\|toggleLauncher\|resetOverrides`, `automation <mode index>` |
| `/overdub/arranger`, `/overdub/launcher` | Arranger and launcher overdub switches (instead of the track's `overdub`) | `transport overdub/arranger\|overdub/launcher <0\|1>` |
| `/punchIn`, `/punchOut`, `/fill`, `/click/ticks` | Independent punch in/out, fill mode, metronome ticks | – (the commands come with the feature `transportCommands`) |
| `/recordQuantization`, `/recordQuantization/noteLength` | Global record quantization (instead of per track `inQuantResolution`) | `application recordQuantization <grid>`, `…/noteLength/toggle` |
| `/track/pinned <p or -1>` | The cursor track is the pinned track of the host; `/track/fx/param/…` are the parameters of that (cursor) track instead of the selected track's | `track pinned <0\|1>`, `track pinned/toggle` |
| `/track/hasPrevious`, `/track/hasNext` | Cursor track navigation availability from the host | `track selectPrevious\|selectNext` |
| `/track/<p>/canHoldNotes`, `canHoldAudio` | Track capabilities (default: true) | – |
| `/track/<p>/playingnotes` | Playing notes of the cursor track light the pads | – |
| `/project/hasSolo`, `/project/hasMute` | Project-wide solo/mute state | `project unsoloAll` (with `hasSolo`), `project unmuteAll` (with `hasMute`) |
| `/project/param/…`, `/project/page/…` | Project remote controls (instead of a bank on the master FX) | `project param/<i>/…`, `project parameterPage <page>` |
| `/project/cueVolume`, `/project/cueMix` | The cue parameters exist | `project cueVolume\|cueMix` (+ `/inc`, `/reset`, `/touch`) |
| `/crossfade` | The crossfader is the host's transport crossfader (instead of the master FX parameter 0) | `transport crossfade` (+ `/inc`, `/reset`, `/immediately`, `/touch`, `/indicate`) instead of `master user/param/0/value` |
| `/groove/<name>` (six names) | Bitwig's six groove parameters (instead of `groove active\|amount`, −1..1) | `groove <name>` (+ suffixes) |
| `<…>/str/6`, `<…>/str/8` | Host-shortened texts for these lengths (other lengths: the core cuts) | – |
| `/device/page/selected`, `…/page/count`, `…/page/<i>/name` | Named pages; the page selected in the host is shown, also after a device change | `device\|primary\|project parameterPage <page>` |
| `/drum/pad/<i>/…`, `/device\|primary/pad/<i>/…` | The pads come from the host, the core pages over all of them | `drum\|primary pad/<i>/…` |
| `/browser/exists` | The host's browser (content types, filter columns, results) | `browser …` (§4.9) |
| any of `/clip/loopStart\|loopLength\|shuffle\|accent\|pinned` | **Host step grid** of the note clips (instead of Reaper's note list) | `clip grid/<id>/…`, `clip start\|end\|loopStart\|loopLength/inc` |
| `/device/pinned` | Pinnable cursor device (Reaper: always pinned) | `device pinned <0\|1>` |
| `/track/<p>/active`, `/master/active` | Activation state from the host (also makes `set_skip_disabled_items` work); `/master/active` lets the core (de)activate the master | `track <p>/active`, `master active` |
| `/track/<p>/vuholdleft\|vuholdright` | The VU clip state is the hold value above 0 dB (Reaper); without them it is `vuleft\|vuright >= 16241/16384` (Bitwig) | – |

### Host features (`/host/features`, the handled commands)

*Reference host:* `CommandHandler.handled_commands` (the IDs with a handler, passed to `App.use_binary_commands` after `configure`) and `CommandHandler.host_features` (the value operations, `OP_FEATURES`; reported by `Reporter.collect_host_features`), `bindings/python/dbm_host/commands.py`.

Command forms which cannot be detected from a report are switched on by the host (`HostFeature`
in `src/daw/context.rs`). There are two kinds:

- **Command features** (`soloToggle`, `transportCommands`, `deviceCommands`, `layoutStep`,
  `clipLauncherNavigator`: the `feature =` of the commands in `src/protocol/cmd.rs`). A **binary
  host** has a feature if it handles at least one of its commands, by the bitset of
  `dbm_set_command_callback` (null: all commands); the names of command features in its
  `/host/features` are ignored. So a host declares only the commands it executes (zynthian
  without zynseq: no transport commands, hence no `transportCommands`; Bitwig and the reference
  host without the navigator's commands unless the controller wants the navigator).
- **Value operation features** (`valueInc`, `valueReset`, `immediately`, `indication`): the
  operation of a value command (the `op` of its record), not a command of its own, so the bitset
  of the command IDs cannot express them. Every host reports them with `/host/features`.

`/host/features` is one line which lists the features (space separated, unknown names are
ignored, a new line replaces the earlier list):

```
/host/features valueInc valueReset immediately indication
```

A **text host** (Reaper) lists all its features in it, also the command features
(`... soloToggle transportCommands deviceCommands layoutStep`). Report it with the first report and
with every dump (the Bitwig bridge does). Without a feature the core sends the basic (Reaper) form
of the command or nothing. **Compatibility:** a host which reports `/track/offset` but never
`/host/features` supports all value operation features (and, as a text host, all command
features: the Bitwig extension before it reported its features); a host which reports neither
(Reaper) none. So a new host which pages should report `/host/features`, even an empty list.

| Name | What it enables in the core | Commands (instead of) |
| --- | --- | --- |
| `valueInc` | Relative value changes (knobs, `inc`); the core does not change the value itself and shows the host's values also while a control is touched (Reaper: held back until released). Not for the Reaper groove (`groove active\|amount`) | `<value command>/inc <normalized delta>` (absolute `<value command> <0..1>`) |
| `valueReset` | Resets to the DAW's default (track/master/layer/pad volume and pan, sends, device and project parameters, cue, groove) | `<value command>/reset`, no value (the core's default as absolute value) |
| `immediately` | Values without the host's takeover mode (pads, buttons) | `<value command>/immediately <0..1>` (`<value command> <0..1>`) |
| `indication` | Indication of volume, panorama and sends, and of the clip launcher of the track bank (device parameters are always indicated) | `track <p>/volume\|pan/indicate`, `master volume\|pan/indicate`, `track <p>/send/<s>/volume/indicate`, `track indicate <0\|1>` (nothing) |
| `soloToggle` | Solo toggles with the host's exclusive solo preference | `track <p>/solo/toggle`, `master solo/toggle` (`solo <0\|1>`) |
| `transportCommands` | Bitwig's transport commands, the metronome volume of the transport | `transport play\|stop\|restart\|stopAndRewind` (the `play`/`stop`/`record` processors, `time 0`), `transport togglePunchIn\|togglePunchOut\|toggleFill\|toggleMetronomeTicks\|returnToArrangement`, `transport punchIn\|punchOut <0\|1>` without the Reaper punch mode, `transport fill\|metronomeTicks <0\|1>` (nothing), `transport metronomeVolume[/inc]` with the default 0.8 (`metro_vol`, `metro_vol +\|-`) |
| `deviceCommands` | Commands to the device itself instead of the sibling index | `device\|primary\|drum\|eq window/toggle\|expand/toggle\|bypass\|remove\|duplicate\|select`, `device sibling/<i+1>/bypass\|remove\|duplicate\|select` (`device <index+1>/…`, `device window <0\|1>`), `drum\|primary pad/<p>/select` (nothing) |
| `layoutStep` | Previous/next panel layout of the host | `application layout/-\|+` (`application layout <index>`) |
| `clipLauncherNavigator` | The host's clip launcher navigator (only reported if the model setup has `wantsClipLauncherNavigator`; Bitwig: `ClipLauncherNavigatorImpl` of the Java version, a bank of one track following its own cursor track with one scene, its slot selected, the scene window following its scene) | `navigator selectTrack <position>` (then `track <p>/select`), `navigator tracks\|clips\|scenes/-\|+` (the core selects the slot of the new track itself, as the Reaper version) |
| `hostPages` | The host provides parameter pages (doc/HOST_PAGE_PLAN.md): a page surface (the Maschine JAM, Select + SONG) shows them; binary hosts only | `hostpage show\|cell/<c>/turn\|press\|touch\|reset\|row\|page\|set\|section` (§4.12; nothing without) |
| `controllerViews` | The host follows the view the user picks on the controller (zynthian switches its screen); binary hosts only | `controller view <kind>` (§4.13; nothing without) |
| `clipEditWindow` | The host shows the visible window of the controller's step sequencer (zynthian scrolls its editor); binary hosts only | `clip editWindow "<start> <length> <low> <high>"` (§4.13; nothing without) |

Features detected from their own reports need no name (table above): paging, the automation and
overdub switches, punch/fill/ticks state, pinned track, cursor track neighbours, project solo and
mute, playing notes, the host step grid, shortened texts, activation, VU hold, the crossfader.


## 3. Address reference (host → core)

Columns: **Need**: R = required for a working controller, Rec = recommended, O = optional /
feature. **Bitwig** = what the reference bridge reports. "–" in Bitwig = not reported by the
Bitwig bridge (Reaper only, or nobody).

### 3.1 Transport

*Reference host:* `Reporter.collect_transport`, `collect_note_repeat`; model `Transport`, `Arpeggiator` (`bindings/python/dbm_host/model.py`).

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
| `/click/prerollMeasures` | int (bars) | `preRoll`: none 0, one_bar 1, two_bars 2, four_bars 4 | O |
| `/crossfade` (+ `/str`, `/str/6\|8`, `/name`) | 0..1 (feature) | `transport.crossfade()` | O |
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

*Reference host:* `Reporter.collect_application`; model `Application` (undo history, panel layout), `Project`.

| Address | Value | Bitwig | Need |
| --- | --- | --- | --- |
| `/project/canUndo`, `/project/canRedo` | flag | `application.canUndo/canRedo` | Rec |
| `/host/features` | space separated feature names (§2 "Host features"; a binary host: the value operations) | all features, constant | Rec with host paging |
| `/project/engine` | flag | `hasActiveEngine` | O |
| `/project/name` | text (also schedules a note mapping update) | `application.projectName` | O |
| `/project/isDirty` | flag | `project.isModified` | O |
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

*Reference host:* `Reporter.collect_tracks`, `collect_channel`, `collect_sends`, `collect_track_flags`, `collect_vu`, `collect_effect_tracks`, `collect_master`. All items with absolute positions, no windows (§6 rule 4).

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
| `/track/fx/param/<i>/…`, `/track/fx/param/count` | params of the selected track's FX (Reaper), of the cursor track if the host pins it (`/track/pinned`); fields as `/device/param` | remote controls of the cursor track (fixed page cursors as for the device) | O |
| `/track/fx/page/count`, `/track/fx/page/selected`, `/track/fx/page/<i>/name` | (feature) the named pages of the cursor track's remote controls; the page selected in the host is shown | `pageNames`, `selectedPageIndex` of the following cursor of the cursor track | O |
| `/master/<field>` | same fields as a track (`name`, `color`, `select`, `volume`, `pan` + texts, `mute`, `solo`, `active`, `vu*`) | `createMasterTrack(0)` | Rec |
| `/master/fx/param/<i>/…`, `/master/fx/param/count` | master FX (also the crossfader from param 0, unless `/crossfade` is reported) | – (Reaper) | O |

**Effect tracks** (Bitwig's effect track bank, `createEffectTrackBank` with `numFxTracks` of the
model setup, -1 = `numTracks`): `/fxtrack/count`, `/fxtrack/offset` (the core pages them with
`bank fxtrack <offset>`) and `/fxtrack/<p>/<field>` with the fields of a track except slots and
VU (`exists`, `number`, `type`, `select`, `recarm`, `monitor`, `autoMonitor`, `canHoldNotes`,
`canHoldAudio`, the channel fields, `send/<s>/…`, `send/count`). Commands as for tracks with the
processor `fxtrack` (`fxtrack <p>/select|mute|solo|volume|send/<s>/volume…`, `fxtrack indicate`).
A host without them has no effect track bank (Reaper); the core's "toggle track bank" then does
nothing.

### 3.4 Clip launcher: slots and scenes

*Reference host:* `Reporter.collect_scenes`, `collect_slot`; the selection rules of the clip launcher navigator in `bindings/python/dbm_host/navigator.py`.

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

*Reference host:* `Reporter.collect_device`, `collect_specific`, `collect_primary`, `collect_pages`, `collect_eq`; model `bindings/python/dbm_host/device.py` (device chains, pages per role, remote controls, EQ+).

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

*Reference host:* `Reporter.collect_drum`, `collect_layer`, `collect_device_layers`; model `bindings/python/dbm_host/layer.py` (128 pads, the layers of a layered device).

`/<dev>/pad/<i>/<field>` with `<dev>` = `drum`, `device`, `primary`; `/<dev>/layer/<i>/<field>`
with `device`, `primary`; `/<dev>/layer/count`. Reporting any pad switches the pad bank to host
pads (the core pages locally over all reported pads; Bitwig reports all 128).

Fields: `exists` (Bitwig: the pad has a first device), `name`, `color`, `select`
(`addIsSelectedInEditorObserver`), `active`, `mute`, `solo`, `volume`, `volume/str`, `pan`,
`pan/str` (no shortened texts). Pad sends and devices are not reported.

### 3.7 Clip and step grid

*Reference host:* `Reporter.collect_clip`, `collect_grids`; model `bindings/python/dbm_host/clip.py` (notes with all attributes, one grid per `noteClips` size).

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

*Reference host:* not modelled (no scenario uses the browser).

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

*Reference host:* `Reporter.collect_markers`; model `bindings/python/dbm_host/marker.py`.

`/marker/count`, `/marker/<i>/exists`, `number`, `name` (empty: the core shows `Marker <i+1>`),
`color` (`position`, `endPosition` are accepted and ignored). `<i>` is the absolute position, the
core pages over all reported markers. Reaper reports its project markers (position in seconds).
Bitwig: a cue marker bank of 100 (`arranger.createCueMarkerBank`, only if `numMarkers > 0`):
`exists` for all 100, the other fields for existing markers (`position` in beats), `count` =
`itemCount` (at most 100).

### 3.10 The host's parameter pages

*Reference host:* `bindings/python/dbm_host/host_page.py` (`HostPageView.report`).

Only for a binary host with the `hostPages` feature (§2), and only between `hostpage show … 1`
and `… 0` (§4.12); everything again (a dump) after `show … 1` and after `refresh`. The page the
host shows: `/hostpage/exists`, `title` (the source's name on the surface), `epoch` (changed
whenever the cells hold other parameters), `columns`, `rows` (1..8), `/hostpage/page/count`,
`/hostpage/page/<p>/name`, `/hostpage/page/selected`; per cell (`<c>` = row · columns + column):
`/hostpage/cell/<c>/kind` (0 empty, 1 fader, 2 list, 3 toggle, 4 trigger, 5 relative, 6 action),
`name`, `color`, `value` (0..1), `text`, `steps` (list items), `on`, `enabled`, `set` (-1 none);
the control sets (`/hostpage/set/count`, `/hostpage/set/<s>/name|section|color`,
`/hostpage/set/selected`), the sections (`/hostpage/section/count`,
`/hostpage/section/<s>/name|color`, `/hostpage/section/selected`), `/hostpage/row`,
`/hostpage/cell/selected`. **The selection is the host's**: the core shows only what is reported
(doc/HOST_PAGE_PLAN.md §D.4, §3.5; the hand-over for zynthian in §12).

## 4. Command reference (core → host)

*Reference host:* `bindings/python/dbm_host/commands.py`: one handler per processor, registered in `CommandHandler.__init__` (`refresh`, `bank`, `transport`, `track`, `fxtrack`, `master`, `scene`, `action`, `device`, `primary`, `drum`, `noterepeat`, `eq`, `navigator`, `clip`, `project`, `marker`, `application`, `arranger`, `mixer`, `quantize`). An unknown command is logged once and listed in `CommandHandler.unhandled`.

Callbacks: `process_no_arg(processor, command)`, `process_int_arg` (booleans are 1/0),
`process_double_arg`, `process_string_arg`, `process_string_args` (only `track addTrack`). The
command is the path without leading `/`, its parts separated by `/`. The Bitwig bridge treats
int and double the same (`handle(processor, command, double)`), and turns a no-arg command
ending in `/reset` into the value path.

### 4.1 Value command suffixes (host features)

*Reference host:* `Command.parse` (`bindings/python/dbm_host/commands.py`) splits off the suffixes; `bindings/python/dbm_host/parameter.py` executes them (set, inc, reset, immediately, touch, indicate, the takeover mode).

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
| `time loop/start`, `time loop/length` | double seconds | Reaper path (the core sends `beats loop/…`) | converted to beats at the tempo, `arrangerLoopStart/Duration.set` |
| `time signature` | int `num + den*100` | set time signature | `timeSignature.numerator/denominator.set` |
| `beats position`, `beats position/nosnap` | double quarters | set position | `playStartPosition.set`, `jumpToPlayStartPosition` if playing |
| `beats loop/start`, `beats loop/length` | double quarters | set loop | `arrangerLoopStart/Duration.set` |
| `transport play\|stop\|restart` | none (`transportCommands`) | | `play()`, `stop()`, `restart()` |
| `transport stopAndRewind` | none | | `stop()`, `setPosition(0)` 100 ms later |
| `transport end` | none | to the end of the arrangement | action `jump_to_end_of_arrangement` |
| `transport metronome` / `toggleMetronome` | int / none | | `isMetronomeEnabled.set/toggle` |
| `transport togglePrerollMetronome` | none | | `isMetronomeAudibleDuringPreRoll.toggle` |
| `transport metronomeTicks <0\|1>`, `toggleMetronomeTicks` | | | `isMetronomeTickPlaybackEnabled` |
| `transport metronomeVolume[/inc]` | double | | `metronomeVolume` set/inc |
| `transport crossfade` (+ §4.1 suffixes), `crossfade/touch`, `crossfade/indicate` | double / int (with `/crossfade`) | crossfader | `transport.crossfade()` |
| `transport punchIn\|punchOut <0\|1>`, `togglePunchIn\|togglePunchOut` | | | `isPunchIn/OutEnabled` |
| `transport fill <0\|1>`, `toggleFill` | | | `isFillModeActive` |
| `transport overdub/arranger\|overdub/launcher <0\|1>` | | | `isArranger/ClipLauncherOverdubEnabled.set` |
| `transport tapTempo` | none | | `tapTempo` |
| `transport returnToArrangement` | none (`transportCommands`) | all tracks play the arrangement | `returnToArrangement()` |
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
| `track parameterPage` | int page (only after `/track/fx/page/selected`) | `selectedPageIndex.set` of the cursor track's remote controls (clamped) |
| `project createScene` | int (length in beats, ignored by Bitwig) | `createScene` |
| `project createSceneFromPlayingLauncherClips` | none | same name |
| `project previous\|next` | none | `application.previous/nextProject` |
| `project save\|load` | none | actions `Save`, `Open` |
| `groove <name>` (+ `/inc`, `/reset`, `/touch`, `/indicate`) | double | the six groove parameters |
| `groove active\|amount` | double | Reaper path, unhandled |
| `refresh` | none | **report everything with the next collection** |
| `bank track\|scene\|fxtrack` | int offset | `trackBank/sceneBank/effectTrackBank.scrollPosition.set` (`BitwigModel.scrollTrackWindow` for the tracks: the window commands which follow use the requested offset until Bitwig reports it). The core sends `bank fxtrack 0` at the start |

### 4.4 Tracks and master

`track <p>/<cmd>` with the absolute position; `master <cmd>` without position. The Bitwig bridge
resolves `<p>` in the window, else in the 400-track bank; sends and slots only in the window.

| Command | Arg | Meaning / Bitwig |
| --- | --- | --- |
| `track <p>/select`, `master select` | int 1 | select: `selectInEditor`, `selectInMixer`, `makeVisibleInArranger/Mixer`. The core always sends `1`; there is no deselection (Reaper selects with any value, Bitwig ignores `0`) |
| `track <p>/scrollto` | none | Reaper path (select and scroll); Bitwig: `scrollPosition.set(p)` |
| `track <p>/volume\|pan`, `master volume\|pan` | double (+ suffixes §4.1) | `volume/pan` |
| `…/volume/touch`, `…/pan/touch` | int | `touch` |
| `…/volume/indicate`, `…/pan/indicate` | int | `setIndication` |
| `track <p>/mute\|solo\|active`, `master …` | int 0/1 | `mute/solo/isActivated.set` |
| `track <p>/solo/toggle`, `master solo/toggle` | none (`soloToggle`) | `solo.toggleUsingPreferences(false)` (exclusive solo preference applies) |
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
| `track <p>/send/<s>/active` | double (the send volume, meaningless) | a **toggle**: Reaper `ToggleTrackSendUIMute`, Bitwig `isEnabled().toggle()`; the value is ignored |
| `track <p>/clip/<s>/…` | see §4.5 | |
| `track indicate` | int | `trackBank.setShouldShowClipLauncherFeedback` |
| `track pinned` / `track pinned/toggle` | int / none | `cursorTrack.isPinned.set/toggle` |
| `track selectPrevious\|selectNext` | none | `cursorTrack.selectPrevious/Next` |
| `track addTrack` | strings: type (`INSTRUMENT`, `EFFECT`, else audio), name, `RGB(…)` color, Reaper device names… | `createInstrument/Effect/AudioTrack(-1)`; name and color set on the cursor track 300 ms later; devices dropped |
| `track <p>/movePrev\|moveNext` | none | move the track up/down: the neighbour (400-track bank) is moved behind/before it (`after/beforeTrackInsertionPoint.moveTracks`) and selected in the editor |
| `track <p>/toggleMultiSelect` | int 1 | ignored (no multi-selection in the Bitwig API, as the Java version) |
| `track [<p>/]param/<i>/value` (+ §4.1), `param/<i>/touch\|indicate` | | the parameters of the cursor track (Reaper: track FX; Bitwig: the cursor track's remote controls, the position is not needed) |
| `track <p>/pin <pos>`, `recordClip`, `overdub`, `inQuantResolution`, `clearAutomation` | | Reaper commands, unhandled in Bitwig (most are not sent with the matching host feature) |
| `master user/param/0/value` | double | crossfader (Reaper master FX); not sent once the host reports `/crossfade` |

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
| `device <i+1>/bypass\|remove\|duplicate` | int / none (Reaper path) | the sibling whose chain position is `i`; `bypass <1\|0>` sets the bypass state (the core sends the current enabled state, so it toggles) |
| `device\|primary\|project parameterPage` | int page | show the page in the DAW (`selectedPageIndex.set`, clamped) |
| `<dev> param/<i>/value` (+ §4.1), `param/<i>/touch`, `param/<i>/indicate` | | flat index → page `i/8`, param `i%8`; EQ: frequency/gain/Q of band `i/3` |
| `eq band/<b>` | string Reaper type index | sets the EQ+ `TYPE` parameter |
| `eq add` | none | `insertBitwigDevice(EQ+)` at the end of the cursor track's chain |
| `drum\|primary clearMute\|clearSolo` | none | `drumPadBank.clearMutedPads/clearSoloedPads` |
| `drum\|primary indicate` | int | `drumPadBank.setIndication` |
| `drum\|primary pad/<i>/select` | none (`deviceCommands`) | `selectInEditor` only |
| `primary layer/<i>/select` | none | as channel select |
| `drum\|primary pad\|layer/<i>/mute\|solo\|active\|volume\|pan\|…` | as channel commands | on the pad/layer channel |
| `device <i+1>/movePrev\|moveNext` | none | move the cursor device: the neighbour of a 100-device bank of the chain is moved behind/before it (`after/beforeDeviceInsertionPoint.moveDevices`) and selected |
| `device preset` | int | Reaper preset browser (only without a host browser); ignored |
| `device add/0` | string Reaper creation name | ignored (Bitwig has no creation names) |

### 4.7 Cursor clip and step grid

| Command | Arg | Bitwig |
| --- | --- | --- |
| `clip name`, `clip color` | string (`RGB(…)`) | `setName`, `color.set` |
| `clip start\|end\|loopStart\|loopLength` | double beats (absolute; `/inc` relative) | `getPlayStart/PlayStop/LoopStart/LoopLength` set/inc |
| `clip loop\|shuffle\|pinned` | int | `isLoopEnabled`, `getShuffle`, `isPinned` (on all grids) |
| `clip accent`, `clip accent/immediately` | double 0..1 | `getAccent` set / setImmediately |
| `clip duplicate\|duplicateContent` | none | same names |
| `clip transpose` | int semitones | `transpose` (only on note tracks) |
| `clip clear` | none (without host step grid) | `clearSteps` |
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

| Command | Arg | Bitwig |
| --- | --- | --- |
| `marker previous\|next` | none | the play start position to the closest marker before/after the play position (`MarkerBankImpl.findAndSelect`); without a marker bank `jumpToPrevious/NextCueMarker` |
| `marker add` | none | `addCueMarkerAtPlaybackPosition` |
| `marker <i>/launch\|select\|remove` | none | the marker at the absolute position: `launch(true)`, play start position = marker position (jump if playing), `deleteObject` |

### 4.9 Browser

*Reference host:* not handled (see §3.8).

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

Logged once as `Not handled yet: …`: `cursor`, `inifile` keys other than `reaper/prerollmeas`
(the pre-roll length 0, 1, 2 or 4 bars → `preRoll`; other lengths are logged), and the Reaper-only
commands listed in the tables above. `action <id>` (string) invokes the Bitwig action with that
ID (`getAction(id).invoke()`); unknown IDs (e.g. Reaper's) are ignored silently, as the Java
version.

### 4.11 `delay_updates` / `enable_updates`

`enable_updates(processor, enable)`: the core does not need values of this area (e.g. the browser
is closed); `delay_updates(processor)`: do not report this area for ~300 ms (sent while
automation is recorded, so that old values do not jump back). Reaper honours both
(`DataCollector`); Bitwig ignores them (it only records them). Optional, an optimisation.

### 4.12 The host's parameter pages (`hostpage`, feature `hostPages`)

*Reference host:* `bindings/python/dbm_host/host_page.py`, `CommandHandler._host_page`.

| Command | Host action |
| --- | --- |
| `hostpage show/<columns>/<rows>/<encoders>/<rows per set> <0\|1>` | a surface shows (1) or no longer shows (0) the pages; with 1: choose the layout for the shape, report everything |
| `hostpage cell/<c>/turn/<travel\|detent>/<normal\|fine>/<epoch> <delta>` | turn a cell (the host's value math: `detent` one encoder step, `travel` delta/127 of the range); drop it if the epoch is not the current one |
| `hostpage cell/<c>/press/<epoch> <0\|1>` | press / release: a toggle switches, a trigger fires, a list steps, a continuous cell is selected (with its set and row) |
| `hostpage cell/<c>/touch/<epoch> <0\|1>`, `hostpage cell/<c>/reset/<epoch>` | the touch of the cell; its default value |
| `hostpage row <r>`, `hostpage page <p>`, `hostpage set <s>`, `hostpage section <s>` | select a row, show a page, select a set or a section (its first set) |

Every selecting command changes the host's own selection (its screen), which the host then
reports.

### 4.13 The controller's view and edit window (`controller view`, `clip editWindow`)

For a host which follows the controller on its own screen (zynthian). Command features
`controllerViews` and `clipEditWindow`: the core sends them only to a binary host which handles
them (its handled-commands bitset). *Reference host:* `ControllerFollower` in
`bindings/python/dbm_host/commands.py` (only with `SimDaw.controller_follower`).

| ID | Command | Value | When |
| --- | --- | --- | --- |
| 0x8D01 | `ControllerView`, `controller view` | i32 `kind` | the user picked a view with a button of the controller, also the one which is already active (SONG in the session view, STEP cycling the sequencers). Never on startup, a recalled preferred view, a track change, a setting change or a refresh |
| 0x8750 | `ClipEditWindow`, `clip editWindow` | struct `f64 start, f64 length, i32 low, i32 high` (text: `"<start> <length> <low> <high>"`) | the visible window of the step sequencer: right after a sequencer's `ControllerView` (also unchanged), then on every change of the window (page, step size, octave, scale, drum offset) or of the edited clip (track, slot) while the last picked view is a sequencer and a sequencer view is active; only real changes |

`kind` (`ControllerViewKind`, append only): 0 `Launcher`, 1 `Play`, 2 `Sequencer`, 3
`DrumSequencer`, 4 `Mixer`, 5 `Device`, 6 `Other`. It is a plain int on the wire and in the
generated bindings (Python `view.value` is an `int`, compare it with
`protocol.ControllerViewKind.SEQUENCER`); a host treats a number it does not know as `Other`
(do not convert it with `ControllerViewKind(value)`, which raises for a newer kind).

The window: `start` and `length` in beats (the edit page of the sequencer: page × steps × step
length; the raindrops sequencer: the clip's loop), `low` and `high` the MIDI notes (inclusive) of
the visible rows: the scale rows of a note sequencer, the play area of the poly sequencer, the
keys of the visible drum pads (16, 8 or 4 from the drum offset, also keys without a pad in the
host's drum machine). Also sent with the host's step grid (`clip grid/…`, zynthian): `start` is
then the scroll position of the last `GridScroll`/`GridPage` times the `GridStepSize` (in step
with them), and the rows come only from the core (scale, octave, drum offset). Python: `view.value` is a `protocol.EditWindow` (`.start`, `.length`, `.low`,
`.high`).

On the Maschine JAM: SONG → `Launcher`; STEP and STEP + scene → `Sequencer` (note, poly,
raindrops) or `DrumSequencer` (drum, Drum 4, Drum 8), then the window; PAD MODE and PAD MODE +
scene → `Play` (also Drum 64: it plays); LEVEL, AUX → `Mixer`; CONTROL → `Device`. Another
controller calls `Ctx::report_controller_view` from its view buttons
(`ViewMultiSelectCommand::reporting`) and forwards `View::clip_edit_window` of its sequencer
views (`src/framework/controller_view.rs`).

The Bitwig bridge does not handle them (in `CommandHandler.UNHANDLED`): in the Java original the
modes and views never switch Bitwig's panels, and the API has no scrolling of an editor.

## 5. MIDI and note inputs

*Reference host:* `Bridge.send_midi_data`, `process_midi_arg` and `set_note_input_*` (`bindings/python/dbm_host/bridge.py`); the note inputs and the playing notes in `bindings/python/dbm_host/model.py` (`NoteInput`).

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

## 5a. USB devices (the Push 2/3 display)

*Reference host:* `Bridge.usb_bulk_out` (`bindings/python/dbm_host/bridge.py`) and the transports
of `bindings/python/drivenbymoss/usb.py`; Bitwig: `UsbOutput` and
`DbmExtensionDefinition.listHardwareDevices` (`backends/bitwig`).

Some controllers use a USB device besides MIDI: the Push 2 (`2982:1967`) and Push 3
(`2982:1969`) display is USB interface 0, bulk OUT endpoint `0x01` (the pads and buttons stay
class compliant USB MIDI). The core renders the display and hands the host the bytes to write:

```c
typedef int (*DbmUsbBulkOut)(void *user, int instance, int device_index, uint16_t vendor_id,
                             uint16_t product_id, uint8_t interface_number, uint8_t endpoint,
                             const uint8_t *data, size_t length);
int dbm_set_usb_callback(DbmApp *app, uint32_t version /* DBM_USB_VERSION */, DbmUsbBulkOut callback, void *user);
char *dbm_list_usb_devices(const char *controller_id);   /* JSON, dbm_free_string */
```

- **Registration:** `dbm_set_usb_callback` before `dbm_start_infrastructure` (it can be called
  later and again; null removes the callback). Returns 1, or 0 for another `DBM_USB_VERSION` or
  a null `app`. A separate setter (as `dbm_set_command_callback`): `DbmCallbacks` keeps its
  layout. Without a callback the writes are dropped and the controller runs without the device.
- **Arguments:** `instance` is the index of the controller instance (0 for
  `dbm_create_controller`), `device_index` the position in the controller's USB device list
  (`dbm_list_usb_devices`); `vendor_id`, `product_id`, `interface_number` are that device's, so a
  simple host needs no lookup. `data` is the complete byte stream of the write, valid during the
  call. Return non-zero if written or queued.
- **A Push frame** is one write of 327,696 bytes: the 16-byte header `FF CC AA 88` + 12 zero
  bytes, then 327,680 bytes of pixel data (160 lines of 2,048 bytes, RGB565 little endian,
  already XOR-masked with `E7 F3 E7 FF`). Write it as it is; a transport may split it into
  transfers (the Java version writes the header and the image as two bulk writes, the Python
  usbdevfs writer additionally chunks the image into 16 KiB transfers). The core limits the rate:
  changed frames at most ~30 per second, the unchanged frame again about every second (the Push
  blanks its display without frames).
- **Threading:** as all callbacks: on the thread of the call into the library (or the internal
  timer thread), with the instance locked, and the callback must not call into the instance. It
  must not block: copy the data, write it on another thread, and drop a frame rather than stall
  (a newer frame replaces one which was not written yet).
- **Claiming up front:** hosts which declare the devices before the controller runs (Bitwig's
  `listHardwareDevices`, udev rules) take the list from `dbm_list_usb_devices` (or
  `dbm-catalog --usb`; `backends/bitwig/tools/gen_definitions.py` generates it into the Bitwig
  definitions).

The transports of the hosts:

| Host | Transport |
| --- | --- |
| Bitwig | Bitwig's USB API: the definition claims the device (`UsbDeviceMatcher` with an interface and a bulk endpoint matcher), the extension writes with `host.hardwareDevice(i)` → `iface(0).pipe(0)` → `UsbOutputPipe.write` on a thread of its own (`UsbOutput.java`); with `DBM_PUSH_DISPLAY_SOCKET` set when Bitwig starts, `UsbOutput` writes to that socket instead (the Push simulator) and the definition claims no USB device |
| Python (reference host, zynthian) | `drivenbymoss.Backend.usb_bulk_out(device_index, endpoint, data)` (overridable; the device's IDs in `backend.usb_device`), by default queued for `drivenbymoss.usb.UsbOutput.from_environment()`: the Push simulator's socket if `DBM_PUSH_DISPLAY_SOCKET` is set, else on Linux usbdevfs (`/dev/bus/usb/BBB/DDD`, needs the udev rule `support/linux/ni_host/packaging/70-dbm-nihost.rules` or root) |
| Reaper | not a target (no USB transport) |

**The simulator socket** (`DBM_PUSH_DISPLAY_SOCKET`, the simulator's default
`$XDG_RUNTIME_DIR/dbm-push-display.sock`; `DBM_USB_TRANSPORT=socket` uses that default,
`DBM_USB_TRANSPORT=none` disables USB, `=usbdevfs` forces the device): a UNIX stream socket; per
frame exactly the bytes of the write (header + 327,680 data bytes), no length prefix, frames
back to back on one connection. The host connects lazily, writes with a 1 s timeout, and after
any error closes the connection and tries again at most once per second; a frame is either
written completely or the connection is closed, so a new connection always starts with a
header (or the last message of the simulator side channel, see below). Frames which come while
the previous one is still being written replace each other (dropped).

### Simulator side channel (the Push 2/3 encoder state)

*Reference host:* `Backend.simulator_data_out` / `App.use_simulator_data`
(`bindings/python/drivenbymoss/__init__.py`), `UsbOutput.write_simulator` (`usb.py`),
`support/push_simulator/tools/dbm_host_alsa.py`; Bitwig: `UsbOutput.writeSimulator`,
`NativeCore.setSimulatorCallback`. Core: `src/controller/push/sim_values.rs`.

Data for the Push simulator only, which the hardware does not have: the state of the encoders
(the real Push has endless encoders without LED rings, the values are only on the display). The
simulator draws it as rings around its encoders. It is meant to grow (more message kinds, maybe
another transport for a tablet controller), but must never reach a real device. So it has its own
callback, not the USB one:

```c
typedef int (*DbmSimulatorData)(void *user, int instance, const uint8_t *data, size_t length);
int dbm_set_simulator_callback(DbmApp *app, uint32_t version /* DBM_SIMULATOR_VERSION */, DbmSimulatorData callback, void *user);
```

- **Gating:** only a host whose USB data goes to a simulator registers it. The core builds the
  data only while a callback is set (`Backend::simulator_data_enabled`), so other hosts pay
  nothing. The Python binding and Bitwig register it when their USB transport is the socket
  (`DBM_PUSH_DISPLAY_SOCKET`) and `DBM_PUSH_SIM_VALUES` is not `0`
  (`drivenbymoss.usb.simulator_data_enabled()`, `UsbOutput.simulatorDataEnabled()`). Their device
  transports (Bitwig's pipes, usbdevfs) never get the data: `write_simulator` / `writeSimulator`
  return false without the socket. `DbmCallbacks` keeps its layout. It can be set at any time.
- **Data:** each call is one complete message, the exact bytes for the simulator's stream. Write
  it on the same connection as the frames, between two writes (never inside a frame). It is the
  complete state, so a newer message may replace one which was not written yet. The core sends a
  message only on change (the first one after the start). The socket transports keep the last
  message and write it again first after every (re)connect, so a simulator started later gets
  the state at once.
- **Threading:** as `DbmUsbBulkOut`.

**The message:**
`"DBMV"`, version (1 byte, 1), kind (1 byte), payload length (3 bytes, 7 bits each, least
significant first), payload. No message contains the byte `0xFF`: all numbers are 7-bit and UTF-8
has no `0xFF`. A message can therefore never contain the frame header (`FF CC AA 88` + 12 zeros),
so a reader which only scans for frame headers (older simulators) skips messages like garbage. A
reader of messages skips an unknown version or kind by its length, and rejects a "message" with a
`0xFF` in it (garbage which happened to contain `DBMV`, or a frame which follows).

Kind 1, the encoder state: the number of encoders (1 byte), then per encoder:

| Field | Size | Meaning |
| --- | --- | --- |
| index | 1 | the encoder's touch note: 0-7 the encoders over the display, 8 master, 9 the small right encoder (Push 2 "Swing", the play position in DrivenByMoss), 10 tempo |
| flags | 1 | 1 = has a value, 2 = touched, 4 = bipolar (the ring starts at the middle, e.g. panorama) |
| value, max | 2 + 2 | 14 bits each, 7 bits per byte, least significant first; the ring shows value / max |
| name | 1 + n | length (0-127) and UTF-8 bytes (cut at a character boundary) |
| text | 1 + n | the value text, the same way |

The Push 2/3 takes the state from what the display shows, so no mode knows about it: the
components of the 8 columns (a parameter: its name, value, value text and touch state; a channel
in a volume, panorama or crossfader mode: the track name and that value; a sends column: the
edited send), plus the master volume, the play position text and the tempo text from the model.
The modes show the value text of a channel only while its encoder is touched, so the text is
empty otherwise and "touched" means the text is shown. Columns without a single value (the track
mode's channel with all values, lists, options) send the name without a value. Push 3 takes the
same code path (no position encoder: its small encoder is tempo/swing); Push 1 sends nothing.

## 5b. Devices driven by the core (the Maschines over libdbm_nihost)

*Code:* `src/nihost/` (selection, the library, the note inputs), `App::start_instance` /
`tick` / `poll_devices` (`src/app.rs`).

On Linux the core can drive the NI Maschine Mk2 and JAM itself through `libdbm_nihost.so`
(`support/linux/ni_host`), without the host's MIDI ports. A host opts in per instance:

- **`DBM_FLAG_DEVICE_TRANSPORT` (2)** in the flags of `dbm_create_controller` (not
  `dbm_create`). Without it the core always uses the host's ports (as before; the replays, the
  Python `Bridge` by default and the zynthian drivers, which drive the device themselves).
- **The selection** at the start of the controller (logged, `<model>: drives the device
  in-process ...` or `<model>: uses the MIDI ports ...: <reason>`): only for a controller with a
  device (`ControllerFactory::hid_device`), only on Linux. The environment variable
  `DBM_MASCHINE_TRANSPORT`: `midi` (the ports), `hid` (the library, also while another process
  holds the device: it waits), `auto` (default): the library unless it is missing (searched at
  `$DBM_NIHOST_LIBRARY`, `/usr/lib/drivenbymoss`, `/usr/lib/dbm-nihost`,
  `/usr/local/lib/dbm-nihost`, `~/.local/lib/dbm-nihost`, the repository's builds), a dbm-nihost
  process drives the device (`/proc`), or the device's hidraw node is locked by another process
  or not accessible (the udev rule). A device which isn't connected is waited for.
- **While the device is driven in-process**, the first port pair (device ID 0) is the device:
  the core doesn't call `open_midi_*` / `close_midi_*` for it and never `send_midi_data(0, …)`.
  MIDI the host still delivers with `dbm_on_midi_event(0, …)` is handled as before (there is
  none without a connected port). The host needs no MIDI ports for it (Bitwig: the existing
  definitions, the ports left unassigned).
- **Note inputs:** the core still calls `set_note_input_filters` and the key/velocity tables (a
  host creates its note input as before; Bitwig during `init`), but it applies them itself:
  each matching message of the device goes to the host as a played note,
  `process_midi_arg(0, status, data1, data2)`, already translated (a note-off to the key of its
  note-on). So the host must inject `process_midi_arg` into its default note input (§5), which it
  does for the chords anyway.
- **Threading and timing:** the library has its own thread (it reads the device into a queue);
  the core reads the queue without waiting, under the instance lock, in `dbm_tick` and in
  **`dbm_poll_devices(app)`**, which only reads the devices and handles their input (the number
  of messages; -1 if the instance drives no device in-process, then there is no need to call it).
  Call it every few ms between the ticks so that the pads play without the tick's delay (Bitwig:
  every 3 ms, `DbmExtension.pollLoop`). The changed LEDs and displays are written after each
  tick's flush.
- **Unplug and replug:** the library sends "gone" and the hello (support/linux/ni_host/README.md);
  the controller keeps running, the core ends the sounding notes on "gone" and sends all states
  again on the next hello.

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
   tells the bridge (`bank track|scene|fxtrack <offset>`); the bridge scrolls its window and
   reports the window with absolute positions. A bridge may also report all items, as the Reaper
   backend and the Python reference host do; then it only keeps the offset for `/track/offset`. Commands address absolute positions; a bridge with only a
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
8. **Known gaps of the Bitwig bridge** (useful when a new host does better): hierarchical track
   lists, extra scene/slot banks, a slot bank of the cursor track (scene
   navigation), the cursor layer, sends and devices of pads/layers, the page names of the cursor
   track's remote controls.

## 7. Minimal bridge checklist, then progressive features

The Python reference host (`bindings/python/dbm_host/`) has all of these steps; its
`bindings/python/tests/test_host.py` starts with step 1 (a Launch Control XL mixing simulated
tracks).

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
after value commands, §6.1), `valueReset`, `immediately`, `indication`; on the binary form the
command features come with the commands the host declares as handled (`soloToggle`,
`transportCommands`, `deviceCommands`, `layoutStep`), on the text form they are listed too. `track pinned|selectPrevious|selectNext` come
with the reports `/track/pinned|hasPrevious|hasNext`.

**Step 3: features, each independent** (report the address, handle its commands, §2 table):
sends; slots and scenes; named parameter pages; shortened texts; automation and overdub switches;
punch/fill/ticks; record quantization; project solo/mute, cue, remote controls; application
layout and arranger/mixer flags; groove; drum pads (`/drum`, `/primary`), layers, EQ; host step
grid of the note clips; browser; playing notes; markers.

## 8. Testing a bridge

- **Unit tests of the core** (`cargo test`): every address has a parser test in
  `src/parser.rs` (e.g. `host_paging`, `host_automation_switches`), every command a test of the
  core type that sends it, using `RecordingBackend` (records `BackendCall`s, the commands typed as `Cmd`). A new address or
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
  traces against a Bitwig run of the same scenarios.
- **In-process conformance on the reference host** (`bindings/python/conformance/`,
  PYTHON_HOST_PLAN.md §4): the same scenarios against a bridge in the same process, without MIDI
  ports or a DAW.
  - **How it runs:** the core runs on the simulated time (`dbm_set_time`). The scenarios' OSC
    commands become model operations (`conformance/osc.py`), and a state view gives the keys of
    the Java OSC observer (`conformance/state.py`).
  - **What it compares against:** the results of the Java version on Bitwig
    (`tests/conformance/<setup>/replay/*-java.ref.json`, written by `python3 -m
    conformance.extract` from the Bitwig runs' traces).
  - **Commands:**
    - `cd bindings/python && python3 -m conformance --all` runs all four setups; `ci/linux.sh`
      runs the passing files.
    - For another host: put your bridge behind the runner instead of `SimDaw` (or reuse the
      runner's comparison with your own DAW observer). A difference then shows where your bridge
      differs from Bitwig.
- **Debugging aids of the Bitwig bridge worth copying:** log each unhandled command once
  (with digits replaced by `<n>`), `DBM_LOG_MIDI` to log MIDI in/out, a dev mode that reloads the
  library when it is rebuilt (`DBM_DEV_LIBRARY`), and `tools/osc_monitor.py`.
