// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The setup of the Electra One and the Electra One Mini (former `ElectraOneControllerSetup` and
//! `ElectraOneMiniControllerSetup`).
//!
//! Differences to Java:
//! - The CTRL port is the further MIDI input and output of the instance
//!   (`ControllerFactory::num_extra_midi_inputs` and `num_extra_midi_outputs`); its SysEx goes
//!   to `handle_extra_input_midi`.
//! - The note inputs of the MIDI channels 1-15 have the filters of the channel messages of the
//!   channel (`?X????`, the channel messages 8X-EX).
//! - The observers are replaced by [`ControllerSetup::handle_changes`].
//! - The knob sensitivity settings are not applied to the knobs (a known gap of the framework,
//!   doc/PORTING_GUIDE.md §5); the emulated Shift sets the slow sensitivity, which the play
//!   position and tempo parameters use.
//! - `layoutControls` (the bounds of the simulator GUI) is not ported.

use std::rc::Rc;

use super::colors::create_color_manager;
use super::definition::*;
use super::mode::device::{DeviceMode, EqualizerMode, ProjectTrackMode};
use super::mode::mini::{MiniDeviceMode, MiniMixerMode, MiniTransportMode};
use super::mode::mixer::{MixerMode, SendsMode};
use super::mode::session::SessionMode;
use super::mode::transport::TransportMode;
use super::surface::{self as electra, ElectraState};
use crate::backend::Sender;
use crate::config::electra as config;
use crate::config::Configuration;
use crate::controller::{ControllerSetup, InstanceParts, MidiPorts};
use crate::daw::constants::DeviceId;
use crate::daw::events::ModelEvent;
use crate::daw::model::Model;
use crate::daw::model_setup::ModelSetup;
use crate::framework::command::shared;
use crate::framework::commands::mode::ButtonRowModeCommand;
use crate::framework::ctx::Ctx;
use crate::framework::ids::{ButtonId, ContinuousId, Modes, Views};
use crate::framework::mode::dummy::DummyMode;
use crate::framework::setup_helpers;
use crate::framework::surface::{ContinuousKind, Surface};
use crate::framework::value_changer::ValueChanger;
use crate::framework::view::dummy_view::DummyView;
use crate::host::Host;
use crate::midi::{BindType, MidiInput, MidiOutput};
use crate::scale::Scales;

/// The note input filters of Port 1: one note input per MIDI channel 1-15 (the controls use
/// channel 16), former `createNoteInput ("Channel " + (i + 1), "?X????")`.
fn channel_filter(channel: usize) -> String {
    format!("?{channel:X}????")
}

/// Create an Electra One instance. `ctrl_input` and `ctrl_output` are the device IDs of the
/// CTRL port, if found.
pub fn create(
    variant: ElectraVariant,
    host: Host,
    sender: Sender,
    ports: MidiPorts,
    ctrl_output: Option<i32>,
    configuration: Configuration,
) -> Result<InstanceParts, String> {
    let colors = create_color_manager();
    let value_changer = ValueChanger::twos_complement(128, 1);
    let scales = Scales::new(&value_changer, 36, 100, 8, 8);

    // createModel
    let mut model_setup = ModelSetup::new();
    model_setup.enable_main_drum_device(false);
    model_setup.enable_device(DeviceId::Eq);
    model_setup.wants_focused_parameter = true;
    model_setup.set_has_flat_track_list(true);
    model_setup.set_has_full_flat_track_list(true);
    let size = match variant {
        ElectraVariant::Normal => 5,
        ElectraVariant::Mini => 8,
    };
    model_setup.set_num_tracks(size);
    model_setup.set_num_scenes(size);
    model_setup.set_num_sends(6);
    model_setup.set_num_markers(8);
    // Not used (yet)
    model_setup.num_filter_column_entries = 0;
    model_setup.num_results = 0;
    model_setup.num_device_layers = 0;
    model_setup.set_num_drum_pad_layers(0);
    let mut model = Model::new(host.clone(), sender.clone(), model_setup, value_changer, scales);
    model.track_bank_mut().set_indication(true);

    // createSurface: Port 1 with the note inputs of the channels 1-15, CTRL for the SysEx
    let output = MidiOutput::new(sender.clone(), Some(ports.output));
    let first = channel_filter(0);
    let mut input = MidiInput::new(sender.clone(), Some(ports.input), &[first.as_str()])?;
    for channel in 1..15 {
        input.create_note_input(&[channel_filter(channel).as_str()])?;
    }
    let surface = Surface::new(output, input, None);
    let ctrl_output = MidiOutput::new(sender.clone(), ctrl_output);

    let mut ctx = Ctx::new(host, sender, model, configuration, surface, colors);
    let state = ElectraState::new(variant, ctrl_output);
    // Only send MIDI to the pads if the DrivenByMoss preset is selected (former setTrigger)
    let online = state.online.clone();
    let trigger_sender = ctx.surface.trigger_sender.clone();
    ctx.surface.trigger_sender = Rc::new(move |output, bind_type, channel, cc, value| {
        if online.get() {
            trigger_sender(output, bind_type, channel, cc, value);
        }
    });
    ctx.surface.shift_pressed_override = Some(false);
    ctx.ext = Box::new(state);

    Ok(InstanceParts { ctx, setup: Box::new(ElectraSetup { variant }) })
}

/// The Electra One controller setup.
pub struct ElectraSetup {
    variant: ElectraVariant,
}

impl ControllerSetup for ElectraSetup {
    fn init(&mut self, ctx: &mut Ctx) {
        create_modes(ctx, self.variant);
        // We need this for triggering updateDisplay in the modes
        ctx.register_view(Views::Control, DummyView::new("Control"));
        match self.variant {
            ElectraVariant::Normal => register_normal_controls(ctx),
            ElectraVariant::Mini => register_mini_controls(ctx),
        }
        ctx.modes.set_default_id(Modes::Volume);
    }

