// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The setup of the Launchpad controllers (former `LaunchpadControllerSetup`).
//!
//! Differences to Java:
//! - The observers are replaced by [`ControllerSetup::handle_changes`], which drains the model
//!   events and the changed settings.
//! - `layoutControls` (the bounds of the simulator GUI) is not ported.
//! - The browser observer compares the active state of the browser on every change check.
//! - Indications of the effect track bank are not set (the model has no mutable access to it;
//!   all indication setters of the Reaper model do nothing anyway).

use std::rc::Rc;

use super::colors::*;
use super::command::cursor::LaunchpadCursorCommand;
use super::command::misc::{DeleteCommand, LaunchpadDuplicateCommand, PlayAndNewCommand};
use super::command::track::{TrackButton, TrackButtonCommand};
use super::command::view_select::{ProjectCommand, SelectDeviceViewCommand, SelectMainViewCommand, SelectSessionViewCommand};
use super::definition::{LaunchpadButton, LaunchpadDefinition};
use super::view::browser_view::LaunchpadBrowserView;
use super::view::chords_view::ChordsView;
use super::view::device_view::DeviceView;
use super::view::mix_view::MixView;
use super::view::note_edit_view::NoteEditView;
use super::view::note_view_select_view::NoteViewSelectView;
use super::view::pan_view::PanView;
use super::view::project_view::ProjectView;
use super::view::sends_view::SendsView;
use super::view::session_view::{is_session_view, SessionView};
use super::view::shift_view::ShiftView;
use super::view::tempo_view::{LaunchpadShuffleView, LaunchpadTempoView};
use super::view::user_view::UserView;
use super::view::volume_view::VolumeView;
use super::view::drum64_view::Drum64View;
use super::view::drum_lane_view::DrumLaneView;
use super::view::drum_view::DrumView;
use super::view::piano_view::PianoView;
use super::view::play_view::PlayView;
use super::view::poly_sequencer_view::PolySequencerView;
use super::view::raindrops_view::RaindropsView;
use super::view::sequencer_view::SequencerView;
use super::surface::{self as lp, create_pad_grid, LaunchpadExt, LAUNCHPAD_TRACK1, PRO3_LAUNCHPAD_FIXED_LENGTH};
use crate::backend::Sender;
use crate::config::values::REAPER_ARP_MODES;
use crate::config::{Configuration, LaunchpadConfiguration, SettingId};
use crate::controller::{ControllerSetup, InstanceParts, MidiPorts};
use crate::daw::bank::BankItem;
use crate::daw::color::DawColor;
use crate::daw::events::ModelEvent;
use crate::daw::model::Model;
use crate::daw::model_setup::ModelSetup;
use crate::framework::command::{shared, FnTrigger, SharedTrigger};
use crate::framework::commands::application::UndoCommand;
use crate::framework::commands::clip::{NewCommand, QuantizeCommand};
use crate::framework::commands::cursor::Direction;
use crate::framework::commands::transport::{ConfiguredRecordCommand, MetronomeCommand};
use crate::framework::commands::view::{SelectPlayViewCommand, ToggleShiftViewCommand, ViewButtonCommand, ViewMultiSelectCommand};
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, Modes, OutputId, Views};
use crate::framework::mode::track::{
    TrackMuteMode, TrackPanMode, TrackRecArmMode, TrackSelectMode, TrackSendMode, TrackSoloMode, TrackStopClipMode, TrackVolumeMode,
};
use crate::framework::surface::Surface;
use crate::framework::view::scene_play_view::ScenePlayView;
use crate::framework::view::sequencer::clip_length_view::ClipLengthView;
use crate::framework::value_changer::ValueChanger;
use crate::host::Host;
use crate::midi::{BindType, ButtonEvent, DeviceInquiry, MidiInput, MidiOutput};
use crate::scale::Scales;

/// The play views selected by the Note button of the Pro Mk3.
pub const PLAY_VIEWS: [Views; 3] = [Views::Play, Views::Piano, Views::Drum64];

/// The sequencer views selected by the Sequencer button of the Pro Mk3.
pub const SEQUENCER_VIEWS: [Views; 3] = [Views::Sequencer, Views::PolySequencer, Views::Raindrops];

/// The drum views selected by the Drum button of the Pro Mk3.
pub const DRUM_VIEWS: [Views; 3] = [Views::Drum, Views::Drum4, Views::Drum8];

/// All play and sequencer views.
pub const MAIN_VIEWS: [Views; 11] = [
    Views::Play,
    Views::Chords,
    Views::Piano,
    Views::Drum64,
    Views::Sequencer,
    Views::PolySequencer,
    Views::Raindrops,
    Views::Drum,
    Views::Drum4,
    Views::Drum8,
    Views::ClipLength,
];

