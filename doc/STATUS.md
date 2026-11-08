# DrivenByMoss4Reaper → Rust: Port Status

As of 2026-11-02 · branch `main`

## Summary

Seven controller families are ported: Novation Launchpad (5 models), Novation Launch Control XL,
NI Maschine Mk2 and Jam, Mackie MCU and HUI (with extenders), Generic Flexi, and the Akai APC mini,
APC40, MPC/Force (ACVS) and Fire. The crate has about 90,000 lines of Rust in 315 files. 372 tests
pass, 2 are ignored, and `cargo clippy --all-targets` reports 0 warnings. The release build
produces `libdrivenbymoss.so` (about 6.5 MB), which exports the 25 `dbm_*` functions of the
handle-based C ABI.

**First successful test in Reaper (2026-10-29):** the Reaper extension in `backends/reaper`,
switched from the JVM to the C ABI through the C++ wrapper (`bindings/cpp`), was built on Linux,
loaded in Reaper and tested with a Novation Launch Control XL, including template switching. The
Maschine Mk2 and JAM work in Reaper on Linux through `dbm-nihost` (see below). The Launchpad has
not been tested with hardware yet.

The CMake paths for Windows and macOS are written but untested. The Visual Studio, Xcode and
Eclipse project files in `backends/reaper` still reference the removed JVM files; CMake is the
supported build.

Building the extension needs CMake, a C++ compiler and Rust:

```
cd backends/reaper
cmake -S CMake -B build -D CMAKE_BUILD_TYPE=Release
cmake --build build
```

CMake runs cargo itself and links the core statically into `reaper_drivenbymoss.so` (`.dll` on
Windows, `.dylib` on macOS).

The protocol between the core and a host (commands, model updates, call sequence) is written down
in [BACKEND_PROTOCOL.md](BACKEND_PROTOCOL.md), with a list of mismatches between the Rust core
and the Reaper backend. [BITWIG_BACKEND_PLAN.md](BITWIG_BACKEND_PLAN.md) drafts a Bitwig Studio 6.1
backend (a Java extension over FFM) with a dev mode that reloads the rebuilt native library.

`ci/linux.sh` builds and tests everything that builds on Linux: the crate (with the replays of the
recorded Bitwig sessions), the C++ wrapper, `dbm-nihost`, the Reaper and the Bitwig extension.
`.github/workflows/linux.yml` runs it on every push, after `cargo clippy --all-targets -D warnings`
and `shellcheck` of the own scripts. Next: a Windows build (the Reaper extension; `dbm-nihost` with
hidapi would give Windows users the Shift button of the Mk2, which NI's driver does not send in
MIDI mode).

| Controller | Models registered | Rust path | Integration tests |
| --- | --- | --- | --- |
| Novation Launchpad | MkII, Pro, Pro Mk3, X, Mini MkIII | `src/controller/launchpad/` | `launchpad_smoke.rs` (2), `launchpad_setup.rs` (10), `launchpad_views.rs` (11) |
| Novation Launch Control XL | XL | `src/controller/launchcontrol/` | `launchcontrol_xl.rs` (12) |
| NI Maschine | Mk2, Jam (Mk3, Mikro Mk3, Plus, Studio prepared, not registered) | `src/controller/maschine/` | `maschine_mk2_setup.rs` (15), `maschine_jam_setup.rs` (12) |
| Mackie MCU | MCU with 0-3 extenders (`MCU1..4ControllerInstance`) | `src/controller/mackie/mcu/` | `mackie_mcu.rs` (9) |
| Mackie HUI | HUI with 0-2 extenders (`HUI1..3ControllerInstance`) | `src/controller/mackie/hui/` | `mackie_hui.rs` (5) |
| Generic Flexi | Flexi | `src/controller/generic/` | `generic_flexi_setup.rs` (14), `generic_flexi_touch.rs` (2) |
| Akai APC mini | Mk1, Mk2 | `src/controller/akai/apcmini/` | `akai_apcmini.rs` (9) |
| Akai APC40 | Mk1, Mk2 | `src/controller/akai/apc/` | `akai_apc40.rs` (6) |
| Akai MPC/Force (ACVS) | MPC Live I/II, One, X, Force | `src/controller/akai/acvs/` | `akai_acvs.rs` (4) |
| Akai Fire | Fire (with the OLED display) | `src/controller/akai/fire/` | `akai_fire.rs` (4) |

There are also 220 unit tests in the sources.

