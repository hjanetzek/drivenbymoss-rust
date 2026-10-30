# Bitwig Studio backend: draft plan

**Goal.** Run the DrivenByMoss Rust core in Bitwig Studio, through a thin Java extension that
speaks the backend protocol ([BACKEND_PROTOCOL.md](BACKEND_PROTOCOL.md)) on top of the Bitwig
Controller API. The original DrivenByMoss is developed against Bitwig in Java, so the backend also
gets a **dev mode**: rebuild the native library and the running extension picks it up, without
restarting Bitwig.

**Baseline: Bitwig Studio 6.1** with Controller API 25 (`com.bitwig:extension-api:25`).
- Bitwig 6.1.3 ships Java 25 (`lib/jre/release`: `JAVA_VERSION="25.0.4.1"`). Bitwig 5.3 ships
  Java 21.
- With Java 22+, the Foreign Function & Memory API (FFM, `java.lang.foreign`) is final. The
  extension calls the C ABI directly: no JNI code, no JNA. Bitwig 5.x would need JNA or a JNI
  shim, and is not a target.

Status (2026-10-29): steps 1–3 are implemented. The C ABI for single-controller hosts, the
extension in `backends/bitwig/` (FFM binding, generated definitions, dev mode) and the stage 1
model and commands (transport, tracks, sends, master, cursor device with remote-control pages,
mapped action IDs). Tested in Bitwig 5.3 on JDK 25 with the Maschine Mk2, Maschine JAM and Launch
Control XL; the dev reload works in Bitwig. For development on Bitwig 5.3 the extension builds against API 21 (§3, "Bitwig 5.3
with a newer JDK").

Where the Bitwig backend behaves differently from the Java DrivenByMoss for Bitwig is listed in
[BITWIG_DIVERGENCES.md](BITWIG_DIVERGENCES.md).

## 1. Architecture

```
Bitwig Studio 6.1 (Java 25)
 └─ DrivenByMoss-Rust.bwextension            rust/backends/bitwig/ (Maven)
     ├─ one ControllerExtensionDefinition per controller (generated)
     ├─ DbmExtension (ControllerExtension)    init / flush / exit
     │   ├─ NativeCore      FFM bindings of include/drivenbymoss.h, loads the library
     │   ├─ CommandHandler  core → host: processor/command/value → Bitwig API calls
     │   ├─ ModelReporter   Bitwig observers → model update lines, sent on flush()
     │   └─ MidiBridge      MidiIn/MidiOut/NoteInput ↔ dbm_on_midi_event / callbacks
     └─ native/<platform>/libdrivenbymoss.{so,dll,dylib}
```

- **Threading.** Bitwig calls the extension on its controller thread. The core is created with
  `DBM_FLAG_HOST_TICK` and ticked from `host.scheduleTask` every 30 ms, so all callbacks
  (commands, MIDI out) arrive on that thread and may call the Bitwig API directly.
- **Model updates.** Observers only record changes into a line buffer (like `DataCollector`'s
  change cache). The buffer is handed to `dbm_update_model` once per tick. On a `refresh` command
  the reporter emits every value (full dump).
- **MIDI.** `MidiIn.setMidiCallback`/`setSysexCallback` → `dbm_on_midi_event`;
  `send_midi_data` → `MidiOut.sendMidi`/`sendSysex`. The note input callbacks map one to one onto
  Bitwig's `NoteInput` (`setKeyTranslationTable`, `setVelocityTranslationTable`, the input
  filter masks); the Reaper backend emulates exactly this API.

## 2. Controller definitions and instances

Bitwig needs one `ControllerExtensionDefinition` subclass per controller, listed in
`META-INF/services`, each with vendor, model, UUID, port counts and auto-detection port names. One
extension instance is one controller with fixed ports, which the user assigns in Bitwig.

- **Definitions are generated.** The core already has everything in its factories
  (`ControllerDefinitionInfo`: uuid, vendor, hardware model, port counts; `midi_discovery_pairs`
  per OS). New C ABI call `dbm_list_controllers()` returns them as JSON. A Maven step (or a small
  `cargo run --bin dbm-controllers`) generates the Java subclasses and the services file.
- **UUIDs.** The Java DrivenByMoss uses the same UUIDs for the same controllers. Both extensions
  can be installed side by side, so the Rust one derives its own (name-based UUID from the
  original plus a suffix) and shows the model as e.g. `Launchpad Pro (Rust)`.
- **Single-controller instances.** Today `App` reads `CONTROLLER_INSTANCE<n>` from
  `DrivenByMoss4Reaper.config` and opens ports by name. New C ABI entry
  `dbm_create_controller(callbacks, controller_uuid, config_dir, app_version, flags)`: an app
  with exactly one instance whose MIDI ports are the extension's ports (device IDs 0..n-1); port
  discovery is skipped.
- **Settings.**
  - Stage 1: the existing per-controller `.config` files, in a DrivenByMoss folder under the
    Bitwig user directory.
  - Stage 2 (done): the core lists its settings (`dbm_get_settings`: one tab separated line per
    setting with id, scope, kind, category, label, default, value, options or range); the
    extension (`HostSettings`) creates the matching `host.getPreferences()` and
    `host.getDocumentState()` settings, sends Bitwig's values to the core (`dbm_set_setting`) and
    sets the changes of the core in Bitwig (`dbm_take_setting_changes`). Bitwig keeps the values;
    the document settings replace `dbm_get/set_formatted_document_settings`. The `.config` file
    is still written but Bitwig's values win on start.
- **Notifications:** the core queues them (`Host::show_notification`), the extension shows them
  with `showPopupNotification` after each tick (`dbm_take_notifications`).

## 3. Native library loading and dev mode

### Loading (all modes)

1. Find the library: in dev mode the configured path, otherwise `native/<os>-<arch>/` inside the
   `.bwextension`.
2. Copy it to a fresh file in a cache directory (`~/.cache/drivenbymoss/libdrivenbymoss-<hash>.so`,
   by content hash). Reasons:
   - `dlopen` of a path that is already loaded returns the old handle.
   - A Rust `cdylib` that registered thread-local destructors cannot be unloaded on glibc
     (`dlclose` keeps it), so a new version must have a new file name to be loaded at all.
   - The build can overwrite the original at any time without crashing the loaded copy.
3. `SymbolLookup.libraryLookup(copy, arena)` with an `Arena.ofShared()` owned by the extension
   instance; downcall handles for the `dbm_*` functions; upcall stubs for `DbmCallbacks`, allocated
   in the same arena.