    fn startup(&mut self, ctx: &mut Ctx) {
        ctx.set_active_view(Views::Control);
        electra::request_device_info(ctx);
    }

    fn handle_changes(&mut self, ctx: &mut Ctx) {
        // Model events (former track bank selection observer and the observers of the views)
        let events = ctx.model.take_events();
        ctx.dispatch_model_events_to_views(&events);
        for event in &events {
            if let ModelEvent::TrackSelectionChanged { is_selected, .. } = event {
                setup_helpers::handle_track_change(ctx, *is_selected);
            }
        }

        // Setting observers (former createObservers)
        let changed = ctx.config.take_changed_settings();
        let mut update_touch_commands = false;
        for id in &changed {
            if setup_helpers::handle_deactivated_items_setting(ctx, *id) {
                continue;
            }
            if config::is_assignable_setting(*id) {
                update_touch_commands = true;
            } else if *id == config::LOG_TO_CONSOLE {
                electra::set_logging_enabled(ctx);
            }
        }
        if update_touch_commands {
            electra::update_touch_combination_commands_cache(ctx);
        }
        ctx.dispatch_setting_changes_to_views(&changed);
    }

    fn exit(&mut self, ctx: &mut Ctx) {
        // Former internalShutdown (the triggers are only sent while online)
        ctx.turn_off_lights();
    }

    /// The CTRL port.
    fn handle_extra_input_midi(&mut self, ctx: &mut Ctx, input: usize, data: &[u8]) {
        if input == 0 {
            electra::handle_ctrl_sysex(ctx, data);
        }
    }
}

/// Former `createModes`.
fn create_modes(ctx: &mut Ctx, variant: ElectraVariant) {
    match variant {
        ElectraVariant::Normal => {
            let mode = MixerMode::new(ctx);
            ctx.register_mode(Modes::Volume, mode);
            let mode = SendsMode::new(ctx);
            ctx.register_mode(Modes::Send, mode);
            let mode = DeviceMode::new(ctx);
            ctx.register_mode(Modes::DeviceParams, mode);
            let mode = EqualizerMode::new(ctx);
            ctx.register_mode(Modes::EqDeviceParams, mode);
            let mode = TransportMode::new(ctx);
            ctx.register_mode(Modes::Transport, mode);
            let mode = SessionMode::new(ctx);
            ctx.register_mode(Modes::Session, mode);
            let mode = ProjectTrackMode::new(ctx);
            ctx.register_mode(Modes::Project, mode);
        }
        ElectraVariant::Mini => {
            let mode = MiniMixerMode::new(ctx);
            ctx.register_mode(Modes::Volume, mode);
            let mode = MiniDeviceMode::new(ctx);
            ctx.register_mode(Modes::DeviceParams, mode);
            let mode = MiniTransportMode::new(ctx);
            ctx.register_mode(Modes::Transport, mode);
        }
    }
    let mode = DummyMode::new(ctx, variant.knob_ids());
    ctx.register_mode(Modes::Dummy, mode);
}

/// Add a pad or button whose light is the color of the active mode (former `addButton` with
/// `getButtonColor`).
fn add_button(ctx: &mut Ctx, id: ButtonId, label: &str, row: usize, column: usize, cc: i32) {
    let supplier = Rc::new(move |ctx: &Ctx| ctx.mode_button_color(id));
    ctx.add_button(id, label, Some(shared(ButtonRowModeCommand::new(row, column))), BindType::Cc, ELECTRA_MIDI_CHANNEL, cc, Some(supplier));
}

/// Add a relative knob (two's complement) without a command; the modes bind it to their
/// parameters (former `addRelativeKnob`).
fn add_relative_knob(ctx: &mut Ctx, id: ContinuousId, label: &str, cc: i32) {
    ctx.surface.create_continuous(id, label, ContinuousKind::RelativeKnob);
    ctx.surface.bind_continuous(id, BindType::Cc, ELECTRA_MIDI_CHANNEL, cc);
}

/// Former `registerTriggerCommands` and `registerContinuousCommands` of the normal model: a 6x6
/// grid of controls and of pads.
fn register_normal_controls(ctx: &mut Ctx) {
    for row in 0..6 {
        for column in 0..6 {
            let cc = ELECTRA_ROW_1 + 10 * row as i32 + column as i32;
            add_button(ctx, button_id(row, column), &format!("Row {}: {}", row + 1, column + 1), row, column, cc);
        }
    }
    for row in 0..6 {
        for column in 0..6 {
            add_relative_knob(ctx, continuous_id(row, column), &format!("Ctrl {}-{}", row + 1, column + 1), ctrl_cc(row, column));
        }
    }
}

/// Former `registerTriggerCommands` and `registerContinuousCommands` of the Mini: the pushes of
/// the 8 knobs (row 1), the 4 buttons (row 2) and the 8 knobs.
fn register_mini_controls(ctx: &mut Ctx) {
    for index in 0..8 {
        add_button(ctx, ButtonId::get(ButtonId::Row11, index), &(index + 1).to_string(), 0, index, ELECTRA_MINI_CTRL_PUSH_1 + index as i32);
    }
    for index in 0..4 {
        add_button(ctx, ButtonId::get(ButtonId::Row21, index), &(index + 1).to_string(), 1, index, ELECTRA_MINI_FUNCTION_BUTTON_1 + index as i32);
    }
    for index in 0..8 {
        add_relative_knob(ctx, ContinuousId::get(ContinuousId::Knob1, index), &format!("Ctrl {}", index + 1), ELECTRA_MINI_KNOB_1 + index as i32);
    }
}