**Mackie MCU and HUI (2026-11-02):** the first controllers with several surfaces per instance
(the main device and its extenders, each with its own MIDI port pair). The context keeps the
current surface and swaps the others in (`framework/multi_surface.rs`, design in
[MULTI_SURFACE_PLAN.md](MULTI_SURFACE_PLAN.md)); the app resolves and opens one port pair per
surface, the Bitwig extension opens all ports of a definition. Not tested with hardware yet. Not
ported: the "Pin FX tracks to last device" option (needs an effect track bank, which Reaper has
not; the Java version does not activate it in Reaper either).

**Generic Flexi** (2026-11-02): all 1122 commands of the 21 Java handlers, the 300 command slots
with CC (7 and 14 bit), note, program change, pitchbend and MMC triggers, the 10 function layers,
the six knob modes, sending the values back to the device, the keyboard note input with its
filters and the key translation of mapped notes, the program banks (`.programs` file next to the
mapping) and the 12 modes. The mapping files use the Java format (`SLOT<n>_TYPE`, ...), so Java
mappings load unchanged; all Java settings are activated with the same keys. The settings of the
selected slot ("Selected Slot - ...") and the MIDI learn values are settings, so a host with a
settings UI (Bitwig) can edit the mapping. Not ported: the file dialogs and the signal buttons
("Set", "Save", "Load"; `editor::learn_set`, `mapping::save_file` and `mapping::load_file` are
their functions) and the FX track commands (no effect track bank, as in the Java Reaper version).
See the module documentation of `src/controller/generic/` for the deviations.

**Akai (2026-11-02):** APC mini Mk1/Mk2, APC40 Mk1/Mk2, MPC/Force over ACVS and the Fire.
The Fire's 128x64 OLED is drawn with the new dependency-free graphics module
(`src/framework/graphics/`: RGB bitmap, graphics context, embedded 5x8 bitmap font, the
components of the small displays and a display with notifications) and sent as 1 bit stripes.
Not tested with hardware yet. Not ported:
- APC40: the track crossfade (Reaper has no crossfade parameter per track) and the simulator
  colors.
- ACVS: the second MIDI input of the MPC ("Pads (Remote MIDI Port)", one input per port pair).
- Fire: the XoX drum view (needs `AbstractDrumXoXView`), pinning the cursor track and the knob
  sensitivity in the browser.
See the module documentation in `src/controller/akai/` for the details and the Java deviations.

Commits:

- `e00a36ec`: core framework, Reaper DAW layer and Launchpad
- `a3bb3b89`: Launch Control XL and Maschine Mk2/Jam, with the parameter providers, the
  parameter, device, track and layer modes, the text display and more generic commands

**Maschine on Linux:** NI's driver doesn't exist on Linux. `support/linux/ni_host` (`dbm-nihost`)
emulates it for the Mk2 and the JAM: hidraw on one side, ALSA ports with NI's names on the other.
The Linux guard of the Maschine factories is removed.
- Tested on both devices with the Rust port in Reaper, and with the Java DrivenByMoss in Bitwig.
  The Java version needed its Linux check removed (local branch `linux-maschine` in the
  DrivenByMoss repository).
- Reaper sees the ports through PipeWire's MIDI bridge (`Midi-Bridge:Maschine MK2: In (capture)`);
  the discovery pairs include these names.
- Bitwig only opens raw MIDI devices; `dbm-nihost --virmidi` connects the ports to the devices of
  `snd-virmidi`.
- It installs with CMake, including a systemd user service. See
  `support/linux/ni_host/README.md`.

**Scope and size:**

- The Java project has about 240,000 lines in 1,544 files.
- The crate is self-contained in `rust/`: a single crate `drivenbymoss` (cdylib + rlib) with no
  external dependencies. The Java tree is unchanged.
- It can be split into its own repository with `git subtree split --prefix=rust`.

**Not ported:**

- The Swing/JavaFX UI and the simulator windows.
- USB, HID, OSC and the gamepad. Graphics displays: only the Akai Fire OLED (MIDI system
  exclusive); the USB displays (Push 2/3) are not ported.
- All controllers not listed above. See [PORTING_GUIDE.md](PORTING_GUIDE.md) for the
  remaining ones.

## Architecture

The Rust library replaces the embedded JVM.

- **Calls in:** the `dbm_*` functions in `src/ffi.rs` mirror the static methods of the Java
  `Controller` class, but take an instance handle:
  - `dbm_create` / `dbm_destroy`
  - `dbm_update_model`, `dbm_on_midi_event`, `dbm_tick`
  - `dbm_get/set_formatted_document_settings`, and so on