4. On `exit()`: `dbm_destroy`, then `arena.close()`, which releases the upcall stubs and unloads
   the library where the platform allows it.

FFM calls are "restricted methods": without `--enable-native-access` the JVM prints one warning
and allows them (Java 22–25 default). Check what Bitwig 6.1 shows (?).

### Dev mode: reload the native library while Bitwig runs

- **Enable:** a preference "Development: native library" (path, e.g.
  `…/rust/target/debug/libdrivenbymoss.so`) or the environment variable `DBM_DEV_LIBRARY`.
- **Watch:** a `host.scheduleTask` poll every 500 ms compares size and modification time. A
  change counts once it is stable for two polls (cargo writes the file in place).
- **Reload:** the extension calls `host.restart()`. Bitwig runs `exit()` (destroy, unload) and a
  fresh `init()`, which loads the new copy and recreates all Bitwig API objects. The core sends
  `refresh` on start, so it gets a full dump and the controller redraws. Expected turnaround:
  build time plus about 1–2 s.
  - Alternative without `restart()`: swap only the core and keep the Bitwig objects (destroy,
    unload, load, create, start, refresh). Faster, but every Java-side cache has to be reset
    correctly. Only worth it if `restart()` turns out too slow.
- **Kept across reloads:** document settings are kept by Bitwig's document state (stage 1 kept
  them in a static holder with `dbm_get/set_formatted_document_settings`). The core's own state (active mode, view, scroll positions) starts fresh.
- **Leak:** a library that cannot be unloaded stays mapped, one copy per reload (a debug build is
  tens of MB). Fine for a dev session; the cache directory is cleaned on the next start of Bitwig.
- **Java changes:** Bitwig watches the `Extensions` folder and reloads a changed `.bwextension`
  (its log: "Adding file system watchers recursively for directory …/Bitwig Studio/Extensions").
  `mvn install` copies the extension there. Verify on 6.1 that a running controller is reloaded
  (?).
- **Loop:** `cargo watch -x 'build --lib'` (or plain `cargo build --lib`) in `rust/`, Bitwig
  open with the controller active. Logs: the core's log callback goes to `host.println`
  (Controller Script Console) and stderr.

### Bitwig 5.3 with a newer JDK

Until a Bitwig 6.1 license is available, a copy of Bitwig 5.3 runs on JDK 25:

- Copy `/opt/bitwig-studio` to `~/.local/opt/bitwig-studio-jdk25` (the package stays untouched).
- Replace `lib/jre` by a folder whose `bin/java` is a script that drops `-XX:+ZGenerational`
  (newer JDKs reject it) and executes `/usr/lib/jvm/java-25-openjdk/bin/java` (or
  `$BITWIG_JAVA_HOME/bin/java`). Bitwig starts Java through `bin/BitwigStudio` → `lib/jre/bin/java`.
- Java extensions work. Bitwig's JavaScript controller scripts do not load (its GraalJS needs
  `sun.misc.Unsafe.ensureClassInitialized`, removed after Java 21).

## 4. Model and protocol

The core and the protocol are Reaper-shaped: the host sends everything flat and the core pages,
time is in seconds, scenes are regions, slots are media items, and many features are Reaper action
IDs (BACKEND_PROTOCOL.md §4–§6). The Bitwig API is the opposite in the places that matter.

### Differences that shape the design

| Topic | Bitwig | Core / protocol today |
| --- | --- | --- |
| Object creation | Every bank, cursor and device must be created in `init()` with fixed sizes | Core pages over flat lists; sizes are core-internal (`ModelSetup`) |
| Banks | Fixed windows that the host scrolls (`scrollPosition`, `canScrollForwards`, `followCursorTrack`) | Host sends all items; core keeps the offset |
| Device parameters | Named remote-control pages of 8 (`createCursorRemoteControlsPage`) | Flat parameter list, core pages it as "Page n" |
| Time | Beats (`getPosition`, loop start/duration) | Seconds |
| Slots and scenes | Real launcher slots with states (playing, recording, queued, stop queued, has content) | Media items and regions; states derived from the transport position, queued states always false |
| Drum pads, layers | Real channels (`createDrumPadBank`, `createLayerBank`) | Not modeled (layers) or virtual (pads: commands go to `track <pad>`) |
| Cursor clip | Step grid window (`createLauncherCursorClip`) with a dozen per-step fields | Note list in quarters, the core builds the grid |
| Browser | Popup browser with content types, 8 filter columns, results | Presets only (stub) |
| Transport extras, panels | Real API (metronome, tap tempo, preroll, punch, automation write modes, panel layout) | Reaper action IDs |

Details per interface, with effort ratings: see the comparison notes in §7.

### Bank manifest

Bitwig needs the bank sizes during `init()`. The extension creates the core synchronously in
`init()` (`dbm_create_controller`), then asks it for a **manifest**: `dbm_model_setup()` returns
the controller's `ModelSetup` as JSON (numbers of tracks, sends, scenes, devices, parameters,
layers, drum pads, markers, browser columns and results, the specific devices, whether a drum
64 bank, note clip grids, cursor layer, focused parameter are wanted). The extension creates
exactly these Bitwig objects. After a dev reload, `init()` runs again, so a changed `ModelSetup`
takes effect too.

### Stage 1: the current protocol on Bitwig

Map Bitwig onto the existing addresses and commands, without core changes:

- **Flat lists from bounded banks.** Tracks: a track bank of `max(numTracks, 64)` (?) with
  `numSends` and `numScenes`, reported as `/track/<n>/…` plus `/track/count` (capped). Markers:
  a cue marker bank. Device siblings: `createSiblingsDeviceBank(8)` matches the host window of 8.
- **Parameters:** the remote-control pages of the cursor device, reported as one flat list in
  page order (`numParamPages` pages of 8 via several `CursorRemoteControlsPage`s, as
  `ParameterListImpl` does). Track and project remote controls fill `/track/fx/param/*` and
  `/master/fx/param/*`.
- **Time:** beats converted to seconds at the current tempo (wrong with tempo changes; fixed in
  stage 2).
- **Commands:** `track`, `master`, `device`, `eq` (EQ+), `play/stop/record/repeat/time/tempo`,
  `project`, `marker`, `groove`, `noterepeat` (→ `NoteInput.arpeggiator()`), `undo/redo`,
  `refresh`. The Reaper action IDs the core uses are mapped to Bitwig API calls in a table
  (metronome, tap tempo, preroll, punch, duplicate, marker previous/next, panels).
