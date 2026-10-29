// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The setup of the Novation Launch Control XL (former `LaunchControlXLControllerSetup`).
//!
//! The device has 16 templates: 8 user templates (MIDI channels 1-8, their MIDI is forwarded to
//! Reaper) and 8 factory templates (MIDI channels 9-16). The factory templates 1-5 select the
//! main modes (mixer, equalizer, layer mixer, drum sequencer, note sequencer). All controls are
//! re-bound to the MIDI channel of the selected template.
//!
//! Differences to Java:
//! - The observers are replaced by [`ControllerSetup::handle_changes`] and the template change
//!   is handled directly by the system exclusive handler.
//! - The three Java mode managers are the main mode manager of the context and two additional
//!   ones (see `XlState::track_modes` and `XlState::fader_modes`).
//! - The note edit modes are not registered in the fader mode manager (a mode can only be
//!   borrowed once), a mode which does nothing is used instead (see `XlNoteEditFaderMode`).
//! - The LEDs of the knobs are lights without a button.
//! - `layoutControls` (the bounds of the simulator GUI) is not ported.

use std::rc::Rc;

use super::colors::{create_color_manager, LAUNCHCONTROL_COLOR_BLACK};
use super::mode::buttons::{
    ChannelState, XlChannelStateMode, XlDrumSequencerMode, XlLoopLengthMode, XlNoteSequencerMode, XlScenesMode, XlSelectDeviceParamsPageMode,
    XlSequencerResolutionMode, XlTransportMode,
};
use super::mode::faders::{XlEqGainMode, XlLayerVolumeMode, XlNoteEditFaderMode};
use super::mode::main::{XlDummyMode, XlMainMode};
use super::surface::{self as xl, XlState};
use crate::backend::Sender;
use crate::config::{Configuration, SettingId};
use crate::controller::{ControllerSetup, InstanceParts, MidiPorts};
use crate::daw::constants::DeviceId;
use crate::daw::events::{ModelEvent, ParameterBankId};
use crate::daw::model::Model;
use crate::daw::model_setup::ModelSetup;
use crate::framework::command::{shared, SharedTrigger};
use crate::framework::commands::cursor::{CursorCommand, Direction};
use crate::framework::commands::mode::{ButtonRowModeCommand, KnobRowModeCommand};
use crate::framework::ctx::{Ctx, MAIN_MODE_MANAGER};
use crate::framework::ids::{ButtonId, ContinuousId, Modes};
use crate::framework::mode::master::MasterAndFXVolumeMode;
use crate::framework::mode::dummy::DummyMode;
use crate::framework::mode::parameter_mode::sequential_controls;
use crate::framework::mode::track::TrackVolumeMode;
use crate::framework::surface::{ButtonAction, ContinuousKind, LightSupplier, Surface};
use crate::framework::value_changer::ValueChanger;
use crate::host::Host;
use crate::midi::{BindType, MidiInput, MidiOutput};
use crate::scale::Scales;

/// The MIDI filters of the user templates: route all CC, note on and note off from the user
/// templates on channel 1-8 to Reaper.
pub const USER_TEMPLATE_FILTERS: [&str; 24] = [
    "B0????", "B1????", "B2????", "B3????", "B4????", "B5????", "B6????", "B7????", "90????", "91????", "92????", "93????", "94????", "95????", "96????",
    "97????", "80????", "81????", "82????", "83????", "84????", "85????", "86????", "87????",
];

/// The faders (former `FADER_IDS`).
pub fn fader_ids() -> Vec<ContinuousId> {
    sequential_controls(ContinuousId::Fader1, 8)
}

/// The 3 rows of knobs (former `KNOB_CONTROLS`).
pub fn knob_controls() -> Vec<ContinuousId> {
    let mut controls = sequential_controls(ContinuousId::Send1Knob1, 8);
    controls.extend(sequential_controls(ContinuousId::Send2Knob1, 8));
    controls.extend(sequential_controls(ContinuousId::PanKnob1, 8));
    controls
}

