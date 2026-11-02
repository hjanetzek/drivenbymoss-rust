# Porting Guide: remaining DrivenByMoss controllers to Rust

This guide is for agents and developers porting the remaining controllers of DrivenByMoss4Reaper
from Java to the Rust crate in `rust/`. Read [STATUS.md](STATUS.md) first.

- Java source: `src/main/java/de/mossgrabers/` in the DrivenByMoss4Reaper repository.
  - `controller/<vendor>/<device>/`: the device code.
  - `reaper/controller/<vendor>/<device>/*ControllerInstance.java`: the Reaper glue.
  - `framework/`: the generic code.
- Rust crate: `rust/`, a single crate with no external dependencies.
- Reference implementations, simplest first: `src/controller/launchcontrol/` (knobs, faders,
  several mode managers), `src/controller/launchpad/` (pad grid, many views),
  `src/controller/maschine/` (text display, emulated blinking, encoder modes, shared setup for
  several models).

## 1. Remaining controllers

Java lines count `controller/<path>` only. The generic framework parts each one needs are
listed under "Needs".

| Controller | Java path | Java lines | Reaper instance classes | Needs (beyond what exists) | Suggested order |
| --- | --- | --- | --- | --- | --- |
| Akai APC mini Mk1/Mk2 | `akai/apcmini` | 2,747 | `APCminiMk1/Mk2ControllerInstance` | **Ported** (`src/controller/akai/apcmini/`) | — |
| Arturia Beatstep (+ Pro) | `arturia/beatstep` | 2,264 | `BeatstepControllerInstance` | — | 1 |
| ESI Xjam | `esi/xjam` | 474 | `XjamControllerInstance` | — | 1 |
| Yaeltex Turn | `yaeltex/turn` | 3,457 | `YaeltexTurnControllerInstance` | — | 1 |
| Akai APC40 Mk1/Mk2 | `akai/apc` | 3,928 | `APC40mkI/mkIIControllerInstance` | **Ported** (`src/controller/akai/apc/`) | — |
| Intuitive Instruments Exquis | `intuitiveinstruments/exquis` | 2,568 | `ExquisControllerInstance` | — | 2 |
| Faderfox EC4 | `faderfox/ec4` | 1,743 | `EC4ControllerInstance` | Text display | 2 |
| Novation Launchkey Mk3 / Mini Mk3 | `novation/launchkey` | 5,674 | `LaunchkeyMk3`, `LaunchkeyMiniMk3ControllerInstance` | Text display | 2 |
| Novation SL Mk1/Mk2 | `novation/sl` | 3,754 | `SLMkI/SLMkIIControllerInstance` | Text display | 3 |
| Novation SL Mk3 | `novation/slmkiii` | 5,201 | `SLMkIIIControllerInstance` | Text display (pixel display via sysex) | 3 |
| Mackie HUI | `mackie/hui` | 2,969 | `HUI1/2/3ControllerInstance` | Ported (`src/controller/mackie/hui/`) | — |
| Mackie MCU | `mackie/mcu` | 7,654 | `MCU1/2/3/4ControllerInstance` | Ported (`src/controller/mackie/mcu/`) | — |
| Akai ACVS (MPC/Force) | `akai/acvs` | 3,866 | `ACVSLiveControllerInstance` | **Ported** (`src/controller/akai/acvs/`; the second MIDI input is not opened) | — |
| Electra One / One Mini | `electra/one` | 5,441 | `ElectraOne`, `ElectraOneMiniControllerInstance` | Sysex/Lua protocol | 3 |
| Oxi One Mk1/Mk2 | `oxi/one` | 6,215 | `OxiOneMk1/Mk2ControllerInstance` | Graphics display (bitmap → sysex): `framework::graphics` exists, the Oxi components are missing | 4 |
| Akai Fire | `akai/fire` | 6,299 | `FireControllerInstance` | **Ported** (`src/controller/akai/fire/`, OLED with `framework::graphics`; the XoX drum view needs `AbstractDrumXoXView`) | — |
| NI Kontrol Mk1 (S25–S88) | `ni/kontrol/mki` | part of 7,390 | `KontrolMkIS25/49/61/88ControllerInstance` | USB/HID (`Kontrol1UsbDevice`) | 5 |
| NI Kontrol Mk2/Mk3 (protocol V1–V3) | `ni/kontrol/mkii` | part of 7,390 | `KontrolProtocolV1/V2/V3ControllerInstance` | — (NI Kontrol MIDI protocol, track/parameter names via sysex) | 2 |
| NI Maschine Mk3, Mikro Mk3, Plus, Studio | `ni/maschine/mk3` | (ported) | `MaschineMk3/MikroMk3/Plus/StudioControllerInstance` | Mostly done: register in `SUPPORTED_MODELS` (`src/controller/maschine/mk/mod.rs`), add tests; Studio needs the monitor-encoder section (`StudioEncoderModeManager`, MST/GRP/SND/CUE/IN1–4, encoder VU) | 2 |
| Generic Flexi | `generic/**` | 8,759 | `GenericFlexiControllerInstance` | **Ported** (`src/controller/generic/`, see STATUS.md) | — |
| Ableton Push 1/2/3 | `ableton/push` | 21,260 | `Push1/2/3ControllerInstance` | Push 1: text display. Push 2/3: graphics rendering (`framework/graphics`, 3,939 lines + 1,618 Reaper side) and USB bulk transfer of the display | 5 |
| OSC | `osc/**` | 3,801 | `OSCControllerInstance` | UDP networking, OSC encoding (`framework/osc`) | 4 |
| Gamepad | `gamepad` | 291 | `GamepadControllerInstance` | Gamepad input (jamepad in Java): needs a native library | 5 |
| Utilities: Auto color, MIDI monitor | `utilities/**` | 793 | `AutoColorInstance`, `MidiMonitorInstance` | Auto color: track name → color rules; MIDI monitor: log only | 1 |