- **Left out:** clips and scenes beyond what regions/items mean, the browser, layers, real drum
  pads, the cursor clip.

This gets the Launch Control XL and the Launchpad's mixer and device modes running, and with them
the dev loop.

### Stage 2: protocol v2 (core changes, also used by Reaper where it can)

Each item is a capability the backend declares at create time (the BINDINGS_PLAN todo), so the
Reaper backend keeps working:

1. **Real commands instead of action IDs** (done): metronome, tap tempo, preroll, punch, panel
   layout, view toggles, project and marker navigation, track duplicate (BACKEND_PROTOCOL.md §2).
   The Reaper backend maps them back to its action IDs. No capability is needed: a backend
   ignores the commands of features it does not have. The automation write mode is still in
   Reaper's numbering.
2. **Time in beats** (done): `/beats/position`, `/beats/loop/start`, `/beats/loop/length` (the
   real length) and the `beats` commands; the core moves the position and the loop in beats.
   Reaper converts with its tempo map, Bitwig follows `TransportImpl` of the Java version
   (BITWIG_DIVERGENCES.md §3). Bitwig also sends its own position and loop texts
   (`/time/text`, `/beats/text`, `/beats/loop/*/text`), which the core shows as they are.
3. **Slots and scenes** (done): slot states and launch commands with release/alt, track stop
   and return to arrangement, slot color and paste, new clips in a given slot, scenes without
   positions (BACKEND_PROTOCOL.md §2, §3). Reported slot states replace the derived ones, so no
   capability is needed; Reaper keeps its behaviour.
4. **Parameter pages** (done): page names and the page selected in Bitwig are reported, the core
   tells the host its selected page (`device parameterPage`); parameter `modulated` value,
   `touch`, `indicate`. The parameters stay a flat list of up to `numParamPages` pages.
5. **Host-side paging** (done for tracks and scenes): the host reports `/track/offset` and
   `/scene/offset` and gets the core's offsets (`bank track|scene`); Bitwig's track and scene banks
   are windows of `numTracks` and `numScenes` as in the Java version. Device siblings and
   parameter pages are not host paged yet.
6. **Drum pads** (done) as channels with their own addresses and commands (`/drum/pad/<p>/…`,
   `drum pad/<p>/…`); **device layers** (done) of the first instrument (`/primary/layer/<i>/…`,
   `primary layer/<i>/…`), whose commands now go to `primary` instead of `device`.
7. **Browser** (done) with content types, filter columns and results (`/browser/…`, `browser …`);
   the controllers browse slots and drum pads again, the Maschine shows the filter columns.
8. **Cursor clip step grid** (done) with the Bitwig step fields; the grid sizes come from the
   model setup (`noteClips`), one launcher cursor clip per size as in the Java version.

## 5. Steps

1. **C ABI for single-controller hosts.** `dbm_list_controllers()` (JSON: uuid, vendor, model,
   ports, discovery pairs per OS), `dbm_create_controller(...)`, `dbm_model_setup()`, a log
   callback. Rust tests as for the existing C ABI.
2. **Extension skeleton** in `rust/backends/bitwig/` (Maven, API 25, `--release 25`):
   - `NativeCore` with FFM (downcalls, upcall stubs, arena), loading as in §3.
   - Generated definitions (`(Rust)` names, own UUIDs) and the services file.
   - MIDI bridge, note input, `scheduleTask` tick, `exit()`.
   - Dev mode: watch, `host.restart()`, settings carried over.
   - Milestone: the Launch Control XL starts, LEDs light, knob MIDI reaches the core; rebuilding
     the library reloads it in Bitwig.
3. **Stage 1 model and commands** (§4): transport, tracks, sends, master, cursor device with
   remote-control pages, EQ+, first instrument, markers, project, groove, note repeat, the action
   ID table. Milestone: Launch Control XL and Launchpad (mixer, device, note modes) usable.
4. **Packaging:** the native libraries for Linux x86_64/aarch64, Windows and macOS (universal) in
   the `.bwextension`; `ci/linux.sh` builds the extension.
5. **Settings in Bitwig** (preferences and document state from the core's settings list) and
   popup notifications. Done.
6. **Protocol v2** items in order of use: action-ID commands, beats, slots and scenes, parameter
   pages, drum pads, layers, browser, cursor clip. Then the Maschine (session and drum features)
   and the remaining controllers.

## 6. Open questions

1. **`host.restart()` turnaround** and class loader behaviour on Bitwig 6.1: does a restart
   unload the old classes, and does the library copy get unloaded? Measure before choosing
   between restart and in-place swap.
2. **FFM native-access warning** in Bitwig's console; whether Bitwig passes
   `--enable-native-access` or the extension manifest can declare it (`Enable-Native-Access` in
   the jar manifest works for executable jars only (?)).
3. **Flat bank size** for stage 1 (64 tracks?) and its observer cost, versus doing host paging
   early.
4. **Side by side with the Java DrivenByMoss:** separate UUIDs and names are planned; check that
   both can drive different controllers in one Bitwig session.
5. **Hardware surface:** the Java version builds Bitwig's `HardwareSurface` (on-screen simulator,
   Bitwig mappings). The native core handles MIDI itself; decide whether a surface description is
   exported later.
6. **USB devices:** the Maschine on Linux goes through `dbm-nihost`. Bitwig has its own USB API
   (`listHardwareDevices`), which the Java version uses for the Push 2/3 displays; native USB
   access from the core may conflict with Bitwig claiming the device.
7. **One core per extension instance** (simplest) or one shared core for all instances (shared
   model data, less observer load).

## 7. Comparison notes

Bitwig implementation (`DrivenByMoss/src/main/java/de/mossgrabers/bitwig/framework/**`) against
the Reaper implementation and the Rust core, compiled from the code. (?) marks unconfirmed points.
`BW/` = the Bitwig implementation, `RP/` = the Java Reaper implementation
(`src/main/java/de/mossgrabers/reaper/framework/daw/**` in this repository). A bare §n refers to
BACKEND_PROTOCOL.md. The notes describe the Java DrivenByMoss as built today (API 21, Java 21);
the plan targets API 25 and Java 25.

### 7.1 Banks and paging

#### 7.1.1 How BW creates banks (`BW/daw/ModelImpl.java` constructor, all in `init()`)

