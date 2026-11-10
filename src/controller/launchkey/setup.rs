// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The setup of the Launchkey Mk3 and Mini Mk3 (former `LaunchkeyMk3ControllerSetup` and
//! `LaunchkeyMiniMk3ControllerSetup`).
//!
//! Differences to Java:
//! - The observers are replaced by [`ControllerSetup::handle_changes`].
//! - The keyboard port is the further MIDI input of the instance
//!   (`ControllerFactory::num_extra_midi_inputs`); its note input passes the keys, the wheels and
//!   the pedal to the host. The user knobs of the Mini, which send on the keyboard port, are
//!   handled in `handle_extra_input_midi`.
//! - The lights of the view, mode and fader mode selection pads (the `LED*` outputs) are not
//!   created: they only exist for the simulator (they send nothing).
//! - A view which switches the view while it handles a button defers the switch until it is
//!   done (`surface::after_view`).
//! - `layoutControls` (the bounds of the simulator GUI) is not ported.

use std::cell::RefCell;
use std::rc::Rc;

use super::colors::create_color_manager;
use super::command::{ButtonAreaCommand, DeviceLockCommand, LaunchkeyMk3PlayCommand, SelectPrevNextTrackCommand, SELECTED_FLAG};
use super::definition::*;
use super::display::LaunchkeyMk3Display;
use super::mode::{CustomMode, MiniProjectParamsMode, MiniTrackMode, Mk3Mode};
use super::surface::{self as lk, state, state_mut, LaunchkeyState};
use super::view::drum::{DrumConfigView, DrumView};
use super::view::misc::{BrowserView, DeviceConfigView, DummyView, UserPadView};
use super::view::pad_mode_select::PadModeSelectView;
use super::view::session::SessionView;
use crate::backend::Sender;
use crate::config::Configuration;
use crate::controller::{ControllerSetup, InstanceParts, MidiPorts};
use crate::daw::model::Model;
use crate::daw::model_setup::ModelSetup;
use crate::framework::color::{BUTTON_STATE_HI, BUTTON_STATE_ON};
use crate::framework::command::{shared, FnTrigger, SharedTrigger, TriggerCommand};
use crate::framework::commands::application::UndoCommand;
use crate::framework::commands::clip::{NewCommand, QuantizeCommand};
use crate::framework::commands::cursor::{CursorCommand, Direction};
use crate::framework::commands::general::NopCommand;
use crate::framework::commands::mode::ModeSelectCommand;
use crate::framework::commands::transport::{ConfiguredRecordCommand, MetronomeCommand, PlayCommand};
use crate::framework::commands::transport_ext::{StopCommand, ToggleLoopCommand};
use crate::framework::commands::view::ViewMultiSelectCommand;
use crate::framework::ctx::{Ctx, MAIN_MODE_MANAGER};
use crate::framework::ids::{ButtonId, ContinuousId, Modes, Views};
use crate::framework::mode::device::ParameterMode;
use crate::framework::mode::parameter_mode::{default_knob_ids, sequential_controls};
use crate::framework::mode::track::{TrackPanMode, TrackSendMode, TrackVolumeMode};
use crate::framework::parameterprovider::ParamRef;
use crate::framework::setup_helpers::{self, BrowserObserver};
use crate::framework::surface::{ContinuousKind, LightSupplier};
use crate::framework::value_changer::ValueChanger;
use crate::host::Host;
use crate::midi::{BindType, ButtonEvent, DeviceInquiry, MidiInput, MidiOutput, CMD_CC, CMD_PROGRAM_CHANGE};
use crate::scale::Scales;

const NAME_CUSTOM_1: &str = "Custom 1";
const NAME_CUSTOM_2: &str = "Custom 2";
const NAME_CUSTOM_3: &str = "Custom 3";
const NAME_CUSTOM_4: &str = "Custom 4";

/// The drum matrix (former `DRUM_MATRIX`).
#[rustfmt::skip]
const DRUM_MATRIX: [i32; 64] = [
     0,  1,  2,  3,  8,  9, 10, 11,
     4,  5,  6,  7, 12, 13, 14, 15,
    -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -1,
];

/// The note input filters of the DAW port: notes on all channels but 16 (former "Pads" input).
fn pad_filters() -> Vec<String> {
    let mut filters = Vec::with_capacity(30);
    for i in 0..15 {
        filters.push(format!("8{i:X}????"));
        filters.push(format!("9{i:X}????"));
    }
    filters
}

/// The note input filters of the keyboard port (former "Keyboard" input).
fn keyboard_filters(model: LaunchkeyModel) -> &'static [&'static str] {
    match model {
        LaunchkeyModel::Mk3 => &[
            "8?????", // Note off
            "9?????", // Note on
            "A?????", // Polyphonic After-touch
            "B?01??", // Modulation
            "C?????", // Program change
            "B?40??", // Sustain pedal
            "D?????", // Channel After-touch
            "E?????", // Pitch-bend
        ],
        LaunchkeyModel::MiniMk3 => &[
            "8?????", // Note off
            "9?????", // Note on
            "B?01??", // Modulation
            "B?40??", // Sustainpedal
            "E?????", // Pitchbend
        ],
    }
}

