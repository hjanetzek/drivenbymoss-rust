// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The setup of the Maschine JAM (former `MaschineJamControllerSetup`).
//!
//! Differences to Java:
//! - The observers are replaced by [`ControllerSetup::handle_changes`], which drains the model
//!   events and the changed settings.
//! - `layoutControls` (the bounds of the simulator GUI) is not ported.
//! - The fader touch commands are not set (the Java version binds the no-op command); the touch
//!   messages are bound, so that they are not reported as unhandled.
//! - The selection observer of the effect track bank is not registered (the Rust model creates
//!   the events for the current track bank only).

use std::rc::Rc;

use super::command::*;
use super::mode::*;
use super::surface::{self as jam, FaderConfig, JamExt};
use super::view::accent_view::AccentView;
use super::view::drum_views::{Drum64View, DrumLaneView, DrumView};
use super::view::note_repeat_view::NoteRepeatView;
use super::view::play_views::{ChordsView, PlayView};
use super::view::sequencer_views::{PolySequencerView, RaindropsView, SequencerView};
use super::view::session_view::SessionView;
use super::view::shift_view::ShiftView;
use super::view::utility_views::{BrowserView, NumberView};
use crate::backend::Sender;
use crate::config::values::REAPER_ARP_MODES;
use crate::config::{Configuration, SettingId};
use crate::controller::maschine::core::colors::{self as colors, create_color_manager};
use crate::controller::maschine::core::command::group_button::GroupButtonCommand;
use crate::controller::maschine::core::command::note_repeat::FillModeNoteRepeatCommand;
use crate::controller::maschine::core::encoder_mode_manager::{EncoderMode, EncoderModeCommand, MaschineMonitorEncoderCommand};
use crate::controller::maschine::core::maschine::Maschine;
use crate::controller::maschine::core::surface::{create_pad_grid, flush_pad_grid, handle_sysex};
use crate::controller::{ControllerSetup, InstanceParts, MidiPorts};
use crate::daw::constants::GrooveParameterId;
use crate::daw::events::ModelEvent;
use crate::daw::model::Model;
use crate::daw::model_setup::ModelSetup;
use crate::framework::color::{BUTTON_STATE_HI, BUTTON_STATE_ON};
use crate::framework::command::{shared, FnTrigger, SharedTrigger};
use crate::framework::commands::misc::FootswitchCommand;
use crate::framework::commands::mode::KnobRowModeCommand;
use crate::framework::commands::view::{ToggleShiftViewCommand, ViewMultiSelectCommand};
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, ContinuousId, Modes, Views};
use crate::framework::surface::{ContinuousKind, Surface};
use crate::framework::value_changer::ValueChanger;
use crate::framework::view::transpose_view::TransposeView;
use crate::host::Host;
use crate::midi::{BindType, ButtonEvent, MidiInput, MidiOutput};
use crate::scale::Scales;

/// The views selected by the Step button.
pub const SEQUENCER_VIEWS: [Views; 6] = [Views::Sequencer, Views::PolySequencer, Views::Raindrops, Views::Drum, Views::Drum4, Views::Drum8];

/// The views selected by the Pad Mode button.
pub const PLAY_VIEWS: [Views; 4] = [Views::Play, Views::Chords, Views::Piano, Views::Drum64];

/// The environment variable which allows to start the controller on Linux (for tests).
pub const ALLOW_ON_LINUX: &str = "DBM_ALLOW_MASCHINE_ON_LINUX";