| Bank | BW call | Size source |
|---|---|---|
| Cursor track | `createCursorTrack ("MyCursorTrackID", "The Cursor Track", numSends, numScenes, true)` | ModelSetup |
| Track bank | `createTrackBank (numTracks, numSends, numScenes, true)` if `hasFullFlatTrackList`, else `createMainTrackBank (numTracks, numSends, numScenes)`; then `tb.followCursorTrack (bwCursorTrack)`. Hierarchical: `bwCursorTrack.createSiblingsTrackBank (numTracks, numSends, numScenes, false, false)` | `numTracks`, `hasFlatTrackList`, `hasFullFlatTrackList` |
| Scene bank | `tb.sceneBank ()` (scenes come from the track bank) | `numScenes` |
| Effect tracks | `createEffectTrackBank (numFxTracks, numSends, numScenes)` | `numFxTracks` (−1 → numTracks) |
| Master | `createMasterTrack (0)` | - |
| Extra scene banks | `getSceneBank (numScenes)` → `createMainTrackBank (1, numSends, numScenes)` per size, cached in `sceneBanks` | per caller (lazy, but must still be called during init) |
| Extra slot banks | `getSlotBank (numSlots)` → `createCursorTrack ("CursorTrackID"+n, …, 0, numSlots, true).clipLauncherSlotBank ()` | per caller |
| Note clip | `getNoteClip (cols, rows)` → `CursorClipImpl` → `createLauncherCursorClip (numSteps, numRows)` per `cols-rows` key | per caller |
| Cursor device | `bwCursorTrack.createCursorDevice ("CURSOR_DEVICE", "Cursor device", numSends, CursorDeviceFollowMode.FOLLOW_SELECTION)` (a `PinnableCursorDevice`) | - |
| Device siblings | `cursorDevice.createSiblingsDeviceBank (numDevicesInBank)` + `channel ().createDeviceBank (NUM_DEVICES_LARGE_BANK=100)` (`CursorDeviceImpl`) | `numDevicesInBank` |
| Param pages | `device.createCursorRemoteControlsPage (numParams)` (`SpecificDeviceImpl`); page bank over `selectedPageIndex`/`pageCount` (`ParameterPageBankImpl`) | `numParams`, `numParamPages` |
| Param list | `numListParams/8` × `device.createCursorRemoteControlsPage ("Page "+i, 8, "")` (`ParameterListImpl`) | `numListParams` |
| Layers | `device.createLayerBank (numDeviceLayers)`; per layer `layer.createDeviceBank (numDevices)` (`LayerImpl`) | `numDeviceLayers` |
| Drum pads | `device.createDrumPadBank (numDrumPadLayers)`; main drum device via `bwCursorTrack.createDeviceBank (1)` + `setDeviceMatcher (createBitwigDeviceMatcher (INSTRUMENT_DRUM_MACHINE))`; `additionalDrumDevices` (e.g. 64) share the device | `numDrumPadLayers` (16), `enableDrum64Device()` |
| Specific devices | `createDevice (matcher)` = `createDeviceBank (1)` + matcher: `createInstrumentMatcher ()` (FIRST_INSTRUMENT), Bitwig EQ+ UUID `e4815188-…` (EQ), VST2/VST3 Komplete matchers (NI_KOMPLETE) | `getDeviceIDs ()` |
| Cursor layer | `instrumentDevice.createCursorLayer ()` (`wantsCursorLayer`) | - |
| Track remote controls | `cursorTrack.createCursorRemoteControlsPage (numParams)` (`CursorTrackImpl`) | - |
| Project remote controls | `rootTrackGroup.createCursorRemoteControlsPage (numParams)` (`ProjectImpl`) | - |
| Large track bank | `createTrackBank (NUM_TRACKS_LARGE_BANK=400, 0, 0, true)` (`CursorTrackImpl`, for pin/lookup) | const |
| Markers | `bwArranger.createCueMarkerBank (numMarkers)` + `createCueMarkerBank (100)` (`MarkerBankImpl`) | `numMarkers` (0 = none) |
| Browser | `createPopupBrowser ()`; columns `categoryColumn/creatorColumn/deviceColumn/deviceTypeColumn/fileTypeColumn/locationColumn/smartCollectionColumn/tagColumn` → `createCursorItem ().createSiblingsBank (numFilterColumnEntries)`; `resultsColumn ().createCursorItem ().createSiblingsBank (numResults)` | `numFilterColumnEntries` 16, `numResults` 16 (0 = no browser) |
| Focused param | `createLastClickedParameter ("LastHoveredParam", "Last hovered")` | `wantsFocusedParameter` |
| Clip launcher nav. | `createTrackBank (1, 0, 1)`, `createCursorTrack (1, 1)` (`ClipLauncherNavigatorImpl`) | `wantsClipLauncherNavigator` |
| Groove | `host.createGroove ()` | - |

#### 7.1.2 ModelSetup defaults (`framework/daw/ModelSetup.java`; Rust `model_setup.rs` matches)

`numTracks 8, numFxTracks -1, numScenes 8, numSends 8, numDevicesInBank 8, numDeviceLayers 8, numParamPages 8, numParams 8, numDrumPadLayers 16, numListParams 0, numFilterColumnEntries 16, numResults 16, numMarkers 0, hasFlatTrackList true, hasFullFlatTrackList false, wantsDrumDevice true, wantsCursorLayer false, wantsFocusedParameter false, wantsClipLauncherNavigator false`.

Per controller (shared `controller/**/…ControllerSetup.createModel()`, the controllers that the Rust core has):

| Controller | ModelSetup changes |
|---|---|
| Launchpad | `enableDrum64Device()`; `setHasFullFlatTrackList (areMasterTracksIncluded())` |
| Maschine (mk3 setup, also Mk2) | `setHasFullFlatTrackList(true)`, `setWantsFocusedParameter(true)`, `setNumTracks(hasGroupButtons ? 8 : 16)`, `setNumDevicesInBank(16)`, `setNumScenes(16)` |
| Maschine JAM | `enableDrum64Device()` |
| Launch Control XL | `setNumSends(2)`, `setNumDrumPadLayers(8)`, `enableDevice(EQ)`, `enableDevice(FIRST_INSTRUMENT)`, `setHasFullFlatTrackList (areMasterTracksIncluded())` |

Note: settings that change ModelSetup (e.g. `areMasterTracksIncluded`) only take effect after an extension restart in Bitwig, because banks are fixed after init (?). Java Bitwig: `IHost.restart()` → `host.restart()` exists (`HostImpl`).

#### 7.1.3 Paging: method differences