/// The faders of the Mk3 (former `DEFAULT_FADER_IDS`).
fn fader_ids() -> Vec<ContinuousId> {
    sequential_controls(ContinuousId::Fader1, 8)
}

/// Create a Launchkey instance. `keys_input` is the device ID of the keyboard port, if found.
pub fn create(
    model: LaunchkeyModel,
    host: Host,
    sender: Sender,
    ports: MidiPorts,
    keys_input: Option<i32>,
    configuration: Configuration,
) -> Result<InstanceParts, String> {
    let colors = create_color_manager(model.is_mini());
    let value_changer = ValueChanger::twos_complement(128, 1);

    // createScales
    let mut scales = Scales::new(&value_changer, 36, 52, 8, 2);
    scales.set_drum_matrix(DRUM_MATRIX.to_vec());
    scales.set_drum_note_end(52);

    // createModel
    let mut model_setup = ModelSetup::new();
    model_setup.set_has_flat_track_list(true);
    model_setup.set_has_full_flat_track_list(configuration.are_master_tracks_included());
    if !model.is_mini() {
        model_setup.num_param_pages = 16;
    }
    model_setup.set_num_scenes(2);
    model_setup.set_num_sends(if model.is_mini() { 8 } else { 2 });
    let mut daw_model = Model::new(host.clone(), sender.clone(), model_setup, value_changer, scales);
    daw_model.track_bank_mut().set_indication(true);

    // createSurface
    let output = MidiOutput::new(sender.clone(), Some(ports.output));
    let filters = pad_filters();
    let filters: Vec<&str> = filters.iter().map(String::as_str).collect();
    let input = MidiInput::new(sender.clone(), Some(ports.input), &filters)?;
    let keys = match keys_input {
        Some(id) => Some(MidiInput::new(sender.clone(), Some(id), keyboard_filters(model))?),
        None => None,
    };
    let surface = crate::framework::surface::Surface::new(output.clone(), input, Some(lk::create_pad_grid(&colors)));

    let mut ctx = Ctx::new(host, sender, daw_model, configuration, surface, colors);
    let mut launchkey_state = LaunchkeyState::new(model);
    launchkey_state.keys_input = keys;
    if !model.is_mini() {
        // The pad grid starts with the session view (former createPads)
        launchkey_state.pad_view = Some(Views::Session);
    }
    ctx.ext = Box::new(launchkey_state);
    if !model.is_mini() {
        let manager = ctx.add_mode_manager();
        state_mut(&mut ctx).fader_mode_manager = manager;
        let (display, is_88_key_version) = LaunchkeyMk3Display::create(output);
        ctx.text_display = Some(display);
        state_mut(&mut ctx).is_88_key_version = is_88_key_version;
    } else {
        ctx.midi_handler = Some(Rc::new(handle_mini_midi));
    }
    ctx.sysex_handler = Some(Rc::new(lk::handle_sysex));
    lk::create_pads(&mut ctx);
    ctx.surface.output.send_sysex(&DeviceInquiry::QUERY);

    Ok(InstanceParts { ctx, setup: Box::new(LaunchkeySetup::default()) })
}

/// The Launchkey controller setup.
#[derive(Default)]
pub struct LaunchkeySetup {
    browser_observer: BrowserObserver,
}

impl ControllerSetup for LaunchkeySetup {
    fn init(&mut self, ctx: &mut Ctx) {
        if lk::is_mini(ctx) {
            create_mini_modes(ctx);
            create_mini_views(ctx);
            register_mini_trigger_commands(ctx);
        } else {
            create_mk3_modes(ctx);
            create_mk3_views(ctx);
            register_mk3_trigger_commands(ctx);
        }
        register_continuous_commands(ctx);
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        if lk::is_mini(ctx) {
            lk::set_launchkey_to_daw(ctx, true);
            lk::set_pad_view(ctx, Views::Session);
            ctx.set_active_view(Views::Session);
            // Note: the mode cannot be changed from the DAW!
            ctx.set_active_mode(Modes::Volume);
            lk::set_knob_mode(ctx, KNOB_MODE_VOLUME);
            lk::set_pad_mode(ctx, PAD_MODE_SESSION);
            return;
        }

        lk::set_pad_view(ctx, Views::Session);
        ctx.set_active_view(Views::Session);
        // Note: the mode cannot be changed from the DAW!
        ctx.set_active_mode(Modes::Pan);
        let fader_mode_manager = lk::fader_mode_manager(ctx);
        ctx.set_active_mode_in(fader_mode_manager, Modes::Volume);
        lk::set_knob_mode(ctx, KNOB_MODE_VOLUME);
        lk::set_pad_mode(ctx, PAD_MODE_SESSION);

        // Switch the Launchkey to DAW mode and wait until it is ready
        ctx.schedule(200, |ctx| {
            lk::set_launchkey_to_daw(ctx, true);
            wait_for_connection(ctx);
        });
    }