/// Create a Launch Control XL instance.
pub fn create(host: Host, sender: Sender, ports: MidiPorts, configuration: Configuration) -> Result<InstanceParts, String> {
    let value_changer = ValueChanger::twos_complement(128, 1);
    let colors = create_color_manager();
    let scales = Scales::new(&value_changer, 36, 100, 8, 8);

    // createModel
    let mut model_setup = ModelSetup::new();
    model_setup.set_has_flat_track_list(true);
    model_setup.set_has_full_flat_track_list(configuration.are_master_tracks_included());
    model_setup.set_num_sends(2);
    model_setup.set_num_drum_pad_layers(8);
    model_setup.enable_device(DeviceId::Eq);
    model_setup.enable_device(DeviceId::FirstInstrument);
    let mut model = Model::new(host.clone(), sender.clone(), model_setup, value_changer, scales);
    model.track_bank_mut().set_indication(true);

    // createSurface
    let output = MidiOutput::new(sender.clone(), Some(ports.output));
    let input = MidiInput::new(sender.clone(), Some(ports.input), &USER_TEMPLATE_FILTERS)?;
    let surface = Surface::new(output, input, None);

    let mut ctx = Ctx::new(host, sender, model, configuration, surface, colors);
    let track_modes = ctx.add_mode_manager();
    let fader_modes = ctx.add_mode_manager();
    ctx.ext = Box::new(XlState::new(track_modes, fader_modes));
    ctx.sysex_handler = Some(Rc::new(xl::handle_sysex));
    // Ignore 'not bound' warnings for user modes (former handleCC)
    ctx.midi_handler = Some(Rc::new(|_: &mut Ctx, _, _, _| {}));

    Ok(InstanceParts { ctx, setup: Box::new(LaunchControlXlSetup) })
}

/// The Launch Control XL controller setup.
pub struct LaunchControlXlSetup;

impl ControllerSetup for LaunchControlXlSetup {
    fn init(&mut self, ctx: &mut Ctx) {
        create_modes(ctx);
        register_trigger_commands(ctx);
        register_continuous_commands(ctx);
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        let track_modes = xl::track_modes(ctx);
        let fader_modes = xl::fader_modes(ctx);
        ctx.set_active_mode_in(track_modes, Modes::Mute);
        ctx.set_active_mode_in(fader_modes, Modes::Volume);
        xl::select_template(ctx, 8);
    }

    fn handle_changes(&mut self, ctx: &mut Ctx) {
        for event in ctx.model.take_events() {
            if let ModelEvent::TrackSelectionChanged { is_selected, .. } = event {
                eq_track_selection_changed(ctx, is_selected);
            }
        }
        for id in ctx.config.take_changed_settings() {
            handle_setting_change(ctx, id);
        }
    }

    fn exit(&mut self, ctx: &mut Ctx) {
        ctx.turn_off_lights();
    }
}

/// Former `XLEqMode.trackSelectionChanged`: add an equalizer to the selected track if there is
/// none (only while the equalizer mode is active).
fn eq_track_selection_changed(ctx: &mut Ctx, is_selected: bool) {
    if !is_selected || !ctx.modes.is_active(Modes::EqDeviceParams) {
        return;
    }
    if ctx.model.specific_device(DeviceId::Eq).is_some_and(|d| d.does_exist()) {
        return;
    }
    if let Some(track) = ctx.model.current_track_bank_mut().selected_item_mut() {
        track.add_equalizer_device();
    }
}