| Concern | BW (`AbstractItemBankImpl<B extends Bank<?>>`) | RP / Rust (`AbstractPagedBankImpl` / `PagedBank<T>` in `bank.rs`) |
|---|---|---|
| Item count | `bank.itemCount ().get ()` | `item_count`, from `/…/count` |
| Offset | `bank.scrollPosition ().get ()`; `scrollTo` → `scrollPosition ().set (pos)` | local `bank_offset` |
| Can scroll | `canScrollBackwards ()/canScrollForwards ()` (host values) | computed locally |
| Scroll | `scrollBackwards/Forwards ()`, `scrollPageBackwards/Forwards ()` (async in host) | local, immediate, `page_event` |
| Select next/prev page | `scrollPage*` then `host.scheduleTask (() -> getItem (0).select (), 75)` | local + `select` command |
| Skip disabled | `setSkipDisabledItems` | - |
| Observers | `markInterested ()` in ctor; `enableObservers` → `Util.setIsSubscribed (…)` | `enable_updates/delay_updates` (host optimisation) |
| Param pages | `remoteControls.selectedPageIndex ()`, `pageCount ()`, `selectPreviousPage (false)`, `hasNext ()`; page names from the device (`ParameterPageBankImpl`) | pages of 8 over the flat param list; names `"Page {n}"` (`parameter_bank.rs` `ParameterPageBank::item`); RP adds `ParameterMap` (user pages) |
| Device siblings | `createSiblingsDeviceBank (n)` window that follows the cursor | host window of fixed 8 (`Model::DEVICE_BANK_SIZE`) + `device page/±` |
| Track selection follows | `tb.followCursorTrack (cursorTrack)` (host scrolls the bank to the selection) | core logic |

### 7.2 Features in BW that RP/Rust/protocol lack or limit

"RP" column is the Java Reaper impl. The Rust core is a port of it, so in most cases Rust = RP. "Proto" = whether the protocol carries it.