/// Create a Maschine JAM instance.
pub fn create(host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
    if cfg!(target_os = "linux") && std::env::var_os(ALLOW_ON_LINUX).is_none() {
        return Err("Maschine Jam is not supported on Linux since there is no Native Instruments DAW Integration Host.".to_string());
    }

    let value_changer = ValueChanger::twos_complement(128, 1);
    let colors = create_color_manager();

    let scales = Scales::new(&value_changer, 36, 100, 8, 8);
    let mut model_setup = ModelSetup::new();
    model_setup.enable_drum64_device();
    let mut model = Model::new(host.clone(), sender.clone(), model_setup, value_changer, scales);
    model.track_bank_mut().set_indication(true);

    let output = MidiOutput::new(sender.clone(), Some(ports.output));
    let input = MidiInput::new(sender.clone(), Some(ports.input), &["80????", "90????"])?;
    let surface = Surface::new(output, input, Some(create_pad_grid(&colors, 8, 8)));

    let mut ctx = Ctx::new(host, sender, model, configuration, surface, colors);
    ctx.ext = Box::new(JamExt::new());
    ctx.sysex_handler = Some(Rc::new(|ctx: &mut Ctx, data: &str| handle_sysex(ctx, Maschine::Jam, data)));
    ctx.create_pads();

    Ok(InstanceParts { ctx, setup: Box::new(MaschineJamSetup) })
}

/// Create the configuration with all settings of the Maschine JAM.
pub fn create_configuration() -> Configuration {
    crate::config::maschine_jam::MaschineJamConfiguration::for_reaper().into_base()
}

/// The Maschine JAM controller setup.
#[derive(Default)]
pub struct MaschineJamSetup;

impl ControllerSetup for MaschineJamSetup {
    fn init(&mut self, ctx: &mut Ctx) {
        create_modes(ctx);
        create_views(ctx);
        register_trigger_commands(ctx);
        register_continuous_commands(ctx);
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        let mode = ctx.config.startup_mode();
        ctx.set_active_mode(mode);
        let view = if ctx.config.should_start_with_session_view() { Views::Session } else { ctx.config.startup_view() };
        ctx.set_active_view(view);
    }

    fn flush(&mut self, ctx: &mut Ctx) {
        ctx.flush_surface();
        flush_pad_grid(ctx);
        update_faders(ctx);

        // Update the VU meters of the touch strips
        let is_selected_track = jam::encoder_manager(ctx).borrow().is_active_encoder_mode(EncoderMode::SelectedTrackVolume);
        let (vu_left, vu_right) = {
            let track = if is_selected_track { ctx.model.track_bank().selected_item() } else { Some(ctx.model.master_track()) };
            track.map(|t| (t.vu_left(), t.vu_right())).unwrap_or((0, 0))
        };
        let value_changer = ctx.model.value_changer();
        let (vu_left, vu_right) = (value_changer.to_midi_value(vu_left), value_changer.to_midi_value(vu_right));
        ctx.surface.output.send_cc(jam::STRIP_LEFT, vu_left);
        ctx.surface.output.send_cc(jam::STRIP_RIGHT, vu_right);
    }

    fn handle_changes(&mut self, ctx: &mut Ctx) {
        // Model events (former track bank selection observer and the observers of the views)
        let events = ctx.model.take_events();
        ctx.dispatch_model_events_to_views(&events);
        for event in &events {
            if let ModelEvent::TrackSelectionChanged { is_selected, .. } = event {
                handle_track_change(ctx, *is_selected);
                let manager = jam::encoder_manager(ctx);
                manager.borrow_mut().handle_track_change(ctx, *is_selected);
            }
        }

        // Setting observers
        let changed = ctx.config.take_changed_settings();
        for id in &changed {
            handle_setting_change(ctx, *id);
        }
        ctx.dispatch_setting_changes_to_views(&changed);

        // Browser observer
        let browser_active = ctx.model.browser().is_some_and(|browser| browser.is_active());
        if browser_active != jam::ext(ctx).browser_active {
            jam::ext_mut(ctx).browser_active = browser_active;
            let manager = jam::encoder_manager(ctx);
            if browser_active {
                ctx.set_temporary_view(Views::Browser);
                manager.borrow_mut().enable_temporary_encode_mode(ctx, EncoderMode::TemporaryBrowser);
            } else {
                ctx.restore_view();
                manager.borrow_mut().disable_temporary_encode_mode(ctx);
            }
        }
    }