/// The controller notes of the 8x8 grid (row by row from the bottom).
pub fn translate_matrix() -> Vec<i32> {
    (36..100).map(|note| lp::translate_to_controller(note).1).collect()
}

/// Create a Launchpad instance.
pub fn create(definition: LaunchpadDefinition, host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
    let value_changer = ValueChanger::twos_complement(128, 1);
    let colors = create_color_manager();

    let scales = Scales::new(&value_changer, 36, 100, 8, 8).with_grid_translator(Scales::table_grid_translator(36, translate_matrix()));
    let mut model_setup = ModelSetup::new();
    model_setup.enable_drum64_device();
    model_setup.set_has_full_flat_track_list(configuration.are_master_tracks_included());
    let model = Model::new(host.clone(), sender.clone(), model_setup, value_changer, scales);

    let output = MidiOutput::new(sender.clone(), Some(ports.output));
    let input = MidiInput::new(sender.clone(), Some(ports.input), &["80????", "90????"])?;
    let surface = Surface::new(output, input, Some(create_pad_grid(&colors)));

    let mut ctx = Ctx::new(host, sender, model, configuration, surface, colors);
    ctx.ext = Box::new(LaunchpadExt::new(definition));
    ctx.sysex_handler = Some(Rc::new(lp::handle_sysex));
    lp::install_trigger_sender(&mut ctx);
    ctx.surface.output.send_sysex(&DeviceInquiry::QUERY);
    ctx.create_pads();

    Ok(InstanceParts { ctx, setup: Box::new(LaunchpadSetup::default()) })
}

/// The Launchpad controller setup.
#[derive(Default)]
pub struct LaunchpadSetup {
    /// The last seen active state of the browser (replaces the browser observer).
    browser_active: bool,
}

impl ControllerSetup for LaunchpadSetup {
    fn init(&mut self, ctx: &mut Ctx) {
        // createSurface
        lp::set_launchpad_to_standalone(ctx);
        lp::set_launchpad_to_prg_mode(ctx);

        create_modes(ctx);
        create_views(ctx);
        register_trigger_commands(ctx);
        register_continuous_commands(ctx);
        create_observers(ctx);
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        let view = if ctx.config.should_start_with_session_view() { Views::Session } else { ctx.config.startup_view() };
        ctx.set_active_view(view);
        ctx.set_active_mode(Modes::Dummy);
    }

    fn flush(&mut self, ctx: &mut Ctx) {
        ctx.flush_surface();
        lp::flush_pad_grid(ctx);
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

        // Browser observer (former activateBrowserObserver (Views.BROWSER))
        let browser_active = ctx.model.browser().is_some_and(|browser| browser.is_active());
        if browser_active != self.browser_active {
            self.browser_active = browser_active;
            handle_browser_active(ctx, browser_active);
        }
    }

    fn exit(&mut self, ctx: &mut Ctx) {
        ctx.turn_off_lights();
        lp::reset_mode(ctx);
    }
}

// ------------------------------------------------------------------------------------------
// Modes and views

/// Register the modes (former `createModes`).
pub fn create_modes(ctx: &mut Ctx) {
    let rec_arm = TrackRecArmMode::new(ctx);
    ctx.register_mode(Modes::RecArm, rec_arm);
    let track_select = TrackSelectMode::new(ctx);
    ctx.register_mode(Modes::TrackSelect, track_select);
    let mute = TrackMuteMode::new(ctx);
    ctx.register_mode(Modes::Mute, mute);
    let solo = TrackSoloMode::new(ctx);
    ctx.register_mode(Modes::Solo, solo);
    let volume = TrackVolumeMode::new(ctx, true);
    ctx.register_mode(Modes::Volume, volume);
    let pan = TrackPanMode::new(ctx, true);
    ctx.register_mode(Modes::Pan, pan);
    let send = TrackSendMode::new(-1, ctx, true);
    ctx.register_mode(Modes::Send, send);
    let stop_clip = TrackStopClipMode::new(ctx);
    ctx.register_mode(Modes::StopClip, stop_clip);
    let dummy = TrackSelectMode::new(ctx);
    ctx.register_mode(Modes::Dummy, dummy);
}