Order key:

1. Small, pure MIDI.
2. Medium, or needs a small generic addition.
3. Needs a display protocol or multiple surfaces.
4. Large or needs new infrastructure.
5. Needs external I/O (USB, HID, native libraries). **Ask the project owner before adding
   dependencies.**

## 2. How a controller is structured in Rust

```
src/controller/<name>/
  mod.rs          factory: impl ControllerFactory (info, Java class name, MIDI discovery,
                  create_configuration, create) + registration list
  definition.rs   models, sysex headers, MIDI port names for auto detection, UUIDs
  colors.rs       color manager (color IDs → controller color indices)
  surface.rs      controller-specific state stored in `ctx.ext` + helper functions
                  (former XyzControlSurface)
  setup.rs        create(): builds ValueChanger, Scales, ModelSetup, Model, MidiInput/Output,
                  Surface, Ctx; impl ControllerSetup (init: modes, views, buttons, knobs,
                  lights; startup; handle_changes; flush; exit)
  mode/ view/ command/   controller-specific modes, views and commands
src/config/<name>.rs     controller-specific configuration (wraps `Configuration`)
tests/<name>_*.rs        integration tests
```

Register the factory in `src/controller/mod.rs::all_factories()`. The factory's
`java_class_name()` must return the fully qualified Java instance class name, e.g.
`de.mossgrabers.reaper.controller.akai.apcmini.APCminiMk2ControllerInstance`. It is stored as
`CONTROLLER_INSTANCE<n>` in `DrivenByMoss4Reaper.config`, which keeps existing user
configurations working.

## 3. Porting recipe

1. Read the Java device package completely, including its `*ControllerSetup`, `*ControlSurface`,
   `*ControllerDefinition`, `*Configuration` and `*ColorManager` classes, and its Reaper
   instance class. List the generic framework classes it imports and check what already exists
   in Rust (section 5).
2. Port the definition and colors first. Keep the exact sysex bytes, CC and note numbers, port
   names and UUIDs. Write unit tests for the sysex builders.
