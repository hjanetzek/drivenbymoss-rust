# DrivenByMoss4Reaper → Rust: Port Status

As of 2026-10-29 · branch `rust-port`

## Summary

Three controller families are ported: Novation Launchpad (5 models), Novation Launch Control XL,
and NI Maschine Mk2 and Jam. The crate has about 56,000 lines of Rust in 217 files. 187 tests
pass, 1 is ignored, and `cargo clippy --all-targets` reports 0 warnings. The release build
produces `libdrivenbymoss.so` (about 2.2 MB), which exports the 14 `dbm_*` functions.

It has **not run inside Reaper yet**. The C++ Reaper extension (separate repository) still starts
a JVM through JNI and has to switch to the C ABI in `include/drivenbymoss.h`.

| Controller | Models registered | Rust path | Integration tests |
| --- | --- | --- | --- |
| Novation Launchpad | MkII, Pro, Pro Mk3, X, Mini MkIII | `src/controller/launchpad/` | `launchpad_smoke.rs` (2), `launchpad_setup.rs` (10), `launchpad_views.rs` (11) |
| Novation Launch Control XL | XL | `src/controller/launchcontrol/` | `launchcontrol_xl.rs` (12) |
| NI Maschine | Mk2, Jam (Mk3, Mikro Mk3, Plus, Studio prepared, not registered) | `src/controller/maschine/` | `maschine_mk2_setup.rs` (15), `maschine_jam_setup.rs` (12) |

There are also 125 unit tests in the sources.

Commits:

- `e00a36ec`: core framework, Reaper DAW layer and Launchpad
- `a3bb3b89`: Launch Control XL and Maschine Mk2/Jam, with the parameter providers, the
  parameter, device, track and layer modes, the text display and more generic commands

**Maschine on Linux:** Maschine refuses to start on Linux, as in Java. Set
`DBM_ALLOW_MASCHINE_ON_LINUX` to override (the tests do this).

**Scope and size:**

- The Java project has about 240,000 lines in 1,544 files.
- The crate is self-contained in `rust/`: a single crate `drivenbymoss` (cdylib + rlib) with no
  external dependencies. The Java tree is unchanged.
- It can be split into its own repository with `git subtree split --prefix=rust`.

**Not ported:**

- The Swing/JavaFX UI and the simulator windows.
- USB, HID, graphics displays, OSC and the gamepad.
- All controllers not listed above. See [PORTING_GUIDE.md](PORTING_GUIDE.md) for the
  remaining ones.

## Architecture

The Rust library replaces the embedded JVM.

- **Calls in:** the `dbm_*` functions in `src/ffi.rs` mirror the static methods of the Java
  `Controller` class: `dbm_startup`, `dbm_update_model`, `dbm_on_midi_event`,
  `dbm_get/set_formatted_document_settings`, and so on.
- **Calls out:** the `DbmCallbacks` table of function pointers replaces the JNI `native` methods
  of `MainApp`: `process_*_arg`, MIDI open/close/send, and the note-input filters and
  translation tables.
- **Threading:**
  - A global mutex serializes all calls.
  - A background thread calls `App::tick()` every 30 ms, like the Java flush timer.
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
| `tests/` | — | Integration tests per controller |
| `doc/` | — | This file and the porting guide |

## Next steps and open issues

1. Change the C++ extension (separate repository) to load `libdrivenbymoss` and call the C ABI
   instead of the JVM.
2. Test with real hardware in Reaper and compare with the Java version.
3. Register and test Maschine Mk3, Mikro Mk3 and Plus, and port the Studio monitor-encoder
   section.
4. Port the remaining controllers, following [PORTING_GUIDE.md](PORTING_GUIDE.md).
5. Extract `rust/` into its own repository.

Open issues:

- [ ] There is no UI. Configuration comes from the `.config` files, and `dbm_display_window` only
  prints the log. Decide whether a UI is needed.
- [ ] Decide how to support USB/HID (Push 2/3 display, Kontrol Mk1) and graphics displays
  (Push 2/3, Fire, Oxi One), and whether external crates are allowed for these.
- [ ] Device browser: presets only, with no filter columns and no device database (DeviceManager).
  This limits device and drum-pad browsing (Maschine).
- [ ] No parameter renaming or mapping (DeviceManager), and no device layers (Reaper has none).
- [ ] The knob sensitivity setting is not supported (affects slow knobs with Shift on the
  Maschine Mk3).
- [ ] Note attributes are limited to what Reaper supports (pitch, duration, mute, velocity).
- [ ] Small duplications to clean up: `ArpeggiatorMode` and the scale name lists exist in both
  `config/` and `scale/`.
