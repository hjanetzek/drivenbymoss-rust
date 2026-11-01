// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The setup of the Maschine Mk2/Mk3/Mikro Mk3/Plus/Studio (former `MaschineControllerSetup`).
//!
//! Differences to Java:
//! - The observers are replaced by [`ControllerSetup::handle_changes`], which drains the model
//!   events and the changed settings.
//! - `layoutControls` (the bounds of the simulator GUI) is not ported.
//! - The monitor encoder section of the Maschine Studio (`StudioEncoderModeManager`, the
//!   monitor buttons and the VU of the monitor encoder) is not ported yet.
//! - Changing the knob sensitivity of the value changer (former
//!   `updateRelativeKnobSensitivity`) is not supported by the Rust model.

use std::rc::Rc;

use super::command::continuous::{MainKnobRowModeCommand, RibbonCommand, TouchstripCommand};
use super::command::trigger::{
    drum_view_state, is_grid_editor, toggle_grid_editor, with_drum_view, AddDeviceCommand, KeyboardCommand, MaschineCursorCommand, MaschineEncoderPressCommand,
    MaschineMk2PlayViewSwitchCommand, MaschineSelectButtonCommand, MaschineSendSelectCommand, MaschineStopCommand, MaschineTapMetroCommand,
    MaschineToggleShiftViewCommand, PadModeCommand, PageCommand, ProjectButtonCommand, SwingCommand, TempoCommand, ToggleFixedVelCommand,
    VolumePanSendCommand,
};
use super::definition::MaschineDefinition;
use super::mode::browse::BrowseMode;
use super::mode::drum_configuration::DrumConfigurationMode;
use super::mode::edit_note::EditNoteMode;
use super::mode::note_repeat::NoteRepeatMode;
use super::mode::parameters_mode::MaschineParametersMode;
use super::mode::play_configuration::PlayConfigurationMode;
use super::mode::track_modes::{MaschinePanMode, MaschineSendMode, MaschineVolumeMode};
use super::mode::transport_modes::{TransportMode, TransportModeKind};
use super::mode::user_mode::MaschineUserMode;
use super::surface::{self as mk, enable_shift, ext, ext_mut, has_shift, install_trigger_sender, maschine, set_stop_consumed, MaschineExt};
use super::view::clip_view::ClipView;
use super::view::drum_view::DrumView;
use super::view::mute_view::MuteView;
use super::view::note_repeat_view::NoteRepeatView;
use super::view::parameter_view::ParameterView;
use super::view::play_view::PlayView;
use super::view::scene_view::SceneView;
use super::view::select_view::SelectView;
use super::view::shift_view::ShiftView;
use super::view::solo_view::SoloView;
use super::view::ExecuteFunction;
use crate::backend::Sender;
use crate::config::maschine::MaschineConfiguration;
use crate::config::values::REAPER_ARP_MODES;
use crate::config::{Configuration, SettingId};
use crate::controller::maschine::core::colors::*;
use crate::controller::maschine::core::command::group_button::GroupButtonCommand;
use crate::controller::maschine::core::command::note_repeat::FillModeNoteRepeatCommand;
use crate::controller::maschine::core::maschine::Maschine;
use crate::controller::maschine::core::ribbon_mode::RibbonMode;
use crate::controller::maschine::core::surface::{create_pad_grid, flush_pad_grid, handle_sysex, shift_button};
use crate::controller::{ControllerSetup, InstanceParts, MidiPorts};
use crate::daw::events::ModelEvent;
use crate::daw::model::Model;
use crate::daw::model_setup::ModelSetup;
use crate::framework::color::{BUTTON_STATE_HI, BUTTON_STATE_ON};
use crate::framework::command::{shared, ContinuousCommand, FnTrigger, SharedTrigger, TriggerCommand};
use crate::framework::commands::clip::NewCommand;
use crate::framework::commands::cursor::{CursorCommand, Direction};
use crate::framework::commands::misc::{AddTrackCommand, BrowserCommand, ConvertCommand, FootswitchCommand, LayoutCommand, OverdubCommand, PaneCommand, PanelLayoutCommand, Panels};
use crate::framework::commands::mode::{is_send_mode, KnobRowModeCommand, ModeSelectCommand};
use crate::framework::commands::transport::{ConfiguredRecordCommand, MetronomeCommand, PlayCommand};
use crate::framework::commands::transport_ext::{AutomationCommand, ToggleLoopCommand};
use crate::framework::commands::view::{install_shift_tap, ViewMultiSelectCommand};
use crate::framework::ctx::Ctx;
use crate::framework::display::{McuDisplay, McuDisplayOptions};
use crate::framework::ids::{ButtonId, ContinuousId, Modes, Views};
use crate::framework::surface::{ContinuousKind, Surface};
use crate::framework::value_changer::ValueChanger;
use crate::host::Host;
use crate::midi::{to_hex_str, BindType, ButtonEvent, MidiInput, MidiOutput};
use crate::scale::Scales;

/// The drum grid matrix.
#[rustfmt::skip]
const DRUM_MATRIX: [i32; 64] = [
     0,  1,  2,  3,  4,  5,  6,  7,
     8,  9, 10, 11, 12, 13, 14, 15,
    -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1,
];

const RIBBON_MODES_PITCH: [RibbonMode; 3] = [RibbonMode::PitchDown, RibbonMode::PitchDownUp, RibbonMode::PitchUp];
const RIBBON_MODES_CC: [RibbonMode; 2] = [RibbonMode::Cc1, RibbonMode::Cc11];
const RIBBON_MODES_MASTER_VOLUME: [RibbonMode; 1] = [RibbonMode::MasterVolume];
const RIBBON_MODES_NOTE_REPEAT: [RibbonMode; 2] = [RibbonMode::NoteRepeatPeriod, RibbonMode::NoteRepeatLength];

/// The track modes (former `Modes.isTrackMode`).
const TRACK_MODES: [Modes; 15] = [
    Modes::Track,
    Modes::Volume,
    Modes::Pan,
    Modes::Crossfader,
    Modes::Send,
    Modes::Send1,
    Modes::Send2,
    Modes::Send3,
    Modes::Send4,
    Modes::Send5,
    Modes::Send6,
    Modes::Send7,
    Modes::Send8,
    Modes::TrackDetails,
    Modes::Master,
];

/// Is the mode a track mode (former `Modes.isTrackMode`)?
pub fn is_track_mode(mode: Option<Modes>) -> bool {
    mode.is_some_and(|mode| TRACK_MODES.contains(&mode))
}