3. Port the configuration. Follow `src/config/launchpad.rs`: activate exactly the settings the
   Java `init` activates.
4. Port the surface: controller state goes into a struct stored in `ctx.ext`. Overridden Java
   surface methods become helper functions or hooks:
   - `ctx.surface.trigger_sender` for `setTrigger`
   - `ctx.midi_handler` / `ctx.sysex_handler` for `handleMidi` / sysex
   - pad grid translation and blink mode
   - `ctx.text_display`
5. Port the modes, views and commands. Reuse the generic ones; port missing generic parts into
   `src/framework/` so the next controller can use them (section 4).
6. Port the setup: `createModes`, `createViews`, `registerTriggerCommands`,
   `registerContinuousCommands`, `createObservers` (→ `handle_changes`), `startup`, `exit`,
   flush overrides. Skip `layoutControls`, which is simulator-only UI.
7. Write tests (section 6). Run `cargo test` and `cargo clippy --all-targets` until both are
   clean.

## 4. Conventions and lessons learned

- **Context, not references.** Every view, mode and command method gets `&mut Ctx`; light
  suppliers get `&Ctx`. Never store references to the model or surface.
- **Inheritance becomes composition plus hooks.** A Java abstract class becomes a base struct
  (`XyzBase`) with methods named like the Java ones. Overridable template methods become a small
  hook trait with default implementations. The concrete type embeds the base and implements
  `View`/`Mode`, forwarding what it does not change. Examples: `SessionViewBase` + `SessionHooks`,
  `ParameterModeBase`, `DrumBase` + `DrumHooks`.
- **Views and modes are `Rc<RefCell<dyn View|Mode>>` and are borrowed while their methods run.**
  - Calling back into the same view or mode through `ctx` panics or is skipped (`try_borrow_mut`).
  - To switch away from or deactivate the current view from inside it, defer with
    `ctx.schedule(0, |ctx| ...)`, or use the helpers in `src/framework/view/deferred.rs`.
  - Java `instanceof` checks become trait methods with default implementations on `View`/`Mode`
    (e.g. `scroll_states`, `as_transpose_view`, `note_editor`,
    `sequencer_resolution_index`) or downcasts via `as_any()`.
- **Observers become events.** Drain `ctx.model.take_events()` and
  `ctx.config.take_changed_settings()` in `ControllerSetup::handle_changes`. Forward them with
  `ctx.dispatch_model_events_to_views` / `dispatch_setting_changes_to_views`. The app layer
  handles parser events.