/// React on a changed setting (former `createObservers`: deactivated items handler and scale
/// observers).
fn handle_setting_change(ctx: &mut Ctx, id: SettingId) {
    match id {
        SettingId::EXCLUDE_DEACTIVATED_ITEMS => {
            let exclude = ctx.config.are_deactivated_items_excluded();
            ctx.model.track_bank_mut().set_skip_disabled_items(exclude);
        }
        SettingId::SCALES_SCALE => {
            let name = ctx.config.scale().to_string();
            ctx.model.scales_mut().set_scale_by_name(&name);
        }
        SettingId::SCALES_BASE => {
            let name = ctx.config.scale_base().to_string();
            ctx.model.scales_mut().set_scale_offset_by_name(&name);
        }
        SettingId::SCALES_IN_KEY => {
            let chromatic = !ctx.config.is_scale_in_key();
            ctx.model.scales_mut().set_chromatic(chromatic);
        }
        _ => {}
    }
}

// ------------------------------------------------------------------------------------------
// Modes

/// Register the modes (former `createModes`).
pub fn create_modes(ctx: &mut Ctx) {
    // The Rec Arm button is needed by the note edit modes (shifted parameter provider)
    create_button(
        ctx,
        ButtonId::RecArm,
        "REC ARM",
        shared(ButtonRowModeCommand::new(2, 3)),
        xl::LAUNCHCONTROL_RECORD_ARM,
        mode_color_supplier(ButtonId::RecArm),
    );

    let knobs = knob_controls();
    let faders = fader_ids();

    let mode = XlMainMode::track_mix(ctx, knobs.clone());
    ctx.register_mode(Modes::Send, mode);
    let mode = XlMainMode::eq(ctx, knobs.clone());
    ctx.register_mode(Modes::EqDeviceParams, mode);
    let mode = XlMainMode::layer_mix(ctx, knobs.clone());
    ctx.register_mode(Modes::DeviceLayer, mode);
    let mode = XlMainMode::drum_pad_edit(ctx, knobs.clone());
    ctx.register_mode(Modes::DrumSequencer, mode);
    let mode = XlMainMode::note_edit_mode(ctx, knobs.clone());
    ctx.register_mode(Modes::NoteSequencer, mode);
    let mode = XlDummyMode::new(ctx, knobs);
    ctx.register_mode(Modes::Dummy, mode);

    let track_modes = xl::track_modes(ctx);
    let mode = XlChannelStateMode::track(ctx, ChannelState::Mute);
    ctx.register_mode_in(track_modes, Modes::Mute, mode);
    let mode = XlChannelStateMode::track(ctx, ChannelState::Solo);
    ctx.register_mode_in(track_modes, Modes::Solo, mode);
    let mode = XlChannelStateMode::track(ctx, ChannelState::RecArm);
    ctx.register_mode_in(track_modes, Modes::RecArm, mode);
    ctx.register_mode_in(track_modes, Modes::DeviceParams, XlSelectDeviceParamsPageMode::new(Modes::DeviceParams, ParameterBankId::CursorDevice));
    ctx.register_mode_in(
        track_modes,
        Modes::InstrumentDeviceParams,
        XlSelectDeviceParamsPageMode::new(Modes::InstrumentDeviceParams, ParameterBankId::FirstInstrument),
    );
    ctx.register_mode_in(track_modes, Modes::Transport, XlTransportMode::new());
    ctx.register_mode_in(track_modes, Modes::Clip, XlScenesMode);
    let mode = XlChannelStateMode::layer(ctx, ChannelState::Mute);
    ctx.register_mode_in(track_modes, Modes::DeviceLayerMute, mode);
    let mode = XlChannelStateMode::layer(ctx, ChannelState::Solo);
    ctx.register_mode_in(track_modes, Modes::DeviceLayerSolo, mode);
    let mode = XlDrumSequencerMode::new(ctx);
    ctx.register_mode_in(track_modes, Modes::DrumSequencer, mode);
    let mode = XlNoteSequencerMode::new(ctx);
    ctx.register_mode_in(track_modes, Modes::NoteSequencer, mode);
    ctx.register_mode_in(track_modes, Modes::LoopLength, XlLoopLengthMode);
    ctx.register_mode_in(track_modes, Modes::Configuration, XlSequencerResolutionMode);

    let fader_modes = xl::fader_modes(ctx);
    let mode = TrackVolumeMode::new_with_controls(ctx, true, faders.clone());
    ctx.register_mode_in(fader_modes, Modes::Volume, mode);
    let mode = MasterAndFXVolumeMode::new(ctx, true, faders.clone());
    ctx.register_mode_in(fader_modes, Modes::Master, mode);
    let mode = XlEqGainMode::new(ctx, faders.clone());
    ctx.register_mode_in(fader_modes, Modes::EqDeviceParams, mode);
    let mode = XlLayerVolumeMode::new(ctx, faders.clone());
    ctx.register_mode_in(fader_modes, Modes::DeviceLayer, mode);
    ctx.register_mode_in(fader_modes, Modes::DrumSequencer, XlNoteEditFaderMode::new("Drum Sequencer"));
    ctx.register_mode_in(fader_modes, Modes::NoteSequencer, XlNoteEditFaderMode::new("Note Sequencer"));
    let mode = DummyMode::new(ctx, faders);
    ctx.register_mode_in(fader_modes, Modes::Dummy, mode);
}