/// Register the views (former `createViews`).
pub fn create_views(ctx: &mut Ctx) {
    let view = ScenePlayView::new(ctx);
    ctx.register_view(Views::ScenePlay, view);
    let view = PlayView::new(ctx);
    ctx.register_view(Views::Play, view);
    let view = ChordsView::new(ctx);
    ctx.register_view(Views::Chords, view);
    let view = PianoView::new(ctx);
    ctx.register_view(Views::Piano, view);
    let view = SequencerView::new(ctx);
    ctx.register_view(Views::Sequencer, view);
    let view = PolySequencerView::new(ctx, true);
    ctx.register_view(Views::PolySequencer, view);
    let view = RaindropsView::new(ctx);
    ctx.register_view(Views::Raindrops, view);
    let view = DrumView::new(ctx);
    ctx.register_view(Views::Drum, view);
    let view = DrumLaneView::new_drum4(ctx);
    ctx.register_view(Views::Drum4, view);
    let view = DrumLaneView::new_drum8(ctx);
    ctx.register_view(Views::Drum8, view);
    let view = Drum64View::new(ctx);
    ctx.register_view(Views::Drum64, view);
    let view = ClipLengthView::new(ctx, true);
    ctx.register_view(Views::ClipLength, view);

    let view = ShiftView::new(ctx);
    ctx.register_view(Views::Shift, view);
    let view = SessionView::new("Session", ctx);
    ctx.register_view(Views::Session, view);
    let view = DeviceView::new(ctx);
    ctx.register_view(Views::Device, view);
    let view = UserView::new(ctx);
    ctx.register_view(Views::User, view);
    let view = LaunchpadBrowserView::new(ctx);
    ctx.register_view(Views::Browser, view);
    let view = PanView::new(ctx);
    ctx.register_view(Views::TrackPan, view);
    let view = VolumeView::new(ctx);
    ctx.register_view(Views::TrackVolume, view);
    let view = SendsView::new(ctx);
    ctx.register_view(Views::TrackSends, view);
    let view = MixView::new(ctx);
    ctx.register_view(Views::Mix, view);
    let view = NoteViewSelectView::new(ctx);
    ctx.register_view(Views::Control, view);
    let view = NoteEditView::new(ctx);
    ctx.register_view(Views::NoteEditView, view);
    let view = LaunchpadTempoView::new(ctx, LAUNCHPAD_COLOR_BLUE_HI, LAUNCHPAD_COLOR_WHITE, LAUNCHPAD_COLOR_BLACK);
    ctx.register_view(Views::Tempo, view);
    let view = LaunchpadShuffleView::new(ctx, LAUNCHPAD_COLOR_PINK_HI, LAUNCHPAD_COLOR_WHITE, LAUNCHPAD_COLOR_BLACK);
    ctx.register_view(Views::Shuffle, view);
    let view = ProjectView::new(ctx);
    ctx.register_view(Views::Project, view);
}

/// Former `registerContinuousCommands`: the views Play, Piano, Drum and Drum64 create their
/// `AftertouchViewCommand` in their constructors, nothing left to do here.
pub fn register_continuous_commands(_ctx: &mut Ctx) {}

// ------------------------------------------------------------------------------------------
// Observers

/// Former `createObservers`: the parts which are not handled in `handle_changes`.
fn create_observers(ctx: &mut Ctx) {
    ctx.views.add_change_listener(Rc::new(|ctx: &mut Ctx, _previous, _active| {
        if let Some(light) = ctx.surface.light(OutputId::Led1) {
            light.force_flush();
        }
        update_indication(ctx);
    }));
}