    fn flush(&mut self, ctx: &mut Ctx) {
        if lk::is_mini(ctx) {
            ctx.flush_surface();
            return;
        }
        if !(state(ctx).is_ready && lk::is_daw_connected(ctx)) {
            return;
        }
        flush_mk3(ctx);
    }

    fn handle_changes(&mut self, ctx: &mut Ctx) {
        lk::run_after_view(ctx);

        // Model events (the observers of the views)
        let events = ctx.model.take_events();
        ctx.dispatch_model_events_to_views(&events);

        // Setting observers (former createScaleObservers and registerDeactivatedItemsHandler)
        let changed = ctx.config.take_changed_settings();
        for id in &changed {
            if !setup_helpers::handle_scale_setting(ctx, *id) {
                setup_helpers::handle_deactivated_items_setting(ctx, *id);
            }
        }
        ctx.dispatch_setting_changes_to_views(&changed);

        // Former activateBrowserObserver (Views.BROWSER), Mk3 only
        if !lk::is_mini(ctx) {
            self.browser_observer.check_view(ctx, Views::Browser);
        }
    }

    fn exit(&mut self, ctx: &mut Ctx) {
        // Former internalShutdown: only leave the DAW mode
        ctx.surface.is_shutting_down = true;
        lk::set_launchkey_to_daw(ctx, false);
    }

    fn handle_extra_input_midi(&mut self, ctx: &mut Ctx, input: usize, data: &[u8]) {
        if input != 0 || data.len() != 3 || !lk::is_mini(ctx) {
            return;
        }
        // The knobs of the Mini in the custom (user) knob mode send on the keyboard port on MIDI
        // channel 1, they are mapped to the parameters of the user mode
        if data[0] as i32 != CMD_CC {
            return;
        }
        let control = data[1] as i32;
        if !(LAUNCHKEY_KNOB_1..LAUNCHKEY_KNOB_1 + 8).contains(&control) {
            return;
        }
        let id = ContinuousId::get(ContinuousId::DeviceKnob1, (control - LAUNCHKEY_KNOB_1) as usize);
        let parameter = ctx.surface.continuous(id).and_then(|c| c.parameter.clone());
        if let Some(parameter) = parameter {
            parameter.set_normalized_value(ctx, data[2] as f64 / 127.0);
        }
    }
}

/// Former `waitForConnection`: synchronize the modes when the device is in DAW mode.
fn wait_for_connection(ctx: &mut Ctx) {
    if lk::is_daw_connected(ctx) {
        // Sync modes to device
        let output = &ctx.surface.output;
        output.send_cc_ex(15, LAUNCHKEY_MODE_SELECT, KNOB_MODE_PAN);
        output.send_cc_ex(15, LAUNCHKEY_FADER_SELECT, FADER_MODE_VOLUME);
        output.send_cc_ex(15, LAUNCHKEY_VIEW_SELECT, PAD_MODE_SESSION);
        ctx.schedule(200, |ctx| state_mut(ctx).is_ready = true);
        return;
    }
    ctx.schedule(200, wait_for_connection);
}

/// The flush of the Mk3 (former `AbstractControlSurface.flush` with the `updateViewControls` of
/// the Mk3, which also updates the display of the fader mode).
fn flush_mk3(ctx: &mut Ctx) {
    if ctx.surface.is_shutting_down {
        return;
    }
    let view = ctx.active_view();
    if let Some(view) = &view {
        if let Ok(mut view) = view.try_borrow_mut() {
            view.update_control_surface(ctx);
        }
    }
    if lk::has_faders(ctx) {
        let manager = lk::fader_mode_manager(ctx);
        if let Some(mode) = ctx.active_mode_in(manager) {
            if let Ok(mut mode) = mode.try_borrow_mut() {
                mode.update_display(ctx);
            }
        }
    }
    if let Some(view) = &view {
        if let Ok(mut view) = view.try_borrow_mut() {
            view.draw_grid(ctx);
        }
    }
    ctx.flush_lights();
    if let Some(display) = &mut ctx.text_display {
        display.flush();
    }
}

/// Handle the MIDI of the DAW port which is not bound to a control: the program changes of the
/// Mini select the page of the active mode (former `handleProgramChange`).
fn handle_mini_midi(ctx: &mut Ctx, status: i32, data1: i32, _data2: i32) {
    match status & 0xF0 {
        CMD_PROGRAM_CHANGE => {
            let last = state(ctx).last_prg_change;
            let mut is_inc = last - data1 < 0;
            if last == 127 && data1 == 0 {
                is_inc = true;
            } else if last == 0 && data1 == 127 {
                is_inc = false;
            }
            state_mut(ctx).last_prg_change = data1;
            process_program_change_action(ctx, if is_inc { 1 } else { -1 });
        }
        CMD_CC => ctx.host.error(&format!("CC {data1} on MIDI channel {} should be handled in framework...", status & 0xF)),
        _ => {}
    }
}

