# Framework map: Java → Rust

Every class of the Java DrivenByMoss framework (`de.mossgrabers.framework`), its Rust counterpart,
how complete the port is and where it differs. It is for agents and developers porting a
controller: look up each framework class the controller imports. Built 2026-11-02 from the code
(the "former `JavaName`" comments and the module docs); update it when you port a framework
class.

- **Reference:** DrivenByMoss `src/main/java/de/mossgrabers/framework/` and, for the model, its
  `bitwig/framework/daw/` implementations. DrivenByMoss4Reaper has the same framework classes
  (only `view/HeldChords` is missing there). "Reaper:" notes cover its `reaper/framework/daw/`
  classes, which the Rust model started from.
- **Status:**
  - **full**: ported.
  - **partial**: the divergence column says what is missing.
  - **missing**: no Rust counterpart; checked by grepping the class name and its snake_case
    name.
  - **n/a**: replaced by a Rust mechanism or not needed: UI, simulator, observers.
- **Rust paths** are relative to `src/`, except in the mode/view/graphics part, where they are
  relative to `src/framework/`.
  - "ctrl X": a port exists only inside controller X.
  - "BD": [BITWIG_DIVERGENCES.md](BITWIG_DIVERGENCES.md).
  - "HB": [HOST_BRIDGE_GUIDE.md](HOST_BRIDGE_GUIDE.md).
- **Patterns:**
  - A Java abstract class became a `*Base` struct, which the concrete type owns and forwards to,
    plus an optional `*Hooks`/`*Behavior` trait for the overridable methods.
  - Interfaces (`I*`) are merged into their struct or trait.
  - Observers became events (`Model::take_events`, `Configuration::take_changed_settings`).

**Rust-only framework parts** (no Java class):
- `framework::page_surface`: a grid controller showing the pages of a source
  ([SURFACE_LINK.md](SURFACE_LINK.md), [HOST_PAGE_PLAN.md](HOST_PAGE_PLAN.md)): the page model
  (`model.rs`), the `PageSource` trait (`source.rs`), `SurfaceClient` and `SurfaceMapping`
  (`client.rs`), `PageSurfaceView` and `PageSurfaceMode` (`view.rs`), the host's pages
  (`host.rs`).
- `framework::surface_link`: the surface link between two controller instances: registry and
  queues (`mod.rs`), `LinkProvider` (`provider.rs`), `LinkSource` (`source.rs`, a page source).
- `Views::SurfaceLink` and `Modes::SurfaceLink` are appended to the ID enums.

## Gaps by upcoming controller