| Area | BW has (API calls) | RP / Rust | Proto ext. needed |
|---|---|---|---|
| Slot states | `ClipLauncherSlot.isPlaying/isPlaybackQueued/isRecording/isRecordingQueued/isStopQueued/hasContent/isSelected/exists/color/name/sceneIndex`; `launch/launchAlt/launchRelease/launchReleaseAlt/record/select/deleteObject/duplicateClip/replaceInsertionPoint ().insertFile/copySlotsOrScenes` | slots = media items. `is_playing/is_recording` derived from transport + region range, `*_queued` false (`slot.rs`). `/clip/all` = `name;selected;R G B;muted;` only. Slot color set only locally. Launch only on press | **yes**: per-slot `hasContent`, `playing`, `recording`, `playQueued`, `recQueued`, `stopQueued`, `color`; launch release/alt; slot `select`, `color`, `paste` |
| Scenes | `Scene.launch/launchAlt/launchRelease*/selectInEditor/deleteObject/nextSceneInsertionPoint/color/name/sceneIndex`; `project.createScene ()`, `createSceneFromPlayingLauncherClips ()` | scenes = regions (`position`, `endPosition`) | partly: drop position semantics; add launch release/alt |
| Track launcher | `track.stop ()`, `stopAlt ()`, `isStopped ()`, `returnToArrangement ()`, `createNewLauncherClip (slot, len)`, `playingNotes ()` | RP `TrackImpl.stop/isPlaying/returnToArrangement` = not supported | **yes** (`stop`, `isStopped`, `returnToArrangement`) |
| Track props | `trackType ()`, `canHoldNoteData/AudioData ()`, `crossFadeMode ()`, `monitorMode ()` (enum), `isMutedBySolo ()`, `isActivated ()`, `createParentTrack(0,0)` (top group) | `type` only GROUP/HYBRID; `active` not sent (P2); no crossfade mode | **yes**: crossFadeMode, mutedBySolo, real type, active |
| Hierarchy | `createSiblingsTrackBank`, `selectParent`, `enter` (group) | flat visible list + `depth`; `AbstractTrackBankImpl.selectParent` not supported | maybe (`depth` can be emitted (?)) |
| Cursor track pin | `cursorTrack.isPinned ()`, pin toggle | `track pin` stored host-side only | small |
| Cursor device | `PinnableCursorDevice` `isPinned`, `selectParent ()`, `selectNext/Previous ()`, `channel ().selectInEditor ()`, `isNested`, `hasSlots/slotNames/selectSlotChain`, `isExpanded`, `isWindowOpen`, `isRemoteControlsSectionVisible`, `isPlugin`, `presetName`, move via `afterDeviceInsertionPoint ().moveDevices` | RP `CursorDeviceImpl.getSlotChains/selectChannel/selectParent/selectSlotChain/setPinned/togglePinned` not supported. Rust `has_layers/has_slots/is_nested` false, `has_drum_pads` always true (`device.rs`) | **yes** |
| Device layers | `createLayerBank`, `LayerImpl` (channel + device bank), `createCursorLayer` | "Reaper has no device layers, therefore layer banks are not modeled" (`device.rs:10`) | **yes** (new `layer` processor/address) |
| Drum pads | `createDrumPadBank (n)`, `DrumPadImpl` (real channels: mute/solo/vol/pan/sends/color, `selectInEditor`), `hasMutedPads/hasSoloedPads`, `clearMute/clearSolo` | virtual pads; commands go to `track <padPosition>/…` (§6 f) | **yes** (`drumpad <i>/…`) |
| Remote-control pages | named pages, `pageCount`, `selectedPageIndex`, `getParameter (i)` per page; track & project remote controls | flat FX params; TCP FX params as track/project bank (Reaper-specific, §5) | **yes**: page names/count/selected, `page select` command; param `modulatedValue`, `touch`, `setIndication` |
| Parameter extras | `modulatedValue ()`, `touch (bool)`, `setIndication (bool)`, `displayedValue ()`, `discreteValueCount ()` | RP `getModulatedValue`/`setIndication` not supported | yes (`modulated`, `indicate`) |
| Bank indication | `setIndication` on track bank, groove, scenes | not supported (`AbstractTrackBankImpl`, `DrumPadBankImpl`, `LayerBankImpl`) | yes (optional) |
| Browser | `PopupBrowser`: content types (`contentTypeNames/selectedContentTypeIndex`), 8 filter columns with item banks (`hitCount`, `isSelected`), results bank, `shouldAudition` (preview), `commit/cancel`, insert/replace via insertion points | RP: own `DeviceManager` with filter columns from `dbm_add_device`. Rust `browser.rs`: content types/filter selection/preview are empty stubs; presets via `/browser/presetsfile`, `/browser/selected/index` | **yes** (full browser addresses + commands) |
| Markers | `createCueMarkerBank`: `exists/name/getColor/position/launch/deleteObject` | Reaper markers; position parsed as no-op (H2) | small (position in beats) |
| Groove | `createGroove ()`: `getEnabled`, `getShuffleAmount`, `getShuffleRate`, `getAccentAmount`, `getAccentPhase`, `getAccentRate` | only `active`, `amount` (grid swing) | yes (rate, accent ×3) |
| Note input | `createNoteInput (name, filters)`, `setKeyTranslationTable`, `setVelocityTranslationTable`, `setShouldConsumeEvents`, `setUseExpressiveMidi`, `sendRawMidiEvent`, `noteLatch ()`, `arpeggiator ()` (note repeat) | callbacks exist: `set_note_input_filters/_key_translation_table/_velocity_translation_table` (`drivenbymoss.h`). Note repeat = Reaper arp JSFX | note repeat → map `noterepeat` to `NoteInput.arpeggiator ()` (S) |
| Cursor clip (step data) | `createLauncherCursorClip (cols, rows)` window: `addNoteStepObserver`, `getStep (ch,x,y)` → `NoteStep` (velocity, release, duration, chance, timbre, pressure, pan, gain, transpose, repeat, occurrence, recurrence, muted), `scrollToStep/Key`, `setStepSize`, `playingStep`, `getAccent`, `getShuffle`, `isPinned`, `getLoopStart/Length`, `getPlayStart/Stop`, `quantize`, `transpose`, `duplicate(Content)` | RP `CursorClipImpl`: 41 methods not supported (step chance/gain/pan/pressure/timbre/transpose/repeat/occurrence/recurrence/release/velocitySpread, accent, shuffle, pin). Notes as list `sel:mute:start:end:chan:pitch:vel` in quarters, core builds the grid | **yes**: either step-grid addresses (Bitwig shape) or extended note list fields + clip loop/play start/stop. Grid window size must be known at init |
| Transport | `isPunchInEnabled/isPunchOutEnabled`, `isMetronomeEnabled`, `metronomeVolume`, `isMetronomeTickPlaybackEnabled`, `isMetronomeAudibleDuringPreRoll`, `preRoll` (enum), `tapTempo ()`, `automationWriteMode ()` (`"latch"/"touch"/"write"`), `isArrangerAutomationWriteEnabled`, `isClipLauncherAutomationWriteEnabled`, `resetAutomationOverrides ()`, `isArrangerOverdubEnabled/isClipLauncherOverdubEnabled`, `defaultLaunchQuantization`, `clipLauncherPostRecordingAction` + time offset, `isFillModeActive`, `crossfade ()`, `timeSignature ()`, `playStartPosition`, `jumpToPlayStartPosition ()`, `restart ()`, `returnToArrangement ()` | metronome/tap/preroll/punch via action IDs (§5). RP not supported: fill mode, metronome ticks, launcher automation write, post-recording action/offset, default launch quantization, reset overrides, returnToArrangement, tempo indication. Automation modes in Reaper numbering 0..5 | **yes**: real commands + addresses for all of these |
| Application | `panelLayout ()` (`nextPanelLayout/previousPanelLayout`), `toggleMixer/NoteEditor/AutomationEditor/Devices/Inspector/BrowserVisibility/FullScreen`, `arrowKey*`, `enter/escape`, `duplicate/remove`, `undo/redo/canUndo/canRedo`, `activateEngine/deactivateEngine/hasActiveEngine`, `recordQuantizationGrid`, `recordQuantizeNoteLength`, `createAudio/Effect/InstrumentTrack (-1)`, `getAction (id)` / `getActionCategories` | panels via Reaper actions; RP `duplicate`, `setInternalEngineActive`, `toggleDevices` not supported | yes (panel layout + view toggles as commands); Bitwig action IDs differ from Reaper's |
| Arranger/Mixer views | `Arranger.areCueMarkersVisible/areEffectTracksVisible/hasDoubleRowTrackHeight/isClipLauncherVisible/isIoSectionVisible/isPlaybackFollowEnabled/isTimelineVisible`; `Mixer.is*SectionVisible` | most not supported in RP (`ArrangerImpl`, `MixerImpl`) | yes (optional) |
| Project | `cueMix ()`, `cueVolume ()`, `hasMutedTracks/hasSoloedTracks`, `unmuteAll/unsoloAll`, `isModified`, `projectName` | RP cue mix/volume not supported | yes (cue, hasMuted/Soloed) |
| Focused param | `createLastClickedParameter` | RP `FocusedParameterImpl` (`/device/touchedParam`) | small |
| Specific devices | EQ+ (`createSpecificBitwigDevice` params `TYPEn/FREQn/GAINn/Qn`), Komplete (VST2/3 IDs, `createParameter`) | ReaEQ band type numbers (§5), `KompleteDevice` | EQ: band type mapping differs (M) |
| Effect tracks | `createEffectTrackBank` | RP `effectTrackBank = null` (`ModelImpl:157`); Rust has `effect_track_bank()` (?) unused for Reaper | yes (`/fxtrack/<n>/…` or a flag) |
| User params | none in BW framework (`createUserControls` not used) | RP `UserParameterImpl` only for crossfader | - (crossfader → `transport.crossfade ()`) |
| MIDI | `MidiIn.setMidiCallback`, `setSysexCallback`, `MidiOut.sendMidi/sendSysex`; hardware surface matchers (`createAbsoluteCCValueMatcher` …, `HwSurfaceFactoryImpl` → `createHardwareSurface ()`) | core does its own MIDI parsing; `dbm_on_midi_event` / `send_midi_data` | no: use the raw callbacks, skip `HardwareSurface` (loses Bitwig's on-screen controller simulation/mapping (?)) |
| Slot muted | not available ("Info not available via the API", `SlotImpl.isMuted` false) | sent by Reaper | - |

### 7.3 RP/Rust parts Bitwig does not need

| Reaper part | Bitwig equivalent / action |
|---|---|
| `action` with Reaper command IDs (`actions.rs`), `action ""` named commands, `action select` | map the IDs the core needs to API calls, or replace them with real commands. Bitwig `Application.getAction (id)` uses different IDs |
| `inifile reaper/prerollmeas` | `transport.preRoll ()` enum |
| `metro_vol` via action 999 | `transport.metronomeVolume ()` |
| `tempo +/++/-/--` fixed actions | `transport.tempo ().incRaw (…)` (?) |
| `quantize` via MIDI editor actions | `clip.quantize (amount)` (amount really used) |
| `track <t>/active` via lock/offline/mute actions | `channel.isActivated ()` |
| `track <t>/inQuantResolution` | global `application.recordQuantizationGrid ()` (not per track) |
| `track <t>/param` / `master param` (TCP FX params) | track remote controls (`cursorTrack.createCursorRemoteControlsPage`), project remote controls (`rootTrackGroup.createCursorRemoteControlsPage`) |
| `eq` ReaEQ band numbers, `eq add` | EQ+ device matcher; insert via browser/insertion point (?) |
| `device add/<pos>` FX creation names, `addTrack` device list, `dbm_add_device` | Bitwig browser/insertion points; `insertBitwigDevice (UUID)` / `insertVST3Device (id)` (?) |
| `device window/expand` (FX windows) | `isWindowOpen ().toggle ()`, `isExpanded ().toggle ()`: same semantics, different impl |
| `noterepeat` arp JSFX | `NoteInput.arpeggiator ()` |
| regions = scenes, items = slots, `insertFile` via `InsertMedia` | real scenes/slots; `replaceInsertionPoint ().insertFile` |
| visible-track index (hidden tracks skipped) | bank index |
| `device sibling` window of 8 | `createSiblingsDeviceBank (numDevicesInBank)` |
| `delay_updates`/`enable_updates` (300 ms hold-off for automation feedback) | `Util.setIsSubscribed` (`subscribe/unsubscribe`) may map to `enable_updates`; delay is not needed (?) |
| `ParameterMap*` (user parameter pages, `ParameterMappingDialog`) | remote-control pages exist natively |
| `cursor` (`CSurf_OnArrow`) | `application.arrowKey*` |
| `project engine` (`Audio_Init/Quit`) | `activateEngine/deactivateEngine` |
| own settings UI / `DrivenByMoss4Reaper.config` / `CONTROLLER_INSTANCE<n>` / MIDI port selection | Bitwig controller preferences + port assignment per extension instance |
| `/time/hzoom`, `ZOOM_*` actions | `arranger.zoomIn/Out` (?) / `getContentPerPixel` (TransportImpl marks it) |

### 7.4 Effort per framework interface group (Bitwig backend speaking the existing protocol)

S = mapping only; M = mapping plus conversions/extra objects; L = new protocol plus core changes. "Ext" = protocol extension needed.

| Group (Java interface → Rust) | Effort | Ext | Notes |
|---|---|---|---|
| ITransport (`transport.rs`) basics: play/stop/record/repeat/tempo/time sig | S | no | Seconds↔beats conversion (see §4 of this plan); `/time/str`, `/beat` via `getPosition ().getFormatted (…)` |
| ITransport extras: metronome, tap, preroll, punch, automation modes, overdub, launcher settings, fill | M | **yes** | Today action IDs; automation numbering differs (Reaper override 0..5 vs Bitwig `"latch"/"touch"/"write"` + write-enable flags) |
| ITrackBank / ITrack / IChannel (`track_bank.rs`, `track.rs`, `channel.rs`) | M | small | Option A: large flat bank (e.g. 400 like `NUM_TRACKS_LARGE_BANK`) × sends × scenes is heavy; Option B: host paging = **ext** (`track scroll`, `/track/offset`, `/track/canScroll*`). Group navigation `enter/selectParent` needs ext |
| ISendBank (`/track/<n>/send/…`) | S | no | Bitwig sends exist per bank size; flat "all sends" = `numSends` fixed at init |
| IMasterTrack | S | no | crossfader → `transport.crossfade ()` |
| IEffectTrackBank | M | **yes** | Not in RP at all |
| ISlotBank / ISlot (`slot.rs`) | L | **yes** | Explicit states; slot index = scene index; launch release/alt; Rust must stop deriving `is_playing` from transport |
| ISceneBank / IScene (`scene.rs`) | M | **yes** | No positions; `launch` alt/release; `createScene*` present |
| ICursorDevice / IDeviceBank (`device.rs`) | M | small | Sibling window size = core `num_devices_in_bank` (host constant 8 today); pin/selectParent/nested = ext |
| IParameterBank / IParameterPageBank (`parameter_bank.rs`) | L | **yes** | Remote-control pages: page names, count, selection are host-driven; flat `param/<i>` only works for the current page (or via many `CursorRemoteControlsPage`s) |
| Track / project parameter banks (remote controls) | M | small | Reuse `/track/fx/param/*`, `/master/fx/param/*`; semantics change from TCP params to remote controls |
| ILayerBank / ILayer / ICursorLayer | L | **yes** | Not modeled in Rust |
| IDrumDevice / IDrumPadBank / IDrumPad | L | **yes** | Real pads; 16 + 64 pad banks (`enableDrum64Device`) |
| ISpecificDevice: FIRST_INSTRUMENT (`/primary`) | S | no | `createInstrumentMatcher ()` |
| ISpecificDevice: EQ (`/eq`) | M | no | EQ+ params; band-type mapping (`EqTypeParameter`) differs from ReaEQ numbers |
| IBrowser / IBrowserColumn | L | **yes** | Rust browser is a stub |
| INoteClip / cursor clip (`clip/cursor_clip.rs`) | L | **yes** | Bitwig step grid vs Reaper note list; extended step fields; grid sizes at init |
| IMarkerBank / IMarker | S | small | cue markers; position in beats (parser no-ops position today) |
| IGroove | S/M | yes (for rate/accent) | `active`/`amount` map directly |
| IApplication / IArranger / IMixer | M | **yes** | Panel layout, view toggles, arrow keys; replace action IDs |
| IProject | S | small | name/modified/undo; cue mix/volume = ext |
| INoteInput / INoteRepeat (`midi.rs`) | S | no | Callbacks already in `DbmCallbacks`; note repeat → arpeggiator (map `noterepeat` cmds) |
| IFocusedParameter | S | no | `createLastClickedParameter` → `/device/touchedParam`-like (?) |
| Settings (global/document) | M | **yes** | Bitwig settings must be declared in init from core descriptors; or keep core config files (S, no Bitwig UI) |
| Controller lifecycle / MIDI ports | M | **yes (C ABI)** | New create/start API per controller UUID, port indices from Bitwig |

### 7.5 Bitwig API constraints for a native-core design

| Constraint | Where in BW Java | Consequence for a Rust core |
|---|---|---|
| Discovery via `META-INF/services/com.bitwig.extension.ExtensionDefinition` (54 lines, one class per controller, e.g. `de.mossgrabers.bitwig.controller.ableton.push.Push1ControllerExtensionDefinition`) | `src/main/resources/META-INF/services/com.bitwig.extension.ExtensionDefinition` | Bitwig loads Java classes, not native lists. You need one generated or hand-written Java class per Rust controller, or a single generic class (?) per UUID. A C export of `ControllerDefinitionInfo` + discovery pairs would allow generating them |
| Definition = `ControllerExtensionDefinition` subclass | `bitwig/framework/extension/AbstractControllerExtensionDefinition.java`: `getId ()` (UUID), `getName ()` = `getHardwareModel ()`, `getAuthor`, `getVersion`, `getHardwareVendor`, `getHardwareModel`, `getRequiredAPIVersion ()` = **21**, `getNumMidiInPorts/OutPorts ()`, `listAutoDetectionMidiPortNames (list, platformType)` (from `getMidiDiscoveryPairs (os)`), `listHardwareDevices (matchers)` (USB from `claimUSBDevice ()`), `createInstance (host)` → `new GenericControllerExtension (getControllerSetup (host), this, host)`; optional `isUsingBetaAPI ()` (Push 2/3 return true) | Rust `ControllerDefinitionInfo` (`controller/mod.rs`: uuid, vendor, hardware_model, num_midi_in/out_ports) + `midi_discovery_pairs (os)` already match 1:1 (e.g. `launchcontrol/definition.rs` UUID `5AFA5045-…`, same as Java) |
| Concrete definition | e.g. `bitwig/controller/ableton/push/Push1ControllerExtensionDefinition` → `super (new Push1ControllerDefinition ())`, `getControllerSetup` → `new PushControllerSetup (new HostImpl (host), new BitwigSetupFactory (host), new SettingsUIImpl (host, host.getPreferences ()), new SettingsUIImpl (host, host.getDocumentState ()), …)`. Shared `controller/ableton/push/Push1ControllerDefinition extends DefaultControllerDefinition` (`super (UUID, "Push 1", "Ableton", 1, 1)`, `getMidiDiscoveryPairs` with `createWindowsDeviceDiscoveryPairs`, `createLinuxDeviceDiscoveryPairs`, `addDeviceDiscoveryPair`) | - |
| Fixed port count per definition | `getNumMidiInPorts ()` from definition; ports via `host.getMidiInPort (i)`/`getMidiOutPort (i)` (`MidiInputImpl`, `MidiOutputImpl`) | Core must not open ports itself; `open_midi_input(device_id)` → port index. `MidiPorts` is one in/one out today |
| One extension instance = one controller | `GenericControllerExtension` holds one `IControllerSetup` | One `DbmApp` per extension instance (or one shared app with per-instance controller); no `CONTROLLER_INSTANCE<n>` config |
| Everything created in `init ()` | `GenericControllerExtension.init ()` → `setup.init ()` (model, banks, surface, settings); `ModelImpl` ctor creates all banks; lazy `getSceneBank/getSlotBank/getNoteClip` are only safe while init runs | Core must create the controller (`ControllerSetup::init`, model, views) **synchronously inside Java `init()`** and report all bank needs then (bank manifest). Settings descriptors also come out then |
| `markInterested ()` / subscriptions | every `*Impl` ctor; `enableObservers` → `Util.setIsSubscribed` | Backend marks everything it may send; `enable_updates` can map to subscribe/unsubscribe |
| Delayed startup | `init ()` → `host.scheduleTask (() -> setup.startup (), 1000)` | Matches Rust `STARTUP_DELAY_MS` (~1 s) |
| No periodic tick | `ModelImpl.flushWorkaround`: `host.scheduleTask (…, 100)` + `requestFlush ()` when not playing | Drive `dbm_tick` from `flush ()` plus a rescheduling `scheduleTask` loop (min ~ tens of ms (?)); use `DBM_FLAG_HOST_TICK` |
| `flush ()` | `GenericControllerExtension.flush ()` → `setup.flush ()` | Natural point for `dbm_update_model` (collected observer changes) + `dbm_tick` + draining outgoing MIDI |
| `exit ()` | `setup.exit ()` | `dbm_destroy` |
| Settings | `SettingsUIImpl` on `host.getPreferences ()` (global) / `host.getDocumentState ()` (project): `getEnumSetting`, `getBooleanSetting`, `getStringSetting`, `getNumberSetting`, `getSignalSetting`, `getColorSetting` (`bitwig/framework/configuration/*`) | Map core settings (`config/setting.rs`) to these at init; document settings ↔ `dbm_get/set_formatted_document_settings` (could live in a hidden document string setting (?)) |
| Single thread | all callbacks on the controller thread | Matches the core's mutex model; never call `dbm_*` from inside a core callback |
| Raw MIDI | `port.setMidiCallback`, `setSysexCallback`, `sendMidi`, `sendSysex`, `createNoteInput (name, filters)` | maps to `dbm_on_midi_event`, `send_midi_data`, `set_note_input_*` |
| USB/HID | only via `listHardwareDevices` + `host.hardwareDevice (index)` (`HostImpl`, `bitwig/framework/usb/*`) | Native libusb/hidapi from Rust may conflict with Bitwig's claim (?); Maschine on Linux is handled through `dbm-nihost` today |
| OSC | `host.getOscModule ()` (`HostImpl.createOSCServer`) | n/a for current Rust controllers |

### 7.6 Build of the Java DrivenByMoss: API and Java level

| Item | Value | Where |
|---|---|---|
| Bitwig API | `com.bitwig:extension-api` **21** | `pom.xml:42-43` |
| Required API in definitions | `getRequiredAPIVersion ()` returns **21** | `AbstractControllerExtensionDefinition` |
| Java level | maven-compiler-plugin 3.16.0, `<source>21</source>`, `<target>21</target>` (no `<release>`) | `pom.xml:106-111` |
| Packaging | maven-shade (excludes `com.bitwig:extension-api`), output `${bitwig.extension.directory}/DrivenByMoss.bwextension` | `pom.xml:143-207` |
| Project version | 26.7.0 | `pom.xml:9` |
| Beta API | `isUsingBetaAPI ()` true in 8 classes (e.g. Push 2/3) | grep |