// ------------------------------------------------------------------------------------------
// Buttons

/// The bind type of a button (former `getTriggerBindType`).
pub fn trigger_bind_type(button: ButtonId) -> BindType {
    match button {
        ButtonId::MoveTrackLeft | ButtonId::MoveTrackRight | ButtonId::MoveBankLeft | ButtonId::MoveBankRight => BindType::Cc,
        _ => BindType::Note,
    }
}

/// The color of a button which is controlled by the active main mode (former `getModeColor`).
fn mode_color_supplier(button: ButtonId) -> LightSupplier {
    Rc::new(move |ctx: &Ctx| ctx.mode_button_color_in(MAIN_MODE_MANAGER, button))
}

/// The color of a button which is controlled by the active track button mode (former
/// `getTrackModeColor`).
fn track_mode_color_supplier(button: ButtonId) -> LightSupplier {
    Rc::new(move |ctx: &Ctx| {
        let track_modes = xl::track_modes(ctx);
        match ctx.active_mode_in(track_modes) {
            Some(_) => ctx.mode_button_color_in(track_modes, button),
            None => LAUNCHCONTROL_COLOR_BLACK,
        }
    })
}

/// Create a button with a command and a light (former `createButton`). The MIDI binding is
/// done when a template is selected (see [`bind_to_template`]).
fn create_button(ctx: &mut Ctx, id: ButtonId, label: &str, command: SharedTrigger, midi_control: i32, supplier: LightSupplier) {
    let bind_type = trigger_bind_type(id);
    xl::xl_mut(ctx).button_controls.push((id, bind_type, midi_control));
    ctx.surface.create_button(id, label).action = ButtonAction::Command(command);
    ctx.surface.create_light(None, Some(id), supplier, Rc::new(move |ctx: &mut Ctx, state| xl::set_trigger(ctx, bind_type, midi_control, state)));
}