/// Create the configuration with all settings of the Maschine model.
pub fn create_configuration(maschine: Maschine) -> Configuration {
    MaschineConfiguration::for_reaper(maschine).into_base()
}

/// Create a Maschine instance.
pub fn create(definition: MaschineDefinition, host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
    let maschine = definition.maschine;
    let value_changer = ValueChanger::twos_complement(128, 1);
    let colors = create_color_manager();

    // createScales
    let mut scales = Scales::new(&value_changer, 36, 52, 4, 4);
    scales.set_drum_matrix(DRUM_MATRIX.to_vec());

    // createModel
    let mut model_setup = ModelSetup::new();
    model_setup.set_has_full_flat_track_list(true);
    model_setup.wants_focused_parameter = true;
    model_setup.set_num_tracks(if maschine.has_group_buttons() { 8 } else { 16 });
    model_setup.set_num_devices_in_bank(16);
    model_setup.set_num_scenes(16);
    let mut model = Model::new(host.clone(), sender.clone(), model_setup, value_changer, scales);
    model.track_bank_mut().set_indication(true);

    // createSurface
    let output = MidiOutput::new(sender.clone(), Some(ports.output));
    let input = MidiInput::new(sender.clone(), Some(ports.input), &["80????", "90????"])?;
    let display_output = output.clone();
    let mut surface = Surface::new(output, input, Some(create_pad_grid(&colors, 4, 4)));
    surface.shift_button = shift_button(maschine, maschine.has_shift());

    let mut ctx = Ctx::new(host, sender, model, configuration, surface, colors);
    ctx.ext = Box::new(MaschineExt::new(maschine));
    if maschine.has_mcu_display() {
        ctx.text_display = Some(McuDisplay::create(display_output, true, false, false, McuDisplayOptions::default()));
        // Do not notify on display!
        ctx.notify_view_change = false;
    }
    ctx.sysex_handler = Some(Rc::new(move |ctx: &mut Ctx, data: &str| handle_maschine_sysex(ctx, maschine, data)));
    install_trigger_sender(&mut ctx);
    ctx.create_pads();

    Ok(InstanceParts { ctx, setup: Box::new(MaschineSetup::new(maschine)) })
}

/// The Maschine controller setup.
pub struct MaschineSetup {
    maschine: Maschine,
    /// The last seen active state of the browser (replaces the browser observer).
    browser_active: bool,
}

impl MaschineSetup {
    pub fn new(maschine: Maschine) -> Self {
        MaschineSetup { maschine, browser_active: false }
    }
}

impl ControllerSetup for MaschineSetup {
    fn init(&mut self, ctx: &mut Ctx) {
        create_modes(ctx, self.maschine);
        create_views(ctx, self.maschine);
        register_trigger_commands(ctx, self.maschine);
        register_continuous_commands(ctx, self.maschine);
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        ctx.set_active_mode(Modes::Volume);
        let view = ctx.config.startup_view();
        ctx.set_active_view(view);
        // Ask dbm-nihost whether it sends the Shift button (not in the Java version)
        if self.maschine == Maschine::Mk2 {
            ctx.surface.output.send_sysex(&HOST_QUERY);
        }
    }

    fn flush(&mut self, ctx: &mut Ctx) {
        ctx.flush_surface();
        flush_pad_grid(ctx);
        TouchstripCommand::update_value(ctx);
        flush_button_labels(ctx);
    }

    fn handle_changes(&mut self, ctx: &mut Ctx) {
        // Model events (former track bank selection observer and the observers of the views)
        let events = ctx.model.take_events();
        ctx.dispatch_model_events_to_views(&events);
        for event in &events {
            if let ModelEvent::TrackSelectionChanged { is_selected, .. } = event {
                handle_track_change(ctx, *is_selected);
            }
        }

        // Setting observers
        let changed = ctx.config.take_changed_settings();
        for id in &changed {
            handle_setting_change(ctx, *id);
        }
        ctx.dispatch_setting_changes_to_views(&changed);

        // Browser observer (former activateBrowserObserver (Modes.BROWSER))
        let browser_active = ctx.model.browser().is_some_and(|browser| browser.is_active());
        if browser_active != self.browser_active {
            self.browser_active = browser_active;
            if browser_active {
                ctx.set_temporary_mode(Modes::Browser);
            } else if ctx.modes.is_active(Modes::Browser) {
                ctx.restore_mode();
            }
        }
    }

    fn exit(&mut self, ctx: &mut Ctx) {
        // Former surface.shutdown: turn off all LEDs and clear the display
        if let Some(grid) = &mut ctx.surface.pad_grid {
            grid.turn_off();
        }
        ctx.turn_off_lights();
        flush_pad_grid(ctx);
        if let Some(display) = &mut ctx.text_display {
            display.shutdown();
        }
    }
}

/// The system exclusive message with the labels of the 8 buttons above the display (not in the
/// Java version): `F0 7D 44 42 4D 10 00 <56 characters> F7`, 8 cells of 7 characters as the MCU
/// display. Only dbm-nihost shows it (on the Mk2 displays, above the MCU lines); NI's driver
/// ignores it.
pub const BUTTON_LABELS_HEADER: [u8; 7] = [0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x10, 0x00];

/// The query of the core to dbm-nihost (not in the Java version): `F0 7D 44 42 4D 11 F7`. dbm-nihost
/// answers with [`HOST_SENDS_SHIFT`]; NI's driver ignores it, then Shift stays emulated with the
/// Stop button (STEP LEFT).
pub const HOST_QUERY: [u8; 7] = [0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x11, 0xF7];

/// The answer to [`HOST_QUERY`], also sent by dbm-nihost when the Mk2 is attached or the DAW is
/// connected: `F0 7D 44 42 4D 11 01 F7`, dbm-nihost sends the Shift button of the Mk2.
pub const HOST_SENDS_SHIFT: [u8; 8] = [0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x11, 0x01, 0xF7];

/// The Shift button of the Mk2 from dbm-nihost: `F0 7D 44 42 4D 12 <01 down, 00 up> F7`.
pub const HOST_SHIFT_HEADER: [u8; 6] = [0xF0, 0x7D, 0x44, 0x42, 0x4D, 0x12];

/// Handle the system exclusive messages of dbm-nihost (Mk2 only, not in the Java version) and
/// otherwise the ones of NI's driver.
fn handle_maschine_sysex(ctx: &mut Ctx, maschine: Maschine, data: &str) {
    if maschine == Maschine::Mk2 && handle_host_sysex(ctx, data) {
        return;
    }
    handle_sysex(ctx, maschine, data);
}