/// Former `processProgramChangeAction` of the Mini.
fn process_program_change_action(ctx: &mut Ctx, value: i32) {
    let Some(mode_id) = ctx.modes.active_id() else {
        return;
    };
    match mode_id {
        Modes::Volume | Modes::Pan | Modes::Send1 | Modes::Send2 => {
            let tb = ctx.model.current_track_bank_mut();
            if value > 0 {
                tb.select_next_page();
            } else {
                tb.select_previous_page();
            }
            crate::framework::commands::mv_helper::notify_selected_track(ctx);
        }
        Modes::DeviceParams => {
            if let Some(bank) = ctx.model.cursor_device_mut().parameter_bank_mut() {
                if value > 0 {
                    bank.select_next_item();
                } else {
                    bank.select_previous_item();
                }
            }
            crate::framework::commands::mv_helper::notify_selected_parameter_page(ctx);
        }
        Modes::User => {
            let Some(mode) = ctx.modes.get(Modes::User) else {
                return;
            };
            let Ok(mut mode) = mode.try_borrow_mut() else {
                return;
            };
            if let Some(mode) = mode.as_any_mut().downcast_mut::<MiniProjectParamsMode>() {
                if value > 0 {
                    mode.select_next_parameter_page(ctx);
                } else {
                    mode.select_previous_parameter_page(ctx);
                }
                mode.notify_page(ctx);
            }
        }
        _ => {}
    }
}

// ------------------------------------------------------------------------------------------
// Modes and views

/// Register the modes of the Mk3 (former `createModes`).
fn create_mk3_modes(ctx: &mut Ctx) {
    let knobs = default_knob_ids();
    let mode = Mk3Mode::volume(ctx, knobs.clone());
    ctx.register_mode(Modes::Volume, mode);
    let mode = Mk3Mode::pan(ctx, knobs.clone());
    ctx.register_mode(Modes::Pan, mode);
    let mode = Mk3Mode::send(0, ctx, knobs.clone());
    ctx.register_mode(Modes::Send1, mode);
    let mode = Mk3Mode::send(1, ctx, knobs.clone());
    ctx.register_mode(Modes::Send2, mode);
    let mode = Mk3Mode::parameters(ctx, knobs);
    ctx.register_mode(Modes::DeviceParams, mode);
    // Layer send X IDs are used for custom modes
    for i in 0..4 {
        ctx.register_mode(Modes::get(Modes::DeviceLayerSend1, i), CustomMode::new(i as i32 + 1));
    }

    let manager = lk::fader_mode_manager(ctx);
    let faders = fader_ids();
    let mode = Mk3Mode::parameters(ctx, faders.clone());
    ctx.register_mode_in(manager, Modes::DeviceParams, mode);
    let mode = Mk3Mode::volume(ctx, faders.clone());
    ctx.register_mode_in(manager, Modes::Volume, mode);
    let mode = Mk3Mode::send(0, ctx, faders.clone());
    ctx.register_mode_in(manager, Modes::Send1, mode);
    let mode = Mk3Mode::send(1, ctx, faders);
    ctx.register_mode_in(manager, Modes::Send2, mode);
    for i in 0..4 {
        ctx.register_mode_in(manager, Modes::get(Modes::DeviceLayerSend1, i), CustomMode::new(i as i32 + 1));
    }
}

/// Register the modes of the Mini (former `createModes`).
fn create_mini_modes(ctx: &mut Ctx) {
    let knobs = default_knob_ids();
    let mode = MiniTrackMode::new(TrackVolumeMode::new_with_controls(ctx, true, knobs.clone()));
    ctx.register_mode(Modes::Volume, mode);
    let mode = MiniTrackMode::new(TrackPanMode::new_with_controls(ctx, true, knobs.clone()));
    ctx.register_mode(Modes::Pan, mode);
    let mode = MiniTrackMode::new(TrackSendMode::new_with_controls(0, ctx, true, knobs.clone()));
    ctx.register_mode(Modes::Send1, mode);
    let mode = MiniTrackMode::new(TrackSendMode::new_with_controls(1, ctx, true, knobs.clone()));
    ctx.register_mode(Modes::Send2, mode);
    let mode = ParameterMode::new(ctx, true, knobs).with_alternative_function(|_| false);
    ctx.register_mode(Modes::DeviceParams, mode);
    let mode = MiniProjectParamsMode::new(ctx);
    ctx.register_mode(Modes::User, mode);
}

/// Register the views of the Mk3 (former `createViews`).
fn create_mk3_views(ctx: &mut Ctx) {
    ctx.register_view(Views::Session, SessionView::new(false));
    ctx.register_view(Views::Control, PadModeSelectView::new(false));
    ctx.register_view(Views::Drum, DrumView::new(false));
    ctx.register_view(Views::Play, UserPadView);
    ctx.register_view(Views::Shift, DrumConfigView::new(false));
    ctx.register_view(Views::Device, DeviceConfigView);
    ctx.register_view(Views::Browser, BrowserView);
    ctx.register_view(Views::Dummy1, DummyView::new("Scale Chords"));
    ctx.register_view(Views::Dummy2, DummyView::new("User Chords"));
    ctx.register_view(Views::Dummy3, DummyView::new(NAME_CUSTOM_1));
    ctx.register_view(Views::Dummy4, DummyView::new(NAME_CUSTOM_2));
    ctx.register_view(Views::Dummy5, DummyView::new(NAME_CUSTOM_3));
    ctx.register_view(Views::Dummy6, DummyView::new(NAME_CUSTOM_4));
}