| Controller | Missing in the framework |
| --- | --- |
| Push 2/3 | Done (2026-11-19): USB bulk out, the large-display components, icons, anti-aliasing, vector font, `ColorView`, `AbstractDrumXoXView`, `IExpressionView` (`View::is_expression_view`), `CreateSceneCommand`. Still missing: track crossfade, layer `enter`, the host MPE note input, `IColorSetting` (colors are text settings) |
| Akai Fire (DRUM view), Oxi One | `AbstractDrumXoXView` exists now (`DrumXoXBase`, without the Fire's `adjustPage`); Oxi: `NoteGenerator`, the large-display components, maybe an `AbstractNoteParameterMode` base |
| Kontrol Mk1 | HID (`IHidDevice`, `UsbMatcher`) + backend; `ILightGuide` (key LEDs); `MasterVolumeCommand`; `IMemoryBlock` |
| Kontrol Mk2/Mk3 | `CreateSceneCommand`, `StartClipCommand`, `WriteArranger/ClipLauncherAutomationCommand` |
| ESI Xjam | `SelectPrevious/NextDeviceCommand`, `SelectParamPageCommand`, `ToggleDeviceWindowCommand`, `EnableCommand` |
| SL MkIII | `MonitorCommand`, `ColorView`, `FeatureGroupButtonColorSupplier` |
| Yaeltex Turn | `AbstractMonophonicSequencerView` |
| Faderfox EC4 | mute/solo/crossfade as parameters (+ providers), `DummyView` |
| Electra One | - (ported 2026-11-22: a JSON reader, further MIDI outputs, `PlayPositionParameter`/`TempoParameter`; `MasterVolumeMode` was not needed, the Electra shares only its error text; `DummyView` was already in the framework) |
| OSC | - (ported: `framework::osc`) |

Used by several controllers, but ported only inside one, so move them to the framework first:
- `DeleteCommand` (Launchpad).
- `NoteRepeatCommand` and `FillModeNoteRepeatCommand` (Maschine).
- `KnobRowTouchModeCommand` (Fire, inline).
- `FaderAbsoluteCommand` and `MasterFaderAbsoluteCommand` (HUI).
- `TrackCrossfadeA/BMode` (ACVS).
- `MasterVolumeMode` (MCU).
- The `Modes`/`Views` helpers (`isTrackMode`, …).
- `NoteParameter`, and the wrapper providers FourKnob, SelectedLayerOrDrumPad and Reset.

Model gaps which affect behaviour on Bitwig:
- No cursor layer, layer sends or layer device chains; the layer providers always use the drum
  pads (BD §8).
- `NoteRepeat` has no octaves, latch, shuffle or free-running mode.
- Track lists are flat (no hierarchy).
- `MasterAndFXVolumeMode` does not bind the effect track bank yet.
- No runtime knob sensitivity (`ISensitivityCallback`).

## Commands, controller, feature groups, configuration, utils

### framework/command/core

| Java | Rust | Status | Divergences |
|---|---|---|---|
| TriggerCommand, AbstractTriggerCommand | `framework::command::TriggerCommand` (trait; `shared`, `SharedTrigger`, `FnTrigger`) | full | Default `execute` dispatches to `execute_normal`/`execute_shifted` on Shift |
| ContinuousCommand, AbstractContinuousCommand | `framework::command::ContinuousCommand`, `FnContinuous` | full | |
| AftertouchCommand, AbstractAftertouchCommand | `framework::command::AftertouchCommand` | full | |
| PitchbendCommand, AbstractPitchbendCommand | `framework::command::PitchbendCommand` | full | |
| NopCommand | `framework::commands::general::NopCommand` | full | |

### framework/command (root), command/aftertouch, command/pitchbend

| Java | Rust | Status | Divergences |
|---|---|---|---|
| TempoCommand | `framework::commands::transport_ext::TempoCommand` | full | |
| AftertouchViewCommand | `framework::commands::aftertouch::AftertouchViewCommand` | full | |
| FaderRowModeCommand | - | missing | Pitchbend fader that feeds the active mode (no Java users in controllers) |

### framework/command/continuous

| Java | Rust | Status | Divergences |
|---|---|---|---|
| KnobRowModeCommand | `framework::commands::mode::KnobRowModeCommand` | full | Works on a chosen mode manager (`Ctx::mode_manager`) |
| JogWheelCommand | `framework::commands::general::JogWheelCommand` | full | |
| LoopLengthCommand, LoopPositionCommand | `framework::commands::transport_more::{LoopLengthCommand, LoopPositionCommand}` | full | |
| PlayPositionCommand | `framework::commands::channel::PlayPositionCommand` | full | |
| SwingCommand | `framework::commands::transport_ext::SwingCommand` | full | |
| FaderAbsoluteCommand, MasterFaderAbsoluteCommand | `controller::mackie::hui::setup::{FaderAbsoluteCommand, MasterFaderAbsoluteCommand}` | partial | Only in ctrl HUI; not generic |
| CrossfaderCommand | - | missing | `Model::crossfade_parameter_mut` exists; use `FnContinuous` |
| MasterVolumeCommand | - | missing | Java user: Kontrol Mk1 main encoder |

### framework/command/trigger (root)

| Java | Rust | Status | Divergences |
|---|---|---|---|
| AbstractDoubleTriggerCommand | `framework::commands::transport::DoubleClick` (helper, `DOUBLE_CLICK_DELAY`) | full | Composition helper, not a base class |
| BrowserCommand | `framework::commands::misc::BrowserCommand` | partial | Device filter columns/DeviceManager missing (see PORTING_GUIDE) |
| Direction | `framework::commands::cursor::Direction` | full | |
| FootswitchCommand | `framework::commands::misc::FootswitchCommand` | full | |
| MarkerCommand | `framework::commands::general::MarkerCommand` | full | |
| ShiftCommand | `framework::commands::general::ShiftCommand` | full | |

### framework/command/trigger/application

| Java | Rust | Status | Divergences |
|---|---|---|---|
| UndoCommand, DuplicateCommand | `framework::commands::application::{UndoCommand, DuplicateCommand}` | full | |
| RedoCommand | `framework::commands::channel::RedoCommand` | full | |
| SaveCommand | `framework::commands::general::SaveCommand` | full | |
| LoadCommand | `framework::commands::transport_more::LoadCommand` | full | |
| OverdubCommand, PaneCommand, LayoutCommand, PanelLayoutCommand | `framework::commands::misc::{OverdubCommand, PaneCommand, LayoutCommand, PanelLayoutCommand}` | full | |
| DeleteCommand | `controller::launchpad::command::misc::DeleteCommand` | partial | Only in ctrl Launchpad; Push and Oxi One also need it |

### framework/command/trigger/clip

| Java | Rust | Status | Divergences |
|---|---|---|---|
| NewCommand, TemporaryNewCommand | `framework::commands::clip::NewCommand` | full | TemporaryNew is the fixed-length option of `NewCommand` |
| QuantizeCommand | `framework::commands::clip::QuantizeCommand` | full | |
| ConvertCommand, DoubleCommand, StartSceneCommand | `framework::commands::misc::{ConvertCommand, DoubleCommand, StartSceneCommand}` | full | |
| StopClipCommand, StopAllClipsCommand | `framework::commands::channel::{StopClipCommand, StopAllClipsCommand}` | full | |
| NoteRepeatCommand, FillModeNoteRepeatCommand | `controller::maschine::core::command::note_repeat::{NoteRepeatCommand, FillModeNoteRepeatCommand}` | partial | Only in ctrl Maschine; Push and JAM use them too |
| StartClipCommand | - | missing | Use `Slot::launch` directly; Java users: Push SessionMode, Kontrol Mk2 |
| CreateSceneCommand | - | missing | `Project::create_scene_from_playing_launcher_clips` exists; Java user: Push |

### framework/command/trigger/device

| Java | Rust | Status | Divergences |
|---|---|---|---|
| AddEffectCommand | `framework::commands::misc::BrowserCommand` (insert-only flag) | full | Merged into BrowserCommand |
| DeviceLayerLeftCommand, DeviceLayerRightCommand | `framework::commands::device_nav::{DeviceLayerLeftCommand, DeviceLayerRightCommand}` | full | |
| SelectPreviousDeviceOrParamPageCommand, SelectNextDeviceOrParamPageCommand | `framework::commands::device_nav::{SelectPreviousDeviceOrParamPageCommand, SelectNextDeviceOrParamPageCommand}` | full | |
| DeviceOnOffCommand | `framework::commands::channel::DeviceOnOffCommand` | full | |
| SelectPreviousDeviceCommand, SelectNextDeviceCommand, SelectParamPageCommand | - | missing | `framework::mode::device` has `select_previous/next(_page)`; Java user: ESI Xjam |
| ToggleDeviceWindowCommand | - | missing | `CursorDevice::toggle_window_open` exists; Java user: Xjam |
| DeviceParamsKnobRowCommand | - | missing | No Java users in controllers |

### framework/command/trigger/mode

| Java | Rust | Status | Divergences |
|---|---|---|---|
| ButtonRowModeCommand, ModeSelectCommand, ModeMultiSelectCommand | `framework::commands::mode::{ButtonRowModeCommand, ModeSelectCommand, ModeMultiSelectCommand}` | full | Works on a chosen mode manager |
| CursorCommand, ModeCursorCommand | `framework::commands::cursor::CursorCommand` (`CursorKind`, `new_mode_cursor`) | full | Merged into one command; override the `scroll_*` functions by wrapping it |
| KnobRowTouchModeCommand | inline `FnTrigger` in `controller::akai::fire::setup` calling `Mode::on_knob_touch` | partial | No generic command; Push and Kontrol Mk1 also need it |

### framework/command/trigger/track

| Java | Rust | Status | Divergences |
|---|---|---|---|
| SoloCommand, MuteCommand, RecArmCommand, CrossfadeModeCommand, MasterCommand | `framework::commands::channel::{SoloCommand, MuteCommand, RecArmCommand, CrossfadeModeCommand, MasterCommand}` | full | Use `RecArmCommand::with_shifted` where Java subclasses override executeShifted |
| SelectCommand | `framework::commands::general::SelectTrackCommand` | full | Renamed |
| RecArmAllCommand, ToggleTrackBanksCommand | `framework::commands::general::{RecArmAllCommand, ToggleTrackBanksCommand}` | full | Effect track bank: see PORTING_GUIDE |
| AddTrackCommand | `framework::commands::misc::AddTrackCommand` | full | |
| (TrackState helper) | `framework::commands::general::TrackStateCommand` | n/a | Rust-only: combined rec arm/solo/mute (MCU/HUI) |
| EnableCommand | - | missing | `Channel::toggle_is_activated` exists; Java user: Xjam |
| MonitorCommand | - | missing | `Track::toggle_monitor` exists; Java user: SL MkIII |
| SelectPrevNextTrackCommand | ctrl launchkey (`command::SelectPrevNextTrackCommand`) | full | Only in the Launchkey port; move to the framework when a second controller needs it |
| ToggleVUCommand | - | missing | Toggles config VU meters; no Java users in controllers |

### framework/command/trigger/transport

| Java | Rust | Status | Divergences |
|---|---|---|---|
| PlayCommand, MetronomeCommand, ConfiguredRecordCommand | `framework::commands::transport::{PlayCommand, MetronomeCommand, ConfiguredRecordCommand}` | full | |
| WindCommand, StopCommand, TapTempoCommand, ToggleLoopCommand, AutomationCommand | `framework::commands::transport_ext::{WindCommand, StopCommand, TapTempoCommand, ToggleLoopCommand, AutomationCommand}` | full | |
| RecordCommand | `framework::commands::transport_more::RecordCommand` (with triggers), `framework::commands::general::RecordCommand` (default triggers) | full | Two variants |
| RestartCommand | `framework::commands::transport_more::RestartCommand` | full | |
| AutomationModeCommand, PunchInCommand, PunchOutCommand | `framework::commands::general::{AutomationModeCommand, PunchInCommand, PunchOutCommand}` | full | |
| WriteArrangerAutomationCommand, WriteClipLauncherAutomationCommand | - (part of `AutomationCommand`) | missing | Transport toggles exist; Java user: Kontrol Mk2 |
| RecQuantizationCommand | - | missing | No Java users in controllers |

### framework/command/trigger/view

| Java | Rust | Status | Divergences |
|---|---|---|---|
| ViewButtonCommand, ViewMultiSelectCommand, SelectPlayViewCommand, ToggleShiftViewCommand | `framework::commands::view::{ViewButtonCommand, ViewMultiSelectCommand, SelectPlayViewCommand, ToggleShiftViewCommand}` | full | |
| FeatureGroupButtonColorSupplier | - (closures of type `framework::surface::LightSupplier` with `Ctx::button_color_from_active_view`) | missing | No generic supplier; Java users: APC, Launchkey (ported with the closure), SL MkIII |

### framework/controller

| Java | Rust | Status | Divergences |
|---|---|---|---|
| ButtonID, ContinuousID, OutputID | `framework::ids::{ButtonId, ContinuousId, OutputId}` | full | Generated from Java enums; `ButtonId::get(base, i)` |
| IControlSurface, AbstractControlSurface | `framework::surface::Surface` (+ `framework::ctx::Ctx` for handle_midi/flush) | partial | No knob sensitivity callback; no light guide; no piano keyboard |
| IControllerSetup, AbstractControllerSetup | `controller::ControllerSetup` trait, `framework::setup_helpers`, `framework::multi_surface` | full | Java `init` template is split across helpers; settings by `take_changed_settings` |
| IControllerDefinition, DefaultControllerDefinition | `controller::{ControllerFactory, ControllerDefinitionInfo}` | full | `midi_discovery_pairs(OperatingSystem)` |
| ISetupFactory | `controller::ControllerFactory::create` | n/a | Replaced by the factory trait |
| DummyControlSurface | - | n/a | Not needed (no OSC/headless surface) |

### framework/controller/color

| Java | Rust | Status | Divergences |
|---|---|---|---|
| ColorManager | `framework::color::ColorManager` | full | Not in Java: the path of RGB colors to the palette (see below) |
| ColorEx | `daw::color::ColorEx` | full | Not in Java: `to_lab`, `perceptual_distance` (CIEDE2000) |
| ColorIndexException | - | n/a | Replaced by Rust error handling |

**The path of an RGB color to the palette** (track, clip, scene and drum pad colors, the slots of
the surface link): `ColorManager::color_id (color)` (a color ID, for the code which passes IDs on)
and `ColorManager::index_for_color (color)` (its palette index) replace Java's
`colorManager.getColorIndex (DAWColor.getColorID (color))`. The path depends on the
`ColorSource` of the color manager, which the app sets by the host before `init`:

- **Bitwig** (`ColorSource::for_host`: the `app_version` of `dbm_create_controller` starts with
  "Bitwig"; the Bitwig extension gives "Bitwig Studio 5.3.13", the conformance runner, its replays
  and zynthian's conformance runs give the same): as Java, the closest of Bitwig's track colors
  (`DawColor::closest`), then the controller's DAW color table. Exact for Bitwig, whose track
  colors are these entries; unchanged, so the conformance runs and replays stay the same (a test
  checks every controller).
- **Direct** (all other hosts: the reference host, zynthian, Reaper's `dbm_create`): on a hue ×
  level palette the hue of the color at the level of its role (see "Roles" below); otherwise the
  perceptually nearest entry of the controller's own palette (`set_palette_rgb`), CIEDE2000 in
  CIE L*a*b*, which keeps the hue and lets the lightness count less: orange stays orange (F49500
  on NI's palette: 15, not the lime green 22 of the table), dark colors keep their hue, greys stay
  grey. Black entries are no candidates (an RGB color is never off; Bitwig's path makes black a
  grey too). The color ID is then `PALETTE_<index>`. The lookups are cached per color.
- **The surface link** always takes the direct path (`direct_color_id_for_role`): its colors
  never come from Bitwig.

**Roles** (`ColorRole`, `index_for_color_role (color, role)`, `color_id_for_role`;
`index_for_color` is the normal role). A palette made of hues × levels describes itself with
`set_palette_hue_levels (hues, levels, whites)` (NI's: the 16 hues at their full level, the level
offsets `NI_LEVELS` and the whites `NI_WHITES`). On the direct path such a palette takes only the
**hue** from the RGB color and the **level** from the role the caller knows:

| Role | Meaning | NI level | Achromatic |
|---|---|---|---|
| `Disabled` | exists, but disabled, hidden or covered | dim (0) | 76 (dark grey) |
| `Inactive` | available but not active (Java's `*_LO`) | medium (1) | 77 (grey) |
| `Normal` | the color itself, active (default) | full (2) | 78 (white) |
| `Highlight` | highlighted, selected | pale (3) | 79 (white) |

Off stays black at the caller. The hue is the nearest HSV hue angle of the palette's full-level
colors (`palette_hue`): neither brightness nor saturation counts, so a dark red is red and a
mid-bright Electra teal (03A598) is teal at full, not a medium or dim entry as with the nearest
entry; colors with a saturation below 0.25 or near black are achromatic (white/grey; on the JAM
68..79 are one white). The angle, not CIEDE2000, since NI's table holds LED drive values (the JAM's
factory palette, feature report 0xF1, `support/linux/ni_host/data/jam-feature-reports-factory.json`),
whose sRGB hue is not the perceived one. Examples (normal / highlight / inactive / disabled):
F49500 → 14/15/13/12 (orange), FFFF00 → 18/19/17/16, 03A598 → 38/39/37/36, 529DEC → 42/43/41/40,
C44795 → 58/59/57/56, 640000 → 6/7/5/4 (before: the nearest entry, 64, a dim red), 808080 and
202020 → 78/79/77/76. Where roles are used: the surface link (normal slots full, the selected slot
pale, disabled and covered slots dim, the default colors of inactive row and page selectors
medium) and the Maschine's `dim_or_highlight_color` on other hosts (the group buttons, the JAM's
scene buttons: selected full, the others medium); all other callers are normal (full). Palettes
without the description (the 128 colors of Novation, Push, APC) keep the nearest entry and ignore
the role; the surface link draws disabled and covered slots off there. The Bitwig path ignores
the role (Java's tables, conformance). The Maschine's "LED Brightness" Bright is applied after
this, when the LEDs are sent: it raises only dim (to medium), so in Bright `Disabled` and
`Inactive` merge (the "brighter, fewer steps" option), while inactive, normal and highlight stay
distinct.

Palette tables: NI's Maschines (`maschine::core::colors::NI_PALETTE`, hue × 4 levels: the JAM's
factory palette read from the device, the greys and white from cabl, MIT), Push 1 (`push::colors::DEFAULT_PALETTE`), Push 2/3 (the palette the core writes, with the
DAW colors at 70..96), the APC40 mkII, the Launchpads and the Launchkey Mk3 (Novation's velocity
palette, the same `DEFAULT_PALETTE`). The Maschine's dim color (`dim_or_highlight_color`) is the
inactive role (medium) of the same hue on the direct path (Java's `index / 8 * 8 + 5` only fits its table).
Without a table (the DAW color path everywhere): the Fire and the MPC/Force (their indices are a
virtual palette whose RGB colors are the DAW colors), the APC40 mk1 and the APCmini mk1/mk2, the
Beatstep, the Launch Control XL, the MCU, the OSC controller, the Electra One (it sends RGB in its
JSON, its indices only switch pads on and off). Not changed: `selected_channel_color_entry` of
the track and drum pad banks (the Push display shows the closest DAW color) and the Fire's fixed
UI colors.

### framework/controller/display

| Java | Rust | Status | Divergences |
|---|---|---|---|
| IDisplay, ITextDisplay, AbstractTextDisplay | `framework::display::{TextDisplay, LineWriter}`, `Ctx::text_display`, `Ctx::notify` | full | Only ASCII is sent; lines are sent synchronously |
| Format | `framework::display::Format` | full | |
| IGraphicDisplay, AbstractGraphicDisplay | `framework::graphics::GraphicDisplay` | full | All `IGraphicDisplay` methods (snake_case, Java order); components take plain data (`SceneInfo`, `SlotInfo`, `MidiClipInfo`, `SendData`); `render` + `has_changed`/`generation` |
| DummyDisplay | - | n/a | OSC: `Ctx::notify` without a text display shows the host notification, as DummyDisplay |

### framework/controller/grid

| Java | Rust | Status | Divergences |
|---|---|---|---|
| IPadGrid, PadGridImpl | `framework::grid::PadGrid` | full | Controller-specific sending through `GridTranslation`/closures, not subclasses |
| BlinkingPadGrid | `framework::grid::{BlinkMode::Emulated, BlinkingState}` | full | |
| LightInfo | `framework::grid::LightInfo` | full | |
| IVirtualFader, VirtualFaderImpl, IVirtualFaderCallback | `framework::virtual_fader::{VirtualFader, VirtualFaderCallback}` | full | Shared as `Rc<RefCell<..>>` |
| ILightGuide, LightGuideImpl | - | missing | Play view draws only the grid; Java users: Kontrol Mk1, keyboards with key LEDs |

### framework/controller/hardware

| Java | Rust | Status | Divergences |
|---|---|---|---|
| IHwButton, AbstractHwButton, ButtonEventHandler | `framework::surface::{HwButton, ButtonAction, ButtonEventHandler}` | full | |
| IHwLight | `framework::surface::{HwLight, LightSupplier, LightSender}` | full | |
| IHwControl, IHwInputControl, IHwContinuousControl, IHwAbsoluteControl, IHwAbsoluteKnob, IHwRelativeKnob, IHwFader, Abstract*Control | `framework::surface::{HwContinuous, ContinuousKind}` | full | One struct; the kind selects the behaviour; parameter binding uses `ParamRef` |
| BindType | `midi::BindType` | full | |
| IHwSurfaceFactory | `framework::surface::Surface::create_*` | n/a | Replaced by the surface methods |
| IHwPianoKeyboard | - | missing | Only the simulator UI uses it |
| IHwTextDisplay, IHwGraphicsDisplay | - | n/a | Simulator UI proxies |
| BindException | - | n/a | Replaced by Rust error handling |

### framework/controller/valuechanger

| Java | Rust | Status | Divergences |
|---|---|---|---|
| IValueChanger, TwosComplementValueChanger, OffsetBinaryRelativeValueChanger, SignedBitRelativeValueChanger, SignedBit2RelativeValueChanger | `framework::value_changer::ValueChanger` | full | One struct with a `RelativeEncoding` |
| RelativeEncoding | `framework::value_changer::RelativeEncoding` | full | |
| RelativeValueChangers | `ValueChanger::relative` | full | |
| ISensitivityCallback | - | missing | Sensitivity can't change at runtime in the model (PORTING_GUIDE: slow mode) |

### framework/featuregroup

| Java | Rust | Status | Divergences |
|---|---|---|---|
| IFeatureGroup, AbstractFeatureGroup | `framework::feature::{View, Mode}` traits, `framework::view::base` | full | No common base trait |
| IView, AbstractView | `framework::feature::View`, `framework::view::base::ViewBase` | full | Composition; `forward.rs` macros forward to the base |
| IMode, AbstractMode | `framework::feature::Mode`, parts in `framework::mode::parameter_mode` | full | |
| AbstractParameterMode | `framework::mode::parameter_mode::{ParameterModeBase, ParameterBinding, ModeBank}` | full | `ParamRef`s resolved on access, no rebinding on bank scroll |
| FeatureGroupManager, ModeManager, ViewManager | `framework::feature::{FeatureGroupManager, ModeManager, ViewManager}` | full | Several mode managers through `Ctx::add_mode_manager` |
| FeatureGroupChangeListener | `framework::feature::ChangeListener` (closure) | full | |
| IScrollableView | `View::scroll_states` / `framework::feature::ScrollStates` | full | |
| IExpressionView | - | missing | MPE/expression pads; Java user: Push |

### framework/observer

| Java | Rust | Status | Divergences |
|---|---|---|---|
| IValueObserver, IIndexedValueObserver, IItemSelectionObserver, IBankPageObserver, IParameterValueObserver, IParametersAdjustObserver, INoteObserver, IObserverManagement | `daw::events` (drained with `Model::take_events`) | n/a | Replaced by polling and model events, e.g. `TrackBankPageChanged`, `TrackNote`; `ParamRef` needs no adjust observers |
| ISettingObserver | `Configuration::take_changed_settings` | n/a | Replaced by polling the changed setting IDs |

### framework/configuration

| Java | Rust | Status | Divergences |
|---|---|---|---|
| Configuration, AbstractConfiguration | `config::configuration::Configuration` (+ `config::values`, `config::properties`) | full | `activate_*` replaces init; deactivated items must be applied by the caller |
| ISetting, IEnumSetting, IIntegerSetting, IBooleanSetting, IStringSetting, IActionSetting, ISignalSetting, IValueSetting | `config::setting::{SettingId, SettingKind, SettingDescriptor}` | full | Descriptors only, no UI objects |
| IColorSetting, IDoubleSetting | - | missing | No `SettingKind` for them; Java user of color: Push |
| ISettingsUI | `config::setting::Scope` | n/a | Replaced by global/document scope |

### framework/utils

| Java | Rust | Status | Divergences |
|---|---|---|---|
| ButtonEvent | `midi::ButtonEvent` | full | |
| ChordIdentifier | `framework::chord_identifier` | full | |
| KeyManager | `scale::KeyManager` | full | |
| ScrollStates | `framework::feature::ScrollStates` | full | |
| StringUtils | `framework::string_utils`, `framework::display::pad`, `config::values::fix_ascii`, `daw::util::{optimize_name, limit}` | partial | `formatColor` is `controller::electra::colors::format_color` |
| (Jackson ObjectMapper/JsonNode, JsonGenerator) | `json::{JsonValue, JsonWriter}` | partial | A minimal reader (parse, `get`, `elements`, `as_text`, `as_int`) and writer, no dependency |
| OperatingSystem | `controller::OperatingSystem` | full | |
| Timeout | `framework::commands::channel::Timeout` | full | |
| TimerCallback | `Ctx::schedule` (closure) | n/a | Replaced by closures |
| LatestTaskExecutor, LatestTaskException | - | n/a | Replaced by synchronous sending / `Ctx::schedule` |
| TimeoutOptimizer | - | n/a | Java hardware plumbing |
| FileEx | `controller::generic::mapping` (name helper only) | partial | Only `getNameWithoutType` |
| ConsoleLogger | `host::Host::log` | n/a | Replaced by host logging |
| Pair | Rust tuples | n/a | |
| IntConsumerSupplier | - | n/a | Replaced by closures (Java user: Kontrol Mk2) |
| ContinuousInfo | - | n/a | Not used by Java controllers |
| FrameworkException | - | n/a | Replaced by Rust error handling |
| TestFramework, TestCallback | - | n/a | Replaced by Rust integration tests (`App` + `RecordingBackend`) |
| NoteGenerator | - | missing | `generateEuclideanPattern`; Java user: Oxi One |

## Model: daw, parameter, parameter providers, scale

### framework/daw

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| IModel, AbstractModel; Bitwig/Reaper ModelImpl | `daw::model::Model` | full | No observers: drain `ModelEvent`s with `take_events`; call `run_due_tasks` regularly |
| ModelSetup | `daw::model_setup::ModelSetup` | full | Note clip sizes must be in the setup for Bitwig (BD §2 "Note clips") |
| DataSetup; Reaper DataSetupEx, BaseImpl | `daw::context::DawContext` | full | `HostFeature` switches Bitwig command forms (HB "Host features") |
| IApplication; ApplicationImpl | `daw::application::Application` | full | |
| IArranger, IMixer; ArrangerImpl, MixerImpl | `daw::application::{Arranger, Mixer}` | full | |
| IProject; ProjectImpl | `daw::project::Project` | full | Mute/solo-all are on `TrackBank` (`has_solo`…); cue volume/mix only Bitwig (`ParameterKind::ProjectCue`) |
| ITransport; TransportImpl | `daw::transport::Transport` | full | Crossfader and launcher overdub are on `Model`; Bitwig transport commands need `HostFeature::TransportCommands` |
| IGroove; GrooveImpl | `daw::groove::Groove` | full | Reaper: enabled + shuffle amount only; Bitwig: 6 params once reported (`/groove/<name>`) |
| IBrowser, AbstractBrowser; BrowserImpl, BrowserColumnImpl, BrowserColumnItemImpl | `daw::browser::{Browser, BrowserColumn, BrowserColumnItem}` | partial | `Model::browser()` is Option. Reaper: preset browsing only; Bitwig: full popup browser (BD §2) |
| IClipLauncherNavigator; ClipLauncherNavigatorImpl | `framework::clip_launcher_navigator::ClipLauncherNavigator` | full | Owned by the controller (`Rc<RefCell>`); Bitwig needs `HostFeature::ClipLauncherNavigator` |
| IHost; HostImpl | `host::Host` (+ `config` for capabilities) | partial | Logging, notifications, task scheduling only; no memory blocks, OSC/UDP or other host services |
| IMemoryBlock; MemoryBlockImpl | — | n/a | The Push frame is a `Vec<u8>` handed to `Backend::usb_bulk_out`; the Bitwig extension copies it into a memory block |
| DAWColor | `daw::color::{DawColor, ColorEx}` | full | |
| GrooveParameterID | `daw::constants::GrooveParameterId` | full | |

### framework/daw/clip

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| IClip, INoteClip; CursorClipImpl | `daw::clip::cursor_clip::CursorClip` | full | Get it with `Model::note_clip(cols, rows)`; Bitwig only has grids for setup-known sizes |
| IStepInfo, DefaultStepInfo, EmptyStepInfo; StepInfoImpl | `daw::clip::step_info::StepInfo` | full | |
| NotePosition, StepState, NoteOccurrenceType | `daw::clip::{NotePosition, StepState, NoteOccurrenceType}` | full | |
| ISessionAlternative | `daw::clip::session_alternative` | full | |
| Reaper Note | `daw::clip::note::{Note, parse_notes}` | full | Also used for the playing notes |

### framework/daw/constants

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| AutomationMode, LaunchQuantization, PostRecordingAction, RecordQuantization, Resolution | `daw::constants::*` | full | |
| Capability | `daw::constants::Capability` | full | |
| DeviceID | `daw::constants::DeviceId` | partial | `NiKomplete` is the first instrument (no KompleteDevice matcher) |
| TransportConstants | `daw::constants` (`MIN_TEMPO`, …) | full | |

### framework/daw/data

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| IItem, AbstractItemImpl; Reaper ItemImpl | `daw::bank::BankItem` (trait) | full | |
| IChannel; ChannelImpl | `daw::channel::Channel` (`ChannelKind` Track/Master/DrumPad/Layer/Empty) | full | |
| ITrack; TrackImpl | `daw::track::Track` (derefs to `Channel`) | partial | No track crossfade A/B parameter (`crossfade_parameter` None); hierarchy/group ops on `TrackBank` |
| IMasterTrack; MasterTrackImpl | `daw::track::Track` (master) | full | Reaper: crossfader = master user param 0; Bitwig: `HostCrossfade` once `/crossfade` reported |
| ICursorTrack; CursorTrackImpl | `daw::model::Model::cursor_track*` | full | Pinning: `Model::toggle_cursor_track_pinned`; Bitwig track remotes as `/track/fx/param` (BD §2) |
| ICursor, IPinnable | merged into `Model` / `SpecificDevice` | full | |
| IDevice; DeviceImpl | `daw::device::Device` (sibling bank items) | full | Always 8 siblings (BD §2) |
| ISpecificDevice, ICursorDevice, IDrumDevice, IEqualizerDevice; SpecificDeviceImpl, CursorDeviceImpl, DrumDeviceImpl, EqualizerDeviceImpl | `daw::device::SpecificDevice` + `DeviceKind` | partial | Reaper: always virtual pads, no layers; Bitwig: 128 pads/32 layers reported, no pad/layer device chains |
| Bitwig KompleteDevice | `DeviceId::NiKomplete` → first instrument | partial | No NI Komplete matcher |
| IDrumPad; DrumPadImpl | `daw::device::DrumPad` (derefs to `Channel`) | partial | Bitwig: pad sends and pad devices not reported |
| ILayer; LayerImpl, Bitwig AbstractDeviceChainImpl | `daw::channel::Channel` (`ChannelKind::Layer`) | partial | Reaper: none; Bitwig: reported, no layer sends or layer devices |
| ICursorLayer; CursorLayerImpl | — | missing | Needed by Kontrol; Bitwig cursor layer not reported (BD §2) |
| IDeviceMetadata; Bitwig DeviceMetadataImpl | `daw::browser::Browser::add_device(name, target)` | partial | Insert by name; no metadata objects (Push, OSC use them) |
| IParameterList; ParameterListImpl | — | missing | Use the flat `ParameterBank` instead |
| IMarker; MarkerImpl | `daw::marker::Marker` | full | Bitwig: bank of 100, core pages (BD §2) |
| IScene; SceneImpl | `daw::scene::Scene` | full | Reaper: scenes are regions |
| ISend; SendImpl | `daw::parameter::Send` | full | Bitwig: no indication/touch on sends (BD §2) |
| ISlot; SlotImpl | `daw::slot::Slot` | full | Reaper: clips are media items, states derived from transport |
| EqualizerBandType | `daw::device::EqualizerBandType` | full | |
| Bitwig MuteParameterImpl, SoloParameterImpl, CrossfadeParameter | — | missing | No mute/solo/crossfade-mode as `Parameter` |
| Bitwig FocusedParameterImpl | `framework::commands::general` (focused parameter) | full | |
| Bitwig MetronomeVolumeParameterImpl | `daw::transport::MetronomeVolumeParameter` | full | |
| Bitwig ParameterImpl, RangedValueImpl, RawParameterImpl, EqTypeParameter | `daw::parameter::Parameter` (`ParameterKind`) | full | `modulatedValue`/indication/touch only on cursor-device params on Bitwig (BD §2) |
| Bitwig Util | n/a | n/a | |

### framework/daw/data/bank

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| IBank, AbstractBank, AbstractItemBank; Bitwig AbstractItemBankImpl; Reaper AbstractBankImpl, AbstractPagedBankImpl | `daw::bank::PagedBank<T>` | full | Items by page index or absolute position; host paging on Bitwig (BD §2 "Bank paging", "Selection after a page change") |
| IChannelBank; AbstractChannelBankImpl | merged into `TrackBank` / `LayerBank` / `DrumPadBank` | full | |
| ITrackBank; AbstractTrackBankImpl, TrackBankImpl | `daw::track_bank::TrackBank` | partial | Core keeps all tracks by position. Bitwig: flat list only, depth 0 (BD §2) |
| EffectTrackBankImpl | `daw::model::Model::effect_track_bank()` (`TrackBank`) | partial | Option: None on Reaper; Bitwig `/fxtrack/…`. BD §2 row "Effect tracks" is outdated |
| ISceneBank; SceneBankImpl | `daw::scene::SceneBank` (owned by `TrackBank`) | partial | Bitwig: extra scene banks of other sizes only see the window |
| ISlotBank; SlotBankImpl | `daw::slot::SlotBank` | partial | Page follows the scene bank; no separate slot bank sizes |
| Reaper ResizedSlotBank | — | n/a | Reaper only |
| ISendBank; SendBankImpl | `daw::channel::SendBank` | full | |
| IDeviceBank; DeviceBankImpl | `daw::device::DeviceBank` | full | Fixed 8 siblings |
| IDrumPadBank; DrumPadBankImpl | `daw::device::DrumPadBank` | full | Bitwig: all 128 pads reported, core pages |
| ILayerBank; LayerBankImpl | `daw::device::LayerBank` | partial | Reaper: empty; Bitwig: 32 layers, core pages |
| IMarkerBank; MarkerBankImpl | `daw::marker::MarkerBank` | full | |
| IParameterBank; ParameterBankImpl; Reaper ParameterBankDelegator | `daw::parameter_bank::ParameterBank` | full | Bitwig: pages beyond `numParamPages` unreachable (BD §2) |
| IParameterPageBank; ParameterPageBankImpl | `daw::parameter_bank::ParameterPageBank` | full | |

### framework/daw/data/empty

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| EmptyBank, EmptyChannel, EmptyDevice, EmptyDeviceBank, EmptyDrumPad, EmptyItem, EmptyLayer, EmptyLayerBank, EmptyMarker, EmptyScene, EmptySend, EmptySendBank, EmptySlot, EmptySlotBank, EmptyTrack | `PagedBank` empty placeholder (`Channel::empty` etc.) | full | Out-of-range items report `does_exist() == false` |
| EmptyParameter, EmptyParameterBank, EmptyParameterPageBank | `ParameterKind::Empty`, `ParamRef::Empty` | full | |
| EmptyStepInfo | `daw::clip::step_info::StepInfo` | full | |

### framework/daw/midi

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| IMidiAccess, IMidiInput, MidiShortCallback, MidiSysExCallback | `midi::{MidiInput, MidiDispatch}` | full | Callbacks replaced by dispatch enum. `createInput (index > 0, ...)` of one port pair: `ControllerFactory::num_extra_midi_inputs` (Launchkey), their MIDI goes to `ControllerSetup::handle_extra_input_midi` |
| IMidiOutput, AbstractMidiOutput | `midi::MidiOutput` | partial | MPE config is in `controller::generic::setup::configure_mpe`. `createOutput (index > 0)` of one port pair: `ControllerFactory::num_extra_midi_outputs` and `create_with_extra_ports` (Electra One CTRL) |
| INoteInput, AbstractNoteInput | `midi::NoteInput` | full | Key/velocity translation tables; filters as hex masks |
| INoteRepeat | `midi::NoteRepeat` | partial | No octaves, latch, shuffle, free-running (BD §5) |
| ArpeggiatorMode | `config::*` (per-controller) | partial | Reaper mode list only |
| DeviceInquiry | `midi::DeviceInquiry` | full | `device_family_code` added for the Launchkey |
| MidiConstants | — | missing | Use literals |

### framework/daw/resource

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| ChannelType | `daw::constants::ChannelType` | full | |
| DeviceTypes, ResourceHandler | — | missing | No device-type/resource tables |

### framework/daw (Reaper-only implementation classes)

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| BrowserContentType | `daw::browser::BrowserContentType` | full | |
| NotificationWindow | `host::Host::show_notification` | partial | The host shows them |
| data/parameter: ParameterImpl, VolumeParameterImpl, PanningParameterImpl, MasterVolume/MasterPanningParameterImpl, GrooveParameter, UserParameterImpl, ParameterProxy, IParameterEx | `daw::parameter::{Parameter, ParameterKind}` | full | |
| TempoParameterImpl, MetronomeVolumeParameterImpl | `daw::transport::{TempoParameter, MetronomeVolumeParameter}` | full | |
| EqBandTypeParameterImpl, FocusedParameterImpl | `ParamRef::EqBandType`; `framework::commands::general` | full | |
| data/parameter/map: ParameterMap, ParameterMapPage, ParameterMapPageParameter, RenamedParameter | — | n/a | Reaper parameter maps; `refresh_parameter_mapping` is a no-op |
| MuteParameterImpl, SoloParameterImpl, CrossfadeParameter | — | missing | See data |

### framework/parameter

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| IParameter, AbstractParameterImpl | `daw::parameter::Parameter`; `ParamRef` | full | Providers return `ParamRef`, resolved when used |
| AbstractParameterWrapper, AbstractStaticParameter, StaticIntegerRangeParameter, StaticPercentageParameter | `framework::parameterprovider::CustomParameter` (trait) | partial | No generic static classes; OXI One uses them |
| IFocusedParameter | `framework::commands::general` | full | |
| TempoParameter | `parameterprovider::transport::TempoParameter` (a `CustomParameter`); the model's tempo is `daw::transport::TempoParameter` | full | No value (Java throws on `getValue`; 0 here) |
| AutomationModeParameter | `daw::transport::Transport` (automation-mode label/change) | full | Methods, not a parameter |
| NoteParameter, NoteAttribute | `framework::mode::note_parameter`; controller-local (e.g. `controller::launchcontrol::mode::note_parameter::XlNoteParameter`) | partial | Re-implemented per controller |
| ZoomParameter | `framework::commands::general` (zoom command) | partial | Command, not a parameter |
| PlayPositionParameter | `parameterprovider::transport::PlayPositionParameter` (a `CustomParameter`) | full | No value (Java throws on `getValue`; 0 here). The Maschine still has its own inline copy |
| LoopStartParameter | inline in `controller::maschine::core::encoder_mode_manager` | partial | No reusable parameter |
| IProject.getCueVolumeParameter, IGroove.getParameter | `parameterprovider::transport::{CueVolumeParameter, GrooveParameter}` | full | As `CustomParameter`s, since `ParamRef` has no variant for them |
| MuteParameter, SoloParameter | — | missing | Needed by Faderfox EC4 |
| ResetParameter | — | missing | |

### framework/parameterprovider

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| IParameterProvider, AbstractParameterProvider | `framework::parameterprovider::{ParameterProvider, SharedProvider, FnParameterProvider}` | full | No re-binding observers; `ParamRef` follows bank scrolling |

### framework/parameterprovider/device

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| BankParameterProvider | `framework::parameterprovider::device::BankParameterProvider` | full | |
| AbstractLayerOrDrumPadParameterProvider, Volume/Pan/SendLayerOrDrumPadParameterProvider | `framework::parameterprovider::device::LayerOrDrumPadParameterProvider` | partial | Always drum pads, never layers, also on Bitwig (BD §8 Layer Mix) |
| AbstractChannelParameterProvider, AbstractSelectedChannelParameterProvider | merged into the providers | full | |
| SelectedLayerOrDrumPadParameterProvider | controller-local (`controller::akai::fire::mode`, `controller::mackie::mcu::mode`) | partial | No shared implementation |
| SelectedLayerDeviceBankParameterProvider | — | missing | Needed by Kontrol; no layer devices |

### framework/parameterprovider/special

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| EmptyParameterProvider | `…::special::EmptyParameterProvider` | full | |
| NullParameterProvider | `EmptyParameterProvider` | partial | Use `EmptyParameterProvider::new(0)` (Yaeltex Turn) |
| FixedParameterProvider, CombinedParameterProvider, RangeFilterParameterProvider | `…::special::*` | full | |
| AbstractWrapperProvider | — | missing | |
| FourKnobProvider | controller-local `controller::akai::fire::mode` | partial | |
| ResetParameterProvider | — | missing | Needed by SL MkIII |

### framework/parameterprovider/track

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| AbstractTrackParameterProvider | merged into the providers | full | Always the current track bank (works with the effect bank via `Model::current_track_bank`) |
| VolumeParameterProvider, PanParameterProvider, SendParameterProvider, SelectedTrackParameterProvider | `framework::parameterprovider::track::*` | full | |
| MuteParameterProvider, SoloParameterProvider | — | missing | Needed by Faderfox EC4 |
| CrossfadeParameterProvider | — | missing | Needed by Push; no track crossfade parameter |

### framework/scale

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| Scale | `scale::scale::Scale` | full | |
| ScaleLayout | `scale::scale_layout::ScaleLayout` | full | |
| ScaleGrid | `scale::scale_grid::ScaleGrid` | full | |
| ChordGrid | `scale::chord_grid::ChordGrid` | full | |
| Scales | `scale::scales::Scales` | full | Note matrices are `[i32; 128]` |
| MPEStatus | `scale::mpe_status::MpeStatus` | full | |
| (framework.utils.KeyManager) | `scale::key_manager::KeyManager` | full | |

## Modes, views, graphics, OSC, USB

Rust paths in this part are relative to `src/framework/`.

### framework/mode

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| AbstractParameterMode / AbstractMode (in framework/featuregroup) | mode::parameter_mode::ParameterModeBase, ParameterBinding, ModeBank | full | ParamRefs are resolved when a value arrives, so there are no parameter observers or re-binding on scroll |
| AbstractNoteParameterMode | mode::note_parameter::{NoteAttribute, note_value, set_note_value, change_note_value} | partial | No base mode, only the functions. NoteAttribute has no occurrence/recurrence. Used by Fire and Oxi |
| DummyMode | mode::dummy::DummyMode | full | |
| MasterVolumeMode | controller::mackie::mcu::mode (local port) | partial | Not in the framework (the Electra One does not need it after all; only its focused parameter error text is shared) |
| Modes (IDs + helpers) | ids::Modes | partial | Only the enum. isTrackMode/isLayerMode/... and the names exist only in controllers, commands::mode::is_send_mode |
| INoteEditor, INoteEditorMode, NoteEditor | mode::note_editor::NoteEditor, View/Mode::note_editor(_mut) | full | Notes are keyed by ClipKey (cols, rows), not by the clip object |
| sequencer/AbstractSequencerMode | mode::sequencer::SequencerModeBase | full | Embed it and call on_activate yourself |
| device/BrowserMode | mode::browser::BrowserMode | partial | The model has no device database or filter column contents (see guide) |
| device/ParameterMode | mode::device::ParameterMode | full | |
| device/ProjectParamsMode | mode::device::ProjectParamsMode | full | |
| device/SelectedDeviceMode | mode::device::SelectedDeviceMode | full | |
| layer/DefaultLayerMode, LayerMuteMode, LayerSoloMode | mode::layer::{DefaultLayerMode<B: LayerModeBehavior>, LayerMuteMode, LayerSoloMode} | full | Works on the drum pads of the first instrument when it has any, otherwise on the layers |
| track/DefaultTrackMode | mode::track::DefaultTrackMode<B: TrackModeBehavior> | full | The bank is always the current track bank. The navigator is given with set_navigator |
| track/TrackSelect/Mute/Solo/RecArm/StopClip/Volume/Pan/Send/TrackMode | mode::track::{TrackSelectMode, ..., TrackSendMode, TrackMode} (type aliases) | full | new_with_controls binds providers. Without controls the knob functions act on the tracks directly |
| track/TrackCrossfadeAMode, TrackCrossfadeBMode | controller::akai::acvs::mode::{crossfade_a_mode, crossfade_b_mode} | partial | Only local to the ACVS. Reaper has no crossfade A/B |
| track/MasterAndFXVolumeMode | mode::master::MasterAndFXVolumeMode | partial | The FX volumes use empty parameters, although the model now has effect_track_bank |

### framework/view

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| AbstractView / AbstractFeatureGroup | view::base::{ViewBase, on_scene_button, activate_preferred_view} | full | The note mapping is updated immediately (Java delays it by 6 to 100 ms) |
| AbstractPlayView | view::play_view::{PlayViewBase, play_scale_matrix, ScaleMatrixFn} | full | The concrete view hands in the scale matrix (no getScaleMatrix override) |
| AbstractChordView, HeldChords | view::play_view::{ChordViewBase, HeldChords} | full | Same as DrivenByMoss. Reaper: there is no HeldChords |
| PianoViewHelper | view::play_view::draw_piano_grid | full | |
| AbstractDrum64View | view::drum64_view::Drum64ViewBase | full | |
| AbstractNumberDisplayView, TempoView, ShuffleView | view::number_display_view::{NumberDisplayViewBase, tempo_number, shuffle_number} | full | |
| AbstractSessionView | view::session_view::{SessionViewBase, SessionHooks, SessionColors} | full | The hooks are called while the state is borrowed, so implement them on a helper type |
| AbstractShiftView | view::shift_view::ShiftViewBase | full | |
| BrowserView | view::browser_view::BrowserViewBase | full | |
| ControlOnlyView | view::control_only_view::ControlOnlyView | full | |
| ScenePlayView | view::scene_play_view::ScenePlayView | full | |
| TransposeView | view::transpose_view::TransposeView (trait) | full | |
| ColorView, ColorSelectMode | view::color_view::ColorView, view::color_select_mode::ColorSelectMode | full | `View::set_color_select_mode` replaces the cast |
| DummyView | `framework::view::dummy_view::DummyView` | full | Used by the Launchkey and the Electra One |
| Views (IDs + helpers) | ids::Views | partial | Only the enum. isSequencerView/isSessionView/isNoteView and the names are missing |
| (none) | view::deferred::{activate_preferred_view_later, restore_view_later} | n/a | Rust only: a view switch requested by the view itself runs after the current event |

### framework/view/sequencer

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| AbstractSequencerView | view::sequencer::abstract_sequencer_view::{SequencerBase, activate_note_editor} | full | The note edit view is activated deferred, not immediately |
| AbstractNoteSequencerView | view::sequencer::abstract_note_sequencer_view::{NoteSequencerBase, NoteSequencerHooks} | full | |
| AbstractPolySequencerView | view::sequencer::abstract_poly_sequencer_view::{PolySequencerBase, PolySequencerHooks} | full | The combined step state is a flag in SequencerBase |
| AbstractRaindropsView | view::sequencer::abstract_raindrops_view::RaindropsBase | full | |
| AbstractDrumView, AbstractDrumExView | view::sequencer::abstract_drum_view::{DrumBase, DrumHooks, DrumExState} | full | Turn the Ex buttons on with with_extra_buttons. They are always handled, not registered on activation |
| AbstractDrumLaneView, AbstractDrum4View, AbstractDrum8View | view::sequencer::abstract_drum_lane_view::{DrumLaneBase, DrumLaneHooks} | full | Drum4 means 4 lanes x 16 steps, Drum8 means 8 lanes x 8 steps (constructors) |
| ClipLengthView | view::sequencer::clip_length_view::ClipLengthView | full | |
| AbstractDrumXoXView | view::sequencer::abstract_drum_xox_view::DrumXoXBase | full | Used by the Push; `adjustPage`/`blockSelectKnob` (Fire only) not ported |
| AbstractMonophonicSequencerView | - | missing | Needed by the Yaeltex Turn |

### framework/graphics

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| IBitmap | graphics::bitmap::Bitmap | full | RGB. encode_bgra, to_ascii_art (in place of the debug window) |
| IGraphicsContext | graphics::bitmap::GraphicsContext | full | Anti-aliasing (own rasterizer `graphics::raster`, exact coverage) when the configuration enables it, rounded/gradient rectangles, triangles, circles, icon masks; lines are 1 px, clipping on whole pixels |
| Align | graphics::bitmap::Align | full | |
| IGraphicsConfiguration, ChromaticGraphicsConfiguration | graphics::GraphicsConfiguration (::chromatic) | full | |
| IGraphicsDimensions, DefaultGraphicsDimensions | graphics::GraphicsDimensions | full | |
| IBounds, DefaultBounds | - (plain f64 params) | n/a | |
| IGraphicsInfo, DefaultGraphicsInfo | graphics::canvas::DrawInfo | full | |
| IImage, IRenderer, IEncoder | graphics::icons (+ generated icon_data.rs) | partial | The SVG icons are pre-rendered alpha masks (tools/gen_icons.py), tinted when drawn |
| (fonts) | graphics::font (5x8 bitmap), graphics::vector_font (DejaVu Sans outlines, generated font_data.rs) | full | The vector font is used with anti-aliasing (Push); no hinting or kerning |

### framework/graphics/display

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| ModelInfo | graphics::ModelInfo (private) | full | |
| (controller/display/AbstractGraphicDisplay) | graphics::GraphicDisplay | full | Renders synchronously and only on changes. The notification stays for NOTIFICATION_TIME ms |

### framework/graphics/canvas/component

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| IComponent | graphics::component::Component (enum) | partial | An enum, not a trait. PartialEq decides when to re-render |
| LabelComponent | Component::Label, Component::LabelWithIcon, LabelLayout | full | |
| ListComponent | Component::List | full | |
| ChannelComponent, ChannelSelectComponent, ParameterComponent, SendsComponent, OptionsComponent, MenuComponent | graphics::canvas::* (Component::{Channel, ChannelSelect, Parameter, Sends, Options, Menu}) | full | Java layout math |
| ClipListComponent, SceneListGridElement, MidiClipComponent, GraphOverlayComponent | graphics::canvas::* (Component::{ClipList, SceneList, MidiClip}, overlays) | full | Plain data instead of model objects, so they re-render only on changes |

### framework/graphics/canvas/component/simple

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| AbstractBaseComponent | (shared code in graphics::component) | full | |
| TitleValueComponent, TitleValueMenuComponent, TitleChannelsComponent | Component::{TitleValue, TitleValueMenu, TitleChannels} | full | These are the small-display components that the Fire and Oxi One use |

### framework/graphics/canvas/utils

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| SendData | graphics::canvas::SendData | full | |

### framework/osc

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| IOpenSoundControlMessage | `framework::osc::message::{OscMessage, OscArg, OscPacket}` | full | Own OSC 1.0 codec (Java uses the host's); bundles are flattened, time tags ignored |
| IOpenSoundControlServer, -Client; Bitwig OpenSoundControlServer/ClientImpl | `framework::osc::udp::{OscServer, OscClient}` | full | `std::net::UdpSocket`, non-blocking, polled at the tick of the core; the bundles of a large update are sent one per tick (Java: 10 ms sleep) |
| IOpenSoundControlCallback | `controller::osc::setup::OscSetup::handle_message` | full | Called for each received message at the tick |
| IOpenSoundControlWriter, AbstractOpenSoundControlWriter | `framework::osc::writer::OscWriter` | full | Caches the sent value (not the Java object); typed keys (`Key<T>`, `osc_keys!` groups, `KeyList`) intern the addresses once, the values are compared by key |
| AbstractOpenSoundControlConfiguration | `config::osc` (logging settings) | full | |
| AbstractOpenSoundControlParser | `controller::osc::setup` | full | |

The `osc` in src/parser.rs is unrelated: it is Reaper's own path protocol.

### framework/usb

| Java | Rust | Status | Divergences |
| --- | --- | --- | --- |
| IUsbDevice, IUsbEndpoint, IUsbCallback, UsbException | `Backend::usb_bulk_out` / `Sender::usb_bulk_out` | partial | Bulk OUT only: the core hands the bytes to the host (C ABI `dbm_set_usb_callback`; Bitwig USB API, Python usbdevfs or the simulator socket, see HOST_BRIDGE_GUIDE §5a). No IN endpoints |
| IHidDevice, IHidCallback | - | missing | Reaper: reaper/framework/usb/HidDeviceImpl. Needed by Kontrol Mk1 |
| UsbMatcher | controller::UsbMatcher, `ControllerFactory::usb_devices` | full | Exposed by `dbm_list_usb_devices` and `dbm-catalog --usb` (Bitwig definitions) |