    fn exit(&mut self, ctx: &mut Ctx) {
        // Former internalShutdown
        jam::setup_faders(ctx, &jam::EMPTY);
        if let Some(grid) = &mut ctx.surface.pad_grid {
            grid.turn_off();
        }
        ctx.turn_off_lights();
        flush_pad_grid(ctx);
    }
}

// ------------------------------------------------------------------------------------------
// Modes and views

/// Register the modes (former `createModes`).
pub fn create_modes(ctx: &mut Ctx) {
    let mode = MaschineJamTrackMode::new(ctx);
    ctx.register_mode(Modes::Track, mode);
    let mode = MaschineJamVolumeMode::new(ctx);
    ctx.register_mode(Modes::Volume, mode);
    let mode = MaschineJamPanMode::new(ctx);
    ctx.register_mode(Modes::Pan, mode);
    for i in 0..8 {
        let mode = MaschineJamSendMode::new(i, ctx);
        ctx.register_mode(Modes::get(Modes::Send1, i), mode);
    }
    let mode = MaschineJamParameterMode::new(ctx);
    ctx.register_mode(Modes::DeviceParams, mode);
    let mode = MaschineJamUserMode::new(ctx);
    ctx.register_mode(Modes::User, mode);
    ctx.modes.set_default_id(Modes::Volume);
}