/// React on a changed setting (former setting observers).
fn handle_setting_change(ctx: &mut Ctx, id: SettingId) {
    match id {
        LaunchpadConfiguration::PAD_BRIGHTNESS => {
            if ctx.config.can_setting_be_observed(id) {
                let brightness = ctx.config.custom_index(LaunchpadConfiguration::PAD_BRIGHTNESS) as i32;
                lp::update_brightness(ctx, brightness);
            }
        }

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

/// De-/Activate the browser view depending on the browsers' active state.
fn handle_browser_active(ctx: &mut Ctx, is_active: bool) {
    if is_active {
        let previous_view_id = ctx.views.previous_id();
        ctx.set_temporary_view(Views::Browser);
        if ctx.views.previous_id() == Some(Views::Shift) {
            ctx.views.set_previous_id(previous_view_id);
        }
    } else if ctx.views.is_active(Views::Browser) {
        ctx.restore_view();
    }
}

/// Set the indications of the controlled parameters (former `updateIndication`).
pub fn update_indication(ctx: &mut Ctx) {
    let is_volume = ctx.views.is_active(Views::TrackVolume);
    let is_pan = ctx.views.is_active(Views::TrackPan);
    let is_sends = ctx.views.is_active(Views::TrackSends);
    let is_device = ctx.views.is_active(Views::Device);

    let (sel_send, is_session_type) = match ctx.active_view() {
        Some(view) => match view.try_borrow() {
            Ok(view) => (
                view.as_any().downcast_ref::<SendsView>().map_or(-1, |v| v.selected_send() as i32),
                is_session_view(&*view),
            ),
            Err(_) => (-1, false),
        },
        None => (-1, false),
    };
    let is_session = is_session_type && !is_volume && !is_pan && !is_sends;

    let is_effect = ctx.model.is_effect_track_bank_active();

    let tb = ctx.model.track_bank_mut();
    tb.set_indication(!is_effect && is_session);
    for i in 0..8 {
        let track = tb.item_mut(i);
        track.set_volume_indication(!is_effect && is_volume);
        track.set_pan_indication(!is_effect && is_pan);
        let send_bank = track.send_bank_mut();
        for j in 0..8.min(send_bank.page_size()) {
            send_bank.item_mut(j).set_indication(!is_effect && is_sends && sel_send == j as i32);
        }
    }

    if let Some(parameter_bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
        for i in 0..8.min(parameter_bank.page_size()) {
            parameter_bank.item_mut(i).set_indication(is_device);
        }
    }
}

/// Handle a track selection change (former `handleTrackChange`).
pub fn handle_track_change(ctx: &mut Ctx, is_selected: bool) {
    if !is_selected {
        return;
    }

    // Do not restore preferred play view view if certain views are active
    if ctx.views.is_any_active(&[Views::Mix, Views::User, Views::TrackPan, Views::TrackVolume, Views::TrackSends, Views::Device]) {
        return;
    }

    // updateView
    recall_last_view(ctx);
    reset_drum_octave(ctx);
    update_mode(ctx);
}

/// Recall last used view (if we are not in session, mix or project mode, former
/// `recallLastView`).
fn recall_last_view(ctx: &mut Ctx) {
    if !ctx.views.is_any_active(&[Views::Session, Views::Mix, Views::Project]) || ctx.surface.is_pressed(ButtonId::Track) {
        ctx.recall_preferred_view();
    }
}

/// Reset drum octave because the drum pad bank is also reset (former `resetDrumOctave`).
fn reset_drum_octave(ctx: &mut Ctx) {
    let Some(view) = ctx.active_view() else {
        return;
    };
    let Ok(mut view) = view.try_borrow_mut() else {
        return;
    };
    if ctx.views.is_any_active(&[Views::Drum, Views::Play]) {
        view.update_note_mapping(ctx);
    } else if ctx.views.is_active(Views::Drum64) {
        if let Some(transpose) = view.as_transpose_view_mut() {
            transpose.reset_octave(ctx);
        }
    }
}

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

/// Update the used mode (former `updateMode`).
fn update_mode(ctx: &mut Ctx) {
    if ctx.modes.is_active(Modes::Master) && !ctx.model.master_track().is_selected() {
        if ctx.modes.previous_id().is_some_and(|id| TRACK_MODES.contains(&id)) {
            ctx.restore_mode();
        } else {
            ctx.set_active_mode(Modes::Track);
        }
    }
}

// ------------------------------------------------------------------------------------------
// Buttons

/// Get the bind type for a trigger button (former `getTriggerBindType`).
pub fn trigger_bind_type(definition: &LaunchpadDefinition, button: ButtonId) -> BindType {
    if button >= ButtonId::Scene1 && button <= ButtonId::Scene8 {
        if definition.scene_buttons_use_cc() {
            BindType::Cc
        } else {
            BindType::Note
        }
    } else {
        BindType::Cc
    }
}

type Supplier = Rc<dyn Fn(&Ctx) -> i32>;

/// Add a button with the default bind type of the button (former `addButton`).
fn add_button(ctx: &mut Ctx, id: ButtonId, label: &str, command: SharedTrigger, midi_control: i32, supplier: Supplier) {
    let bind_type = trigger_bind_type(&lp::ext(ctx).definition, id);
    ctx.add_button(id, label, Some(command), bind_type, 0, midi_control, Some(supplier));
}

/// Register the commands of all buttons (former `registerTriggerCommands`).
pub fn register_trigger_commands(ctx: &mut Ctx) {
    let definition = lp::ext(ctx).definition.clone();
    let control = |button: LaunchpadButton| definition.button(button).control;

    add_button(
        ctx,
        ButtonId::Shift,
        "Shift",
        shared(ToggleShiftViewCommand::new()),
        control(LaunchpadButton::Shift),
        Rc::new(|ctx: &Ctx| {
            if ctx.views.is_active(Views::Shift) || ctx.surface.is_shift_pressed() {
                LAUNCHPAD_COLOR_WHITE
            } else {
                LAUNCHPAD_COLOR_GREY_LO
            }
        }),
    );

    let cursors = [
        (ButtonId::Up, "Up", Direction::Up, LaunchpadButton::ArrowUp),
        (ButtonId::Down, "Down", Direction::Down, LaunchpadButton::ArrowDown),
        (ButtonId::Left, "Left", Direction::Left, LaunchpadButton::ArrowLeft),
        (ButtonId::Right, "Right", Direction::Right, LaunchpadButton::ArrowRight),
    ];
    for (id, label, direction, button) in cursors {
        add_button(
            ctx,
            id,
            label,
            shared(LaunchpadCursorCommand::new(direction)),
            control(button),
            Rc::new(move |ctx: &Ctx| if LaunchpadCursorCommand::can_scroll(ctx, direction) { view_color(ctx) } else { LAUNCHPAD_COLOR_BLACK }),
        );
    }

    add_button(
        ctx,
        ButtonId::Session,
        "Session",
        shared(SelectSessionViewCommand::new()),
        control(LaunchpadButton::Session),
        Rc::new(|ctx: &Ctx| view_state_color(ctx, LAUNCHPAD_COLOR_LIME, &[Views::Session, Views::Mix])),
    );
    add_button(
        ctx,
        ButtonId::Device,
        "Device",
        shared(SelectDeviceViewCommand::new()),
        control(LaunchpadButton::Device),
        Rc::new(SelectDeviceViewCommand::button_color),
    );

    let user_info = definition.button(LaunchpadButton::User);
    if !user_info.is_virtual() {
        add_button(
            ctx,
            ButtonId::User,
            "User",
            shared(ViewMultiSelectCommand::new(&[Views::ClipLength])),
            user_info.control,
            Rc::new(|ctx: &Ctx| if ctx.views.is_active(Views::ClipLength) { LAUNCHPAD_COLOR_MAGENTA } else { LAUNCHPAD_COLOR_GREY_LO }),
        );
    }

    let project_info = definition.button(LaunchpadButton::Project);
    if !project_info.is_virtual() {
        add_button(
            ctx,
            ButtonId::Project,
            "Project",
            shared(ProjectCommand::new()),
            project_info.control,
            Rc::new(|ctx: &Ctx| if ctx.views.is_active(Views::Project) { LAUNCHPAD_COLOR_CYAN_HI } else { LAUNCHPAD_COLOR_GREY_LO }),
        );
    }

    // The following buttons are only available on the Pro but the commands are used by all
    // Launchpad models!
    let click_info = definition.button(LaunchpadButton::Click);
    add_button(
        ctx,
        ButtonId::Metronome,
        "Metronome",
        shared(MetronomeCommand::new(true)),
        if click_info.is_shifted { -1 } else { click_info.control },
        Rc::new(|ctx: &Ctx| {
            if ctx.surface.is_shift_pressed() {
                return if ctx.surface.is_pressed(ButtonId::Metronome) { LAUNCHPAD_COLOR_WHITE } else { LAUNCHPAD_COLOR_GREEN_SPRING };
            }
            if ctx.model.transport().is_metronome_on() {
                LAUNCHPAD_COLOR_GREEN_HI
            } else {
                LAUNCHPAD_COLOR_GREEN_LO
            }
        }),
    );

    let undo_info = definition.button(LaunchpadButton::Undo);
    add_button(
        ctx,
        ButtonId::Undo,
        "Undo",
        shared(UndoCommand::new()),
        if undo_info.is_shifted { -1 } else { undo_info.control },
        Rc::new(|ctx: &Ctx| two_state_color(ctx, ButtonId::Undo)),
    );

    add_button(ctx, ButtonId::Delete, "Delete", shared(DeleteCommand::new()), control(LaunchpadButton::Delete), Rc::new(delete_state_color));
    add_button(
        ctx,
        ButtonId::Quantize,
        "Quantize",
        shared(QuantizeCommand::new()),
        control(LaunchpadButton::Quantize),
        Rc::new(|ctx: &Ctx| state_color(ctx, ButtonId::Quantize)),
    );
    add_button(
        ctx,
        ButtonId::Duplicate,
        "Duplicate",
        shared(LaunchpadDuplicateCommand::new()),
        control(LaunchpadButton::Duplicate),
        Rc::new(duplicate_state_color),
    );
    add_button(ctx, ButtonId::Play, "Play", shared(PlayAndNewCommand::new()), control(LaunchpadButton::Play), Rc::new(play_state_color));

    if definition.is_pro() {
        add_button(
            ctx,
            ButtonId::Record,
            "Record",
            shared(ConfiguredRecordCommand::new()),
            control(LaunchpadButton::Record),
            Rc::new(|ctx: &Ctx| {
                let is_shift = ctx.surface.is_shift_pressed();
                let is_lit = ConfiguredRecordCommand::is_lit(ctx, is_shift);
                match (is_shift, is_lit) {
                    (true, true) => LAUNCHPAD_COLOR_ROSE,
                    (true, false) => LAUNCHPAD_COLOR_RED_AMBER,
                    (false, true) => LAUNCHPAD_COLOR_RED_HI,
                    (false, false) => LAUNCHPAD_COLOR_RED_LO,
                }
            }),
        );
    }

    // Mode selection buttons

    let mode_buttons = [
        (ButtonId::RecArm, "Rec Arm", TrackButton::RecordArm, LaunchpadButton::RecArm),
        (ButtonId::Track, "Track", TrackButton::TrackSelect, LaunchpadButton::TrackSelect),
        (ButtonId::Mute, "Mute", TrackButton::Mute, LaunchpadButton::Mute),
        (ButtonId::Solo, "Solo", TrackButton::Solo, LaunchpadButton::Solo),
        (ButtonId::Volume, "Volume", TrackButton::Volume, LaunchpadButton::Volume),
        (ButtonId::PanSend, "Panning", TrackButton::Pan, LaunchpadButton::Pan),
        (ButtonId::Sends, "Sends", TrackButton::Sends, LaunchpadButton::Sends),
        (ButtonId::StopClip, "Stop Clip", TrackButton::StopClip, LaunchpadButton::StopClip),
    ];
    for (id, label, function, button) in mode_buttons {
        add_button(ctx, id, label, shared(TrackButtonCommand::new(function)), control(button), Rc::new(move |ctx: &Ctx| mode_color_index(ctx, id)));
    }

    // Scene buttons

    for (i, scene) in LaunchpadButton::SCENES.iter().enumerate() {
        let id = ButtonId::get(ButtonId::Scene1, i);
        add_button(
            ctx,
            id,
            &format!("Scene {}", i + 1),
            shared(ViewButtonCommand::new(id)),
            control(*scene),
            Rc::new(move |ctx: &Ctx| ctx.button_color_from_active_view(id)),
        );
    }

    // Pro Mk3
    if definition.has_track_selection_buttons() {
        // Additional Track selection buttons
        for i in 0..8 {
            add_button(
                ctx,
                ButtonId::get(ButtonId::Row11, i),
                &format!("Track Select {}", i + 1),
                shared(FnTrigger(move |ctx: &mut Ctx, event, _velocity| handle_track_selection(ctx, event, i))),
                LAUNCHPAD_TRACK1 + i as i32,
                Rc::new(move |ctx: &Ctx| track_mode_color_index(ctx, i)),
            );
        }

        add_button(
            ctx,
            ButtonId::Note,
            "Note",
            shared(SelectPlayViewCommand::new(&PLAY_VIEWS, &MAIN_VIEWS)),
            control(LaunchpadButton::Note),
            Rc::new(|ctx: &Ctx| view_state_color(ctx, LAUNCHPAD_COLOR_AMBER_HI, &PLAY_VIEWS)),
        );
        add_button(
            ctx,
            ButtonId::Drum,
            "Drum Seq",
            shared(SelectPlayViewCommand::new(&DRUM_VIEWS, &MAIN_VIEWS)),
            95,
            Rc::new(|ctx: &Ctx| view_state_color(ctx, LAUNCHPAD_COLOR_OCEAN_HI, &DRUM_VIEWS)),
        );
        add_button(
            ctx,
            ButtonId::Sequencer,
            "Sequencer",
            shared(SelectPlayViewCommand::new(&SEQUENCER_VIEWS, &MAIN_VIEWS)),
            97,
            Rc::new(|ctx: &Ctx| view_state_color(ctx, LAUNCHPAD_COLOR_YELLOW_HI, &SEQUENCER_VIEWS)),
        );
        add_button(
            ctx,
            ButtonId::New,
            "New",
            shared(NewCommand::new()),
            PRO3_LAUNCHPAD_FIXED_LENGTH,
            Rc::new(|ctx: &Ctx| if ctx.surface.is_pressed(ButtonId::New) { LAUNCHPAD_COLOR_AMBER_HI } else { LAUNCHPAD_COLOR_AMBER_LO }),
        );
    } else {
        add_button(
            ctx,
            ButtonId::Note,
            "Note",
            shared(SelectMainViewCommand::new(&MAIN_VIEWS)),
            control(LaunchpadButton::Note),
            Rc::new(note_button_color),
        );
    }

    // Update the front or logo LED with the color of the current track
    ctx.surface.create_light(
        Some(OutputId::Led1),
        None,
        Rc::new(|ctx: &Ctx| {
            let cursor_track = ctx.model.cursor_track();
            if cursor_track.does_exist() {
                ctx.colors.color_index(DawColor::color_id(cursor_track.color()))
            } else {
                0
            }
        }),
        Rc::new(lp::set_logo_color),
    );

    // Workaround for some pads not redrawn all the time (the Java condition is true for all
    // buttons)
    for id in ctx.surface.button_ids() {
        ctx.surface.add_button_event_handler(
            id,
            ButtonEvent::Up,
            Rc::new(move |ctx: &mut Ctx, _event| {
                if let Some(light) = ctx.surface.button_light(id) {
                    light.force_flush();
                }
            }),
        );
    }
}

/// Forward a track selection button of the Pro Mk3 to the active mode.
fn handle_track_selection(ctx: &mut Ctx, event: ButtonEvent, index: usize) {
    if let Some(mode) = ctx.active_mode() {
        if let Ok(mut mode) = mode.try_borrow_mut() {
            mode.on_button(ctx, 0, index, event);
        }
    }
}

// ------------------------------------------------------------------------------------------
// Colors

fn note_button_color(ctx: &Ctx) -> i32 {
    let active = ctx.views.active_id();
    if active == Some(Views::Control) {
        return COLOR_VIEW_SELECTED;
    }
    if active.is_some_and(|id| MAIN_VIEWS.contains(&id)) {
        view_color(ctx)
    } else {
        LAUNCHPAD_COLOR_GREY_LO
    }
}

fn state_color(ctx: &Ctx, button: ButtonId) -> i32 {
    if ctx.surface.is_shift_pressed() {
        return LAUNCHPAD_COLOR_GREEN_SPRING;
    }
    if ctx.surface.is_pressed(button) {
        LAUNCHPAD_COLOR_GREEN
    } else {
        LAUNCHPAD_COLOR_GREEN_LO
    }
}

fn two_state_color(ctx: &Ctx, button: ButtonId) -> i32 {
    let is_pressed = ctx.surface.is_pressed(button);
    match (ctx.surface.is_shift_pressed(), is_pressed) {
        (true, true) => LAUNCHPAD_COLOR_WHITE,
        (true, false) => LAUNCHPAD_COLOR_GREEN_SPRING,
        (false, true) => LAUNCHPAD_COLOR_GREEN,
        (false, false) => LAUNCHPAD_COLOR_GREEN_LO,
    }
}

fn delete_state_color(ctx: &Ctx) -> i32 {
    if ctx.surface.is_shift_pressed() {
        return if ctx.model.transport().is_loop() { LAUNCHPAD_COLOR_BLUE_HI } else { LAUNCHPAD_COLOR_BLUE_LO };
    }
    if ctx.surface.is_pressed(ButtonId::Delete) {
        LAUNCHPAD_COLOR_MAGENTA_HI
    } else {
        LAUNCHPAD_COLOR_MAGENTA_LO
    }
}

fn duplicate_state_color(ctx: &Ctx) -> i32 {
    if ctx.surface.is_shift_pressed() {
        LAUNCHPAD_COLOR_BLUE_ORCHID
    } else {
        LAUNCHPAD_COLOR_BLUE
    }
}

fn play_state_color(ctx: &Ctx) -> i32 {
    if ctx.surface.is_shift_pressed() {
        return if ctx.surface.is_pressed(ButtonId::Play) { LAUNCHPAD_COLOR_WHITE } else { LAUNCHPAD_COLOR_GREEN_SPRING };
    }
    if ctx.model.transport().is_playing() {
        LAUNCHPAD_COLOR_GREEN
    } else {
        LAUNCHPAD_COLOR_GREEN_LO
    }
}

/// The view color if one of the views is active, otherwise grey.
fn view_state_color(ctx: &Ctx, color: i32, views: &[Views]) -> i32 {
    if ctx.views.is_any_active(views) {
        color
    } else {
        LAUNCHPAD_COLOR_GREY_LO
    }
}

/// The color of a track selection button of the Pro Mk3 (former `getTrackModeColorIndex`).
pub fn track_mode_color_index(ctx: &Ctx, index: usize) -> i32 {
    if ctx.surface.is_pressed(ButtonId::New) {
        return LAUNCHPAD_COLOR_WHITE;
    }

    let track = ctx.model.current_track_bank().item(index);
    if !track.does_exist() {
        return LAUNCHPAD_COLOR_BLACK;
    }

    let Some(active) = ctx.modes.active_id() else {
        return LAUNCHPAD_COLOR_BLACK;
    };
    match active {
        Modes::RecArm => {
            if track.is_rec_arm() {
                LAUNCHPAD_COLOR_RED_HI
            } else {
                LAUNCHPAD_COLOR_RED_LO
            }
        }
        Modes::TrackSelect => {
            if track.is_selected() {
                LAUNCHPAD_COLOR_GREEN_HI
            } else {
                LAUNCHPAD_COLOR_GREEN_LO
            }
        }
        Modes::Mute => {
            if track.is_mute() {
                LAUNCHPAD_COLOR_YELLOW_HI
            } else {
                LAUNCHPAD_COLOR_YELLOW_LO
            }
        }
        Modes::Solo => {
            if track.is_solo() {
                LAUNCHPAD_COLOR_BLUE_HI
            } else {
                LAUNCHPAD_COLOR_BLUE_LO
            }
        }
        Modes::StopClip => {
            if ctx.surface.is_pressed(ButtonId::get(ButtonId::Pad1, index)) {
                LAUNCHPAD_COLOR_RED
            } else {
                LAUNCHPAD_COLOR_ROSE
            }
        }
        _ => ctx.colors.color_index(DawColor::color_id(track.color())),
    }
}

/// The color of a mode selection button (former `getModeColorIndex`).
pub fn mode_color_index(ctx: &Ctx, button: ButtonId) -> i32 {
    let mode_color = |mode: Modes, color: i32| if ctx.modes.is_active(mode) { color } else { LAUNCHPAD_COLOR_GREY_LO };
    let view_color = |view: Views, color: i32| if ctx.views.is_active(view) { color } else { LAUNCHPAD_COLOR_GREY_LO };
    match button {
        ButtonId::RecArm => mode_color(Modes::RecArm, LAUNCHPAD_COLOR_RED),
        ButtonId::Track => mode_color(Modes::TrackSelect, LAUNCHPAD_COLOR_GREEN),
        ButtonId::Mute => mode_color(Modes::Mute, LAUNCHPAD_COLOR_YELLOW),
        ButtonId::Solo => {
            // Pro Mk3 -> color Click
            if lp::has_track_selection_buttons(ctx) && ctx.surface.is_shift_pressed() {
                return if ctx.model.transport().is_metronome_on() { LAUNCHPAD_COLOR_GREEN } else { LAUNCHPAD_COLOR_GREY_LO };
            }
            mode_color(Modes::Solo, LAUNCHPAD_COLOR_BLUE)
        }
        ButtonId::Volume => view_color(Views::TrackVolume, LAUNCHPAD_COLOR_CYAN),
        ButtonId::PanSend => view_color(Views::TrackPan, LAUNCHPAD_COLOR_SKY),
        ButtonId::Sends => view_color(Views::TrackSends, LAUNCHPAD_COLOR_ORCHID),
        ButtonId::StopClip => mode_color(Modes::StopClip, LAUNCHPAD_COLOR_ROSE),
        _ => LAUNCHPAD_COLOR_BLACK,
    }
}

/// The color which represents the active view (former `getViewColor`).
pub fn view_color(ctx: &Ctx) -> i32 {
    let Some(active) = ctx.views.active_id() else {
        return COLOR_VIEW_OFF;
    };
    match active {
        Views::Session | Views::TrackVolume | Views::TrackPan | Views::TrackSends | Views::Mix => COLOR_VIEW_MIXER,
        Views::Play | Views::Chords | Views::Piano | Views::Drum64 => COLOR_VIEW_PLAY,
        Views::Sequencer | Views::PolySequencer | Views::Raindrops => COLOR_VIEW_NOTE_SEQUENCER,
        Views::Drum | Views::Drum4 | Views::Drum8 => COLOR_VIEW_DRUM_SEQUENCER,
        Views::Device | Views::Browser => COLOR_VIEW_DEVICE,
        Views::ClipLength => COLOR_VIEW_CLIP,
        Views::Project => COLOR_VIEW_PROJECT,
        Views::Shift => COLOR_VIEW_SHIFT,
        Views::Tempo => COLOR_VIEW_TEMPO,
        Views::Shuffle => COLOR_VIEW_SHUFFLE,
        Views::User => COLOR_VIEW_USER,
        _ => COLOR_VIEW_OFF,
    }
}