/// Register the views of the Mini (former `createViews`).
fn create_mini_views(ctx: &mut Ctx) {
    ctx.register_view(Views::Session, SessionView::new(true));
    ctx.register_view(Views::Control, PadModeSelectView::new(true));
    ctx.register_view(Views::Drum, DrumView::new(true));
    ctx.register_view(Views::Play, UserPadView);
    ctx.register_view(Views::Shift, DrumConfigView::new(true));
}

// ------------------------------------------------------------------------------------------
// Buttons

/// The light of a button with a boolean state (former `addButton` with a `BooleanSupplier`):
/// `BUTTON_STATE_HI` if on, otherwise `BUTTON_STATE_ON`.
fn on_hi(state: impl Fn(&Ctx) -> bool + 'static) -> Option<LightSupplier> {
    Some(Rc::new(move |ctx: &Ctx| ctx.colors.color_index(if state(ctx) { BUTTON_STATE_HI } else { BUTTON_STATE_ON })))
}

/// The light of a button without a supplier: on if pressed (former `ButtonPressedSupplier`).
fn pressed(id: ButtonId) -> Option<LightSupplier> {
    on_hi(move |ctx| ctx.surface.is_pressed(id))
}

/// The light of a button which is colored by the active view (former
/// `FeatureGroupButtonColorSupplier`).
fn view_color(id: ButtonId) -> Option<LightSupplier> {
    Some(Rc::new(move |ctx: &Ctx| ctx.button_color_from_active_view(id)))
}

/// A button which forwards its events to the active view (former `ViewButtonCommand`).
fn view_button(id: ButtonId) -> SharedTrigger {
    shared(FnTrigger(move |ctx: &mut Ctx, event, velocity| lk::view_button(ctx, id, event, velocity)))
}

/// A button without a command and a light, which only consumes the MIDI (former
/// `addDummyButton`).
fn add_dummy_button(ctx: &mut Ctx, id: ButtonId, channel: i32, control: i32) {
    ctx.surface.create_button(id, "");
    ctx.surface.bind_button(id, BindType::Cc, channel, control, -1);
}

/// A view select pad of the pad mode menu (Shift + pads), which the device sends as the value of
/// a CC (former `createViewButton`). The light only exists for the simulator.
fn create_view_button(ctx: &mut Ctx, id: ButtonId, label: &str, view: Views, view_index: i32) {
    let is_mini = lk::is_mini(ctx);
    let command = ViewMultiSelectCommand::new(&[view]);
    let button = ctx.surface.create_button(id, label);
    button.action = crate::framework::surface::ButtonAction::Command(shared(FnTrigger(move |ctx: &mut Ctx, event, _velocity| {
        command.select_next(ctx, event);
        if is_mini || event == ButtonEvent::Down {
            lk::set_pad_view(ctx, view);
        }
    })));
    ctx.surface.bind_button(id, BindType::Cc, 15, LAUNCHKEY_VIEW_SELECT, view_index);
}

/// A knob mode select pad of the knob mode menu (former `createModeButton`).
fn create_mode_button(ctx: &mut Ctx, id: ButtonId, label: &str, mode: Modes, mode_index: i32) {
    create_mode_select_button(ctx, MAIN_MODE_MANAGER, id, label, mode, LAUNCHKEY_MODE_SELECT, mode_index);
}

/// A fader mode select button of the fader mode menu (former `createFaderModeButton`).
fn create_fader_mode_button(ctx: &mut Ctx, id: ButtonId, label: &str, mode: Modes, mode_index: i32) {
    let manager = lk::fader_mode_manager(ctx);
    create_mode_select_button(ctx, manager, id, label, mode, LAUNCHKEY_FADER_SELECT, mode_index);
}

fn create_mode_select_button(ctx: &mut Ctx, manager: usize, id: ButtonId, label: &str, mode: Modes, control: i32, mode_index: i32) {
    let command = Rc::new(RefCell::new(ModeSelectCommand::with_options(manager, mode, false, true, false)));
    let button = ctx.surface.create_button(id, label);
    button.action = crate::framework::surface::ButtonAction::Command(shared(FnTrigger(move |ctx: &mut Ctx, event, _velocity| {
        command.borrow_mut().execute_normal(ctx, event);
    })));
    ctx.surface.bind_button(id, BindType::Cc, 15, control, mode_index);
}

/// The DAW online button, which the device sends when it enters or leaves the DAW mode.
fn add_daw_online_button(ctx: &mut Ctx) {
    ctx.add_button(
        ButtonId::Control,
        "DAW Online",
        Some(shared(FnTrigger(|ctx: &mut Ctx, _event, velocity| state_mut(ctx).is_daw_connected = velocity > 0))),
        BindType::Note,
        15,
        LAUNCHKEY_DAW_ONLINE,
        on_hi(lk::is_daw_connected),
    );
}