- **Header:** `include/drivenbymoss.h` is generated by cbindgen in `build.rs`; the
  configuration is in `cbindgen.toml`. See [BINDINGS_PLAN.md](BINDINGS_PLAN.md) for the plan
  for C++, Python and Java bindings.
- **Calls out:** the `DbmCallbacks` table of function pointers replaces the JNI `native` methods
  of `MainApp`: `process_*_arg`, MIDI open/close/send, and the note-input filters and
  translation tables. Each pointer may be NULL, which means it is ignored.
- **Threading:**
  - Each instance serializes its calls with its own mutex, so several instances can run in one
    process.
  - A background thread per instance calls `App::tick()` every 30 ms, like the Java flush
    timer. With `DBM_FLAG_HOST_TICK` there is no thread, and the host calls `dbm_tick()`
    itself.
  - Inside the lock everything is single-threaded (`Rc`/`RefCell`, no `Arc`).
  - Panics are caught at the FFI boundary.
- **Context passing:**
  - In Java, views, modes and commands keep references to the surface and the model. In Rust
    they get `&mut Ctx` (`src/framework/ctx.rs`).
  - `Ctx` holds the host, sender, model, configuration, surface, view and mode managers, color
    manager, scheduler, an optional text display, and a typed `ext` slot for controller state.
- **Events instead of observers:** the model, the configuration and the parser record changes
  (`take_events()`, `take_changed_settings()`, `ParserEvent`). The controller setup handles them
  in `handle_changes`, before each flush.
- **Inheritance becomes composition:** Java template methods became hook traits on composed base
  structs, e.g. `SessionViewBase` + `SessionHooks`.
- **Flush cycle:**
  1. The active view updates its controls and draws the grid.
  2. Only changed light states are sent to the hardware.
  3. The controller-specific flush runs (Launchpad blink sysex, Maschine emulated blinking,
     text displays).
- **Settings:** stored in the same `.config` property files as Java, so existing user
  configurations keep working. Project settings use the same base64 document format.

## Directory layout

All paths are under `rust/`.