- **Delayed tasks:** Java `host.scheduleTask(r, ms)` → `ctx.schedule(ms, |ctx| ...)` (or the
  model's own scheduler for model-internal tasks).
- **Displays:**
  - Text displays use `src/framework/display.rs` (`TextDisplay` + a device-specific
    `LineWriter`; `McuDisplay` exists).
  - Controllers without a display use `ctx.notify(...)`, which goes to the text display when one
    is set, otherwise to the host.
- **Several mode managers** (e.g. knob modes and fader modes):
  - `ctx.add_mode_manager()`, `register_mode_in`, `set_active_mode_in`, and so on.
  - One mode object cannot be registered in two managers; use a small forwarding mode instead.
- **Knobs and faders:**
  - Bind them to parameters with `ctx.surface.bind_continuous_parameter(id, Some(ParamRef))`.
    While bound, values go to the parameter instead of the command, as with Java
    `bind(IParameter)`.
  - Parameter modes do this automatically through their parameter provider.
- **Keep Java semantics, including quirks**, unless they are clearly bugs. Document every
  deviation in a comment and in your report. Earlier ports kept, among others, the double-tempo
  long press and the "resolution mode does not mark itself as used" behaviour.
- **Reaper limitations:** Reaper has no effect track bank, no device layers (layers map to the
  first instrument's drum pads) and only a few note attributes. Mirror what the Java Reaper
  implementation does, usually nothing.
- **No external dependencies** without the project owner's agreement. The C ABI and MIDI go
  through the Reaper backend, so most controllers need none.
- **Style:** LGPL header in every file (copy from an existing file), snake_case versions of the
  Java names, comments where the Java code has them or where Rust differs. No warnings,
  including clippy.

### Working in parallel (several agents)

- Give every agent exclusive ownership of its directories and of the new generic framework files
  it creates. Decide up front who owns shared generic parts both controllers need, and let the
  owner land the API first.
- Shared files (`ctx.rs`, `surface.rs`, `feature.rs`, `grid.rs`, the model, `config/mod.rs`,
  `controller/mod.rs`, `app.rs`):
  - Additions only, never signature changes.
  - Re-read right before editing.
  - Report every edit.
  - `mod.rs` files: append `pub mod` lines only.
- Keep the shared build green. Add a `pub mod` line only when the file compiles. If someone
  else's code breaks the build, wait and retry instead of editing their files.
- Agents must not commit. The lead verifies the combined build (`cargo test`,
  `cargo clippy --all-targets`, `cargo build --release`) and commits.

## 5. Framework catalog (what exists)

### Core

| Module | Contents |
| --- | --- |
| `framework::ctx` | `Ctx` |
| `framework::multi_surface` | Several surfaces per instance (MCU/HUI extenders): `add_surface`, `select_surface`, `with_surface`, `for_each_surface`, `share_button_states`, `connect_surface_mode_managers`; see [MULTI_SURFACE_PLAN.md](MULTI_SURFACE_PLAN.md) |
| `framework::feature` | `View`/`Mode` traits, managers, `ScrollStates`, `impl_as_any!` |
| `framework::command` | `TriggerCommand`, `ContinuousCommand`, `AftertouchCommand`, `PitchbendCommand`, `shared`, `FnTrigger`, `FnContinuous` |
| `framework::surface` | `Surface`, `HwButton`, `HwLight`, `HwContinuous`, `ButtonAction`, `TriggerSender` |
| `framework::grid` | `PadGrid`, `LightInfo`, `BlinkMode::{Channels, Collect, Emulated}` |
| `framework::display` | `TextDisplay`, `LineWriter`, `McuDisplay`, `Format` |
| `framework::graphics` | Graphics displays without dependencies: `Bitmap` (RGB, `encode_bgra`, `to_ascii_art`), `GraphicsContext` (fill/stroke rectangle, line, text in bounds/height; no anti-aliasing), the embedded 5x8 bitmap `font` (integer scaling), `GraphicsConfiguration::chromatic`, `GraphicsDimensions`, `Component` (Label, List, TitleValue, TitleValueMenu, TitleChannels) and `GraphicDisplay` (renders only on changes, notifications); `Ctx::graphics_display` shows the notifications |
| `framework::setup_helpers` | Scale, deactivated items and note repeat settings, `BrowserObserver`, track change handling |
| `framework::string_utils` | `format_percentage`, `format_measures`, `pad` |
| `framework::virtual_fader` | Virtual faders on pad columns |
| `framework::clip_launcher_navigator` | `IClipLauncherNavigator` (Reaper `ClipLauncherNavigatorImpl`): track/scene/clip navigation which keeps the selected slot |
| `framework::chord_identifier` | Chord names from notes |
| `framework::value_changer` | `ValueChanger` with all four `RelativeEncoding`s (`ValueChanger::new`, `ValueChanger::relative` = `RelativeValueChangers.get`) |
| `framework::ids` | `ButtonId`, `ContinuousId`, `OutputId`, `Views`, `Modes`, generated from the Java enums |
| `framework::color` | `ColorManager`, `BUTTON_STATE_*` |
| `framework::color_ids` | Color ID constants of the generic views |

`Ctx` provides:

- **Views and modes:** `register_view/mode`, `set_active/temporary_view`, `restore_view`, the
  same for modes, `activate_preferred_view`, `recall_preferred_view`.
- **Buttons:** `add_button`, `add_continuous`, `trigger_button`, `is_button_combination`,
  `simulate_button_press`.
- **Events and timing:** `handle_midi`, `handle_sysex`, `schedule`, `notify`, `flush_surface`.
- **Multiple mode managers:** `add_mode_manager`, `register_mode_in`, `set_active_mode_in`, and
  so on.

### Parameters

`framework::parameterprovider`:

- `ParamRef` addresses a parameter in the model and is resolved when a value arrives, so bank
  scrolling needs no rebinding.
- `CustomParameter`, `ParameterProvider`, `FnParameterProvider`.
- Providers: Empty, Fixed, Combined, RangeFilter, Bank, Volume, Pan, Send, SelectedTrack,
  LayerOrDrumPad.

### Modes

| Module | Contents |
| --- | --- |
| `framework::mode::parameter_mode` | `ParameterModeBase` (AbstractParameterMode), `ParameterBinding`, `ModeBank` |
| `framework::mode::track` | Track select/mute/solo/rec arm/stop clip/volume/pan/send/track modes (`new` / `new_with_controls`) |
| `framework::mode::device` | `ParameterMode`, `SelectedDeviceMode`, `ProjectParamsMode` |
| `framework::mode::{layer, master, sequencer, dummy, note_editor}` | `LayerMute/SoloMode`, `MasterAndFXVolumeMode`, `SequencerModeBase`, `DummyMode`, `NoteEditor` |
| `framework::mode::browser` | `BrowserMode` (device browser: knobs select the filter columns and the result) |
| `framework::mode::note_parameter` | `NoteAttribute`, `note_value`, `set_note_value`, `change_note_value` (former `NoteParameter`) |

### Views

| Module | Contents |
| --- | --- |
| `framework::view` | `base` (`ViewBase`, `on_scene_button`), `play_view` (`PlayViewBase`, `ChordViewBase`, `draw_piano_grid`), `session_view`, `scene_play_view`, `shift_view`, `browser_view`, `drum64_view`, `number_display_view` (tempo/shuffle), `transpose_view`, `deferred`, `control_only_view` (`ControlOnlyView`) |
| `framework::view::sequencer` | `SequencerBase`, `NoteSequencerBase`, `PolySequencerBase`, `RaindropsBase`, `DrumBase` (incl. DrumEx), `DrumLaneBase` (Drum4/8), `ClipLengthView` |

### Commands

| Module | Contents |
| --- | --- |
| `framework::commands::application` | Undo, Duplicate |
| `framework::commands::clip` | New (+ fixed length), Quantize |
| `framework::commands::cursor` | `CursorCommand` (+ `new_mode_cursor`), `Direction` |
| `framework::commands::transport` | Play (with the double click setting), Metronome, ConfiguredRecord |
| `framework::commands::transport_ext` | Tempo, Wind, Stop, TapTempo, ToggleLoop, Automation, Swing |
| `framework::commands::mode` | KnobRowMode, ButtonRowMode, ModeSelect, ModeMultiSelect |
| `framework::commands::view` | ViewButton, ViewMultiSelect, SelectPlayView, ToggleShiftView |
| `framework::commands::misc` | Overdub, Pane, Layout, PanelLayout, Browser, AddEffect, AddTrack, Convert, Double, StartScene, Footswitch |
| `framework::commands::aftertouch` | AftertouchViewCommand |
| `framework::commands::mv_helper` | Delayed notifications |
| `framework::commands::channel` | Solo, Mute, RecArm, CrossfadeMode, Master, StopClip, StopAllClips, Redo, DeviceOnOff, PlayPosition, `Timeout` |
| `framework::commands::device_nav` | DeviceLayerLeft/Right, SelectPrevious/NextDeviceOrParamPage |
| `framework::commands::transport_more` | Record (with triggers), Restart, Load, Save, LoopPosition, LoopLength |
| `framework::commands::general` | Nop, Shift, Save, Marker, Record (simple), AutomationMode, PunchIn/Out, ToggleTrackBanks, SelectTrack, TrackState (rec arm/solo/mute), RecArmAll, JogWheel; `FocusedParameter`, `ZoomParameter` |

### DAW model

`daw::model::Model` is the root. All banks are paged (`item(i)`, `item_mut(i)`, scrolling).

- `track_bank`, `cursor_track`, `master_track`
- `scene_bank`, `cursor_device`, `specific_device`, `drum_device`
- `note_clip(cols, rows)` (`CursorClip`, sparse step storage)
- `transport`, `project`, `application`, `browser` (presets only), `groove`, `marker_bank`

Events are drained with `take_events()`. The Reaper message parser is in `src/parser.rs`.

### Configuration and app

- `config::Configuration`: all generic settings, with change tracking, the properties file and
  document settings.
- `src/app.rs`: instance management, MIDI port detection, document settings.
- `src/ffi.rs`: the C ABI.

### Missing generic parts

These are not ported yet; port them when a controller needs them:

- Graphics: the base exists (`framework::graphics`, used by the Fire). Missing: the components
  of the large displays (Channel, Parameter, Sends, Options, Menu, ClipList, SceneList,
  MidiClip, GraphOverlay, ChannelSelect), icons (SVG images), anti-aliased drawing, rounded
  rectangles and vector fonts. Needed by Push 2/3 and Oxi One.
- `AbstractDrumXoXView` (the XoX drum sequencer). Needed by the Fire (Views.DRUM).
- A second MIDI input per port pair (the ACVS "Pads (Remote MIDI Port)").
- USB/HID: `framework/usb` and `reaper/framework/usb`. Needed by Push 2/3 (display) and
  Kontrol Mk1.
- OSC: `framework/osc` and `reaper/framework/osc`. Needed by the OSC controller.
- ~~Multiple surfaces per controller instance~~: done (`framework::multi_surface`, see
  [MULTI_SURFACE_PLAN.md](MULTI_SURFACE_PLAN.md)). A factory declares `num_port_pairs()` and
  implements `create_with_ports`; the context swaps the current surface in.
- Device browser filter columns and the device database (DeviceManager), and parameter
  mapping/renaming.
- Knob sensitivity (slow mode) of the model's value changer: the `DawContext` holds a copy of the
  value changer per parameter, so the sensitivity cannot be changed at runtime. Generic Flexi keeps
  its own copies of the relative value changers with the sensitivity and passes them to
  `Parameter::change_value_with`; modes which call `change_value` (e.g. Flexi's "Modes: Item"
  commands) still use the model's value changer without sensitivity.
- The effect track bank: the model has none (as Reaper), so the Flexi "FX Track" commands do
  nothing.

## 6. Testing requirements

Every controller needs:

- **Unit tests** for definitions (sysex builders, button maps), configuration defaults and
  persistence, and any non-trivial logic.
- **Integration tests** in `tests/<controller>_*.rs`. Use `App` + `RecordingBackend`, as in
  `tests/launchpad_smoke.rs`:
  1. Write a temporary `DrivenByMoss4Reaper.config` with the Java class name, and provide MIDI
     port names that match the discovery names.
  2. Call `app.start_infrastructure()`, feed model lines with `app.update_model(...)`, then call
     `app.tick()`. The setup's `startup` runs about 1 s after `init`; wait for it or call it
     explicitly.
  3. Send MIDI with `app.on_midi_event(port, &[status, d1, d2])`.
  4. Assert the backend calls (`BackendCall::SendMidi`, processor commands).
- **A "touch everything" test:** activate every registered view and mode of every model, and
  trigger every button and continuous control once (DOWN, LONG, UP), with `tick()` in between.
  No panics. This found real bugs in earlier ports.
- **Before handing back:** `cargo test`, `cargo clippy --all-targets` (0 warnings) and
  `cargo build --release`.
- **Conformance with the Java version (in progress, see CONFORMANCE_TESTING_PLAN.md):** scenarios
  from the manual (`tests/conformance/<controller>/*.toml`), run with the Java DrivenByMoss and
  the port on Bitwig and compared (`conformance_runner.py pair`), recorded runs replayed in
  `cargo test` (`tests/conformance_replay.rs`).