/// Register the buttons of the Mk3 (former `registerTriggerCommands`).
fn register_mk3_trigger_commands(ctx: &mut Ctx) {
    let cc = BindType::Cc;
    ctx.add_button(ButtonId::Shift, "Shift", Some(shared(NopCommand)), cc, 0, LAUNCHKEY_SHIFT, pressed(ButtonId::Shift));
    ctx.add_button(ButtonId::Play, "Play", Some(shared(LaunchkeyMk3PlayCommand::new())), cc, 15, LAUNCHKEY_PLAY, on_hi(|ctx| ctx.model.transport().is_playing()));
    ctx.add_button(ButtonId::Stop, "Stop", Some(shared(StopCommand::new())), cc, 15, LAUNCHKEY_STOP, on_hi(|ctx| !ctx.model.transport().is_playing()));
    ctx.add_button(
        ButtonId::Record,
        "Record",
        Some(shared(ConfiguredRecordCommand::new())),
        cc,
        15,
        LAUNCHKEY_RECORD,
        on_hi(|ctx| ConfiguredRecordCommand::is_lit(ctx, ctx.surface.is_shift_pressed())),
    );
    ctx.add_button(ButtonId::Repeat, "Repeat", Some(shared(ToggleLoopCommand::new())), cc, 15, LAUNCHKEY_LOOP, on_hi(|ctx| ctx.model.transport().is_loop()));
    ctx.add_button(ButtonId::New, "Capture MIDI", Some(shared(NewCommand::new())), cc, 15, LAUNCHKEY_CAPTURE_MIDI, pressed(ButtonId::New));
    ctx.add_button(ButtonId::Quantize, "Quantize", Some(shared(QuantizeCommand::new())), cc, 15, LAUNCHKEY_QUANTIZE, pressed(ButtonId::Quantize));
    ctx.add_button(
        ButtonId::Metronome,
        "Metronome",
        Some(shared(MetronomeCommand::new(true))),
        cc,
        15,
        LAUNCHKEY_CLICK,
        on_hi(|ctx| ctx.model.transport().is_metronome_on()),
    );
    ctx.add_button(ButtonId::Undo, "Undo", Some(shared(UndoCommand::new())), cc, 15, LAUNCHKEY_UNDO, pressed(ButtonId::Undo));
    add_dummy_button(ctx, ButtonId::F3, 15, LAUNCHKEY_DEVICE_SELECT);
    ctx.add_button(
        ButtonId::ToggleDevice,
        "Device Lock",
        Some(shared(DeviceLockCommand)),
        cc,
        15,
        LAUNCHKEY_DEVICE_LOCK,
        Some(Rc::new(|ctx: &Ctx| if ctx.model.cursor_device().is_pinned() { 127 } else { 40 })),
    );
    ctx.add_button(ButtonId::MoveTrackLeft, "Previous", Some(shared(SelectPrevNextTrackCommand::new(true))), cc, 15, LAUNCHKEY_TRACK_LEFT, pressed(ButtonId::MoveTrackLeft));
    ctx.add_button(ButtonId::MoveTrackRight, "Next", Some(shared(SelectPrevNextTrackCommand::new(false))), cc, 15, LAUNCHKEY_TRACK_RIGHT, pressed(ButtonId::MoveTrackRight));

    // Scene buttons
    ctx.add_button(ButtonId::Scene1, "Scene 1", Some(view_button(ButtonId::Scene1)), cc, 0, LAUNCHKEY_SCENE1, view_color(ButtonId::Scene1));
    ctx.add_button(ButtonId::Scene2, "Scene 2", Some(view_button(ButtonId::Scene2)), cc, 0, LAUNCHKEY_SCENE2, view_color(ButtonId::Scene2));
    ctx.add_button(ButtonId::ArrowUp, "Up", Some(view_button(ButtonId::ArrowUp)), cc, 15, LAUNCHKEY_ARROW_UP, view_color(ButtonId::ArrowUp));
    ctx.add_button(ButtonId::ArrowDown, "Down", Some(view_button(ButtonId::ArrowDown)), cc, 15, LAUNCHKEY_ARROW_DOWN, view_color(ButtonId::ArrowDown));
    // Ignore redundant messages sent by arrow up/down buttons
    add_dummy_button(ctx, ButtonId::F1, 15, LAUNCHKEY_SCENE1);
    add_dummy_button(ctx, ButtonId::F2, 15, LAUNCHKEY_SCENE2);

    // View selection with Pads in Shift mode
    create_view_button(ctx, ButtonId::Row21, "Session", Views::Session, PAD_MODE_SESSION);
    create_view_button(ctx, ButtonId::Row22, "Drum", Views::Drum, PAD_MODE_DRUM);
    create_view_button(ctx, ButtonId::Row23, "Scale Chords", Views::Dummy1, PAD_MODE_SCALE_CHORDS);
    create_view_button(ctx, ButtonId::Row24, "User Chords", Views::Dummy2, PAD_MODE_USER_CHORDS);
    create_view_button(ctx, ButtonId::Row25, NAME_CUSTOM_1, Views::Dummy3, PAD_MODE_CUSTOM_MODE0);
    create_view_button(ctx, ButtonId::Row26, NAME_CUSTOM_2, Views::Dummy4, PAD_MODE_CUSTOM_MODE1);
    create_view_button(ctx, ButtonId::Row27, NAME_CUSTOM_3, Views::Dummy5, PAD_MODE_CUSTOM_MODE2);
    create_view_button(ctx, ButtonId::Row28, NAME_CUSTOM_4, Views::Dummy6, PAD_MODE_CUSTOM_MODE3);
    create_view_button(ctx, ButtonId::Device, "Device Select", Views::Device, PAD_MODE_DEVICE_SELECT);
    create_view_button(ctx, ButtonId::Browse, "Browser", Views::Browser, PAD_MODE_NAVIGATION);

    // Knob mode selection with Pads in Shift mode
    create_mode_button(ctx, ButtonId::Row11, "Device", Modes::DeviceParams, KNOB_MODE_PARAMS);
    create_mode_button(ctx, ButtonId::Row12, "Volume", Modes::Volume, KNOB_MODE_VOLUME);
    create_mode_button(ctx, ButtonId::Row13, "Pan", Modes::Pan, KNOB_MODE_PAN);
    create_mode_button(ctx, ButtonId::Row14, "Send 1", Modes::Send1, KNOB_MODE_SEND1);
    create_mode_button(ctx, ButtonId::Row15, "Send 2", Modes::Send2, KNOB_MODE_SEND2);
    create_mode_button(ctx, ButtonId::Row16, NAME_CUSTOM_1, Modes::DeviceLayerSend1, KNOB_MODE_CUSTOM1);
    create_mode_button(ctx, ButtonId::Row17, NAME_CUSTOM_2, Modes::DeviceLayerSend2, KNOB_MODE_CUSTOM2);
    create_mode_button(ctx, ButtonId::Row18, NAME_CUSTOM_3, Modes::DeviceLayerSend3, KNOB_MODE_CUSTOM3);
    create_mode_button(ctx, ButtonId::User, NAME_CUSTOM_4, Modes::DeviceLayerSend4, KNOB_MODE_CUSTOM4);

    // Fader mode selection with fader buttons in Shift mode
    create_fader_mode_button(ctx, ButtonId::Row31, "Device", Modes::DeviceParams, FADER_MODE_PARAMS);
    create_fader_mode_button(ctx, ButtonId::Row32, "Volume", Modes::Volume, FADER_MODE_VOLUME);
    create_fader_mode_button(ctx, ButtonId::Row33, "Send 1", Modes::Send1, FADER_MODE_SEND1);
    create_fader_mode_button(ctx, ButtonId::Row34, "Send 2", Modes::Send2, FADER_MODE_SEND2);
    create_fader_mode_button(ctx, ButtonId::Row35, NAME_CUSTOM_1, Modes::DeviceLayerSend1, FADER_MODE_CUSTOM1);
    create_fader_mode_button(ctx, ButtonId::Row36, NAME_CUSTOM_2, Modes::DeviceLayerSend2, FADER_MODE_CUSTOM2);
    create_fader_mode_button(ctx, ButtonId::Row37, NAME_CUSTOM_3, Modes::DeviceLayerSend3, FADER_MODE_CUSTOM3);
    create_fader_mode_button(ctx, ButtonId::Row38, NAME_CUSTOM_4, Modes::DeviceLayerSend4, FADER_MODE_CUSTOM4);

    // Fader mode buttons (the buttons below the faders)
    for i in 0..8 {
        let id = ButtonId::get(ButtonId::RowSelect1, i);
        let control = LAUNCHKEY_SELECT1 + i as i32;
        let button = ctx.surface.create_button(id, &format!("Select {}", i + 1));
        button.action = crate::framework::surface::ButtonAction::Command(shared(ButtonAreaCommand::new(i)));
        ctx.surface.bind_button(id, cc, 15, control, -1);
        ctx.surface.create_light(
            None,
            Some(id),
            Rc::new(move |ctx: &Ctx| ButtonAreaCommand::button_color(ctx, i)),
            Rc::new(move |ctx: &mut Ctx, color| {
                let is_selected = color >= SELECTED_FLAG;
                ctx.surface.output.send_cc_ex(if is_selected { 2 } else { 0 }, control, if is_selected { color - SELECTED_FLAG } else { color });
            }),
        );
    }
    ctx.add_button(
        ButtonId::Mastertrack,
        "Toggle Select/RecArm",
        Some(shared(FnTrigger(|ctx: &mut Ctx, event, _velocity| {
            if event == ButtonEvent::Down {
                ButtonAreaCommand::toggle_select(ctx);
            }
        }))),
        cc,
        15,
        LAUNCHKEY_TOGGLE_SELECT,
        Some(Rc::new(|ctx: &Ctx| if ButtonAreaCommand::is_select(ctx) { 40 } else { 127 })),
    );

    // Online state
    add_daw_online_button(ctx);
}