/// Register the commands of all buttons (former `registerTriggerCommands`).
pub fn register_trigger_commands(ctx: &mut Ctx) {
    let cursors = [
        (ButtonId::MoveBankLeft, "Send Previous", Direction::Left, xl::LAUNCHCONTROL_SEND_PREV),
        (ButtonId::MoveBankRight, "Send Next", Direction::Right, xl::LAUNCHCONTROL_SEND_NEXT),
        (ButtonId::MoveTrackLeft, "Previous", Direction::Down, xl::LAUNCHCONTROL_TRACK_PREV),
        (ButtonId::MoveTrackRight, "Next", Direction::Up, xl::LAUNCHCONTROL_TRACK_NEXT),
    ];
    for (id, label, direction, control) in cursors {
        let command = Rc::new(std::cell::RefCell::new(CursorCommand::new_mode_cursor(direction, false)));
        let light_command = command.clone();
        let supplier: LightSupplier = Rc::new(move |ctx: &Ctx| match light_command.try_borrow() {
            Ok(command) if command.can_scroll(ctx) => 127,
            _ => 0,
        });
        create_button(ctx, id, label, command, control, supplier);
    }

    create_button(ctx, ButtonId::Device, "DEVICE", shared(ButtonRowModeCommand::new(2, 0)), xl::LAUNCHCONTROL_DEVICE, mode_color_supplier(ButtonId::Device));
    create_button(ctx, ButtonId::Mute, "MUTE", shared(ButtonRowModeCommand::new(2, 1)), xl::LAUNCHCONTROL_MUTE, mode_color_supplier(ButtonId::Mute));
    create_button(ctx, ButtonId::Solo, "SOLO", shared(ButtonRowModeCommand::new(2, 2)), xl::LAUNCHCONTROL_SOLO, mode_color_supplier(ButtonId::Solo));

    let track_modes = xl::track_modes(ctx);
    for i in 0..8 {
        let (focus, control) = if i < 4 {
            (xl::LAUNCHCONTROL_TRACK_FOCUS_1 + i as i32, xl::LAUNCHCONTROL_TRACK_CONTROL_1 + i as i32)
        } else {
            (xl::LAUNCHCONTROL_TRACK_FOCUS_5 + i as i32 - 4, xl::LAUNCHCONTROL_TRACK_CONTROL_5 + i as i32 - 4)
        };
        let row1 = ButtonId::get(ButtonId::Row11, i);
        create_button(ctx, row1, &format!("Row 1: {}", i + 1), shared(ButtonRowModeCommand::new(0, i)), focus, mode_color_supplier(row1));
        let row2 = ButtonId::get(ButtonId::Row21, i);
        create_button(
            ctx,
            row2,
            &format!("Row 2: {}", i + 1),
            shared(ButtonRowModeCommand::with_manager(track_modes, 0, i)),
            control,
            track_mode_color_supplier(row2),
        );
    }
}

// ------------------------------------------------------------------------------------------
// Knobs and faders

/// The knob rows: first control, label, index offset of the knob row mode command.
const KNOB_ROWS: [(ContinuousId, &str, usize); 3] =
    [(ContinuousId::Send1Knob1, "Send A Knob", 0), (ContinuousId::Send2Knob1, "Send B Knob", 8), (ContinuousId::PanKnob1, "Pan Knob", 16)];

/// Register the knobs and faders (former `registerContinuousCommands`). The command bindings
/// are only for 'non-parameter' modes like note editing. The MIDI binding happens separately
/// since each template has its own MIDI channel (see [`bind_to_template`]).
pub fn register_continuous_commands(ctx: &mut Ctx) {
    for i in 0..8 {
        for (row, (first, label, offset)) in KNOB_ROWS.iter().enumerate() {
            let id = ContinuousId::get(*first, i);
            let control = ctx.surface.create_continuous(id, &format!("{label} {}", i + 1), ContinuousKind::AbsoluteKnob);
            control.command = Some(Rc::new(std::cell::RefCell::new(KnobRowModeCommand::new(offset + i))));
            let knob_index = offset + i;
            ctx.surface.create_light(
                None,
                None,
                Rc::new(move |ctx: &Ctx| knob_value(ctx, knob_index)),
                Rc::new(move |ctx: &mut Ctx, value| set_knob_row_color(ctx, row, i, value)),
            );
        }

        let id = ContinuousId::get(ContinuousId::Fader1, i);
        let control = ctx.surface.create_continuous(id, &format!("Fader {}", i + 1), ContinuousKind::Fader);
        control.command = Some(Rc::new(std::cell::RefCell::new(KnobRowModeCommand::new(24 + i))));
    }
}