/// Register the views (former `createViews`).
pub fn create_views(ctx: &mut Ctx) {
    let view = ShiftView::new(ctx);
    ctx.register_view(Views::Shift, view);
    let view = NoteRepeatView::new(ctx);
    ctx.register_view(Views::RepeatNote, view);
    let view = SessionView::new(ctx);
    ctx.register_view(Views::Session, view);
    let view = PlayView::new(ctx);
    ctx.register_view(Views::Play, view);
    let view = ChordsView::new(ctx);
    ctx.register_view(Views::Chords, view);
    let view = PlayView::new_piano(ctx);
    ctx.register_view(Views::Piano, view);
    let view = Drum64View::new(ctx);
    ctx.register_view(Views::Drum64, view);
    let view = DrumView::new(ctx);
    ctx.register_view(Views::Drum, view);
    let view = DrumLaneView::new_drum4(ctx);
    ctx.register_view(Views::Drum4, view);
    let view = DrumLaneView::new_drum8(ctx);
    ctx.register_view(Views::Drum8, view);
    let view = RaindropsView::new(ctx);
    ctx.register_view(Views::Raindrops, view);
    let view = SequencerView::new(ctx);
    ctx.register_view(Views::Sequencer, view);
    let view = PolySequencerView::new(ctx);
    ctx.register_view(Views::PolySequencer, view);
    let view = NumberView::new_tempo(ctx, colors::COLOR_BLUE, colors::COLOR_WHITE, colors::COLOR_BLACK);
    ctx.register_view(Views::Tempo, view);
    let view = NumberView::new_shuffle(ctx, colors::COLOR_PINK, colors::COLOR_WHITE, colors::COLOR_BLACK);
    ctx.register_view(Views::Shuffle, view);
    let view = BrowserView::new(ctx);
    ctx.register_view(Views::Browser, view);
    let view = AccentView::new(ctx);
    ctx.register_view(Views::Control, view);
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
fn handle_track_change(ctx: &mut Ctx, is_selected: bool) {
    if !is_selected {
        return;
    }

    // Former recallLastView
    if !ctx.views.is_any_active(&[Views::Session, Views::Mix, Views::Project]) {
        ctx.recall_preferred_view();
    }

    // Former resetDrumOctave
    let Some(view) = ctx.active_view() else {
        return;
    };
    let Ok(mut view) = view.try_borrow_mut() else {
        return;
    };
    if ctx.views.is_any_active(&[Views::Drum, Views::Play]) {
        view.update_note_mapping(ctx);
    } else if let Some(drum64) = view.as_any_mut().downcast_mut::<Drum64View>() {
        drum64.reset_octave(ctx);
    }
    // Former updateMode: there is no master mode
}

/// Update the touch strips from the active mode (former `updateFaders`).
fn update_faders(ctx: &mut Ctx) {
    let configs: Option<[FaderConfig; 8]> = setup_faders_of_active_mode(ctx);
    if let Some(configs) = configs {
        jam::setup_faders(ctx, &configs);
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

fn bool_supplier(f: impl Fn(&Ctx) -> bool + 'static) -> Option<Supplier> {
    Some(Rc::new(move |ctx: &Ctx| on_hi(ctx, f(ctx))))
}

/// Add a button which is bound to a MIDI CC on channel 1 (former `addButton`).
fn add_button(ctx: &mut Ctx, id: ButtonId, label: &str, command: Option<SharedTrigger>, midi_control: i32, supplier: Option<Supplier>) {
    // The default supplier (pressed state) is mapped to the button colors as well
    let supplier = supplier.or_else(|| bool_supplier(move |ctx: &Ctx| ctx.surface.is_pressed(id)));
    ctx.add_button(id, label, command, BindType::Cc, 0, midi_control, supplier);
}

/// Register the commands of all buttons (former `registerTriggerCommands`).
pub fn register_trigger_commands(ctx: &mut Ctx) {
    add_button(ctx, ButtonId::Shift, "SHIFT", Some(shared(ToggleShiftViewCommand::new())), -1, None);
    add_button(ctx, ButtonId::Select, "SELECT", Some(shared(MaschineJamSelectCommand)), jam::SELECT, None);
    add_button(ctx, ButtonId::Delete, "CLEAR", None, jam::CLEAR, None);
    add_button(ctx, ButtonId::Duplicate, "DUPLICATE", Some(shared(MaschineJamDoubleCommand::new())), jam::DUPLICATE, None);
    add_button(ctx, ButtonId::Play, "PLAY", Some(shared(MaschineJamPlayCommand::new())), jam::PLAY, bool_supplier(|ctx| ctx.model.transport().is_playing()));
    add_button(
        ctx,
        ButtonId::Record,
        "REC",
        Some(shared(MaschineJamRecordCommand::new())),
        jam::RECORD,
        bool_supplier(|ctx| crate::framework::commands::transport::ConfiguredRecordCommand::is_lit(ctx, ctx.surface.is_shift_pressed())),
    );
    add_button(ctx, ButtonId::PageLeft, "PAGE LEFT", Some(shared(MaschineJamPageCommand::new_left())), jam::LEFT, None);
    add_button(ctx, ButtonId::PageRight, "PAGE RIGHT", Some(shared(MaschineJamPageCommand::new_right())), jam::RIGHT, None);
    add_button(ctx, ButtonId::Solo, "SOLO", Some(shared(MaschineJamSoloCommand)), jam::SOLO, None);
    add_button(ctx, ButtonId::Mute, "MUTE", Some(shared(MaschineJamMuteCommand)), jam::MUTE, None);

    add_button(
        ctx,
        ButtonId::Repeat,
        "NOTE REPEAT",
        Some(shared(FillModeNoteRepeatCommand::new(false))),
        jam::NOTE_REPEAT,
        bool_supplier(|ctx| ctx.config.is_note_repeat_active()),
    );

    add_button(ctx, ButtonId::Track, "MACRO", Some(shared(MaschineJamMacroCommand::new())), jam::MACRO, bool_supplier(|ctx| ctx.modes.is_active(Modes::Track)));
    add_button(
        ctx,
        ButtonId::Volume,
        "LEVEL",
        Some(shared(MaschineJamLevelCommand::new())),
        jam::LEVEL,
        bool_supplier(|ctx| ctx.modes.is_any_active(&[Modes::Volume, Modes::Pan])),
    );
    add_button(
        ctx,
        ButtonId::Sends,
        "AUX",
        Some(shared(MaschineJamAuxCommand::new())),
        jam::AUX,
        bool_supplier(|ctx| ctx.modes.active_id().is_some_and(crate::framework::commands::mode::is_send_mode)),
    );
    add_button(
        ctx,
        ButtonId::Device,
        "CONTROL",
        Some(shared(MaschineJamControlCommand::new())),
        jam::CONTROL,
        bool_supplier(|ctx| ctx.modes.is_any_active(&[Modes::DeviceParams, Modes::User])),
    );
    add_button(
        ctx,
        ButtonId::Automation,
        "AUTO",
        Some(shared(MaschineJamAutomationCommand::new())),
        jam::AUTO,
        bool_supplier(crate::framework::commands::transport_ext::AutomationCommand::is_lit),
    );

    for i in 0..8 {
        let label = ((b'A' + i as u8) as char).to_string();
        add_button(
            ctx,
            ButtonId::get(ButtonId::RowSelect1, i),
            &label,
            Some(shared(GroupButtonCommand::new(i))),
            jam::GROUP_A + i as i32,
            Some(Rc::new(move |ctx: &Ctx| GroupButtonCommand::button_color(ctx, i))),
        );
    }

    add_button(ctx, ButtonId::Session, "SONG", Some(shared(MaschineJamSessionViewCommand)), jam::SONG, bool_supplier(|ctx| ctx.views.is_active(Views::Session)));
    add_button(
        ctx,
        ButtonId::Sequencer,
        "STEP",
        Some(shared(ViewMultiSelectCommand::with_options(&SEQUENCER_VIEWS, true, ButtonEvent::Up, false))),
        jam::STEP,
        bool_supplier(|ctx| ctx.views.is_any_active(&SEQUENCER_VIEWS)),
    );
    add_button(
        ctx,
        ButtonId::Note,
        "PAD MODE",
        Some(shared(ViewMultiSelectCommand::with_options(&PLAY_VIEWS, true, ButtonEvent::Up, false))),
        jam::PAD_MODE,
        bool_supplier(|ctx| ctx.views.is_any_active(&PLAY_VIEWS)),
    );

    for i in 0..8 {
        add_button(
            ctx,
            ButtonId::get(ButtonId::Scene1, i),
            &format!("SCENE {i}"),
            Some(shared(MaschineJamStartSceneCommand::new(i))),
            jam::SCENE1 + i as i32,
            Some(Rc::new(move |ctx: &Ctx| MaschineJamStartSceneCommand::button_color(ctx, i))),
        );
    }

    let arrows = [
        (ButtonId::ArrowLeft, "LEFT", jam::NAV_LEFT),
        (ButtonId::ArrowRight, "RIGHT", jam::NAV_RIGHT),
        (ButtonId::ArrowUp, "UP", jam::NAV_UP),
        (ButtonId::ArrowDown, "DOWN", jam::NAV_DOWN),
    ];
    for (id, label, control) in arrows {
        add_button(
            ctx,
            id,
            label,
            Some(shared(MaschineJamViewButtonCommand::new(id))),
            control,
            bool_supplier(move |ctx| MaschineJamViewButtonCommand::can_scroll(ctx, id)),
        );
    }

    add_button(
        ctx,
        ButtonId::Browse,
        "Browser",
        Some(shared(MaschineJamBrowserCommand::new())),
        jam::BROWSE,
        bool_supplier(|ctx| ctx.model.browser().is_some_and(|b| b.is_active())),
    );

    add_button(ctx, ButtonId::Footswitch1, "Foot Controller (Tip)", Some(shared(FootswitchCommand::new(0))), jam::FOOTSWITCH_TIP, None);
    add_button(ctx, ButtonId::Footswitch2, "Foot Controller (Ring)", Some(shared(FootswitchCommand::new(1))), jam::FOOTSWITCH_RING, None);
}

/// Register the faders and the encoder (former `registerContinuousCommands`).
pub fn register_continuous_commands(ctx: &mut Ctx) {
    for i in 0..8 {
        let id = ContinuousId::get(ContinuousId::Fader1, i);
        ctx.add_continuous(
            id,
            &format!("Fader {}", i + 1),
            ContinuousKind::Fader,
            Rc::new(std::cell::RefCell::new(KnobRowModeCommand::new(i))),
            BindType::Cc,
            0,
            jam::FADER1 + i as i32,
        );
        // The touch is bound to the no-op command
        ctx.surface.input.bind_touch(id, BindType::Cc, 0, jam::FADER_TOUCH1 + i as i32);
    }

    let manager = jam::encoder_manager(ctx);
    manager.borrow_mut().set_change_option_handler(Rc::new(super::view::change_option));
    ctx.add_continuous(
        ContinuousId::MasterKnob,
        "Encoder",
        ContinuousKind::RelativeKnob,
        Rc::new(std::cell::RefCell::new(EncoderModeCommand::new(manager.clone()))),
        BindType::Cc,
        0,
        jam::KNOB_TURN,
    );

    let monitor_buttons = [
        (ButtonId::Mastertrack, "MST", EncoderMode::MasterVolume, jam::MASTER),
        (ButtonId::Alt, "GRP", EncoderMode::PlayPosition, jam::GROUP),
        (ButtonId::Metronome, "IN 1", EncoderMode::MetronomeVolume, jam::IN),
        (ButtonId::Mixer, "HEADPHONE", EncoderMode::CueVolume, jam::HEADPHONE),
    ];
    for (id, label, encoder_mode, control) in monitor_buttons {
        let lit_manager = manager.clone();
        add_button(
            ctx,
            id,
            label,
            Some(shared(MaschineMonitorEncoderCommand::new(manager.clone(), encoder_mode))),
            control,
            bool_supplier(move |_ctx| lit_manager.borrow().is_lit(encoder_mode)),
        );
    }

    manager.borrow_mut().set_active_encoder_mode(ctx, EncoderMode::MasterVolume);

    let push_manager = manager.clone();
    add_button(
        ctx,
        ButtonId::Enter,
        "ENC_PRESS",
        Some(shared(FnTrigger(move |ctx: &mut Ctx, event: ButtonEvent, _velocity: i32| {
            if event != ButtonEvent::Down {
                return;
            }
            if ctx.model.browser().is_some_and(|b| b.is_active()) {
                if let Some(browser) = ctx.model.browser_mut() {
                    browser.stop_browsing(true);
                }
            } else {
                push_manager.borrow_mut().toggle_function(ctx);
            }
        }))),
        jam::KNOB_PUSH,
        None,
    );
    add_button(ctx, ButtonId::MastertrackTouch, "ENC_TOUCH", None, jam::KNOB_TOUCH, None);
    add_button(ctx, ButtonId::TapTempo, "TEMPO", Some(shared(MaschineJamTapTempoCommand::new(manager.clone()))), jam::TEMPO, None);
    add_button(
        ctx,
        ButtonId::Accent,
        "SWING",
        Some(shared(MaschineJamSwingCommand::new(manager.clone()))),
        jam::SWING,
        bool_supplier(|ctx| ctx.model.groove().parameter(GrooveParameterId::Enabled).value() > 0),
    );
    add_button(ctx, ButtonId::Groove, "GRID", Some(shared(MaschineJamQuantizeCommand::new())), jam::GRID, None);

    let view_buttons = [
        (ButtonId::Row11, "PERFORM", EncoderMode::TemporaryPerform, jam::PERFORM),
        (ButtonId::Row12, "NOTES", EncoderMode::TemporaryNotes, jam::NOTES),
        (ButtonId::Row13, "LOCK", EncoderMode::TemporaryLock, jam::LOCK),
        (ButtonId::Row14, "TUNE", EncoderMode::TemporaryTune, jam::TUNE),
    ];
    for (id, label, encoder_mode, control) in view_buttons {
        let supplier = if encoder_mode == EncoderMode::TemporaryLock { bool_supplier(|ctx| ctx.config.is_accent_active()) } else { None };
        add_button(ctx, id, label, Some(shared(MaschineJamViewCommand::new(manager.clone(), encoder_mode))), control, supplier);
    }
}