/// Register the buttons of the Mini (former `registerTriggerCommands`).
fn register_mini_trigger_commands(ctx: &mut Ctx) {
    let cc = BindType::Cc;
    ctx.add_button(ButtonId::Shift, "Shift", Some(shared(NopCommand)), cc, 0, LAUNCHKEY_SHIFT, pressed(ButtonId::Shift));
    ctx.add_button(ButtonId::Play, "Play", Some(shared(PlayCommand::new())), cc, 15, LAUNCHKEY_PLAY, on_hi(|ctx| ctx.model.transport().is_playing()));
    ctx.add_button(
        ButtonId::Record,
        "Record",
        Some(shared(ConfiguredRecordCommand::new())),
        cc,
        15,
        LAUNCHKEY_RECORD,
        on_hi(|ctx| ConfiguredRecordCommand::is_lit(ctx, ctx.surface.is_shift_pressed())),
    );
    for (id, label, direction, control) in
        [(ButtonId::MoveTrackLeft, "Previous", Direction::Left, LAUNCHKEY_TRACK_LEFT), (ButtonId::MoveTrackRight, "Next", Direction::Right, LAUNCHKEY_TRACK_RIGHT)]
    {
        let command = CursorCommand::new_mode_cursor(direction, true);
        let light = CursorCommand::new_mode_cursor(direction, true);
        ctx.add_button(id, label, Some(shared(command)), cc, 15, control, on_hi(move |ctx| light.can_scroll(ctx)));
    }

    // Scene buttons
    ctx.add_button(ButtonId::Scene1, "Scene 1", Some(view_button(ButtonId::Scene1)), cc, 0, LAUNCHKEY_SCENE1, view_color(ButtonId::Scene1));
    ctx.add_button(ButtonId::Scene2, "Scene 2", Some(view_button(ButtonId::Scene2)), cc, 0, LAUNCHKEY_SCENE2, view_color(ButtonId::Scene2));

    // View selection with Pads in Shift mode
    create_view_button(ctx, ButtonId::Row21, "Session", Views::Session, PAD_MODE_SESSION);
    create_view_button(ctx, ButtonId::Row22, "Drum", Views::Drum, PAD_MODE_DRUM);
    create_view_button(ctx, ButtonId::Row23, "Custom", Views::Play, PAD_MODE_MINI_CUSTOM);

    // Knob mode selection with Pads in Shift mode
    create_mode_button(ctx, ButtonId::Row11, "Device", Modes::DeviceParams, KNOB_MODE_PARAMS);
    create_mode_button(ctx, ButtonId::Row12, "Volume", Modes::Volume, KNOB_MODE_VOLUME);
    create_mode_button(ctx, ButtonId::Row13, "Pan", Modes::Pan, KNOB_MODE_PAN);
    create_mode_button(ctx, ButtonId::Row14, "Send 1", Modes::Send1, KNOB_MODE_SEND1);
    create_mode_button(ctx, ButtonId::Row15, "Send 2", Modes::Send2, KNOB_MODE_SEND2);
    create_mode_button(ctx, ButtonId::Row16, "Custom", Modes::User, KNOB_MODE_MINI_CUSTOM);

    // Online state
    add_daw_online_button(ctx);
}