/// Handle the system exclusive messages of dbm-nihost; true if it was one.
fn handle_host_sysex(ctx: &mut Ctx, data: &str) -> bool {
    if data.eq_ignore_ascii_case(&to_hex_str(&HOST_SENDS_SHIFT)) {
        if !has_shift(ctx) {
            ctx.host.println("dbm-nihost sends the Shift button, STEP LEFT is Stop");
            enable_shift(ctx);
        }
        return true;
    }
    let header = to_hex_str(&HOST_SHIFT_HEADER);
    if !data.get(..header.len()).is_some_and(|start| start.eq_ignore_ascii_case(&header)) {
        return false;
    }
    // Shift stays emulated with STEP LEFT until dbm-nihost announced that it sends Shift
    if has_shift(ctx) {
        match &data[header.len()..] {
            "01F7" => ctx.trigger_button(ButtonId::Shift, ButtonEvent::Down, 1.0),
            "00F7" => ctx.trigger_button(ButtonId::Shift, ButtonEvent::Up, 0.0),
            _ => {}
        }
    }
    true
}

/// The labels of the buttons above the display of the Mk2: the active mode is marked with '>',
/// Pin and Active show whether they act on the device or the track.
pub fn button_labels(ctx: &Ctx) -> String {
    let active = ctx.modes.active_id();
    let is_send = active.is_some_and(is_send_mode);
    let is_device = active == Some(Modes::DeviceParams);
    let cells = [
        (active == Some(Modes::Volume), "Volume"),
        (active == Some(Modes::Pan), "Pan"),
        (is_send, "Send-"),
        (is_send, "Send+"),
        (false, if is_device { "PinDev" } else { "PinTrk" }),
        (false, if is_device { "DevOn" } else { "TrkOn" }),
        (active == Some(Modes::User), "User"),
        (is_device, "Params"),
    ];
    cells.iter().map(|(marked, label)| format!("{}{:<6}", if *marked { '>' } else { ' ' }, label)).collect()
}

/// Send the labels of the buttons above the display if they changed (Mk2 only, see
/// [`BUTTON_LABELS_HEADER`]).
fn flush_button_labels(ctx: &mut Ctx) {
    if maschine(ctx) != Maschine::Mk2 {
        return;
    }
    let labels = button_labels(ctx);
    if ext(ctx).button_labels == labels {
        return;
    }
    let mut data = BUTTON_LABELS_HEADER.to_vec();
    data.extend(labels.bytes().map(|b| if b.is_ascii() { b } else { b'?' }));
    data.push(0xF7);
    ctx.surface.output.send_sysex(&data);
    ext_mut(ctx).button_labels = labels;
}

// ------------------------------------------------------------------------------------------
// Modes and views

/// Register the modes (former `createModes`).
pub fn create_modes(ctx: &mut Ctx, maschine: Maschine) {
    let has_display = maschine.has_mcu_display();

    let mode = BrowseMode::new(ctx);
    ctx.register_mode(Modes::Browser, mode);

    let mode = MaschineVolumeMode::new(ctx, has_display);
    ctx.register_mode(Modes::Volume, mode);
    let mode = MaschinePanMode::new(ctx, has_display);
    ctx.register_mode(Modes::Pan, mode);
    for i in 0..8 {
        let mode = MaschineSendMode::new(i, ctx, has_display);
        ctx.register_mode(Modes::get(Modes::Send1, i), mode);
    }

    let kinds = [
        (Modes::Tempo, TransportModeKind::Tempo),
        (Modes::Position, TransportModeKind::Position),
        (Modes::LoopStart, TransportModeKind::LoopStart),
        (Modes::LoopLength, TransportModeKind::LoopLength),
    ];
    for (id, kind) in kinds {
        let mode = TransportMode::new(ctx, kind);
        ctx.register_mode(id, mode);
    }

    let mode = NoteRepeatMode::new(ctx);
    ctx.register_mode(Modes::RepeatNote, mode);
    let mode = PlayConfigurationMode::new(ctx);
    ctx.register_mode(Modes::Scales, mode);
    let mode = DrumConfigurationMode::new(ctx);
    ctx.register_mode(Modes::PlayOptions, mode);
    let mode = EditNoteMode::new(ctx);
    ctx.register_mode(Modes::Note, mode);

    let mode = MaschineParametersMode::new(ctx, has_display);
    ctx.register_mode(Modes::DeviceParams, mode);
    if has_display {
        let mode = MaschineUserMode::new(ctx);
        ctx.register_mode(Modes::User, mode);
    }

    ctx.modes.set_default_id(Modes::Volume);
}

/// Register the views (former `createViews`).
pub fn create_views(ctx: &mut Ctx, maschine: Maschine) {
    let view = SceneView::new(ctx);
    ctx.register_view(Views::ScenePlay, view);
    let view = ClipView::new(ctx);
    ctx.register_view(Views::Session, view);

    let drum_view = Rc::new(std::cell::RefCell::new(DrumView::new(ctx)));
    ctx.views.register(Views::Drum, drum_view.clone());
    let view = PlayView::new(ctx, drum_view);
    ctx.register_view(Views::Play, view);

    let view = ParameterView::new(ctx);
    ctx.register_view(Views::Device, view);

    if !maschine.has_group_buttons() {
        let view = SelectView::new(ctx);
        ctx.register_view(Views::TrackSelect, view);
        let view = SoloView::new(ctx);
        ctx.register_view(Views::TrackSolo, view);
        let view = MuteView::new(ctx);
        ctx.register_view(Views::TrackMute, view);
    }

    let view = NoteRepeatView::new(ctx);
    ctx.register_view(Views::RepeatNote, view);

    let view = ShiftView::new(ctx);
    ctx.register_view(Views::Shift, view);
}

// ------------------------------------------------------------------------------------------
// Observers