/// Get the value of one of the knobs from the active main mode (former `getKnobValue`).
fn knob_value(ctx: &Ctx, index: usize) -> i32 {
    ctx.active_mode().and_then(|mode| mode.try_borrow().ok().map(|m| m.knob_value(ctx, index).max(0))).unwrap_or(0)
}

/// Set the color of a knob LED by the active main mode (former `setKnobRowColor`).
fn set_knob_row_color(ctx: &mut Ctx, row: usize, column: usize, value: i32) {
    let Some(mode) = ctx.active_mode() else {
        return;
    };
    let Ok(mode) = mode.try_borrow() else {
        return;
    };
    if let Some(main) = mode.as_any().downcast_ref::<XlMainMode>() {
        main.set_knob_color(ctx, row, column, value);
    } else if let Some(dummy) = mode.as_any().downcast_ref::<XlDummyMode>() {
        dummy.set_knob_color(ctx, row, column, value);
    }
}

/// Re-bind all controls to the MIDI channel of the selected factory template. Each template
/// has its own MIDI channel (former `bindToTemplate`).
fn bind_to_template(ctx: &mut Ctx) {
    let midi_channel = xl::xl(ctx).template_id;
    if midi_channel < 0 {
        return;
    }
    for i in 0..8 {
        let index = i as i32;
        ctx.surface.bind_continuous(ContinuousId::get(ContinuousId::Send1Knob1, i), BindType::Cc, midi_channel, xl::LAUNCHCONTROL_KNOB_SEND_A_1 + index);
        ctx.surface.bind_continuous(ContinuousId::get(ContinuousId::Send2Knob1, i), BindType::Cc, midi_channel, xl::LAUNCHCONTROL_KNOB_SEND_B_1 + index);
        ctx.surface.bind_continuous(ContinuousId::get(ContinuousId::PanKnob1, i), BindType::Cc, midi_channel, xl::LAUNCHCONTROL_KNOB_PAN_1 + index);
        ctx.surface.bind_continuous(ContinuousId::get(ContinuousId::Fader1, i), BindType::Cc, midi_channel, xl::LAUNCHCONTROL_FADER_1 + index);
    }
    let button_controls = xl::xl(ctx).button_controls.clone();
    for (id, bind_type, control) in button_controls {
        ctx.surface.bind_button(id, bind_type, midi_channel, control, -1);
    }
    ctx.surface.clear_cache();
}

/// A different factory template has been selected: update all modes and bindings (former
/// `activateTemplate`).
pub fn activate_template(ctx: &mut Ctx) {
    let template_id = xl::xl(ctx).template_id;
    ctx.host.println(&format!("Switch to template: {template_id}"));

    let (mode, fader_mode, message) = match template_id {
        // Factory mode 1 - Track Mixer
        8 => (Modes::Send, Modes::Volume, "Mixer"),
        // Factory mode 2 - Equalizer
        9 => (Modes::EqDeviceParams, Modes::EqDeviceParams, "Equalizer"),
        // Factory mode 3 - Layer Mixer
        10 => (Modes::DeviceLayer, Modes::DeviceLayer, "Layer Mixer"),
        // Factory mode 4 - Drum Sequencer
        11 => (Modes::DrumSequencer, Modes::DrumSequencer, "Drum Sequencer"),
        // Factory mode 5 - Note Sequencer
        12 => (Modes::NoteSequencer, Modes::NoteSequencer, "Note Sequencer"),
        _ => (Modes::Dummy, Modes::Dummy, ""),
    };

    ctx.set_active_mode(mode);
    let fader_modes = xl::fader_modes(ctx);
    ctx.set_active_mode_in(fader_modes, fader_mode);

    if template_id >= 8 {
        bind_to_template(ctx);
        if !message.is_empty() {
            ctx.host.show_notification(&format!("Selected Mode: {message}"));
        }
    }
}