/// Register the knobs and faders (former `registerContinuousCommands`).
fn register_continuous_commands(ctx: &mut Ctx) {
    let is_mini = lk::is_mini(ctx);
    for i in 0..8 {
        let knob = ContinuousId::get(ContinuousId::Knob1, i);
        ctx.surface.create_continuous(knob, &format!("Knob {}", i + 1), ContinuousKind::AbsoluteKnob);
        ctx.surface.bind_continuous(knob, BindType::Cc, 15, LAUNCHKEY_KNOB_1 + i as i32);
        if is_mini {
            // Knobs in user mode send on the keys input on MIDI channel 1 (instead of 16), a
            // command is not needed since it is mapped to a parameter when user mode is
            // activated (see handle_extra_input_midi)
            let user_knob = ContinuousId::get(ContinuousId::DeviceKnob1, i);
            ctx.surface.create_continuous(user_knob, &format!("User Knob {}", i + 1), ContinuousKind::AbsoluteKnob);
        } else {
            let fader = ContinuousId::get(ContinuousId::Fader1, i);
            ctx.surface.create_continuous(fader, &format!("Fader {}", i + 1), ContinuousKind::Fader);
            ctx.surface.bind_continuous(fader, BindType::Cc, 15, LAUNCHKEY_FADER_1 + i as i32);
        }
    }
    if !is_mini {
        ctx.surface.create_continuous(ContinuousId::FaderMaster, "Master Fader", ContinuousKind::Fader);
        ctx.surface.bind_continuous(ContinuousId::FaderMaster, BindType::Cc, 15, LAUNCHKEY_FADER_MASTER);
        ctx.surface.bind_continuous_parameter(ContinuousId::FaderMaster, Some(ParamRef::MasterVolume));
    }
}