/// React on a changed setting (former setting observers).
fn handle_setting_change(ctx: &mut Ctx, id: SettingId) {
    match id {
        // If fixed velocity is active forward it to the note repeat as well
        // (NOTE_REPEAT_USE_PRESSURE_TO_VELOCITY is supported by Reaper)
        SettingId::ACTIVATE_FIXED_ACCENT => {
            let use_pressure = !ctx.config.is_accent_active();
            ctx.surface.input.default_note_input_mut().note_repeat.set_use_pressure(use_pressure);
        }

        // Former registerDeactivatedItemsHandler. Only the track bank supports it in the model.
        SettingId::EXCLUDE_DEACTIVATED_ITEMS => {
            let exclude = ctx.config.are_deactivated_items_excluded();
            ctx.model.track_bank_mut().set_skip_disabled_items(exclude);
        }

        // Former createScaleObservers
        SettingId::SCALES_SCALE => {
            let name = ctx.config.scale().to_string();
            ctx.model.scales_mut().set_scale_by_name(&name);
            ctx.update_view_note_mapping();
        }
        SettingId::SCALES_BASE => {
            let name = ctx.config.scale_base().to_string();
            ctx.model.scales_mut().set_scale_offset_by_name(&name);
            ctx.update_view_note_mapping();
        }
        SettingId::SCALES_IN_KEY => {
            let chromatic = !ctx.config.is_scale_in_key();
            ctx.model.scales_mut().set_chromatic(chromatic);
            ctx.update_view_note_mapping();
        }
        SettingId::SCALES_LAYOUT => {
            let name = ctx.config.scale_layout().to_string();
            ctx.model.scales_mut().set_scale_layout_by_name(&name);
            ctx.update_view_note_mapping();
        }

        // Former createNoteRepeatObservers (Reaper supports length and mode but not octaves)
        SettingId::NOTEREPEAT_ACTIVE => {
            let active = ctx.config.is_note_repeat_active();
            ctx.surface.input.default_note_input_mut().note_repeat.set_active(active);
        }
        SettingId::NOTEREPEAT_PERIOD => {
            let period = ctx.config.note_repeat_period().value();
            ctx.surface.input.default_note_input_mut().note_repeat.set_period(period);
        }
        SettingId::NOTEREPEAT_LENGTH => {
            let length = ctx.config.note_repeat_length().value();
            ctx.surface.input.default_note_input_mut().note_repeat.set_note_length(length);
        }
        SettingId::NOTEREPEAT_MODE => {
            let mode = ctx.config.note_repeat_mode().and_then(|mode| REAPER_ARP_MODES.iter().position(|m| *m == mode));
            if let Some(index) = mode {
                ctx.surface.input.default_note_input_mut().note_repeat.set_mode(index);
            }
        }

        _ => {}
    }
}

/// Handle a track selection change (former `handleTrackChange`).
pub fn handle_track_change(ctx: &mut Ctx, is_selected: bool) {
    if !is_selected {
        return;
    }

    // Former recallLastView
    if !ctx.views.is_any_active(&[Views::Session, Views::Mix, Views::Project]) {
        ctx.recall_preferred_view();
    }

    // Former resetDrumOctave (there is no 64 pad drum view)
    if ctx.views.is_any_active(&[Views::Drum, Views::Play]) {
        ctx.update_view_note_mapping();
    }

    // Former updateMode
    if ctx.modes.is_active(Modes::Master) && !ctx.model.master_track().is_selected() {
        if is_track_mode(ctx.modes.previous_id()) {
            ctx.restore_mode();
        } else {
            ctx.set_active_mode(Modes::Track);
        }
    }
}

// ------------------------------------------------------------------------------------------
// Buttons

type Supplier = Rc<dyn Fn(&Ctx) -> i32>;

/// Map a boolean state to the color of the button (former `addButton` with a boolean supplier
/// and the colors BUTTON_STATE_ON / BUTTON_STATE_HI).
fn on_hi(ctx: &Ctx, state: bool) -> i32 {
    ctx.colors.color_index(if state { BUTTON_STATE_HI } else { BUTTON_STATE_ON })
}

/// A light supplier from a boolean state.
fn lit(f: impl Fn(&Ctx) -> bool + 'static) -> Option<Supplier> {
    Some(Rc::new(move |ctx: &Ctx| on_hi(ctx, f(ctx))))
}

/// Get the default bind type for triggering buttons (former `getTriggerBindType`).
pub fn trigger_bind_type(button: ButtonId) -> BindType {
    if button == ButtonId::Row27 {
        BindType::Note
    } else {
        BindType::Cc
    }
}

/// Add a button which is bound to a MIDI CC (or note) on channel 1 (former `addButton`). The
/// default light shows the pressed state.
fn add_button(ctx: &mut Ctx, id: ButtonId, label: &str, command: SharedTrigger, midi_control: i32, supplier: Option<Supplier>) {
    let supplier = supplier.or_else(|| lit(move |ctx: &Ctx| ctx.surface.is_pressed(id)));
    ctx.add_button(id, label, Some(command), trigger_bind_type(id), 0, midi_control, supplier);
}

/// A light supplier which returns the color index directly.
fn color(f: impl Fn(&Ctx) -> i32 + 'static) -> Option<Supplier> {
    Some(Rc::new(f))
}

/// A command which does nothing (former `NopCommand`).
fn nop() -> SharedTrigger {
    shared(FnTrigger(|_ctx: &mut Ctx, _event, _velocity| {}))
}