| Path | Java origin | Status |
| --- | --- | --- |
| `Cargo.toml`, `include/drivenbymoss.h` | `build.gradle`, JNI interface | Done |
| `src/ffi.rs` | `Controller`, `MainApp` natives | Done |
| `src/app.rs` | `MainApp`, `ControllerInstanceManager`, `AbstractControllerInstance` | Done |
| `src/backend.rs`, `src/host.rs` | `BackendExchange`, `Processor`, `IHost` | Done |
| `src/midi.rs` | `reaper/framework/midi/*`, `DeviceInquiry` | Done |
| `src/config/` | `AbstractConfiguration`, Reaper settings, controller configurations | Done (Launchpad, Launch Control, Maschine, Maschine Jam) |
| `src/daw/`, `src/daw/clip/` | `framework/daw/**` + `reaper/framework/daw/**` | Done; clip steps stored sparsely |
| `src/scale/` | `framework/scale/**`, `KeyManager` | Done |
| `src/parser.rs` | `MessageParser` | Done |
| `src/framework/` core | ids, value changer, color, grid (incl. emulated blinking), surface, feature (View/Mode traits), command traits, ctx, virtual fader, chord identifier, text display (`display.rs`) | Done |
| `src/framework/parameterprovider/` | `framework/parameterprovider/**` | Done |
| `src/framework/mode/` | track, parameter, device, layer, master, sequencer, dummy modes, note editor | Done |
| `src/framework/commands/` | Trigger, continuous, mode, transport and misc commands, `MVHelper` | Done for everything the 3 controllers use |
| `src/framework/view/` | Play, chord, piano, session, scene play, shift, browser, Drum64, tempo/shuffle, transpose views | Done |
| `src/framework/view/sequencer/` | Sequencer, note/poly/raindrops, drum, Drum4/8, clip length views | Done |
| `src/controller/mod.rs` | `IControllerDefinition`, `IControllerSetup`, controller registry | Done |
| `src/controller/launchpad/` | `controller/novation/launchpad/**` | Done |
| `src/controller/launchcontrol/` | `controller/novation/launchcontrol/**` | Done |
| `src/controller/maschine/{core,mk,jam}/` | `controller/ni/maschine/**` | Mk2 and Jam done |
| `src/controller/mackie/{mcu,hui}/` | `controller/mackie/**` | Done (MCU 1-4 devices, HUI 1-3 devices) |
| `src/framework/multi_surface.rs` | `AbstractControllerSetup.surfaces`, connected mode managers | Done |
| `bindings/cpp/` | — | Header-only C++ wrapper (`dbm::App`, `dbm::Backend`) and its test |
| `tests/` | — | Integration tests per controller |
| `backends/reaper/` | [DrivenByMoss4Reaper-Backend](https://github.com/git-moss/DrivenByMoss4Reaper-Backend) (C++ Reaper extension) | Squashed git subtree of upstream `master` (0e292a08), switched from JNI to the C ABI. Update with `git subtree pull --prefix=rust/backends/reaper https://github.com/git-moss/DrivenByMoss4Reaper-Backend master --squash` |
| `doc/` | — | This file and the porting guide |

## Next steps and open issues

**Planned: a typed protocol between core and hosts** (decided 2026-11-08). Today every address
(`/track/3/volume`, host → core) and every command (`track 3/mute 1`, core → host) is a string
that is formatted on one side and parsed on the other.
- **The problems:**
  - An address or command one side doesn't support only shows at run time, as "not handled".
  - The places that use an address are hard to find.
  - Formatting and parsing strings costs CPU in every host. The Zynthian drivers spend most of
    their tick in Python formatting and comparing ~950 values; see ZYNTHIAN_CPU.md.
- **The plan:**
  - Enums for all paths and commands, defined in one table in the core. The C header and the
    bindings (C++, Python, the Bitwig extension's Java) are generated from that table.
  - A binary message type: an enum ID, indices (track, send, slot, …) and a typed value.
  - The text form stays for logs, the protocol recordings (`.rec`) and the replays, with a
    conversion between both forms.
- **Scope:** `src/parser.rs`, the commands in `src/backend.rs` and the `daw` types, the C ABI
  (`src/ffi.rs`, `include/drivenbymoss.h`), the hosts (Bitwig `backends/bitwig`, the Python
  binding and reference host, the Zynthian adapter), the recorder and the replay test. Reaper is
  out of scope: not a target of the project, its backend stays on the text protocol.
- **Migration:**
  - The hosts move one by one, keeping the text protocol until the last one has moved.
  - The conformance replays must stay valid, recorded or converted.
- To plan in detail before it is started (BACKEND_PROTOCOL.md describes the current text
  protocol).
- The plan: [TYPED_PROTOCOL_PLAN.md](TYPED_PROTOCOL_PLAN.md).

1. Test the Launchpad with hardware in Reaper, and test the Launch Control XL and the Maschine
   (Mk2, JAM) in more depth. Compare the behaviour with the Java version.
2. Verify the Windows and macOS CMake builds, and update or remove the Visual Studio, Xcode and
   Eclipse project files.
3. Register and test Maschine Mk3, Mikro Mk3 and Plus, and port the Studio monitor-encoder
   section.
4. Port the remaining controllers, following [PORTING_GUIDE.md](PORTING_GUIDE.md).
5. Extract `rust/` into its own repository.

Open issues:

- [ ] Launch Control XL: the lights do not turn off when Reaper quits. Check whether the Java
  version does it, and whether the shutdown path (`dbm_destroy` → controller `exit` → queued
  MIDI → `SendMIDIEventsToOutputs`) still reaches the device at that point.
- [ ] There is no UI. Configuration comes from the `.config` files, and `dbm_display_window` only
  prints the log. Decide whether a UI is needed.
- [ ] Decide how to support USB/HID (Push 2/3 display, Kontrol Mk1) and whether external crates
  are allowed for these. Graphics displays are drawn with `src/framework/graphics/` (bitmap font,
  no anti-aliasing, only the components of the small displays; the Push components, icons and
  anti-aliased drawing are missing).
- [ ] Device browser: presets only, with no filter columns and no device database (DeviceManager).
  This limits device and drum-pad browsing (Maschine).
- [ ] No parameter renaming or mapping (DeviceManager), and no device layers (Reaper has none).
- [ ] The knob sensitivity setting is not supported (affects slow knobs with Shift on the
  Maschine Mk3).
- [ ] Note attributes are limited to what Reaper supports (pitch, duration, mute, velocity). With
  the step grid of the host (Bitwig) the Launch Control XL, Maschine and Launchpad note editors
  support all attributes.
- [ ] Small duplications to clean up: `ArpeggiatorMode` and the scale name lists exist in both
  `config/` and `scale/`.

Host bridge findings (2026-10-31, from writing [HOST_BRIDGE_GUIDE.md](HOST_BRIDGE_GUIDE.md)), to
investigate:

- [x] Bug: `master solo/toggle` (master commands without a value) arrived through
  `process_no_arg`, whose handler in the Bitwig extension had no `master` case, so it was dropped.
  Fixed 2026-10-31; no ported controller toggles the master's solo (they toggle tracks of the
  bank), so it did not show yet.
- [x] The report of `/track/offset` (host paging) switched on all Bitwig command forms (`/inc`,
  `/reset`, `/immediately`, `solo/toggle`, `transport play|stop|restart`, device commands
  without index, …). Done 2026-10-31: the host reports its command forms with `/host/features`
  (`HostFeature`: valueInc, valueReset, immediately, indication, soloToggle, transportCommands,
  deviceCommands, layoutStep; HOST_BRIDGE_GUIDE.md §2); pinning, cursor track navigation, project
  unsolo/unmute, master activation and the VU clip state follow their own reports; a paging host
  without `/host/features` gets all features.
- [x] Commands the core sends which the Bitwig extension did not handle. Done 2026-10-31
  (BITWIG_DIVERGENCES.md §6 "Commands the extension did not handle"): `clip clear` (`clearSteps`),
  `time signature`, `time loop/start|length` (in beats), `track <p>/movePrev|moveNext` and
  `device <i>/movePrev|moveNext` (`moveTracks`/`moveDevices` as `swapWithPrevious/Next`), markers
  (`marker add`, `marker <i>/launch|select|remove`, reported from a cue marker bank of 100 if
  `numMarkers > 0`), `inifile reaper/prerollmeas` (`preRoll`, reported as
  `/click/prerollMeasures`), `action` (Bitwig action IDs), the crossfader (reported as
  `/crossfade`, the core then sends `transport crossfade` instead of `master user/param/0/value`),
  the track parameters `track <p>/param/…` (remote controls of the cursor track as
  `/track/fx/param/…`, on the pinned track with host pinning). Ignored: `toggleMultiSelect` (as
  Java), `device preset`, `device add/0` (Reaper only). The core does not send `device indicate`
  (the drum views use `drum indicate`). Also fixed: the parser read every marker value as a color
  and logged errors (Reaper too).
- [x] The cursor track's remote controls have no page names and do not follow the page selected
  in Bitwig (the project and device remote controls do); previous/next marker use
  `jumpToPrevious/NextCueMarker` instead of Java's search in the marker bank. Now the extension
  reports `/track/fx/page/…` and handles `track parameterPage`, and previous/next marker search
  the marker bank as `MarkerBankImpl.findAndSelect`. No ported controller uses markers yet, so
  the marker bank is untested in Bitwig.
- [x] Commands whose value the extension ignores: `track <p>/send/<s>/active` and
  `device <i>/bypass` toggle; `track <p>/select 0` is ignored. `send active` is a toggle in
  Reaper too (its value is the send volume), and the core only sends `select 1`; Reaper also
  ignores the value. `device <i>/bypass <value>` now sets the state, as in Reaper.
- [x] Addresses the parser accepts which the extension does not report: `/project/name`,
  `/project/isDirty`, `/track/<p>/depth|vuhold*|overdub|inQuant*`,
  `/device/touchedParam`, `/clip/notes|playposition|all`, `/marker/<i>/endPosition`,
  `/scene/<s>/position|endPosition`, `/master/fx/…`, the Reaper browser and groove
  addresses. The extension now reports `/project/name` and `/project/isDirty` (ProjectImpl).
  The others have no Bitwig counterpart in the Java version: track depth, VU hold, track
  overdub, touched parameter, clip play position and note list, region positions, master FX,
  and the Reaper browser and groove are Reaper-only. The track `inQuant*` comes from the
  application's `/recordQuantization/noteLength`, which the extension reports. The guide
  marks them as Reaper-only.
- [x] Docs lag the code: BITWIG_DIVERGENCES.md §3 (automation) and the "Protocol additions" of
  BITWIG_CORE_PORT_PLAN.md (now split into implemented and planned) are updated (2026-10-31).
- [x] The step gain is reported as `gain () / 2` but written with `setGain (value)`. This is
  the same as in Java (`StepInfoImpl.updateData`, `CursorClipImpl.updateStepGain`, clamped to
  0..1). Checked on Bitwig with the Java Mk2 (2026-10-31): a 0 dB note shows 50 %, 60 % is
  +5 dB, 40 % is -14 dB, and a knob step changes the gain a little. So Bitwig's `gain ()`
  returns twice the scale of `setGain` (0.5 = 0 dB), and the halving is right. No bug. But
  right after a setter the NoteStep object returns the set value in the setter's scale, and the
  extension read the live objects: with fast knob turns the core got half the gain. The
  extension now formats the steps when Bitwig's observer reports them (as
  `StepInfoImpl.updateData`).