/// Register the commands of all buttons (former `registerTriggerCommands`).
pub fn register_trigger_commands(ctx: &mut Ctx, maschine: Maschine) {
    let is_studio_or_mk2 = maschine == Maschine::Studio || maschine == Maschine::Mk2;
    let has_display = maschine.has_mcu_display();

    add_button(
        ctx,
        ButtonId::Shift,
        "SHIFT",
        shared(MaschineToggleShiftViewCommand::new()),
        if maschine == Maschine::Studio { mk::NAVIGATE_BACK } else { -1 },
        lit(|ctx: &Ctx| ctx.views.is_active(Views::Shift) || ctx.surface.is_shift_pressed()),
    );
    install_shift_tap(ctx);

    // Transport
    add_button(ctx, ButtonId::Play, "PLAY", shared(PlayCommand::new()), mk::PLAY, lit(|ctx: &Ctx| ctx.model.transport().is_playing()));
    add_button(
        ctx,
        ButtonId::Record,
        "RECORD",
        shared(ConfiguredRecordCommand::new()),
        mk::REC,
        lit(|ctx: &Ctx| ConfiguredRecordCommand::is_lit(ctx, ctx.surface.is_shift_pressed())),
    );
    add_button(ctx, ButtonId::Stop, "STOP", shared(MaschineStopCommand::default()), mk::STOP, lit(|ctx: &Ctx| !ctx.model.transport().is_playing()));
    add_button(ctx, ButtonId::Loop, "LOOP", shared(ToggleLoopCommand::new()), mk::RESTART, lit(|ctx: &Ctx| ctx.model.transport().is_loop()));
    add_button(ctx, ButtonId::Delete, "ERASE", nop(), mk::ERASE, None);
    add_button(
        ctx,
        ButtonId::TapTempo,
        "TAP/METRO",
        shared(MaschineTapMetroCommand::new()),
        mk::TAP_METRO,
        lit(|ctx: &Ctx| ctx.model.transport().is_metronome_on()),
    );

    add_button(
        ctx,
        ButtonId::Flip,
        if is_studio_or_mk2 { "GRID" } else { "FOLLOW" },
        shared(FnTrigger(|ctx: &mut Ctx, event, _velocity| toggle_grid_editor(ctx, event))),
        mk::FOLLOW,
        lit(is_grid_editor),
    );

    add_button(ctx, ButtonId::New, if maschine.has_cursor_keys() { "MACRO" } else { "GROUP" }, shared(NewCommand::new()), mk::GROUP, None);

    let mut automation_command = AutomationCommand::new();
    add_button(
        ctx,
        ButtonId::Automation,
        "AUTO",
        shared(FnTrigger(move |ctx: &mut Ctx, event, velocity| {
            let cleanup_shift = event == ButtonEvent::Down && !ctx.surface.is_select_pressed() && ctx.surface.is_shift_pressed();
            automation_command.execute(ctx, event, velocity);
            // Former cleanupShift
            if cleanup_shift {
                set_stop_consumed(ctx);
            }
        })),
        mk::AUTO,
        lit(AutomationCommand::is_lit),
    );

    let mut overdub_command = OverdubCommand::new();
    add_button(
        ctx,
        ButtonId::Overdub,
        if is_studio_or_mk2 { "ENTER" } else { "LOCK" },
        shared(FnTrigger(move |ctx: &mut Ctx, event, velocity| {
            // Former shiftedFunction
            if event == ButtonEvent::Up && ctx.surface.is_shift_pressed() {
                set_stop_consumed(ctx);
            }
            overdub_command.execute(ctx, event, velocity);
        })),
        mk::LOCK,
        lit(|ctx: &Ctx| if ctx.surface.is_shift_pressed() { ctx.model.is_launcher_overdub() } else { ctx.model.is_arranger_overdub() }),
    );
    add_button(
        ctx,
        ButtonId::Repeat,
        "NOTE REPEAT",
        shared(FillModeNoteRepeatCommand::new(has_display)),
        mk::NOTE_REPEAT,
        lit(|ctx: &Ctx| ctx.config.is_note_repeat_active()),
    );

    // Ribbon
    let ribbons: [(ButtonId, &str, &'static [RibbonMode], i32); 4] = [
        (ButtonId::F1, "PITCH", &RIBBON_MODES_PITCH, mk::PITCH),
        (ButtonId::F2, "MOD", &RIBBON_MODES_CC, mk::MOD),
        (ButtonId::F3, "PERFORM", &RIBBON_MODES_MASTER_VOLUME, mk::PERFORM),
        (ButtonId::F4, "NOTES", &RIBBON_MODES_NOTE_REPEAT, mk::NOTES),
    ];
    for (id, label, modes, control) in ribbons {
        add_button(ctx, id, label, shared(RibbonCommand::new(modes)), control, lit(move |ctx: &Ctx| RibbonCommand::is_ribbon_mode(ctx, modes)));
    }
    add_button(ctx, ButtonId::FaderTouch1, "ENCODER PRESS", shared(MaschineEncoderPressCommand), mk::ENCODER_PUSH, None);

    // Encoder Modes
    add_button(ctx, ButtonId::Volume, "VOLUME", shared(VolumePanSendCommand::default()), mk::VOLUME, lit(|ctx: &Ctx| is_track_mode(ctx.modes.active_id())));
    add_button(
        ctx,
        ButtonId::Swing,
        "SWING",
        shared(SwingCommand::default()),
        mk::SWING,
        lit(|ctx: &Ctx| ctx.modes.is_any_active(&[Modes::Position, Modes::LoopStart, Modes::LoopLength])),
    );
    add_button(ctx, ButtonId::TempoTouch, "TEMPO", shared(TempoCommand::default()), mk::TEMPO, lit(|ctx: &Ctx| ctx.modes.is_active(Modes::Tempo)));

    let mut panel_layout_command = PanelLayoutCommand::new();
    let mut device_mode_command = ModeSelectCommand::new(Modes::DeviceParams);
    add_button(
        ctx,
        ButtonId::Device,
        "PLUG-IN",
        shared(FnTrigger(move |ctx: &mut Ctx, event, velocity| {
            if has_display || ctx.surface.is_shift_pressed() {
                panel_layout_command.execute_normal(ctx, event);
            } else {
                device_mode_command.execute(ctx, event, velocity);
            }
        })),
        mk::PLUGIN,
        lit(move |ctx: &Ctx| {
            if has_display || ctx.surface.is_shift_pressed() {
                ctx.model.cursor_device().is_window_open()
            } else {
                ctx.modes.is_active(Modes::DeviceParams)
            }
        }),
    );

    let mut convert_command = ConvertCommand::new();
    add_button(
        ctx,
        ButtonId::DeviceOnOff,
        "SAMPLING",
        shared(FnTrigger(move |ctx: &mut Ctx, event, velocity| {
            // Former sliceToSampler
            if event == ButtonEvent::Down && ctx.surface.is_shift_pressed() && ctx.model.can_convert_clip() {
                set_stop_consumed(ctx);
            }
            convert_command.execute(ctx, event, velocity);
        })),
        mk::SAMPLING,
        None,
    );

    // Browser
    let file_label = if maschine.has_cursor_keys() {
        if is_studio_or_mk2 {
            "ALL/SAVE"
        } else {
            "FILE"
        }
    } else {
        "PROJECT"
    };
    add_button(ctx, ButtonId::AddTrack, file_label, shared(ProjectButtonCommand::default()), mk::PROJECT, None);
    add_button(ctx, ButtonId::AddEffect, if maschine.has_cursor_keys() { "SETTINGS" } else { "FAVORITES" }, shared(AddDeviceCommand), mk::FAVORITES, None);
    add_button(
        ctx,
        ButtonId::Browse,
        if is_studio_or_mk2 { "BROWSE" } else { "BROWSER" },
        shared(FnTrigger(|ctx: &mut Ctx, event, _velocity| {
            // Former BrowserCommand (SHIFT, SELECT) which discards the browser, confirmation is
            // via the encoder
            if event != ButtonEvent::Up {
                return;
            }
            if ctx.model.browser().is_some_and(|b| b.is_active()) {
                BrowserCommand::discard_browser(ctx, false);
                return;
            }
            if ctx.surface.is_pressed(ButtonId::Shift) {
                BrowserCommand::start_browser(ctx, true, true);
            } else {
                let insert = ctx.surface.is_pressed(ButtonId::Select);
                BrowserCommand::start_browser(ctx, insert, false);
            }
        })),
        mk::BROWSER,
        lit(|ctx: &Ctx| ctx.model.browser().is_some_and(|b| b.is_active())),
    );

    // Pad modes
    add_button(ctx, ButtonId::Accent, "ACCENT", shared(ToggleFixedVelCommand), mk::FIXED_VEL, lit(|ctx: &Ctx| ctx.config.is_accent_active()));

    add_button(
        ctx,
        ButtonId::Scene1,
        "SCENE",
        shared(ViewMultiSelectCommand::new(&[Views::ScenePlay])),
        mk::SCENE,
        lit(|ctx: &Ctx| ctx.views.is_active(Views::ScenePlay)),
    );
    add_button(
        ctx,
        ButtonId::Clip,
        "PATTERN",
        shared(ViewMultiSelectCommand::new(&[Views::Session])),
        mk::PATTERN,
        lit(|ctx: &Ctx| ctx.views.is_any_active(&[Views::ClipLength, Views::Session])),
    );
    add_button(ctx, ButtonId::Note, "EVENTS", shared(ModeSelectCommand::with_toggle(Modes::Note, true)), mk::EVENTS, lit(|ctx: &Ctx| ctx.modes.is_active(Modes::Note)));
    add_button(
        ctx,
        ButtonId::ToggleDevice,
        if is_studio_or_mk2 { "NAVIGATE" } else { "VARIATION" },
        shared(ViewMultiSelectCommand::new(&[Views::Device])),
        mk::VARIATION,
        lit(|ctx: &Ctx| ctx.views.is_active(Views::Device)),
    );
    add_button(ctx, ButtonId::Duplicate, "DUPLICATE", nop(), mk::DUPLICATE, None);

    if maschine.has_group_buttons() {
        add_button(ctx, ButtonId::Select, "SELECT", shared(MaschineSelectButtonCommand::default()), mk::SELECT, None);
        add_button(
            ctx,
            ButtonId::Solo,
            "SOLO",
            shared(FnTrigger(|ctx: &mut Ctx, event, _velocity| {
                if event == ButtonEvent::Up && ctx.surface.is_shift_pressed() {
                    set_stop_consumed(ctx);
                    ctx.model.track_bank_mut().clear_solo();
                }
            })),
            mk::SOLO,
            None,
        );
        add_button(
            ctx,
            ButtonId::Mute,
            "MUTE",
            shared(FnTrigger(|ctx: &mut Ctx, event, _velocity| {
                if event == ButtonEvent::Up && ctx.surface.is_shift_pressed() {
                    set_stop_consumed(ctx);
                    ctx.model.track_bank_mut().clear_mute();
                }
            })),
            mk::MUTE,
            None,
        );
    } else {
        add_button(
            ctx,
            ButtonId::Select,
            "SELECT",
            shared(ViewMultiSelectCommand::new(&[Views::TrackSelect])),
            mk::SELECT,
            lit(|ctx: &Ctx| ctx.views.is_active(Views::TrackSelect)),
        );
        add_button(ctx, ButtonId::Solo, "SOLO", shared(ViewMultiSelectCommand::new(&[Views::TrackSolo])), mk::SOLO, lit(|ctx: &Ctx| ctx.views.is_active(Views::TrackSolo)));
        add_button(ctx, ButtonId::Mute, "MUTE", shared(ViewMultiSelectCommand::new(&[Views::TrackMute])), mk::MUTE, lit(|ctx: &Ctx| ctx.views.is_active(Views::TrackMute)));
    }

    if is_studio_or_mk2 {
        add_button(
            ctx,
            ButtonId::Row11,
            "PAD MODE",
            shared(MaschineMk2PlayViewSwitchCommand),
            mk::PAD_MODE,
            lit(|ctx: &Ctx| ctx.views.is_any_active(&[Views::Drum, Views::Play])),
        );
    } else {
        add_button(ctx, ButtonId::Row11, "PAD MODE", shared(PadModeCommand::new(true)), mk::PAD_MODE, lit(|ctx: &Ctx| ctx.views.is_active(Views::Drum)));
        add_button(ctx, ButtonId::Row12, "KEYBOARD", shared(KeyboardCommand), mk::KEYBOARD, lit(|ctx: &Ctx| ctx.views.is_active(Views::Play)));
        add_button(
            ctx,
            ButtonId::Row13,
            "CHORDS",
            shared(FnTrigger(|ctx: &mut Ctx, _event, velocity| {
                if velocity == 0 {
                    super::command::trigger::with_play_view(ctx, |view, _ctx| view.toggle_chord_mode());
                }
            })),
            mk::CHORDS,
            lit(|ctx: &Ctx| super::command::trigger::play_view_state(ctx, |v| v.is_chord_mode()).unwrap_or(false)),
        );
    }

    add_button(
        ctx,
        ButtonId::Row14,
        "STEP",
        shared(FnTrigger(|ctx: &mut Ctx, event, _velocity| {
            if event == ButtonEvent::Up {
                with_drum_view(ctx, |view, ctx| view.toggle_sequencer_visible(ctx));
            }
        })),
        mk::STEP,
        lit(|ctx: &Ctx| drum_view_state(ctx, |v| v.is_sequencer_visible()).unwrap_or(false)),
    );

    register_cursor_keys(ctx, maschine);
    register_display_buttons(ctx, maschine);
    register_group_buttons(ctx, maschine);

    if maschine == Maschine::Studio {
        register_maschine_studio_buttons(ctx);
    }

    // Register foot switches
    let footswitches = maschine.footswitches();
    if footswitches >= 2 {
        add_button(ctx, ButtonId::Footswitch1, "Foot Controller (Tip)", shared(FootswitchCommand::new(0)), mk::FOOTSWITCH1_TIP, None);
        add_button(ctx, ButtonId::Footswitch2, "Foot Controller (Ring)", shared(FootswitchCommand::new(1)), mk::FOOTSWITCH1_RING, None);
        if footswitches == 4 {
            add_button(ctx, ButtonId::Footswitch3, "Foot Controller 2 (Tip)", shared(FootswitchCommand::new(2)), mk::FOOTSWITCH2_TIP, None);
            add_button(ctx, ButtonId::Footswitch4, "Foot Controller 2 (Ring)", shared(FootswitchCommand::new(3)), mk::FOOTSWITCH2_RING, None);
        }
    }
}

fn register_cursor_keys(ctx: &mut Ctx, maschine: Maschine) {
    if !maschine.has_cursor_keys() {
        return;
    }

    add_button(ctx, ButtonId::ArrowLeft, "LEFT", shared(MaschineCursorCommand::new(Direction::Left)), mk::CURSOR_LEFT, color(|ctx: &Ctx| encoder_color(ctx, ButtonId::ArrowLeft)));
    add_button(
        ctx,
        ButtonId::ArrowRight,
        "RIGHT",
        shared(MaschineCursorCommand::new(Direction::Right)),
        mk::CURSOR_RIGHT,
        color(|ctx: &Ctx| encoder_color(ctx, ButtonId::ArrowRight)),
    );
    add_button(ctx, ButtonId::ArrowUp, "UP", shared(CursorCommand::new_mode_cursor(Direction::Up, false)), mk::CURSOR_UP, color(|ctx: &Ctx| encoder_color(ctx, ButtonId::ArrowUp)));
    add_button(
        ctx,
        ButtonId::ArrowDown,
        "DOWN",
        shared(CursorCommand::new_mode_cursor(Direction::Down, false)),
        mk::CURSOR_DOWN,
        color(|ctx: &Ctx| encoder_color(ctx, ButtonId::ArrowDown)),
    );

    add_button(ctx, ButtonId::PageLeft, "PAGE LEFT", shared(PageCommand::new(Direction::Left)), mk::PAGE_LEFT, None);
    add_button(ctx, ButtonId::PageRight, "PAGE RIGHT", shared(PageCommand::new(Direction::Right)), mk::PAGE_RIGHT, None);

    add_button(ctx, ButtonId::LayoutArrange, if maschine == Maschine::Studio { "ARRANGE" } else { "ARRANGER" }, shared(LayoutCommand::new()), mk::ARRANGER, None);
    add_button(ctx, ButtonId::Mixer, if maschine == Maschine::Studio { "MIX" } else { "MIXER" }, shared(PaneCommand::new(Panels::Mixer)), mk::MIXER, None);
    add_button(
        ctx,
        ButtonId::Drum,
        if maschine == Maschine::Mk2 { "CONTROL" } else { "CHANNEL" },
        shared(AddTrackCommand::with_combinations(None, Some(ButtonId::Stop))),
        mk::CHANNEL,
        None,
    );
}

fn register_display_buttons(ctx: &mut Ctx, maschine: Maschine) {
    if !maschine.has_mcu_display() {
        return;
    }

    add_button(ctx, ButtonId::Row21, "Volume", shared(ModeSelectCommand::new(Modes::Volume)), mk::MODE_BUTTON_1, lit(|ctx: &Ctx| ctx.modes.is_active(Modes::Volume)));
    add_button(ctx, ButtonId::Row22, "Pan", shared(ModeSelectCommand::new(Modes::Pan)), mk::MODE_BUTTON_2, lit(|ctx: &Ctx| ctx.modes.is_active(Modes::Pan)));

    let send_select_command = Rc::new(std::cell::RefCell::new(MaschineSendSelectCommand::new()));
    let is_send = |ctx: &Ctx| ctx.modes.active_id().is_some_and(is_send_mode);
    let command = send_select_command.clone();
    add_button(
        ctx,
        ButtonId::Row23,
        "Send -",
        shared(FnTrigger(move |ctx: &mut Ctx, event, _velocity| command.borrow_mut().execute_shifted(ctx, event))),
        mk::MODE_BUTTON_3,
        lit(is_send),
    );
    add_button(ctx, ButtonId::Row24, "Send +", send_select_command, mk::MODE_BUTTON_4, lit(is_send));

    add_button(
        ctx,
        ButtonId::Row25,
        "Pin",
        shared(FnTrigger(|ctx: &mut Ctx, event, _velocity| {
            if event != ButtonEvent::Down {
                return;
            }
            if ctx.modes.is_active(Modes::DeviceParams) {
                ctx.model.cursor_device_mut().toggle_pinned();
            } else {
                ctx.model.toggle_cursor_track_pinned();
            }
        })),
        mk::MODE_BUTTON_5,
        lit(|ctx: &Ctx| if ctx.modes.is_active(Modes::DeviceParams) { ctx.model.cursor_device().is_pinned() } else { ctx.model.is_cursor_track_pinned() }),
    );

    add_button(
        ctx,
        ButtonId::Row26,
        "Active",
        shared(FnTrigger(|ctx: &mut Ctx, event, _velocity| {
            if event != ButtonEvent::Down {
                return;
            }
            if ctx.modes.is_active(Modes::DeviceParams) {
                ctx.model.cursor_device_mut().toggle_enabled_state();
            } else {
                ctx.model.cursor_track_mut().toggle_is_activated();
            }
        })),
        mk::MODE_BUTTON_6,
        lit(|ctx: &Ctx| {
            if ctx.modes.is_active(Modes::DeviceParams) {
                return ctx.model.cursor_device().is_enabled();
            }
            let track = ctx.model.cursor_track();
            track.does_exist() && track.is_activated()
        }),
    );

    // This button is mapped as Note not CC since it requires at least 1 MCU button to make
    // the MCU display activate!
    add_button(ctx, ButtonId::Row27, "User Params", shared(ModeSelectCommand::new(Modes::User)), mk::MODE_BUTTON_7, lit(|ctx: &Ctx| ctx.modes.is_active(Modes::User)));

    add_button(
        ctx,
        ButtonId::Row28,
        "Parameters",
        shared(ModeSelectCommand::new(Modes::DeviceParams)),
        mk::MODE_BUTTON_8,
        lit(|ctx: &Ctx| ctx.modes.is_active(Modes::DeviceParams)),
    );
}

fn register_group_buttons(ctx: &mut Ctx, maschine: Maschine) {
    if !maschine.has_group_buttons() {
        return;
    }
    for i in 0..8 {
        add_button(
            ctx,
            ButtonId::get(ButtonId::Row31, i),
            &format!("Bank {}", i + 1),
            shared(GroupButtonCommand::new(i)),
            mk::BANK_1 + i as i32,
            color(move |ctx: &Ctx| GroupButtonCommand::button_color(ctx, i)),
        );
    }
}

fn register_maschine_studio_buttons(ctx: &mut Ctx) {
    add_button(ctx, ButtonId::Metronome, "METRO", shared(MetronomeCommand::new(false)), mk::METRO, lit(|ctx: &Ctx| ctx.model.transport().is_metronome_on()));

    let functions = [
        (ButtonId::Undo, "UNDO", 0, 2, mk::EDIT_UNDO),
        (ButtonId::Redo, "REDO", 1, 3, mk::EDIT_REDO),
        (ButtonId::Quantize, "QUANTIZE", 4, 5, mk::EDIT_QUANTIZE),
        (ButtonId::Control, "CLEAR", 8, 9, mk::EDIT_CLEAR),
        (ButtonId::Copy, "COPY", 10, 10, mk::EDIT_COPY),
        (ButtonId::Paste, "PASTE", 11, 11, mk::EDIT_PASTE),
        (ButtonId::NudgeMinus, "NOTE", 13, 12, mk::EDIT_NOTE),
        (ButtonId::NudgePlus, "NUDGE", 15, 14, mk::EDIT_NUDGE),
    ];
    for (id, label, pad_index, shift_pad_index, control) in functions {
        add_button(ctx, id, label, create_shift_view_function(pad_index, shift_pad_index), control, None);
    }
}

/// Execute a function of the shift view (former `createShiftViewFunction`).
fn create_shift_view_function(pad_index: i32, shift_pad_index: i32) -> SharedTrigger {
    shared(FnTrigger(move |ctx: &mut Ctx, event, _velocity| {
        let index = if ctx.surface.is_shift_pressed() { shift_pad_index } else { pad_index };
        if let Some(view) = ctx.views.get(Views::Shift) {
            if let Ok(mut view) = view.try_borrow_mut() {
                if let Some(shift_view) = view.as_any_mut().downcast_mut::<ShiftView>() {
                    shift_view.execute_function(ctx, index, event);
                }
            }
        }
    }))
}

/// The color of the cursor buttons depending on the active mode (former `getEncoderColor`).
fn encoder_color(ctx: &Ctx, arrow_button: ButtonId) -> i32 {
    let Some(mode_id) = ctx.modes.active_id() else {
        return COLOR_BLACK;
    };
    let Some(mode) = ctx.modes.active() else {
        return COLOR_BLACK;
    };
    let Ok(mode) = mode.try_borrow() else {
        return COLOR_BLACK;
    };

    let is_on = match arrow_button {
        ButtonId::ArrowLeft => mode.has_previous_item(ctx),
        ButtonId::ArrowRight => mode.has_next_item(ctx),
        ButtonId::ArrowUp => mode.has_next_item_page(ctx),
        ButtonId::ArrowDown => mode.has_previous_item_page(ctx),
        // Never reached
        _ => return COLOR_BLACK,
    };
    if !is_on {
        return COLOR_BLACK;
    }

    let is_pressed = ctx.surface.is_pressed(arrow_button);
    let pick = |pressed: i32, released: i32| if is_pressed { pressed } else { released };
    match mode_id {
        Modes::Volume => pick(COLOR_BLUE, COLOR_BLUE_LO),
        Modes::Pan => pick(COLOR_SKY, COLOR_SKY_LO),
        Modes::Send1 | Modes::Send2 | Modes::Send3 | Modes::Send4 | Modes::Send5 | Modes::Send6 | Modes::Send7 | Modes::Send8 => pick(COLOR_WHITE, COLOR_YELLOW),
        Modes::DeviceParams | Modes::User => pick(COLOR_PINK, COLOR_PINK_LO),
        Modes::Browser => pick(COLOR_AMBER, COLOR_AMBER_LO),
        Modes::Note => pick(COLOR_GREEN, COLOR_GREEN_LO),
        _ => COLOR_BLACK,
    }
}

// ------------------------------------------------------------------------------------------
// Continuous controls

/// Signal the shift view that it was used and that the Stop button should not stop (the
/// former `addHasChangedObserver` of the knobs).
fn knob_changed(ctx: &mut Ctx) {
    if let Some(view) = ctx.views.get(Views::Shift) {
        if let Ok(mut view) = view.try_borrow_mut() {
            if let Some(shift_view) = view.as_any_mut().downcast_mut::<ShiftView>() {
                shift_view.set_was_used();
            }
        }
    }
    set_stop_consumed(ctx);
}

/// Add a relative knob (former `addRelativeKnob`) with a touch command, which calls the
/// `onKnobTouch` function of the active mode with the given index.
fn add_relative_knob(ctx: &mut Ctx, id: ContinuousId, label: &str, command: Rc<std::cell::RefCell<dyn ContinuousCommand>>, midi_control: i32, touch_control: i32, touch_index: usize) {
    ctx.add_continuous(id, label, ContinuousKind::RelativeKnob, command, BindType::Cc, 0, midi_control);
    let touch_command = shared(FnTrigger(move |ctx: &mut Ctx, event, _velocity| {
        if event == ButtonEvent::Long {
            return;
        }
        if let Some(mode) = ctx.active_mode() {
            if let Ok(mut mode) = mode.try_borrow_mut() {
                mode.on_knob_touch(ctx, touch_index, event == ButtonEvent::Down);
            }
        }
    }));
    if let Some(control) = ctx.surface.continuous.get_mut(&id) {
        control.touch_command = Some(touch_command);
        control.changed_observer = Some(Rc::new(knob_changed));
    }
    ctx.surface.input.bind_touch(id, BindType::Cc, 0, touch_control);
}

/// Register the commands of the knobs, the encoder and the touch strip (former
/// `registerContinuousCommands`).
pub fn register_continuous_commands(ctx: &mut Ctx, maschine: Maschine) {
    add_relative_knob(
        ctx,
        ContinuousId::MasterKnob,
        "Encoder",
        Rc::new(std::cell::RefCell::new(MainKnobRowModeCommand::new())),
        mk::ENCODER,
        mk::ENCODER_TOUCH,
        8,
    );

    if maschine.has_mcu_display() {
        for i in 0..8 {
            add_relative_knob(
                ctx,
                ContinuousId::get(ContinuousId::Knob1, i),
                &format!("Knob {}", i + 1),
                Rc::new(std::cell::RefCell::new(KnobRowModeCommand::new(i))),
                mk::MODE_KNOB_1 + i as i32,
                mk::MODE_KNOB_TOUCH_1 + i as i32,
                i,
            );
        }
    }

    ctx.add_continuous(
        ContinuousId::Crossfader,
        "Touchstrip",
        ContinuousKind::Fader,
        Rc::new(std::cell::RefCell::new(TouchstripCommand::new())),
        BindType::Cc,
        0,
        mk::TOUCHSTRIP,
    );
    if let Some(control) = ctx.surface.continuous.get_mut(&ContinuousId::Crossfader) {
        control.touch_command = Some(shared(TouchstripCommand::new()));
    }
    ctx.surface.input.bind_touch(ContinuousId::Crossfader, BindType::Cc, 0, mk::TOUCHSTRIP_TOUCH);

    // Aftertouch is enabled in the play and drum views (they own their aftertouch command)

    // The monitor encoder of the Maschine Studio (StudioEncoderModeManager) is not ported
}
